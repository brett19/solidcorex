// Written for solidcorex. Apache-2.0.
//! Non-manifold results (BO25): selections over the cells of two operands that keep walls
//! between solid cells, laminas and sheets between void cells, judged against closed forms.
//! Every body finishes (scx-brep's structure check); a regularised one passes
//! `scx_check::contract`, and one with a wall or a lamina is refused by it (the contract
//! proves one manifold solid) and classed by `scx_check::check` as `Mixed` or `Sheet`.
//!
//! Truths: two unit boxes sharing a face, an edge or a vertex; a box on another sharing a
//! patch of face; a box inside another; overlapping boxes; a box's and a cylinder's
//! sections by a plane.

use std::f64::consts::PI;

use scx_brep::op::{Cx, Done, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, RegionId};
use scx_check::{Class, Contacts, Scope};
use scx_num::budget::{Budget, Stop};

use super::face_face::{X, Z, cyl};
use super::whole::{INTERSECT, SUBTRACT, UNION, area, near, op, volume};
use super::{cuboid, unit};
use crate::{BooleanError, Faces, Membership, NoItems, Rule, combine};

fn run(a: &Brep, b: &Brep, rule: &Rule<'_>, doc: Document) -> Result<Done<NoItems>, BooleanError> {
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let settings = Settings::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    combine(a, b, rule, &mut cx)
}

fn built(a: &Brep, b: &Brep, rule: &Rule<'_>, doc: Document) -> Brep {
    run(a, b, rule, doc)
        .unwrap_or_else(|e| panic!("{rule:?}: {e:?}"))
        .body
}

/// What the body is made of, by `scx_check::check`, and whether the contract (with radial
/// edges allowed) proves it one manifold solid.
fn judged(body: &Brep, doc: Document) -> (Class, bool) {
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let settings = Settings::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let class = scx_check::check(body, &mut cx, Scope::Import)
        .unwrap()
        .class;
    let proved = scx_check::contract(body, &cx, Contacts::Radial).is_ok();
    (class, proved)
}

/// The shells of each solid region (by region id), and of the outside.
fn form(body: &Brep) -> (Vec<usize>, usize) {
    let solid = body
        .regions()
        .filter(|r| r.is_solid())
        .map(|r| r.shells().count())
        .collect();
    let outside = body
        .regions()
        .find(|r| r.is_outside())
        .map_or(0, |r| r.shells().count());
    (solid, outside)
}

/// The volume of each solid region, in region order: the signed volumes of its shells.
fn region_volumes(body: &Brep) -> Vec<f64> {
    body.regions()
        .filter(|r| r.is_solid())
        .map(|r| {
            r.shells()
                .map(|s| {
                    let e = scx_mass::enclose_shell(
                        body,
                        s.id(),
                        1e-9,
                        &mut Budget::unlimited(),
                        &Stop::new(),
                    )
                    .unwrap();
                    0.5 * (e.lo + e.hi)
                })
                .sum()
        })
        .collect()
}

/// The faces with solid on both sides, and with void on both.
fn two_sided(body: &Brep) -> (usize, usize) {
    let mut walls = 0;
    let mut laminas = 0;
    for f in body.faces() {
        let solid = [scx_brep::FaceSide::Front, scx_brep::FaceSide::Back]
            .map(|s| f.shell(s).region().is_solid());
        match solid {
            [true, true] => walls += 1,
            [false, false] => laminas += 1,
            _ => {}
        }
    }
    (walls, laminas)
}

/// The total area of the faces.
fn faces_area(body: &Brep) -> f64 {
    body.faces().map(|f| area(body, f.id())).sum()
}

fn doc() -> Document {
    Document::default()
}

