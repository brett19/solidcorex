// Written for solidcorex. Apache-2.0.
//! The outline of a body on its edges against closed forms and against a dense scan of
//! its own (research 74 §43, HL3): which edges and which stretches of them lie between a
//! face that faces the viewer and one that does not. Never against the pin's output.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::similar_names,
    clippy::many_single_char_names
)]

use std::f64::consts::{PI, TAU};

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use std::f64::consts::FRAC_1_SQRT_2;
use std::sync::Arc;

use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, Face, FaceRecord, FaceSide,
    LoopRecord, RegionId, ShellId, ShellRecord, UseGeom, VertexId, VertexRecord,
};
use scx_curve::Curve;
use scx_curve::{Curve2, Line2};
use scx_hlr::{
    HlrError, Outline, OutlineUndecided, Stretch, View, drop_coincident, outline, prepare,
};
use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::frame::Frame;
use scx_num::frame::Sense;
use scx_num::units::Length;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_nurbs::{KnotVector, NurbsCurve};
use scx_nurbs_surface::{Net, NurbsSurface};
use scx_planar::Curve2 as Image2;
use scx_surface::Surface;
use scx_testkit::bodies::made::{block, cone, cylinder, sphere, torus};
use scx_testkit::bodies::{Kind, generate};

// ---- helpers -------------------------------------------------------------------------------

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn up_for(d: Vec3) -> Vec3 {
    if d.cross(v(0.0, 0.0, 1.0)).length() > 0.1 * d.length() {
        v(0.0, 0.0, 1.0)
    } else {
        v(1.0, 0.0, 0.0)
    }
}

fn view(d: Vec3) -> View {
    View::new(d, up_for(d)).unwrap()
}

fn directions() -> Vec<Vec3> {
    vec![
        v(1.0, 0.0, 0.0),
        v(0.0, 1.0, 0.0),
        v(0.0, 0.0, 1.0),
        v(1.0, 1.0, 1.0),
        v(1.0, -2.0, 0.5),
        v(-0.3, 0.7, -2.0),
        v(0.05, -0.02, 1.0),
        v(-1.0, 0.4, 0.1),
    ]
}

fn run_on(body: &Brep, view: &View, settings: &Settings, stop: &Stop) -> Result<Outline, HlrError> {
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let mut cx = Cx::new(&doc, settings, &mut scratch, stop, &NoTrace);
    outline(body, view, &mut cx)
}

fn run(body: &Brep, view: &View) -> Outline {
    run_on(body, view, &Settings::new(), &Stop::new()).unwrap()
}

