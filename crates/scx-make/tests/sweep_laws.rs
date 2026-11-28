// Written for solidcorex. Apache-2.0.
//! `sweep` (research 75 MK7b): the pin's `sweep_stations.rs` law and rail tests (those `MK7a`'s
//! `frames.rs` left out on purpose) judged against closed forms, and the entry's answer.
//!
//! - a straight path with twist `T` and end scale `k` turns the profile by `T u` and scales
//!   it by `1 + (k - 1) u` (pin :138), so a square's corners turn by `T u` at a fixed
//!   distance and a circle's radius is `r k(u)` (the app's `sweep_path.rs` twisted and
//!   scaled sweeps, their placements);
//! - a straight rail diverging at slope `s` scales the profile by `(d_0 + s x) / d_0`, both
//!   ways or only across (pin :238); an arc rail of radius `R` about the start by
//!   `sqrt(R^2 - x^2) / R` (pin :268); two rails span the profile (pin :420);
//! - a rail is followed exactly: the profile's point on a rail lands on the rail, at the
//!   station's plane, at every station (new; along an arc and along a line);
//! - a rail stopping short, touching a plane, crossing it twice or lying in it is refused
//!   by name; a polyline's vertex on a plane is one crossing (pin :291, :364, :387, :407);
//!   rails along a path with a corner are unsupported (pin :455);
//! - the parallel and direction orientations (pin :627, :665), a profile normal against the
//!   path (pin :692), the laws' refusals (pin :706) and the skin's fractions (pin :309);
//! - `sweep` with any law checks its input and then answers that it needs the procedural
//!   sweep kind; the plain one builds (its walls are `tests/sweep.rs`'s).
//!
//! Removed on purpose, from the pin's tests: the "more than two rails" and "rails with a
//! twist" refusals of `out_of_range_paths_laws_and_fractions_are_refused` (a `PathFrame`
//! cannot hold either: `Rails` is one or two, and rails and a `Shape` are separate
//! variants) and `the_caller_s_twist_and_scale_limits_refuse_past_them` (the caller's own
//! limits on twist and scale, SolidCraft's four turns and 0.01 to 100, are the caller's to
//! check before calling; only the geometric limits are refused here). The volume truths
//! (the twisted prism, the frustum) need the walls: step 5.

// The closed forms are written with the platform's libm; tolerances (1e-12) absorb its
// differences from `fmath`.
#![allow(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use core::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};
use std::sync::Arc;

use scx_brep::Document;
use scx_make::{
    LawFault, MIN_SKIN_STATIONS, MakeError, Needed, Orientation, Path, PathFrame, Piece, Placement,
    Rail, RailFault, RailFit, Rails, Refusal, Shape, Stop, Unsupported, fractions, placements,
    sweep,
};
use scx_num::frame::Frame;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::{KnotVector, NurbsCurve};
use scx_planar::{Region, Segment};

