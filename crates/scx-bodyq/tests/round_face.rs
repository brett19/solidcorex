// Written for solidcorex. Apache-2.0.
//! What a body has on and past a round face (research 74 §34, BW11): each reading against a
//! body of revolution given as plain numbers (pieces whose inner and outer radii run straight
//! along the axis), whose probes are answered by hand, as the pin's `teeth/tests/round_face.rs`
//! does; the band read on real bodies; and hostile numbers refused, never a panic.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

mod along_common;

use std::sync::atomic::{AtomicUsize, Ordering};

use along_common::{doc, stadium, tilted_sheet};
use scx_bodyq::{
    Band, BodyqError, BodyqRefusal, BodyqUnsupported, End, Extent, Inward, Met, Near, Probe,
    SeatFrame, beside_clear, extent, extent_in, far_end, grow_block, into_seat, past_end,
    stands_over, whole_band,
};
use scx_brep::op::{Cx, Scratch, Settings};
use scx_num::budget::Stop;
use scx_num::frame::Frame;
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::cylinder;

const HOSTILE: [f64; 10] = [
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::MAX,
    f64::MIN,
    f64::MIN_POSITIVE,
    -1.0,
    0.0,
    1e-300,
    1e300,
];

fn near() -> Near {
    Near::of(
        doc().resolution(),
        Accuracy::ZERO,
        Accuracy::ZERO,
        Length::from_mm(50.0),
    )
    .unwrap()
}

/// A piece of a body of revolution about the seat's axis: from `z0` to `z1` along it, its inner
/// radius running straight from `in0` to `in1` and its outer from `out0` to `out1`.
#[derive(Clone, Copy, Debug)]
struct Turned {
    z0: f64,
    z1: f64,
    in0: f64,
    in1: f64,
    out0: f64,
    out1: f64,
}

fn solid(z0: f64, z1: f64, r: f64) -> Turned {
    Turned {
        z0,
        z1,
        in0: 0.0,
        in1: 0.0,
        out0: r,
        out1: r,
    }
}

fn ring(z0: f64, z1: f64, hole: f64, out: f64) -> Turned {
    Turned {
        z0,
        z1,
        in0: hole,
        in1: hole,
        out0: out,
        out1: out,
    }
}

/// The part of `lo..hi` where a value running straight from `at_lo` to `at_hi` is above zero.
fn above_zero(lo: f64, hi: f64, at_lo: f64, at_hi: f64) -> Option<(f64, f64)> {
    match (at_lo > 0.0, at_hi > 0.0) {
        (true, true) => Some((lo, hi)),
        (false, false) => None,
        (first, _) => {
            let z = lo + at_lo / (at_lo - at_hi) * (hi - lo);
            if first { Some((lo, z)) } else { Some((z, hi)) }
        }
    }
}

impl Turned {
    fn at(&self, z: f64) -> (f64, f64) {
        let t = (z - self.z0) / (self.z1 - self.z0);
        (
            self.in0 + (self.in1 - self.in0) * t,
            self.out0 + (self.out1 - self.out0) * t,
        )
    }

    /// The box of what of it lies within the probe's ring over its stretch.
    fn met(&self, p: &Probe) -> Option<([f64; 3], [f64; 3])> {
        let pin = p.inner.unwrap_or(0.0);
        let (mut lo, mut hi) = (self.z0.max(p.from), self.z1.min(p.to));
        if lo >= hi {
            return None;
        }
        (lo, hi) = above_zero(lo, hi, p.outer - self.at(lo).0, p.outer - self.at(hi).0)?;
        if lo >= hi {
            return None;
        }
        (lo, hi) = above_zero(lo, hi, self.at(lo).1 - pin, self.at(hi).1 - pin)?;
        if lo >= hi {
            return None;
        }
        let across = self.at(lo).1.max(self.at(hi).1).min(p.outer);
        Some(([-across, -across, lo], [across, across, hi]))
    }
}

