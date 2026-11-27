// Written for solidcorex. Apache-2.0.
//! Closed loops and regions: an outer loop with holes, validated once when built.

use scx_algo::box_tree::BoxTree;
use scx_num::certified::Verdict;
use scx_num::extent2d::Extent2d;
use scx_num::units::Resolution;
use scx_num::vector2d::Point2;

use crate::cert::{IP, Iv, greater};
use crate::contact::{Contact, Shape, contact, point_apart};
use crate::error::{Degenerate, LoopId, PlanarError, SegmentRef};
use crate::segment::Segment;
use crate::winding::{orientation, signed_area, winding};

/// A closed loop: each segment ends exactly where the next (cyclically) starts.
#[derive(Debug, Clone, PartialEq)]
pub struct Loop {
    segments: Vec<Segment>,
}

impl Loop {
    /// Its segments in order.
    #[must_use]
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// The area it encloses, positive when counter-clockwise.
    #[must_use]
    pub fn signed_area(&self) -> f64 {
        signed_area(&self.segments).0
    }

    /// The number of turns it makes about `p` counter-clockwise, for a point off it.
    fn winding(&self, p: Point2) -> Option<i64> {
        winding(&self.segments, p)
    }
}

/// Where a point lies against a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Inside the outer loop and outside every hole, more than the resolution from each.
    Inside,
    /// Outside the outer loop or inside a hole, more than the resolution from each.
    Outside,
    /// Not proved more than the resolution from the boundary.
    Boundary,
}

/// A planar region: one outer loop, counter-clockwise, and holes inside it, clockwise;
/// no two segments cross or come within the resolution away from a shared joint, and no
/// hole lies in another.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    outer: Loop,
    holes: Vec<Loop>,
    resolution: Resolution,
    outer_reversed: bool,
    holes_reversed: Vec<bool>,
}

/// A loop being checked: its id and its segments.
struct Pending {
    id: LoopId,
    segments: Vec<Segment>,
}

impl Region {
    /// The region bounded by `outer` with `holes`, at resolution `res`.
    ///
    /// The checks run in this order, and the first that fails is the one cause returned:
    ///
    /// 1. every coordinate is finite ([`PlanarError::NotFinite`]) and in range
    ///    ([`PlanarError::OutOfRange`]), no loop is empty ([`PlanarError::Empty`]) and a
    ///    whole circle is alone in its loop ([`PlanarError::CircleWithOthers`]): closure
    ///    cannot be judged on a NaN, nor between a circle and another segment;
    /// 2. every loop closes ([`PlanarError::Open`]): each segment ends within `res` of
    ///    where the next starts, the outer loop first, then the holes, the first joint
    ///    first. The joints are then made exact: each segment's end becomes the next
    ///    segment's start;
    /// 3. no segment is degenerate ([`PlanarError::Degenerate`]);
    /// 4. no two segments cross ([`PlanarError::Crossing`]) or touch
    ///    ([`PlanarError::Touching`]), within or between loops: a crossing anywhere is
    ///    reported before a touch, each as the least pair by loop and index;
    /// 5. every hole is inside the outer loop ([`PlanarError::HoleOutside`]) and none is
    ///    inside another ([`PlanarError::HoleInHole`]), least index first;
    /// 6. each loop's orientation is decided by its signed area; the outer loop is made
    ///    counter-clockwise and the holes clockwise, reversing those given otherwise
    ///    ([`Region::outer_reversed`], [`Region::holes_reversed`]).
    pub fn new(
        outer: Vec<Segment>,
        holes: Vec<Vec<Segment>>,
        res: Resolution,
    ) -> Result<Self, PlanarError> {
        let mut loops: Vec<Pending> = core::iter::once(Pending {
            id: LoopId::Outer,
            segments: outer,
        })
        .chain(holes.into_iter().enumerate().map(|(i, segments)| Pending {
            id: LoopId::Hole(i),
            segments,
        }))
        .collect();
        let r = res.zone().mm();
        for l in &loops {
            check_numbers(l)?;
        }
        for l in &mut loops {
            close(l, r)?;
        }
        for l in &loops {
            check_degenerate(l, r)?;
        }
        check_contacts(&loops, r)?;
        check_nesting(&loops)?;
        let mut out = loops.into_iter().map(|l| {
            let ccw = orientation(&l.segments).ok_or(PlanarError::Undecided { in_loop: l.id })?;
            Ok((l, ccw))
        });
        let (outer, outer_ccw) = out.next().ok_or(PlanarError::Empty {
            in_loop: LoopId::Outer,
        })??;
        let outer_reversed = !outer_ccw;
        let mut holes = Vec::new();
        let mut holes_reversed = Vec::new();
        for h in out {
            let (h, ccw) = h?;
            holes.push(oriented(h.segments, !ccw));
            holes_reversed.push(ccw);
        }
        Ok(Self {
            outer: oriented(outer.segments, outer_ccw),
            holes,
            resolution: res,
            outer_reversed,
            holes_reversed,
        })
    }

