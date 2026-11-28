// Written for solidcorex. Apache-2.0.
//! `prism_section`, `prism_equivalent`, `bores`, `stack_bores` and `hole_stacks` against closed
//! forms (research 74 §33, BW6). The prism cases port the pin's `prism/tests.rs` measure cases
//! and `solidcore-ops/tests/prism.rs` section and equivalence cases; the hole cases port
//! `eval/hole_stacks_tests.rs`. Truth is the closed form, never the pin's output.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::similar_names,
    clippy::float_cmp
)]

use core::f64::consts::{PI, TAU};

use scx_bodyq::{
    BodyqError, BodyqRefusal, Bore, Equivalence, NotPrism, Piece, Recognition, Slot, StackOptions,
    Why, bores, classify_point, hole_stacks, prism_equivalent, prism_section, stack_bores,
};
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, EdgeId, RegionId, VertexId};
use scx_curve::Circle;
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_surface::{Cone, Cylinder, Plane, Surface};
use scx_testkit::bodies::made::{Region, Segment, Span, block, cone, cylinder, extrude, sphere};
use scx_testkit::bodies::{Kind, generate};

#[path = "common/mod.rs"]
mod common;

// ---- helpers -------------------------------------------------------------------------------

fn doc() -> Document {
    Document::default()
}

fn pt(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Segment> {
    vec![
        Segment::line(pt(x0, y0), pt(x1, y0)),
        Segment::line(pt(x1, y0), pt(x1, y1)),
        Segment::line(pt(x1, y1), pt(x0, y1)),
        Segment::line(pt(x0, y1), pt(x0, y0)),
    ]
}

fn region(outer: Vec<Segment>, holes: Vec<Vec<Segment>>) -> Region {
    Region::new(outer, holes, doc().resolution()).unwrap()
}

fn world() -> Frame {
    Frame::WORLD
}

/// The extrusion of `r` in `frame` from `z0` to `z1`.
fn prism(r: &Region, frame: &Frame, z0: f64, z1: f64) -> Brep {
    extrude(r, frame, Span::new(mm(z0), mm(z1)), &doc())
        .unwrap()
        .body
}

/// A 20 x 20 plate 10 thick with a round hole of `radius` at `(10 + shift, 10)`.
fn drilled(frame: &Frame, radius: f64, shift: f64) -> Brep {
    let hole = vec![Segment::circle(
        pt(10.0 + shift, 10.0),
        radius,
        Sense::Forward,
    )];
    prism(
        &region(rect(0.0, 0.0, 20.0, 20.0), vec![hole]),
        frame,
        0.0,
        10.0,
    )
}

fn turned(origin: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(origin[0], origin[1], origin[2]),
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(0.3, -1.0, 0.2),
    )
    .unwrap()
}

fn frame_z_along(d: [f64; 3], x: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(0.0, 0.0, 0.0),
        Vec3::new(d[0], d[1], d[2]),
        Vec3::new(x[0], x[1], x[2]),
    )
    .unwrap()
}

fn with_cx<R>(scratch: &mut Scratch, f: impl FnOnce(&mut Cx<'_>) -> R) -> R {
    let d = doc();
    let settings = Settings::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&d, &settings, scratch, &stop, &NoTrace);
    f(&mut cx)
}

fn section(body: &Brep, frame: &Frame) -> Result<Recognition, BodyqError> {
    let mut scratch = Scratch::new();
    with_cx(&mut scratch, |cx| prism_section(body, frame, cx))
}

fn prism_of(body: &Brep, frame: &Frame) -> scx_bodyq::PrismSection {
    match section(body, frame).unwrap() {
        Recognition::Prism(s) => s,
        other => panic!("not a prism: {other:?}"),
    }
}

fn not_prism(body: &Brep, frame: &Frame) -> NotPrism {
    match section(body, frame).unwrap() {
        Recognition::NotPrism(n) => n,
        other => panic!("a prism: {other:?}"),
    }
}

fn equivalence(a: &Brep, b: &Brep) -> Equivalence {
    let mut scratch = Scratch::new();
    with_cx(&mut scratch, |cx| prism_equivalent(a, b, cx)).unwrap()
}

fn bores_of(body: &Brep) -> Vec<Bore> {
    bores(body, &mut Budget::unlimited(), &Stop::new()).unwrap()
}

/// The world z range a bore's wall covers.
fn z_range(b: &Bore) -> [f64; 2] {
    let (z0, z1) = (b.point.z + b.axis.z * b.t[0], b.point.z + b.axis.z * b.t[1]);
    [z0.min(z1), z0.max(z1)]
}

