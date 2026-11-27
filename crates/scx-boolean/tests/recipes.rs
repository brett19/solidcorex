// Written for solidcorex. Apache-2.0.
//! The Boolean's benchmark, contact and coincident-face cases from the testkit's recipes
//! (T12), run through the public [`boolean`]. Each recipe is its operands (placed
//! primitives), the Boolean steps, and the truth stated in closed form (volume, area,
//! regions, shells), so no case's expected answer is read from the pin.
//!
//! Two kinds of test live here.
//! - *Live today:* the operand generators. Every part of every case builds through
//!   `scx-make` and its volume is the closed form's, so a later failure of a case is the
//!   Boolean's and not the operands'.
//! - *The cases themselves,* judged against their truth. Cases whose recipe says
//!   `Unsupported` (a touch inside a face, D5) expect that answer by name. The vertex and
//!   edge contacts are built (BO7): a vertex of two regions, and an edge of four faces in
//!   radial order.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeSet;

use scx_bodyq::{PointClass, classify_point};
use scx_boolean::{BooleanError, BooleanOp, boolean};
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::hash::mix;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::Surface;
use scx_testkit::csg::{Csg, ProbeSpec, Side, box_points};
use scx_testkit::recipe::{self, Op, Outcome, Part, Recipe, Shape, Topology, Unsupported};

/// Plate-and-gear benchmark bodies (BO6): many cylinder or block cuts in one result.
const BENCH: [&str; 4] = ["box-cyl-hole", "plate-4", "plate-12", "gear-60"];
/// Contacts at a point or along a line (BO7). The first two are built (two regions
/// pinched at a vertex or an edge); the rest touch inside a face and are
/// `Unsupported(NonManifold)` by D5.
const CONTACT_BUILT: [&str; 2] = ["corner-contact-union", "edge-contact-union"];
const CONTACT_IN_FACE: [&str; 6] = [
    "tangent-sphere-box",
    "seam-sphere-box",
    "seam-box-sphere",
    "off-seam-sphere-box",
    "seam-sphere-sphere",
    "seam-box-minus-sphere",
];
/// Coincident and near-coincident faces.
const COINCIDENT: [&str; 9] = [
    "box-box-union-flush",
    "coplanar-union",
    "coplanar-offset-0",
    "coplanar-offset-0.1c",
    "coplanar-offset-0.5c",
    "coplanar-offset-4c",
    "coplanar-offset-10c",
    "interleaved-grids-4",
    "interleaved-grids-8",
];

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn frame(p: &Part) -> Frame {
    let f = &p.frame;
    Frame::new(
        Point3::new(f.origin[0], f.origin[1], f.origin[2]),
        Vec3::new(f.z[0], f.z[1], f.z[2]),
        Vec3::new(f.x[0], f.x[1], f.x[2]),
    )
    .unwrap()
}

/// A part as a body of `doc`, through the constructors.
fn operand(p: &Part, doc: Document) -> Brep {
    let f = frame(p);
    match &p.shape {
        Shape::Block { size } => scx_make::block(&f, size.map(mm), &doc).map(|m| m.body),
        Shape::Cylinder { radius, height } => {
            scx_make::cylinder(&f, mm(*radius), mm(*height), &doc).map(|m| m.body)
        }
        Shape::Cone { radii, height } => {
            scx_make::cone(&f, radii.map(mm), mm(*height), &doc).map(|m| m.body)
        }
        Shape::Sphere { radius } => scx_make::sphere(&f, mm(*radius), &doc).map(|m| m.body),
        Shape::Torus { major, minor } => {
            scx_make::torus(&f, mm(*major), mm(*minor), &doc).map(|m| m.body)
        }
        Shape::Extrusion { .. } => panic!("no Boolean case here extrudes a profile"),
    }
    .unwrap()
}

fn op(o: Op) -> BooleanOp {
    match o {
        Op::Union => BooleanOp::Union,
        Op::Intersection => BooleanOp::Intersect,
        Op::Difference => BooleanOp::Subtract,
    }
}

fn case(name: &str) -> Recipe {
    recipe::all(Document::default().resolution().zone().mm())
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no recipe named {name}"))
}

fn all_names() -> impl Iterator<Item = &'static str> {
    BENCH
        .into_iter()
        .chain(CONTACT_BUILT)
        .chain(CONTACT_IN_FACE)
        .chain(COINCIDENT)
}

/// The recipe's steps on the running result.
fn run(r: &Recipe) -> Result<Brep, BooleanError> {
    let doc = Document::default();
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut body = operand(&r.base, doc);
    for s in &r.steps {
        let tool = operand(&s.tool, doc);
        let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
        body = boolean(&body, &tool, op(s.op), &mut cx)?.body;
    }
    Ok(body)
}

