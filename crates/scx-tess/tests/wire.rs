// Written for solidcorex. Apache-2.0.
//! The wireframe of the MK2 primitives and the block (research 72 TS1): each polyline runs
//! from its start vertex's index to its end vertex's, each sample lies on every adjacent
//! face through that face's pcurve at the same parameter, each circle edge holds the chord
//! against the exact circle with the closed-form count, and the result is a function of the
//! body alone. Spec, budget and stop refusals.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use scx_brep::{Brep, Document, EdgeShape, UseGeom};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::frame::Frame;
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_tess::{Mesh, TessError, TessRefusal, TessSpec, sample_edge, wireframe};
use scx_testkit::bodies::made::{block, cone, cylinder, sphere, torus};

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn at(o: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

/// A frame turned off every world axis, at `o`.
fn tilted(o: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(0.3, -0.5, 0.8),
        Vec3::new(1.0, 0.2, 0.1),
    )
    .unwrap()
}

/// Each primitive, at the origin and tilted off it.
fn bodies() -> Vec<(String, Brep)> {
    let doc = Document::default();
    let mut out = Vec::new();
    for (place, f) in [("at", at([0.0; 3])), ("tilted", tilted([40.0, -25.0, 7.0]))] {
        let mut add = |name: &str, body: Brep| out.push((format!("{name} {place}"), body));
        add(
            "block",
            block(&f, [mm(10.0), mm(20.0), mm(30.0)], &doc)
                .unwrap()
                .body,
        );
        add(
            "cylinder",
            cylinder(&f, mm(10.0), mm(20.0), &doc).unwrap().body,
        );
        add(
            "cone",
            cone(&f, [mm(10.0), mm(4.0)], mm(15.0), &doc).unwrap().body,
        );
        add(
            "apex cone",
            cone(&f, [mm(6.0), mm(0.0)], mm(9.0), &doc).unwrap().body,
        );
        add("sphere", sphere(&f, mm(7.0), &doc).unwrap().body);
        add("torus", torus(&f, mm(12.0), mm(3.0), &doc).unwrap().body);
    }
    out
}

fn spec(chord: f64) -> TessSpec {
    TessSpec::new(mm(chord), Resolution::DEFAULT).unwrap()
}

fn wire(body: &Brep, spec: &TessSpec) -> Result<Mesh, TessError> {
    wireframe(body, spec, &mut Budget::unlimited(), &Stop::new())
}

#[test]
fn polylines_run_from_their_start_vertex_to_their_end_vertex() {
    for (name, body) in bodies() {
        let mesh = wire(&body, &spec(0.01)).unwrap();
        assert_eq!(mesh.edges.len(), body.edges().len(), "{name}");
        assert!(
            mesh.triangles.is_empty() && mesh.face_of.is_empty(),
            "{name}"
        );
        for (line, e) in mesh.edges.iter().zip(body.edges()) {
            assert_eq!(line.edge, e.id(), "{name}");
            let (first, last) = (line.points[0], *line.points.last().unwrap());
            assert_eq!(mesh.positions[first as usize], e.start().point(), "{name}");
            assert_eq!(mesh.positions[last as usize], e.end().point(), "{name}");
            let [s, t] = e.ends();
            assert_eq!(
                s == t,
                first == last,
                "{name} {}: a closed edge closes",
                e.id()
            );
            match e.shape() {
                EdgeShape::Collapsed => assert_eq!(line.points, vec![first, first], "{name}"),
                EdgeShape::Curve { .. } => {
                    assert!(line.points.len() >= 2, "{name}");
                    // Interior samples are the edge's own, each once.
                    let inner = &line.points[1..line.points.len() - 1];
                    assert!(inner.windows(2).all(|w| w[1] == w[0] + 1), "{name}");
                }
            }
        }
        // Every position is a vertex or one edge's sample.
        let named = mesh
            .edges
            .iter()
            .map(|l| l.points.len().saturating_sub(2))
            .sum::<usize>();
        assert_eq!(
            mesh.positions.len(),
            body.vertices().len() + named,
            "{name}"
        );
    }
}

#[test]
fn samples_lie_on_every_adjacent_face_through_its_pcurve() {
    let zone = Resolution::DEFAULT.zone().mm();
    for (name, body) in bodies() {
        for e in body.edges() {
            let s = sample_edge(e, &spec(0.01), &mut Budget::unlimited(), &Stop::new()).unwrap();
            for u in e.uses() {
                let UseGeom::Pcurve { curve, .. } = u.geom() else {
                    continue;
                };
                let surface = u.face().surface();
                for (&t, &p) in s.params().iter().zip(s.points()) {
                    let off = surface.point(curve.point(t)).distance(p);
                    assert!(off <= zone, "{name} {} at {t}: {off}", e.id());
                }
            }
        }
    }
}

