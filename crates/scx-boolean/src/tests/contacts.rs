// Written for solidcorex. Apache-2.0.
//! Where a result meets itself, or one lump nests in another's cavity (BO7, 72 §14.36).
//! D5: a vertex or edge contact is built (a vertex of two regions, an edge of four fins in
//! radial order); a touch inside a face needs a point loop scx-brep lacks and is
//! `NonManifold`. Every built result passes `scx_check::contract`, except an empty one or
//! one with an edge of four fins, which the contract refuses by design (see [`result`]);
//! every volume is the closed form's.
//!
//! Ported (judged by outcome, against closed forms): solidcore's `boolean_api` cases
//! `tangent-sphere-box`, `seam-sphere-box`, `seam-box-sphere`, `off-seam-sphere-box`,
//! `seam-sphere-sphere` and `seam-box-minus-sphere`, and `point_contact_unions_check_clean`
//! over all eleven of its point contacts. The pin builds each with a point loop; here the
//! operation the pin runs is `NonManifold` (D5), and the operations that keep only one
//! side of the touch are built.

use std::f64::consts::PI;

use scx_bodyq::{PointClass, classify_point};
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, Entity, RegionId};
use scx_check::{Breach, ContractError};
use scx_make::torus;
use scx_mass::RelAccuracy;
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::Vec3;
use scx_tess::{TessSpec, tessellate};

use super::face_face::{X, Z, ball, cyl};
use super::whole::{INTERSECT, SUBTRACT, UNION, checked, near, op, volume};
use super::{cuboid, pt};
use crate::{BooleanError, BooleanOp, BooleanUnsupported};

fn sphere_v(r: f64) -> f64 {
    4.0 / 3.0 * PI * r * r * r
}

