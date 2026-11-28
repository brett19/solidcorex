// Written for solidcorex. Apache-2.0.
//! [`section`] and [`face_section`] (BO14) judged against closed forms: the areas of a
//! plane's sections of a box, sphere, cylinder, cone and torus, the lengths of their
//! boundaries, the boundary on both the plane and the body's surface, a sheet's section as a
//! wire, one face's section, and the typed refusals. No answer is read from the pin; its
//! `box_section_is_four_lines_ten_long` and `a_box_s_record_faces_section_at_their_index`
//! are ported as the box and face cases.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use core::f64::consts::{PI, TAU};

use scx_boolean::{
    BooleanError, BooleanOp, BooleanRefusal, BooleanUnsupported, NoItems, SectionRefusal,
    SectionUnsupported, boolean, face_section, section,
};
use scx_brep::op::{Cx, Done, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, EdgeShape, FaceId};
use scx_check::{Class, Scope};
use scx_mass::RelAccuracy;
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Plane, Sphere, Surface, SurfaceKind};
use scx_testkit::bodies::made::{block, cone, cylinder, sphere, torus};

fn frame(o: [f64; 3], z: [f64; 3]) -> Frame {
    let x = if z[2].abs() > 0.5 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(z[0], z[1], z[2]),
        Vec3::new(x[0], x[1], x[2]),
    )
    .unwrap()
}

/// The plane through `o` square to `n`.
fn plane(o: [f64; 3], n: [f64; 3]) -> Surface {
    Surface::Plane(Plane::new(frame(o, n)))
}

fn cuboid(lo: [f64; 3], hi: [f64; 3], doc: Document) -> Brep {
    let size = [0, 1, 2].map(|i| Length::from_mm(hi[i] - lo[i]));
    block(&frame(lo, [0.0, 0.0, 1.0]), size, &doc).unwrap().body
}

fn cyl(o: [f64; 3], r: f64, h: f64, doc: Document) -> Brep {
    cylinder(
        &frame(o, [0.0, 0.0, 1.0]),
        Length::from_mm(r),
        Length::from_mm(h),
        &doc,
    )
    .unwrap()
    .body
}

fn ball(o: [f64; 3], r: f64, doc: Document) -> Brep {
    sphere(&frame(o, [0.0, 0.0, 1.0]), Length::from_mm(r), &doc)
        .unwrap()
        .body
}

fn ring(major: f64, minor: f64, doc: Document) -> Brep {
    torus(
        &frame([0.0; 3], [0.0, 0.0, 1.0]),
        Length::from_mm(major),
        Length::from_mm(minor),
        &doc,
    )
    .unwrap()
    .body
}

fn run(body: &Brep, by: &Surface, doc: Document) -> Result<Done<NoItems>, BooleanError> {
    let (mut scratch, stop, settings) = (Scratch::new(), Stop::new(), Settings::new());
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    section(body, by, &mut cx)
}

fn run_face(
    body: &Brep,
    face: FaceId,
    by: &Surface,
    doc: Document,
) -> Result<Done<NoItems>, BooleanError> {
    let (mut scratch, stop, settings) = (Scratch::new(), Stop::new(), Settings::new());
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    face_section(body, face, by, &mut cx)
}

fn cut(body: &Brep, by: &Surface, doc: Document) -> Brep {
    run(body, by, doc).unwrap_or_else(|e| panic!("{e:?}")).body
}

fn acc() -> RelAccuracy {
    RelAccuracy::new(1e-10).unwrap()
}