/// The operand pairs every regularised case runs: (name, a, b, volume of union, of
/// intersection, of `a` less `b`).
fn pairs(doc: Document) -> Vec<(&'static str, Brep, Brep, [f64; 3])> {
    vec![
        (
            "face",
            unit(doc),
            cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0], doc),
            [2.0, 0.0, 1.0],
        ),
        (
            "edge",
            unit(doc),
            cuboid([1.0, 1.0, 0.0], [2.0, 2.0, 1.0], doc),
            [2.0, 0.0, 1.0],
        ),
        (
            "overlap",
            cuboid([0.0, 0.0, 0.0], [2.0, 1.0, 1.0], doc),
            cuboid([1.0, 0.5, 0.5], [3.0, 1.5, 1.5], doc),
            [3.75, 0.25, 1.75],
        ),
    ]
}

/// A rule spelt out by its cells is the Boolean: the same body as `boolean`'s.
#[test]
fn spelt_rules_are_the_booleans() {
    let doc = doc();
    let union = |m: Membership| m != Membership::NONE;
    let inter = |m: Membership| m.contains(0) && m.contains(1);
    let less = |m: Membership| m.contains(0) && !m.contains(1);
    for (name, a, b, want) in pairs(doc) {
        for ((which, solid), v) in [
            (UNION, &union as &dyn Fn(Membership) -> bool),
            (INTERSECT, &inter),
            (SUBTRACT, &less),
        ]
        .into_iter()
        .zip(want)
        {
            let rule = Rule {
                solid,
                faces: Faces::Boundary,
            };
            let got = built(&a, &b, &rule, doc);
            let pin = op(&a, &b, which, doc).unwrap().body;
            let count = |b: &Brep| [b.faces().len(), b.edges().len(), b.vertices().len()];
            assert_eq!(count(&got), count(&pin), "{name} {which:?}");
            assert_eq!(form(&got), form(&pin), "{name} {which:?}");
            if v > 0.0 {
                near(volume(&got), v, 1e-9);
            }
        }
    }
}

/// Two unit boxes sharing a face. Their closed intersection is that face: a lamina of area
/// 1 in the outside, volume 0, its four edges free. Their union with walls is two unit
/// regions with the face between them, solid on both sides.
#[test]
fn boxes_sharing_a_face() {
    let doc = doc();
    let a = unit(doc);
    let b = cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0], doc);

    let lamina = built(&a, &b, &Rule::with(INTERSECT, Faces::Closure), doc);
    assert_eq!(lamina.faces().len(), 1);
    assert_eq!(lamina.edges().len(), 4);
    assert!(lamina.edges().all(|e| e.fins().len() == 1));
    assert_eq!(two_sided(&lamina), (0, 1));
    assert_eq!(form(&lamina), (vec![], 1));
    near(faces_area(&lamina), 1.0, 1e-12);
    near(volume(&lamina), 0.0, 1e-12);
    assert_eq!(judged(&lamina, doc), (Class::Sheet, false));

    let walls = built(&a, &b, &Rule::with(UNION, Faces::Walls), doc);
    assert_eq!(
        [
            walls.faces().len(),
            walls.edges().len(),
            walls.vertices().len()
        ],
        [11, 20, 12]
    );
    assert_eq!(two_sided(&walls), (1, 0));
    assert_eq!(form(&walls), (vec![1, 1], 1));
    near(volume(&walls), 2.0, 1e-12);
    for v in region_volumes(&walls) {
        near(v, 1.0, 1e-9);
    }
    assert_eq!(judged(&walls, doc), (Class::Mixed, false));

    // The closure keeps nothing more where the cells either side are solid, or where the
    // face bounds the result.
    for which in [UNION, SUBTRACT] {
        let closed = built(&a, &b, &Rule::with(which, Faces::Closure), doc);
        let regular = op(&a, &b, which, doc).unwrap().body;
        assert_eq!(closed.faces().len(), regular.faces().len(), "{which:?}");
        assert_eq!(judged(&closed, doc), (Class::Solid, true), "{which:?}");
    }
}