fn ring(major: f64, minor: f64, doc: Document) -> Brep {
    let frame = Frame::new(
        pt([0.0; 3]),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    torus(&frame, Length::from_mm(major), Length::from_mm(minor), &doc)
        .unwrap()
        .body
}

fn non_manifold(a: &Brep, b: &Brep, which: BooleanOp, doc: Document, name: &str) {
    let e = op(a, b, which, doc).unwrap_err();
    assert!(
        matches!(
            e,
            BooleanError::Unsupported(BooleanUnsupported::NonManifold { .. })
        ),
        "{name} {which:?}: {e:?}"
    );
    assert_eq!(e.name(), "non_manifold", "{name}");
}

/// The Boolean's result, checked. A body with no face is the empty result (the contract
/// refuses it by its first test, so it is not run). A body with an edge of more than two
/// fins is a pinch D5 builds and the contract refuses by its third test: it must fail
/// there and nowhere sooner, and its orientation is proved by `scx_mass::outward` (the
/// contract's fifth test). Any other body passes the whole contract.
fn result(a: &Brep, b: &Brep, which: BooleanOp, doc: Document) -> Brep {
    let body = op(a, b, which, doc)
        .unwrap_or_else(|e| panic!("{which:?}: {e:?}"))
        .body;
    if body.faces().len() == 0 {
        assert_eq!(body.regions().filter(|r| r.is_solid()).count(), 0);
        return body;
    }
    let Some(pinch) = body.edges().find(|e| e.fins().len() > 2) else {
        return checked(a, b, which, doc).body;
    };
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let settings = Settings::new();
    let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    match scx_check::contract(&body, &cx) {
        Err(ContractError::Broken(Breach::NonManifoldEdge { edge, fins })) => {
            assert_eq!((edge, fins), (pinch.id(), pinch.fins().len()), "{which:?}");
        }
        other => panic!("{which:?}: contract {other:?}"),
    }
    scx_mass::outward(&body, &mut Budget::unlimited(), &stop)
        .unwrap_or_else(|e| panic!("{which:?}: outward {e:?}"));
    body
}

/// Solid regions, and shells per region (solid ones first, then the outside).
fn form(body: &Brep) -> (usize, Vec<usize>, usize) {
    let solid: Vec<usize> = body
        .regions()
        .filter(|r| r.is_solid())
        .map(|r| r.shells().count())
        .collect();
    let outside = body
        .regions()
        .find(|r| r.is_outside())
        .map_or(0, |r| r.shells().count());
    (solid.len(), solid, outside)
}

/// One of the pin's point contacts: operands, the pin's operation, and what each
/// operation that keeps one side of the touch builds (its volume; zero is empty).
struct Point {
    name: &'static str,
    a: Brep,
    b: Brep,
    pin: BooleanOp,
    built: Vec<(BooleanOp, bool, f64)>,
    /// A pair with no closed-form intersection: every operation is `SsiPair` until step 5,
    /// which builds them (and then the touch is `NonManifold` as above).
    ssi: bool,
}

fn point_contacts(doc: Document) -> Vec<Point> {
    let box_v = 1312.5;
    let tor = 2.0 * PI * PI * 5.0 * 1.5 * 1.5;
    let (sin, cos) = (fmath::sin(PI / 6.0), fmath::cos(PI / 6.0));
    let a45 = PI / 4.0;
    let box_a = || cuboid([10.0, 0.0, 0.0], [15.0, 17.5, 15.0], doc);
    // (op, operands swapped, volume): b - a is `SUBTRACT` with the operands swapped.
    let apart = |va: f64, vb: f64| {
        vec![
            (SUBTRACT, false, va),
            (SUBTRACT, true, vb),
            (INTERSECT, false, 0.0),
        ]
    };
    vec![
        Point {
            name: "tangent-sphere-box",
            a: cuboid([0.0; 3], [10.0; 3], doc),
            b: ball([5.0, 5.0, 13.0], 3.0, doc),
            pin: UNION,
            built: apart(1000.0, sphere_v(3.0)),
            ssi: false,
        },
        Point {
            name: "seam-sphere-box",
            a: ball([2.5, 15.0, 12.5], 7.5, doc),
            b: box_a(),
            pin: UNION,
            built: apart(sphere_v(7.5), box_v),
            ssi: false,
        },
        Point {
            name: "seam-box-sphere",
            a: box_a(),
            b: ball([2.5, 15.0, 12.5], 7.5, doc),
            pin: UNION,
            built: apart(box_v, sphere_v(7.5)),
            ssi: false,
        },
        Point {
            name: "off-seam-sphere-box",
            a: ball([12.5, -7.5, 7.5], 7.5, doc),
            b: box_a(),
            pin: UNION,
            built: apart(sphere_v(7.5), box_v),
            ssi: false,
        },
        Point {
            name: "seam-sphere-sphere",
            a: ball([0.0; 3], 3.0, doc),
            b: ball([5.0 * cos, 0.0, 5.0 * sin], 2.0, doc),
            pin: UNION,
            built: apart(sphere_v(3.0), sphere_v(2.0)),
            ssi: false,
        },
        Point {
            name: "seam-cylinder-sphere",
            a: cyl([0.0; 3], Z, X, 2.0, 10.0, doc),
            b: ball([5.0, 0.0, 5.0], 3.0, doc),
            pin: UNION,
            built: apart(40.0 * PI, sphere_v(3.0)),
            ssi: true,
        },
        Point {
            name: "off-seam-cylinder-sphere",
            a: cyl([0.0; 3], Z, X, 2.0, 10.0, doc),
            b: ball([0.0, 5.0, 5.0], 3.0, doc),
            pin: UNION,
            built: apart(40.0 * PI, sphere_v(3.0)),
            ssi: true,
        },
        Point {
            name: "seam-torus-sphere",
            a: ring(5.0, 1.5, doc),
            b: ball(
                [5.0 + 3.5 * fmath::cos(a45), 0.0, 3.5 * fmath::sin(a45)],
                2.0,
                doc,
            ),
            pin: UNION,
            built: apart(tor, sphere_v(2.0)),
            ssi: true,
        },
        Point {
            name: "seam-torus-box",
            a: ring(5.0, 1.5, doc),
            b: cuboid([-3.0, 6.5, -3.0], [3.0, 10.5, 3.0], doc),
            pin: UNION,
            built: apart(tor, 144.0),
            ssi: true,
        },
        Point {
            name: "seam-vertex-torus-box",
            a: ring(5.0, 1.5, doc),
            b: cuboid([6.5, -3.0, -3.0], [10.5, 3.0, 3.0], doc),
            pin: UNION,
            built: apart(tor, 144.0),
            ssi: true,
        },
        Point {
            // The box less its inscribed sphere, which touches every face at its centre.
            name: "seam-box-minus-sphere",
            a: cuboid([0.0; 3], [10.0; 3], doc),
            b: ball([5.0; 3], 5.0, doc),
            pin: SUBTRACT,
            built: vec![
                (UNION, false, 1000.0),
                (INTERSECT, false, sphere_v(5.0)),
                (SUBTRACT, true, 0.0),
            ],
            ssi: false,
        },
    ]
}

#[test]
fn point_contacts_are_non_manifold_and_one_sided_results_are_built() {
    // `point_contact_unions_check_clean`: the pin builds each with a point loop. D5: the
    // operation that keeps both sides of the touch is `NonManifold`; each operation that
    // keeps one side builds, passes the contract and has the closed-form volume.
    let doc = Document::default();
    for c in point_contacts(doc) {
        if c.ssi {
            for (which, swapped) in [
                (UNION, false),
                (SUBTRACT, false),
                (SUBTRACT, true),
                (INTERSECT, false),
            ] {
                let (a, b) = if swapped { (&c.b, &c.a) } else { (&c.a, &c.b) };
                let e = op(a, b, which, doc).unwrap_err();
                assert_eq!(e.name(), "ssi_pair", "{} {which:?}", c.name);
            }
            continue;
        }
        non_manifold(&c.a, &c.b, c.pin, doc, c.name);
        for (which, swapped, want) in c.built {
            let (a, b) = if swapped { (&c.b, &c.a) } else { (&c.a, &c.b) };
            let body = result(a, b, which, doc);
            if want == 0.0 {
                assert_eq!(body.faces().count(), 0, "{} {which:?}: empty", c.name);
            } else {
                let v = volume(&body);
                assert!(
                    (v - want).abs() <= 1e-7 * want,
                    "{} {which:?} swapped {swapped}: {v} against {want}",
                    c.name
                );
            }
        }
    }
}

/// The unit cube and the cube sharing its edge x = y = 1.
fn edge_pair(doc: Document) -> (Brep, Brep) {
    (
        cuboid([0.0; 3], [1.0; 3], doc),
        cuboid([1.0, 1.0, 0.0], [2.0, 2.0, 1.0], doc),
    )
}

#[test]
fn blocks_sharing_an_edge_unite_into_two_regions_with_one_outer_shell() {
    // D5: built. The edge x = y = 1 has four fins; the two solids are two regions, one
    // shell each, and the outside has one shell: the two void sectors at the edge are one
    // piece of space.
    let doc = Document::default();
    let (a, b) = edge_pair(doc);
    let body = result(&a, &b, UNION, doc);
    near(volume(&body), 2.0, 1e-9);
    assert_eq!(form(&body), (2, vec![1, 1], 1));
    let pinched: Vec<_> = body.edges().filter(|e| e.fins().len() > 2).collect();
    assert_eq!(pinched.len(), 1);
    assert_eq!(pinched[0].fins().len(), 4);
    // Both orders, and the swapped operands, build the same form.
    let body = result(&b, &a, UNION, doc);
    assert_eq!(form(&body), (2, vec![1, 1], 1));
}

#[test]
fn blocks_sharing_a_corner_unite_into_two_regions_and_two_outer_shells() {
    // D5: built. The vertex (1, 1, 1) is shared by two regions; no edge has more than two
    // fins, so the outside keeps one shell per lump: two shells meeting at the vertex.
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cuboid([1.0; 3], [2.0; 3], doc);
    let body = result(&a, &b, UNION, doc);
    near(volume(&body), 2.0, 1e-9);
    assert_eq!(form(&body), (2, vec![1, 1], 2));
    assert!(body.edges().all(|e| e.fins().len() == 2));
}

#[test]
fn a_body_touching_itself_along_an_edge_is_one_region_and_one_shell() {
    // Two pillars on the diagonal, bridged at the top: once the second pillar is added,
    // the body touches itself along the line x = y = 1, z in [0, 2].
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0, 1.0, 3.0], doc);
    let bridge = cuboid([0.0, 0.0, 2.0], [2.0, 2.0, 3.0], doc);
    let b = cuboid([1.0, 1.0, 0.0], [2.0, 2.0, 3.0], doc);
    let ad = result(&a, &bridge, UNION, doc);
    let body = result(&ad, &b, UNION, doc);
    near(volume(&body), 8.0, 1e-9);
    assert_eq!(form(&body), (1, vec![1], 1));
    let pinched: Vec<_> = body.edges().filter(|e| e.fins().len() > 2).collect();
    assert_eq!(pinched.len(), 1);
    // The same body cut back: less the bridge's middle is two pillars meeting on a line.
    let cut = cuboid([0.0, 0.0, 2.0], [2.0, 2.0, 3.0], doc);
    let pillars = result(&body, &cut, SUBTRACT, doc);
    near(volume(&pillars), 4.0, 1e-9);
    assert_eq!(form(&pillars), (2, vec![1, 1], 1));
}

