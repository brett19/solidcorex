// Written for solidcorex. Apache-2.0.
//! The stitch's truth tests (74 §4.4, ST4), each judged against the solid the sheets came
//! from and against closed forms, never against the pin's output.
//!
//! - *Exploded recipes:* testkit bodies and Boolean results taken apart into one sheet per
//!   face and stitched back are the same solid: shells, faces, edges and vertices, the
//!   closed-form volume and area through `scx-mass`, and `scx-check`'s contract.
//! - *Perturbation:* one sheet moved just inside the coincidence stitches; just outside it
//!   joins nothing, with the typed report.
//! - *Permutation:* any order of the sheets gives the same bits.
//! - *The pin's truths:* a cap laid on a stitched open box closes it (the pin's
//!   `a_cap_on_a_cube_s_top_edges_closes_into_one_box`, volume 1500).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeSet;
use std::f64::consts::PI;

use scx_boolean::{BooleanOp, boolean};
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, Entity, FaceSide};
use scx_check::{Contacts, contract};
use scx_mass::{RelAccuracy, area, outward};
use scx_num::budget::{Budget, Stop};
use scx_num::math::Affine;
use scx_num::vector3d::Vec3;
use scx_stitch::{Left, SolidFault, StitchError, StitchRefusal, Want, stitch};
use scx_testkit::bodies::Kind;

mod common;
use common::*;

/// The coincidence of the default document, in mm.
const ZONE: f64 = 1.0e-5;
/// Just inside the coincidence: a join leaves the kept edge this far off the absorbed face,
/// past the exact pcurve writer's `Resolution::approx` (half the zone), so the absorbed
/// face's use gets the kept curve's projection with its certified gap recorded (74 §40,
/// `a_join_past_the_pcurve_tolerance_records_its_certified_gap`).
const INSIDE: f64 = 0.9 * ZONE;

/// Shells, faces, edges and vertices.
fn counts(b: &Brep) -> [usize; 4] {
    [
        b.shells().count(),
        b.faces().count(),
        b.edges().count(),
        b.vertices().count(),
    ]
}