    /// The outer loop, counter-clockwise.
    #[must_use]
    pub const fn outer(&self) -> &Loop {
        &self.outer
    }

    /// The holes, clockwise, in the order given.
    #[must_use]
    pub fn holes(&self) -> &[Loop] {
        &self.holes
    }

    /// The resolution it was built at.
    #[must_use]
    pub const fn resolution(&self) -> Resolution {
        self.resolution
    }

    /// The outer loop was given clockwise and was reversed.
    #[must_use]
    pub const fn outer_reversed(&self) -> bool {
        self.outer_reversed
    }

    /// Per hole, it was given counter-clockwise and was reversed.
    #[must_use]
    pub fn holes_reversed(&self) -> &[bool] {
        &self.holes_reversed
    }

    /// The area: the outer loop's, less the holes'.
    #[must_use]
    pub fn area(&self) -> f64 {
        self.outer.signed_area() + self.holes.iter().map(Loop::signed_area).sum::<f64>()
    }

    /// Where `p` lies: [`Place::Boundary`] unless it is proved more than the resolution
    /// from every segment, then by winding number.
    pub fn classify(&self, p: Point2) -> Result<Place, PlanarError> {
        let ip = IP::exact(p);
        let res = Iv::exact(self.resolution.zone().mm());
        let all = core::iter::once(&self.outer).chain(&self.holes);
        for l in all {
            for s in &l.segments {
                if point_apart(ip, &Shape::new(s), res) != Verdict::Yes {
                    return Ok(Place::Boundary);
                }
            }
        }
        let wind = |l: &Loop, id| l.winding(p).ok_or(PlanarError::Undecided { in_loop: id });
        if wind(&self.outer, LoopId::Outer)? == 0 {
            return Ok(Place::Outside);
        }
        for (i, h) in self.holes.iter().enumerate() {
            if wind(h, LoopId::Hole(i))? != 0 {
                return Ok(Place::Outside);
            }
        }
        Ok(Place::Inside)
    }
}

fn oriented(segments: Vec<Segment>, keep: bool) -> Loop {
    let segments = if keep {
        segments
    } else {
        segments.iter().rev().map(Segment::reversed).collect()
    };
    Loop { segments }
}

/// The largest coordinate magnitude accepted: a kilometre's thousandfold, in millimetres.
const MAX_COORD: f64 = 1e9;
/// The least non-zero coordinate magnitude accepted, keeping the orientation sign exact.
const MIN_COORD: f64 = 1e-100;

fn check_numbers(l: &Pending) -> Result<(), PlanarError> {
    if l.segments.is_empty() {
        return Err(PlanarError::Empty { in_loop: l.id });
    }
    for (i, s) in l.segments.iter().enumerate() {
        let at = SegmentRef {
            in_loop: l.id,
            segment: i,
        };
        let c = s.coordinates();
        if c.iter().any(|x| !x.is_finite()) {
            return Err(PlanarError::NotFinite { at });
        }
        if c.iter()
            .any(|x| *x != 0.0 && !(MIN_COORD..=MAX_COORD).contains(&x.abs()))
        {
            return Err(PlanarError::OutOfRange { at });
        }
    }
    if l.segments.len() > 1
        && let Some(segment) = l
            .segments
            .iter()
            .position(|s| matches!(s, Segment::Circle { .. }))
    {
        return Err(PlanarError::CircleWithOthers {
            at: SegmentRef {
                in_loop: l.id,
                segment,
            },
        });
    }
    Ok(())
}