fn finish(d: Draft) -> Brep {
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

fn generated(kind: Kind, seed: u64, size: usize) -> Brep {
    finish(generate(kind, seed, size).unwrap().draft)
}

fn curve_of(body: &Brep, e: EdgeId) -> (&Curve, f64, f64) {
    let EdgeShape::Curve { curve, interval } = body.edge(e).unwrap().shape() else {
        panic!("an edge with a curve")
    };
    (curve, interval.min(), interval.max())
}

/// The total parameter length of the stretches on edges satisfying `pick`.
fn length_on(body: &Brep, o: &Outline, pick: impl Fn(&Curve) -> bool) -> f64 {
    o.stretches
        .iter()
        .filter(|s| pick(curve_of(body, s.edge).0))
        .map(|s| s.range.length())
        .sum()
}

fn circle_radius(c: &Curve) -> Option<f64> {
    match c {
        Curve::Circle(c) => Some(c.radius().mm()),
        _ => None,
    }
}

// ---- the dense scan: an independent oracle -----------------------------------------------------

/// The face's normal out of the material, dotted with `v`, at the body point `q` of the face;
/// `-inf` for a face between two solids (never seen), the absolute value for a sheet.
fn lean(face: Face<'_>, q: Point3, v: Vec3) -> f64 {
    let uv = face.uv_box();
    let inv = face
        .surface()
        .invert(q, None, uv.as_ref(), &mut Budget::unlimited(), &Stop::new())
        .unwrap();
    let n = *face
        .surface()
        .normal(inv.foot().at, [Approach::Above; 2])
        .unwrap();
    let s = n.dot(v);
    let solid = |side| face.shell(side).region().is_solid();
    match (solid(FaceSide::Front), solid(FaceSide::Back)) {
        (true, false) => -s,
        (false, true) => s,
        (false, false) => s.abs(),
        (true, true) => f64::NEG_INFINITY,
    }
}

/// Every edge sampled at 2048 even parameters, away from the reported stretches' ends: the
/// stretches hold exactly the samples between a facing and a non-facing face. Returns how
/// many samples were compared.
fn agrees_with_scan(body: &Brep, o: &Outline, d: Vec3) -> usize {
    let toward = -d / d.length();
    let mut compared = 0;
    for edge in body.edges() {
        let faces: Vec<_> = edge.fins().map(|f| f.face).collect();
        let mut set: Vec<_> = faces.clone();
        set.sort_unstable();
        set.dedup();
        if set.is_empty() || (set.len() == 1 && faces.len() > 1) || faces.len() > 2 {
            continue;
        }
        if o.undecided.iter().any(|u| u.edge == edge.id()) {
            continue;
        }
        let EdgeShape::Curve { curve, interval } = edge.shape() else {
            continue;
        };
        let mine: Vec<&Stretch> = o.stretches.iter().filter(|s| s.edge == edge.id()).collect();
        let (lo, hi) = (interval.min(), interval.max());
        let n = 2048;
        for k in 0..n {
            let t = lo + (hi - lo) * (f64::from(k) + 0.5) / f64::from(n);
            let q = curve.point(t);
            let s: Vec<f64> = set
                .iter()
                .map(|f| lean(body.face(*f).unwrap(), q, toward))
                .collect();
            // Edge-on samples are the lean rule's, not the scan's.
            if s.iter().any(|x| x.is_finite() && x.abs() < 1e-5) {
                continue;
            }
            let facing: Vec<bool> = s.iter().map(|x| *x > 0.0).collect();
            let on = match facing.as_slice() {
                [one] => *one,
                [a, b] => a != b,
                _ => unreachable!(),
            };
            // Not within a sample step of a stretch's end: the scan is only a sample.
            let step = (hi - lo) / f64::from(n);
            if mine.iter().any(|m| {
                (t - m.range.min()).abs() < 2.0 * step || (t - m.range.max()).abs() < 2.0 * step
            }) {
                continue;
            }
            let inside = mine.iter().any(|m| m.range.min() < t && t < m.range.max());
            assert_eq!(inside, on, "edge {:?} at t = {t}, d = {d:?}", edge.id());
            compared += 1;
        }
    }
    compared
}

// ---- boxes ---------------------------------------------------------------------------------

fn cube() -> Brep {
    block(
        &Frame::WORLD,
        [mm(1.0), mm(1.0), mm(1.0)],
        &Document::default(),
    )
    .unwrap()
    .body
}

#[test]
fn an_iso_cube_has_the_six_outline_edges_whole_and_none_of_the_creases() {
    let b = cube();
    let o = run(&b, &view(v(1.0, 1.0, 1.0)));
    assert!(o.undecided.is_empty());
    assert_eq!(o.stretches.len(), 6, "{o:?}");
    for s in &o.stretches {
        assert!(s.whole);
        // The six edges not at the corner (0,0,0) nor (1,1,1): their ends sum to 1 or 2
        // coordinates of one.
        let sum = |q: Point3| q.x + q.y + q.z;
        let (a, b) = (sum(s.ends[0]), sum(s.ends[1]));
        assert!(
            (a.min(b) - 1.0).abs() < 1e-12 && (a.max(b) - 2.0).abs() < 1e-12,
            "{s:?}"
        );
    }
}

#[test]
fn a_cube_along_an_axis_has_the_four_edges_of_its_near_face() {
    let b = cube();
    // Looking along -z: the viewer is above, the top face (z = 1) faces it.
    let o = run(&b, &view(v(0.0, 0.0, -1.0)));
    assert_eq!(o.stretches.len(), 4, "{o:?}");
    for s in &o.stretches {
        assert!(s.whole);
        assert!(
            s.ends.iter().all(|q| (q.z - 1.0).abs() < 1e-12),
            "the top face's edges: {s:?}"
        );
    }
    let o = run(&b, &view(v(0.0, 0.0, 1.0)));
    assert!(
        o.stretches
            .iter()
            .all(|s| s.ends.iter().all(|q| q.z == 0.0))
    );
    assert_eq!(o.stretches.len(), 4);
}

// ---- cylinder, cone, sphere, torus: the closed forms ---------------------------------------

#[test]
fn a_cylinder_seen_across_its_axis_has_the_near_half_of_each_rim() {
    let c = cylinder(&Frame::WORLD, mm(2.0), mm(5.0), &Document::default())
        .unwrap()
        .body;
    let o = run(&c, &view(v(1.0, 0.0, 0.0)));
    // The caps are edge-on (away); the wall faces the viewer on the half of each rim
    // nearer to it: pi of each rim's turn, the two rims.
    let rims = length_on(&c, &o, |c| circle_radius(c) == Some(2.0));
    assert!((rims - 2.0 * PI).abs() < 1e-9, "{rims}");
    assert!(o.undecided.is_empty());
    // Their ends lie on the silhouette generators, `y = +-2` (the view along x).
    for s in &o.stretches {
        for q in s.ends {
            assert!(q.x.abs() < 1e-9 && (q.y.abs() - 2.0).abs() < 1e-9, "{s:?}");
        }
    }
    // The seam ruling is no edge between two faces.
    assert!(
        o.stretches
            .iter()
            .all(|s| matches!(curve_of(&c, s.edge).0, Curve::Circle(_)))
    );
}

#[test]
fn a_cylinder_seen_along_its_axis_has_the_whole_rim_of_its_near_cap() {
    let c = cylinder(&Frame::WORLD, mm(2.0), mm(5.0), &Document::default())
        .unwrap()
        .body;
    // The viewer above: the top cap faces it; the wall is edge-on all along each rim
    // (the lean rule), so the top rim is the outline and the bottom one is not.
    let o = run(&c, &view(v(0.0, 0.0, -1.0)));
    assert_eq!(o.stretches.len(), 1, "{o:?}");
    let s = o.stretches[0];
    assert!(s.whole && s.ends.iter().all(|q| (q.z - 5.0).abs() < 1e-12));
}

#[test]
fn a_cylinder_seen_aslant_has_the_far_half_of_the_top_rim_and_the_near_half_of_the_bottom() {
    let c = cylinder(&Frame::WORLD, mm(2.0), mm(5.0), &Document::default())
        .unwrap()
        .body;
    // The viewer above and to +x: the top cap faces it; the wall faces on the near half
    // (x > 0), so the top rim's outline is the far half, the bottom rim's the near half.
    let o = run(&c, &view(v(-1.0, 0.0, -1.0)));
    let top: Vec<_> = o.stretches.iter().filter(|s| s.ends[0].z > 1.0).collect();
    let bottom: Vec<_> = o.stretches.iter().filter(|s| s.ends[0].z < 1.0).collect();
    let length = |v: &[&Stretch]| v.iter().map(|s| s.range.length()).sum::<f64>();
    assert!((length(&top) - PI).abs() < 1e-9 && (length(&bottom) - PI).abs() < 1e-9);
    for s in &top {
        let mid = curve_of(&c, s.edge).0.point(s.range.mid());
        assert!(mid.x < 0.0, "the far half: {s:?}");
    }
    for s in &bottom {
        let mid = curve_of(&c, s.edge).0.point(s.range.mid());
        assert!(mid.x > 0.0, "the near half: {s:?}");
    }
}

#[test]
fn a_sphere_has_no_outline_edge_and_a_torus_seen_along_its_axis_has_both_equators() {
    let doc = Document::default();
    let s = sphere(&Frame::WORLD, mm(3.0), &doc).unwrap().body;
    for d in directions() {
        let o = run(&s, &view(d));
        assert!(o.stretches.is_empty() && o.undecided.is_empty(), "{d:?}");
    }
    // The made torus is one face with two seams: no edge lies between two faces.
    let t = torus(&Frame::WORLD, mm(5.0), mm(1.0), &doc).unwrap().body;
    for d in directions() {
        assert!(run(&t, &view(d)).stretches.is_empty(), "{d:?}");
    }
}

#[test]
fn a_generator_that_splits_the_wall_on_the_silhouette_is_outline_by_the_lean_rule() {
    // A cylinder wall in 5 faces: each generator between two of them, seen along the
    // tangent there, is the cylinder's silhouette. Both walls are edge-on at it; the one
    // whose interior leans toward the viewer faces, the other does not, so the generator is
    // outline, whole, from either side. The caps meet it only at its ends.
    let body = generated(Kind::Cylinder, 1, 5);
    // The generated body is in a tilted frame: its axis is a rim circle's.
    let (o, a) = body
        .edges()
        .find_map(|e| match e.shape() {
            EdgeShape::Curve {
                curve: Curve::Circle(c),
                ..
            } => Some((c.frame().origin(), *c.frame().z())),
            _ => None,
        })
        .unwrap();
    let mut checked = 0;
    for edge in body.edges() {
        let EdgeShape::Curve {
            curve: Curve::Line(l),
            ..
        } = edge.shape()
        else {
            continue;
        };
        let walls: std::collections::BTreeSet<_> = edge.fins().map(|f| f.face).collect();
        let along = l.direction() / l.direction().length();
        if walls.len() != 2 || along.cross(a).length() > 1e-9 {
            continue;
        }
        let r = l.origin() - o;
        let radial = r - a * r.dot(a);
        let tangent = a.cross(radial);
        for d in [tangent, -tangent] {
            let o = run(&body, &view(d));
            let s: Vec<_> = o.stretches.iter().filter(|s| s.edge == edge.id()).collect();
            assert_eq!(s.len(), 1, "edge {:?}, {d:?}: {o:?}", edge.id());
            assert!(s[0].whole);
        }
        checked += 1;
    }
    assert!(checked >= 4, "{checked}");
}

// ---- what sampling misses ------------------------------------------------------------------

#[test]
fn a_face_that_faces_for_a_sliver_of_its_rim_is_found_where_32_samples_miss_it() {
    // A frustum narrowing upward (radii 2 to 1 over 2: slope -1/2). Its wall's normal is
    // proportional to `rho + a / 2`; seen from `v = (sin b cos f, sin b sin f, cos b)` the
    // wall faces where `cos(theta - f) > c = -cot(b) / 2`. With `c = 1 - delta` it faces
    // only within `acos c` of the azimuth `f`: here `delta = 1e-3`, a 0.09-radian arc of
    // the top rim (1.4 % of its turn), centred half a step of the pin's 32 even samples
    // from the nearest, so none lands on it. The top cap faces away (`cos b < 0`), so the
    // rim's outline is exactly that arc.
    let delta = 1e-3;
    let b = f64::atan2(1.0, -2.0 * (1.0 - delta));
    let f = TAU / 64.0;
    let toward = v(b.sin() * f.cos(), b.sin() * f.sin(), b.cos());
    let cn = cone(
        &Frame::WORLD,
        [mm(2.0), mm(1.0)],
        mm(2.0),
        &Document::default(),
    )
    .unwrap()
    .body;
    let o = run(&cn, &view(-toward));
    let theta0 = (1.0 - delta).acos();
    let top: Vec<_> = o
        .stretches
        .iter()
        .filter(|s| circle_radius(curve_of(&cn, s.edge).0) == Some(1.0))
        .collect();
    assert_eq!(top.len(), 1, "{o:?}");
    let s = top[0];
    assert!(
        (s.range.length() - 2.0 * theta0).abs() < 1e-9,
        "{} against {}",
        s.range.length(),
        2.0 * theta0
    );
    for q in s.ends {
        let a = q.y.atan2(q.x);
        assert!(((a - f).abs() - theta0).abs() < 1e-9, "{a}");
    }
    // The bottom rim (radius 2): the bottom cap faces (`-cos b > 0`) and the wall only on
    // the sliver, so the outline is the rest of the turn.
    let bottom: f64 = o
        .stretches
        .iter()
        .filter(|s| circle_radius(curve_of(&cn, s.edge).0) == Some(2.0))
        .map(|s| s.range.length())
        .sum();
    assert!((bottom - (TAU - 2.0 * theta0)).abs() < 1e-9, "{bottom}");
}

// ---- the dense scan agrees ----------------------------------------------------------------

#[test]
fn every_edge_of_every_generated_body_agrees_with_a_dense_scan_in_8_views() {
    let mut compared = 0;
    let mut outlined = 0;
    let kinds = [
        (Kind::Block, 2),
        (Kind::Prism, 2),
        (Kind::QuadSheet, 2),
        (Kind::Cylinder, 3),
        (Kind::Cone, 3),
        (Kind::Sphere, 3),
        (Kind::Torus, 3),
        (Kind::Holed, 2),
        (Kind::Stack, 2),
    ];
    for (kind, size) in kinds {
        for seed in 1..=2 {
            let body = generated(kind, seed, size);
            for d in directions() {
                let o = run(&body, &view(d));
                compared += agrees_with_scan(&body, &o, d);
                outlined += o.stretches.len();
            }
        }
    }
    assert!(
        compared > 100_000 && outlined > 100,
        "{compared} {outlined}"
    );
}

#[test]
fn made_bodies_in_a_tilted_frame_agree_with_a_dense_scan() {
    let doc = Document::default();
    let tilt = Frame::new(p(3.0, -1.0, 2.0), v(0.3, -0.5, 0.8), v(1.0, 0.2, 0.1)).unwrap();
    let bodies = [
        cylinder(&tilt, mm(2.0), mm(3.0), &doc).unwrap().body,
        cone(&tilt, [mm(2.0), mm(0.5)], mm(3.0), &doc).unwrap().body,
        cone(&tilt, [mm(1.0), mm(0.0)], mm(3.0), &doc).unwrap().body,
        torus(&tilt, mm(4.0), mm(1.5), &doc).unwrap().body,
        torus(&tilt, mm(4.0), mm(3.9), &doc).unwrap().body,
    ];
    let mut compared = 0;
    for body in &bodies {
        for d in directions() {
            let o = run(body, &view(d));
            assert!(o.undecided.is_empty(), "{:?}", o.undecided);
            compared += agrees_with_scan(body, &o, d);
        }
    }
    assert!(compared > 50_000, "{compared}");
}

// ---- sheets, interiors, edges of many faces ----------------------------------------------------

#[test]
fn a_sheet_is_seen_from_either_side_and_its_border_is_the_outline() {
    let sheet = generated(Kind::QuadSheet, 1, 2);
    // Seen square on, from either side: every border edge (8 of the 2 x 2 quads) and no
    // edge inside; edge-on, nothing.
    for d in [v(0.0, 0.0, 1.0), v(0.0, 0.0, -1.0)] {
        let o = run(&sheet, &view(d));
        assert_eq!(o.stretches.len(), 8, "{d:?}: {o:?}");
        assert!(o.stretches.iter().all(|s| s.whole));
    }
    // Along one of its own edges the sheet is edge-on.
    let o = run(&sheet, &view(v(0.0, 0.0, 1.0)));
    let e = o.stretches[0].ends;
    let o = run(&sheet, &view(e[1] - e[0]));
    assert!(o.stretches.is_empty(), "{o:?}");
}

#[test]
fn an_edge_of_four_uses_is_named_not_guessed() {
    let body = generated(Kind::Checker, 1, 2);
    let o = run(&body, &view(v(1.0, 0.3, 0.2)));
    assert!(
        o.undecided
            .iter()
            .any(|u| u.why == OutlineUndecided::MoreThanTwoUses),
        "{o:?}"
    );
}

// ---- images and coincident pieces ----------------------------------------------------------

#[test]
fn a_cube_along_an_axis_has_four_exact_lines_and_two_bodies_in_one_place_draw_them_once() {
    let b = cube();
    let v0 = view(v(0.0, 0.0, -1.0));
    let o = run(&b, &v0);
    let images = o.images(&b, &v0, p(0.5, 0.5, 0.5)).unwrap();
    assert!(images.pending.is_empty());
    let lines: Vec<Image2> = images
        .images
        .iter()
        .flat_map(|i| i.curves.clone())
        .collect();
    assert_eq!(lines.len(), 4);
    // The unit square about the centre: every end at (+-0.5, +-0.5).
    for c in &lines {
        let Image2::Line { start, end } = c else {
            panic!("{c:?}")
        };
        for q in [start, end] {
            assert!((q.x.abs() - 0.5).abs() < 1e-12 && (q.y.abs() - 0.5).abs() < 1e-12);
        }
    }
    let twice: Vec<Image2> = lines.iter().chain(lines.iter()).cloned().collect();
    assert_eq!(drop_coincident(twice, 1e-9).len(), 4);
    // Lines on one line are drawn as the stretch they cover together.
    let overlap = vec![
        Image2::Line {
            start: scx_num::vector2d::Point2::new(0.0, 0.0),
            end: scx_num::vector2d::Point2::new(2.0, 0.0),
        },
        Image2::Line {
            start: scx_num::vector2d::Point2::new(3.0, 0.0),
            end: scx_num::vector2d::Point2::new(1.0, 0.0),
        },
    ];
    let merged = drop_coincident(overlap, 1e-9);
    assert_eq!(merged.len(), 1);
    let Image2::Line { start, end } = &merged[0] else {
        panic!()
    };
    assert!(start.x.abs() < 1e-12 && (end.x - 3.0).abs() < 1e-12);
}

#[test]
fn coaxial_cylinders_seen_along_their_axis_draw_one_circle() {
    let doc = Document::default();
    let a = cylinder(&Frame::WORLD, mm(2.0), mm(5.0), &doc)
        .unwrap()
        .body;
    let b = cylinder(&Frame::WORLD, mm(2.0), mm(3.0), &doc)
        .unwrap()
        .body;
    let v0 = view(v(0.0, 0.0, -1.0));
    let mut curves = Vec::new();
    for body in [&a, &b] {
        let o = run(body, &v0);
        let images = o.images(body, &v0, p(0.0, 0.0, 0.0)).unwrap();
        curves.extend(images.images.into_iter().flat_map(|i| i.curves));
    }
    // Each body's near rim is a whole circle: arcs of weight cos(pi/4) (quarter turns),
    // all on the circle of radius 2; the two bodies' circles are one.
    assert!(curves.len() >= 8, "{}", curves.len());
    for c in &curves {
        let Image2::Conic { start, weight, .. } = c else {
            panic!("{c:?}")
        };
        assert!((start.x.hypot(start.y) - 2.0).abs() < 1e-12);
        assert!((weight - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
    }
    let once = drop_coincident(curves.clone(), 1e-9);
    assert_eq!(once.len(), curves.len() / 2, "{once:?}");
}

// ---- scenes, far from the origin, determinism, stop and budget ----------------------------------

#[test]
fn a_placed_body_has_the_outline_of_the_body_placed_and_far_from_the_origin_the_same_ranges() {
    let doc = Document::default();
    let c = cylinder(&Frame::WORLD, mm(2.0), mm(5.0), &doc)
        .unwrap()
        .body;
    let d = v(1.0, 0.4, -0.3);
    let home = run(&c, &view(d));
    let place = Frame::new(p(1.0e6, -2.0e6, 3.0e6), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)).unwrap();
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let scene = prepare(&[(&c, place)], &mut cx).unwrap();
    let placed = scene.outlines(&view(d), &mut cx).unwrap();
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0].stretches.len(), home.stretches.len());
    for (a, b) in placed[0].stretches.iter().zip(&home.stretches) {
        assert_eq!(a.edge, b.edge);
        assert!((a.range.min() - b.range.min()).abs() < 1e-12);
        assert!((a.range.max() - b.range.max()).abs() < 1e-12);
        for (qa, qb) in a.ends.iter().zip(&b.ends) {
            let moved = *qb - p(0.0, 0.0, 0.0);
            let want = p(
                place.origin().x + moved.x,
                place.origin().y + moved.y,
                place.origin().z + moved.z,
            );
            assert!(qa.distance(want) < 1e-6, "{qa:?} {want:?}");
        }
    }
}