fn surface_area(b: &Brep) -> f64 {
    area(
        b,
        RelAccuracy::new(1e-12).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

fn close(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= 1e-9 * want.abs().max(1.0),
        "{what}: {got}, want {want}"
    );
}

/// `body` taken apart and stitched back is the same solid: the counts, the volume and
/// the area (`truth`, closed form, and the body's own), the outward certificate and the
/// check's contract.
fn same_solid(body: &Brep, truth: Truth, what: &str) -> Brep {
    let sheets = explode(body, |_| false);
    let out = run(&refs(&sheets)).unwrap_or_else(|e| panic!("{what}: {e:?}"));
    let b = out.done.body;
    assert_eq!(counts(&b), counts(body), "{what}: counts");
    assert_eq!(
        b.regions().count(),
        body.regions().count(),
        "{what}: regions"
    );
    assert!(out.report.left.is_empty(), "{what}: {:?}", out.report.left);
    close(vol(&b), vol(body), what);
    close(surface_area(&b), surface_area(body), what);
    if let Some((v, a)) = truth {
        close(vol(&b), v, what);
        close(surface_area(&b), a, what);
    }
    outward(&b, &mut Budget::unlimited(), &Stop::new()).unwrap();
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    contract(&b, &cx, Contacts::Refused).unwrap_or_else(|e| panic!("{what}: {e:?}"));
    b
}

#[test]
fn exploded_recipes_are_the_same_solid() {
    // Seed 0 of each: the unit block; radius 1, height 2 (cylinder); radius 1, height 1
    // (cone); radius 1 (sphere); major 2, minor 1 (torus); a 3 x 3 x 3 block with a unit
    // void.
    let cases: [(Kind, usize, Truth); 12] = [
        (Kind::Block, 1, Some((1.0, 6.0))),
        (Kind::Block, 3, Some((1.0, 6.0))),
        (Kind::Prism, 2, None),
        (Kind::Cylinder, 3, Some((2.0 * PI, 6.0 * PI))),
        (Kind::Cylinder, 7, Some((2.0 * PI, 6.0 * PI))),
        (Kind::Cone, 4, Some((PI / 3.0, PI * (1.0 + 2f64.sqrt())))),
        (Kind::Sphere, 4, Some((4.0 / 3.0 * PI, 4.0 * PI))),
        (Kind::Sphere, 6, Some((4.0 / 3.0 * PI, 4.0 * PI))),
        (Kind::Torus, 3, Some((4.0 * PI * PI, 8.0 * PI * PI))),
        (Kind::Torus, 4, Some((4.0 * PI * PI, 8.0 * PI * PI))),
        (Kind::Holed, 2, None),
        (Kind::Cavity, 1, Some((26.0, 60.0))),
    ];
    for (kind, n, truth) in cases {
        let body = solid(kind, 0, n);
        same_solid(&body, truth, &format!("{kind:?} {n}"));
    }
}

#[test]
fn spheres_of_many_lunes_are_the_same_solid() {
    // Every meridian joins the two poles: 16 to 64 pieces with the same ends, searched by
    // midpoint rather than all pairs; the result is the sphere.
    for n in [8, 16, 32] {
        let body = solid(Kind::Sphere, 0, n);
        same_solid(
            &body,
            Some((4.0 / 3.0 * PI, 4.0 * PI)),
            &format!("sphere {n}"),
        );
    }
}

#[test]
fn exploded_random_recipes_are_the_same_solid() {
    for kind in [
        Kind::Block,
        Kind::Prism,
        Kind::Cylinder,
        Kind::Cone,
        Kind::Sphere,
    ] {
        for seed in 1..4 {
            let body = solid(kind, seed, 3);
            same_solid(&body, None, &format!("{kind:?} seed {seed}"));
        }
    }
}

fn boolean_of(a: &Brep, b: &Brep, op: BooleanOp) -> Brep {
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    boolean(a, b, op, &mut cx).unwrap().body
}

#[test]
fn exploded_boolean_results_are_the_same_solid() {
    // Unit cubes: [0,1]^3 and [1/2,3/2]^3 overlap in an eighth.
    let (a, b) = (cube(0.0, 1.0), cube(0.5, 1.0));
    let union = boolean_of(&a, &b, BooleanOp::Union);
    let cut = boolean_of(&a, &b, BooleanOp::Subtract);
    let both = boolean_of(&a, &b, BooleanOp::Intersect);
    // The L-shaped difference has the cube's area: the notch swaps three faces for three.
    same_solid(&union, Some((1.875, 6.0 + 6.0 - 1.5)), "union");
    same_solid(&cut, Some((0.875, 6.0)), "difference");
    same_solid(&both, Some((0.125, 1.5)), "intersection");
    // A cube less a smaller cube cutting through a corner, then a cylinder of faces.
    let small = cube(0.75, 0.5);
    let notched = boolean_of(&a, &small, BooleanOp::Subtract);
    same_solid(&notched, Some((1.0 - 0.015_625, 6.0)), "notched");
}

/// The sheets of `body` exploded, the sheet of face `moved` translated by `by`, then
/// stitched for `want`.
fn perturbed(
    body: &Brep,
    moved: usize,
    by: Vec3,
    want: Want,
) -> Result<scx_stitch::Stitched, StitchError> {
    let doc = Document::default();
    let mut sheets = explode(body, |_| false);
    let mut d = sheets[moved].edit();
    d.transform(&Affine::translation(by), &doc).unwrap();
    sheets[moved] = finish(d);
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    stitch(&refs(&sheets), want, &mut cx)
}

/// The unit normal of the plane of the `i`th face, from its first loop's surface.
fn normal(body: &Brep, i: usize) -> Vec3 {
    let f = body.faces().nth(i).unwrap();
    match &f.record().surface {
        scx_surface::Surface::Plane(p) => Vec3::from(p.frame().z()),
        s => panic!("{s:?}"),
    }
}

/// The faces of `body` that share an edge with no other, and the vertices of face `i`.
fn shared(body: &Brep, i: usize) -> (usize, usize) {
    let face = body.faces().nth(i).unwrap();
    let id = face.id();
    let mut edges = BTreeSet::new();
    let mut verts = BTreeSet::new();
    for u in face.uses() {
        let e = u.edge();
        if e.uses().any(|o| o.face().id() != id) {
            edges.insert(e.id());
        }
        verts.insert(u.start().id());
    }
    (edges.len(), verts.len())
}

#[test]
fn a_sheet_just_inside_the_coincidence_stitches_and_just_outside_does_not() {
    let cases = [
        (Kind::Block, 1usize),
        (Kind::Block, 2),
        (Kind::Prism, 2),
        (Kind::Cylinder, 4),
    ];
    for (kind, n) in cases {
        let body = solid(kind, 0, n);
        for (i, face) in body.faces().enumerate() {
            if !matches!(face.record().surface, scx_surface::Surface::Plane(_)) {
                continue;
            }
            let dir = normal(&body, i);
            let what = format!("{kind:?} {n} face {i}");
            // Inside: the same solid, to within the coincidence.
            for sign in [1.0, -1.0] {
                let by = dir * (sign * INSIDE);
                let out = perturbed(&body, i, by, Want::Solid)
                    .unwrap_or_else(|e| panic!("{what} inside: {e:?}"));
                let b = &out.done.body;
                assert_eq!(counts(b), counts(&body), "{what} inside");
                assert!(out.report.left.is_empty(), "{what}: {:?}", out.report.left);
                let v = vol(b);
                assert!(
                    (v - vol(&body)).abs() <= 4e-5 * vol(&body).max(1.0),
                    "{what}: volume {v}"
                );
                outward(b, &mut Budget::unlimited(), &Stop::new()).unwrap();
            }
            // Outside: nothing of the moved sheet joins; as sheets it is the body less
            // the moved face's joins, and as a solid it is refused at an open edge.
            let by = dir * (1.1 * ZONE);
            let (joins, verts) = shared(&body, i);
            let out = perturbed(&body, i, by, Want::Sheets).unwrap();
            let want = counts(&body);
            let b = &out.done.body;
            assert_eq!(b.faces().count(), want[1], "{what} outside");
            assert_eq!(b.edges().count(), want[2] + joins, "{what} outside edges");
            assert_eq!(
                b.vertices().count(),
                want[3] + verts,
                "{what} outside verts"
            );
            assert!(out.report.left.is_empty(), "{what}: {:?}", out.report.left);
            let err = perturbed(&body, i, by, Want::Solid).unwrap_err();
            let StitchError::Refused(StitchRefusal::NotSolid {
                at: Entity::Edge(_),
                why: SolidFault::Open,
                ..
            }) = err
            else {
                panic!("{what} outside: {err:?}");
            };
        }
    }
}

/// The just-inside stitch moves nothing the report does not name: each identified vertex
/// of a sheet moved by `INSIDE` has that spread, certified.
#[test]
fn the_identified_spread_is_the_perturbation() {
    let body = solid(Kind::Block, 0, 1);
    let top = top_face(&body);
    let out = perturbed(&body, top, Vec3::new(0.0, 0.0, INSIDE), Want::Solid).unwrap();
    // Every vertex of the block stands for three sheets; the four of the moved face are
    // the ones that moved, by the perturbation, certified (spread is an upper bound).
    assert_eq!(out.report.identified.len(), 8);
    let moved = out
        .report
        .identified
        .iter()
        .filter(|i| i.spread.mm() > 0.0)
        .count();
    assert_eq!(moved, 4);
    for i in &out.report.identified {
        let s = i.spread.mm();
        assert!((0.0..=INSIDE * (1.0 + 1e-9)).contains(&s), "{s}");
    }
    assert_eq!(out.report.joined.len(), 12);
    let widest = out
        .report
        .joined
        .iter()
        .map(|j| j.gap.mm())
        .fold(0.0, f64::max);
    assert!((INSIDE..=ZONE).contains(&widest), "{widest}");
    assert!(
        !out.report
            .left
            .iter()
            .any(|l| matches!(l, Left::Undecided { .. }))
    );
}

/// A closed-form volume and area.
type Truth = Option<(f64, f64)>;

type Key = Vec<String>;

/// The body's geometry as sorted text, which holds every float to its bits (`Debug` prints
/// the shortest text that reads back as the same float) and no id.
fn geometry(b: &Brep) -> (Key, Key, Key) {
    let pt = |v: scx_brep::VertexId| format!("{:?}", b.vertex(v).unwrap().record());
    let mut vs: Key = b.vertices().map(|v| format!("{:?}", v.record())).collect();
    let edge = |e: scx_brep::EdgeId| {
        let g = b.edge(e).unwrap().geometry().clone();
        format!(
            "{:?} {:?} {}|{}",
            g.shape,
            g.accuracy,
            pt(g.ends[0]),
            pt(g.ends[1])
        )
    };
    let mut es: Key = b.edges().map(|e| edge(e.id())).collect();
    let mut fs: Key = b
        .faces()
        .map(|f| {
            let mut uses: Vec<String> = f
                .uses()
                .map(|u| format!("{} {:?}", edge(u.edge().id()), u.sense()))
                .collect();
            uses.sort();
            let side = |s| f.shell(s).region().is_solid();
            format!(
                "{:?} {} {}{} {uses:?}",
                f.record().surface,
                f.record().loops.len(),
                u8::from(side(FaceSide::Back)),
                u8::from(side(FaceSide::Front)),
            )
        })
        .collect();
    vs.sort();
    es.sort();
    fs.sort();
    (vs, es, fs)
}

/// A deterministic shuffle of `0..n`.
fn shuffled(n: usize, mut seed: u64) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let j = usize::try_from(seed >> 33).unwrap() % (i + 1);
        v.swap(i, j);
    }
    v
}