fn doc() -> Document {
    Document::default()
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn unit(v: Vec3) -> UnitVec3 {
    v.normalized().unwrap()
}

fn near(a: Vec3, b: Vec3, tol: f64) -> bool {
    (a - b).length() <= tol
}

fn near_p(a: Point3, b: Point3, tol: f64) -> bool {
    a.distance(b) <= tol
}

fn stops(us: &[f64]) -> Vec<Stop> {
    us.iter().map(|&u| Stop::At(u)).collect()
}

fn path(pieces: &[Piece]) -> Path {
    Path::new(pieces, &doc()).unwrap()
}

fn along_x(len: f64) -> Path {
    path(&[Piece::Line {
        from: p(0.0, 0.0, 0.0),
        to: p(len, 0.0, 0.0),
    }])
}

fn shaped(shape: Shape) -> PathFrame {
    PathFrame::Shaped(shape)
}

fn one(rail: Rail) -> PathFrame {
    PathFrame::Railed {
        rails: Rails::One(rail),
        fit: RailFit::Scale,
    }
}

fn line(a: Point3, b: Point3) -> Rail {
    Rail::Line { from: a, to: b }
}

fn place(path: &Path, normal: Vec3, frame: &PathFrame, at: &[Stop]) -> Vec<Placement> {
    placements(path, unit(normal), frame, at, &doc()).unwrap()
}

fn refusal(path: &Path, normal: Vec3, frame: &PathFrame, at: &[Stop]) -> MakeError {
    placements(path, unit(normal), frame, at, &doc()).unwrap_err()
}

fn rail_fault(e: &MakeError) -> RailFault {
    match e {
        MakeError::Refused(Refusal::Law(LawFault::Rail { fault, .. })) => *fault,
        other => panic!("not a rail fault: {other:?}"),
    }
}

/// A non-rational clamped B-spline rail of `degree` through `points`, `inner` its interior
/// knots in `(0, 1)`.
fn bspline(points: &[[f64; 3]], degree: usize, inner: &[f64]) -> Rail {
    let mut knots = vec![0.0; degree + 1];
    knots.extend_from_slice(inner);
    knots.extend(std::iter::repeat_n(1.0, degree + 1));
    let kv = KnotVector::new(degree, knots).unwrap();
    Rail::Nurbs(Arc::new(
        NurbsCurve::new(kv, points.to_vec(), None).unwrap(),
    ))
}

#[test]
fn a_straight_path_twists_and_scales_linearly() {
    let len = 12.0;
    let up = path(&[Piece::Line {
        from: p(0.0, 0.0, 0.0),
        to: p(0.0, 0.0, len),
    }]);
    let (twist, scale) = (1.5 * PI, 3.0);
    let frame = shaped(Shape {
        twist,
        end_scale: scale,
        ..Shape::PLAIN
    });
    let at = [0.0, 0.3, 0.5, 1.0];
    for (s, u) in place(&up, Vec3::Z, &frame, &stops(&at)).iter().zip(at) {
        let k = 1.0 + (scale - 1.0) * u;
        let turn = twist * u;
        let want = Vec3::new(turn.cos(), turn.sin(), 0.0) * k;
        assert!(near(s.apply(Vec3::X), want, 1e-12), "{u}");
        assert!(near(s.apply(Vec3::Z), Vec3::Z, 1e-12), "{u}");
        assert!(near_p(s.at, p(0.0, 0.0, len * u), 1e-12));
        // the square of half-width 5 (the app's twisted prism): each corner turned by the
        // twist run at its distance, so the section is the square turned, its area kept
        // (the scale 1 below); and a circle of radius 10 has radius 10 k (the frustum)
        let corner = p(5.0, 5.0, 0.0);
        let placed = s.place(corner) - s.at;
        let (c, sn) = (turn.cos(), turn.sin());
        let want = Vec3::new(5.0 * c - 5.0 * sn, 5.0 * sn + 5.0 * c, 0.0) * k;
        assert!(near(placed, want, 1e-11), "{u}");
        assert!((s.apply(Vec3::new(10.0, 0.0, 0.0)).length() - 10.0 * k).abs() < 1e-11);
    }
}

#[test]
fn a_twist_alone_keeps_the_section_s_size() {
    // the app's `twisted_sweep_turns_in_proportion`: h 40, a quarter turn; the map is a
    // rotation at every station (columns orthonormal), so the area is kept
    let up = path(&[Piece::Line {
        from: p(0.0, 0.0, 0.0),
        to: p(0.0, 0.0, 40.0),
    }]);
    let frame = shaped(Shape {
        twist: FRAC_PI_2,
        ..Shape::PLAIN
    });
    let at: Vec<f64> = (0..=8).map(|i| f64::from(i) / 8.0).collect();
    for (s, u) in place(&up, Vec3::Z, &frame, &stops(&at)).iter().zip(&at) {
        let [a, b, c] = s.m;
        for (x, y) in [(a, a), (b, b), (c, c)] {
            assert!((x.dot(y) - 1.0).abs() < 1e-14);
        }
        assert!(a.dot(b).abs() < 1e-14 && b.dot(c).abs() < 1e-14 && a.dot(c).abs() < 1e-14);
        let corner = s.place(p(5.0, -5.0, 0.0));
        let angle = (corner.y).atan2(corner.x) - (-5.0_f64).atan2(5.0);
        assert!((angle - FRAC_PI_2 * u).abs() < 1e-12, "{u}");
    }
}

#[test]
fn a_diverging_straight_rail_scales_the_profile() {
    let (len, d0, slope) = (20.0, 4.0, 0.25);
    let rail = line(p(0.0, d0, 0.0), p(len, d0 + slope * len, 0.0));
    let at = [0.0, 0.5, 1.0];
    for fit in [RailFit::Scale, RailFit::Stretch] {
        let frame = PathFrame::Railed {
            rails: Rails::One(rail.clone()),
            fit,
        };
        for (s, u) in place(&along_x(len), Vec3::X, &frame, &stops(&at))
            .iter()
            .zip(at)
        {
            let x = len * u;
            let k = (d0 + slope * x) / d0;
            assert!(near(s.apply(Vec3::Y), Vec3::Y * k, 1e-12), "{fit:?} {u}");
            let z = if fit == RailFit::Scale { k } else { 1.0 };
            assert!(near(s.apply(Vec3::Z), Vec3::Z * z, 1e-12), "{fit:?} {u}");
            assert!(near_p(s.at, p(x, 0.0, 0.0), 1e-12));
        }
    }
}

#[test]
fn an_arc_rail_scales_by_its_distance_across() {
    let (len, r): (f64, f64) = (6.0, 10.0);
    let start = (len / r).acos();
    let rail = Rail::Arc {
        centre: p(0.0, 0.0, 0.0),
        axis: UnitVec3::Z,
        start: p(r * start.cos(), r * start.sin(), 0.0),
        angle: FRAC_PI_2 - start,
    };
    // the rail runs from x = 6 back to x = 0: it reaches both ends' planes
    let at = [0.0, 0.25, 0.5, 1.0];
    for (s, u) in place(&along_x(len), Vec3::X, &one(rail), &stops(&at))
        .iter()
        .zip(at)
    {
        let x = len * u;
        let k = (r * r - x * x).sqrt() / r;
        assert!(near(s.apply(Vec3::Y), Vec3::Y * k, 1e-12), "{u}");
    }
}

/// A rail is followed exactly: the profile's point drawn on the rail lands on the rail, in
/// the station's plane, at every station. Along a quarter arc of radius 10 a concentric
/// rail of radius 13 is crossed on the radius, 3 out: the map is the frame's turn, and the
/// rail's start lands at `13 (cos phi, sin phi)`. Two rails at radii 13 and 8 span 5 across:
/// both starts land on their rails.
#[test]
fn a_rail_is_followed_exactly() {
    let arc = path(&[Piece::Arc {
        centre: p(0.0, 0.0, 0.0),
        axis: UnitVec3::Z,
        start: p(10.0, 0.0, 0.0),
        angle: FRAC_PI_2,
    }]);
    let ring = |r: f64| Rail::Arc {
        centre: p(0.0, 0.0, 0.0),
        axis: UnitVec3::Z,
        start: p(r, 0.0, 0.0),
        angle: FRAC_PI_2,
    };
    let at = [0.0, 0.2, 0.5, 0.9, 1.0];
    for (s, u) in place(&arc, Vec3::Y, &one(ring(13.0)), &stops(&at))
        .iter()
        .zip(at)
    {
        let phi = FRAC_PI_2 * u;
        let on = p(13.0 * phi.cos(), 13.0 * phi.sin(), 0.0);
        assert!(near_p(s.place(p(13.0, 0.0, 0.0)), on, 1e-12), "{u}");
        // the third way, square to the path's plane, is kept (k = 1)
        assert!(near(s.apply(Vec3::Z), Vec3::Z, 1e-12), "{u}");
    }
    let two = PathFrame::Railed {
        rails: Rails::Two(ring(13.0), ring(8.0)),
        fit: RailFit::Scale,
    };
    for (s, u) in place(&arc, Vec3::Y, &two, &stops(&at)).iter().zip(at) {
        let (c, sn) = ((FRAC_PI_2 * u).cos(), (FRAC_PI_2 * u).sin());
        assert!(near_p(
            s.place(p(13.0, 0.0, 0.0)),
            p(13.0 * c, 13.0 * sn, 0.0),
            1e-12
        ));
        assert!(near_p(
            s.place(p(8.0, 0.0, 0.0)),
            p(8.0 * c, 8.0 * sn, 0.0),
            1e-12
        ));
    }
    // a line rail rising out of the path's plane: the rail's start lands on the rail at the
    // station's plane, x = 10 u
    let rail = line(p(0.0, 2.0, 1.0), p(10.0, 5.0, 7.0));
    for fit in [RailFit::Scale, RailFit::Stretch] {
        let frame = PathFrame::Railed {
            rails: Rails::One(rail.clone()),
            fit,
        };
        for (s, u) in place(&along_x(10.0), Vec3::X, &frame, &stops(&at))
            .iter()
            .zip(at)
        {
            let want = p(10.0 * u, 2.0 + 3.0 * u, 1.0 + 6.0 * u);
            assert!(
                near_p(s.place(p(0.0, 2.0, 1.0)), want, 1e-12),
                "{fit:?} {u}"
            );
        }
    }
}

#[test]
fn a_rail_that_stops_short_is_refused() {
    let rail = one(line(p(0.0, 3.0, 0.0), p(5.0, 3.0, 0.0)));
    let e = refusal(&along_x(20.0), Vec3::X, &rail, &stops(&[0.0, 1.0]));
    assert_eq!(rail_fault(&e), RailFault::Short);
    assert_eq!(e.name(), "sweep_law");
    assert!(e.to_string().contains("does not reach"), "{e}");
}

/// A polyline rail with its vertex exactly on a station's plane crosses once there: the root
/// at the knot both spans share is one root, and the profile scales by 5 / 3 there.
#[test]
fn a_polyline_rail_s_vertex_on_a_plane_is_one_crossing() {
    let rail = bspline(
        &[[0.0, 3.0, 0.0], [5.0, 5.0, 0.0], [10.0, 3.0, 0.0]],
        1,
        &[0.5],
    );
    let s = place(&along_x(10.0), Vec3::X, &one(rail), &stops(&[0.5]));
    assert!(near(s[0].apply(Vec3::Y), Vec3::Y * (5.0 / 3.0), 1e-12));
}

/// A cubic rail `x(s) = 18 s - 24 s^2 + 8 s^3` (control x 0, 6, 4, 2): `x - 4 = -4 (1 - 2 s)^2
/// (1 - s / 2)`, so it touches the plane `x = 4` at `s = 1/2` without crossing (a double
/// root; the distance is negative at both ends), and crosses `x = 3` twice (it rises to 4 and
/// falls to 2). It meets `x = 0` once, at its start, and `x = 1` once.
#[test]
fn a_rail_touching_or_crossing_twice_is_refused() {
    let pts = [
        [0.0, 3.0, 0.0],
        [6.0, 3.0, 0.0],
        [4.0, 3.0, 0.0],
        [2.0, 3.0, 0.0],
    ];
    let rail = one(bspline(&pts, 3, &[]));
    let to4 = along_x(4.0);
    assert_eq!(place(&to4, Vec3::X, &rail, &stops(&[0.0, 0.25])).len(), 2);
    let e = refusal(&to4, Vec3::X, &rail, &stops(&[1.0]));
    assert_eq!(rail_fault(&e), RailFault::Touches);
    let e = refusal(&to4, Vec3::X, &rail, &stops(&[0.75]));
    assert_eq!(rail_fault(&e), RailFault::Twice);
}

#[test]
fn a_rail_in_a_station_s_plane_is_refused() {
    let rail = one(line(p(0.0, 1.0, 0.0), p(0.0, 3.0, 0.0)));
    let e = refusal(&along_x(5.0), Vec3::X, &rail, &stops(&[0.0]));
    assert_eq!(rail_fault(&e), RailFault::InPlane);
}

/// Two rails `y = 2` and `y = -2 - s x`: the profile spans them, so `y` scales by
/// `k = (4 + s x) / 4` (and `z` by `k` too when scaled evenly), and the map's origin goes to
/// the first rail less the map of its start, `(x, 2 - 2 k, 0)`.
#[test]
fn two_rails_span_the_profile() {
    let (len, slope) = (8.0, 0.5);
    let a = line(p(0.0, 2.0, 0.0), p(len, 2.0, 0.0));
    let b = line(p(0.0, -2.0, 0.0), p(len, -2.0 - slope * len, 0.0));
    let at = [0.0, 0.5, 1.0];
    for fit in [RailFit::Scale, RailFit::Stretch] {
        let frame = PathFrame::Railed {
            rails: Rails::Two(a.clone(), b.clone()),
            fit,
        };
        for (s, u) in place(&along_x(len), Vec3::X, &frame, &stops(&at))
            .iter()
            .zip(at)
        {
            let x = len * u;
            let k = (4.0 + slope * x) / 4.0;
            assert!(near(s.apply(Vec3::Y), Vec3::Y * k, 1e-12), "{fit:?} {u}");
            let z = if fit == RailFit::Scale { k } else { 1.0 };
            assert!(near(s.apply(Vec3::Z), Vec3::Z * z, 1e-12), "{fit:?} {u}");
            assert!(near(s.apply(Vec3::X), Vec3::X, 1e-12), "{fit:?} {u}");
            let origin = p(x, 2.0 - 2.0 * k, 0.0);
            assert!(
                near_p(s.place(p(0.0, 0.0, 0.0)), origin, 1e-12),
                "{fit:?} {u}"
            );
        }
    }
}

#[test]
fn rails_meeting_the_path_are_refused() {
    let rail = one(line(p(0.0, 2.0, 0.0), p(4.0, 0.0, 0.0)));
    let e = refusal(&along_x(4.0), Vec3::X, &rail, &stops(&[1.0]));
    assert!(
        matches!(
            e,
            MakeError::Refused(Refusal::Law(LawFault::RailsMeet { .. }))
        ),
        "{e:?}"
    );
}

#[test]
fn rails_along_a_path_with_a_corner_are_unsupported() {
    let bent = path(&[
        Piece::Line {
            from: p(0.0, 0.0, 0.0),
            to: p(5.0, 0.0, 0.0),
        },
        Piece::Line {
            from: p(5.0, 0.0, 0.0),
            to: p(5.0, 0.0, 5.0),
        },
    ]);
    let rail = one(line(p(0.0, 3.0, 0.0), p(5.0, 3.0, 5.0)));
    let e = refusal(&bent, Vec3::X, &rail, &stops(&[0.0]));
    assert_eq!(e, MakeError::Unsupported(Unsupported::RailsAtCorner));
}

/// The parallel orientation keeps the profile as it is, and refuses a station the path crosses
/// at no more than the least angle asked (here SolidCraft's 5 degrees) to its plane: a quarter
/// arc from `y` reaches `-x`, square to it.
#[test]
fn the_parallel_orientation_keeps_the_profile() {
    let arc = path(&[Piece::Arc {
        centre: p(0.0, 0.0, 0.0),
        axis: UnitVec3::Z,
        start: p(5.0, 0.0, 0.0),
        angle: FRAC_PI_2,
    }]);
    let parallel = |deg: f64| {
        shaped(Shape {
            orientation: Orientation::Parallel {
                least_crossing: deg.to_radians(),
            },
            ..Shape::PLAIN
        })
    };
    let st = place(&arc, Vec3::Y, &parallel(5.0), &stops(&[0.0, 0.5]));
    for s in &st {
        for e in [Vec3::X, Vec3::Y, Vec3::Z] {
            assert!(near(s.apply(e), e, 1e-15));
        }
    }
    let q = 5.0 * FRAC_PI_4.cos();
    assert!(near_p(st[1].at, p(q, q, 0.0), 1e-12));
    let e = refusal(&arc, Vec3::Y, &parallel(5.0), &stops(&[1.0]));
    assert!(
        matches!(e, MakeError::Refused(Refusal::Law(LawFault::Crossing { stop: Stop::At(u) })) if u == 1.0),
        "{e:?}"
    );
    // 84 degrees round, the tangent is 6 degrees off the plane: crossed at 5, not at 7
    let u = 84.0 / 90.0;
    place(&arc, Vec3::Y, &parallel(5.0), &stops(&[u]));
    let e = refusal(&arc, Vec3::Y, &parallel(7.0), &stops(&[u]));
    assert_eq!(e.name(), "sweep_law");
}

fn helix_frame(a: f64, b: f64, phi: f64) -> Vec3 {
    let c = a.hypot(b);
    let (s, co) = phi.sin_cos();
    Vec3::new(-a * s, a * co, b) * (1.0 / c)
}

/// The direction orientation along a helix about `z` with the direction `z`: the frame is
/// `[t, r, t x r]`, `r` the unit part of `z` square to the tangent, carried from the start's.
#[test]
fn the_direction_orientation_along_a_helix_keeps_the_direction_across() {
    let (a, b) = (2.0_f64, 0.5);
    let coil = path(&[Piece::Helix {
        centre: p(0.0, 0.0, 0.0),
        axis: UnitVec3::Z,
        start: p(a, 0.0, 0.0),
        angle: TAU,
        rise: b,
    }]);
    let frame = shaped(Shape {
        orientation: Orientation::Direction {
            along: UnitVec3::Z,
            least_sine: 0.0,
        },
        ..Shape::PLAIN
    });
    let across = |t: Vec3| {
        let r = Vec3::Z - t * t.dot(Vec3::Z);
        r * (1.0 / r.length())
    };
    let t0 = helix_frame(a, b, 0.0);
    let at = [0.25, 0.6];
    for (s, u) in place(&coil, t0, &frame, &stops(&at)).iter().zip(at) {
        let t = helix_frame(a, b, TAU * u);
        assert!(near(s.apply(t0), t, 1e-12), "{u}");
        assert!(near(s.apply(across(t0)), across(t), 1e-12), "{u}");
    }
    // a path along the direction is refused
    let up = path(&[Piece::Line {
        from: p(0.0, 0.0, 0.0),
        to: p(0.0, 0.0, 3.0),
    }]);
    let e = refusal(&up, Vec3::Z, &frame, &stops(&[0.5]));
    assert!(
        matches!(
            e,
            MakeError::Refused(Refusal::Law(LawFault::AlongDirection { .. }))
        ),
        "{e:?}"
    );
}

/// A profile normal against the path is the same profile plane: the placements agree, twist
/// included (it turns about the path's own direction either way).
#[test]
fn a_profile_normal_against_the_path_gives_the_same_placements() {
    let frame = shaped(Shape {
        twist: 1.0,
        end_scale: 2.0,
        ..Shape::PLAIN
    });
    let at = stops(&[0.0, 0.3, 1.0]);
    let with = place(&along_x(4.0), Vec3::X, &frame, &at);
    let against = place(&along_x(4.0), -Vec3::X, &frame, &at);
    assert_eq!(with, against);
}

/// A right-angle corner's mitre: the profile turned the eighth and stretched by `sqrt 2`
/// along `t_1 - t_0`, the twist and scale at the corner's fraction (the pin's corner test's
/// stretch half, which `MK7a` left to the sweep).
#[test]
fn a_corner_s_mitre_is_stretched() {
    let bent = path(&[
        Piece::Line {
            from: p(0.0, 0.0, 0.0),
            to: p(5.0, 0.0, 0.0),
        },
        Piece::Line {
            from: p(5.0, 0.0, 0.0),
            to: p(5.0, 5.0, 0.0),
        },
    ]);
    let s = place(&bent, Vec3::X, &shaped(Shape::PLAIN), &[Stop::Corner(0)]);
    let b = unit(Vec3::new(-1.0, 1.0, 0.0)).as_vec();
    let bisector = unit(Vec3::new(1.0, 1.0, 0.0)).as_vec();
    // the profile's normal x goes to the bisector; its y, square to it in the plane, to the
    // mitre's direction b stretched by sqrt 2; z is kept
    assert!(near(s[0].apply(Vec3::X), bisector, 1e-12));
    assert!(near(s[0].apply(Vec3::Y), b * 2.0_f64.sqrt(), 1e-12));
    assert!(near(s[0].apply(Vec3::Z), Vec3::Z, 1e-12));
    let scaled = shaped(Shape {
        end_scale: 3.0,
        ..Shape::PLAIN
    });
    let s = place(&bent, Vec3::X, &scaled, &[Stop::Corner(0)]);
    // at the corner's fraction 1/2 the scale is 2
    assert!(near(s[0].apply(Vec3::Z), Vec3::Z * 2.0, 1e-12));
}

#[test]
fn out_of_range_laws_are_refused() {
    let line5 = along_x(5.0);
    let ends = stops(&[0.0, 1.0]);
    let law = |e: &MakeError| match e {
        MakeError::Refused(Refusal::Law(f)) => *f,
        other => panic!("{other:?}"),
    };
    for twist in [f64::INFINITY, f64::NAN, 1001.0 * TAU] {
        let e = refusal(
            &line5,
            Vec3::X,
            &shaped(Shape {
                twist,
                ..Shape::PLAIN
            }),
            &ends,
        );
        assert_eq!(law(&e).name(), "twist");
    }
    // any finite twist up to the turns, either way, and any positive scale
    place(
        &line5,
        Vec3::X,
        &shaped(Shape {
            twist: -1000.0 * TAU,
            end_scale: 200.0,
            ..Shape::PLAIN
        }),
        &ends,
    );
    for end_scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let e = refusal(
            &line5,
            Vec3::X,
            &shaped(Shape {
                end_scale,
                ..Shape::PLAIN
            }),
            &ends,
        );
        assert_eq!(law(&e).name(), "end_scale");
    }
    for least_crossing in [-0.1, FRAC_PI_2, f64::NAN] {
        let frame = shaped(Shape {
            orientation: Orientation::Parallel { least_crossing },
            ..Shape::PLAIN
        });
        assert_eq!(
            law(&refusal(&line5, Vec3::X, &frame, &ends)).name(),
            "least_crossing"
        );
    }
    for least_sine in [-0.1, 1.0, f64::NAN] {
        let frame = shaped(Shape {
            orientation: Orientation::Direction {
                along: UnitVec3::Y,
                least_sine,
            },
            ..Shape::PLAIN
        });
        assert_eq!(
            law(&refusal(&line5, Vec3::X, &frame, &ends)).name(),
            "least_sine"
        );
    }
    let bad_rails = [
        line(p(0.0, 3.0, 0.0), p(0.0, 3.0, 0.0)),
        line(p(0.0, 3.0, 0.0), p(f64::NAN, 3.0, 0.0)),
        Rail::Arc {
            centre: p(0.0, 0.0, 0.0),
            axis: UnitVec3::Z,
            start: p(0.0, 0.0, 1.0),
            angle: 1.0,
        },
        Rail::Arc {
            centre: p(0.0, 0.0, 0.0),
            axis: UnitVec3::Z,
            start: p(1.0, 0.0, 0.0),
            angle: 7.0,
        },
    ];
    for rail in bad_rails {
        let e = refusal(&line5, Vec3::X, &one(rail), &ends);
        assert_eq!(rail_fault(&e), RailFault::Shape);
    }
    // the frames' refusals come through as the path's
    let e = refusal(&line5, Vec3::X, &shaped(Shape::PLAIN), &stops(&[1.5]));
    assert_eq!(e.name(), "path");
    let e = refusal(&line5, Vec3::X, &shaped(Shape::PLAIN), &[Stop::Corner(0)]);
    assert_eq!(e.name(), "path");
}