#[test]
fn the_outline_is_the_same_twice_and_a_stop_or_a_small_budget_say_so() {
    let doc = Document::default();
    let t = torus(&Frame::WORLD, mm(5.0), mm(1.0), &doc).unwrap().body;
    let w = view(v(1.0, 2.0, -0.5));
    assert_eq!(format!("{:?}", run(&t, &w)), format!("{:?}", run(&t, &w)));
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        run_on(&t, &w, &Settings::new(), &stop).unwrap_err(),
        HlrError::Stopped
    );
    let mut seen_exhausted = false;
    for limit in 0..4 {
        match run_on(&t, &w, &Settings::new().with_budget(limit), &Stop::new()) {
            Ok(_) => {}
            Err(e) => {
                assert_eq!(e.kind(), ErrorKind::Exhausted, "{limit}");
                seen_exhausted = true;
            }
        }
    }
    assert!(seen_exhausted);
}

// ---- NURBS faces ---------------------------------------------------------------------------

/// A sheet being written: the outside shell on both sides of every face.
struct B {
    d: Draft,
    outside: ShellId,
}

impl B {
    fn new() -> Self {
        let mut d = Draft::new();
        let outside = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
        Self { d, outside }
    }

    fn vertex(&mut self, q: [f64; 3]) -> VertexId {
        self.d
            .add_vertex(VertexRecord::new(p(q[0], q[1], q[2])))
            .unwrap()
    }