fn measure(
    b: &Brep,
    f: fn(
        &Brep,
        scx_mass::RelAccuracy,
        &mut Scratch,
        &mut Budget,
        &Stop,
    ) -> Result<scx_mass::Estimate, scx_mass::MassError>,
    accuracy: f64,
) -> f64 {
    let a = scx_mass::RelAccuracy::new(accuracy).unwrap();
    f(
        b,
        a,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

/// Judge one case against its recipe: built with the stated volume (T9), area, region and
/// shell counts and genus, and T10's points where the recipe has no slack; or `Unsupported`
/// (a refusal never applies to these).
fn check(name: &str) {
    let r = case(name);
    let got = run(&r);
    match &r.outcome {
        Outcome::Built(t) => {
            let body = got.unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let truth = r.truth.expect("a built recipe states its truth");
            let acc = 1e-9;
            let v = measure(&body, scx_mass::volume, acc);
            assert!(
                (v - truth.volume).abs() <= acc * truth.volume + truth.slack,
                "{name}: volume {v}, truth {}",
                truth.volume
            );
            if let Some(a) = truth.area {
                let got = measure(&body, scx_mass::area, acc);
                assert!(
                    (got - a).abs() <= acc * a + truth.slack,
                    "{name}: area {got}, truth {a}"
                );
            }
            let solid: Vec<_> = body.regions().filter(|r| r.is_solid()).collect();
            let shells: usize = solid.iter().map(|r| r.shells().count()).sum();
            assert_eq!(solid.len() as u32, t.solid_regions, "{name}: regions");
            assert_eq!(shells as u32, t.shells, "{name}: shells");
            assert_eq!(genus(&body, t), t.genus, "{name}: genus");
            // A merged gap moves the boundary by up to the slack, past the probes' offsets.
            if truth.slack == 0.0 {
                points(name, &body, &r.csg().unwrap());
            }
        }
        Outcome::Unsupported(u) => {
            let e = got.expect_err(name);
            // D5: a touch inside a face is named `non_manifold`; a pair with no closed
            // form is `ssi_pair`.
            let want = match u {
                Unsupported::NonManifold { .. } => "non_manifold",
                Unsupported::SsiPair { .. } => "ssi_pair",
                Unsupported::HornTorus => panic!("{name}: not a case here"),
            };
            assert_eq!(e.name(), want, "{name}: {e:?}");
        }
        other => panic!("{name}: not a case here: {other:?}"),
    }
}

/// The genus of `body` from the Euler-Poincare relation over its own cells, shell by shell:
/// `V_s - E_s + sum (2 - 2 g_f - L_f) = 2 (1 - G_s)` per shell `s` of a solid region, with
/// `L_f` the face's loops and `g_f` one only for a torus face with no loop. A vertex or edge
/// is counted once in each shell whose faces use it, so shells that touch at a vertex or
/// along an edge (D5 contacts) each keep their own. A seam is an edge like any other (the
/// face it cuts open is a disc), so no edge is dropped.
fn genus(body: &Brep, t: &Topology) -> u32 {
    let mut chi = 0i64;
    let mut shells = 0i64;
    for s in body.shells().filter(|s| !s.region().is_outside()) {
        shells += 1;
        let mut vertices = BTreeSet::new();
        let mut edges = BTreeSet::new();
        for (f, _) in s.faces() {
            let loops = f.loops().len() as i64;
            let g = i64::from(loops == 0 && matches!(f.surface(), Surface::Torus(_)));
            chi += 2 - 2 * g - loops;
            for u in f.uses() {
                edges.insert(u.edge().id());
                vertices.insert(u.start().id());
            }
        }
        chi += vertices.len() as i64 - edges.len() as i64;
    }
    assert_eq!(shells, i64::from(t.shells), "shells of solid regions");
    assert_eq!(chi % 2, 0, "Euler characteristic {chi} is odd");
    u32::try_from(shells - chi / 2).unwrap_or_else(|_| panic!("chi {chi} over {shells} shells"))
}

/// The coincidence the points are spaced by: the default resolution.
const C: f64 = 1e-5;
/// Seeded points in the result's grown box, and the most points on purpose taken.
const BOX_POINTS: usize = 300;
const PROBES: usize = 300;

/// T10's points against the result (72 §8.1): seeded points through its box, and points
/// on purpose (on each face, edge, vertex and meet of the recipe's parts that lies on the
/// result's boundary, and `1/2`, `2` and `10` coincidences off it). Each is classified by
/// `classify_point` on the result and judged by the recipe's CSG tree: a point on the
/// boundary is `On` and within 1.5 coincidences of the tree's; a point two coincidences
/// clear of it is in the region the tree says, solid or not.
fn points(name: &str, body: &Brep, csg: &Csg) {
    let (lo, hi) = csg.bounds().expect("a bounded recipe");
    let seed = mix(&[name.len() as u64, 0x0b06]);
    let mut at: Vec<([f64; 3], bool)> = box_points(lo, hi, 0.1, BOX_POINTS, seed)
        .into_iter()
        .map(|p| (p, false))
        .collect();
    let mut probes: Vec<(u64, [f64; 3], bool)> = csg
        .probes(&ProbeSpec {
            seed: mix(&[seed, 1]),
            per_face: 2,
            coincidence: C,
            extent: 5.0,
        })
        .into_iter()
        .enumerate()
        .map(|(i, p)| (mix(&[seed, 2, i as u64]), p.point, p.offset == 0.0))
        .collect();
    probes.sort_by_key(|&(k, ..)| k);
    at.extend(probes.into_iter().take(PROBES).map(|(_, p, on)| (p, on)));
    let [mut on, mut inside, mut outside] = [0usize; 3];
    for (p, on_feature) in at {
        let got = classify_point(
            body,
            Point3::new(p[0], p[1], p[2]),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap_or_else(|e| panic!("{name} {p:?}: {e}"));
        let solid = match got {
            PointClass::On(_) => None,
            PointClass::In(r) => Some(body.region(r).is_some_and(scx_brep::Region::is_solid)),
            _ => panic!("{name} {p:?}: {got:?}"),
        };
        let truth = csg.side(p, 2.0 * C);
        let ok = match solid {
            None => csg.side(p, 1.5 * C) == Side::Band,
            Some(true) => {
                truth == Side::In || (truth == Side::Band && csg.side(p, 1e-9) != Side::Out)
            }
            Some(false) => {
                truth == Side::Out || (truth == Side::Band && csg.side(p, 1e-9) != Side::In)
            }
        };
        assert!(ok, "{name} {p:?}: {got:?}, truth {truth:?}");
        assert!(
            !on_feature || solid.is_none(),
            "{name} {p:?}: on the boundary, but {got:?}"
        );
        match solid {
            None => on += 1,
            Some(true) => inside += 1,
            Some(false) => outside += 1,
        }
    }
    assert!(
        on > 20 && inside > 20 && outside > 20,
        "{name}: {on} on, {inside} in, {outside} out"
    );
}

// ---------------------------------------------------------------------------------------
// Live today

#[test]
fn every_named_case_exists_and_states_its_truth() {
    for n in all_names() {
        let r = case(n);
        assert!(!r.steps.is_empty(), "{n} is a Boolean");
        assert!(r.truth.is_some(), "{n} states a truth");
        if let Outcome::Built(t) = &r.outcome {
            assert_eq!(t.euler_defect(), Some(0), "{n}: Euler-Poincare");
        }
    }
}

#[test]
fn every_operand_builds_with_the_closed_form_volume() {
    let doc = Document::default();
    for n in all_names() {
        let r = case(n);
        for (i, p) in r.parts().enumerate() {
            let body = operand(p, doc);
            let want = p.solid().unwrap().volume();
            let got = measure(&body, scx_mass::volume, 1e-9);
            assert!(
                (got - want).abs() <= 1e-7 * want,
                "{n} part {i}: volume {got}, closed form {want}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// BO6: the benchmark bodies

#[test]
fn plate_12_has_144_holes() {
    let r = case("plate-12");
    assert_eq!(r.steps.len(), 144);
    check("plate-12");
}

#[test]
fn gear_60_has_sixty_slots() {
    let r = case("gear-60");
    assert_eq!(r.steps.len(), 60);
    check("gear-60");
}

#[test]
fn smaller_hole_cases() {
    check("box-cyl-hole");
    check("plate-4");
}

// ---------------------------------------------------------------------------------------
// BO7: contacts

#[test]
fn blocks_touching_at_a_vertex_or_along_an_edge_are_built() {
    for n in CONTACT_BUILT {
        check(n);
    }
}

#[test]
fn touches_inside_a_face_are_unsupported() {
    for n in CONTACT_IN_FACE {
        check(n);
    }
}

// ---------------------------------------------------------------------------------------
// Coincident and near-coincident faces

#[test]
fn coincident_and_near_coincident_faces() {
    for n in COINCIDENT {
        check(n);
    }
}
