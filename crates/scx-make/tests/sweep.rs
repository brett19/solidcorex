// Written for solidcorex. Apache-2.0.
//! `sweep` with no law (research 75 MK5): line-and-arc profiles along line-and-arc paths,
//! judged against closed forms. A line piece is an extrusion (a prism's volume), an arc
//! piece a partial revolve (Pappus's volume and area), and a corner between lines a mitre:
//! a profile whose centroid is on the path sweeps its area times the path's length either
//! side, and the mitre plane is where the arms meet, so a mitred tube's volume and area are
//! the straight tube's. The mitre's own section is checked by its curves: a circle's is the
//! ellipse of semi-axes `r` and `r / cos(angle / 2)`.
//!
//! Each body is also checked for structure (two fins of opposite senses on every edge, the
//! Euler-Poincare count) and by the BR8 oracle, and each refusal by its typed error.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use core::f64::consts::{FRAC_PI_2, PI, TAU};

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, EdgeShape, FaceSide};
use scx_curve::Curve;
use scx_make::{MakeError, Path, Piece, Refusal, SweepFaces, Unsupported, sweep};
use scx_mass::{Mass, MassSpec, RelAccuracy, mass};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_planar::{Region, Segment};
use scx_surface::Surface;
use scx_testkit::bodies::check::{Tolerance, violations};
use scx_testkit::profile::Seg;

fn doc() -> Document {
    Document::default()
}

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn unit(x: f64, y: f64, z: f64) -> UnitVec3 {
    Vec3::new(x, y, z).normalized().unwrap()
}