    fn edge(&mut self, curve: Curve, lo: f64, hi: f64, ends: [VertexId; 2]) -> EdgeId {
        let interval = Extent1d::new(lo, hi).unwrap();
        self.d
            .add_edge(EdgeGeom::new(EdgeShape::Curve { curve, interval }, ends))
            .unwrap()
    }

    fn line(&mut self, a: VertexId, b: VertexId) -> EdgeId {
        let q0 = self.d.vertex(a).unwrap().point();
        let q1 = self.d.vertex(b).unwrap().point();
        self.edge(
            Curve::Line(Line::new(q0, q1 - q0).unwrap()),
            0.0,
            1.0,
            [a, b],
        )
    }

    fn face(&mut self, surface: Surface, uses: Vec<(EdgeId, Sense, Curve2)>) {
        let uses = uses
            .into_iter()
            .map(|(e, s, c)| {
                EdgeUse::new(
                    e,
                    s,
                    UseGeom::Pcurve {
                        curve: c,
                        interval: Extent1d::new(0.0, 1.0).unwrap(),
                    },
                )
            })
            .collect();
        self.d
            .add_face(FaceRecord::new(
                surface,
                [self.outside, self.outside],
                vec![LoopRecord::new(uses)],
            ))
            .unwrap();
    }
}

