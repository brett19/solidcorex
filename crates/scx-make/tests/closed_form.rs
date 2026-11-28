// Written for solidcorex. Apache-2.0.
//! The closed-form make table (research 75 MK16): `extrude`, `revolve` and `sweep` judged
//! against closed forms, each case asserting volume, area, first moment where a truth has
//! one, face, edge and vertex counts, and the check contract.
//!
//! Truths come from three places that share no code with the constructors:
//!
//! - T9 (`scx_testkit::solid`, research 72 §2.4), the closed-form volume, area and moments of
//!   primitives, tori and extrusions of line-and-arc profiles, carried to the case's frame;
//! - Pappus: a revolved profile of area `A` with centroid `x` from the axis sweeps `A x t`
//!   (`t` the turn in radians) and its rim of moment `int x ds` sweeps `t int x ds`, a partial
//!   turn adding the two caps; here `A` and `x` are T9's extrusion of the profile and
//!   `int x ds` is a few lines of arc and chord geometry in this file;
//! - the mitre: a profile whose area and boundary are centred on the path keeps its area and
//!   perimeter times the path's length through any corner between lines, so a mitred tube is
//!   T9's straight extrusion of the profile over the whole length.
//!
//! The first table is the pin's 26-case ops table (`solidcore-ops/tests/sweep.rs`) of the
//! cases the constructors here build. Not ported, with what each waits on: the drafted
//! extrusions (5 cases: a draft is `scx-taper`'s, step 8), the lofts (5: MK10 to MK12), the
//! swept stations with scale (2: `MK7b`'s laws) and the screw sweeps (2: `MK6`). The pin's
//! `sweep_stations` helix truths are `tests/frames.rs` (`MK7a`) and its `sweep_cornered`
//! truths are the cornered table here.
//!
//! The one call to `sweep` is [`sweep_along`], so a signature change touches one line.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::cast_possible_wrap
)]

use core::f64::consts::{FRAC_PI_2, PI, TAU};

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, EdgeShape, FaceId, FaceSide};
use scx_make::{
    ExtrudeFaces, Made, Path, PathFrame, Piece, RevolveFaces, Shape, Span, SweepFaces, Turn,
    extrude, revolve, sheet, sweep,
};
use scx_mass::{Mass, MassSpec, RelAccuracy, mass};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{Region, Segment};
use scx_surface::Surface;
use scx_testkit::analytic::Frame as TruthFrame;
use scx_testkit::bodies::check::{Tolerance, violations};
use scx_testkit::profile::Seg;
use scx_testkit::solid::{self, Solid};

// ---------------------------------------------------------------------------------------
// Building