/// Checks each joint closes within `res` and makes it exact.
fn close(l: &mut Pending, res: f64) -> Result<(), PlanarError> {
    let n = l.segments.len();
    let res2 = Iv::exact(res).sq();
    let starts: Vec<Point2> = l.segments.iter().map(Segment::start).collect();
    for (j, (seg, next)) in l
        .segments
        .iter_mut()
        .zip(starts.iter().cycle().skip(1))
        .enumerate()
    {
        let gap2 = IP::exact(seg.end()).sub(IP::exact(*next)).norm2();
        if greater(gap2, res2) != Verdict::No {
            return Err(PlanarError::Open {
                in_loop: l.id,
                joint: j,
                gap: seg.end().distance(*next),
            });
        }
        *seg = seg.with_end(*next);
    }
    debug_assert_eq!(starts.len(), n);
    Ok(())
}

fn check_degenerate(l: &Pending, res: f64) -> Result<(), PlanarError> {
    let ires = Iv::exact(res);
    let res2 = ires.sq();
    for (i, s) in l.segments.iter().enumerate() {
        let at = SegmentRef {
            in_loop: l.id,
            segment: i,
        };
        let fail = |why| Err(PlanarError::Degenerate { at, why });
        if let Segment::Circle { radius, .. } = *s {
            // No ends: only the radius can be degenerate.
            if greater(Iv::exact(radius), ires) != Verdict::Yes {
                return fail(Degenerate::Radius);
            }
            continue;
        }
        if let Segment::Arc {
            centre, start, end, ..
        } = *s
        {
            let c = IP::exact(centre);
            let rs = IP::exact(start).sub(c).norm2().sqrt();
            let re = IP::exact(end).sub(c).norm2().sqrt();
            if greater(rs.hull(re), ires) != Verdict::Yes
                || greater(rs, ires) != Verdict::Yes
                || greater(re, ires) != Verdict::Yes
            {
                return fail(Degenerate::Radius);
            }
            let dr = rs - re;
            if greater(dr, ires) == Verdict::Yes || greater(-dr, ires) == Verdict::Yes {
                return fail(Degenerate::EndOffCircle);
            }
        }
        let len2 = IP::exact(s.end()).sub(IP::exact(s.start())).norm2();
        if greater(len2, res2) != Verdict::Yes {
            return fail(Degenerate::Short);
        }
    }
    Ok(())
}

/// The pairs `i < j` of segments whose boxes (grown by the resolution's reach) overlap,
/// ascending, from `scx-algo`'s box tree: `O(n log n)` to build, and a self-join whose cost
/// follows the overlapping pairs in both axes (a sweep on one axis costs `O(n^2)` when every
/// box spans the same `x`, a comb on its side). Every box is finite (the numbers are checked
/// first); were one not, every pair is a candidate.
pub(crate) fn candidate_pairs(boxes: &[[f64; 4]]) -> Vec<(usize, usize)> {
    let extents: Option<Vec<Extent2d>> = boxes
        .iter()
        .map(|b| Extent2d::from_bounds(b[0], b[2], b[1], b[3]))
        .collect();
    match extents {
        Some(e) => BoxTree::new(&e).self_pairs(0.0),
        None => (0..boxes.len())
            .flat_map(|i| (i + 1..boxes.len()).map(move |j| (i, j)))
            .collect(),
    }
}