#[test]
fn permuted_sheets_give_the_same_bits_for_every_recipe() {
    let (a, b) = (cube(0.0, 1.0), cube(0.5, 1.0));
    let bodies = [
        solid(Kind::Block, 2, 2),
        solid(Kind::Cylinder, 0, 5),
        solid(Kind::Cone, 1, 4),
        solid(Kind::Sphere, 0, 5),
        solid(Kind::Torus, 0, 3),
        solid(Kind::Holed, 0, 2),
        solid(Kind::Cavity, 0, 1),
        boolean_of(&a, &b, BooleanOp::Union),
    ];
    for (k, body) in bodies.iter().enumerate() {
        let sheets = explode(body, |_| false);
        let first = run(&refs(&sheets)).unwrap();
        let want = geometry(&first.done.body);
        let want_gaps = first.report.joined.len();
        let n = sheets.len();
        let mut orders = vec![(0..n).rev().collect::<Vec<_>>()];
        orders.extend((1..=5).map(|s| shuffled(n, s)));
        for order in orders {
            let ordered: Vec<&Brep> = order.iter().map(|&i| &sheets[i]).collect();
            let out = run(&ordered).unwrap();
            assert_eq!(geometry(&out.done.body), want, "body {k}, order {order:?}");
            assert_eq!(out.report.joined.len(), want_gaps);
            assert_eq!(out.report.identified.len(), first.report.identified.len());
        }
    }
}