#[test]
fn faces_tangent_along_a_contact_edge_are_non_manifold() {
    // A cylinder lying along the unit cube's edge x = 1, z = 1, its side tangent to the
    // face x = 1 there: two faces leave that edge in one direction, so the result meets
    // itself inside a face as well as along the edge.
    let doc = Document::default();
    let a = cuboid([0.0; 3], [1.0; 3], doc);
    let b = cyl([2.0, 0.0, 1.0], [0.0, 1.0, 0.0], X, 1.0, 1.0, doc);
    non_manifold(&a, &b, UNION, doc, "tangent cylinder");
}

#[test]
fn the_pinched_body_answers_queries() {
    // D5's smoke test on the edge contact: point classification, mass, tessellation and the
    // vertex star all take a vertex of two regions and an edge of four fins.
    let doc = Document::default();
    let (a, b) = edge_pair(doc);
    let body = result(&a, &b, UNION, doc);
    let (mut budget, stop) = (Budget::unlimited(), Stop::new());
    let class = |p: [f64; 3], budget: &mut Budget| classify_point(&body, pt(p), budget, &stop);
    let (PointClass::In(ra), PointClass::In(rb)) = (
        class([0.5; 3], &mut budget).unwrap(),
        class([1.5, 1.5, 0.5], &mut budget).unwrap(),
    ) else {
        panic!("inside points");
    };
    assert!(ra != rb && ra != RegionId::OUTSIDE && rb != RegionId::OUTSIDE);
    for p in [[1.5, 0.5, 0.5], [0.5, 1.5, 0.5], [3.0; 3]] {
        assert_eq!(
            class(p, &mut budget).unwrap(),
            PointClass::In(RegionId::OUTSIDE),
            "{p:?}"
        );
    }
    let pinched = body.edges().find(|e| e.fins().len() == 4).unwrap();
    assert_eq!(
        class([1.0, 1.0, 0.5], &mut budget).unwrap(),
        PointClass::On(Entity::Edge(pinched.id()))
    );
    // Mass: volume and area (each cube's six faces).
    near(volume(&body), 2.0, 1e-9);
    let area = scx_mass::area(
        &body,
        RelAccuracy::new(1e-10).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &stop,
    )
    .unwrap()
    .value;
    near(area, 12.0, 1e-9);
    // Tessellation: the mesh encloses the volume.
    let spec = TessSpec::new(Length::from_mm(0.01), body.resolution()).unwrap();
    let mesh = tessellate(&body, &spec, &mut Budget::unlimited(), &stop).unwrap();
    let six: f64 = mesh
        .triangles
        .iter()
        .map(|t| {
            let [p, q, r] = t.map(|i| mesh.positions[i as usize]);
            let (p, q, r) = (p.to_vec(), q.to_vec(), r.to_vec());
            p.dot(q.cross(r))
        })
        .sum();
    near(six / 6.0, 2.0, 1e-9);
    // The vertex star: each end of the contact edge has the contact edge and two edges of
    // each cube.
    for v in [pinched.start(), pinched.end()] {
        assert_eq!(v.edges().count(), 5, "{:?}", v.point());
    }
}

