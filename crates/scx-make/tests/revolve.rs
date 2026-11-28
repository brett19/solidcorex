// Written for solidcorex. Apache-2.0.
//! `revolve` and `revolve_sheet` (research 75 MK4): lines and arcs turned about the frame's
//! y axis, full and partial, judged against closed forms (Pappus's volume and area), the
//! face kinds each segment sweeps, and the closed-solid structure with the BR8 oracle.
//!
//! Volume and area are read by `scx_mass::mass` and compared to the closed forms; each
//! tolerance is also checked against the estimate's error bound.
//!
//! The axis-touch cases: a segment on the axis sweeps nothing, a vertex on it is a pole, and
//! an arc touching it inside itself (WR-590's circle, a horn torus) is split or re-seamed at
//! the touch. Only a true crossing is refused.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use core::f64::consts::{PI, TAU};

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, EdgeShape, FaceId, FaceSide};
use scx_make::{MakeError, Refusal, RevolveFaces, Turn, Unsupported, Wall, revolve, revolve_sheet};
use scx_mass::{Mass, MassSpec, RelAccuracy, mass};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{Region, Segment};
use scx_surface::Surface;
use scx_testkit::bodies::check::{Tolerance, violations};
use scx_testkit::profile::Seg;

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
        Document::default().resolution(),
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

/// A circle starting at `p` (on it) about `c`.
fn circle_at(c: [f64; 2], p: [f64; 2], ccw: bool) -> Seg {
    arc(c, p, p, ccw)
}

fn polygon(p: &[[f64; 2]]) -> Vec<Seg> {
    p.iter()
        .zip(p.iter().cycle().skip(1))
        .map(|(&a, &b)| ln(a, b))
        .collect()
}

/// The `x` mirror of a loop (the profile on the axis's other side), still counter-clockwise.
fn mirrored(l: &[Seg]) -> Vec<Seg> {
    let m = |p: [f64; 2]| [-p[0], p[1]];
    l.iter()
        .rev()
        .map(|s| match *s {
            Seg::Line { start, end } => ln(m(end), m(start)),
            Seg::Arc {
                centre,
                start,
                end,
                ccw,
            } => arc(m(centre), m(end), m(start), ccw),
        })
        .collect()
}

fn make(loops: &[Vec<Seg>], frame: &Frame, turn: Turn) -> (Brep, RevolveFaces) {
    let m = revolve(&region(loops), frame, turn, &Document::default()).unwrap();
    (m.body, m.faces)
}

fn quarter(from: f64) -> Turn {
    Turn::Partial {
        from,
        to: from + PI / 2.0,
    }
}

/// The fraction of a whole turn.
fn fraction(t: Turn) -> f64 {
    match t {
        Turn::Full => 1.0,
        Turn::Partial { from, to } => (to - from) / TAU,
    }
}

fn collapsed(b: &Brep) -> usize {
    b.edges()
        .filter(|e| *e.shape() == EdgeShape::Collapsed)
        .count()
}

/// Two regions with a shell each, every curve edge with two fins of opposite senses and every
/// collapsed one with one, every face's front in the outside's shell, every pcurve exact, the
/// Euler-Poincare count of a closed surface of `genus` over the curve edges, and the BR8
/// oracle silent.
fn assert_closed_solid(b: &Brep, genus: i64) {
    assert_closed_with(b, 2 - 2 * genus, [2, 2], Tolerance::default());
}