/// With every sheet within coincidence of its neighbours (just-inside perturbation of every
/// other face), the order still does not change the geometry.
#[test]
fn permuted_perturbed_sheets_give_the_same_bits() {
    let body = solid(Kind::Block, 0, 2);
    let doc = Document::default();
    let mut sheets = explode(&body, |_| false);
    for (i, s) in sheets.iter_mut().enumerate() {
        if i % 2 == 0 {
            let by = normal(&body, i) * (0.4 * ZONE);
            let mut d = s.edit();
            d.transform(&Affine::translation(by), &doc).unwrap();
            *s = finish(d);
        }
    }
    let first = run(&refs(&sheets)).unwrap();
    let want = geometry(&first.done.body);
    assert_eq!(first.done.body.faces().count(), 24);
    for seed in 1..6 {
        let ordered: Vec<&Brep> = shuffled(sheets.len(), seed)
            .into_iter()
            .map(|i| &sheets[i])
            .collect();
        let out = run(&ordered).unwrap();
        assert_eq!(geometry(&out.done.body), want, "seed {seed}");
    }
}

/// The face of `body` whose plane faces up.
fn top_face(body: &Brep) -> usize {
    (0..body.faces().count())
        .filter(|&i| {
            matches!(
                body.faces().nth(i).unwrap().record().surface,
                scx_surface::Surface::Plane(_)
            )
        })
        .max_by(|&a, &b| {
            let h = |i: usize| match &body.faces().nth(i).unwrap().record().surface {
                scx_surface::Surface::Plane(p) => {
                    let n = Vec3::from(p.frame().z());
                    n.z * 1e6 + p.frame().origin().z
                }
                _ => f64::MIN,
            };
            h(a).total_cmp(&h(b))
        })
        .unwrap()
}

