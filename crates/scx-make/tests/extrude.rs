// Written for solidcorex. Apache-2.0.
//! `extrude` (research 72 MK3): the pin's line-and-arc extrusion tests
//! (`sweep/extrude.rs`) as outcomes, every T11 profile fixture through `Region::new` and
//! `extrude`, and extrusions of lines, arcs and whole circles at plain and rotated frames
//! against the closed-form truth of `scx_testkit::solid` (volume, area) and the membership
//! oracle of `scx_testkit::csg` (points just off each face).
//!
//! Volume and area are read by `scx_mass::mass` (relative accuracy each test's tolerance) and compared to
//! the closed forms; each tolerance is also checked against the Estimate's error bound.
//!
//! Removed on purpose, from the pin's tests:
//! - `a_long_direction_extrudes_the_distance`'s zero-direction refusal: the direction is
//!   the frame's z axis, a unit vector by type, so a zero direction cannot be written.
//! - `a_draft_across_the_profile_or_an_open_profile_is_refused`'s draft half, and the
//!   draft halves of `a_draft_goes_the_way_the_direction_points` and
//!   `a_rounded_rectangle_drafted_slightly_inward_keeps_every_corner_wall`: a draft is
//!   `scx-taper`'s (step 8), with no draft argument here.
//! - `an_uncapped_profile_with_a_short_segment_keeps_every_wall`'s uncapped (open sheet)
//!   half: there is no `cap: bool`; a sheet is not this constructor's.
//!
//! Deferred: `a_profile_with_a_short_cubic_extrudes_to_a_valid_solid` waits for NURBS
//! profile segments (step 5).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use core::f64::consts::{PI, TAU};

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, EdgeShape, UseGeom};
use scx_curve::{Curve, Curve2};
use scx_make::{ExtrudeFaces, Made, MakeError, Refusal, Span, Unsupported, extrude};
use scx_mass::{MassSpec, RelAccuracy, mass};
use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{LoopId, PlanarError, Region, Segment};
use scx_surface::Surface;
use scx_testkit::analytic::Frame as TruthFrame;
use scx_testkit::csg::{Csg, Primitive, Profile, Side};
use scx_testkit::profile::{Cause, Seg, Verdict, fixtures, reversed};
use scx_testkit::solid;

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn res(r: f64) -> Resolution {
    Resolution::new(mm(r)).unwrap()
}