fn doc() -> Document {
    Document::default()
}

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn v3(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

fn frame(origin: Point3, z: Vec3, x: Vec3) -> Frame {
    Frame::new(origin, z, x).unwrap()
}

/// The frame at the origin along `+z`, its x axis `+x`.
fn plain() -> Frame {
    frame(Point3::ORIGIN, v3(0.0, 0.0, 1.0), v3(1.0, 0.0, 0.0))
}

/// A frame far from the origin, its axes along no coordinate direction.
fn tilted() -> Frame {
    frame(
        p3(1250.0, -2125.0, 600.0),
        v3(1.0, 2.0, 3.0),
        v3(-3.0, 0.5, 1.0),
    )
}

fn segment(s: &Seg) -> Segment {
    match *s {
        Seg::Line { start, end } => Segment::line(Point2::from(start), Point2::from(end)),
        Seg::Arc {
            centre,
            start,
            end,
            ccw,
        } => {
            let sense = if ccw { Sense::Forward } else { Sense::Reversed };
            if start == end {
                let r = fmath::hypot(start[0] - centre[0], start[1] - centre[1]);
                Segment::circle(Point2::from(centre), r, sense)
            } else {
                Segment::arc(
                    Point2::from(centre),
                    Point2::from(start),
                    Point2::from(end),
                    sense,
                )
            }
        }
    }
}

fn region(loops: &[Vec<Seg>]) -> Region {
    Region::new(
        loops[0].iter().map(segment).collect(),
        loops[1..]
            .iter()
            .map(|h| h.iter().map(segment).collect())
            .collect(),
        doc().resolution(),
    )
    .unwrap()
}

fn ln(a: [f64; 2], b: [f64; 2]) -> Seg {
    Seg::Line { start: a, end: b }
}

fn arc(centre: [f64; 2], start: [f64; 2], end: [f64; 2], ccw: bool) -> Seg {
    Seg::Arc {
        centre,
        start,
        end,
        ccw,
    }
}

fn circle(c: [f64; 2], r: f64) -> Seg {
    let p = [c[0] + r, c[1]];
    arc(c, p, p, true)
}

fn polygon(p: &[[f64; 2]]) -> Vec<Seg> {
    p.iter()
        .zip(p.iter().cycle().skip(1))
        .map(|(&a, &b)| ln(a, b))
        .collect()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Seg> {
    polygon(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
}

/// Half sides `a`, `b`, corner radius `r`, counter-clockwise, about `(cx, cy)`.
fn rounded(cx: f64, cy: f64, a: f64, b: f64, r: f64) -> Vec<Seg> {
    let q = |p: [f64; 2]| [cx + p[0], cy + p[1]];
    vec![
        ln(q([-a + r, -b]), q([a - r, -b])),
        arc(q([a - r, -b + r]), q([a - r, -b]), q([a, -b + r]), true),
        ln(q([a, -b + r]), q([a, b - r])),
        arc(q([a - r, b - r]), q([a, b - r]), q([a - r, b]), true),
        ln(q([a - r, b]), q([-a + r, b])),
        arc(q([-a + r, b - r]), q([-a + r, b]), q([-a, b - r]), true),
        ln(q([-a, b - r]), q([-a, -b + r])),
        arc(q([-a + r, -b + r]), q([-a, -b + r]), q([-a + r, -b]), true),
    ]
}

fn extruded(loops: &[Vec<Seg>], frame: &Frame, span: [f64; 2]) -> Made<ExtrudeFaces> {
    extrude(
        &region(loops),
        frame,
        Span::new(Length::from_mm(span[0]), Length::from_mm(span[1])),
        &doc(),
    )
    .unwrap()
}

fn revolved(loops: &[Vec<Seg>], at: &Frame, turn: Turn) -> Made<RevolveFaces> {
    revolve(&region(loops), at, turn, &doc()).unwrap()
}

/// The one place `sweep` is called (`MK7b` changes its signature).
fn sweep_along(loops: &[Vec<Seg>], at: &Frame, pieces: &[Piece]) -> Made<SweepFaces> {
    let path = Path::new(pieces, &doc()).unwrap();
    sweep(
        &region(loops),
        at,
        &path,
        &PathFrame::Shaped(Shape::PLAIN),
        &doc(),
    )
    .unwrap()
}

fn line(a: Point3, b: Point3) -> Piece {
    Piece::Line { from: a, to: b }
}

fn arc_piece(centre: Point3, axis: Vec3, start: Point3, angle: f64) -> Piece {
    Piece::Arc {
        centre,
        axis: axis.normalized().unwrap(),
        start,
        angle,
    }
}

// ---------------------------------------------------------------------------------------
// Judging

/// What a case must come to.
struct Want {
    volume: f64,
    area: f64,
    /// `int p dV` about the world origin, where the truth has it.
    first: Option<[f64; 3]>,
    /// Where the measure is taken about (a body far from the origin is measured about its frame).
    about: Point3,
    /// Faces, curve edges (collapsed ones apart) and vertices.
    counts: [usize; 3],
    /// Euler's `V - E + F - (L - F)` over the curve edges, summed over the shells.
    chi: i64,
    /// Regions: the outside and the solid, and one void per hollow.
    regions: usize,
}

impl Want {
    fn of(t: &Solid, counts: [usize; 3], chi: i64) -> Self {
        Self {
            volume: t.volume(),
            area: t.area,
            first: Some(t.moments.first),
            about: Point3::ORIGIN,
            counts,
            chi,
            regions: 2,
        }
    }

    fn pappus(volume: f64, area: f64, counts: [usize; 3], chi: i64, regions: usize) -> Self {
        Self {
            volume,
            area,
            first: None,
            about: Point3::ORIGIN,
            counts,
            chi,
            regions,
        }
    }
}

fn measure(b: &Brep, about: Point3) -> Mass {
    let spec = MassSpec::new(RelAccuracy::new(1e-12).unwrap()).with_about(about);
    mass(
        b,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

fn within(got: f64, error: f64, want: f64, scale: f64, what: &str) {
    let allowed = 1e-9 * scale;
    assert!(
        (got - want).abs() <= allowed && error <= allowed,
        "{what}: {got:e} (estimated error {error:e}) vs {want:e}, allowed {allowed:e}"
    );
}

/// The case against its truth: structure, the check contract, counts, volume, area and
/// first moment.
fn judge(b: &Brep, want: &Want, name: &str) {
    // Structure: every curve edge with two fins of opposite senses, every collapsed one with
    // one, each face's front outside the solid and its back in it.
    assert_eq!(b.regions().len(), want.regions, "{name}: regions");
    // A void is a region and a shell of its own, with a second shell of the solid behind it.
    assert_eq!(b.shells().len(), 2 * want.regions - 2, "{name}: shells");
    let mut collapsed = 0;
    for e in b.edges() {
        let f: Vec<_> = e.fins().collect();
        if *e.shape() == EdgeShape::Collapsed {
            collapsed += 1;
            assert_eq!(f.len(), 1, "{name}: {:?}", e.id());
        } else {
            assert_eq!(f.len(), 2, "{name}: {:?}", e.id());
            assert_ne!(f[0].sense, f[1].sense, "{name}: {:?}", e.id());
        }
    }
    let mut loops = 0;
    for face in b.faces() {
        assert!(!face.shell(FaceSide::Front).region().is_solid(), "{name}");
        assert!(face.shell(FaceSide::Back).region().is_solid(), "{name}");
        loops += face.loops().len();
        for u in face.uses() {
            assert!(u.bound().is_none(), "{name}: {:?}", face.id());
        }
    }
    let (f, e, v) = (
        b.faces().len(),
        b.edges().len() - collapsed,
        b.vertices().len(),
    );
    assert_eq!([f, e, v], want.counts, "{name}: faces, edges, vertices");
    let euler = v as i64 - e as i64 + f as i64 - (loops as i64 - f as i64);
    assert_eq!(euler, want.chi, "{name}: V - E + F - (L - F)");
    // The check contract.
    let bad = violations(&b.edit(), Tolerance::default());
    assert!(bad.is_empty(), "{name}: {bad:?}");
    // The measures.
    let m = measure(b, want.about);
    within(
        m.volume,
        m.error.volume,
        want.volume,
        want.volume,
        &format!("{name}: volume"),
    );
    within(
        m.area,
        m.error.area,
        want.area,
        want.area,
        &format!("{name}: area"),
    );
    if let Some(first) = want.first {
        let o = want.about;
        let first = [
            first[0] - want.volume * o.x,
            first[1] - want.volume * o.y,
            first[2] - want.volume * o.z,
        ];
        let size = fmath::cbrt(want.volume.abs());
        let scale = first.iter().map(|c| c.abs()).fold(0.0, f64::max) + want.volume * size;
        let got = [m.first.x, m.first.y, m.first.z];
        let err = [m.error.first.x, m.error.first.y, m.error.first.z];
        for i in 0..3 {
            within(
                got[i],
                err[i],
                first[i],
                scale,
                &format!("{name}: first[{i}]"),
            );
        }
    }
}

/// T9's frame for a revolved body: the truth's axis `z` is the revolve's axis `y`, its `y` the
/// world's `-z`, so a sector's angle runs as `revolve`'s turn does (right-handed about `y`).
fn about_y() -> TruthFrame {
    TruthFrame {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 0.0, -1.0],
        z: [0.0, 1.0, 0.0],
    }
}

fn truth_frame(f: &Frame) -> TruthFrame {
    let a = |v: Vec3| [v.x, v.y, v.z];
    let o = f.origin();
    TruthFrame {
        origin: [o.x, o.y, o.z],
        x: a(*f.x()),
        y: a(*f.y()),
        z: a(*f.z()),
    }
}

fn kind(b: &Brep, f: FaceId) -> &'static str {
    match b.face(f).unwrap().surface() {
        Surface::Plane(_) => "plane",
        Surface::Cylinder(_) => "cylinder",
        Surface::Cone(_) => "cone",
        Surface::Sphere(_) => "sphere",
        Surface::Torus(_) => "torus",
        _ => "other",
    }
}

// ---------------------------------------------------------------------------------------
// The ops table

/// The pin's table: rectangle and circle extrudes, the circle drawn in pieces, the half disc
/// with a split arc; the rectangle revolved whole and by a quarter; a circle along two lines
/// end to end, along a quarter arc and round a bend; a rectangle round a closed rounded
/// rectangle; and a planar face with a hole. Against T9 where it has the body, else a closed
/// form stated beside the case.
#[test]
fn the_ops_table_of_the_pin_against_closed_forms() {
    // V = A h for a 30 x 20 rectangle and a circle of radius 5, both 10 high: two caps and one
    // wall per segment, the vertices and rim edges of both caps and the rails.
    let rect30 = rect(0.0, 0.0, 30.0, 20.0);
    let t = solid::extrusion(core::slice::from_ref(&rect30), 10.0).unwrap();
    assert_eq!(t.volume(), 6000.0);
    let m = extruded(&[rect30], &plain(), [0.0, 10.0]);
    judge(&m.body, &Want::of(&t, [6, 12, 8], 2), "extrude_rectangle");

    let disc = vec![circle([0.0, 0.0], 5.0)];
    let t = solid::cylinder(5.0, 10.0).unwrap();
    assert!((t.volume() - 250.0 * PI).abs() < 1e-9);
    let m = extruded(&[disc], &plain(), [0.0, 10.0]);
    judge(&m.body, &Want::of(&t, [3, 3, 2], 2), "extrude_circle");

    // The same circle drawn as two halves, and as a quarter and three quarters, swept as
    // given: a wall each. Two vertices where the arcs meet, in each cap.
    let halves = vec![vec![
        arc([0.0, 0.0], [5.0, 0.0], [-5.0, 0.0], true),
        arc([0.0, 0.0], [-5.0, 0.0], [5.0, 0.0], true),
    ]];
    let m = extruded(&halves, &plain(), [0.0, 10.0]);
    judge(
        &m.body,
        &Want::of(&t, [4, 6, 4], 2),
        "extrude_circle_in_two_halves",
    );
    let quarters = vec![vec![
        arc([0.0, 0.0], [5.0, 0.0], [0.0, 5.0], true),
        arc([0.0, 0.0], [0.0, 5.0], [5.0, 0.0], true),
    ]];
    let m = extruded(&quarters, &plain(), [0.0, 10.0]);
    judge(
        &m.body,
        &Want::of(&t, [4, 6, 4], 2),
        "extrude_circle_in_a_quarter_and_the_rest",
    );

    // A half disc whose arc is drawn in two quarters, closed by its diameter: V = pi r^2 h / 2.
    let r = 5.0;
    let s = r;
    let half = vec![vec![
        arc([0.0, 0.0], [r, 0.0], [0.0, s], true),
        arc([0.0, 0.0], [0.0, s], [-r, 0.0], true),
        ln([-r, 0.0], [r, 0.0]),
    ]];
    let t = solid::extrusion(&half, 10.0).unwrap();
    assert!((t.volume() - 125.0 * PI).abs() < 1e-9);
    let m = extruded(&half, &plain(), [0.0, 10.0]);
    judge(
        &m.body,
        &Want::of(&t, [5, 9, 6], 2),
        "extrude_half_disc_split_arc",
    );

    // Pappus: A theta x = 12 theta 6.5 for the 3 x 4 section at radius 5 to 8, whole and by a
    // quarter. T9's torus is the same pappus for a circle; the rectangle's are by hand: the
    // walls are the cylinders at 5 and 8 (height 4), the annuli (width 3) at radius 6.5.
    let ring = rect(5.0, 0.0, 8.0, 4.0);
    let whole = revolved(core::slice::from_ref(&ring), &plain(), Turn::Full);
    let area = TAU * (5.0 * 4.0 + 8.0 * 4.0 + 2.0 * 6.5 * 3.0);
    judge(
        &whole.body,
        &Want::pappus(12.0 * TAU * 6.5, area, [4, 6, 4], 0, 2),
        "revolve_rectangle_full",
    );
    let quarter = revolved(
        &[ring],
        &plain(),
        Turn::Partial {
            from: 0.0,
            to: FRAC_PI_2,
        },
    );
    judge(
        &quarter.body,
        &Want::pappus(
            12.0 * FRAC_PI_2 * 6.5,
            area / 4.0 + 2.0 * 12.0,
            [6, 12, 8],
            2,
            2,
        ),
        "revolve_rectangle_quarter",
    );

    // pi r^2 L along two lines end to end (the pin's cylinder from two lines). The frame
    // at the first piece's start along `+z`; the section between them is no corner.
    let rod = [vec![circle([0.0, 0.0], 2.0)]];
    let pieces = [
        line(Point3::ORIGIN, p3(0.0, 0.0, 6.0)),
        line(p3(0.0, 0.0, 6.0), p3(0.0, 0.0, 10.0)),
    ];
    let m = sweep_along(&rod, &plain(), &pieces);
    let t = solid::cylinder(2.0, 10.0).unwrap();
    judge(
        &m.body,
        &Want::of(&t, [4, 5, 3], 2),
        "sweep_circle_straight",
    );

    // A quarter torus: pi r^2 R theta; T9's sector about the origin from the +x axis, the
    // path's start at (R, 0, 0) heading +y, the profile's frame x radial.
    let at = frame(p3(10.0, 0.0, 0.0), v3(0.0, 1.0, 0.0), v3(1.0, 0.0, 0.0));
    let unit_rod = [vec![circle([0.0, 0.0], 1.0)]];
    let m = sweep_along(
        &unit_rod,
        &at,
        &[arc_piece(
            Point3::ORIGIN,
            v3(0.0, 0.0, 1.0),
            p3(10.0, 0.0, 0.0),
            FRAC_PI_2,
        )],
    );
    let t = solid::torus_sector(10.0, 1.0, FRAC_PI_2).unwrap();
    assert!((t.volume() - PI * 10.0 * FRAC_PI_2).abs() < 1e-9);
    judge(
        &m.body,
        &Want::of(&t, [3, 3, 2], 2),
        "sweep_circle_along_an_arc",
    );

    // A line then a tangent quarter arc: a cylinder into a quarter torus, pi r^2 times the
    // path's length, 10 + 10 pi / 2; T9's two bodies added (the cylinder along z from 0, the
    // sector about the arc's centre (10, 0, 10) with its x axis to -x, its y to +z, its z to +y).
    let m = sweep_along(
        &unit_rod,
        &plain(),
        &[
            line(Point3::ORIGIN, p3(0.0, 0.0, 10.0)),
            arc_piece(
                p3(10.0, 0.0, 10.0),
                v3(0.0, 1.0, 0.0),
                p3(0.0, 0.0, 10.0),
                FRAC_PI_2,
            ),
        ],
    );
    let cyl = solid::cylinder(1.0, 10.0).unwrap();
    let tor = solid::torus_sector(10.0, 1.0, FRAC_PI_2)
        .unwrap()
        .place(&TruthFrame {
            origin: [10.0, 0.0, 10.0],
            x: [-1.0, 0.0, 0.0],
            y: [0.0, 0.0, 1.0],
            z: [0.0, 1.0, 0.0],
        });
    // The two disjoint truths joined at their shared disc: the volumes and first moments
    // add, the area is both walls and the two outer caps (each solid's other disc cancels).
    let want = Want {
        volume: cyl.volume() + tor.volume(),
        area: cyl.area + tor.area - 2.0 * PI,
        first: Some(cyl.moments.plus(&tor.moments).first),
        about: Point3::ORIGIN,
        counts: [4, 5, 3],
        chi: 2,
        regions: 2,
    };
    assert!((want.volume - PI * (10.0 + 10.0 * FRAC_PI_2)).abs() < 1e-9);
    judge(&m.body, &want, "sweep_along_a_bent_path");

    // A 4 x 3 rectangle round a closed 60 x 40 rounded rectangle (corner radius 8) from the
    // middle of its bottom line: no caps. Pappus: V = A L with A = 12 and
    // L = 2 (44 + 24) + 4 (pi / 2) 8; the rim is centred on the path, so the area is the
    // perimeter 14 times the length.
    let corner = |c: [f64; 2], start: [f64; 2]| {
        arc_piece(
            p3(c[0], c[1], 0.0),
            v3(0.0, 0.0, 1.0),
            p3(start[0], start[1], 0.0),
            FRAC_PI_2,
        )
    };
    let path = [
        line(p3(0.0, -20.0, 0.0), p3(22.0, -20.0, 0.0)),
        corner([22.0, -12.0], [22.0, -20.0]),
        line(p3(30.0, -12.0, 0.0), p3(30.0, 12.0, 0.0)),
        corner([22.0, 12.0], [30.0, 12.0]),
        line(p3(22.0, 20.0, 0.0), p3(-22.0, 20.0, 0.0)),
        corner([-22.0, 12.0], [-22.0, 20.0]),
        line(p3(-30.0, 12.0, 0.0), p3(-30.0, -12.0, 0.0)),
        corner([-22.0, -12.0], [-30.0, -12.0]),
        line(p3(-22.0, -20.0, 0.0), p3(0.0, -20.0, 0.0)),
    ];
    let along_x = frame(p3(0.0, -20.0, 0.0), v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0));
    let m = sweep_along(&[rect(-2.0, -1.5, 2.0, 1.5)], &along_x, &path);
    let length = 2.0 * (44.0 + 24.0) + 16.0 * PI;
    // 9 pieces (the first line is split at the start) of 4 walls, 8 shared sections of 4
    // vertices; the closing section is the first.
    judge(
        &m.body,
        &Want::pappus(12.0 * length, 14.0 * length, [36, 72, 36], 0, 2),
        "sweep_round_a_closed_path",
    );

    // A square of 10 with a hole of radius 2: the planar face of area 100 - 4 pi, one face with
    // two loops.
    let sq = rect(0.0, 0.0, 10.0, 10.0);
    let hole = vec![arc([5.0, 5.0], [7.0, 5.0], [7.0, 5.0], false)];
    let m = sheet(&[region(&[sq, hole])], &plain(), &doc()).unwrap();
    let mass = measure(&m.body, Point3::ORIGIN);
    assert_eq!(m.body.faces().len(), 1);
    assert_eq!(m.body.faces().next().unwrap().loops().len(), 2);
    // A sheet has two sides: the mass area counts the face once.
    within(
        mass.area,
        mass.error.area,
        100.0 - 4.0 * PI,
        100.0,
        "planar_faces_with_a_hole",
    );
    let bad = violations(&m.body.edit(), Tolerance::default());
    assert!(bad.is_empty(), "{bad:?}");
}

// ---------------------------------------------------------------------------------------
// Revolve against T9's bodies

/// Revolving a profile on or off the axis gives T9's primitive: its volume, area and first
/// moment, the counts of a body with poles (a collapsed edge each, not counted as an edge).
fn primitives(frame: &Frame) {
    let at = about_y();
    let truth = |t: Solid| t.place(&at).place(&truth_frame(frame));
    let want = |t: &Solid, counts, chi| Want {
        about: frame.origin(),
        ..Want::of(t, counts, chi)
    };
    let whole = Turn::Full;

    // A cylinder: radius 3, height 5.
    let m = revolved(&[rect(0.0, 0.0, 3.0, 5.0)], frame, whole);
    let t = truth(solid::cylinder(3.0, 5.0).unwrap());
    judge(&m.body, &want(&t, [3, 3, 2], 2), "cylinder");

    // A cone to an apex, and a frustum.
    let m = revolved(
        &[polygon(&[[0.0, 0.0], [3.0, 0.0], [0.0, 5.0]])],
        frame,
        whole,
    );
    let t = truth(solid::cone(3.0, 0.0, 5.0).unwrap());
    judge(&m.body, &want(&t, [2, 2, 2], 2), "cone");
    let m = revolved(
        &[polygon(&[[0.0, 0.0], [3.0, 0.0], [2.0, 5.0], [0.0, 5.0]])],
        frame,
        whole,
    );
    let t = truth(solid::cone(3.0, 2.0, 5.0).unwrap());
    judge(&m.body, &want(&t, [3, 3, 2], 2), "frustum");

    // A sphere from a half disc on the axis.
    let r = 4.0;
    let half = vec![vec![
        arc([0.0, 0.0], [0.0, -r], [0.0, r], true),
        ln([0.0, r], [0.0, -r]),
    ]];
    let m = revolved(&half, frame, whole);
    let t = truth(solid::sphere(r).unwrap());
    judge(&m.body, &want(&t, [1, 1, 2], 2), "sphere");

    // A ring torus from a circle, and its half and quarter sectors by partial turns.
    let tube = vec![vec![circle([5.0, 0.0], 2.0)]];
    let m = revolved(&tube, frame, whole);
    let t = truth(solid::torus(5.0, 2.0).unwrap());
    judge(&m.body, &want(&t, [1, 2, 1], 0), "torus");
    for turn in [FRAC_PI_2, PI, 1.5 * PI] {
        let m = revolved(
            &tube,
            frame,
            Turn::Partial {
                from: 0.0,
                to: turn,
            },
        );
        let t = truth(solid::torus_sector(5.0, 2.0, turn).unwrap());
        judge(
            &m.body,
            &want(&t, [3, 3, 2], 2),
            &format!("torus sector {turn}"),
        );
    }
}

#[test]
fn revolved_profiles_are_t9_s_primitives() {
    primitives(&plain());
}

#[test]
fn revolved_profiles_are_t9_s_primitives_in_a_tilted_frame() {
    primitives(&tilted());
}

// ---------------------------------------------------------------------------------------
// Pappus

/// `int x ds` over the loops' rims: a chord's length times its mean `x`, an arc's
/// `r (cx t + r (sin a1 - sin a0))` from its start angle `a0` through `t` counter-clockwise.
fn rim_moment(loops: &[Vec<Seg>]) -> f64 {
    loops
        .iter()
        .flatten()
        .map(|s| match *s {
            Seg::Line { start, end } => {
                fmath::hypot(end[0] - start[0], end[1] - start[1]) * 0.5 * (start[0] + end[0])
            }
            Seg::Arc {
                centre,
                start,
                end,
                ccw,
            } => {
                let r = fmath::hypot(start[0] - centre[0], start[1] - centre[1]);
                // Counter-clockwise from `a` to `b`, the sweep in (0, tau], a circle whole.
                let (a, b) = if ccw { (start, end) } else { (end, start) };
                let a0 = fmath::atan2(a[1] - centre[1], a[0] - centre[0]);
                let a1 = fmath::atan2(b[1] - centre[1], b[0] - centre[0]);
                let mut t = (a1 - a0).rem_euclid(TAU);
                if start == end {
                    t = TAU;
                }
                r * (centre[0] * t + r * (fmath::sin(a0 + t) - fmath::sin(a0)))
            }
        })
        .sum()
}

/// The segments of a whole turn that need a seam: everything but a line level with the
/// profile's plane's x axis, whose plane annulus has none.
fn seams(loops: &[Vec<Seg>]) -> usize {
    loops
        .iter()
        .flatten()
        .filter(|s| !matches!(s, Seg::Line { start, end } if start[1] == end[1]))
        .count()
}

fn turns() -> [Turn; 4] {
    [
        Turn::Full,
        Turn::Partial {
            from: 0.0,
            to: FRAC_PI_2,
        },
        Turn::Partial {
            from: -1.0,
            to: 2.5,
        },
        Turn::Partial {
            from: 0.5,
            to: 0.5 + TAU - 0.25,
        },
    ]
}

/// A profile off the axis, `n` segments (a circle one), holes among the loops: volume
/// `t A x`, area `t int x ds` and, for a partial turn, the two caps `2 A`; one face per
/// segment (and two caps); a whole turn has a circle edge and a vertex per profile vertex and
/// a seam per segment but the level lines, a partial turn a rotation arc and two section
/// vertices per vertex, the Euler count `2 - 2 holes` (`0` whole).
fn pappus(at: &Frame) {
    let profiles: Vec<(&str, Vec<Vec<Seg>>)> = vec![
        ("rectangle", vec![rect(2.0, 0.0, 5.0, 3.0)]),
        (
            "trapezoid",
            vec![polygon(&[[2.0, 0.0], [6.0, 0.0], [4.0, 3.0], [2.0, 3.0]])],
        ),
        (
            "L",
            vec![polygon(&[
                [1.0, 0.0],
                [5.0, 0.0],
                [5.0, 1.0],
                [2.0, 1.0],
                [2.0, 4.0],
                [1.0, 4.0],
            ])],
        ),
        (
            "triangle",
            vec![polygon(&[[3.0, 0.0], [7.0, 0.0], [3.0, 5.0]])],
        ),
        (
            "half disc",
            vec![vec![
                arc([6.0, 0.0], [8.0, 0.0], [4.0, 0.0], true),
                ln([4.0, 0.0], [8.0, 0.0]),
            ]],
        ),
        ("rounded rectangle", vec![rounded(6.0, 2.0, 3.0, 2.0, 1.0)]),
        ("circle", vec![vec![circle([5.0, 1.0], 2.0)]]),
        (
            "rectangle with a round hole",
            vec![
                rect(2.0, 0.0, 8.0, 6.0),
                // clockwise
                vec![arc([5.0, 3.0], [6.0, 3.0], [6.0, 3.0], false)],
            ],
        ),
        (
            "rectangle with a square hole",
            vec![
                rect(2.0, 0.0, 8.0, 6.0),
                // clockwise
                polygon(&[[4.0, 2.0], [4.0, 4.0], [6.0, 4.0], [6.0, 2.0]]),
            ],
        ),
    ];
    for (name, loops) in &profiles {
        let n: usize = loops.iter().map(Vec::len).sum();
        let holes = loops.len() - 1;
        let prism = solid::extrusion(loops, 1.0).unwrap();
        let (a, first) = (prism.volume(), prism.moments.first[0]);
        let rim = rim_moment(loops);
        for turn in turns() {
            let (t, whole) = match turn {
                Turn::Full => (TAU, true),
                Turn::Partial { from, to } => (to - from, false),
            };
            let want = if whole {
                Want::pappus(t * first, t * rim, [n, n + seams(loops), n], 0, 2 + holes)
            } else {
                Want::pappus(
                    t * first,
                    t * rim + 2.0 * a,
                    [n + 2, 3 * n, 2 * n],
                    2 - 2 * holes as i64,
                    2,
                )
            };
            let m = revolved(loops, at, turn);
            let want = Want {
                about: at.origin(),
                ..want
            };
            judge(
                &m.body,
                &want,
                &format!("{name}, {turn:?} at {:?}", at.origin()),
            );
        }
    }
}

#[test]
fn pappus_volume_and_area_of_revolved_profiles() {
    pappus(&plain());
}

#[test]
fn pappus_volume_and_area_of_revolved_profiles_in_a_tilted_frame() {
    pappus(&tilted());
}

// ---------------------------------------------------------------------------------------
// Cornered and stations

/// The plane section of the end of a swept body: the cap's plane through the station's point,
/// its normal along the tangent (either way).
fn assert_cap_at(b: &Brep, cap: FaceId, at: Point3, tangent: Vec3, name: &str) {
    let Surface::Plane(plane) = b.face(cap).unwrap().surface() else {
        panic!("{name}: a cap off a plane");
    };
    let n = plane.normal(Point2::new(0.0, 0.0));
    let o = plane.point(Point2::new(0.0, 0.0));
    let along = Vec3::new(n.x, n.y, n.z).dot(tangent);
    assert!(
        (along.abs() - 1.0).abs() < 1e-12,
        "{name}: normal {n:?} vs {tangent:?}"
    );
    let off = Vec3::new(n.x, n.y, n.z).dot(o - at);
    assert!(
        off.abs() < 1e-12,
        "{name}: the cap is {off:e} off the station"
    );
}

/// A profile centred on the path along an L at any angle, in any plane: the mitred tube's
/// volume and area are T9's straight extrusion's (area and perimeter times the length), the
/// four sections' vertices on the planes they must be (the start, the bisecting mitre and the
/// end), the caps square to the path's end tangents. Counts: `n` segments give two pieces of
/// `n` walls, three sections of `n` vertices and rim edges, `2 n` rails and two caps.
#[test]
fn cornered_tubes_keep_area_and_perimeter_times_length() {
    let profiles: Vec<(&str, Vec<Vec<Seg>>)> = vec![
        ("square", vec![rect(-1.0, -1.0, 1.0, 1.0)]),
        ("rectangle", vec![rect(-1.5, -0.5, 1.5, 0.5)]),
        ("circle", vec![vec![circle([0.0, 0.0], 1.0)]]),
        ("rounded rectangle", vec![rounded(0.0, 0.0, 1.5, 1.0, 0.5)]),
        ("hexagon", vec![polygon(&hexagon(1.0))]),
    ];
    let (l1, l2) = (10.0, 8.0);
    let planes = [v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0), v3(0.6, 0.8, 0.0)];
    for (name, loops) in &profiles {
        let n = loops[0].len();
        let sections = if n == 1 { 1 } else { n };
        for angle in [30.0_f64, 60.0, 90.0, 120.0, 150.0] {
            let turn = angle.to_radians();
            for e in planes {
                let t2 = e * fmath::sin(turn) + v3(0.0, 0.0, fmath::cos(turn));
                let corner = p3(0.0, 0.0, l1);
                let end = corner + t2 * l2;
                let m = sweep_along(
                    loops,
                    &frame(Point3::ORIGIN, v3(0.0, 0.0, 1.0), v3(1.0, 0.0, 0.0)),
                    &[line(Point3::ORIGIN, corner), line(corner, end)],
                );
                let t = solid::extrusion(loops, l1 + l2).unwrap();
                // Faces: a wall per segment and piece and two caps; edges: three sections and
                // a rail per vertex-less seam or vertex and piece; a circle has a seam each.
                let counts = if n == 1 {
                    [4, 5, 3]
                } else {
                    [2 * n + 2, 5 * n, 3 * n]
                };
                let label = format!("{name} at {angle} about {e:?}");
                let want = Want {
                    first: None,
                    ..Want::of(&t, counts, 2)
                };
                judge(&m.body, &want, &label);
                // The vertices: `sections` on each of the start plane, the mitre plane and the
                // end plane.
                let bisect = v3(0.0, 0.0, 1.0) + t2;
                let on = |plane_at: Point3, normal: Vec3| {
                    m.body
                        .vertices()
                        .filter(|v| {
                            let d = (v.point() - plane_at).dot(normal) / normal.length();
                            d.abs() < 1e-9
                        })
                        .count()
                };
                assert_eq!(
                    on(Point3::ORIGIN, v3(0.0, 0.0, 1.0)),
                    sections,
                    "{label}: start"
                );
                assert_eq!(on(corner, bisect), sections, "{label}: mitre");
                assert_eq!(on(end, t2), sections, "{label}: end");
                assert_cap_at(
                    &m.body,
                    m.faces.start.unwrap(),
                    Point3::ORIGIN,
                    v3(0.0, 0.0, 1.0),
                    &label,
                );
                assert_cap_at(&m.body, m.faces.end.unwrap(), end, t2, &label);
            }
        }
    }
}

fn hexagon(r: f64) -> Vec<[f64; 2]> {
    (0..6)
        .map(|i| {
            let a = f64::from(i) * PI / 3.0;
            [r * fmath::cos(a), r * fmath::sin(a)]
        })
        .collect()
}

/// Stations: the end caps of a swept body stand at the closed-form station of the path, the
/// frame carried along a line, a quarter arc and a line: the end point and tangent of each
/// piece, and the circle's end cap a disc of the profile's area.
#[test]
fn the_caps_of_a_sweep_stand_at_the_paths_stations() {
    let (l1, rho, l2) = (7.0, 5.0, 4.0);
    let pieces = [
        line(Point3::ORIGIN, p3(0.0, 0.0, l1)),
        arc_piece(
            p3(rho, 0.0, l1),
            v3(0.0, 1.0, 0.0),
            p3(0.0, 0.0, l1),
            FRAC_PI_2,
        ),
        line(p3(rho, 0.0, l1 + rho), p3(rho + l2, 0.0, l1 + rho)),
    ];
    let end = p3(rho + l2, 0.0, l1 + rho);
    let planes = vec!["plane"; 4];
    for (name, loops, area, perimeter, counts, kinds) in [
        (
            "circle",
            vec![vec![circle([0.0, 0.0], 1.5)]],
            PI * 2.25,
            TAU * 1.5,
            [5, 7, 4],
            [vec!["cylinder"], vec!["torus"], vec!["cylinder"]],
        ),
        (
            "square",
            vec![rect(-1.0, -1.0, 1.0, 1.0)],
            4.0,
            8.0,
            [14, 28, 16],
            // About the bend's axis (y): the sides along it are cylinders, the others planes.
            [
                planes.clone(),
                vec!["plane", "cylinder", "plane", "cylinder"],
                planes.clone(),
            ],
        ),
    ] {
        let m = sweep_along(&loops, &plain(), &pieces);
        for (piece, want) in m.faces.walls.iter().zip(&kinds) {
            let got: Vec<_> = piece[0].iter().map(|&f| kind(&m.body, f)).collect();
            assert_eq!(&got, want, "{name}: wall kinds");
        }
        assert_cap_at(
            &m.body,
            m.faces.start.unwrap(),
            Point3::ORIGIN,
            v3(0.0, 0.0, 1.0),
            name,
        );
        assert_cap_at(&m.body, m.faces.end.unwrap(), end, v3(1.0, 0.0, 0.0), name);
        // Pappus about the bend: the quarter turn's centroid path is rho, so the pieces'
        // volumes are A l1, A rho pi / 2 and A l2; the walls' P times the same lengths, and
        // two caps.
        let length = l1 + rho * FRAC_PI_2 + l2;
        let want = Want::pappus(area * length, perimeter * length + 2.0 * area, counts, 2, 2);
        judge(&m.body, &want, name);
    }
}