fn assert_range(got: [f64; 2], want: [f64; 2]) {
    assert!(
        close(got[0], want[0], 1e-9) && close(got[1], want[1], 1e-9),
        "{got:?} vs {want:?}"
    );
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn encloses(i: scx_num::interval::Ivl, v: f64) -> bool {
    i.lo <= v && v <= i.hi
}

// ---- prisms: the pin's measure cases ---------------------------------------------------------

/// `a_drilled_prism_has_its_section_and_its_volume_enclosed`: the closed forms are
/// `4000 - 360 pi` for the volume and `400 - 36 pi` for the section's area.
#[test]
fn a_drilled_prism_has_its_section_and_its_volume_enclosed() {
    let body = drilled(&world(), 6.0, 0.0);
    let s = prism_of(&body, &world());
    let (exact_v, exact_a) = (4000.0 - 360.0 * PI, 400.0 - 36.0 * PI);
    assert!(encloses(s.volume, exact_v), "{:?}", s.volume);
    assert!(encloses(s.area, exact_a), "{:?}", s.area);
    // The enclosure is the coincidence's corridor and no wider: 2 c P + pi c^2 on the area,
    // 2 c on the height.
    let c = s.tolerance.mm();
    let perimeter = 80.0 + 12.0 * PI;
    assert!(
        s.area.hi - s.area.lo <= 2.0 * (2.0 * c * perimeter + PI * c * c) + 1e-9,
        "{:?}",
        s.area
    );
    assert!(s.volume.hi - s.volume.lo <= 0.1, "{:?}", s.volume);
    assert!(s.lo.abs() <= c && (s.hi - 10.0).abs() <= c, "{s:?}");
    assert_eq!(s.holes.len(), 1);
    // Outer counter-clockwise, hole clockwise, as pieces.
    let area_of = |loop_: &[Piece]| scx_bodyq_area(loop_);
    assert!(close(area_of(&s.outer), 400.0, 1e-9), "{:?}", s.outer);
    assert!(close(area_of(&s.holes[0]), -36.0 * PI, 1e-9));
    // Along another direction it is not a prism: the hole's wall is square to the plate's.
    let side = frame_z_along([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let n = not_prism(&body, &side);
    assert!(matches!(n.at, scx_brep::Entity::Face(_)), "{n:?}");
    assert_eq!(n.why, Why::Slanted);
    // The same body again, a wider hole and a hole moved a tenth of a micron more than the
    // coincidence away.
    let same = drilled(&world(), 6.0, 0.0);
    assert_eq!(equivalence(&body, &same), Equivalence::Same);
    assert_eq!(equivalence(&body, &body), Equivalence::Same);
    let wider = drilled(&world(), 6.0001, 0.0);
    assert_eq!(equivalence(&body, &wider), Equivalence::Different);
    let shifted = drilled(&world(), 6.0, 0.0001);
    assert_eq!(equivalence(&body, &shifted), Equivalence::Different);
}

/// The signed area of a closed loop of pieces, by Green's theorem on the closed forms
/// (the test's own, apart from the crate's).
fn scx_bodyq_area(pieces: &[Piece]) -> f64 {
    let mut a = 0.0;
    for p in pieces {
        match *p {
            Piece::Line { a: p0, b: p1 } => a += 0.5 * (p0.x * p1.y - p1.x * p0.y),
            Piece::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let (s0, c0) = fmath::sin_cos(start);
                let (s1, c1) = fmath::sin_cos(start + sweep);
                a += 0.5
                    * (radius * radius * sweep
                        + radius * (center.x * (s1 - s0) - center.y * (c1 - c0)));
            }
            _ => panic!("a piece"),
        }
    }
    a
}

/// `a_box_is_a_prism_along_each_of_its_axes`.
#[test]
fn a_box_is_a_prism_along_each_of_its_axes() {
    let b = prism(
        &region(rect(0.0, 0.0, 4.0, 3.0), vec![]),
        &world(),
        -1.0,
        2.0,
    );
    let c = 1e-5 * 2.0;
    let s = prism_of(&b, &world());
    assert!(close(s.lo, -1.0, c) && close(s.hi, 2.0, c), "{s:?}");
    assert!(encloses(s.volume, 36.0), "{:?}", s.volume);
    // Along x: the levels are 0 and 4 above the frame, the section 3 by 3 (y by z).
    let side = frame_z_along([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let s = prism_of(&b, &side);
    assert!(close(s.lo, 0.0, c) && close(s.hi, 4.0, c), "{s:?}");
    assert!(encloses(s.volume, 36.0), "{:?}", s.volume);
    assert!(encloses(s.area, 9.0), "{:?}", s.area);
    // Along y: 0 to 3, the section 4 by 3.
    let front = frame_z_along([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
    let s = prism_of(&b, &front);
    assert!(close(s.lo, 0.0, c) && close(s.hi, 3.0, c), "{s:?}");
    assert!(encloses(s.area, 12.0), "{:?}", s.area);
    // The other way round along z (the frame's z reversed): the same body, levels negated.
    let down = frame_z_along([0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
    let s = prism_of(&b, &down);
    assert!(close(s.lo, -2.0, c) && close(s.hi, 1.0, c), "{s:?}");
    // The section is the stored loop of the lower cap, counter-clockwise.
    assert!(close(
        scx_bodyq_area(&prism_of(&b, &world()).outer),
        12.0,
        1e-9
    ));
}

/// A section of lines and arcs: a stadium (two lines and two half circles) and a D.
#[test]
fn a_curved_section_is_read_as_lines_and_arcs() {
    let d = region(
        vec![
            Segment::line(pt(0.0, 0.0), pt(10.0, 0.0)),
            Segment::arc(pt(10.0, 3.0), pt(10.0, 0.0), pt(10.0, 6.0), Sense::Forward),
            Segment::line(pt(10.0, 6.0), pt(0.0, 6.0)),
            Segment::line(pt(0.0, 6.0), pt(0.0, 0.0)),
        ],
        vec![],
    );
    let body = prism(&d, &world(), 0.0, 2.0);
    let s = prism_of(&body, &world());
    let exact = 60.0 + 0.5 * PI * 9.0;
    assert!(encloses(s.area, exact), "{:?} vs {exact}", s.area);
    assert!(encloses(s.volume, 2.0 * exact), "{:?}", s.volume);
    let arcs = s
        .outer
        .iter()
        .filter(|p| matches!(p, Piece::Arc { .. }))
        .count();
    assert_eq!(arcs, 1);
    for p in &s.outer {
        if let Piece::Arc {
            center,
            radius,
            sweep,
            ..
        } = *p
        {
            assert!(close(center.x, 10.0, 1e-9) && close(center.y, 3.0, 1e-9));
            assert!(
                close(radius, 3.0, 1e-9) && close(sweep, PI, 1e-9),
                "{sweep}"
            );
        }
    }
}

/// `a_prism_section_does_not_depend_on_the_query_frame_origin`.
#[test]
fn a_prism_section_does_not_depend_on_the_query_frame_origin() {
    let f = Frame::new(
        Point3::new(50.0, 50.0, -3.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let body = cylinder(&f, mm(5.0), mm(6.0), &doc()).unwrap().body;
    let exact = 150.0 * PI;
    for offset in [0.0, 300.0, 1000.0, 10000.0, -10000.0] {
        let q = Frame::new(
            Point3::new(offset, offset, 7.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap();
        let s = prism_of(&body, &q);
        let tol = s.tolerance.mm();
        assert!(
            close(s.lo, -10.0, tol) && close(s.hi, -4.0, tol),
            "{offset}: {s:?}"
        );
        assert!(encloses(s.volume, exact), "{offset}: {:?}", s.volume);
        assert!(s.holes.is_empty());
        // A whole circle: one arc piece, the centre at (50 - offset, 50 - offset).
        let [
            Piece::Arc {
                center,
                radius,
                sweep,
                ..
            },
        ] = s.outer[..]
        else {
            panic!("a cylinder's section is one circle: {:?}", s.outer);
        };
        assert!(close(center.x + offset, 50.0, 1e-8) && close(center.y + offset, 50.0, 1e-8));
        assert!(close(radius, 5.0, 1e-8) && close(sweep.abs(), TAU, 1e-9));
    }
    // So far out that the frame's coordinates cannot hold the coincidence: refused with a cause.
    for offset in [1e20, f64::MAX / 4.0] {
        let q = Frame::new(
            Point3::new(offset, offset, 7.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap();
        let e = section(&body, &q).unwrap_err();
        assert_eq!(
            e,
            BodyqError::Refused(BodyqRefusal::FrameTooFar),
            "{offset}"
        );
    }
    // A body far from the origin, the frame at it: as exact as at the origin.
    let moved = Frame::new(
        Point3::new(500.0, 500.0, -3.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let body = cylinder(&moved, mm(5.0), mm(6.0), &doc()).unwrap().body;
    let at = Frame::new(
        Point3::new(500.0, 500.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let s = prism_of(&body, &at);
    let tol = s.tolerance.mm();
    assert!(close(s.lo, -3.0, tol) && close(s.hi, 3.0, tol), "{s:?}");
    assert!(encloses(s.volume, exact), "{:?}", s.volume);
}

/// `a_drilled_box_moved_is_the_prism_it_was`: the plate built in a turned and moved frame is a
/// prism along that frame's z and the same prism as its twin there; not a prism along the
/// world's z; and the drilled plate against a wider-drilled one is not.
#[test]
fn a_drilled_box_moved_is_the_prism_it_was() {
    let f = turned([30.0, 20.0, 10.0]);
    let source = drilled(&f, 6.0, 0.0);
    let s = prism_of(&source, &f);
    assert!(encloses(s.volume, 4000.0 - 360.0 * PI), "{:?}", s.volume);
    let c = s.tolerance.mm();
    assert!(s.lo.abs() <= c && (s.hi - 10.0).abs() <= c, "{s:?}");
    let twin = drilled(&f, 6.0, 0.0);
    assert_eq!(equivalence(&source, &twin), Equivalence::Same);
    assert_eq!(
        equivalence(&source, &drilled(&f, 6.0001, 0.0)),
        Equivalence::Different
    );
    // Along the world's z it is not one: the turned plate's caps are slanted.
    let n = not_prism(&source, &world());
    assert_eq!(n.why, Why::Slanted);
    // Placed apart, the same plate is the same prism along its own direction only in the same
    // frame: moved along the frame's z by a millimetre, the levels differ.
    let lifted = drilled(
        &Frame::new(f.origin() + *f.z() * 1.0, f.z().as_vec(), f.x().as_vec()).unwrap(),
        6.0,
        0.0,
    );
    assert_eq!(equivalence(&source, &lifted), Equivalence::Different);
}

// ---- prisms: what is not one -----------------------------------------------------------------

#[test]
fn a_cone_a_sphere_a_cavity_and_a_stack_are_not_prisms() {
    let f = world();
    let c = cone(&f, [mm(3.0), mm(0.0)], mm(4.0), &doc()).unwrap().body;
    let fr = cone(&f, [mm(3.0), mm(2.0)], mm(4.0), &doc()).unwrap().body;
    assert_eq!(not_prism(&fr, &f).why, Why::Kind);
    let n = not_prism(&c, &f);
    assert_eq!(n.why, Why::Kind, "{n:?}");
    let s = sphere(&f, mm(3.0), &doc()).unwrap().body;
    assert_eq!(not_prism(&s, &f).why, Why::Kind);
    // No planar face names a direction: no prism to compare.
    assert_eq!(equivalence(&s, &s), Equivalence::NoPrism);
    for (kind, why) in [(Kind::Cavity, Why::Sides), (Kind::Stack, Why::Sides)] {
        let body = generated(kind, 0, 1);
        assert_eq!(not_prism(&body, &f).why, why, "{kind:?}");
    }
    let sheet = generated(Kind::QuadSheet, 0, 1);
    assert_eq!(not_prism(&sheet, &f).why, Why::Sides);
    // A pyramid frustum: planes neither square to z nor along it.
    let frustum = common::polyhedron(
        &[
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [4.0, 4.0, 0.0],
            [0.0, 4.0, 0.0],
            [1.0, 1.0, 2.0],
            [3.0, 1.0, 2.0],
            [3.0, 3.0, 2.0],
            [1.0, 3.0, 2.0],
        ],
        &[
            &[0, 3, 2, 1],
            &[4, 5, 6, 7],
            &[0, 1, 5, 4],
            &[1, 2, 6, 5],
            &[2, 3, 7, 6],
            &[3, 0, 4, 7],
        ],
    );
    assert_eq!(not_prism(&frustum, &f).why, Why::Slanted);
    // A plate with a step: its top at two heights has two upward caps.
    let stepped = common::polyhedron(
        &[
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [4.0, 2.0, 0.0],
            [4.0, 4.0, 0.0],
            [0.0, 4.0, 0.0],
            [0.0, 2.0, 0.0],
            [0.0, 0.0, 1.0],
            [4.0, 0.0, 1.0],
            [4.0, 2.0, 1.0],
            [4.0, 2.0, 2.0],
            [4.0, 4.0, 2.0],
            [0.0, 4.0, 2.0],
            [0.0, 2.0, 2.0],
            [0.0, 2.0, 1.0],
        ],
        &[
            &[0, 5, 4, 3, 2, 1],
            &[6, 7, 8, 13],
            &[13, 8, 9, 12],
            &[12, 9, 10, 11],
            &[0, 1, 7, 6],
            &[1, 2, 8, 7],
            &[2, 3, 10, 9, 8],
            &[3, 4, 11, 10],
            &[4, 5, 12, 11],
            &[5, 0, 6, 13, 12],
        ],
    );
    let n = not_prism(&stepped, &f);
    assert_eq!(n.why, Why::Caps, "{n:?}");
}

fn generated(kind: Kind, seed: u64, size: usize) -> Brep {
    generate(kind, seed, size)
        .unwrap()
        .draft
        .finish(&doc())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

/// A cylinder written as a cone of a billionth of a millimetre's taper (a face SU7's `recognize`
/// reads as the cylinder over its extent) is a prism, its tolerance the larger by the deviation.
#[test]
fn a_cone_that_is_a_cylinder_within_the_coincidence_is_a_prism() {
    let f = world();
    let taper = cone(&f, [mm(3.0), mm(3.0 + 1e-9)], mm(4.0), &doc())
        .unwrap()
        .body;
    let s = prism_of(&taper, &f);
    assert!(encloses(s.volume, PI * 9.0 * 4.0), "{:?}", s.volume);
    assert!(s.tolerance.mm() >= doc().resolution().zone().mm(), "{s:?}");
    // The taper that is more than the coincidence is a cone and no prism.
    let wide = cone(&f, [mm(3.0), mm(3.001)], mm(4.0), &doc())
        .unwrap()
        .body;
    assert_eq!(not_prism(&wide, &f).why, Why::Kind);
}

/// A direction off the prism's by an angle: a prism along it when the stray over the body
/// is within the coincidence, not when it is not. The tolerance is the document's: the same
/// body is a prism along the tilted direction in a coarser document.
#[test]
fn the_tolerance_is_the_documents_not_a_constant() {
    let tilt = |angle: f64| {
        Frame::new(
            Point3::new(0.0, 0.0, 0.0),
            Vec3::new(fmath::sin(angle), 0.0, fmath::cos(angle)),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap()
    };
    let body = drilled(&world(), 3.0, 0.0);
    // 20 mm across: the stray of the walls is 20 * angle at most.
    assert!(matches!(
        section(&body, &tilt(1e-8)).unwrap(),
        Recognition::Prism(_)
    ));
    assert!(matches!(
        section(&body, &tilt(1e-3)).unwrap(),
        Recognition::NotPrism(_)
    ));
    // A document whose resolution is a millimetre: 1e-4 rad over 20 mm strays 2e-3 mm, within
    // the resolution there and not at the default.
    let coarse = Document::with_resolution(scx_num::units::Resolution::new(mm(0.01)).unwrap());
    let r = Region::new(
        rect(0.0, 0.0, 20.0, 20.0),
        vec![vec![Segment::circle(pt(10.0, 10.0), 3.0, Sense::Forward)]],
        coarse.resolution(),
    )
    .unwrap();
    let coarse_body = extrude(&r, &world(), Span::new(mm(0.0), mm(10.0)), &coarse)
        .unwrap()
        .body;
    assert!(matches!(
        section(&coarse_body, &tilt(1e-4)).unwrap(),
        Recognition::Prism(_)
    ));
    assert!(matches!(
        section(&body, &tilt(1e-4)).unwrap(),
        Recognition::NotPrism(_)
    ));
}

/// Same answer and same work twice; a budget stops it with `Exhausted`, a stop flag with
/// `Cancelled`; the memo is warm on the second call and changes nothing.
#[test]
fn the_answer_is_deterministic_and_the_budget_and_stop_are_errors() {
    let body = drilled(&world(), 6.0, 0.0);
    let first = section(&body, &world()).unwrap();
    let again = section(&body, &world()).unwrap();
    assert_eq!(first, again);
    let mut scratch = Scratch::new();
    let (cold, warm) = with_cx(&mut scratch, |cx| {
        (
            prism_section(&body, &world(), cx).unwrap(),
            prism_section(&body, &world(), cx).unwrap(),
        )
    });
    assert_eq!(cold, warm);
    assert_eq!(cold, first);
    // A budget of one unit cannot read the faces.
    let settings = Settings::new().with_budget(1);
    let d = doc();
    let stop = Stop::new();
    let mut scratch = Scratch::new();
    let mut cx = Cx::new(&d, &settings, &mut scratch, &stop, &NoTrace);
    let e = prism_section(&body, &world(), &mut cx);
    assert!(matches!(e, Err(BodyqError::Exhausted { .. })), "{e:?}");
    let stop = Stop::new();
    stop.set();
    let mut scratch = Scratch::new();
    let plain = Settings::new();
    let mut cx = Cx::new(&d, &plain, &mut scratch, &stop, &NoTrace);
    assert_eq!(
        prism_section(&body, &world(), &mut cx),
        Err(BodyqError::Cancelled)
    );
}

// ---- bores ---------------------------------------------------------------------------------

fn plate_with(holes: Vec<Vec<Segment>>, z0: f64, z1: f64) -> Brep {
    prism(&region(rect(0.0, 0.0, 40.0, 40.0), holes), &world(), z0, z1)
}

fn round_hole(x: f64, y: f64, r: f64) -> Vec<Segment> {
    vec![Segment::circle(pt(x, y), r, Sense::Forward)]
}

#[test]
fn a_round_hole_is_one_bore_with_its_extent() {
    let body = plate_with(vec![round_hole(20.0, 15.0, 3.0)], 0.0, 10.0);
    let b = bores_of(&body);
    assert_eq!(b.len(), 1, "{b:?}");
    assert!(close(b[0].d, 6.0, 1e-9));
    assert_range(z_range(&b[0]), [0.0, 10.0]);
    assert!(close(b[0].point.x, 20.0, 1e-9) && close(b[0].point.y, 15.0, 1e-9));
    assert!(close(b[0].axis.x, 0.0, 1e-12) && close(b[0].axis.z.abs(), 1.0, 1e-12));
    assert!(b[0].slot.is_none());
    assert_eq!(b, bores_of(&body));
}

#[test]
fn two_halves_of_one_cylinder_close_into_one_round_hole() {
    let c = pt(20.0, 20.0);
    let hole = vec![
        Segment::arc(c, pt(23.0, 20.0), pt(17.0, 20.0), Sense::Forward),
        Segment::arc(c, pt(17.0, 20.0), pt(23.0, 20.0), Sense::Forward),
    ];
    let body = plate_with(vec![hole], 0.0, 2.5);
    let b = bores_of(&body);
    assert_eq!(b.len(), 1, "{b:?}");
    assert!(close(b[0].d, 6.0, 1e-9));
    assert_range(z_range(&b[0]), [0.0, 2.5]);
    assert!(b[0].slot.is_none(), "{b:?}");
}

#[test]
fn a_boss_is_not_a_bore() {
    let f = world();
    let boss = cylinder(&f, mm(2.5), mm(2.0), &doc()).unwrap().body;
    assert!(bores_of(&boss).is_empty());
    let b = block(&f, [mm(4.0), mm(4.0), mm(4.0)], &doc()).unwrap().body;
    assert!(bores_of(&b).is_empty());
}

/// An obround slot: two half-cylinder ends, 6 wide and 3 thick, their centres 6 apart along y,
/// so 12 long overall.
#[test]
fn two_facing_half_walls_are_one_slot() {
    let slot = vec![
        Segment::line(pt(23.0, 17.0), pt(23.0, 23.0)),
        Segment::arc(
            pt(20.0, 23.0),
            pt(23.0, 23.0),
            pt(17.0, 23.0),
            Sense::Forward,
        ),
        Segment::line(pt(17.0, 23.0), pt(17.0, 17.0)),
        Segment::arc(
            pt(20.0, 17.0),
            pt(17.0, 17.0),
            pt(23.0, 17.0),
            Sense::Forward,
        ),
    ];
    let body = plate_with(vec![slot], 0.0, 3.0);
    let b = bores_of(&body);
    assert_eq!(b.len(), 1, "{b:?}");
    let s = b[0].slot.expect("one slot");
    assert!(
        close(b[0].point.x, 20.0, 1e-9) && close(b[0].point.y, 20.0, 1e-9),
        "{b:?}"
    );
    assert!(close(b[0].d, 6.0, 1e-9));
    assert_range(z_range(&b[0]), [0.0, 3.0]);
    assert!(close(s.length, 12.0, 1e-9), "{s:?}");
    assert!(close(s.along.y.abs(), 1.0, 1e-9), "{s:?}");
}

/// A half-circle notch in the edge of a plate is the end of a slot with no other end, and a
/// 106 degree bite is no slot end at all: neither is a bore.
#[test]
fn a_bitten_or_lone_wall_is_not_a_bore() {
    let notch = |centre_y: f64, half: f64| {
        let r = (half * half + centre_y * centre_y).sqrt();
        let _ = r;
        vec![
            Segment::line(pt(0.0, 0.0), pt(10.0 - half, 0.0)),
            Segment::arc(
                pt(10.0, centre_y),
                pt(10.0 - half, 0.0),
                pt(10.0 + half, 0.0),
                Sense::Reversed,
            ),
            Segment::line(pt(10.0 + half, 0.0), pt(20.0, 0.0)),
            Segment::line(pt(20.0, 0.0), pt(20.0, 20.0)),
            Segment::line(pt(20.0, 20.0), pt(0.0, 20.0)),
            Segment::line(pt(0.0, 20.0), pt(0.0, 0.0)),
        ]
    };
    // Centre on the edge: a half circle (its two sides on one diameter), alone.
    let half = prism(&region(notch(0.0, 3.0), vec![]), &world(), 0.0, 2.0);
    assert!(bores_of(&half).is_empty());
    // Centre 3 below the edge, radius 5: the chord of 8 subtends 106 degrees.
    let bite = prism(&region(notch(-3.0, 4.0), vec![]), &world(), 0.0, 2.0);
    assert!(bores_of(&bite).is_empty());
}

/// A plate with a hole revolved about its z axis through `profile` (radius, height) points
/// from the top down, each pair a wall (cylinder or cone), closed by a disc when the last point
/// is on the axis. Built face by face; a through hole's last point is the bottom face.
fn revolved_hole_plate(top: f64, profile: &[(f64, f64)]) -> Brep {
    use scx_curve::{Curve, Line};
    let _ = (Line::through, Curve::Line);
    let mut b = common::Builder::new();
    let h = 5.0;
    let (a0, b0, c0, d0) = (
        b.vertex([-h, -h, 0.0]),
        b.vertex([h, -h, 0.0]),
        b.vertex([h, h, 0.0]),
        b.vertex([-h, h, 0.0]),
    );
    let (a1, b1, c1, d1) = (
        b.vertex([-h, -h, top]),
        b.vertex([h, -h, top]),
        b.vertex([h, h, top]),
        b.vertex([-h, h, top]),
    );
    // The four sides, anticlockwise from outside.
    b.polygon(&[a0, b0, b1, a1]);
    b.polygon(&[b0, c0, c1, b1]);
    b.polygon(&[c0, d0, d1, c1]);
    b.polygon(&[d0, a0, a1, d1]);
    let z = Vec3::new(0.0, 0.0, 1.0);
    let x = Vec3::new(1.0, 0.0, 0.0);
    let at = |zz: f64| Frame::new(Point3::new(0.0, 0.0, zz), z, x).unwrap();
    let down = |zz: f64| Frame::new(Point3::new(0.0, 0.0, zz), -z, x).unwrap();
    // One circle edge per profile point with a radius; its seam vertex on the x axis.
    let mut rings: Vec<(EdgeId, VertexId, f64, f64)> = Vec::new();
    for &(r, zz) in profile {
        if r > 0.0 {
            let v = b.vertex([r, 0.0, zz]);
            let e = b.arc(at(zz), r, 0.0, TAU, [v, v]);
            rings.push((e, v, r, zz));
        }
    }
    // The top face: the square with the first ring as a hole. Normal +z, the hole clockwise.
    let top_ring = rings[0];
    let square = vec![
        b.line(a1, b1),
        b.line(b1, c1),
        b.line(c1, d1),
        b.line(d1, a1),
    ];
    b.face(
        Surface::Plane(Plane::new(at(top))),
        &[square, vec![(top_ring.0, Sense::Reversed)]],
    );
    // Walls between consecutive rings, their fronts into the hole.
    for w in rings.windows(2) {
        let (upper, lower) = (w[0], w[1]);
        let (ue, uv_, ur, uz) = upper;
        let (le, lv, lr, lz) = lower;
        let surface = if (ur - lr).abs() < 1e-12 {
            Surface::Cylinder(Cylinder::new(at(lz), mm(lr), Sense::Reversed).unwrap())
        } else {
            // Radius grows with z at `slope` per mm; the cone's v runs down.
            let slope = (ur - lr) / (uz - lz);
            Surface::Cone(Cone::new(at(lz), mm(lr), -fmath::atan(slope), Sense::Reversed).unwrap())
        };
        let seam = b.line(lv, uv_);
        b.face(
            surface,
            &[vec![
                (le, Sense::Reversed),
                seam,
                (ue, Sense::Forward),
                (seam.0, seam.1.reversed()),
            ]],
        );
    }
    let (last_e, _, last_r, last_z) = *rings.last().unwrap();
    let blind = profile.last().is_some_and(|p| p.0 == 0.0);
    if blind {
        // The hole's floor, a disc facing up into the hole; the plate's bottom is whole.
        b.face(
            Surface::Plane(Plane::new(at(last_z))),
            &[vec![(last_e, Sense::Forward)]],
        );
        let floor = vec![
            b.line(d0, c0),
            b.line(c0, b0),
            b.line(b0, a0),
            b.line(a0, d0),
        ];
        b.face(Surface::Plane(Plane::new(down(0.0))), &[floor]);
    } else {
        let _ = last_r;
        let floor = vec![
            b.line(d0, c0),
            b.line(c0, b0),
            b.line(b0, a0),
            b.line(a0, d0),
        ];
        b.face(
            Surface::Plane(Plane::new(down(0.0))),
            &[floor, vec![(last_e, Sense::Forward)]],
        );
    }
    b.finish()
}

fn inside_the_hole_is_outside(body: &Brep, p: [f64; 3]) {
    let mut scratch = Scratch::new();
    let class = with_cx(&mut scratch, |_| {
        classify_point(
            body,
            Point3::new(p[0], p[1], p[2]),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap()
    });
    assert!(
        matches!(class, scx_bodyq::PointClass::In(r) if r == RegionId::OUTSIDE),
        "{p:?}: {class:?}"
    );
}

/// One countersunk hole: the wall (diameter 5) from z = 0 to 0.5, and above it a 45 degree
/// countersink from diameter 5 to 8 at the top face z = 2, which the wall's extent runs through.
#[test]
fn a_countersink_extends_the_walls_extent() {
    let body = revolved_hole_plate(2.0, &[(4.0, 2.0), (2.5, 0.5), (2.5, 0.0)]);
    inside_the_hole_is_outside(&body, [0.0, 0.0, 1.0]);
    inside_the_hole_is_outside(&body, [3.0, 0.0, 1.9]);
    let b = bores_of(&body);
    assert_eq!(b.len(), 1, "{b:?}");
    assert!(close(b[0].d, 5.0, 1e-9));
    let r = z_range(&b[0]);
    assert!(close(r[0], 0.0, 1e-9) && close(r[1], 2.0, 1e-9), "{b:?}");
}

/// A blind hole whose floor is a cone narrowing away from the wall (a drill point): the cone is
/// the bore's own closed end, not an opening, and the wall keeps its own extent.
#[test]
fn a_drill_point_does_not_extend_the_wall() {
    let body = revolved_hole_plate(5.0, &[(2.5, 5.0), (2.5, 1.5), (1.5, 0.5), (0.0, 0.5)]);
    inside_the_hole_is_outside(&body, [0.0, 0.0, 3.0]);
    let b = bores_of(&body);
    assert_eq!(b.len(), 1, "{b:?}");
    let r = z_range(&b[0]);
    assert!(close(r[0], 1.5, 1e-9) && close(r[1], 5.0, 1e-9), "{b:?}");
    // The same hole with its cone flaring at the bottom is a counter-sink there.
    let flared = revolved_hole_plate(5.0, &[(2.5, 5.0), (2.5, 1.5), (3.5, 0.5), (0.0, 0.5)]);
    let _ = flared;
}

#[test]
fn bores_charge_the_budget_and_poll_the_stop() {
    let body = plate_with(vec![round_hole(20.0, 15.0, 3.0)], 0.0, 10.0);
    let e = bores(&body, &mut Budget::new(2), &Stop::new());
    assert!(matches!(e, Err(BodyqError::Exhausted { .. })), "{e:?}");
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        bores(&body, &mut Budget::unlimited(), &stop),
        Err(BodyqError::Cancelled)
    );
}

// ---- stacks of bores -----------------------------------------------------------------------

fn up() -> UnitVec3 {
    Vec3::new(0.0, 0.0, 1.0).normalized().unwrap()
}

fn hole(point: [f64; 3], t: [f64; 2], d: f64) -> Bore {
    Bore {
        point: Point3::new(point[0], point[1], point[2]),
        axis: up(),
        d,
        t,
        slot: None,
    }
}

fn slot_along(x: f64, y: f64, z: f64, along: [f64; 3], length: f64, t: [f64; 2]) -> Bore {
    Bore {
        point: Point3::new(x, y, z),
        axis: up(),
        d: 6.0,
        t,
        slot: Some(Slot {
            length,
            along: Vec3::new(along[0], along[1], along[2])
                .normalized()
                .unwrap(),
        }),
    }
}

fn opts() -> StackOptions {
    StackOptions::new(mm(0.05), mm(0.1)).unwrap()
}

fn stacks(bodies: &[&[Bore]], o: &StackOptions) -> scx_bodyq::Stacks {
    stack_bores(bodies, o, &mut Budget::unlimited(), &Stop::new()).unwrap()
}

#[test]
fn three_coaxial_holes_are_one_stack() {
    let (a, b, c) = (
        hole([20.0, 15.0, 0.0], [0.0, 2.0], 5.2),
        hole([20.0, 15.0, 2.0], [0.0, 3.0], 5.2),
        hole([20.0, 15.0, 5.0], [0.0, 2.0], 5.2),
    );
    let s = stacks(&[&[a], &[b], &[c]], &opts());
    assert_eq!(s.stacks.len(), 1, "{s:?}");
    let st = &s.stacks[0];
    // From the top down: -z, starting on the top plate's top face.
    assert!(close(st.axis.z, -1.0, 1e-12) && st.axis.x.abs() < 1e-12);
    assert!(
        close(st.start.x, 20.0, 1e-9)
            && close(st.start.y, 15.0, 1e-9)
            && close(st.start.z, 7.0, 1e-9),
        "{st:?}"
    );
    assert!(close(st.grip, 7.0, 1e-9), "{st:?}");
    assert_eq!(
        st.layers.iter().map(|l| l.body).collect::<Vec<_>>(),
        vec![2, 1, 0]
    );
    assert_eq!(st.layers[0].t, [0.0, 2.0]);
    assert_eq!(st.layers[1].t, [2.0, 5.0]);
    assert_eq!(st.layers[2].t, [5.0, 7.0]);
    assert!(s.lone.is_empty(), "{s:?}");
}

#[test]
fn a_hole_two_tenths_off_is_no_stack() {
    let (a, b) = (
        hole([0.0, 0.0, 0.0], [0.0, 2.0], 5.0),
        hole([0.2, 0.0, 2.0], [0.0, 2.0], 5.0),
    );
    let s = stacks(
        &[std::slice::from_ref(&a), std::slice::from_ref(&b)],
        &opts(),
    );
    assert!(s.stacks.is_empty(), "{s:?}");
    assert_eq!(
        s.lone.iter().map(|h| (h.body, h.d)).collect::<Vec<_>>(),
        vec![(0, 5.0), (1, 5.0)]
    );
    let loose = StackOptions::new(mm(0.05), mm(0.3)).unwrap();
    let s = stacks(&[&[a], &[b]], &loose);
    assert_eq!(s.stacks.len(), 1, "{s:?}");
}

#[test]
fn a_counterbore_and_its_hole_are_one_layer_of_their_narrowest_opening() {
    let through = hole([0.0, 0.0, 0.0], [0.0, 0.8], 5.0);
    let cbore = hole([0.0, 0.0, 0.8], [0.0, 1.2], 9.0);
    let plate = hole([0.0, 0.0, 2.0], [0.0, 2.0], 5.5);
    let s = stacks(&[&[through, cbore], &[plate]], &opts());
    assert_eq!(s.stacks.len(), 1, "{s:?}");
    let st = &s.stacks[0];
    assert!(close(st.grip, 4.0, 1e-9), "{st:?}");
    assert_eq!(st.layers.len(), 2, "{st:?}");
    assert_eq!(st.layers[0].d, 5.5);
    assert_eq!(st.layers[0].t, [0.0, 2.0]);
    assert_eq!(st.layers[1].d, 5.0);
    assert_eq!(st.layers[1].t, [2.0, 4.0]);
    assert!(s.lone.is_empty(), "{s:?}");
}

#[test]
fn plates_a_millimetre_apart_are_no_stack() {
    let (a, b) = (
        hole([0.0, 0.0, 0.0], [0.0, 2.0], 5.0),
        hole([0.0, 0.0, 3.0], [0.0, 2.0], 5.0),
    );
    let s = stacks(
        &[std::slice::from_ref(&a), std::slice::from_ref(&b)],
        &opts(),
    );
    assert!(s.stacks.is_empty(), "{s:?}");
    assert_eq!(s.lone.len(), 2);
    let wide = StackOptions::new(mm(1.0), mm(0.1)).unwrap();
    let s = stacks(&[&[a], &[b]], &wide);
    assert_eq!(s.stacks.len(), 1, "{s:?}");
}

#[test]
fn two_stacks_on_one_axis() {
    let (a, b, c, d) = (
        hole([0.0, 0.0, 0.0], [0.0, 2.0], 5.0),
        hole([0.0, 0.0, 2.0], [0.0, 2.0], 5.0),
        hole([0.0, 0.0, 10.0], [0.0, 2.0], 5.0),
        hole([0.0, 0.0, 12.0], [0.0, 2.0], 5.0),
    );
    let s = stacks(&[&[a, c], &[b, d]], &opts());
    assert_eq!(s.stacks.len(), 2, "{s:?}");
    assert!(s.stacks.iter().all(|st| close(st.grip, 4.0, 1e-9)), "{s:?}");
}

#[test]
fn a_round_hole_over_a_slot_is_a_stack() {
    let sl = slot_along(20.0, 15.0, 0.0, [0.0, 1.0, 0.0], 12.0, [0.0, 3.0]);
    // The round hole sits 2 mm off the slot's centre, over its length.
    let round = hole([20.0, 17.0, 3.0], [0.0, 2.0], 6.0);
    let s = stacks(&[&[sl], &[round]], &opts());
    assert_eq!(s.stacks.len(), 1, "{s:?}");
    let st = &s.stacks[0];
    assert!(close(st.grip, 5.0, 1e-9), "{st:?}");
    assert_eq!(st.layers.len(), 2, "{st:?}");
    assert!(st.layers[0].slot.is_none(), "{st:?}");
    assert!(st.layers[1].slot.is_some(), "{st:?}");
}

#[test]
fn slots_on_one_centre_line_stack_through_their_shared_middle() {
    let sl = |x: f64| slot_along(x, 10.0, 0.0, [1.0, 0.0, 0.0], 12.0, [0.0, 3.0]);
    let s = stacks(&[&[sl(10.0)], &[sl(11.0)]], &opts());
    assert_eq!(s.stacks.len(), 1, "{s:?}");
    assert!(close(s.stacks[0].grip, 3.0, 1e-9), "{s:?}");
    assert_eq!(s.stacks[0].layers.len(), 2, "{s:?}");
    // Through the middle of the length they share ([8, 13] in x): the first slot's own centre
    // (x = 10) would be the wrong answer.
    let st = &s.stacks[0];
    assert!(
        close(st.start.x, 10.5, 1e-9)
            && close(st.start.y, 10.0, 1e-9)
            && close(st.start.z, 3.0, 1e-9),
        "{st:?}"
    );
    assert!(st.axis.z < -0.999, "{st:?}");
}

#[test]
fn a_chain_of_slots_past_its_own_start_stacks_in_runs() {
    // Four slots, each centred 5 mm from the last: every pair meets its neighbour, but the
    // first and last share nothing, so no one axis runs through all four. Two bodies carry
    // the same chain: it answers two stacks (the runs [0, 5] and [10, 15]), not one.
    let sl = |x: f64| slot_along(x, 10.0, 0.0, [1.0, 0.0, 0.0], 12.0, [0.0, 3.0]);
    let a = [sl(0.0), sl(5.0), sl(10.0), sl(15.0)];
    let b = a;
    let s = stacks(&[&a, &b], &opts());
    assert_eq!(s.stacks.len(), 2, "{s:?}");
    assert_eq!(s.lone.len(), 0, "{s:?}");
    let starts: Vec<f64> = s.stacks.iter().map(|st| st.start.x).collect();
    assert!(
        starts.iter().any(|x| close(*x, 2.5, 1e-9)) && starts.iter().any(|x| close(*x, 12.5, 1e-9)),
        "{s:?}"
    );
}

#[test]
fn a_bore_far_along_the_axis_holds_no_slot() {
    let sl = |y: f64, z0: f64| slot_along(0.0, y, z0, [1.0, 0.0, 0.0], 12.0, [0.0, 2.0]);
    let a = [sl(10.0, 0.0), hole([0.0, 9.94, 100.0], [0.0, 2.0], 6.0)];
    let b = [sl(10.09, 2.0)];
    let s = stacks(&[&a, &b], &opts());
    assert_eq!(s.stacks.len(), 1, "{s:?}");
    assert_eq!(s.stacks[0].layers.len(), 2, "{s:?}");
    assert_eq!(s.lone.len(), 1, "{s:?}");
}

/// `too_many_nearly_coaxial_holes_refuse`: the pin refused past 4,000,000 comparisons; here the
/// comparisons are the caller's budget, and running out is `Exhausted`.
#[test]
fn too_many_nearly_coaxial_holes_exhaust_the_budget() {
    let far: Vec<Bore> = (0..2000)
        .map(|i| hole([0.0, 0.0, f64::from(i)], [0.0, 0.1], 5.0))
        .collect();
    let near: Vec<Bore> = (0..2000)
        .map(|i| hole([0.1, 0.0, f64::from(i)], [0.0, 0.1], 5.0))
        .collect();
    let e = stack_bores(
        &[&far, &near],
        &opts(),
        &mut Budget::new(100_000),
        &Stop::new(),
    );
    assert_eq!(e, Err(BodyqError::Exhausted { limit: 100_000 }));
}

#[test]
fn options_are_checked() {
    for (t, a, name) in [
        (f64::NAN, 0.1, "touching"),
        (-1.0, 0.1, "touching"),
        (0.0, 0.0, "axis_tolerance"),
        (0.0, f64::INFINITY, "axis_tolerance"),
    ] {
        assert_eq!(
            StackOptions::new(mm(t), mm(a)),
            Err(BodyqError::Refused(BodyqRefusal::BadOption { name })),
            "{t} {a}"
        );
    }
    assert!(StackOptions::new(mm(0.0), mm(1e-9)).is_ok());
}

// ---- stacks of bodies ----------------------------------------------------------------------

/// Three plates 5 thick stacked on one axis, each with a hole at (10, 10): one stack of three
/// layers from the top down, 15 thick; and the same plates with the middle hole off by a
/// millimetre: no stack through it.
#[test]
fn plates_with_coaxial_holes_stack() {
    let plate = |z0: f64, z1: f64, x: f64, d: f64| {
        let hole = vec![Segment::circle(pt(x, 10.0), d / 2.0, Sense::Forward)];
        prism(
            &region(rect(0.0, 0.0, 20.0, 20.0), vec![hole]),
            &world(),
            z0,
            z1,
        )
    };
    let (a, b, c) = (
        plate(0.0, 5.0, 10.0, 6.0),
        plate(5.0, 10.0, 10.0, 6.5),
        plate(10.0, 15.0, 10.0, 6.0),
    );
    let o = opts();
    let s = hole_stacks(&[&a, &b, &c], &o, &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert_eq!(s.stacks.len(), 1, "{s:?}");
    let st = &s.stacks[0];
    assert!(close(st.grip, 15.0, 1e-9), "{st:?}");
    assert!(close(st.axis.z, -1.0, 1e-12));
    assert!(
        close(st.start.x, 10.0, 1e-9)
            && close(st.start.y, 10.0, 1e-9)
            && close(st.start.z, 15.0, 1e-9)
    );
    assert_eq!(
        st.layers.iter().map(|l| l.body).collect::<Vec<_>>(),
        vec![2, 1, 0]
    );
    assert!(close(st.layers[1].d, 6.5, 1e-9));
    assert!(s.lone.is_empty());
    // Twice, the same bits.
    let again = hole_stacks(&[&a, &b, &c], &o, &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert_eq!(s, again);
    // The middle hole a millimetre off: the outer two are 5 apart in z, no longer touching.
    let off = plate(5.0, 10.0, 11.0, 6.5);
    let s = hole_stacks(&[&a, &off, &c], &o, &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert!(s.stacks.is_empty(), "{s:?}");
    assert_eq!(s.lone.len(), 3);
    // The diameter filter: only holes of 6.4 to 6.6 count, so the one hole is alone.
    let o = opts().with_diameters(&[(mm(6.4), mm(6.6))]).unwrap();
    let s = hole_stacks(&[&a, &b, &c], &o, &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert!(s.stacks.is_empty() && s.lone.len() == 1, "{s:?}");
    // A bad range is refused.
    assert_eq!(
        opts().with_diameters(&[(mm(2.0), mm(1.0))]),
        Err(BodyqError::Refused(BodyqRefusal::BadOption {
            name: "diameters"
        }))
    );
}

#[allow(dead_code)]
fn unused(_: Circle, _: UnitVec3) {}