/// The pin's `a_cap_on_a_cube_s_top_edges_closes_into_one_box`, as sheets: the five other
/// faces of a 10 mm cube stitched into an open box, the sixth laid on its rim as a cap, the
/// stitch of the two is the closed cube, volume 1000, 6 faces, 12 edges, 8 vertices. (The
/// pin's cube is a solid operand and its cap a sheet, `SolidOperand` here.)
#[test]
fn a_cap_on_an_open_box_closes_it() {
    let block = cube(0.0, 10.0);
    let sheets = explode(&block, |_| false);
    let top = top_face(&block);
    let rest: Vec<&Brep> = sheets
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != top)
        .map(|(_, s)| s)
        .collect();
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let open = stitch(&rest, Want::Sheets, &mut cx).unwrap().done.body;
    assert_eq!(open.faces().count(), 5);
    assert_eq!(open.edges().count(), 12);
    assert_eq!(open.shells().count(), 1);
    // Solid is refused until the cap is on.
    assert!(matches!(
        run(&[&open]),
        Err(StitchError::Refused(StitchRefusal::NotSolid {
            why: SolidFault::Open,
            ..
        }))
    ));
    let closed = run(&[&open, &sheets[top]]).unwrap().done.body;
    close(vol(&closed), 1000.0, "cap");
    close(surface_area(&closed), 600.0, "cap");
    assert_eq!(counts(&closed), counts(&block));
    outward(&closed, &mut Budget::unlimited(), &Stop::new()).unwrap();
}

/// Every use with a certified bound: its claim and bound, which the bound's accuracy is.
/// A measured bound is the input's own (a lune's use at a pole) and is skipped.
fn use_bounds(b: &Brep) -> Vec<f64> {
    let mut out = Vec::new();
    for f in b.faces() {
        for u in f.uses() {
            let Some(r) = u.bound() else { continue };
            let scx_num::cert::Basis::Certified(c) = r.basis else {
                continue;
            };
            assert_eq!(c.claim, scx_num::cert::Claim::PcurveOnSurface);
            assert_eq!(c.bound, r.acc.length());
            out.push(r.acc.length().mm());
        }
    }
    out
}

/// A join proved within coincidence but past the exact pcurve writer's tolerance (the kept
/// curve stands between `approx` and the zone from the absorbed face) stitches: the
/// absorbed face keeps its plane and its use of the kept edge gets the curve's projection,
/// its bound the certified gap, which is the perturbation to the search's width (a 64th of
/// the zone) and within the zone, so no edge is raised. Before ST4a it was
/// `Unsupported(Edges { why: Fit })`.
#[test]
fn a_join_past_the_pcurve_tolerance_records_its_certified_gap() {
    let body = solid(Kind::Block, 0, 1);
    let top = top_face(&body);
    let out = perturbed(&body, top, Vec3::new(0.0, 0.0, 0.9 * ZONE), Want::Solid).unwrap();
    let b = &out.done.body;
    assert_eq!(counts(b), counts(&body));
    let bounds = use_bounds(b);
    // The top's four edges, each kept from one sheet and projected onto the other.
    assert_eq!(bounds.len(), 4, "{bounds:?}");
    for g in bounds {
        assert!((0.9 * ZONE..=0.9 * ZONE + ZONE / 64.0).contains(&g), "{g}");
    }
    assert!(b.edges().all(|e| e.accuracy().is_none()));
    assert!(
        out.done
            .report
            .raised
            .iter()
            .all(|r| !matches!(r.entity, Entity::Edge(_)))
    );
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    contract(b, &cx, Contacts::Refused).unwrap();
}

/// The truth the stitch owes (74 §4.4): a sheet 0.9 of the coincidence off stitches, for
/// each face of the block, out and in, to the same solid.
#[test]
fn a_sheet_at_nine_tenths_of_the_coincidence_stitches() {
    let body = solid(Kind::Block, 0, 1);
    for i in 0..6 {
        for sign in [1.0, -1.0] {
            let by = normal(&body, i) * (sign * 0.9 * ZONE);
            let out = perturbed(&body, i, by, Want::Solid).unwrap();
            let b = &out.done.body;
            assert_eq!(counts(b), counts(&body), "face {i} by {sign}");
            close_within(vol(b), 1.0, 0.9 * ZONE, "volume");
            close_within(surface_area(b), 6.0, 4.0 * 0.9 * ZONE, "area");
            let doc = Document::default();
            let mut scratch = Scratch::new();
            let settings = Settings::new();
            let stop = Stop::new();
            let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
            contract(b, &cx, Contacts::Refused).unwrap_or_else(|e| panic!("face {i}: {e:?}"));
        }
    }
}