/// [`assert_closed_solid`] for `regions` and `shells` counts (a void is a region and a shell
/// with the solid's shell round it), the Euler-Poincare count `chi` summed over the shells,
/// and the BR8 oracle at `tol`. Every face's front is in a non-solid region, its back in
/// the solid.
fn assert_closed_with(b: &Brep, chi: i64, [regions, shells]: [usize; 2], tol: Tolerance) {
    assert_eq!(b.regions().len(), regions);
    assert_eq!(b.shells().len(), shells);
    for e in b.edges() {
        let f: Vec<_> = e.fins().collect();
        if *e.shape() == EdgeShape::Collapsed {
            assert_eq!(f.len(), 1, "{:?}", e.id());
        } else {
            assert_eq!(f.len(), 2, "{:?}", e.id());
            assert_ne!(f[0].sense, f[1].sense, "{:?}", e.id());
        }
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
    let e = e - collapsed(b);
    let euler = v as i64 - e as i64 + f as i64 - (loops as i64 - f as i64);
    assert_eq!(euler, chi, "V - E + F - (L - F)");
    let bad = violations(&b.edit(), tol);
    assert!(bad.is_empty(), "{bad:?}");
}

/// A pinched body (a vertex shared by faces that meet nowhere else): the structure of
/// [`assert_closed_solid`] without the Euler count.
fn assert_valid(b: &Brep) {
    let bad = violations(&b.edit(), Tolerance::default());
    assert!(bad.is_empty(), "{bad:?}");
    for face in b.faces() {
        for u in face.uses() {
            assert!(u.bound().is_none(), "{:?}", face.id());
        }
    }
}

fn measure(b: &Brep, o: Point3) -> Mass {
    let spec = MassSpec::new(RelAccuracy::new(1e-12).unwrap()).with_about(o);
    mass(
        b,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

fn assert_within(got: f64, error: f64, want: f64, tol: f64, what: &str) {
    let allowed = tol * want.abs();
    assert!(
        (got - want).abs() <= allowed && error <= allowed,
        "{what}: {got:e} (estimated error {error:e}) vs {want:e}, allowed {allowed:e}"
    );
}

/// Volume and area of `b` against `[volume, area]`.
fn assert_mass(b: &Brep, frame: &Frame, want: [f64; 2], what: &str) {
    let m = measure(b, frame.origin());
    let tol = 1e-9;
    assert_within(
        m.volume,
        m.error.volume,
        want[0],
        tol,
        &format!("{what}: volume"),
    );
    assert_within(m.area, m.error.area, want[1], tol, &format!("{what}: area"));
}

/// Pappus for a profile of area `a`, centroid radius `ra`, perimeter swept `l` at centroid
/// radius `rl`, turned by `t`: the caps of a partial turn add `2a`.
fn pappus(a: f64, ra: f64, l: f64, rl: f64, t: Turn) -> [f64; 2] {
    let f = fraction(t);
    let caps = if f < 1.0 { 2.0 * a } else { 0.0 };
    [TAU * ra * a * f, TAU * rl * l * f + caps]
}

/// What kind of surface a face lies on.
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

fn wall_kinds(b: &Brep, walls: &[Wall]) -> Vec<&'static str> {
    walls
        .iter()
        .map(|w| match *w {
            Wall::OnAxis => "axis",
            Wall::Face(f) => kind(b, f),
            Wall::Split { before, after } => {
                assert_eq!(kind(b, before), kind(b, after));
                kind(b, before)
            }
        })
        .collect()
}

fn turns() -> [Turn; 4] {
    [
        Turn::Full,
        quarter(0.0),
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

#[test]
fn a_rectangle_off_the_axis_sweeps_a_tube() {
    // x in [2, 5], y in [0, 4]: two planes, two cylinders.
    let (a, b, h) = (2.0, 5.0, 4.0);
    let p = polygon(&[[a, 0.0], [b, 0.0], [b, h], [a, h]]);
    for frame in [plain(), tilted()] {
        for t in turns() {
            let (body, faces) = make(core::slice::from_ref(&p), &frame, t);
            let area = (b - a) * h;
            let perim_moment = 2.0 * (b - a) * (a + b) / 2.0 + h * a + h * b;
            let want = pappus(area, (a + b) / 2.0, 1.0, perim_moment, t);
            assert_mass(&body, &frame, want, &format!("tube {t:?}"));
            assert_eq!(
                wall_kinds(&body, &faces.walls[0]),
                ["plane", "cylinder", "plane", "cylinder"]
            );
            let partial = t != Turn::Full;
            assert_eq!(faces.start.is_some(), partial);
            assert_eq!(faces.end.is_some(), partial);
            assert_eq!(body.faces().len(), if partial { 6 } else { 4 });
            assert_closed_solid(&body, i64::from(!partial));
        }
    }
}

#[test]
fn the_mirrored_side_turns_the_same_solid_the_other_way() {
    let (a, b, h) = (2.0, 5.0, 4.0);
    let p = mirrored(&polygon(&[[a, 0.0], [b, 0.0], [b, h], [a, h]]));
    for t in turns() {
        let (body, faces) = make(core::slice::from_ref(&p), &tilted(), t);
        let perim_moment = 2.0 * (b - a) * (a + b) / 2.0 + h * a + h * b;
        let want = pappus((b - a) * h, (a + b) / 2.0, 1.0, perim_moment, t);
        assert_mass(&body, &tilted(), want, &format!("mirrored {t:?}"));
        assert_closed_solid(&body, i64::from(t == Turn::Full));
        assert_eq!(faces.walls[0].len(), 4);
    }
}

#[test]
fn a_segment_on_the_axis_sweeps_nothing() {
    // A solid cylinder: its axis segment is no face; its end discs have a pole each when
    // whole and none when partial (a plane needs no pole edge).
    let (r, h) = (3.0, 7.0);
    let p = polygon(&[[0.0, 0.0], [r, 0.0], [r, h], [0.0, h]]);
    for frame in [plain(), tilted()] {
        for t in turns() {
            let (body, faces) = make(core::slice::from_ref(&p), &frame, t);
            let want = pappus(r * h, r / 2.0, 1.0, r * r / 2.0 * 2.0 + h * r, t);
            assert_mass(&body, &frame, want, &format!("cylinder {t:?}"));
            assert_eq!(
                wall_kinds(&body, &faces.walls[0]),
                ["plane", "cylinder", "plane", "axis"]
            );
            assert_closed_solid(&body, 0);
        }
    }
}

#[test]
fn a_cone_and_a_frustum_have_their_closed_forms() {
    // A cone to an apex on the axis (a pole), and a frustum between radii 2 and 5.
    let (r, h) = (4.0, 6.0);
    let cone = polygon(&[[0.0, 0.0], [r, 0.0], [0.0, h]]);
    let slant = fmath::hypot(r, h);
    for t in turns() {
        let (body, faces) = make(core::slice::from_ref(&cone), &tilted(), t);
        // The cone: V = pi r^2 h / 3, the slant's centroid at r / 2.
        let want = pappus(r * h / 2.0, r / 3.0, 1.0, r * r / 2.0 + slant * r / 2.0, t);
        assert_mass(&body, &tilted(), want, &format!("cone {t:?}"));
        assert_eq!(
            wall_kinds(&body, &faces.walls[0]),
            ["plane", "cone", "axis"]
        );
        assert_closed_solid(&body, 0);
        if t == Turn::Full {
            assert_eq!(collapsed(&body), 1, "the apex");
        }
    }
    let (r1, r2) = (5.0, 2.0);
    let frustum = polygon(&[[0.0, 0.0], [r1, 0.0], [r2, h], [0.0, h]]);
    let slant = fmath::hypot(r1 - r2, h);
    let volume = PI * h * (r1 * r1 + r1 * r2 + r2 * r2) / 3.0;
    let area = PI * (r1 * r1 + r2 * r2 + (r1 + r2) * slant);
    let (body, faces) = make(&[frustum], &plain(), Turn::Full);
    assert_mass(&body, &plain(), [volume, area], "frustum");
    assert_eq!(
        wall_kinds(&body, &faces.walls[0]),
        ["plane", "cone", "plane", "axis"]
    );
    assert_closed_solid(&body, 0);
}

#[test]
fn a_half_disc_on_the_axis_sweeps_a_sphere() {
    let r = 3.0;
    let p = vec![
        arc([0.0, 1.0], [0.0, 1.0 - r], [0.0, 1.0 + r], true),
        ln([0.0, 1.0 + r], [0.0, 1.0 - r]),
    ];
    for frame in [plain(), tilted()] {
        for t in turns() {
            let (body, faces) = make(core::slice::from_ref(&p), &frame, t);
            let f = fraction(t);
            let caps = if f < 1.0 { PI * r * r } else { 0.0 };
            let want = [
                4.0 / 3.0 * PI * (r * r * r) * f,
                4.0 * PI * r * r * f + caps,
            ];
            assert_mass(&body, &frame, want, &format!("sphere {t:?}"));
            assert_eq!(wall_kinds(&body, &faces.walls[0]), ["sphere", "axis"]);
            assert_closed_solid(&body, 0);
        }
    }
}

#[test]
fn a_circle_off_the_axis_sweeps_a_ring_torus() {
    let (big, r) = (6.0, 2.0);
    let p = vec![circle([big, 1.0], r, true)];
    for frame in [plain(), tilted()] {
        for t in turns() {
            let (body, faces) = make(core::slice::from_ref(&p), &frame, t);
            let want = pappus(PI * r * r, big, TAU * r, big, t);
            assert_mass(&body, &frame, want, &format!("ring torus {t:?}"));
            assert_eq!(wall_kinds(&body, &faces.walls[0]), ["torus"]);
            assert_closed_solid(&body, i64::from(t == Turn::Full));
        }
    }
}

#[test]
fn a_profile_with_holes_sweeps_their_walls_inward() {
    // A rectangle with a round hole and a square one.
    let outer = polygon(&[[2.0, 0.0], [14.0, 0.0], [14.0, 8.0], [2.0, 8.0]]);
    let round = vec![circle([5.0, 4.0], 2.0, false)];
    let square = polygon(&[[9.0, 2.0], [9.0, 5.0], [12.0, 5.0], [12.0, 2.0]]);
    let a = 12.0 * 8.0 - PI * 4.0 - 9.0;
    let moment = 12.0 * 8.0 * 8.0 - PI * 4.0 * 5.0 - 9.0 * 10.5;
    let l_moment = 2.0 * 12.0 * 8.0 + 8.0 * 2.0 + 8.0 * 14.0 // the outer rectangle
        + TAU * 2.0 * 5.0 // the round hole
        + 2.0 * 3.0 * 10.5 + 3.0 * 9.0 + 3.0 * 12.0; // the square
    for t in turns() {
        let (body, faces) = make(
            &[outer.clone(), round.clone(), square.clone()],
            &tilted(),
            t,
        );
        let want = pappus(a, moment / a, 1.0, l_moment, t);
        assert_mass(&body, &tilted(), want, &format!("holes {t:?}"));
        assert_eq!(faces.walls.len(), 3);
        assert_eq!(wall_kinds(&body, &faces.walls[1]), ["torus"]);
        if let Some(s) = faces.start {
            assert_eq!(body.face(s).unwrap().loops().len(), 3);
        }
        if t == Turn::Full {
            // A tube with two hollow rings in it: the outside, the solid and two voids; the
            // outside's and the solid's shells and each void's own shell and lining; three tori.
            assert_closed_with(&body, 0, [4, 6], Tolerance::default());
        } else {
            // Each hole's walls join the caps: one shell, genus two.
            assert_closed_solid(&body, 2);
        }
    }
}

#[test]
fn near_flat_and_near_upright_lines_snap_to_planes_and_cylinders() {
    // Each end is moved off a flat or upright line by a fraction of the resolution.
    let e = Document::default().resolution().zone().mm() * 0.25;
    let p = polygon(&[[2.0, 0.0], [5.0, e], [5.0 + e, 4.0], [2.0, 4.0 - e]]);
    let (body, faces) = make(&[p], &plain(), Turn::Full);
    assert_eq!(
        wall_kinds(&body, &faces.walls[0]),
        ["plane", "cylinder", "plane", "cylinder"]
    );
    // The snapped walls pass within the resolution of the given vertices, not exactly
    // through them: the oracle reads at the resolution.
    let zone = Document::default().resolution().zone().mm();
    let tol = Tolerance {
        space: zone,
        uv: zone,
    };
    assert_closed_with(&body, 0, [2, 2], tol);
}

#[test]
fn wr590_a_circle_touching_the_axis_is_a_horn_torus() {
    // WR-590: a circle of centre (5, 0) and radius 5 touching the axis at the origin, turned
    // a quarter, a half and three quarters from three starts; the pin split it at the touch
    // and made a valid solid of three faces (wall, two caps) and volume turn 2 pi 5 pi 25.
    let p = vec![circle_at([5.0, 0.0], [10.0, 0.0], true)];
    for frame in [plain(), tilted()] {
        for turn in [0.25, 0.5, 0.75] {
            for from in [0.0, PI / 2.0, PI] {
                let t = Turn::Partial {
                    from,
                    to: from + turn * TAU,
                };
                let (body, faces) = make(core::slice::from_ref(&p), &frame, t);
                assert_eq!(body.faces().len(), 3, "{t:?}");
                assert_eq!(wall_kinds(&body, &faces.walls[0]), ["torus"]);
                let want = pappus(PI * 25.0, 5.0, TAU * 5.0, 5.0, t);
                assert_mass(&body, &frame, want, &format!("wr590 {t:?}"));
                assert_valid(&body);
            }
        }
        // The whole turn: one face pinched at the origin.
        let (body, _) = make(core::slice::from_ref(&p), &frame, Turn::Full);
        assert_eq!(body.faces().len(), 1);
        let want = pappus(PI * 25.0, 5.0, TAU * 5.0, 5.0, Turn::Full);
        assert_mass(&body, &frame, want, "wr590 full");
        assert_valid(&body);
    }
}

#[test]
fn an_arc_touching_the_axis_inside_itself_is_split_there() {
    // The left half of a circle of centre (2, 0) radius 2, through the origin, closed by its
    // diameter x = 2: a horn torus's inner half and a cylinder.
    let r = 2.0;
    let p = vec![arc([r, 0.0], [r, r], [r, -r], true), ln([r, -r], [r, r])];
    for t in turns() {
        let (body, faces) = make(core::slice::from_ref(&p), &tilted(), t);
        assert!(matches!(faces.walls[0][0], Wall::Split { .. }));
        assert_eq!(wall_kinds(&body, &faces.walls[0]), ["torus", "cylinder"]);
        let a = PI * r * r / 2.0;
        let ra = r - 4.0 * r / (3.0 * PI);
        let l_moment = PI * r * (r - 2.0 * r / PI) + 2.0 * r * r;
        let want = pappus(a, ra, 1.0, l_moment, t);
        assert_mass(&body, &tilted(), want, &format!("split {t:?}"));
        assert_valid(&body);
    }
}

#[test]
fn a_sheet_has_the_walls_alone() {
    let p = polygon(&[[2.0, 0.0], [5.0, 0.0], [5.0, 4.0], [2.0, 4.0]]);
    for t in [Turn::Full, quarter(0.3)] {
        let m = revolve_sheet(
            &region(core::slice::from_ref(&p)),
            &tilted(),
            t,
            &Document::default(),
        )
        .unwrap();
        assert_eq!(m.body.faces().len(), 4);
        assert_eq!(m.faces.walls[0].len(), 4);
        assert_eq!(m.body.regions().len(), 1);
        for face in m.body.faces() {
            assert_eq!(
                face.shell(FaceSide::Front).id(),
                face.shell(FaceSide::Back).id()
            );
        }
        let bad = violations(&m.body.edit(), Tolerance::default());
        assert!(bad.is_empty(), "{bad:?}");
    }
}

#[test]
fn a_profile_across_the_axis_is_refused() {
    let p = polygon(&[[-1.0, 0.0], [5.0, 0.0], [5.0, 4.0], [-1.0, 4.0]]);
    let r = revolve(&region(&[p]), &plain(), Turn::Full, &Document::default());
    assert!(
        matches!(r, Err(MakeError::Refused(Refusal::CrossesAxis { .. }))),
        "{r:?}"
    );
    // An arc bulging across the axis between ends on its near side.
    let p = vec![
        arc([1.0, 0.0], [1.0, 2.0], [1.0, -2.0], true),
        ln([1.0, -2.0], [1.0, 2.0]),
    ];
    let r = revolve(&region(&[p]), &plain(), Turn::Full, &Document::default());
    assert!(
        matches!(r, Err(MakeError::Refused(Refusal::CrossesAxis { .. }))),
        "{r:?}"
    );
}

#[test]
fn an_arc_whose_circle_crosses_the_axis_outside_it_is_unsupported() {
    // The right part of a circle of centre (1, 0) radius 2: the circle crosses x = 0 at
    // y = +-sqrt 3, outside the arc, so its surface is a spindle torus.
    let p = vec![
        arc([1.0, 0.0], [1.0, -2.0], [1.0, 2.0], true),
        ln([1.0, 2.0], [1.0, -2.0]),
    ];
    let r = revolve(&region(&[p]), &plain(), Turn::Full, &Document::default());
    assert!(
        matches!(
            r,
            Err(MakeError::Unsupported(Unsupported::SpindleTorus { .. }))
        ),
        "{r:?}"
    );
}

#[test]
fn turns_that_are_not_partial_turns_are_refused() {
    let p = polygon(&[[2.0, 0.0], [5.0, 0.0], [5.0, 4.0], [2.0, 4.0]]);
    let reg = region(&[p]);
    let e = Document::default().resolution().zone().mm();
    for (from, to) in [
        (0.0, 0.0),
        (1.0, 0.5),
        (0.0, TAU),
        (0.0, 7.0),
        (f64::NAN, 1.0),
        (0.0, f64::INFINITY),
        (0.0, 0.1 * e),
        (0.0, TAU - 0.1 * e),
    ] {
        let r = revolve(
            &reg,
            &plain(),
            Turn::Partial { from, to },
            &Document::default(),
        );
        assert!(
            matches!(r, Err(MakeError::Refused(Refusal::Turn { .. }))),
            "{from} {to}: {r:?}"
        );
    }
}

#[test]
fn making_is_deterministic() {
    let p = vec![
        arc([0.0, 1.0], [0.0, -2.0], [0.0, 4.0], true),
        ln([0.0, 4.0], [0.0, -2.0]),
    ];
    let a = make(core::slice::from_ref(&p), &tilted(), quarter(0.2));
    let b = make(&[p], &tilted(), quarter(0.2));
    let bits = |b: &Brep| -> Vec<[u64; 3]> {
        b.vertices()
            .map(|v| v.point().to_array().map(f64::to_bits))
            .collect()
    };
    assert_eq!(a.1, b.1);
    assert_eq!(bits(&a.0), bits(&b.0));
}

#[test]
fn walls_follow_the_segments_as_given() {
    // A clockwise outer loop is turned by the region; the walls still follow the given order.
    let p = polygon(&[[2.0, 0.0], [2.0, 4.0], [5.0, 4.0], [5.0, 0.0]]);
    let (body, faces) = make(&[p], &plain(), Turn::Full);
    assert_eq!(
        wall_kinds(&body, &faces.walls[0]),
        ["cylinder", "plane", "cylinder", "plane"]
    );
    let Wall::Face(f) = faces.walls[0][0] else {
        panic!()
    };
    let Surface::Cylinder(c) = body.face(f).unwrap().surface() else {
        panic!()
    };
    assert_eq!(c.radius(), Length::from_mm(2.0));
}
