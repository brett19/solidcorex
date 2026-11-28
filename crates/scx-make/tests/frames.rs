// Written for solidcorex. Apache-2.0.
//! `frames` (research 75 MK7a): the pin's `sweep_stations.rs` frame tests judged against
//! closed forms: the identity on a line, the turn of an arc, a helix frame's torsion, the
//! corner angles, and the composition of corner and helix.
//!
//! Removed on purpose, from the pin's tests (all of them the sweep's laws or rails, MK7b):
//! `a_straight_path_twists_and_scales_linearly`, the parallel and direction orientations,
//! the rail tests (`a_diverging_straight_rail_scales_the_profile` and the other rail cases),
//! the twist and scale limits of `out_of_range_paths_laws_and_fractions_are_refused` (its
//! path, piece and fraction refusals are kept below), `station_fractions_are_a_degree_of_turn_each`
//! (the skin's station rule, not a frame), and the mitre's stretch of the corner test (the
//! stretch is checked as `Corner::stretch`, its application is the sweep's).

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

use scx_brep::Document;
use scx_make::{FrameError, Path, Piece, Station, Stop, frames};
use scx_num::fmath;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};

fn doc() -> Document {
    Document::default()
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn unit(v: Vec3) -> UnitVec3 {
    v.normalized().unwrap()
}

fn line(a: [f64; 3], b: [f64; 3]) -> Piece {
    Piece::Line {
        from: p(a[0], a[1], a[2]),
        to: p(b[0], b[1], b[2]),
    }
}

fn helix(axis: Vec3, e: Vec3, a: f64, b: f64, angle: f64) -> Piece {
    Piece::Helix {
        centre: p(0.0, 0.0, 0.0),
        axis: unit(axis),
        start: Point3::new(e.x * a, e.y * a, e.z * a),
        angle,
        rise: b,
    }
}

fn path(pieces: &[Piece]) -> Path {
    Path::new(pieces, &doc()).unwrap()
}

fn at(path: &Path, us: &[f64]) -> Vec<Station> {
    let stops: Vec<Stop> = us.iter().map(|&u| Stop::At(u)).collect();
    frames(path, &stops).unwrap()
}

fn near(a: Vec3, b: Vec3, tol: f64) -> bool {
    (a - b).length() <= tol
}

fn near_p(a: Point3, b: Point3, tol: f64) -> bool {
    a.distance(b) <= tol
}

/// The signed angle about the unit `axis` from `a` to `b` (both square to it).
fn angle_about(axis: Vec3, a: Vec3, b: Vec3) -> f64 {
    a.cross(b).dot(axis).atan2(a.dot(b))
}

/// A helix's tangent and principal normal at `phi` (axis z, starting on x).
fn helix_frame(a: f64, b: f64, phi: f64) -> (Vec3, Vec3) {
    let c = fmath::hypot(a, b);
    let (s, co) = phi.sin_cos();
    (
        Vec3::new(-a * s, a * co, b) * (1.0 / c),
        Vec3::new(-co, -s, 0.0),
    )
}

#[test]
fn a_line_is_the_identity() {
    let l = path(&[line([1.0, 2.0, 3.0], [4.0, -2.0, 15.0])]);
    for s in at(&l, &[0.0, 0.3, 1.0]) {
        assert_eq!(s.turn.0, [Vec3::X, Vec3::Y, Vec3::Z]);
        assert!(near(
            *s.tangent,
            Vec3::new(3.0, -4.0, 12.0) * (1.0 / 13.0),
            1e-15
        ));
    }
    let end = at(&l, &[1.0])[0];
    assert!(near_p(end.at, p(4.0, -2.0, 15.0), 1e-14));
    assert!((l.length() - 13.0).abs() < 1e-14);
}

/// A planar arc's frame is its Frenet frame's turn: every station turns by the angle run,
/// whichever axis the arc is about.
#[test]
fn a_planar_arc_turns_the_frame_by_the_angle_run() {
    let r = 7.0;
    let a = path(&[Piece::Arc {
        centre: p(0.0, 0.0, 0.0),
        axis: UnitVec3::Z,
        start: p(r, 0.0, 0.0),
        angle: FRAC_PI_2,
    }]);
    let us = [0.0, 0.5, 1.0];
    for (s, u) in at(&a, &us).iter().zip(us) {
        let phi = FRAC_PI_2 * u;
        assert!(near_p(s.at, p(r * phi.cos(), r * phi.sin(), 0.0), 1e-12));
        assert!(near(s.turn.apply(Vec3::Z), Vec3::Z, 1e-12));
        assert!(near(
            s.turn.apply(Vec3::X),
            Vec3::new(phi.cos(), phi.sin(), 0.0),
            1e-12
        ));
        assert!(near(
            *s.tangent,
            Vec3::new(-phi.sin(), phi.cos(), 0.0),
            1e-12
        ));
        assert!(s.turn.defect() < 1e-13);
    }
}

/// After a whole helix turn the tangent is the start's and the frame has turned about it by
/// `-2 pi b / sqrt(a^2 + b^2)`, the torsion's integral against the screw.
#[test]
fn a_helix_frame_turns_by_its_torsion_against_the_screw() {
    let (a, b) = (5.0, 2.0);
    let h = path(&[helix(Vec3::Z, Vec3::X, a, b, TAU)]);
    let t0 = Vec3::new(0.0, a, b) * (1.0 / fmath::hypot(a, b));
    let st = at(&h, &[0.0, 0.25, 1.0]);
    for s in &st {
        assert!(s.turn.defect() < 1e-13, "{}", s.turn.defect());
    }
    let end = st[2];
    assert!(near(end.turn.apply(t0), t0, 1e-12));
    let normal = Vec3::new(-1.0, 0.0, 0.0);
    let want = -TAU * b / fmath::hypot(a, b);
    let got = angle_about(t0, normal, end.turn.apply(normal));
    let diff = (got - want).rem_euclid(TAU);
    assert!(diff.min(TAU - diff) < 1e-12, "{got} vs {want}");
    assert!(near_p(end.at, p(a, 0.0, TAU * b), 1e-12));
}

/// At phi the start's normal `N_0` is turned into `cos psi N + sin psi B`,
/// `psi = -b phi / sqrt(a^2 + b^2)`, right- and left-handed.
#[test]
fn a_helix_turns_its_normal_by_minus_the_torsion() {
    let a = 2.0_f64;
    for b in [0.5, -0.5] {
        let h = path(&[helix(Vec3::Z, Vec3::X, a, b, TAU)]);
        let (t0, n0) = helix_frame(a, b, 0.0);
        for (s, u) in at(&h, &[0.25, 0.5]).iter().zip([0.25, 0.5]) {
            let phi = TAU * u;
            let (t, n) = helix_frame(a, b, phi);
            let psi = -b * phi / fmath::hypot(a, b);
            let want = n * psi.cos() + t.cross(n) * psi.sin();
            assert!(near(s.turn.apply(t0), t, 1e-12), "{b} {u}");
            assert!(near(s.turn.apply(n0), want, 1e-12), "{b} {u}");
            assert!(near_p(
                s.at,
                p(a * phi.cos(), a * phi.sin(), b * phi),
                1e-12
            ));
        }
    }
}

/// Parallel transport turns about no axis along the path: the infinitesimal rotation
/// between close stations has no component along the chord, on tilted and left-handed helices
/// and a tilted arc (the pin's check, 1e-9).
#[test]
fn the_frames_turn_about_no_axis_along_the_path() {
    let tilt = Vec3::new(1.0, 1.0, 1.0) * (1.0 / 3.0_f64.sqrt());
    let across = Vec3::new(1.0, -1.0, 0.0) * (1.0 / 2.0_f64.sqrt());
    let n = 1000;
    let us: Vec<f64> = (0..=n).map(|i| f64::from(i) / f64::from(n)).collect();
    let pieces = [
        helix(tilt, across, 2.0, 0.5, TAU),
        helix(tilt, across, 2.0, -0.5, TAU),
        Piece::Arc {
            centre: p(0.0, 0.0, 0.0),
            axis: unit(tilt),
            start: p(across.x * 3.0, across.y * 3.0, across.z * 3.0),
            angle: 3.0,
        },
    ];
    for piece in pieces {
        let st = at(&path(&[piece]), &us);
        let mut worst = 0.0_f64;
        for w in st.windows(2) {
            // R = M1 M0^T, the rotation between the two stations
            let r = w[1].turn.after(&w[0].turn.inverse());
            let [c0, c1, c2] = r.0;
            let skew = Vec3::new(c1.z - c2.y, c2.x - c0.z, c0.y - c1.x) * 0.5;
            let chord = w[1].at - w[0].at;
            worst = worst.max((skew.dot(chord) / (skew.length() * chord.length())).abs());
        }
        assert!(worst < 1e-9, "{piece:?}: {worst}");
    }
}

/// A right angle: the corner is a quarter turn about y, its mitre station the eighth, the
/// stretch `sqrt 2`; the fraction asked at the joint is the second piece's start, never
/// the mitre.
#[test]
fn a_right_angle_corner_turns_the_frame_a_quarter_and_its_mitre_an_eighth() {
    let l = path(&[
        line([0.0, 0.0, 0.0], [0.0, 0.0, 10.0]),
        line([0.0, 0.0, 10.0], [10.0, 0.0, 10.0]),
    ]);
    let corners: Vec<_> = l.corners().collect();
    assert_eq!(corners.len(), 1);
    let c = corners[0];
    assert!((c.angle - FRAC_PI_2).abs() < 1e-14);
    assert!((c.stretch - 2.0_f64.sqrt()).abs() < 1e-14);
    assert!((c.fraction - 0.5).abs() < 1e-15);
    assert!(near_p(c.at, p(0.0, 0.0, 10.0), 0.0));

    let st = frames(&l, &[Stop::Corner(0), Stop::At(1.0)]).unwrap();
    let (mitre, end) = (st[0], st[1]);
    assert!(near(end.turn.apply(Vec3::Z), Vec3::X, 1e-12));
    assert!(near(end.turn.apply(Vec3::X), -Vec3::Z, 1e-12));
    assert!(near(end.turn.apply(Vec3::Y), Vec3::Y, 1e-12));
    let half = Vec3::new(FRAC_PI_4.sin(), 0.0, FRAC_PI_4.cos());
    assert!(near(mitre.turn.apply(Vec3::Z), half, 1e-12));
    assert!(near(*mitre.tangent, half, 1e-12));
    assert!(near_p(mitre.at, p(0.0, 0.0, 10.0), 1e-12));

    let joint = c.fraction;
    let before = f64::from_bits(joint.to_bits() - 1);
    let st = at(&l, &[before, joint]);
    assert!(near(st[0].turn.apply(Vec3::Z), Vec3::Z, 1e-12));
    assert!(near(st[1].turn.apply(Vec3::Z), Vec3::X, 1e-12));
    for s in &st {
        assert!(near(s.turn.apply(Vec3::Y), Vec3::Y, 1e-12));
        assert!(s.turn.defect() < 1e-13);
    }
    assert_eq!(
        frames(&l, &[Stop::Corner(1)]).unwrap_err(),
        FrameError::NoCorner(1)
    );
}

/// A corner's turn is the least rotation: at every angle it takes the arriving tangent to the
/// leaving one and leaves the tangents' common normal alone.
#[test]
fn a_corner_is_the_least_rotation_at_any_angle() {
    for deg in [1.0_f64, 30.0, 90.0, 135.0, 179.0] {
        let a = deg.to_radians();
        let l = path(&[
            line([0.0, 0.0, 0.0], [5.0, 0.0, 0.0]),
            line([5.0, 0.0, 0.0], [5.0 + 5.0 * a.cos(), 5.0 * a.sin(), 0.0]),
        ]);
        let c = *l.corners().next().unwrap();
        assert!((c.angle - a).abs() < 1e-12, "{deg}");
        assert!((c.stretch - 1.0 / (0.5 * a).cos()).abs() < 1e-9 * c.stretch);
        let end = at(&l, &[1.0])[0];
        assert!(near(end.turn.apply(Vec3::X), *end.tangent, 1e-12), "{deg}");
        assert!(near(end.turn.apply(Vec3::Z), Vec3::Z, 1e-12), "{deg}");
        assert!(end.turn.defect() < 1e-13);
    }
}

/// A line along z into a helix about z (radius 2, rise 1/2): the corner is the least rotation
/// about `z x t_h` (along -x), so x stays; one helix turn is then the turn about `t_h` by
/// `psi = -2 pi b / c`, so at the end z goes to `t_h` and x to
/// `x cos psi + (t_h x x) sin psi`.
#[test]
fn a_corner_then_a_helix_compose() {
    let (a, b) = (2.0_f64, 0.5);
    let c = fmath::hypot(a, b);
    let l = path(&[
        line([a, 0.0, -5.0], [a, 0.0, 0.0]),
        helix(Vec3::Z, Vec3::X, a, b, TAU),
    ]);
    let end = at(&l, &[1.0])[0];
    let th = Vec3::new(0.0, a, b) * (1.0 / c);
    let psi = -TAU * b / c;
    let want = Vec3::X * psi.cos() + th.cross(Vec3::X) * psi.sin();
    assert!(near(end.turn.apply(Vec3::Z), th, 1e-12));
    assert!(near(end.turn.apply(Vec3::X), want, 1e-12));
}

#[test]
fn the_g1_rule_decides_what_is_a_corner() {
    let kinked = |a: f64| {
        path(&[
            line([0.0, 0.0, 0.0], [5.0, 0.0, 0.0]),
            line([5.0, 0.0, 0.0], [5.0 + 5.0 * a.cos(), 5.0 * a.sin(), 0.0]),
        ])
    };
    assert_eq!(kinked(1e-10).corners().count(), 0);
    assert_eq!(kinked(1e-7).corners().count(), 1);
}

#[test]
fn paths_pieces_and_fractions_out_of_range_are_refused() {
    let d = doc();
    let back = [
        line([0.0; 3], [5.0, 0.0, 0.0]),
        line([5.0, 0.0, 0.0], [0.0; 3]),
    ];
    assert_eq!(
        Path::new(&back, &d).unwrap_err(),
        FrameError::DoublesBack { index: 1 }
    );
    let apart = [
        line([0.0; 3], [5.0, 0.0, 0.0]),
        line([6.0, 0.0, 0.0], [9.0, 0.0, 0.0]),
    ];
    assert!(matches!(
        Path::new(&apart, &d).unwrap_err(),
        FrameError::Gap { index: 1, .. }
    ));
    assert_eq!(Path::new(&[], &d).unwrap_err(), FrameError::Pieces(0));
    assert!(matches!(
        Path::new(&[line([0.0; 3], [0.0; 3])], &d).unwrap_err(),
        FrameError::Piece(_)
    ));
    let on_axis = Piece::Arc {
        centre: p(0.0, 0.0, 0.0),
        axis: UnitVec3::Z,
        start: p(0.0, 0.0, 1.0),
        angle: 1.0,
    };
    assert!(matches!(
        Path::new(&[on_axis], &d).unwrap_err(),
        FrameError::Piece(_)
    ));
    let backwards = Piece::Arc {
        centre: p(0.0, 0.0, 0.0),
        axis: UnitVec3::Z,
        start: p(1.0, 0.0, 0.0),
        angle: -1.0,
    };
    assert!(matches!(
        Path::new(&[backwards], &d).unwrap_err(),
        FrameError::Piece(_)
    ));
    let l = path(&[line([0.0; 3], [5.0, 0.0, 0.0])]);
    for u in [-0.1, 1.1, f64::NAN] {
        assert!(matches!(
            frames(&l, &[Stop::At(u)]).unwrap_err(),
            FrameError::Fraction(_)
        ));
    }
    assert!(!l.is_closed(&d));
    let ring = path(&[Piece::Arc {
        centre: p(0.0, 0.0, 0.0),
        axis: UnitVec3::Z,
        start: p(1.0, 0.0, 0.0),
        angle: TAU,
    }]);
    assert!(ring.is_closed(&d));
    // a whole turn of an arc leaves the frame where it began
    let end = at(&ring, &[1.0])[0];
    assert!(near(end.turn.apply(Vec3::X), Vec3::X, 1e-12));
    let _ = PI;
}