/// `got` against `want` to within `slack` and a part in 1e-9.
fn close_within(got: f64, want: f64, slack: f64, what: &str) {
    assert!(
        (got - want).abs() <= slack + 1e-9 * want.abs().max(1.0),
        "{what}: {got}, want {want} within {slack}"
    );
}

/// The stitch of `sheets` and the check's contract on it.
fn checked(sheets: &[Brep], what: &str) -> scx_stitch::Stitched {
    let out = run(&refs(sheets)).unwrap_or_else(|e| panic!("{what}: {e:?}"));
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    contract(&out.done.body, &cx, Contacts::Refused).unwrap_or_else(|e| panic!("{what}: {e:?}"));
    outward(&out.done.body, &mut Budget::unlimited(), &Stop::new()).unwrap();
    out
}

/// Curved faces moved 0.9 of the coincidence stitch to the same solid: a quarter of a
/// cylinder's wall moved off the axis (its arcs leave the caps' circles, so a cap's use of
/// the wall's arc, or the wall's of the cap's circle, is the projection through the chart,
/// and its rulings leave the next quarters' cylinder), and a quarter of a sphere grown
/// about the centre (its meridians a great circle of the larger sphere, projected onto the
/// next quarters' by the meridian's closed form). Each records certified gaps within the
/// zone, and the volume and area are the closed form's to within the move.
#[test]
fn curved_sheets_at_nine_tenths_of_the_coincidence_stitch() {
    let d = 0.9 * ZONE;
    let doc = Document::default();
    // Radius 1, height 2, four wall faces and two caps.
    let cyl = solid(Kind::Cylinder, 0, 4);
    let wall = (0..cyl.faces().count())
        .find(|&i| {
            matches!(
                cyl.faces().nth(i).unwrap().record().surface,
                scx_surface::Surface::Cylinder(_)
            )
        })
        .unwrap();
    let mid = {
        let f = cyl.faces().nth(wall).unwrap();
        let mut c = Vec3::ZERO;
        let mut n = 0.0;
        for u in f.uses() {
            let p = u.start().point();
            c += Vec3::new(p.x, p.y, 0.0);
            n += 1.0;
        }
        c * (1.0 / n)
    };
    let by = mid * (d / mid.length());
    let mut sheets = explode(&cyl, |_| false);
    let mut e = sheets[wall].edit();
    e.transform(&Affine::translation(by), &doc).unwrap();
    sheets[wall] = finish(e);
    let out = checked(&sheets, "cylinder");
    let b = &out.done.body;
    assert_eq!(counts(b), counts(&cyl), "cylinder");
    close_within(vol(b), 2.0 * PI, 2.0 * 2.0 * d, "cylinder volume");
    close_within(surface_area(b), 6.0 * PI, 4.0 * 2.0 * d, "cylinder area");
    let bounds = use_bounds(b);
    assert!(!bounds.is_empty());
    assert!(bounds.iter().all(|&g| g <= ZONE), "{bounds:?}");
    // Radius 1, four wall faces (lunes), each meeting the others at the poles.
    let sphere = solid(Kind::Sphere, 0, 4);
    let mut sheets = explode(&sphere, |_| false);
    let grown =
        scx_num::math::Affine::uniform_scaling(scx_num::vector3d::Point3::ORIGIN, 1.0 + d).unwrap();
    let mut e = sheets[0].edit();
    e.transform(&grown, &doc).unwrap();
    sheets[0] = finish(e);
    let out = checked(&sheets, "sphere");
    let b = &out.done.body;
    assert_eq!(counts(b), counts(&sphere), "sphere");
    close_within(vol(b), 4.0 / 3.0 * PI, 4.0 * PI * d, "sphere volume");
    close_within(surface_area(b), 4.0 * PI, 8.0 * PI * d, "sphere area");
    let bounds = use_bounds(b);
    assert!(!bounds.is_empty());
    assert!(bounds.iter().all(|&g| g <= ZONE), "{bounds:?}");
}