/// A body of revolution about the seat's axis.
struct Body(Vec<Turned>);

impl Body {
    fn extent(&self) -> Extent {
        Extent {
            low: self.0.iter().map(|t| t.z0).fold(f64::INFINITY, f64::min),
            high: self
                .0
                .iter()
                .map(|t| t.z1)
                .fold(f64::NEG_INFINITY, f64::max),
            across: self
                .0
                .iter()
                .map(|t| t.out0.max(t.out1))
                .fold(0.0, f64::max),
        }
    }

    fn met(&self, p: &Probe) -> Met {
        let boxes: Vec<_> = self.0.iter().filter_map(|t| t.met(p)).collect();
        if boxes.is_empty() {
            return Met::Nothing;
        }
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for (lo, hi) in boxes {
            for (m, l) in min.iter_mut().zip(lo) {
                *m = m.min(l);
            }
            for (m, h) in max.iter_mut().zip(hi) {
                *m = m.max(h);
            }
        }
        Met::Piece { min, max }
    }
}

fn xy() -> Frame {
    Frame::WORLD
}

/// A round face of radius 10 whose free end is at z = 0, running on along +Z for 30.
fn seat() -> SeatFrame {
    SeatFrame {
        plane: xy(),
        inward: Inward::Along,
        room: 30.0,
        at: Point3::new(10.0, 0.0, 15.0),
    }
}

fn shaft(past: &[Turned]) -> Body {
    let mut pieces = vec![solid(0.0, 30.0, 10.0)];
    pieces.extend_from_slice(past);
    Body(pieces)
}

fn hub(past: &[Turned]) -> Body {
    let mut pieces = vec![ring(0.0, 30.0, 5.0, 20.0)];
    pieces.extend_from_slice(past);
    Body(pieces)
}

/// What lies past the free end of the seat's face of `radius` on `body`, asking no more than
/// six probes.
fn end_of(body: &Body, radius: f64, bore: bool, reach: f64) -> End {
    let asked = AtomicUsize::new(0);
    let mut probe = |p: &Probe| {
        asked.fetch_add(1, Ordering::Relaxed);
        body.met(p)
    };
    let end = past_end(
        &seat(),
        radius,
        bore,
        reach,
        &body.extent(),
        &near(),
        &mut probe,
    )
    .unwrap();
    assert!(
        asked.load(Ordering::Relaxed) <= 6,
        "{} probes",
        asked.load(Ordering::Relaxed)
    );
    end
}

#[test]
fn a_free_end_has_nothing_past_it() {
    let end = end_of(&shaft(&[]), 10.0, false, 9.0);
    assert!(
        end.free && end.open && end.out && !end.step && !end.cone,
        "{end:?}"
    );
    assert_eq!((end.end, end.through, end.run_out()), (0.0, 0.0, 0.0));
    assert_eq!(end.run_past(0.1), 0.1);
}

#[test]
fn a_shoulder_shuts_the_end() {
    let end = end_of(&shaft(&[solid(-5.0, 0.0, 15.0)]), 10.0, false, 9.0);
    assert_eq!(end, End::shut(10.0));
    assert!(!end.open && !end.out);
    assert_eq!((end.run_out(), end.run_past(0.1)), (0.0, 0.0));
}

#[test]
fn a_neck_is_a_step_with_its_radius() {
    let end = end_of(&shaft(&[solid(-20.0, 0.0, 6.0)]), 10.0, false, 9.0);
    assert!(end.open && end.out && end.step && !end.free, "{end:?}");
    assert!(
        (end.neck - 6.0).abs() < 1e-12 && end.through == 0.0,
        "{end:?}"
    );
    assert_eq!(end.run_past(0.1), 0.0);
}

/// A cone past the end: radius `rim` at z = -`long`, the face's radius 10 at z = 0.
fn chamfer(long: f64, rim: f64) -> Turned {
    Turned {
        z0: -long,
        z1: 0.0,
        in0: 0.0,
        in1: 0.0,
        out0: rim,
        out1: 10.0,
    }
}