fn check_contacts(loops: &[Pending], res: f64) -> Result<(), PlanarError> {
    // Every segment, flattened, with its reference, its shape and its box.
    let mut refs = Vec::new();
    let mut shapes = Vec::new();
    let mut boxes = Vec::new();
    let mut lens = Vec::new();
    for l in loops {
        lens.push(l.segments.len());
        for (i, s) in l.segments.iter().enumerate() {
            let shape = Shape::new(s);
            refs.push(SegmentRef {
                in_loop: l.id,
                segment: i,
            });
            boxes.push(shape.bbox(3.0 * res));
            shapes.push((shape, *s));
        }
    }
    let mut worst: Option<(Contact, SegmentRef, SegmentRef)> = None;
    for (i, j) in candidate_pairs(&boxes) {
        let (Some(&ra), Some(&rb), Some((sa, ga)), Some((sb, gb))) =
            (refs.get(i), refs.get(j), shapes.get(i), shapes.get(j))
        else {
            continue;
        };
        let joints = shared_joints(ra, ga, rb, gb, loops);
        let c = contact(sa, sb, &joints, res);
        if c == Contact::Apart {
            continue;
        }
        let (a, b) = if ra <= rb { (ra, rb) } else { (rb, ra) };
        let better = match worst {
            None => true,
            Some((wc, wa, wb)) => c > wc || (c == wc && (a, b) < (wa, wb)),
        };
        if better {
            worst = Some((c, a, b));
        }
    }
    match worst {
        Some((Contact::Crossing, a, b)) => Err(PlanarError::Crossing { a, b }),
        Some((_, a, b)) => Err(PlanarError::Touching { a, b }),
        None => Ok(()),
    }
}

/// The joints two segments share as neighbours in one loop: `a`'s end where `b` starts, or
/// `b`'s end where `a` starts (both, in a loop of two).
fn shared_joints(
    ra: SegmentRef,
    ga: &Segment,
    rb: SegmentRef,
    gb: &Segment,
    loops: &[Pending],
) -> Vec<Point2> {
    if ra.in_loop != rb.in_loop {
        return Vec::new();
    }
    let n = loops
        .iter()
        .find(|l| l.id == ra.in_loop)
        .map_or(0, |l| l.segments.len());
    let mut joints = Vec::new();
    if (ra.segment + 1) % n == rb.segment {
        joints.push(ga.end());
    }
    if (rb.segment + 1) % n == ra.segment && !joints.contains(&gb.end()) {
        joints.push(gb.end());
    }
    joints
}

fn check_nesting(loops: &[Pending]) -> Result<(), PlanarError> {
    let Some((outer, holes)) = loops.split_first() else {
        return Ok(());
    };
    let vertex = |l: &Pending| l.segments.first().map(Segment::start);
    for (i, h) in holes.iter().enumerate() {
        let Some(p) = vertex(h) else { continue };
        let w = winding(&outer.segments, p).ok_or(PlanarError::Undecided {
            in_loop: LoopId::Outer,
        })?;
        if w == 0 {
            return Err(PlanarError::HoleOutside { hole: i });
        }
    }
    // A hole in another has its box inside the other's, so the boxes overlap: the box tree's
    // pairs, both ways round, in the order of (outer, inner).
    let boxes: Vec<[f64; 4]> = holes.iter().map(|h| loop_box(&h.segments)).collect();
    let within = |i: usize, j: usize| match (boxes.get(i), boxes.get(j)) {
        (Some(bi), Some(bj)) => {
            bi[0] <= bj[0] && bj[1] <= bi[1] && bi[2] <= bj[2] && bj[3] <= bi[3]
        }
        _ => false,
    };
    let mut nested: Vec<(usize, usize)> = candidate_pairs(&boxes)
        .into_iter()
        .flat_map(|(i, j)| [(i, j), (j, i)])
        .filter(|&(i, j)| within(i, j))
        .collect();
    nested.sort_unstable();
    for (i, j) in nested {
        let (Some(hi), Some(p)) = (holes.get(i), holes.get(j).and_then(vertex)) else {
            continue;
        };
        let w = winding(&hi.segments, p).ok_or(PlanarError::Undecided {
            in_loop: LoopId::Hole(i),
        })?;
        if w != 0 {
            return Err(PlanarError::HoleInHole { outer: i, inner: j });
        }
    }
    Ok(())
}

fn loop_box(segments: &[Segment]) -> [f64; 4] {
    segments.iter().map(|s| Shape::new(s).bbox(0.0)).fold(
        [
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ],
        |a, b| {
            [
                a[0].min(b[0]),
                a[1].max(b[1]),
                a[2].min(b[2]),
                a[3].max(b[3]),
            ]
        },
    )
}