/// A box standing on another over a 1 by 1 patch of its top face: the closed intersection
/// is the patch, the union with walls two regions with the patch between them.
#[test]
fn box_on_a_box_shares_a_patch() {
    let doc = doc();
    let a = cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cuboid([1.0, 1.0, 1.0], [3.0, 3.0, 2.0], doc);

    let lamina = built(&a, &b, &Rule::with(INTERSECT, Faces::Closure), doc);
    assert_eq!(lamina.faces().len(), 1);
    assert_eq!(two_sided(&lamina), (0, 1));
    near(faces_area(&lamina), 1.0, 1e-12);
    assert_eq!(form(&lamina), (vec![], 1));
    assert_eq!(judged(&lamina, doc), (Class::Sheet, false));

    let walls = built(&a, &b, &Rule::with(UNION, Faces::Walls), doc);
    assert_eq!(two_sided(&walls), (1, 0));
    assert_eq!(form(&walls), (vec![1, 1], 1));
    near(volume(&walls), 8.0, 1e-12);
    for v in region_volumes(&walls) {
        near(v, 4.0, 1e-9);
    }
    let wall = walls
        .faces()
        .find(|f| {
            f.shell(scx_brep::FaceSide::Back).region().is_solid()
                && f.shell(scx_brep::FaceSide::Front).region().is_solid()
        })
        .unwrap();
    near(area(&walls, wall.id()), 1.0, 1e-12);
    assert_eq!(judged(&walls, doc), (Class::Mixed, false));
}

/// Two unit boxes sharing an edge: no face lies between two cells of one kind, so the
/// union with walls is the regularised union (two regions meeting at a four-fin edge),
/// and the closed intersection, a segment, keeps no face: the closure keeps faces only.
#[test]
fn boxes_sharing_an_edge() {
    let doc = doc();
    let a = unit(doc);
    let b = cuboid([1.0, 1.0, 0.0], [2.0, 2.0, 1.0], doc);
    let walls = built(&a, &b, &Rule::with(UNION, Faces::Walls), doc);
    assert_eq!(form(&walls), (vec![1, 1], 1));
    assert_eq!(walls.edges().filter(|e| e.fins().len() == 4).count(), 1);
    near(volume(&walls), 2.0, 1e-12);
    assert_eq!(judged(&walls, doc), (Class::Solid, true));
    let closed = built(&a, &b, &Rule::with(INTERSECT, Faces::Closure), doc);
    assert_eq!(closed.faces().len(), 0);
}

/// Two unit boxes sharing a vertex: as for an edge, with two outer shells meeting at it.
#[test]
fn boxes_sharing_a_vertex() {
    let doc = doc();
    let a = unit(doc);
    let b = cuboid([1.0, 1.0, 1.0], [2.0, 2.0, 2.0], doc);
    let walls = built(&a, &b, &Rule::with(UNION, Faces::Walls), doc);
    assert_eq!(form(&walls), (vec![1, 1], 2));
    near(volume(&walls), 2.0, 1e-12);
    assert_eq!(judged(&walls, doc).0, Class::Solid);
    let closed = built(&a, &b, &Rule::with(INTERSECT, Faces::Closure), doc);
    assert_eq!(closed.faces().len(), 0);
}

/// Overlapping boxes, every cell its own region: `a` only (1.75), both (0.25), `b` only
/// (1.75), with the six faces of the overlap two-sided walls.
#[test]
fn overlapping_boxes_cell_by_cell() {
    let doc = doc();
    let a = cuboid([0.0, 0.0, 0.0], [2.0, 1.0, 1.0], doc);
    let b = cuboid([1.0, 0.5, 0.5], [3.0, 1.5, 1.5], doc);
    let cells = built(&a, &b, &Rule::with(UNION, Faces::Walls), doc);
    assert_eq!(form(&cells), (vec![1, 1, 1], 1));
    assert_eq!(two_sided(&cells), (6, 0));
    near(volume(&cells), 3.75, 1e-12);
    let mut v = region_volumes(&cells);
    v.sort_by(f64::total_cmp);
    for (got, want) in v.into_iter().zip([0.25, 1.75, 1.75]) {
        near(got, want, 1e-9);
    }
    assert_eq!(judged(&cells, doc), (Class::Mixed, false));

    // Either operand but not both: two regions that meet along the overlap's edges.
    let xor = |m: Membership| m == Membership::A || m == Membership::B;
    let rule = Rule {
        solid: &xor,
        faces: Faces::Boundary,
    };
    let body = built(&a, &b, &rule, doc);
    near(volume(&body), 3.5, 1e-12);
    assert_eq!(two_sided(&body), (0, 0));
}