#[test]
fn a_chamfer_is_read_as_a_cone_with_its_rim() {
    let end = end_of(&shaft(&[chamfer(1.0, 9.0)]), 10.0, false, 9.5);
    assert!(end.open && end.out && end.cone && !end.step, "{end:?}");
    assert!((end.end - 1.0).abs() < 1e-12, "{end:?}");
    assert!((end.rim - 9.0).abs() < 1e-9, "{end:?}");
    // The cuts reach radius 9.5: the chamfer is that wide half way along it.
    assert!((end.through - 0.5).abs() < 1e-4, "{end:?}");
    assert!((end.run_out() - 1.0).abs() < 1e-12);
    assert!((end.run_past(0.1) - 1.1).abs() < 1e-12);
}

#[test]
fn a_carried_chamfer_reaches_in_by_its_cone() {
    // A shaft of radius 10 with a 1 by 1 chamfer on its end: the rim at 9, 1 mm long.
    let end = End {
        open: true,
        out: true,
        cone: true,
        end: 1.0,
        rim: 9.0,
        ..End::shut(10.0)
    };
    assert_eq!(end.carried(10.0, 12.0), 2.0);
    assert_eq!(end.carried(10.0, 10.0), 0.0);
    assert_eq!(end.carried(10.0, 9.5), -0.5);
    // A bore of radius 10 with a 1 by 2 entry chamfer: made smaller to radius 8 the cone
    // meets the bore 1 mm further in.
    let entry = End { rim: 12.0, ..end };
    assert_eq!(entry.carried(10.0, 8.0), 1.0);
    // No chamfer of its own: a round (no cone), no length, a shut end.
    assert_eq!(End { cone: false, ..end }.carried(10.0, 12.0), 0.0);
    assert_eq!(End { end: 0.0, ..end }.carried(10.0, 12.0), 0.0);
    assert_eq!(End::shut(10.0).carried(10.0, 12.0), 0.0);
    // A rim on the face's own radius gives no slope: no length, not an infinite one.
    assert_eq!(End { rim: 10.0, ..end }.carried(10.0, 12.0), 0.0);
    for bad in HOSTILE {
        assert!(end.carried(bad, 12.0).is_finite(), "{bad}");
        assert!(end.carried(10.0, bad).is_finite(), "{bad}");
        assert!(End { end: bad, ..end }.carried(10.0, 12.0).is_finite());
        assert!(End { rim: bad, ..end }.carried(10.0, 12.0).is_finite());
    }
}

#[test]
fn a_round_is_told_from_a_chamfer_by_its_slope() {
    // A round of radius 1 on the end: r(z) = 9 + sqrt(1 - z^2) from z = -1 to 0, in short
    // straight pieces.
    let n = 2000;
    let r = |z: f64| 9.0 + (1.0 - z * z).max(0.0).sqrt();
    let pieces: Vec<Turned> = (0..n)
        .map(|i| {
            let z0 = -1.0 + f64::from(i) / f64::from(n);
            let z1 = -1.0 + f64::from(i + 1) / f64::from(n);
            Turned {
                z0,
                z1,
                in0: 0.0,
                in1: 0.0,
                out0: r(z0),
                out1: r(z1),
            }
        })
        .collect();
    let end = end_of(&shaft(&pieces), 10.0, false, 9.5);
    assert!(end.open && end.out && !end.cone && !end.step, "{end:?}");
    assert!((end.end - 1.0).abs() < 1e-12, "{end:?}");
    // The rim is where the round is near the body's end, not where a chamfer would put it.
    assert!(end.rim > 9.1 && end.rim < 9.2, "{end:?}");
    assert!((end.through - 0.75_f64.sqrt()).abs() < 1e-3, "{end:?}");
}