fn plain() -> Frame {
    Frame::new(
        Point3::ORIGIN,
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

/// A frame far from the origin, its axes along no coordinate direction.
fn tilted() -> Frame {
    Frame::new(
        Point3::new(1250.0, -2125.0, 600.0),
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(-3.0, 0.5, 1.0),
    )
    .unwrap()
}

/// The one adapter: plain segment data to the planar layer's; equal arc ends are a circle.
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

fn region(loops: &[Vec<Seg>], r: Resolution) -> Result<Region, PlanarError> {
    Region::new(
        loops[0].iter().map(segment).collect(),
        loops[1..]
            .iter()
            .map(|h| h.iter().map(segment).collect())
            .collect(),
        r,
    )
}

fn doc() -> Document {
    Document::default()
}

fn make(loops: &[Vec<Seg>], frame: &Frame, span: [f64; 2]) -> Made<ExtrudeFaces> {
    let d = doc();
    let r = region(loops, d.resolution()).unwrap();
    extrude(&r, frame, Span::new(mm(span[0]), mm(span[1])), &d).unwrap()
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

/// The pin's rounded rectangle: half sides `a`, `b`, corner radius `r`, counter-clockwise.
fn rounded(a: f64, b: f64, r: f64) -> Vec<Seg> {
    vec![
        ln([-a + r, -b], [a - r, -b]),
        arc([a - r, -b + r], [a - r, -b], [a, -b + r], true),
        ln([a, -b + r], [a, b - r]),
        arc([a - r, b - r], [a, b - r], [a - r, b], true),
        ln([a - r, b], [-a + r, b]),
        arc([-a + r, b - r], [-a + r, b], [-a, b - r], true),
        ln([-a, b - r], [-a, -b + r]),
        arc([-a + r, -b + r], [-a, -b + r], [-a + r, -b], true),
    ]
}

/// A slot (stadium) from `(x0, y)` to `(x1, y)` of half width `w`, counter-clockwise or not.
fn slot(x0: f64, x1: f64, y: f64, w: f64, ccw: bool) -> Vec<Seg> {
    let l = vec![
        ln([x0, y - w], [x1, y - w]),
        arc([x1, y], [x1, y - w], [x1, y + w], true),
        ln([x1, y + w], [x0, y + w]),
        arc([x0, y], [x0, y + w], [x0, y - w], true),
    ];
    if ccw { l } else { reversed(&l) }
}

/// The test profiles, each outer counter-clockwise and holes clockwise (as `solid` wants).
fn profiles() -> Vec<(&'static str, Vec<Vec<Seg>>)> {
    let square = polygon(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]);
    let bite = vec![
        ln([0.0, 0.0], [10.0, 0.0]),
        ln([10.0, 0.0], [10.0, 10.0]),
        arc([5.0, 10.0], [10.0, 10.0], [0.0, 10.0], false),
        ln([0.0, 10.0], [0.0, 0.0]),
    ];
    let mut plate = vec![polygon(&[
        [0.0, 0.0],
        [100.0, 0.0],
        [100.0, 50.0],
        [0.0, 50.0],
    ])];
    for i in 0..4 {
        for j in 0..3 {
            plate.push(vec![circle(
                [12.5 + 25.0 * f64::from(i), 10.0 + 15.0 * f64::from(j)],
                2.0,
                false,
            )]);
        }
    }
    vec![
        ("square", vec![square]),
        ("rounded", vec![rounded(11.5, 4.5, 1.25)]),
        ("bar", vec![vec![circle([0.5, -1.0], 3.0, true)]]),
        (
            "ring",
            vec![
                vec![circle([0.0, 0.0], 3.0, true)],
                vec![circle([0.0, 0.0], 1.0, false)],
            ],
        ),
        ("bite", vec![bite]),
        (
            "slots",
            vec![
                slot(0.0, 20.0, 0.0, 6.0, true),
                slot(2.0, 8.0, 0.0, 2.0, false),
                vec![circle([15.0, 0.0], 3.0, false)],
            ],
        ),
        ("plate", plate),
    ]
}

/// Faces, edges and vertices.
fn counts(b: &Brep) -> (usize, usize, usize) {
    (b.faces().len(), b.edges().len(), b.vertices().len())
}

/// A closed one-solid body: two regions, every edge with two fins of opposite senses, every
/// face's front outside and back solid, every pcurve exact and of the closed-form kind.
fn assert_closed_solid(b: &Brep) {
    assert_eq!(b.regions().len(), 2);
    assert_eq!(b.shells().len(), 2);
    for e in b.edges() {
        let f: Vec<_> = e.fins().collect();
        assert_eq!(f.len(), 2, "{:?}", e.id());
        assert_ne!(f[0].sense, f[1].sense, "{:?}", e.id());
    }
    for face in b.faces() {
        assert!(face.shell(scx_brep::FaceSide::Front).region().is_outside());
        assert!(face.shell(scx_brep::FaceSide::Back).region().is_solid());
        for u in face.uses() {
            assert!(u.bound().is_none());
            let UseGeom::Pcurve { curve, .. } = u.geom() else {
                panic!("a point use")
            };
            let EdgeShape::Curve { curve: c3, .. } = u.edge().shape() else {
                panic!("a collapsed edge")
            };
            let ok = matches!(
                (face.surface(), c3, curve),
                (
                    Surface::Plane(_) | Surface::Cylinder(_),
                    Curve::Line(_),
                    Curve2::Line(_)
                ) | (Surface::Plane(_), Curve::Circle(_), Curve2::Circle(_))
                    | (Surface::Cylinder(_), Curve::Circle(_), Curve2::AngleLine(_))
            );
            assert!(ok, "{:?} on {:?}", curve, face.surface());
        }
    }
}

/// The body's volume and area about `o` with their error estimates, by `scx_mass::mass` at
/// relative accuracy `accuracy`, each test's own tolerance: `(volume, its error, area, its error)`.
fn volume_and_area(b: &Brep, o: Point3, accuracy: f64) -> (f64, f64, f64, f64) {
    let spec = MassSpec::new(RelAccuracy::new(accuracy).unwrap()).with_about(o);
    let m = mass(
        b,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    (m.volume, m.error.volume, m.area, m.error.area)
}

fn truth_frame(f: &Frame, lo: f64) -> TruthFrame {
    let a = |v: Vec3| [v.x, v.y, v.z];
    let o = f.origin() + *f.z() * lo;
    TruthFrame {
        origin: [o.x, o.y, o.z],
        x: a(*f.x()),
        y: a(*f.y()),
        z: a(*f.z()),
    }
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

/// T9: every profile at the plain frame and at the tilted one (two tests, so they run side
/// by side) and spans of either sign, against `solid::extrusion`.
#[test]
fn volume_and_area_match_the_closed_forms_plain() {
    volume_and_area_match_the_closed_forms(plain());
}

#[test]
fn volume_and_area_match_the_closed_forms_tilted() {
    volume_and_area_match_the_closed_forms(tilted());
}

fn volume_and_area_match_the_closed_forms(frame: Frame) {
    let mut worst = (0.0_f64, 0.0_f64);
    for (name, loops) in profiles() {
        {
            let tol = 1e-12;
            for span in [[0.0, 3.0], [5.0, -2.0], [-1.0, 0.25]] {
                let m = make(&loops, &frame, span);
                assert_closed_solid(&m.body);
                let (lo, h) = (span[0].min(span[1]), (span[1] - span[0]).abs());
                let truth = solid::extrusion(&loops, h)
                    .unwrap()
                    .place(&truth_frame(&frame, lo));
                let (v, ve, a, ae) = volume_and_area(&m.body, frame.origin(), tol);
                let (dv, da) = (rel(v, truth.volume()), rel(a, truth.area));
                // The tolerance, and the Estimate's error within it; the volume's also carries
                // the rounding of the faces' points where they lie (a few `eps` of the frame's
                // distance from the world origin, over the area: research 72 §14.31).
                let far = 4.0 * f64::EPSILON * (frame.origin() - Point3::ORIGIN).length();
                let ve_max = tol * truth.volume() + far * truth.area;
                assert!(
                    dv <= tol && da <= tol && ve <= ve_max && ae <= tol * truth.area,
                    "{name} {span:?}: {v} (err {ve:e}) vs {}, {a} (err {ae:e}) vs {}",
                    truth.volume(),
                    truth.area
                );
                worst = (worst.0.max(dv), worst.1.max(da));
            }
        }
    }
    eprintln!("worst relative volume {:e}, area {:e}", worst.0, worst.1);
}

/// Points on the body's faces with their front normals: a plane's at interior points of
/// its loops (the region's own `classify` picks a cap's), a cylinder's along its rim arc.
fn face_points(b: &Brep, r: &Region, frame: &Frame) -> Vec<(Point3, Vec3)> {
    let mut out = Vec::new();
    for face in b.faces() {
        match face.surface() {
            Surface::Plane(pl) => {
                let n: Vec3 = *pl.frame().z();
                let vs: Vec<Point3> = face.uses().map(|u| u.start().point()).collect();
                if n.dot(*frame.z()).abs() > 0.5 {
                    // A cap: a grid over the profile, kept where the region says Inside.
                    let o = pl.frame().origin();
                    let (x, y): (Vec3, Vec3) = (*frame.x(), *frame.y());
                    let to2 = |p: Point3| ((p - o).dot(x), (p - o).dot(y));
                    let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
                    for p in &vs {
                        let (a, b) = to2(*p);
                        lo = (lo.0.min(a), lo.1.min(b));
                        hi = (hi.0.max(a), hi.1.max(b));
                    }
                    // Whole circles have one vertex; widen to their extent by the bar's
                    // radius at most by sampling past it.
                    let pad = (hi.0 - lo.0).max(hi.1 - lo.1).max(1.0);
                    for i in 0..23 {
                        for j in 0..23 {
                            let a = lo.0 - pad
                                + (hi.0 - lo.0 + 2.0 * pad) * (f64::from(i) + 0.5) / 23.0;
                            let b = lo.1 - pad
                                + (hi.1 - lo.1 + 2.0 * pad) * (f64::from(j) + 0.5) / 23.0;
                            if r.classify(Point2::new(a, b)) == Ok(scx_planar::Place::Inside) {
                                out.push((o + x * a + y * b, n));
                            }
                        }
                    }
                } else {
                    // A rectangular wall: interior points of its four corners.
                    for (s, t) in [(0.5, 0.5), (0.2, 0.7), (0.8, 0.3)] {
                        let p = vs[0] + (vs[1] - vs[0]) * s + (vs[3] - vs[0]) * t;
                        out.push((p, n));
                    }
                }
            }
            Surface::Cylinder(cy) => {
                let s = cy.sense().sign();
                let mut arc = None;
                let mut low = f64::MAX;
                let mut high = f64::MIN;
                for u in face.uses() {
                    let z = (u.start().point() - frame.origin()).dot(*frame.z());
                    low = low.min(z);
                    high = high.max(z);
                    if let EdgeShape::Curve {
                        curve: Curve::Circle(c),
                        interval,
                    } = u.edge().shape()
                    {
                        arc = Some((*c, interval.max() - interval.min()));
                    }
                }
                let (c, t) = arc.unwrap();
                let base = c.frame().origin()
                    - *frame.z() * (c.frame().origin() - frame.origin()).dot(*frame.z());
                for (k, f) in [(0.5, 0.5), (0.1, 0.8), (0.9, 0.2)] {
                    let th = t * k;
                    let e = *c.frame().x() * fmath::cos(th) + *c.frame().y() * fmath::sin(th);
                    let z = low + (high - low) * f;
                    out.push((base + e * c.radius().mm() + *frame.z() * z, e * s));
                }
            }
            s => panic!("{s:?}"),
        }
    }
    out
}

/// T10: just off each face, the front side is outside the truth solid and the back inside.
#[test]
fn points_off_every_face_are_on_the_right_side() {
    for (name, loops) in profiles() {
        for frame in [plain(), tilted()] {
            for span in [[0.0, 3.0], [4.0, -1.5]] {
                let m = make(&loops, &frame, span);
                let r = region(&loops, doc().resolution()).unwrap();
                let (lo, h) = (span[0].min(span[1]), (span[1] - span[0]).abs());
                let truth = Csg::leaf(
                    &truth_frame(&frame, lo),
                    Primitive::Extrusion {
                        profile: Profile::new(&loops).unwrap(),
                        height: h,
                    },
                )
                .unwrap();
                let pts = face_points(&m.body, &r, &frame);
                assert!(
                    pts.len() > 3 * m.body.faces().len() - 3,
                    "{name}: {}",
                    pts.len()
                );
                for (p, n) in pts {
                    let d = 1e-3;
                    let a = |q: Point3| [q.x, q.y, q.z];
                    let out = truth.side(a(p + n * d), 1e-6);
                    let inn = truth.side(a(p - n * d), 1e-6);
                    assert_eq!(
                        (out, inn),
                        (Side::Out, Side::In),
                        "{name} {span:?} at {p:?}"
                    );
                }
            }
        }
    }
}

/// The face record: one wall per segment as given, each on its segment's surface kind, and
/// the start cap at the span's start with its front away from the end.
#[test]
fn the_record_names_every_face_by_role() {
    let given = vec![
        rounded(11.5, 4.5, 1.25),
        reversed(&slot(-5.0, 5.0, 0.0, 2.0, false)),
        vec![circle([9.0, 0.0], 0.8, true)],
    ];
    let d = doc();
    let r = region(&given, d.resolution()).unwrap();
    assert_eq!(r.holes_reversed(), &[true, true]);
    for span in [[1.0, 4.0], [4.0, 1.0]] {
        let frame = tilted();
        let m = extrude(&r, &frame, Span::new(mm(span[0]), mm(span[1])), &d).unwrap();
        let f = &m.faces;
        assert_eq!(f.walls.len(), given.len());
        let mut all = vec![f.start, f.end];
        for (k, (walls, segs)) in f.walls.iter().zip(&given).enumerate() {
            let turned = if k == 0 {
                r.outer_reversed()
            } else {
                r.holes_reversed()[k - 1]
            };
            assert_eq!(walls.len(), segs.len());
            for (&w, s) in walls.iter().zip(segs) {
                let surface = m.body.face(w).unwrap().surface();
                match (s, surface) {
                    (Seg::Line { start, end }, Surface::Plane(pl)) => {
                        // The plane holds the segment, lifted.
                        for q in [start, end] {
                            let p = frame.origin() + *frame.x() * q[0] + *frame.y() * q[1];
                            let off = (p - pl.frame().origin()).dot(*pl.frame().z());
                            assert!(off.abs() < 1e-9, "{off}");
                        }
                    }
                    (Seg::Arc { centre, ccw, .. }, Surface::Cylinder(cy)) => {
                        let c = frame.origin() + *frame.x() * centre[0] + *frame.y() * centre[1];
                        let off = (c - cy.frame().origin()).cross(*frame.z()).length();
                        assert!(off < 1e-9, "{off}");
                        // Convex where the arc, as the region runs it, is counter-clockwise.
                        let want = if *ccw == turned {
                            Sense::Reversed
                        } else {
                            Sense::Forward
                        };
                        assert_eq!(cy.sense(), want);
                    }
                    (s, surface) => panic!("{s:?} on {surface:?}"),
                }
                all.push(w);
            }
        }
        all.sort();
        all.dedup();
        assert_eq!(all.len(), m.body.faces().len());
        for (id, z) in [(f.start, span[0]), (f.end, span[1])] {
            let Surface::Plane(pl) = m.body.face(id).unwrap().surface() else {
                panic!("a cap is a plane")
            };
            let h = (pl.frame().origin() - frame.origin()).dot(*frame.z());
            assert!((h - z).abs() < 1e-9, "{h} vs {z}");
            let away = if z == span[0] {
                span[0] - span[1]
            } else {
                span[1] - span[0]
            };
            assert!(pl.frame().z().dot(*frame.z()) * away > 0.0);
        }
    }
}

/// A round bar is the cylinder's topology: one wall with its seam, two caps.
#[test]
fn a_round_bar_has_one_wall_and_a_seam() {
    let m = make(&[vec![circle([0.0, 0.0], 2.0, true)]], &plain(), [0.0, 5.0]);
    assert_eq!(counts(&m.body), (3, 3, 2));
    assert_eq!(m.faces.walls, vec![vec![m.faces.walls[0][0]]]);
    let (v, ve, a, ae) = volume_and_area(&m.body, Point3::ORIGIN, 1e-14);
    // The stated error carries the samples' rounding (research 72 §14.31), some tens of
    // `eps` here: within twice the accuracy asked, not within it.
    let (tv, ta) = (20.0 * PI, 2.0 * 4.0 * PI + 4.0 * PI * 5.0);
    assert!(
        rel(v, tv) < 1e-14 && rel(a, ta) < 1e-14 && ve < 2e-14 * tv && ae < 2e-14 * ta,
        "{v} (err {ve:e}) {a} (err {ae:e})"
    );
}

/// A plate with circle holes: one wall per hole.
#[test]
fn circle_holes_give_one_wall_each() {
    let (_, plate) = profiles().pop().unwrap();
    let m = make(&plate, &plain(), [0.0, 10.0]);
    // 2 caps + 4 sides + 12 holes; 4 x 3 edges + 12 x 3; 8 + 12 x 2.
    assert_eq!(counts(&m.body), (18, 48, 32));
    let truth = solid::plate(
        100.0,
        50.0,
        10.0,
        &(0..12)
            .map(|k| solid::Hole {
                x: 12.5 + 25.0 * f64::from(k / 3),
                y: 10.0 + 15.0 * f64::from(k % 3),
                r: 2.0,
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let (v, ve, a, ae) = volume_and_area(&m.body, Point3::ORIGIN, 1e-12);
    assert!(
        rel(v, truth.volume()) < 1e-12
            && rel(a, truth.area) < 1e-12
            && ve < 1e-12 * truth.volume()
            && ae < 1e-12 * truth.area,
        "{v} (err {ve:e}) {a} (err {ae:e})"
    );
}

/// The pin's `a_long_direction_extrudes_the_distance`: a 2 x 2 square extruded 3 along a
/// frame given a long z hint lands at height 3, its corners where the profile puts them.
#[test]
fn a_long_direction_extrudes_the_distance() {
    let frame = Frame::new(
        Point3::ORIGIN,
        Vec3::new(0.0, 0.0, 1e6),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let square = polygon(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]);
    let m = make(&[square], &frame, [0.0, 3.0]);
    assert_eq!(counts(&m.body), (6, 12, 8));
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for v in m.body.vertices() {
        let p = v.point();
        for (k, c) in [p.x, p.y, p.z].into_iter().enumerate() {
            lo[k] = lo[k].min(c);
            hi[k] = hi[k].max(c);
        }
    }
    assert_eq!((lo, hi), ([0.0; 3], [2.0, 2.0, 3.0]));
}

/// The pin's `a_draft_goes_the_way_the_direction_points`, undrafted: a span whose end is
/// below its start extrudes along -z.
#[test]
fn a_reversed_span_goes_the_other_way() {
    let square = polygon(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]);
    let m = make(&[square], &plain(), [0.0, -3.0]);
    let zs: Vec<f64> = m.body.vertices().map(|v| v.point().z).collect();
    assert!(zs.iter().all(|&z| z == 0.0 || z == -3.0));
    assert_eq!(zs.iter().filter(|&&z| z == -3.0).count(), 4);
}

/// The pin's open-profile refusal: the planar layer's `Open` comes through as a refusal.
#[test]
fn an_open_profile_is_refused() {
    let open = vec![
        ln([0.0, 0.0], [2.0, 0.0]),
        ln([2.0, 0.0], [2.0, 2.0]),
        ln([2.0, 2.0], [0.0, 2.5]),
    ];
    let e = MakeError::from(region(&[open], doc().resolution()).unwrap_err());
    assert!(
        matches!(
            e,
            MakeError::Refused(Refusal::Profile(PlanarError::Open {
                in_loop: LoopId::Outer,
                ..
            }))
        ),
        "{e:?}"
    );
    assert_eq!(e.kind(), ErrorKind::Refused);
    assert_eq!(e.name(), "profile");
    assert!(core::error::Error::source(&e).is_some());
}

/// The planar layer's undecided verdict is this build's limit, not the caller's fault.
#[test]
fn an_undecided_profile_is_unsupported() {
    let e = MakeError::from(PlanarError::Undecided {
        in_loop: LoopId::Hole(0),
    });
    assert!(matches!(e, MakeError::Unsupported(Unsupported::Profile(_))));
    assert_eq!(e.kind(), ErrorKind::Unsupported);
}

/// The pin's `an_uncapped_profile_with_a_short_segment_keeps_every_wall`, capped: a wall as
/// short as 3e-5 is kept, with its two rim edges that short.
#[test]
fn a_short_segment_keeps_its_wall() {
    for len in [3e-5, 1.7e-4, 3.9e-4] {
        let l = polygon(&[
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [len, 10.0],
            [0.0, 10.0],
        ]);
        let m = make(&[l], &plain(), [0.0, 1.2]);
        assert_eq!(counts(&m.body), (7, 15, 10), "{len}");
        let short = m
            .body
            .edges()
            .filter(|e| (e.end().point() - e.start().point()).length() < 2.0 * len)
            .count();
        assert_eq!(short, 2, "{len}");
        let (v, ve, ..) = volume_and_area(&m.body, Point3::ORIGIN, 1e-13);
        assert!(
            rel(v, 120.0) < 1e-13 && ve < 1e-13 * 120.0,
            "{v} (err {ve:e})"
        );
    }
}

/// The pin's rounded rectangle, undrafted: ten faces, every corner wall kept, and the
/// closed-form volume `h (2a 2b - (4 - pi) r^2)`.
#[test]
fn a_rounded_rectangle_keeps_every_corner_wall() {
    let (a, b, r) = (111.48254626441849, 41.48254626441848, 8.982546264418483);
    for h in [2.0, 2.01, 2.5] {
        let m = make(&[rounded(a, b, r)], &plain(), [0.0, h]);
        assert_eq!(counts(&m.body), (10, 24, 16));
        let (v, ve, ..) = volume_and_area(&m.body, Point3::ORIGIN, 1e-12);
        let want = h * (4.0 * a * b - (4.0 - PI) * r * r);
        assert!(
            rel(v, want) < 1e-12 && ve < 1e-12 * want,
            "{v} (err {ve:e}) vs {want}"
        );
    }
}

/// T11: every profile fixture, at a document of its resolution: an accepted one extrudes to
/// a closed body of volume `area h`; a refused one is refused with the planar cause.
#[test]
fn every_profile_fixture_comes_through() {
    let mut bad = Vec::new();
    for f in fixtures() {
        let d = Document::with_resolution(res(f.resolution));
        let mut loops = vec![f.outer.clone()];
        loops.extend(f.holes.iter().cloned());
        let got = region(&loops, d.resolution()).map_err(MakeError::from);
        match (&f.verdict, got) {
            (Verdict::Accepted { area, .. }, Ok(r)) => {
                match extrude(&r, &plain(), Span::new(mm(0.0), mm(1.5)), &d) {
                    Ok(m) => {
                        assert_closed_solid(&m.body);
                        let (v, ve, ..) = volume_and_area(&m.body, Point3::ORIGIN, 1e-10);
                        if rel(v, area * 1.5) > 1e-10 || ve > 1e-10 * area * 1.5 {
                            bad.push(format!(
                                "{}: volume {v} (err {ve:e}), want {}",
                                f.name,
                                area * 1.5
                            ));
                        }
                    }
                    Err(e) => bad.push(format!("{}: {e}", f.name)),
                }
            }
            (Verdict::Refused(want), Err(MakeError::Refused(Refusal::Profile(e)))) => {
                let ok = matches!(
                    (want, e),
                    (Cause::Open { .. }, PlanarError::Open { .. })
                        | (Cause::Crossing, PlanarError::Crossing { .. })
                        | (Cause::Touching, PlanarError::Touching { .. })
                        | (Cause::HoleOutside { .. }, PlanarError::HoleOutside { .. })
                        | (Cause::HoleInHole { .. }, PlanarError::HoleInHole { .. })
                );
                if !ok {
                    bad.push(format!("{}: {e:?}, want {want:?}", f.name));
                }
            }
            (_, got) => bad.push(format!("{}: {:?}", f.name, got.map(|r| r.area()))),
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The heights must be finite and more than the resolution apart.
#[test]
fn bad_spans_are_refused() {
    let d = doc();
    let zone = d.resolution().zone().mm();
    let r = region(
        &[polygon(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]])],
        d.resolution(),
    )
    .unwrap();
    for (a, b) in [
        (0.0, 0.0),
        (1.0, 1.0 + 0.5 * zone),
        (0.0, f64::NAN),
        (f64::INFINITY, 1.0),
        (-f64::MAX, f64::MAX),
    ] {
        let e = extrude(&r, &plain(), Span::new(mm(a), mm(b)), &d).unwrap_err();
        assert!(
            matches!(e, MakeError::Refused(Refusal::Span { .. })),
            "{a} {b}: {e:?}"
        );
        assert_eq!(e.name(), "span");
    }
    assert!(extrude(&r, &plain(), Span::new(mm(1.0), mm(1.0 + 2.0 * zone)), &d).is_ok());
}

/// A region proved at another resolution is refused rather than trusted.
#[test]
fn a_region_at_another_resolution_is_refused() {
    let d = doc();
    let square = polygon(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]);
    let other = res(d.resolution().zone().mm() * 10.0);
    let r = region(&[square], other).unwrap();
    let e = extrude(&r, &plain(), Span::new(mm(0.0), mm(1.0)), &d).unwrap_err();
    assert_eq!(
        e,
        MakeError::Refused(Refusal::Resolution {
            region: other,
            document: d.resolution()
        })
    );
}

/// The same input gives the same body, bit for bit.
#[test]
fn extrusion_is_deterministic() {
    for (_, loops) in profiles() {
        let a = make(&loops, &tilted(), [0.5, -2.0]);
        let b = make(&loops, &tilted(), [0.5, -2.0]);
        assert_eq!(format!("{:?}", a.body), format!("{:?}", b.body));
        assert_eq!(a.faces, b.faces);
    }
}

/// A whole circle's rim edges run one turn, `[0, TAU]`, from the seam vertex.
#[test]
fn whole_circle_edges_run_one_turn() {
    let m = make(&[vec![circle([1.0, 1.0], 0.5, true)]], &plain(), [0.0, 1.0]);
    let mut turns = 0;
    for e in m.body.edges() {
        if let EdgeShape::Curve {
            curve: Curve::Circle(_),
            interval,
        } = e.shape()
        {
            assert_eq!((interval.min(), interval.max()), (0.0, TAU));
            assert_eq!(e.start().id(), e.end().id());
            assert_eq!(
                e.start().point(),
                Point3::new(1.5, 1.0, e.start().point().z)
            );
            turns += 1;
        }
    }
    assert_eq!(turns, 2);
}