/// SolidCraft's station count: a station per degree of turn. A quarter arc twisted a quarter
/// turn turns 180 degrees in all: 180 steps (as the rule's ceiling rounds pi over a degree),
/// 0 and 1 among the fractions; a straight path with no twist takes the least, 16 steps; a
/// joint's fraction is among them exactly.
#[test]
fn station_fractions_are_a_degree_of_turn_each() {
    let arc = path(&[Piece::Arc {
        centre: p(0.0, 0.0, 0.0),
        axis: UnitVec3::Z,
        start: p(5.0, 0.0, 0.0),
        angle: FRAC_PI_2,
    }]);
    let twisted = shaped(Shape {
        twist: FRAC_PI_2,
        ..Shape::PLAIN
    });
    let f = fractions(&arc, &twisted, &doc()).unwrap();
    let n = (PI / (PI / 180.0)).ceil() as usize;
    assert!(n == 180 || n == 181, "{n}");
    assert_eq!(f.len(), n + 1);
    assert_eq!((f[0], f[n]), (0.0, 1.0));
    let up = path(&[Piece::Line {
        from: p(0.0, 0.0, 0.0),
        to: p(0.0, 0.0, 3.0),
    }]);
    let f = fractions(&up, &shaped(Shape::PLAIN), &doc()).unwrap();
    assert_eq!(f.len(), MIN_SKIN_STATIONS + 1);
    // a helix of radius 3 and rise 4 / turn... its tangent turns angle a / sqrt(a^2 + b^2)
    let coil = path(&[Piece::Helix {
        centre: p(0.0, 0.0, 0.0),
        axis: UnitVec3::Z,
        start: p(3.0, 0.0, 0.0),
        angle: TAU,
        rise: 4.0,
    }]);
    let f = fractions(&coil, &shaped(Shape::PLAIN), &doc()).unwrap();
    assert_eq!(f.len(), 216 + 1, "360 * 3 / 5 degrees");
    // two lines of 3 and 4 meeting G1: the joint at 3/7 is a station
    let joined = path(&[
        Piece::Line {
            from: p(0.0, 0.0, 0.0),
            to: p(3.0, 0.0, 0.0),
        },
        Piece::Line {
            from: p(3.0, 0.0, 0.0),
            to: p(7.0, 0.0, 0.0),
        },
    ]);
    let f = fractions(&joined, &shaped(Shape::PLAIN), &doc()).unwrap();
    assert!(f.contains(&(3.0 / 7.0)));
    assert_eq!(f.len(), MIN_SKIN_STATIONS + 2);
    // a NURBS rail of 40 control points asks for 39 steps at least
    let pts: Vec<[f64; 3]> = (0..40)
        .map(|i| [f64::from(i) / 39.0 * 7.0, 3.0, 0.0])
        .collect();
    let inner: Vec<f64> = (1..37).map(|i| f64::from(i) / 37.0).collect();
    let f = fractions(&joined, &one(bspline(&pts, 3, &inner)), &doc()).unwrap();
    assert_eq!(f.len(), 39 + 2);
}