fn area(body: &Brep) -> f64 {
    body.faces()
        .map(|f| {
            scx_mass::face_area(
                f,
                acc(),
                &mut Scratch::new(),
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap()
            .value
        })
        .sum()
}

/// Each edge's length, ascending.
fn lengths(body: &Brep) -> Vec<f64> {
    let mut v: Vec<f64> = body
        .edges()
        .map(|e| {
            scx_mass::edge_length(
                e,
                acc(),
                &mut Scratch::new(),
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap()
            .value
        })
        .collect();
    v.sort_by(f64::total_cmp);
    v
}

#[track_caller]
fn near(got: f64, want: f64, tol: f64) {
    assert!(
        (got - want).abs() <= tol * want.abs().max(1.0),
        "{got} against {want}"
    );
}

#[track_caller]
fn same(got: &[f64], want: &[f64]) {
    let mut want = want.to_vec();
    want.sort_by(f64::total_cmp);
    assert_eq!(got.len(), want.len(), "{got:?} against {want:?}");
    for (g, w) in got.iter().zip(&want) {
        near(*g, *w, 1e-7);
    }
}

/// A sheet: every face void on both sides, every edge free (one fin), and scx-check's
/// class `Sheet`.
#[track_caller]
fn is_sheet(body: &Brep, doc: Document) {
    assert!(body.faces().len() > 0);
    assert!(body.regions().all(|r| !r.is_solid()));
    assert!(body.edges().all(|e| e.fins().len() == 1));
    let (mut scratch, stop, settings) = (Scratch::new(), Stop::new(), Settings::new());
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let class = scx_check::check(body, &mut cx, Scope::Import)
        .unwrap()
        .class;
    assert_eq!(class, Class::Sheet);
}

/// Every edge of `body` sampled at nine points lies within `tol` of the plane (`o`, `n`)
/// and has `on(p)` (its distance to the body's surface) within `tol`.
#[track_caller]
fn on_both(body: &Brep, o: [f64; 3], n: [f64; 3], on: impl Fn(Point3) -> f64, tol: f64) {
    let o = Point3::new(o[0], o[1], o[2]);
    let n = Vec3::new(n[0], n[1], n[2]);
    for e in body.edges() {
        let EdgeShape::Curve { curve, interval } = e.shape() else {
            continue;
        };
        for k in 0..=8 {
            let p = curve.point(interval.min() + interval.length() * f64::from(k) / 8.0);
            assert!((p - o).dot(n).abs() <= tol, "{p:?} off the plane");
            assert!(on(p) <= tol, "{p:?} off the body by {}", on(p));
        }
    }
}

/// The pin's `box_section_is_four_lines_ten_long`: a 10 mm box cut by x = 5 is a 10 by 10
/// square, its four edges ten long at x = 5 on the box's side faces.
#[test]
fn a_box_s_section_is_a_square_of_four_lines_ten_long() {
    let doc = Document::default();
    let b = cuboid([0.0; 3], [10.0; 3], doc);
    let s = cut(&b, &plane([5.0, 5.0, 5.0], [1.0, 0.0, 0.0]), doc);
    is_sheet(&s, doc);
    assert_eq!(s.faces().len(), 1);
    near(area(&s), 100.0, 1e-12);
    same(&lengths(&s), &[10.0; 4]);
    let off_box = |p: Point3| {
        [p.y, 10.0 - p.y, p.z, 10.0 - p.z]
            .into_iter()
            .map(f64::abs)
            .fold(f64::INFINITY, f64::min)
    };
    on_both(&s, [5.0; 3], [1.0, 0.0, 0.0], off_box, 1e-9);
}

/// A plane at a slant through a box: x + y + z = 15 cuts a 10 mm cube through the middles of
/// six edges, a regular hexagon of side 5 sqrt 2 and area (3 sqrt 3 / 2) s^2.
#[test]
fn a_cube_s_middle_section_on_its_diagonal_is_a_regular_hexagon() {
    let doc = Document::default();
    let b = cuboid([0.0; 3], [10.0; 3], doc);
    let s = cut(&b, &plane([5.0; 3], [1.0, 1.0, 1.0]), doc);
    is_sheet(&s, doc);
    let side = 5.0 * 2f64.sqrt();
    near(area(&s), 1.5 * 3f64.sqrt() * side * side, 1e-10);
    same(&lengths(&s), &[side; 6]);
}

/// A sphere of radius 2 cut at height d from its centre: a disc of area pi (r^2 - d^2),
/// bounded by a circle of that radius on the sphere.
#[test]
fn a_sphere_s_section_is_a_disc() {
    let doc = Document::default();
    let b = ball([1.0, 2.0, 3.0], 2.0, doc);
    for d in [0.0, 1.0, -1.5] {
        let s = cut(&b, &plane([1.0, 2.0, 3.0 + d], [0.0, 0.0, 1.0]), doc);
        is_sheet(&s, doc);
        near(area(&s), PI * (4.0 - d * d), 1e-9);
        let c = Point3::new(1.0, 2.0, 3.0);
        on_both(
            &s,
            [1.0, 2.0, 3.0 + d],
            [0.0, 0.0, 1.0],
            |p| ((p - c).length() - 2.0).abs(),
            1e-9,
        );
        let total: f64 = lengths(&s).iter().sum();
        near(total, TAU * (4.0 - d * d).sqrt(), 1e-9);
    }
}

/// A cylinder of radius 1.5 and height 4: across its axis a disc of pi r^2; along it, through
/// the axis, a 3 by 4 rectangle; at a slant through the wall only, an ellipse of area
/// pi r^2 / cos(theta).
#[test]
fn a_cylinder_s_sections() {
    let doc = Document::default();
    let r = 1.5;
    let b = cyl([0.0; 3], r, 4.0, doc);
    let on_wall = |p: Point3| ((p.x * p.x + p.y * p.y).sqrt() - r).abs();

    let disc = cut(&b, &plane([0.0, 0.0, 2.5], [0.0, 0.0, 1.0]), doc);
    is_sheet(&disc, doc);
    near(area(&disc), PI * r * r, 1e-9);
    on_both(&disc, [0.0, 0.0, 2.5], [0.0, 0.0, 1.0], on_wall, 1e-9);

    let along = cut(&b, &plane([0.0; 3], [1.0, 0.0, 0.0]), doc);
    is_sheet(&along, doc);
    near(area(&along), 2.0 * r * 4.0, 1e-9);

    let theta = 0.3_f64;
    let n = [theta.sin(), 0.0, theta.cos()];
    let slant = cut(&b, &plane([0.0, 0.0, 2.0], n), doc);
    is_sheet(&slant, doc);
    near(area(&slant), PI * r * r / theta.cos(), 1e-9);
    on_both(&slant, [0.0, 0.0, 2.0], n, on_wall, 1e-9);
}

/// A cone of radii 2 and 1 over height 2, cut across its axis at height 1: a disc of
/// radius 1.5.
#[test]
fn a_cone_s_section_across_its_axis_is_a_disc() {
    let doc = Document::default();
    let b = cone(
        &frame([0.0; 3], [0.0, 0.0, 1.0]),
        [Length::from_mm(2.0), Length::from_mm(1.0)],
        Length::from_mm(2.0),
        &doc,
    )
    .unwrap()
    .body;
    let s = cut(&b, &plane([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]), doc);
    is_sheet(&s, doc);
    near(area(&s), PI * 1.5 * 1.5, 1e-9);
}

/// A torus of radii 3 and 1: across its axis at height d, an annulus of area
/// 4 pi R sqrt(r^2 - d^2); through its axis, two discs of pi r^2.
#[test]
fn a_torus_s_sections() {
    let doc = Document::default();
    let (big, small) = (3.0, 1.0);
    let b = ring(big, small, doc);
    let on_tube = |p: Point3| {
        let rho = (p.x * p.x + p.y * p.y).sqrt() - big;
        ((rho * rho + p.z * p.z).sqrt() - small).abs()
    };
    for d in [0.0, 0.5] {
        let s = cut(&b, &plane([0.0, 0.0, d], [0.0, 0.0, 1.0]), doc);
        is_sheet(&s, doc);
        near(
            area(&s),
            4.0 * PI * big * (small * small - d * d).sqrt(),
            1e-9,
        );
        on_both(&s, [0.0, 0.0, d], [0.0, 0.0, 1.0], on_tube, 1e-9);
    }
    let s = cut(&b, &plane([0.0; 3], [1.0, 0.0, 0.0]), doc);
    is_sheet(&s, doc);
    assert_eq!(s.faces().len(), 2);
    near(area(&s), 2.0 * PI * small * small, 1e-9);
    on_both(&s, [0.0; 3], [1.0, 0.0, 0.0], on_tube, 1e-9);
}

/// A plane that misses the body gives a body of nothing.
#[test]
fn a_plane_that_misses_gives_nothing() {
    let doc = Document::default();
    let b = cuboid([0.0; 3], [1.0; 3], doc);
    let s = cut(&b, &plane([0.0, 0.0, 3.0], [0.0, 0.0, 1.0]), doc);
    assert_eq!(s.faces().len(), 0);
    assert_eq!(s.edges().len(), 0);
    assert_eq!(s.vertices().len(), 0);
}

/// A plane on a face of the box: the section is that face, the closure of the meet.
#[test]
fn a_plane_on_a_face_gives_the_face() {
    let doc = Document::default();
    let b = cuboid([0.0; 3], [2.0, 3.0, 1.0], doc);
    for z in [0.0, 1.0] {
        let s = cut(&b, &plane([0.0, 0.0, z], [0.0, 0.0, 1.0]), doc);
        is_sheet(&s, doc);
        near(area(&s), 6.0, 1e-12);
        same(&lengths(&s), &[2.0, 2.0, 3.0, 3.0]);
    }
}

/// A plane that only touches a solid: the closure of the meet is lower-dimensional. A
/// tangent plane on a sphere's pole is that point; a plane through a box's edge and outside
/// it, the edge; one through its corner, the corner; a plane tangent to a cylinder's wall, the
/// line of contact as long as the cylinder.
#[test]
fn a_plane_that_touches_gives_points_and_wires() {
    let doc = Document::default();
    let pole = cut(
        &ball([0.0; 3], 2.0, doc),
        &plane([0.0, 0.0, 2.0], [0.0, 0.0, 1.0]),
        doc,
    );
    assert_eq!((pole.faces().len(), pole.edges().len()), (0, 0));
    let points: Vec<Point3> = pole.vertices().map(|v| v.point()).collect();
    assert_eq!(points.len(), 1);
    assert!((points[0] - Point3::new(0.0, 0.0, 2.0)).length() <= 1e-9);

    let b = cuboid([0.0; 3], [10.0; 3], doc);
    let edge = cut(&b, &plane([0.0; 3], [1.0, 1.0, 0.0]), doc);
    assert_eq!(edge.faces().len(), 0);
    same(&lengths(&edge), &[10.0]);
    on_both(&edge, [0.0; 3], [1.0, 1.0, 0.0], |p| p.x.hypot(p.y), 1e-12);

    let corner = cut(&b, &plane([0.0; 3], [1.0, 1.0, 1.0]), doc);
    assert_eq!((corner.faces().len(), corner.edges().len()), (0, 0));
    let points: Vec<Point3> = corner.vertices().map(|v| v.point()).collect();
    assert_eq!(points.len(), 1);
    assert!((points[0] - Point3::new(0.0, 0.0, 0.0)).length() <= 1e-12);

    let wall = cut(
        &cyl([0.0; 3], 1.5, 4.0, doc),
        &plane([1.5, 0.0, 0.0], [1.0, 0.0, 0.0]),
        doc,
    );
    assert_eq!(wall.faces().len(), 0);
    same(&lengths(&wall), &[4.0]);
    on_both(&wall, [1.5, 0.0, 0.0], [1.0, 0.0, 0.0], |p| p.y.abs(), 1e-9);
}

/// A plane on one solid's face that touches another solid of the same body at a point:
/// the sheet with a point beside it is not built yet.
#[test]
fn a_touch_beside_the_sheet_is_unsupported() {
    let doc = Document::default();
    let (mut scratch, stop, settings) = (Scratch::new(), Stop::new(), Settings::new());
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let two = boolean(
        &cuboid([0.0; 3], [1.0; 3], doc),
        &ball([5.0, 0.0, 0.0], 1.0, doc),
        BooleanOp::Union,
        &mut cx,
    )
    .unwrap()
    .body;
    assert_eq!(
        run(&two, &plane([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]), doc).err(),
        Some(BooleanError::Unsupported(BooleanUnsupported::Section(
            SectionUnsupported::TouchBesideSheet
        )))
    );
    // Lower, the plane crosses both: a square and a disc.
    let s = cut(&two, &plane([0.0, 0.0, 0.5], [0.0, 0.0, 1.0]), doc);
    is_sheet(&s, doc);
    near(area(&s), 1.0 + PI * 0.75, 1e-9);
}

/// A sheet cut by a plane is a wire: the box's square section, cut by z = 5, is the line
/// across it, ten long; the sphere's disc, cut through its centre, its diameter.
#[test]
fn a_sheet_s_section_is_a_wire() {
    let doc = Document::default();
    let b = cuboid([0.0; 3], [10.0; 3], doc);
    let square = cut(&b, &plane([5.0; 3], [1.0, 0.0, 0.0]), doc);
    let line = cut(&square, &plane([0.0, 0.0, 5.0], [0.0, 0.0, 1.0]), doc);
    assert_eq!(line.faces().len(), 0);
    same(&lengths(&line), &[10.0]);
    on_both(
        &line,
        [5.0, 0.0, 5.0],
        [1.0, 0.0, 0.0],
        |p| (p.z - 5.0).abs(),
        1e-9,
    );

    let disc = cut(
        &ball([0.0; 3], 2.0, doc),
        &plane([0.0; 3], [0.0, 0.0, 1.0]),
        doc,
    );
    let chord = cut(&disc, &plane([0.0, 1.0, 0.0], [0.0, 1.0, 0.0]), doc);
    assert_eq!(chord.faces().len(), 0);
    same(&lengths(&chord), &[2.0 * 3f64.sqrt()]);
}

/// The pin's `a_box_s_record_faces_section_at_their_index`: z = 5 cuts each of a 10 mm box's
/// four side faces in one line at z = 5, ten long, and misses the top and bottom.
#[test]
fn one_face_s_section() {
    let doc = Document::default();
    let b = cuboid([0.0; 3], [10.0; 3], doc);
    let by = plane([0.0, 0.0, 5.0], [0.0, 0.0, 2.0]);
    let mut cut_faces = 0;
    for f in b.faces() {
        let w = run_face(&b, f.id(), &by, doc).unwrap().body;
        assert_eq!(w.faces().len(), 0);
        match lengths(&w).as_slice() {
            [] => {}
            [l] => {
                near(*l, 10.0, 1e-12);
                on_both(&w, [0.0, 0.0, 5.0], [0.0, 0.0, 1.0], |_| 0.0, 1e-12);
                cut_faces += 1;
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(cut_faces, 4);
}

/// One face of a sphere through its centre: the great circle, 2 pi r long.
#[test]
fn one_face_of_a_sphere() {
    let doc = Document::default();
    let b = ball([0.0; 3], 2.0, doc);
    let f = b.faces().next().unwrap().id();
    let w = run_face(&b, f, &plane([0.0; 3], [0.0, 0.0, 1.0]), doc)
        .unwrap()
        .body;
    let total: f64 = lengths(&w).iter().sum();
    near(total, TAU * 2.0, 1e-9);
}

#[test]
fn refusals() {
    let doc = Document::default();
    let b = cuboid([0.0; 3], [1.0; 3], doc);
    let ball_surface = Surface::Sphere(
        Sphere::new(
            frame([0.0; 3], [0.0, 0.0, 1.0]),
            Length::from_mm(1.0),
            scx_num::frame::Sense::Forward,
        )
        .unwrap(),
    );
    assert_eq!(
        run(&b, &ball_surface, doc).err(),
        Some(BooleanError::Unsupported(BooleanUnsupported::Section(
            SectionUnsupported::Surface(SurfaceKind::Sphere)
        )))
    );
    let by = plane([0.0, 0.0, 0.5], [0.0, 0.0, 1.0]);
    let missing = FaceId::from_raw(99);
    assert_eq!(
        run_face(&b, missing, &by, doc).err(),
        Some(BooleanError::Refused(BooleanRefusal::Section(
            SectionRefusal::NoFace(missing)
        )))
    );
    // A sheet in the plane: its section is the sheet itself, not a wire.
    let square = cut(&b, &by, doc);
    assert!(matches!(
        run(&square, &by, doc).err(),
        Some(BooleanError::Unsupported(BooleanUnsupported::Section(
            SectionUnsupported::InPlane(_)
        )))
    ));
}

/// The same body and plane give the same answer and the same work.
#[test]
fn deterministic() {
    let doc = Document::default();
    let b = ring(3.0, 1.0, doc);
    let by = plane([0.0, 0.0, 0.25], [0.0, 0.0, 1.0]);
    let one = run(&b, &by, doc).unwrap();
    let two = run(&b, &by, doc).unwrap();
    assert_eq!(one.report.work, two.report.work);
    assert_eq!(lengths(&one.body), lengths(&two.body));
    assert_eq!(one.body.faces().len(), two.body.faces().len());
}