#[test]
fn two_lumps_one_with_a_cavity_place_the_cavity_by_a_point_test() {
    // BO4's open question 1: a block with a cavity, and a block apart from it. The far
    // block's box does not hold the cavity's, so the boxes decide with no point test.
    let doc = Document::default();
    let shell = cuboid([0.0; 3], [4.0; 3], doc);
    let hole = cuboid([1.0; 3], [3.0; 3], doc);
    let holed = result(&shell, &hole, SUBTRACT, doc);
    let far = cuboid([10.0, 0.0, 0.0], [12.0, 2.0, 2.0], doc);
    for (a, b) in [(&holed, &far), (&far, &holed)] {
        let body = result(a, b, UNION, doc);
        near(volume(&body), 56.0 + 8.0, 1e-9);
        let mut f = form(&body);
        f.1.sort_unstable();
        assert_eq!(f, (2, vec![1, 2], 2));
        let mut budget = Budget::unlimited();
        let stop = Stop::new();
        let PointClass::In(r) = classify_point(&body, pt([2.0; 3]), &mut budget, &stop).unwrap()
        else {
            panic!("the cavity's middle");
        };
        assert!(!body.region(r).unwrap().is_solid());
        assert_ne!(r, RegionId::OUTSIDE);
    }
    // A square ring round the holed block: both lumps' boxes hold the cavity's, so the
    // point test, not the boxes, puts the cavity in the holed block.
    let slab = cuboid([-2.0, -2.0, 0.0], [6.0, 6.0, 4.0], doc);
    let through = cuboid([-1.0, -1.0, -1.0], [5.0, 5.0, 5.0], doc);
    let frame = result(&slab, &through, SUBTRACT, doc);
    for (a, b) in [(&holed, &frame), (&frame, &holed)] {
        let body = result(a, b, UNION, doc);
        near(volume(&body), 56.0 + 112.0, 1e-9);
        let mut f = form(&body);
        f.1.sort_unstable();
        assert_eq!(f, (2, vec![1, 2], 2));
        let mut budget = Budget::unlimited();
        let stop = Stop::new();
        let PointClass::In(r) = classify_point(&body, pt([0.5; 3]), &mut budget, &stop).unwrap()
        else {
            panic!("the holed block's material");
        };
        let holder = body.region(r).unwrap();
        assert!(holder.is_solid());
        assert_eq!(
            holder.shells().count(),
            2,
            "the cavity is the holed block's"
        );
    }
}