/// The circle edges' closed-form counts at chord 0.01 mm, from `n = ceil(span / step)` with
/// `step = min(pi/2, 4 asin(sqrt(c / 2r)), L_f / r)`, `L_f = 0.8 sqrt(3) sqrt(c (2 rho - c))`
/// for the least radius of curvature `rho` of an adjacent face (none for a plane, cylinder or
/// cone; 7 for the sphere; 3 for the torus), at least 3 for a closed edge.
#[test]
fn circle_edges_hold_the_chord_against_the_exact_circle_at_the_closed_form_count() {
    let c = 0.01;
    for (name, body) in bodies() {
        let mesh = wire(&body, &spec(c)).unwrap();
        for (line, e) in mesh.edges.iter().zip(body.edges()) {
            let EdgeShape::Curve {
                curve: Curve::Circle(circle),
                ..
            } = e.shape()
            else {
                continue;
            };
            let (o, r) = (circle.frame().origin(), circle.radius().mm());
            let poly: Vec<Point3> = line
                .points
                .iter()
                .map(|&i| mesh.positions[i as usize])
                .collect();
            for p in &poly {
                assert!(
                    (p.distance(o) - r).abs() <= 1e-9 * r,
                    "{name}: off the circle"
                );
            }
            // A chord's farthest point from its arc is its midpoint's radial gap.
            let gap = poly
                .windows(2)
                .map(|w| {
                    let m = Point3::new(
                        0.5 * (w[0].x + w[1].x),
                        0.5 * (w[0].y + w[1].y),
                        0.5 * (w[0].z + w[1].z),
                    );
                    r - m.distance(o)
                })
                .fold(0.0, f64::max);
            assert!(
                gap <= line.chord.mm() * (1.0 + 1e-9) + 1e-12,
                "{name}: {gap}"
            );
            assert!(line.chord.mm() <= c, "{name}");
            let segments = line.points.len() - 1;
            let expected = match (name.split(' ').next().unwrap(), r) {
                ("cylinder" | "cone", 10.0) => 71,
                ("cone", 4.0) => 45,
                ("apex", 6.0) => 55,
                ("sphere", 7.0) => 43,
                ("torus", 15.0) => 278,
                ("torus", 3.0) => 56,
                other => panic!("{name}: no expected count for {other:?}"),
            };
            assert_eq!(segments, expected, "{name} r {r}");
        }
    }
}

#[test]
fn the_wireframe_is_a_function_of_the_body() {
    for (name, body) in bodies() {
        let a = wire(&body, &spec(0.003)).unwrap();
        let b = wire(&body.clone(), &spec(0.003)).unwrap();
        assert_eq!(a, b, "{name}");
        let bits = |m: &Mesh| {
            m.positions
                .iter()
                .flat_map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()])
                .collect::<Vec<_>>()
        };
        assert_eq!(bits(&a), bits(&b), "{name}");
    }
}

#[test]
fn a_finer_chord_never_gives_fewer_points() {
    for (name, body) in bodies() {
        let mut last = 0;
        for c in [0.1, 0.03, 0.01, 0.003] {
            let n = wire(&body, &spec(c)).unwrap().positions.len();
            assert!(n >= last, "{name} at {c}: {n} < {last}");
            last = n;
        }
    }
}

#[test]
fn the_spec_refuses_what_proves_nothing() {
    let res = Resolution::DEFAULT;
    let zone = res.zone();
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, zone.mm()] {
        let e = TessSpec::new(mm(bad), res).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Refused, "{bad}");
        assert_eq!(e.name(), "chord");
    }
    for bad in [0.0, -0.1, f64::NAN, f64::INFINITY] {
        let e = spec(0.01).with_angle(bad).unwrap_err();
        assert_eq!(e.name(), "angle", "{bad}");
        let e = spec(0.01).with_edge_length(mm(bad)).unwrap_err();
        assert_eq!(e.name(), "edge_length", "{bad}");
    }
    let s = spec(0.01)
        .with_angle(0.2)
        .unwrap()
        .with_edge_length(mm(2.0))
        .unwrap();
    assert_eq!(
        (s.chord(), s.angle(), s.edge_length()),
        (mm(0.01), Some(0.2), Some(mm(2.0)))
    );
}

#[test]
fn a_body_of_another_resolution_is_refused() {
    let doc = Document::default();
    let body = cylinder(&at([0.0; 3]), mm(10.0), mm(20.0), &doc)
        .unwrap()
        .body;
    let other = Resolution::new(mm(1e-4)).unwrap();
    let e = wire(&body, &TessSpec::new(mm(0.01), other).unwrap()).unwrap_err();
    assert!(
        matches!(e, TessError::Refused(TessRefusal::Resolution { .. })),
        "{e}"
    );
}

#[test]
fn angle_and_edge_length_bounds_refine_the_count() {
    let doc = Document::default();
    let body = cylinder(&at([0.0; 3]), mm(10.0), mm(20.0), &doc)
        .unwrap()
        .body;
    let count = |s: &TessSpec| wire(&body, s).unwrap().positions.len();
    let plain = count(&spec(0.5));
    // 2 pi / 0.1 rad: 63 segments a circle.
    let turned = count(&spec(0.5).with_angle(0.1).unwrap());
    // A 20 mm seam in 1 mm pieces, and 2 pi 10 / 1 mm: 63 a circle.
    let short = count(&spec(0.5).with_edge_length(mm(1.0)).unwrap());
    assert!(plain < turned && plain < short, "{plain} {turned} {short}");
    assert_eq!(turned, 2 + 2 * 62);
    assert_eq!(short, 2 + 2 * 62 + 19);
}

#[test]
fn an_empty_budget_or_a_set_stop_ends_the_wireframe() {
    let doc = Document::default();
    let body = torus(&at([0.0; 3]), mm(12.0), mm(3.0), &doc).unwrap().body;
    let e = wireframe(&body, &spec(0.01), &mut Budget::new(100), &Stop::new()).unwrap_err();
    assert_eq!(e, TessError::Exhausted { limit: 100 });
    let stop = Stop::new();
    stop.set();
    let e = wireframe(&body, &spec(0.01), &mut Budget::unlimited(), &stop).unwrap_err();
    assert_eq!(e, TessError::Cancelled);
    // The budget is one unit a segment: the torus's two circles.
    let mut b = Budget::unlimited();
    wireframe(&body, &spec(0.01), &mut b, &Stop::new()).unwrap();
    assert_eq!(b.used(), 278 + 56);
}