fn line2(origin: [f64; 2], dir: [f64; 2]) -> Curve2 {
    Curve2::Line(Line2::new(Point2::new(origin[0], origin[1]), Vec2::new(dir[0], dir[1])).unwrap())
}

/// Three quarter spans of a rational quadratic, double knots at 1/3 and 2/3.
fn arc_knots() -> Vec<f64> {
    let (a, b) = (1.0 / 3.0, 2.0 / 3.0);
    vec![0.0, 0.0, 0.0, a, a, b, b, 1.0, 1.0, 1.0]
}

/// The arc's control points `(x, y)` and weights, angle 0 to 270 degrees.
fn arc_net() -> [(f64, f64, f64); 7] {
    let s = FRAC_1_SQRT_2;
    [
        (1.0, 0.0, 1.0),
        (1.0, 1.0, s),
        (0.0, 1.0, 1.0),
        (-1.0, 1.0, s),
        (-1.0, 0.0, 1.0),
        (-1.0, -1.0, s),
        (0.0, -1.0, 1.0),
    ]
}

fn arc_at(z: f64, rational: bool) -> Curve {
    let pts: Vec<[f64; 3]> = arc_net().iter().map(|&(x, y, _)| [x, y, z]).collect();
    let w = rational.then(|| arc_net().iter().map(|&(_, _, w)| w).collect());
    Curve::Nurbs(Arc::new(
        NurbsCurve::<3>::new(KnotVector::new(2, arc_knots()).unwrap(), pts, w).unwrap(),
    ))
}