/// A box inside another, every cell its own region: the outer cell's region has the inner
/// box's faces as its cavity's wall, the inner cell its own region inside.
#[test]
fn box_in_a_box_cell_by_cell() {
    let doc = doc();
    let a = cuboid([0.0; 3], [3.0; 3], doc);
    let b = cuboid([1.0; 3], [2.0; 3], doc);
    let cells = built(&a, &b, &Rule::with(UNION, Faces::Walls), doc);
    let (mut solid, outside) = form(&cells);
    solid.sort_unstable();
    assert_eq!((solid, outside), (vec![1, 2], 1));
    assert_eq!(two_sided(&cells), (6, 0));
    near(volume(&cells), 27.0, 1e-12);
    let mut v = region_volumes(&cells);
    v.sort_by(f64::total_cmp);
    near(v[0], 1.0, 1e-9);
    near(v[1], 26.0, 1e-9);
    // The inner region's shell is not the outer region's cavity wall: two shells on the
    // six faces, one each side.
    let inner = cells
        .regions()
        .find(|r| r.is_solid() && r.shells().count() == 1)
        .unwrap()
        .id();
    assert_ne!(inner, RegionId::OUTSIDE);
    assert_eq!(judged(&cells, doc), (Class::Mixed, false));
}

/// A section: the faces of `b` inside `a`, as a sheet (void both sides, its edges free).
fn section(a: &Brep, b: &Brep, doc: Document) -> Brep {
    let never = |_: Membership| false;
    let inside_a = |m: Membership, o: Membership| {
        m.union(o) == Membership::BOTH && m.intersection(o) == Membership::A
    };
    let rule = Rule {
        solid: &never,
        faces: Faces::Where(&inside_a),
    };
    built(a, b, &rule, doc)
}

/// A box's section by a plane through it is a 2 by 2 square; a cylinder's across its axis
/// a disc of area pi r^2.
#[test]
fn sections_by_a_plane() {
    let doc = doc();
    let a = cuboid([0.0; 3], [2.0; 3], doc);
    let b = cuboid([1.0, -1.0, -1.0], [3.0, 3.0, 3.0], doc);
    let sheet = section(&a, &b, doc);
    assert_eq!(sheet.faces().len(), 1);
    assert_eq!(two_sided(&sheet), (0, 1));
    assert!(sheet.edges().all(|e| e.fins().len() == 1));
    assert_eq!(form(&sheet), (vec![], 1));
    near(faces_area(&sheet), 4.0, 1e-12);
    assert_eq!(judged(&sheet, doc), (Class::Sheet, false));

    let c = cyl([0.0; 3], Z, X, 1.5, 4.0, doc);
    let cut = cuboid([-3.0, -3.0, 2.5], [3.0, 3.0, 6.0], doc);
    let disc = section(&c, &cut, doc);
    assert_eq!(disc.faces().len(), 1);
    near(faces_area(&disc), PI * 1.5 * 1.5, 1e-9);
    assert_eq!(judged(&disc, doc), (Class::Sheet, false));
}

/// The closure of a selection whose cells meet only at faces that bound it is the
/// selection: a regularised result is unchanged, cylinder and box alike.
#[test]
fn closure_of_a_regular_result_is_it() {
    let doc = doc();
    let c = cyl([0.0; 3], Z, X, 1.0, 3.0, doc);
    let b = cuboid([0.0, -2.0, 1.0], [2.0, 2.0, 2.0], doc);
    for which in [UNION, INTERSECT, SUBTRACT] {
        let closed = built(&c, &b, &Rule::with(which, Faces::Closure), doc);
        let regular = op(&c, &b, which, doc).unwrap().body;
        assert_eq!(closed.faces().len(), regular.faces().len(), "{which:?}");
        near(volume(&closed), volume(&regular), 1e-12);
    }
}