#[test]
fn a_long_taper_is_open_with_no_end_of_its_own() {
    // A cone down to radius 2 over 40 past the end: longer than the face's radius.
    let end = end_of(&shaft(&[chamfer(40.0, 2.0)]), 10.0, false, 9.0);
    assert!(end.open && end.out && !end.cone && !end.step, "{end:?}");
    assert_eq!((end.end, end.rim), (0.0, 10.0));
    assert!((end.through - 5.0).abs() < 1e-4, "{end:?}");
}

#[test]
fn a_counterbore_is_a_step_and_an_entry_chamfer_a_cone() {
    let end = end_of(&hub(&[ring(-4.0, 0.0, 8.0, 20.0)]), 5.0, true, 5.5);
    assert!(end.open && end.out && end.step && !end.cone, "{end:?}");
    // The entry chamfered 1 by 1: the bore's mouth is 6 at z = -1.
    let entry = Turned {
        z0: -1.0,
        z1: 0.0,
        in0: 6.0,
        in1: 5.0,
        out0: 20.0,
        out1: 20.0,
    };
    let end = end_of(&hub(&[entry]), 5.0, true, 5.5);
    assert!(end.open && end.out && end.cone && !end.step, "{end:?}");
    assert!(
        (end.end - 1.0).abs() < 1e-12 && (end.rim - 6.0).abs() < 1e-6,
        "{end:?}"
    );
    assert!((end.through - 0.5).abs() < 1e-4, "{end:?}");
    // A bore that narrows past its end shuts it.
    let end = end_of(&hub(&[ring(-4.0, 0.0, 3.0, 20.0)]), 5.0, true, 5.5);
    assert_eq!(end, End::shut(5.0));
}

#[test]
fn what_stands_over_a_stretch_is_its_largest_radius() {
    let body = shaft(&[solid(-5.0, 0.0, 15.0)]);
    let mut probe = |p: &Probe| body.met(p);
    let n = near();
    let ask = |above: f64, from: f64, to: f64, probe: &mut dyn FnMut(&Probe) -> Met| {
        stands_over(&seat(), 10.0, &body.extent(), above, from, to, &n, probe)
    };
    assert!((ask(10.0, -5.0, 0.0, &mut probe).unwrap() - 15.0).abs() < 1e-12);
    // Nothing past the far end, an empty stretch, a body no wider than `above`: `above`.
    assert_eq!(ask(10.0, 30.0, 35.0, &mut probe), Ok(10.0));
    assert_eq!(ask(10.0, 0.0, 0.0, &mut probe), Ok(10.0));
    assert_eq!(ask(16.0, -5.0, 0.0, &mut probe), Ok(16.0));
    // A reading that cannot be made is refused, never taken as nothing there.
    let mut unread = |_: &Probe| Met::Unread;
    assert_eq!(
        ask(10.0, -5.0, 0.0, &mut unread),
        Err(BodyqError::Unsupported(BodyqUnsupported::Probe))
    );
}

#[test]
fn the_stretch_beside_a_bore_s_step_is_clear_within_its_ledge() {
    let body = hub(&[ring(-4.0, 0.0, 8.0, 20.0)]);
    let mut probe = |p: &Probe| body.met(p);
    assert_eq!(
        beside_clear(&seat(), 5.0, 6.0, &near(), &mut probe),
        Ok(true)
    );
    assert_eq!(
        beside_clear(&seat(), 5.0, 9.0, &near(), &mut probe),
        Ok(false)
    );
    let mut unread = |_: &Probe| Met::Unread;
    assert_eq!(
        beside_clear(&seat(), 5.0, 6.0, &near(), &mut unread),
        Err(BodyqError::Unsupported(BodyqUnsupported::Probe))
    );
}