/// The 270 degree cylinder of radius 1 about z over `z` in `[0, 1]` (rational), or the
/// polynomial surface on the same net (not a cylinder).
fn net_surface(rational: bool) -> Surface {
    let pts: Vec<[f64; 3]> = arc_net()
        .iter()
        .flat_map(|&(x, y, _)| [[x, y, 0.0], [x, y, 1.0]])
        .collect();
    let w: Vec<f64> = arc_net().iter().flat_map(|&(_, _, w)| [w, w]).collect();
    Surface::Nurbs(Arc::new(
        NurbsSurface::<3>::new(
            KnotVector::new(2, arc_knots()).unwrap(),
            KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap(),
            Net::new(7, 2, pts).unwrap(),
            rational.then(|| Net::new(7, 2, w).unwrap()),
        )
        .unwrap(),
    ))
}

/// The net surface trimmed to `0 <= z <= top`: one face of four iso-parametric edges.
fn trimmed(top: f64, rational: bool) -> (Brep, EdgeId) {
    let mut b = B::new();
    let v00 = b.vertex([1.0, 0.0, 0.0]);
    let v10 = b.vertex([0.0, -1.0, 0.0]);
    let v11 = b.vertex([0.0, -1.0, top]);
    let v01 = b.vertex([1.0, 0.0, top]);
    let bottom = b.edge(arc_at(0.0, rational), 0.0, 1.0, [v00, v10]);
    let right = b.line(v10, v11);
    let upper = b.edge(arc_at(top, rational), 0.0, 1.0, [v01, v11]);
    let left = b.line(v00, v01);
    b.face(
        net_surface(rational),
        vec![
            (bottom, Sense::Forward, line2([0.0, 0.0], [1.0, 0.0])),
            (right, Sense::Forward, line2([1.0, 0.0], [0.0, top])),
            (upper, Sense::Reversed, line2([0.0, top], [1.0, 0.0])),
            (left, Sense::Reversed, line2([0.0, 0.0], [0.0, top])),
        ],
    );
    (finish(b.d), bottom)
}

#[test]
fn nurbs_rims_and_freeform_faces_are_named_not_sampled() {
    // The rational 270-degree cylinder is recognised, but its rims are NURBS curves:
    // where the wall's facing changes along them is not found in closed form yet.
    let (body, bottom) = trimmed(0.5, true);
    let o = run(&body, &view(v(1.0, 0.3, 0.2)));
    assert!(o.stretches.is_empty(), "{o:?}");
    assert!(
        o.undecided.iter().any(|u| u.edge == bottom
            && u.why == OutlineUndecided::Cuts(scx_hlr::Undecided::NurbsEdge { edge: bottom })),
        "{o:?}"
    );
    // The same net without weights is no quadric: every edge names the face.
    let (body, _) = trimmed(0.5, false);
    let o = run(&body, &view(v(1.0, 0.3, 0.2)));
    assert!(o.stretches.is_empty());
    assert_eq!(o.undecided.len(), 4);
    assert!(o.undecided.iter().all(|u| matches!(
        u.why,
        OutlineUndecided::Face {
            why: scx_hlr::Why::Freeform,
            ..
        }
    )));
}
