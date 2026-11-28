// Written for solidcorex. Apache-2.0.
//! `prism` and `sheet` (research 75 MK14): the pin's compound-extrusion truths
//! (`compound_planar_extrusion_normalizes_overlap_and_retains_islands`) and sheet truths
//! (`sheets_combined_read_back_as_their_regions`), judged against closed forms: volume
//! `sum A h`, area `sum (2 A + P h)`, and the faces, edges and vertices counted from the
//! segments (a prism has `2 + n` faces, `3 n` edges and `2 n` vertices for each region of
//! `n` segments in all its loops; a sheet `1`, `n` and `n`).
//!
//! Removed on purpose, from the pin's tests:
//! - `compound_planar_extrusion_normalizes_overlap_and_retains_islands`'s overlap half (two
//!   overlapping rectangles extrude to the union's volume 300) and its two overlapping
//!   circles `is_err()`: the pin unites regions in the plane first; here no planar Boolean
//!   exists yet, so overlapping regions are `Unsupported(RegionsMeet)`, tested below. The
//!   island halves (a disc in a plate's material is refused to merge; a disc in an
//!   annulus's hole is kept, two solids) and the touching-corner refusal are kept.
//! - `sheets_combined_read_back_as_their_regions` (reading a sheet back as regions) is the
//!   reader's (`scx-bodyq`, step 6), and the unions it combines are the planar Boolean's
//!   (BO17); what is kept here is the sheet's construction and its areas.
//! - `a_sheet_seen_from_below_starts_its_arcs_in_range` and `a_box_is_a_prism_along_each_of_its_axes`
//!   test the reader and the prism recogniser (`scx-bodyq`).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use core::f64::consts::PI;

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, FaceSide};
use scx_make::{
    ExtrudeFaces, MakeError, PrismFaces, Refusal, Span, Unsupported, extrude, prism, sheet,
};
use scx_mass::{MassSpec, RelAccuracy, mass};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{Region, Segment};
use scx_testkit::profile::Seg;
use scx_testkit::solid;

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn plain() -> Frame {
    Frame::new(
        Point3::ORIGIN,
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

fn tilted() -> Frame {
    Frame::new(
        Point3::new(1250.0, -2125.0, 600.0),
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(-3.0, 0.5, 1.0),
    )
    .unwrap()
}

fn doc() -> Document {
    Document::default()
}

/// A region as truth data (loops of `Seg`, outer counter-clockwise, holes clockwise) and
/// the same as the planar layer's.
#[derive(Clone)]
struct Shape {
    loops: Vec<Vec<Seg>>,
}

impl Shape {
    fn segments(&self) -> usize {
        self.loops.iter().map(Vec::len).sum()
    }

    fn region(&self) -> Region {
        let seg = |s: &Seg| match *s {
            Seg::Line { start, end } => Segment::line(Point2::from(start), Point2::from(end)),
            Seg::Arc {
                centre,
                start,
                end,
                ccw,
            } => {
                let sense = if ccw { Sense::Forward } else { Sense::Reversed };
                if start == end {
                    let r = scx_num::fmath::hypot(start[0] - centre[0], start[1] - centre[1]);
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
        };
        Region::new(
            self.loops[0].iter().map(seg).collect(),
            self.loops[1..]
                .iter()
                .map(|h| h.iter().map(seg).collect())
                .collect(),
            doc().resolution(),
        )
        .unwrap()
    }
}

fn disc(c: [f64; 2], r: f64) -> Vec<Seg> {
    let s = [c[0] + r, c[1]];
    vec![Seg::Arc {
        centre: c,
        start: s,
        end: s,
        ccw: true,
    }]
}

fn hole(c: [f64; 2], r: f64) -> Vec<Seg> {
    let s = [c[0] + r, c[1]];
    vec![Seg::Arc {
        centre: c,
        start: s,
        end: s,
        ccw: false,
    }]
}

fn rect(a: [f64; 2], b: [f64; 2]) -> Vec<Seg> {
    let pts = [a, [b[0], a[1]], b, [a[0], b[1]]];
    pts.iter()
        .zip(pts.iter().cycle().skip(1))
        .map(|(&s, &e)| Seg::Line { start: s, end: e })
        .collect()
}

fn square(a: [f64; 2], b: [f64; 2]) -> Shape {
    Shape {
        loops: vec![rect(a, b)],
    }
}

fn circle_shape(c: [f64; 2], r: f64) -> Shape {
    Shape {
        loops: vec![disc(c, r)],
    }
}

fn regions(shapes: &[Shape]) -> Vec<Region> {
    shapes.iter().map(Shape::region).collect()
}

/// A plate with a round hole, a disc island in the hole, a disc beside the plate and a
/// triangle with a slot hole far off.
fn field() -> Vec<Shape> {
    let triangle = vec![
        Seg::Line {
            start: [30.0, 0.0],
            end: [40.0, 0.0],
        },
        Seg::Line {
            start: [40.0, 0.0],
            end: [30.0, 8.0],
        },
        Seg::Line {
            start: [30.0, 8.0],
            end: [30.0, 0.0],
        },
    ];
    vec![
        Shape {
            loops: vec![rect([0.0, 0.0], [10.0, 10.0]), hole([5.0, 5.0], 3.0)],
        },
        circle_shape([5.0, 5.0], 2.0),
        circle_shape([20.0, 5.0], 4.0),
        Shape {
            loops: vec![triangle],
        },
    ]
}

fn closed_form(shapes: &[Shape], h: f64) -> (f64, f64) {
    shapes.iter().fold((0.0, 0.0), |(v, a), s| {
        let t = solid::extrusion(&s.loops, h).unwrap();
        (v + t.volume(), a + t.area)
    })
}

fn volume_area(b: &Brep, about: Point3) -> (f64, f64) {
    let spec = MassSpec::new(RelAccuracy::new(1e-12).unwrap()).with_about(about);
    let m = mass(
        b,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    (m.volume, m.area)
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

/// Every edge has two fins of opposite senses, every face is outside in front and solid
/// behind, and there is one solid per region, each in a shell of its own.
fn assert_solids(b: &Brep, n: usize) {
    // The outside region, and a solid region with its own shell for each region, each solid
    // with a shell of its own in the outside.
    assert_eq!(b.regions().len(), 1 + n);
    assert_eq!(b.shells().len(), 2 * n);
    for e in b.edges() {
        let f: Vec<_> = e.fins().collect();
        assert_eq!(f.len(), 2, "{:?}", e.id());
        assert_ne!(f[0].sense, f[1].sense, "{:?}", e.id());
    }
    for face in b.faces() {
        assert!(face.shell(FaceSide::Front).region().is_outside());
        assert!(face.shell(FaceSide::Back).region().is_solid());
    }
}

fn counts(b: &Brep) -> (usize, usize, usize) {
    (b.faces().len(), b.edges().len(), b.vertices().len())
}

#[test]
fn a_prism_of_several_regions_is_the_sum_of_theirs_in_closed_form() {
    let shapes = field();
    for frame in [plain(), tilted()] {
        for span in [[0.0_f64, 2.0], [3.0, -1.5]] {
            let h = (span[1] - span[0]).abs();
            let m = prism(
                &regions(&shapes),
                &frame,
                Span::new(mm(span[0]), mm(span[1])),
                &doc(),
            )
            .unwrap();
            assert_solids(&m.body, shapes.len());
            let (v, a) = closed_form(&shapes, h);
            let (gv, ga) = volume_area(&m.body, frame.origin());
            assert!(
                rel(gv, v) < 1e-12 && rel(ga, a) < 1e-12,
                "{gv} {v} {ga} {a}"
            );
            let n: usize = shapes.iter().map(Shape::segments).sum();
            assert_eq!(
                counts(&m.body),
                (2 * shapes.len() + n, 3 * n, 2 * n),
                "faces, edges, vertices"
            );
        }
    }
}

#[test]
fn the_volume_is_the_pins_island_arithmetic() {
    // The pin's island case: a 10 x 10 x 2 plate with a r3 hole and an r2 disc in the hole
    // (the plate's rectangle 10 x 10; the disc's own pi 4 x 2); its two solids.
    let shapes = vec![
        Shape {
            loops: vec![rect([0.0, 0.0], [10.0, 10.0]), hole([5.0, 5.0], 3.0)],
        },
        circle_shape([5.0, 5.0], 2.0),
    ];
    let m = prism(
        &regions(&shapes),
        &plain(),
        Span::new(mm(0.0), mm(2.0)),
        &doc(),
    )
    .unwrap();
    let want = (100.0 - 9.0 * PI) * 2.0 + 4.0 * PI * 2.0;
    let (v, _) = volume_area(&m.body, Point3::ORIGIN);
    assert!(rel(v, want) < 1e-12, "{v} {want}");
    assert_eq!(m.body.regions().len(), 3);
}

#[test]
fn each_solid_is_what_extrude_makes_of_its_region() {
    let shapes = field();
    let d = doc();
    let frame = tilted();
    let span = Span::new(mm(-1.0), mm(2.5));
    let all = prism(&regions(&shapes), &frame, span, &d).unwrap();
    assert_eq!(all.faces.solids.len(), shapes.len());
    for (s, solid_faces) in shapes.iter().zip(&all.faces.solids) {
        let one = extrude(&s.region(), &frame, span, &d).unwrap();
        // The same record shape: loops and segments as given, and the caps.
        let shape = |f: &ExtrudeFaces| f.walls.iter().map(Vec::len).collect::<Vec<_>>();
        assert_eq!(shape(solid_faces), shape(&one.faces));
        // Each wall is on the same surface as the lone extrusion's.
        for (w, v) in solid_faces
            .walls
            .iter()
            .flatten()
            .zip(one.faces.walls.iter().flatten())
        {
            let a = all.body.face(*w).unwrap();
            let b = one.body.face(*v).unwrap();
            assert_eq!(a.surface(), b.surface());
        }
        assert_eq!(
            all.body.face(solid_faces.start).unwrap().surface(),
            one.body.face(one.faces.start).unwrap().surface()
        );
    }
}

#[test]
fn one_region_is_the_extrusion_and_the_order_does_not_change_the_body_s_measure() {
    let d = doc();
    let shapes = field();
    let span = Span::new(mm(0.0), mm(3.0));
    let one = prism(&regions(&shapes[2..3]), &plain(), span, &d).unwrap();
    let lone = extrude(&shapes[2].region(), &plain(), span, &d).unwrap();
    assert_eq!(counts(&one.body), counts(&lone.body));
    assert_eq!(
        volume_area(&one.body, Point3::ORIGIN),
        volume_area(&lone.body, Point3::ORIGIN)
    );
    let mut flipped = regions(&shapes);
    flipped.reverse();
    let a = prism(&regions(&shapes), &plain(), span, &d).unwrap();
    let b = prism(&flipped, &plain(), span, &d).unwrap();
    assert_eq!(counts(&a.body), counts(&b.body));
    let (va, aa) = volume_area(&a.body, Point3::ORIGIN);
    let (vb, ab) = volume_area(&b.body, Point3::ORIGIN);
    assert!(rel(va, vb) < 1e-13 && rel(aa, ab) < 1e-13);
}

fn meets(a: &Shape, b: &Shape) -> Option<(usize, usize)> {
    let r = prism(
        &regions(&[a.clone(), b.clone()]),
        &plain(),
        Span::new(mm(0.0), mm(2.0)),
        &doc(),
    );
    let s = sheet(&regions(&[a.clone(), b.clone()]), &plain(), &doc());
    // A prism and a sheet of the same regions agree.
    assert_eq!(r.is_ok(), s.is_ok());
    match r {
        Ok(_) => None,
        Err(MakeError::Unsupported(Unsupported::RegionsMeet { first, second })) => {
            Some((first, second))
        }
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn regions_that_overlap_touch_or_cross_are_refused_by_index() {
    let a = square([0.0, 0.0], [10.0, 10.0]);
    // The pin's overlap case: they would merge to 300; here they are refused.
    assert_eq!(meets(&a, &square([5.0, 0.0], [15.0, 10.0])), Some((0, 1)));
    // The pin's touching corner (refused there too), along an edge, and at a vertex on an edge.
    assert_eq!(meets(&a, &square([10.0, 10.0], [20.0, 20.0])), Some((0, 1)));
    assert_eq!(meets(&a, &square([10.0, 2.0], [20.0, 6.0])), Some((0, 1)));
    assert_eq!(meets(&a, &square([10.0, -5.0], [20.0, 5.0])), Some((0, 1)));
    // One inside the other's material, either way round.
    let inner = circle_shape([5.0, 5.0], 2.0);
    assert_eq!(meets(&a, &inner), Some((0, 1)));
    assert_eq!(meets(&inner, &a), Some((0, 1)));
    // The pin's two overlapping circles, and two circles touching at a point.
    assert_eq!(
        meets(
            &circle_shape([0.0, 0.0], 5.0),
            &circle_shape([8.0, 0.0], 5.0)
        ),
        Some((0, 1))
    );
    assert_eq!(
        meets(
            &circle_shape([0.0, 0.0], 5.0),
            &circle_shape([10.0, 0.0], 5.0)
        ),
        Some((0, 1))
    );
    // The first pair, by index, of three.
    let three = [
        square([0.0, 0.0], [1.0, 1.0]),
        square([5.0, 0.0], [6.0, 1.0]),
        square([5.5, 0.5], [7.0, 2.0]),
    ];
    let e = prism(
        &regions(&three),
        &plain(),
        Span::new(mm(0.0), mm(1.0)),
        &doc(),
    )
    .unwrap_err();
    assert_eq!(
        e,
        MakeError::Unsupported(Unsupported::RegionsMeet {
            first: 1,
            second: 2
        })
    );
    assert_eq!(e.name(), "regions_meet");
}

#[test]
fn regions_apart_are_kept_beside_and_in_holes() {
    let a = square([0.0, 0.0], [10.0, 10.0]);
    // A gap of a tenth of a millimetre is apart; one a thousandth of the resolution is not.
    assert_eq!(meets(&a, &square([10.1, 0.0], [20.0, 10.0])), None);
    let zone = doc().resolution().zone().mm();
    assert_eq!(
        meets(&a, &square([10.0 + zone / 1000.0, 0.0], [20.0, 10.0])),
        Some((0, 1))
    );
    // In a hole, with room, either order.
    let ring = Shape {
        loops: vec![rect([0.0, 0.0], [10.0, 10.0]), hole([5.0, 5.0], 3.0)],
    };
    let island = circle_shape([5.0, 5.0], 2.0);
    assert_eq!(meets(&ring, &island), None);
    assert_eq!(meets(&island, &ring), None);
    // Touching the hole's wall is still touching.
    assert_eq!(meets(&ring, &circle_shape([5.0, 5.0], 3.0)), Some((0, 1)));
}

#[test]
fn a_prism_refuses_what_extrude_refuses_and_no_regions() {
    let d = doc();
    let span = Span::new(mm(0.0), mm(1.0));
    let e = prism(&[], &plain(), span, &d).unwrap_err();
    assert_eq!(e, MakeError::Refused(Refusal::NoRegions));
    assert_eq!(e.name(), "no_regions");
    let e = sheet(&[], &plain(), &d).unwrap_err();
    assert_eq!(e, MakeError::Refused(Refusal::NoRegions));
    let r = regions(&field());
    for bad in [
        Span::new(mm(0.0), mm(0.0)),
        Span::new(mm(0.0), mm(f64::NAN)),
        Span::new(mm(f64::INFINITY), mm(1.0)),
    ] {
        assert!(matches!(
            prism(&r, &plain(), bad, &d),
            Err(MakeError::Refused(Refusal::Span { .. }))
        ));
    }
    // A region at another resolution, wherever it stands in the list.
    let fine = Document::with_resolution(Resolution::new(mm(1e-6)).unwrap());
    let other = field()[0].region();
    let mixed = [
        other.clone(),
        Shape::region(&circle_shape([50.0, 0.0], 1.0)),
    ];
    let odd = Region::new(mixed_segments(&mixed[1]), vec![], fine.resolution()).unwrap();
    for list in [vec![other.clone(), odd.clone()], vec![odd.clone(), other]] {
        assert!(matches!(
            prism(&list, &plain(), span, &d),
            Err(MakeError::Refused(Refusal::Resolution { .. }))
        ));
        assert!(matches!(
            sheet(&list, &plain(), &d),
            Err(MakeError::Refused(Refusal::Resolution { .. }))
        ));
    }
}

fn mixed_segments(r: &Region) -> Vec<Segment> {
    r.outer().segments().to_vec()
}

#[test]
fn a_sheet_has_a_face_for_each_region_and_the_area_of_their_sum() {
    let shapes = field();
    for frame in [plain(), tilted()] {
        let m = sheet(&regions(&shapes), &frame, &doc()).unwrap();
        let b = &m.body;
        let area: f64 = shapes
            .iter()
            .map(|s| solid::profile_area(&s.loops).unwrap())
            .sum();
        let (v, a) = volume_area(b, frame.origin());
        // A sheet's face counts once, so its area is the regions' area; it holds no volume.
        assert!(v.abs() < 1e-12 && rel(a, area) < 1e-12, "{v} {a} {area}");
        let n: usize = shapes.iter().map(Shape::segments).sum();
        assert_eq!(counts(b), (shapes.len(), n, n));
        assert_eq!(m.faces.faces.len(), shapes.len());
        // The outside region alone, a shell for each face with both sides in it.
        assert_eq!(b.regions().len(), 1);
        assert_eq!(b.shells().len(), shapes.len());
        for e in b.edges() {
            assert_eq!(
                e.fins().count(),
                1,
                "a sheet edge is a border: {:?}",
                e.id()
            );
        }
        for (f, region) in m.faces.faces.iter().zip(&shapes) {
            let face = b.face(*f).unwrap();
            assert!(face.shell(FaceSide::Front).region().is_outside());
            assert_eq!(
                face.shell(FaceSide::Front).id(),
                face.shell(FaceSide::Back).id()
            );
            assert_eq!(face.loops().count(), region.loops.len());
        }
    }
}

#[test]
fn a_sheet_s_front_is_the_frame_s_z_side() {
    let frame = tilted();
    let m = sheet(&regions(&field()), &frame, &doc()).unwrap();
    for f in &m.faces.faces {
        let scx_surface::Surface::Plane(pl) = m.body.face(*f).unwrap().surface() else {
            panic!("not a plane");
        };
        let (n, z) = (*pl.frame().z(), *frame.z());
        assert!((n - z).length() < 1e-15, "{n:?} {z:?}");
        // The plane passes through the frame's origin.
        assert!(((frame.origin() - pl.frame().origin()).dot(z)).abs() < 1e-9);
    }
}

#[test]
fn a_sheet_of_one_region_matches_a_prism_s_cap() {
    let d = doc();
    let s = Shape {
        loops: vec![rect([0.0, 0.0], [10.0, 10.0]), hole([5.0, 5.0], 3.0)],
    };
    let sh = sheet(&[s.region()], &plain(), &d).unwrap();
    let (_, a) = volume_area(&sh.body, Point3::ORIGIN);
    assert!(rel(a, 100.0 - 9.0 * PI) < 1e-12);
    // 2 loops: 4 + 1 segments; 5 edges, 5 vertices.
    assert_eq!(counts(&sh.body), (1, 5, 5));
}

#[test]
fn prism_faces_record_is_by_region() {
    let shapes = field();
    let m = prism(
        &regions(&shapes),
        &plain(),
        Span::new(mm(0.0), mm(1.0)),
        &doc(),
    )
    .unwrap();
    let PrismFaces { solids } = &m.faces;
    for (s, f) in shapes.iter().zip(solids) {
        assert_eq!(f.walls.len(), s.loops.len());
        for (w, l) in f.walls.iter().zip(&s.loops) {
            assert_eq!(w.len(), l.len());
        }
        // Each solid's faces are in the solid region of that region alone.
        let shell = m.body.face(f.start).unwrap().shell(FaceSide::Back).id();
        for id in f.walls.iter().flatten().chain([&f.end]) {
            assert_eq!(m.body.face(*id).unwrap().shell(FaceSide::Back).id(), shell);
        }
    }
    let shells: std::collections::BTreeSet<_> = solids
        .iter()
        .map(|f| m.body.face(f.start).unwrap().shell(FaceSide::Back).id())
        .collect();
    assert_eq!(shells.len(), shapes.len());
}
