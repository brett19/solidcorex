// Written for solidcorex. Apache-2.0.
//! The profile fixtures of `scx_testkit::profile` (research 72 §14.3): every fixture gives
//! its stated verdict, an accepted one its area and reversal flags; and the verdict's kind
//! is the same with every loop reversed, every start rotated and the holes permuted.

use scx_num::units::{Length, Resolution};
use scx_num::vector2d::Point2;
use scx_planar::{LoopId, PlanarError, Region, Segment, Sense};
use scx_testkit::profile::{Cause, Fixture, Seg, Verdict, fixtures, reversed};

/// The one adapter: a fixture's plain data to `Region::new`.
fn build(outer: &[Seg], holes: &[Vec<Seg>], res: f64) -> Result<Region, PlanarError> {
    let seg = |s: &Seg| match *s {
        Seg::Line { start, end } => Segment::line(Point2::from(start), Point2::from(end)),
        Seg::Arc {
            centre,
            start,
            end,
            ccw,
        } => Segment::arc(
            Point2::from(centre),
            Point2::from(start),
            Point2::from(end),
            if ccw { Sense::Forward } else { Sense::Reversed },
        ),
    };
    let res = Resolution::new(Length::from_mm(res)).ok_or(PlanarError::Empty {
        in_loop: LoopId::Outer,
    })?;
    Region::new(
        outer.iter().map(seg).collect(),
        holes.iter().map(|h| h.iter().map(seg).collect()).collect(),
        res,
    )
}

fn loop_index(id: LoopId) -> usize {
    match id {
        LoopId::Outer => 0,
        LoopId::Hole(i) => 1 + i,
    }
}

/// The fixture's verdict checked against what was built; `None` when they agree.
fn disagreement(f: &Fixture, got: &Result<Region, PlanarError>) -> Option<String> {
    match (&f.verdict, got) {
        (
            Verdict::Accepted {
                area,
                outer_reversed,
                holes_reversed,
            },
            Ok(r),
        ) => {
            let ok = (r.area() - area).abs() <= 1e-12 * area.abs()
                && r.outer_reversed() == *outer_reversed
                && r.holes_reversed() == holes_reversed.as_slice()
                && r.outer().signed_area() > 0.0
                && r.holes().iter().all(|h| h.signed_area() < 0.0);
            (!ok).then(|| {
                format!(
                    "area {} (want {area}), reversed {} {:?}",
                    r.area(),
                    r.outer_reversed(),
                    r.holes_reversed()
                )
            })
        }
        (Verdict::Refused(want), Err(e)) => {
            let ok = match (*want, *e) {
                (
                    Cause::Open { loop_index: k, gap },
                    PlanarError::Open {
                        in_loop, gap: g, ..
                    },
                ) => loop_index(in_loop) == k && g == gap,
                (Cause::Crossing, PlanarError::Crossing { .. })
                | (Cause::Touching, PlanarError::Touching { .. }) => true,
                (Cause::HoleOutside { hole }, PlanarError::HoleOutside { hole: h }) => h == hole,
                (
                    Cause::HoleInHole { outer, inner },
                    PlanarError::HoleInHole { outer: o, inner: i },
                ) => (o, i) == (outer, inner),
                _ => false,
            };
            (!ok).then(|| format!("{e:?}"))
        }
        (_, Ok(r)) => Some(format!("accepted with area {}", r.area())),
        (_, Err(e)) => Some(format!("{e:?}")),
    }
}

#[test]
fn every_fixture_gives_its_verdict() {
    let mut bad = Vec::new();
    for f in fixtures() {
        let got = build(&f.outer, &f.holes, f.resolution);
        if let Some(why) = disagreement(&f, &got) {
            bad.push(format!("{}: {why}", f.name));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The verdict's kind only, with an accepted region's area.
fn outcome(got: &Result<Region, PlanarError>) -> String {
    match got {
        Ok(r) => format!("accepted {:.9}", r.area()),
        Err(e) => e.name().to_owned(),
    }
}

fn rotated(l: &[Seg], k: usize) -> Vec<Seg> {
    let k = k % l.len().max(1);
    l.iter().cycle().skip(k).take(l.len()).copied().collect()
}

#[test]
fn the_verdict_does_not_depend_on_how_the_loops_are_given() {
    let mut bad = Vec::new();
    for f in fixtures() {
        // An open loop's gap moves with its start, and reversing it moves the gap's joint;
        // the cause stays Open.
        let want = outcome(&build(&f.outer, &f.holes, f.resolution));
        for k in 0..4 {
            for flip in [false, true] {
                let turn = |l: &[Seg]| {
                    let l = rotated(l, k);
                    if flip { reversed(&l) } else { l }
                };
                let mut holes: Vec<Vec<Seg>> = f.holes.iter().map(|h| turn(h)).collect();
                // Holes permuted: the indices in a cause move with them, the kind does not.
                holes.reverse();
                let got = outcome(&build(&turn(&f.outer), &holes, f.resolution));
                if got != want {
                    bad.push(format!("{} k={k} flip={flip}: {got} (want {want})", f.name));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn the_reported_pair_is_the_least() {
    // Bow tie: segments 0 and 2 cross; that pair is reported in that order.
    let f = fixtures()
        .into_iter()
        .find(|f| f.name == "bow tie")
        .unwrap_or_else(|| unreachable!());
    let got = build(&f.outer, &f.holes, f.resolution);
    let Err(PlanarError::Crossing { a, b }) = got else {
        unreachable!("{got:?}")
    };
    assert_eq!((a.segment, b.segment), (0, 2));
    assert_eq!((a.in_loop, b.in_loop), (LoopId::Outer, LoopId::Outer));
}