#[test]
fn a_lump_inside_a_cavity_sits_in_the_cavity_void() {
    // A block with a cavity, and a block inside the cavity: the inner block's outer void
    // shell is the cavity's, not the outside's.
    let doc = Document::default();
    let shell = cuboid([0.0; 3], [6.0; 3], doc);
    let hole = cuboid([1.0; 3], [5.0; 3], doc);
    let holed = result(&shell, &hole, SUBTRACT, doc);
    let inner = cuboid([2.0; 3], [4.0; 3], doc);
    for (a, b) in [(&holed, &inner), (&inner, &holed)] {
        let body = result(a, b, UNION, doc);
        near(volume(&body), 216.0 - 64.0 + 8.0, 1e-9);
        let mut f = form(&body);
        f.1.sort_unstable();
        assert_eq!(f, (2, vec![1, 2], 1));
        let voids: Vec<_> = body
            .regions()
            .filter(|r| !r.is_solid() && !r.is_outside())
            .collect();
        assert_eq!(voids.len(), 1);
        assert_eq!(
            voids[0].shells().count(),
            2,
            "the cavity and the lump's outside"
        );
        let mut budget = Budget::unlimited();
        let stop = Stop::new();
        assert_eq!(
            classify_point(&body, pt([1.5; 3]), &mut budget, &stop).unwrap(),
            PointClass::In(voids[0].id())
        );
    }
}

#[test]
fn a_union_that_fills_nothing_new_keeps_working_when_nested() {
    // Subtracting a lump inside a cavity from the holed block changes nothing: the block
    // inside the cavity is outside the material.
    let doc = Document::default();
    let shell = cuboid([0.0; 3], [6.0; 3], doc);
    let hole = cuboid([1.0; 3], [5.0; 3], doc);
    let holed = result(&shell, &hole, SUBTRACT, doc);
    let inner = cuboid([2.0; 3], [4.0; 3], doc);
    let body = result(&holed, &inner, SUBTRACT, doc);
    near(volume(&body), 216.0 - 64.0, 1e-9);
    assert_eq!(result(&holed, &inner, INTERSECT, doc).faces().count(), 0);
}

#[test]
fn one_sided_edge_contacts_build_each_operand() {
    // The two cubes sharing an edge: subtraction and intersection keep one side only.
    let doc = Document::default();
    let (a, b) = edge_pair(doc);
    near(volume(&result(&a, &b, SUBTRACT, doc)), 1.0, 1e-9);
    assert_eq!(result(&a, &b, INTERSECT, doc).faces().count(), 0);
}