fn square() -> Region {
    let d = doc();
    let q = |x: f64, y: f64| Point2::new(x, y);
    let corners = [q(-5.0, -5.0), q(5.0, -5.0), q(5.0, 5.0), q(-5.0, 5.0)];
    let segs: Vec<Segment> = (0..4)
        .map(|i| Segment::line(corners[i], corners[(i + 1) % 4]))
        .collect();
    Region::new(segs, Vec::new(), d.resolution()).unwrap()
}

#[test]
fn a_sweep_with_laws_checks_its_input_then_needs_the_procedural_kind() {
    let plane = Frame::new(p(0.0, 0.0, 0.0), Vec3::Z, Vec3::X).unwrap();
    let up = path(&[Piece::Line {
        from: p(0.0, 0.0, 0.0),
        to: p(0.0, 0.0, 40.0),
    }]);
    let twisted = shaped(Shape {
        twist: FRAC_PI_2,
        ..Shape::PLAIN
    });
    let e = sweep(&square(), &plane, &up, &twisted, &doc()).unwrap_err();
    assert_eq!(
        e,
        MakeError::Unsupported(Unsupported::NeedsKind {
            kind: Needed::ProceduralSweep
        })
    );
    assert_eq!(e.name(), "needs_kind");
    // the plain frame builds the walls by kind
    let plain = sweep(&square(), &plane, &up, &shaped(Shape::PLAIN), &doc());
    assert!(plain.is_ok(), "{plain:?}");
    // a refusal comes first: a rail stopping short of the path's end
    let short = one(line(p(0.0, 7.0, 0.0), p(0.0, 7.0, 20.0)));
    let e = sweep(&square(), &plane, &up, &short, &doc()).unwrap_err();
    assert_eq!(rail_fault(&e), RailFault::Short);
}