/// The frame at the origin along `+z`, its x axis `+x`.
fn plain() -> Frame {
    Frame::new(
        Point3::ORIGIN,
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
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

fn circle(c: [f64; 2], r: f64, ccw: bool) -> Seg {
    let p = [c[0] + r, c[1]];
    arc(c, p, p, ccw)
}

fn polygon(p: &[[f64; 2]]) -> Vec<Seg> {
    p.iter()
        .zip(p.iter().cycle().skip(1))
        .map(|(&a, &b)| ln(a, b))
        .collect()
}

/// The square of side `a` about the origin.
fn square(a: f64) -> Vec<Seg> {
    let h = 0.5 * a;
    polygon(&[[-h, -h], [h, -h], [h, h], [-h, h]])
}

fn line(a: Point3, b: Point3) -> Piece {
    Piece::Line { from: a, to: b }
}

/// The polyline through `points`.
fn polyline(points: &[Point3]) -> Vec<Piece> {
    points.windows(2).map(|w| line(w[0], w[1])).collect()
}

fn path(pieces: &[Piece]) -> Path {
    Path::new(pieces, &doc()).unwrap()
}

/// Line `[0, 0, 0] -> [0, 0, l1]`, then a quarter turn of radius `rho` about `+y` to
/// `[rho, 0, l1 + rho]` heading `+x`, then `l2` along `+x`.
fn bent(l1: f64, rho: f64, l2: f64) -> Vec<Piece> {
    let mut pieces = vec![line(Point3::ORIGIN, p3(0.0, 0.0, l1))];
    pieces.push(Piece::Arc {
        centre: p3(rho, 0.0, l1),
        axis: unit(0.0, 1.0, 0.0),
        start: p3(0.0, 0.0, l1),
        angle: FRAC_PI_2,
    });
    if l2 > 0.0 {
        pieces.push(line(p3(rho, 0.0, l1 + rho), p3(rho + l2, 0.0, l1 + rho)));
    }
    pieces
}

fn make(loops: &[Vec<Seg>], pieces: &[Piece]) -> (Brep, SweepFaces) {
    let m = sweep(&region(loops), &plain(), &path(pieces), &doc()).unwrap();
    (m.body, m.faces)
}

fn refusal(loops: &[Vec<Seg>], frame: &Frame, pieces: &[Piece]) -> MakeError {
    sweep(&region(loops), frame, &path(pieces), &doc()).unwrap_err()
}

/// Two regions and `shells` shells, every edge with two fins of opposite senses, every
/// face's front outside and back in the solid, the Euler-Poincare count `chi` and the BR8
/// oracle silent.
fn assert_closed(b: &Brep, chi: i64, shells: usize) {
    assert_eq!(b.shells().len(), shells);
    for e in b.edges() {
        let f: Vec<_> = e.fins().collect();
        assert_eq!(f.len(), 2, "{:?}", e.id());
        assert_ne!(f[0].sense, f[1].sense, "{:?}", e.id());
    }
    let mut loops = 0;
    for face in b.faces() {
        assert!(!face.shell(FaceSide::Front).region().is_solid());
        assert!(face.shell(FaceSide::Back).region().is_solid());
        loops += face.loops().len();
        for u in face.uses() {
            assert!(u.bound().is_none(), "{:?}", face.id());
        }
    }
    let (f, e, v) = (b.faces().len(), b.edges().len(), b.vertices().len());
    let euler = v as i64 - e as i64 + f as i64 - (loops as i64 - f as i64);
    assert_eq!(euler, chi, "V - E + F - (L - F)");
    let bad = violations(&b.edit(), Tolerance::default());
    assert!(bad.is_empty(), "{bad:?}");
}

fn measure(b: &Brep) -> Mass {
    let spec = MassSpec::new(RelAccuracy::new(1e-12).unwrap()).with_about(Point3::ORIGIN);
    mass(
        b,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

fn assert_within(got: f64, error: f64, want: f64, what: &str) {
    let allowed = 1e-9 * want.abs();
    assert!(
        (got - want).abs() <= allowed && error <= allowed,
        "{what}: {got:e} (estimated error {error:e}) vs {want:e}, allowed {allowed:e}"
    );
}

/// Volume and area of `b` against `[volume, area]`.
fn assert_mass(b: &Brep, want: [f64; 2], what: &str) {
    let m = measure(b);
    assert_within(
        m.volume,
        m.error.volume,
        want[0],
        &format!("{what}: volume"),
    );
    assert_within(m.area, m.error.area, want[1], &format!("{what}: area"));
}

/// A tube of a profile of area `a` and perimeter `p`, both centroids on the path, along
/// `length` of path: the walls and, when open, two caps.
fn tube(a: f64, p: f64, length: f64, open: bool) -> [f64; 2] {
    [a * length, p * length + if open { 2.0 * a } else { 0.0 }]
}

fn kind(s: &Surface) -> &'static str {
    match s {
        Surface::Plane(_) => "plane",
        Surface::Cylinder(_) => "cylinder",
        Surface::Cone(_) => "cone",
        Surface::Sphere(_) => "sphere",
        Surface::Torus(_) => "torus",
        _ => "other",
    }
}

fn kinds(b: &Brep, faces: &[scx_brep::FaceId]) -> Vec<&'static str> {
    faces
        .iter()
        .map(|&f| kind(b.face(f).unwrap().surface()))
        .collect()
}

/// The ellipses among the edges, by semi-axes (larger first).
fn ellipses(b: &Brep) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = b
        .edges()
        .filter_map(|e| match e.shape() {
            EdgeShape::Curve {
                curve: Curve::Ellipse(c),
                ..
            } => {
                let (x, y) = (c.semi_x().mm(), c.semi_y().mm());
                Some([x.max(y), x.min(y)])
            }
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a[0].total_cmp(&b[0]));
    out
}

#[test]
fn straight_circle_is_a_cylinder() {
    let (b, f) = make(
        &[vec![circle([0.0, 0.0], 2.0, true)]],
        &polyline(&[Point3::ORIGIN, p3(0.0, 0.0, 10.0)]),
    );
    assert_closed(&b, 2, 2);
    assert_eq!(kinds(&b, &f.walls[0][0]), ["cylinder"]);
    assert_mass(&b, tube(4.0 * PI, 4.0 * PI, 10.0, true), "rod");
    assert!(f.start.is_some() && f.end.is_some());
}

#[test]
fn straight_matches_a_tilted_line() {
    // Off the origin and along no axis: the frame is the path's start and tangent.
    let o = p3(125.0, -212.5, 60.0);
    let d = Vec3::new(1.0, 2.0, 3.0).normalized().unwrap();
    let frame = Frame::new(o, *d, Vec3::new(-3.0, 0.5, 1.0)).unwrap();
    let m = sweep(
        &region(&[square(3.0)]),
        &frame,
        &path(&[line(o, o + *d * 7.0)]),
        &doc(),
    )
    .unwrap();
    assert_closed(&m.body, 2, 2);
    assert_eq!(kinds(&m.body, &m.faces.walls[0][0]), ["plane"; 4]);
    let mm = scx_mass::mass(
        &m.body,
        &MassSpec::new(RelAccuracy::new(1e-12).unwrap()).with_about(o),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    let [v, a] = tube(9.0, 12.0, 7.0, true);
    assert_within(mm.volume, mm.error.volume, v, "volume");
    assert_within(mm.area, mm.error.area, a, "area");
}

#[test]
fn quarter_arc_is_pappus() {
    let pieces = bent(0.0, 10.0, 0.0);
    let pieces = &pieces[1..];
    let length = 10.0 * FRAC_PI_2;
    let (b, f) = make(&[vec![circle([0.0, 0.0], 2.0, true)]], pieces);
    assert_closed(&b, 2, 2);
    assert_eq!(kinds(&b, &f.walls[0][0]), ["torus"]);
    assert_mass(&b, tube(4.0 * PI, 4.0 * PI, length, true), "quarter torus");

    // Off the path: the centroid at radius 8.5 (the profile's x axis points at the axis).
    let (b, f) = make(&[vec![circle([1.5, 0.0], 1.0, true)]], pieces);
    assert_closed(&b, 2, 2);
    assert_mass(
        &b,
        [PI * 8.5 * FRAC_PI_2, TAU * 8.5 * FRAC_PI_2 + 2.0 * PI],
        "off-centre torus",
    );
    assert_eq!(kinds(&b, &f.walls[0][0]), ["torus"]);

    // A rectangle off the path: cylinders either side, planes across the axis.
    let (b, f) = make(
        &[polygon(&[[0.5, -1.0], [2.5, -1.0], [2.5, 1.0], [0.5, 1.0]])],
        pieces,
    );
    assert_closed(&b, 2, 2);
    assert_eq!(
        kinds(&b, &f.walls[0][0]),
        ["plane", "cylinder", "plane", "cylinder"]
    );
    // Area 4 at centroid radius 8.5; the walls at radii 9.5 and 7.5 (2 wide) and the two
    // annular sectors from 7.5 to 9.5.
    let sector = 0.5 * (9.5 * 9.5 - 7.5 * 7.5) * FRAC_PI_2;
    assert_mass(
        &b,
        [
            4.0 * 8.5 * FRAC_PI_2,
            2.0 * (9.5 + 7.5) * FRAC_PI_2 + 2.0 * sector + 8.0,
        ],
        "box bend",
    );
}

#[test]
fn arc_walls_by_kind() {
    // Across the axis a plane, a slanted line a cone, an arc a ring torus, a line at one
    // radius a cylinder. (A sphere needs a profile arc centred on the axis, which the
    // profile then reaches: refused as a tight arc.)
    let profile = vec![
        ln([-1.0, -1.0], [1.0, -1.0]),
        ln([1.0, -1.0], [2.0, 1.0]),
        arc([0.5, 1.0], [2.0, 1.0], [-1.0, 1.0], true),
        ln([-1.0, 1.0], [-1.0, -1.0]),
    ];
    let pieces = bent(0.0, 10.0, 0.0);
    let (b, f) = make(&[profile], &pieces[1..]);
    assert_closed(&b, 2, 2);
    assert_eq!(
        kinds(&b, &f.walls[0][0]),
        ["plane", "cone", "torus", "cylinder"]
    );
    // Pappus about the axis: the profile's area and its centroid's distance from the axis
    // (the profile's x axis points at it, so distance 10 - x).
    let tri = [(-1.0, -1.0), (1.0, -1.0), (2.0, 1.0), (-1.0, 1.0)];
    let mut a = 0.0;
    let mut mx = 0.0;
    for i in 0..4 {
        let (x0, y0) = tri[i];
        let (x1, y1) = tri[(i + 1) % 4];
        let c = x0 * y1 - x1 * y0;
        a += 0.5 * c;
        mx += (x0 + x1) * c / 6.0;
    }
    // The half disc of radius 1.5 above y = 1 centred at x = 0.5.
    let half = 0.5 * PI * 1.5 * 1.5;
    a += half;
    mx += half * 0.5;
    let xbar = mx / a;
    let m = measure(&b);
    assert_within(
        m.volume,
        m.error.volume,
        a * (10.0 - xbar) * FRAC_PI_2,
        "volume",
    );
}

#[test]
fn bent_rod_is_line_arc_line() {
    let length = 10.0 + 10.0 * FRAC_PI_2 + 5.0;
    let (b, f) = make(
        &[vec![circle([0.0, 0.0], 2.0, true)]],
        &bent(10.0, 10.0, 5.0),
    );
    assert_closed(&b, 2, 2);
    assert_eq!(f.walls.len(), 3);
    assert_eq!(b.faces().len(), 5);
    assert_eq!(kinds(&b, &f.walls[1][0]), ["torus"]);
    assert_mass(&b, tube(4.0 * PI, 4.0 * PI, length, true), "bent rod");
    // Smooth joints: no ellipse anywhere.
    assert!(ellipses(&b).is_empty());
}

#[test]
fn five_segment_profile_counts() {
    let profile = vec![
        ln([-2.0, -1.0], [2.0, -1.0]),
        ln([2.0, -1.0], [2.0, 1.0]),
        arc([1.0, 1.0], [2.0, 1.0], [0.0, 1.0], true),
        ln([0.0, 1.0], [-2.0, 1.0]),
        ln([-2.0, 1.0], [-2.0, -1.0]),
    ];
    let (b, f) = make(&[profile], &bent(10.0, 10.0, 0.0));
    // 5 walls a piece and two caps; 3 sections of 5 rims and 2 x 5 rails; 3 x 5 vertices.
    assert_eq!(
        (b.faces().len(), b.edges().len(), b.vertices().len()),
        (12, 25, 15)
    );
    assert_eq!(
        kinds(&b, &f.walls[0][0]),
        ["plane", "plane", "cylinder", "plane", "plane"]
    );
    assert_eq!(
        kinds(&b, &f.walls[1][0]),
        ["plane", "cylinder", "torus", "plane", "cylinder"]
    );
    assert_closed(&b, 2, 2);
    let a = 8.0 + 0.5 * PI;
    let m = measure(&b);
    // The half disc's centroid is at x = 1, so the profile's x-moment is pi/2 (the box's is
    // zero) and the arc piece's volume Pappus's about radius 10 - xbar.
    let xbar = 0.5 * PI / a;
    let want = a * 10.0 + a * (10.0 - xbar) * FRAC_PI_2;
    assert_within(m.volume, m.error.volume, want, "volume");
}

#[test]
fn square_l_mitre() {
    for turn in [FRAC_PI_2, PI / 3.0, 2.0 * PI / 3.0] {
        let (s, c) = fmath::sin_cos(turn);
        let corner = p3(0.0, 0.0, 10.0);
        let pieces = polyline(&[Point3::ORIGIN, corner, corner + Vec3::new(s, 0.0, c) * 8.0]);
        let (b, f) = make(&[square(2.0)], &pieces);
        assert_closed(&b, 2, 2);
        assert_eq!(b.faces().len(), 10);
        assert_eq!(f.walls.len(), 2);
        assert_mass(&b, tube(4.0, 8.0, 18.0, true), &format!("L at {turn}"));
        // The mitre section is 2 by 2 / cos(turn / 2) about the corner.
        let far = 1.0_f64.hypot(1.0 / fmath::cos(0.5 * turn));
        let mitre: Vec<f64> = b
            .vertices()
            .map(|v| (v.point() - corner).length())
            .filter(|&d| d < 3.0)
            .collect();
        assert_eq!(mitre.len(), 4);
        for d in mitre {
            assert!((d - far).abs() < 1e-12, "{d} {far}");
        }
    }
}

#[test]
fn circle_l_mitre_is_an_ellipse() {
    let corner = p3(0.0, 0.0, 10.0);
    let pieces = polyline(&[Point3::ORIGIN, corner, p3(10.0, 0.0, 10.0)]);
    let (b, f) = make(&[vec![circle([0.0, 0.0], 1.5, true)]], &pieces);
    assert_closed(&b, 2, 2);
    assert_eq!(kinds(&b, &f.walls[0][0]), ["cylinder"]);
    assert_eq!(kinds(&b, &f.walls[1][0]), ["cylinder"]);
    let e = ellipses(&b);
    assert_eq!(e.len(), 1);
    assert!((e[0][0] - 1.5 * 2.0_f64.sqrt()).abs() < 1e-12, "{e:?}");
    assert!((e[0][1] - 1.5).abs() < 1e-12, "{e:?}");
    assert_mass(&b, tube(2.25 * PI, 3.0 * PI, 20.0, true), "circle L");
}

#[test]
fn polyline_tube_out_of_plane() {
    let pieces = polyline(&[
        Point3::ORIGIN,
        p3(0.0, 0.0, 10.0),
        p3(10.0, 0.0, 10.0),
        p3(10.0, 10.0, 10.0),
        p3(10.0, 10.0, 25.0),
    ]);
    let (b, f) = make(&[vec![circle([0.0, 0.0], 1.0, true)]], &pieces);
    assert_closed(&b, 2, 2);
    assert_eq!(f.walls.len(), 4);
    assert_eq!(ellipses(&b).len(), 3);
    assert_mass(&b, tube(PI, TAU, 45.0, true), "tube");

    // A square, so that the frame's turn shows in the plane walls.
    let (b, _) = make(&[square(1.0)], &pieces);
    assert_closed(&b, 2, 2);
    assert_mass(&b, tube(1.0, 4.0, 45.0, true), "square tube");
}

#[test]
fn bend_then_corner() {
    let mut pieces = bent(10.0, 10.0, 6.0);
    pieces.push(line(p3(16.0, 0.0, 20.0), p3(16.0, 7.0, 20.0)));
    let (b, f) = make(&[vec![circle([0.0, 0.0], 1.0, true)]], &pieces);
    assert_closed(&b, 2, 2);
    assert_eq!(f.walls.len(), 4);
    assert_eq!(ellipses(&b).len(), 1);
    let length = 10.0 + 10.0 * FRAC_PI_2 + 6.0 + 7.0;
    assert_mass(&b, tube(PI, TAU, length, true), "bend then corner");
}

#[test]
fn hole_lines_the_tube() {
    let loops = [square(4.0), vec![circle([0.0, 0.0], 1.0, true)]];
    let pieces = bent(10.0, 10.0, 5.0);
    let (b, f) = make(&loops, &pieces);
    // A thick pipe: genus one.
    assert_closed(&b, 0, 2);
    assert_eq!(f.walls[0].len(), 2);
    assert_eq!(kinds(&b, &f.walls[0][1]), ["cylinder"]);
    assert_eq!(kinds(&b, &f.walls[1][1]), ["torus"]);
    let length = 10.0 + 10.0 * FRAC_PI_2 + 5.0;
    assert_mass(
        &b,
        tube(16.0 - PI, 16.0 + TAU, length, true),
        "square with a hole",
    );
}

#[test]
fn closed_square_frame() {
    let pieces = polyline(&[
        Point3::ORIGIN,
        p3(0.0, 0.0, 10.0),
        p3(10.0, 0.0, 10.0),
        p3(10.0, 0.0, 0.0),
        Point3::ORIGIN,
    ]);
    let (b, f) = make(&[square(2.0)], &pieces);
    assert!(f.start.is_none() && f.end.is_none());
    assert_eq!(b.faces().len(), 16);
    assert_closed(&b, 0, 2);
    assert_mass(&b, tube(4.0, 8.0, 40.0, false), "picture frame");

    // A circle in it: four ellipses, one at each corner.
    let (b, _) = make(&[vec![circle([0.0, 0.0], 1.0, true)]], &pieces);
    assert_closed(&b, 0, 2);
    assert_eq!(ellipses(&b).len(), 4);
    assert_mass(&b, tube(PI, TAU, 40.0, false), "round frame");
}

#[test]
fn closed_with_a_hole_has_a_hollow() {
    let pieces = polyline(&[
        Point3::ORIGIN,
        p3(0.0, 0.0, 10.0),
        p3(10.0, 0.0, 10.0),
        p3(10.0, 0.0, 0.0),
        Point3::ORIGIN,
    ]);
    let loops = [
        square(3.0),
        square(1.0)
            .into_iter()
            .rev()
            .map(|s| match s {
                Seg::Line { start, end } => ln(end, start),
                s @ Seg::Arc { .. } => s,
            })
            .collect(),
    ];
    let (b, _) = make(&loops, &pieces);
    // Two tori: the outer surface and the hole's, each a shell.
    assert_closed(&b, 0, 4);
    assert_mass(&b, tube(8.0, 16.0, 40.0, false), "hollow frame");
}

#[test]
fn whole_turn_is_a_torus() {
    let pieces = [Piece::Arc {
        centre: p3(10.0, 0.0, 0.0),
        axis: unit(0.0, 1.0, 0.0),
        start: Point3::ORIGIN,
        angle: TAU,
    }];
    let (b, f) = make(&[vec![circle([0.0, 0.0], 2.0, true)]], &pieces);
    assert!(f.start.is_none());
    assert_eq!(kinds(&b, &f.walls[0][0]), ["torus"]);
    assert_closed(&b, 0, 2);
    assert_mass(&b, tube(4.0 * PI, 4.0 * PI, 10.0 * TAU, false), "torus");

    // A square: two cylinders and two annuli.
    let (b, f) = make(&[square(2.0)], &pieces);
    assert_eq!(
        kinds(&b, &f.walls[0][0]),
        ["plane", "cylinder", "plane", "cylinder"]
    );
    assert_closed(&b, 0, 2);
    assert_mass(&b, tube(4.0, 8.0, 10.0 * TAU, false), "square ring");
}

#[test]
fn deterministic() {
    let pieces = bent(10.0, 10.0, 5.0);
    let loops = [square(4.0), vec![circle([0.5, 0.0], 1.0, true)]];
    let (a, fa) = make(&loops, &pieces);
    let (b, fb) = make(&loops, &pieces);
    assert_eq!(fa, fb);
    let points = |b: &Brep| -> Vec<[u64; 3]> {
        b.vertices()
            .map(|v| v.point().to_array().map(f64::to_bits))
            .collect()
    };
    assert_eq!(points(&a), points(&b));
}

#[test]
fn refuses_a_tight_arc() {
    let pieces = bent(0.0, 2.0, 0.0);
    let e = refusal(
        &[vec![circle([0.0, 0.0], 2.0, true)]],
        &plain(),
        &pieces[1..],
    );
    assert!(
        matches!(e, MakeError::Refused(Refusal::TightArc { piece: 0, .. })),
        "{e:?}"
    );
    // The reach is towards the axis: the same circle moved away from it fits.
    let ok = sweep(
        &region(&[vec![circle([-1.0, 0.0], 2.0, true)]]),
        &plain(),
        &path(&pieces[1..]),
        &doc(),
    );
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn refuses_mitres_that_meet() {
    // A U: the middle piece's mitres lean towards each other, 1 each for a 2-wide square.
    let u = |w: f64| {
        polyline(&[
            Point3::ORIGIN,
            p3(0.0, 0.0, 10.0),
            p3(w, 0.0, 10.0),
            p3(w, 0.0, 0.0),
        ])
    };
    let e = refusal(&[square(2.0)], &plain(), &u(1.5));
    assert!(
        matches!(e, MakeError::Refused(Refusal::MitresMeet { piece: 1 })),
        "{e:?}"
    );
    let (b, _) = make(&[square(2.0)], &u(2.5));
    assert_closed(&b, 2, 2);
    assert_mass(&b, tube(4.0, 8.0, 22.5, true), "U");

    // A zig: the same piece between parallel mitres is a parallelogram, however short.
    let zig = polyline(&[
        Point3::ORIGIN,
        p3(0.0, 0.0, 10.0),
        p3(1.5, 0.0, 10.0),
        p3(1.5, 0.0, 20.0),
    ]);
    let (b, _) = make(&[square(2.0)], &zig);
    assert_closed(&b, 2, 2);
    assert_mass(&b, tube(4.0, 8.0, 21.5, true), "zig");
}

#[test]
fn refuses_a_misplaced_profile() {
    let pieces = polyline(&[Point3::ORIGIN, p3(0.0, 0.0, 10.0)]);
    let off = Frame::new(
        p3(0.5, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let e = refusal(&[square(1.0)], &off, &pieces);
    assert!(
        matches!(e, MakeError::Refused(Refusal::Placement { .. })),
        "{e:?}"
    );
    let tilted = Frame::new(
        Point3::ORIGIN,
        Vec3::new(0.1, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let e = refusal(&[square(1.0)], &tilted, &pieces);
    assert!(
        matches!(e, MakeError::Refused(Refusal::Placement { .. })),
        "{e:?}"
    );
}

#[test]
fn refuses_helix_and_corner_on_arc() {
    let helix = [Piece::Helix {
        centre: p3(10.0, 0.0, 0.0),
        axis: unit(0.0, 1.0, 0.0),
        start: Point3::ORIGIN,
        angle: PI,
        rise: 0.0,
    }];
    // A zero rise is still the helix kind.
    let e = refusal(&[square(1.0)], &plain(), &helix);
    assert!(
        matches!(
            e,
            MakeError::Unsupported(Unsupported::HelixPiece { piece: 0 })
        ),
        "{e:?}"
    );
    // An arc leaving a line at a corner.
    let pieces = [
        line(Point3::ORIGIN, p3(0.0, 0.0, 10.0)),
        Piece::Arc {
            centre: p3(0.0, 0.0, 20.0),
            axis: unit(0.0, 1.0, 0.0),
            start: p3(0.0, 0.0, 10.0),
            angle: FRAC_PI_2,
        },
    ];
    let e = refusal(&[square(1.0)], &plain(), &pieces);
    assert!(
        matches!(
            e,
            MakeError::Unsupported(Unsupported::CornerOnArc { corner: 0 })
        ),
        "{e:?}"
    );
}

#[test]
fn refuses_an_arc_past_a_turn() {
    let pieces = [Piece::Arc {
        centre: p3(10.0, 0.0, 0.0),
        axis: unit(0.0, 1.0, 0.0),
        start: Point3::ORIGIN,
        angle: 1.5 * TAU,
    }];
    let e = refusal(&[square(1.0)], &plain(), &pieces);
    assert!(
        matches!(e, MakeError::Refused(Refusal::ArcTurn { piece: 0, .. })),
        "{e:?}"
    );
}

#[test]
fn refuses_a_closed_path_that_turns_the_profile() {
    // Out of its plane, a closed polyline brings the frame back turned (here by 60
    // degrees: a square's corners would match again only at 90).
    let pieces = polyline(&[
        Point3::ORIGIN,
        p3(0.0, 0.0, 10.0),
        p3(10.0, 0.0, 10.0),
        p3(0.0, 10.0, 0.0),
        Point3::ORIGIN,
    ]);
    let e = refusal(&[square(1.0)], &plain(), &pieces);
    assert!(
        matches!(e, MakeError::Refused(Refusal::ClosedTurned { .. })),
        "{e:?}"
    );
}