#[test]
fn the_map_into_a_seat_takes_its_plane_to_the_origin() {
    // A plane through (1, 2, 3) with x along +Y and its normal (x cross y) +X: y is +Z.
    let plane = Frame::new(
        Point3::new(1.0, 2.0, 3.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    )
    .unwrap();
    assert!((plane.y().as_vec() - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-15);
    let s = SeatFrame { plane, ..seat() };
    let m = into_seat(&s).unwrap();
    let p = m.apply(Point3::new(6.0, 9.0, 14.0));
    assert!((p - Point3::new(7.0, 11.0, 5.0)).length() < 1e-12, "{p:?}");
    // The box of a body there reads as its extent: below zero past the end.
    let e = extent(&seat(), [-3.0, -4.0, -2.0], [5.0, 1.0, 30.0]).unwrap();
    assert_eq!((e.low, e.high, e.across), (-2.0, 30.0, 5.0));
    let back = SeatFrame {
        inward: Inward::Against,
        ..seat()
    };
    let e = extent(&back, [-3.0, -4.0, -2.0], [5.0, 1.0, 30.0]).unwrap();
    assert_eq!((e.low, e.high), (-30.0, 2.0));
}

#[test]
fn the_far_end_is_the_free_end_seen_from_the_other_side() {
    let far = far_end(&seat()).unwrap();
    assert_eq!(far.plane.origin(), Point3::new(0.0, 0.0, 30.0));
    assert_eq!((far.inward, far.room), (Inward::Against, 30.0));
    let back = far_end(&far).unwrap();
    assert_eq!(
        (back.plane.origin(), back.inward),
        (Point3::ORIGIN, Inward::Along)
    );
}

#[test]
fn the_block_of_a_grown_stretch_is_wider_than_the_body_and_the_face() {
    let body = Extent {
        low: -5.0,
        high: 30.0,
        across: 15.0,
    };
    let block = grow_block(&seat(), 10.0, &body, -2.0, 1.0, 0.5).unwrap();
    assert_eq!((block.radius, block.from, block.to), (28.0, -1.0, 30.5));
    let back = SeatFrame {
        inward: Inward::Against,
        ..seat()
    };
    let block = grow_block(&back, 10.0, &body, 2.0, 1.0, 0.5).unwrap();
    assert_eq!((block.from, block.to), (-30.5, 1.0));
    // A stretch of no length.
    assert_eq!(
        grow_block(&seat(), 10.0, &body, 2.0, -30.0, 0.0),
        Err(BodyqError::Refused(BodyqRefusal::NotPositive))
    );
}

// ---- the extent of a real body, in the seat's frame -----------------------------------------

fn cx_parts() -> (scx_brep::Document, Settings, Scratch, Stop) {
    (doc(), Settings::default(), Scratch::default(), Stop::new())
}

#[test]
fn a_real_shaft_is_measured_in_the_seat_s_frame_askew_or_not() {
    let m = cylinder(
        &Frame::WORLD,
        Length::from_mm(10.0),
        Length::from_mm(30.0),
        &doc(),
    )
    .unwrap();
    let (d, settings, mut scratch, stop) = cx_parts();
    let mut cx = Cx::new(&d, &settings, &mut scratch, &stop, &scx_brep::op::NoTrace);
    let e = extent_in(&m.body, &seat(), &mut cx).unwrap();
    assert!(
        e.low.abs() < 1e-4 && (e.high - 30.0).abs() < 1e-4 && (e.across - 10.0).abs() < 1e-4,
        "{e:?}"
    );
    // The same shaft seen from a seat at its far end, running back: the far end is the free
    // end, and the body reaches the whole 30 into the face.
    let far = far_end(&seat()).unwrap();
    let e = extent_in(&m.body, &far, &mut cx).unwrap();
    assert!(e.low.abs() < 1e-4 && (e.high - 30.0).abs() < 1e-4, "{e:?}");
    // A seat askew to the world axes: the shaft built along a tilted axis reads the same.
    let tilt = Frame::new(
        Point3::new(5.0, -2.0, 1.0),
        Vec3::new(1.0, 2.0, 2.0),
        Vec3::new(0.0, 0.0, 1.0),
    )
    .unwrap();
    let tilted = cylinder(&tilt, Length::from_mm(10.0), Length::from_mm(30.0), &doc()).unwrap();
    let askew = SeatFrame {
        plane: tilt,
        ..seat()
    };
    let e = extent_in(&tilted.body, &askew, &mut cx).unwrap();
    assert!(
        e.low.abs() < 1e-4 && (e.high - 30.0).abs() < 1e-4 && (e.across - 10.0).abs() < 1e-4,
        "{e:?}"
    );
}

// ---- whole band on real bodies -------------------------------------------------------------

#[test]
fn a_shaft_s_round_face_is_a_whole_band_and_another_radius_or_axis_has_none() {
    let m = cylinder(
        &Frame::WORLD,
        Length::from_mm(10.0),
        Length::from_mm(30.0),
        &doc(),
    )
    .unwrap();
    assert_eq!(whole_band(&m.body, &seat(), 10.0, &near()), Ok(Band::Whole));
    assert_eq!(whole_band(&m.body, &seat(), 4.0, &near()), Ok(Band::Absent));
    let askew = SeatFrame {
        plane: Frame::new(
            Point3::ORIGIN,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        )
        .unwrap(),
        ..seat()
    };
    assert_eq!(whole_band(&m.body, &askew, 10.0, &near()), Ok(Band::Absent));
    // An axis offset by more than coincidence is another face's.
    let off = SeatFrame {
        plane: Frame::new(
            Point3::new(0.01, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap(),
        ..seat()
    };
    assert_eq!(whole_band(&m.body, &off, 10.0, &near()), Ok(Band::Absent));
}

#[test]
fn a_stadiums_half_cylinders_are_whole_bands_of_their_own_axes() {
    // The arcs of radius 2 about (6, 7) and (-6, 7): each wall's edges are arcs across its
    // axis and the verticals along it.
    let (body, _) = stadium();
    let plane = Frame::new(
        Point3::new(6.0, 7.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let s = SeatFrame {
        plane,
        room: 3.0,
        ..seat()
    };
    assert_eq!(whole_band(&body, &s, 2.0, &near()), Ok(Band::Whole));
}

#[test]
fn a_rim_that_leans_is_no_band_end_and_one_held_as_an_ellipse_is_undecided_when_square() {
    // A scoop cut by a plane square to the axis: its rim is an ellipse of equal semi-axes (a
    // circle in another form): at one height all round, so not refuted; and not proved.
    let (square, _) = tilted_sheet(0.0, 2.0);
    let s = SeatFrame {
        plane: Frame::new(
            Point3::ORIGIN,
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap(),
        room: 2.0,
        ..seat()
    };
    assert!(matches!(
        whole_band(&square, &s, 2.0, &near()).unwrap(),
        Band::Undecided { .. }
    ));
    // Cut slantwise (the axis tilted from the z plane the floor lies in): the rim ellipse has
    // points at two heights along the axis and at two places round it, so no band end.
    let tilt = 0.4_f64;
    let (slant, _) = tilted_sheet(tilt, 2.0);
    let axis = Vec3::new(scx_num::fmath::sin(tilt), 0.0, scx_num::fmath::cos(tilt));
    let s = SeatFrame {
        plane: Frame::new(Point3::ORIGIN, axis, Vec3::new(0.0, -1.0, 0.0)).unwrap(),
        room: 2.0,
        ..seat()
    };
    assert!(matches!(
        whole_band(&slant, &s, 2.0, &near()).unwrap(),
        Band::Broken { .. }
    ));
}

// ---- hostile numbers -----------------------------------------------------------------------

#[test]
fn hostile_numbers_are_refused_or_answered_plainly() {
    let body = shaft(&[chamfer(1.0, 9.0)]);
    let n = near();
    let mut unread = |_: &Probe| Met::Unread;
    let mut garbage = |_: &Probe| Met::Piece {
        min: [f64::NAN; 3],
        max: [f64::INFINITY; 3],
    };
    // Unread probes are not read as a shut end.
    assert_eq!(
        past_end(&seat(), 10.0, false, 9.0, &body.extent(), &n, &mut unread),
        Err(BodyqError::Unsupported(BodyqUnsupported::Probe))
    );
    // A first probe that meets something nonsensical still meets something in the way: shut.
    assert_eq!(
        past_end(&seat(), 10.0, true, 9.0, &body.extent(), &n, &mut garbage),
        Ok(End::shut(10.0))
    );
    // A later one that answers nonsense is refused as a number, not read.
    let asked = AtomicUsize::new(0);
    let mut late = |p: &Probe| {
        asked.fetch_add(1, Ordering::Relaxed);
        if asked.load(Ordering::Relaxed) == 1 {
            body.met(p)
        } else {
            garbage(p)
        }
    };
    assert_eq!(
        past_end(&seat(), 10.0, false, 9.0, &body.extent(), &n, &mut late),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
    for bad in HOSTILE {
        let s = SeatFrame {
            room: bad,
            ..seat()
        };
        let asked = AtomicUsize::new(0);
        let mut probe = |p: &Probe| {
            asked.fetch_add(1, Ordering::Relaxed);
            body.met(p)
        };
        for bore in [false, true] {
            let _ = past_end(&seat(), bad, bore, 9.0, &body.extent(), &n, &mut probe);
            let _ = past_end(&seat(), 10.0, bore, bad, &body.extent(), &n, &mut probe);
            let low = Extent {
                low: bad,
                ..body.extent()
            };
            let _ = past_end(&seat(), 10.0, bore, 9.0, &low, &n, &mut probe);
        }
        assert!(
            asked.load(Ordering::Relaxed) <= 8 * 6,
            "{bad}: {} probes",
            asked.load(Ordering::Relaxed)
        );
        let _ = stands_over(&s, 10.0, &body.extent(), 10.0, -1.0, 0.0, &n, &mut probe);
        let _ = stands_over(
            &seat(),
            10.0,
            &body.extent(),
            10.0,
            bad,
            0.0,
            &n,
            &mut probe,
        );
        let _ = stands_over(
            &seat(),
            10.0,
            &body.extent(),
            10.0,
            -1.0,
            bad,
            &n,
            &mut probe,
        );
        let _ = far_end(&s);
        if !bad.is_finite() {
            assert_eq!(
                stands_over(
                    &seat(),
                    10.0,
                    &body.extent(),
                    bad,
                    -1.0,
                    0.0,
                    &n,
                    &mut probe
                ),
                Err(BodyqError::Refused(BodyqRefusal::NotFinite))
            );
            assert!(extent(&seat(), [bad; 3], [1.0; 3]).is_err());
            assert!(grow_block(&seat(), 10.0, &body.extent(), bad, 0.0, 0.0).is_err());
            assert!(grow_block(&seat(), 10.0, &body.extent(), 1.0, bad, 0.0).is_err());
        }
        if !(bad.is_finite() && bad > 0.0) {
            assert!(past_end(&seat(), bad, false, 9.0, &body.extent(), &n, &mut probe).is_err());
            assert!(
                stands_over(
                    &seat(),
                    bad,
                    &body.extent(),
                    10.0,
                    -1.0,
                    0.0,
                    &n,
                    &mut probe
                )
                .is_err()
            );
            assert!(beside_clear(&seat(), 10.0, bad, &n, &mut probe).is_err());
            assert!(beside_clear(&seat(), bad, 5.0, &n, &mut probe).is_err());
            let m = cylinder(
                &Frame::WORLD,
                Length::from_mm(10.0),
                Length::from_mm(30.0),
                &doc(),
            )
            .unwrap();
            assert!(whole_band(&m.body, &seat(), bad, &n).is_err());
            assert!(grow_block(&seat(), bad, &body.extent(), 1.0, 0.0, 0.0).is_err());
        }
    }
}
