// Written for solidcorex. Apache-2.0.
//! A face's boundary tree (research 63 §6.9): its loops in uv as pieces with certified
//! boxes, a box tree over them, the uv box of its region, and a point-in-face test that
//! answers only what the enclosures prove.
//!
//! **The pieces.** Each use's pcurve is cut into parts (one for a line, [`PARTS`] for any
//! other kind), each boxed by one interval evaluation over its parameter range, in the
//! loop's direction: a pcurve runs over the edge's interval min to max, so a reversed use
//! walks its parts backwards. Where a use ends and the next starts, a straight gap piece
//! joins the two endpoints unless both are the same exact point. A point use (a pole, a
//! lone vertex) is a point the gaps pass through. So each loop is a closed chain, every
//! piece ending where the next starts. A piece keeps its start's enclosure; its end is the
//! next piece's start.
//!
//! **The region** (01 §2.1's orientation). The outer loop runs counter-clockwise about the
//! natural normal and the inner loops clockwise, so the region is where the winding number
//! of the chains is `w_in`: 1 when the signed area is above zero (an outer loop), 0 when it
//! is below or there is no loop (holes in the whole domain). The area is enclosed in
//! intervals, the parts refined until its sign shows; a face whose sign never shows (a
//! sliver, a loop that does not close in uv) has a uv box but no point test.
//!
//! **Point in face.** The winding number about the query point, read from a ray: a piece
//! whose box is wholly above, below or behind the point adds nothing; a run of consecutive
//! pieces wholly ahead adds the change in "at or above" between its first start and its
//! last end, which telescopes, so a whole subtree costs two endpoint tests; a piece whose
//! box meets the point is bisected. An endpoint level with the point leaves the ray
//! undecided, and the next of four rays (`+u`, `-u`, `+v`, `-v`) is tried. A piece still
//! meeting the point at [`DEPTH`] halvings answers [`UvSide::Near`]. The query point is an
//! interval, so a lift by a period is certified too.

use core::cmp::Ordering;
use std::collections::BTreeMap;

use scx_curve::{Circle2, Curve2};
use scx_num::domain::ParamDomain;
use scx_num::extent2d::Extent2d;
use scx_num::frame::Sense;
use scx_num::interval::Ivl;
use scx_num::vector2d::Point2;
use scx_nurbs::Approach;

use crate::cells::parts;
use crate::id::EdgeId;
use crate::record::{FaceRecord, UseGeom};

/// Where a uv point lies against a face's region, as far as the enclosures prove.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UvSide {
    /// Inside the region, away from its boundary.
    Inside,
    /// Outside the region, away from its boundary.
    Outside,
    /// So close to the boundary that the enclosures cannot tell: on it, or within a few
    /// rounding errors of it.
    Near,
}

/// The parts a curved pcurve is cut into for its boxes.
const PARTS: usize = 4;
/// The further cuts of each part, in turn, until the signed area's sign shows: up to 64
/// parts per use.
const AREA_REFINE: [usize; 5] = [1, 2, 4, 8, 16];
/// Pieces per leaf of the box tree.
const LEAF: usize = 4;
/// Halvings of one piece before a point meeting its box is [`UvSide::Near`].
const DEPTH: u32 = 48;
/// Box evaluations one query may spend on halvings before it answers `None`.
const BUDGET: u32 = 4096;
/// Lifts by a period one query may try per axis.
const MAX_LIFTS: i64 = 8;

/// An enclosure of a uv point.
type P = Point2<Ivl>;

/// A face's boundary tree and the uv box of its region.
#[derive(Debug, Clone)]
pub(crate) struct Boundary {
    uv_box: Option<Extent2d>,
    tree: Option<Tree>,
    /// Every piece is enclosed and the area's sign showed: the tree answers queries.
    decided: bool,
}

#[derive(Debug, Clone)]
struct Tree {
    pieces: Vec<Piece>,
    nodes: Vec<Node>,
    runs: Vec<Run>,
    /// The hull of every piece; `None` with no piece.
    hull: Option<Extent2d>,
    w_in: i32,
}

/// One loop's pieces, `first..end`, its tree's root, the loop's index, and its part of the
/// signed area at each refinement tried (`None` where not tried), about `c`.
#[derive(Debug, Clone, Copy)]
struct Run {
    first: u32,
    end: u32,
    root: u32,
    lp: u32,
    area: [Option<Ivl>; AREA_REFINE.len()],
}

#[derive(Debug, Clone, Copy)]
struct Piece {
    start: P,
    bx: Extent2d,
    src: Src,
}

#[derive(Debug, Clone, Copy)]
enum Src {
    /// Part `lo..hi` of the pcurve of use `us` of loop `lp`, walked down when `rev`.
    Part {
        lp: u32,
        us: u32,
        lo: f64,
        hi: f64,
        rev: bool,
    },
    /// The straight join from this piece's start to the next piece's.
    Gap,
}

/// A tree node over pieces `first..end`; its left child follows it, its right child is at
/// `right` (0 for a leaf: no node points back to the root).
#[derive(Debug, Clone, Copy)]
struct Node {
    bx: Extent2d,
    first: u32,
    end: u32,
    right: u32,
}

impl Boundary {
    /// The boundary of `rec`.
    pub(crate) fn build(rec: &FaceRecord) -> Self {
        Self::carried(rec, None).0
    }

    /// The boundary of `rec`, the same bits as [`Boundary::build`], taking each loop's
    /// pieces (and its part of the area, where the centre is the same) from `old`'s run
    /// of an equal loop, and for each of `rec`'s runs the index of the run of `old` it took.
    pub(crate) fn carried(
        rec: &FaceRecord,
        old: Option<(&FaceRecord, &Self)>,
    ) -> (Self, Vec<Option<usize>>) {
        let window = window(&rec.surface);
        let old = old.and_then(|(r, b)| Some((r, b.tree.as_ref()?)));
        let Some((mut tree, from)) = Tree::build(rec, old) else {
            return (
                Self {
                    uv_box: None,
                    tree: None,
                    decided: false,
                },
                Vec::new(),
            );
        };
        let sign = if tree.runs.is_empty() {
            Some(Ordering::Less)
        } else {
            tree.area_sign(rec)
        };
        let (uv_box, decided) = match sign {
            Some(Ordering::Greater) => {
                tree.w_in = 1;
                (tree.hull, true)
            }
            Some(_) => (window, true),
            None => (tree.hull.zip(window).map(|(h, w)| h.union(w)), false),
        };
        (
            Self {
                uv_box,
                tree: Some(tree),
                decided,
            },
            from,
        )
    }

    /// Each run's pieces' boxes, the runs in loop order.
    pub(crate) fn run_boxes(
        &self,
    ) -> impl Iterator<Item = impl Iterator<Item = Extent2d> + '_> + '_ {
        self.tree.iter().flat_map(|t| {
            t.runs.iter().map(|r| {
                t.pieces
                    .get(r.first as usize..r.end as usize)
                    .unwrap_or_default()
                    .iter()
                    .map(|p| p.bx)
            })
        })
    }

    /// A box holding the face's region in uv: the hull of its loops when it has an outer
    /// loop, the domain's window (one period of a closed direction) when it has none, and
    /// `None` when that is unbounded.
    pub(crate) const fn uv_box(&self) -> Option<Extent2d> {
        self.uv_box
    }

    /// Whether the loops' signed area showed its sign, so the region is proved.
    pub(crate) const fn is_decided(&self) -> bool {
        self.decided
    }

    /// Whether the region is bounded by an outer loop, proved.
    pub(crate) fn is_bounded_region(&self) -> bool {
        self.decided && self.tree.as_ref().is_some_and(|t| t.w_in == 1)
    }

    /// How many pieces, and how many bytes the cell holds on the heap.
    #[cfg(test)]
    pub(crate) fn footprint(&self) -> (usize, usize) {
        self.tree.as_ref().map_or((0, 0), |t| {
            (
                t.pieces.len(),
                t.pieces.capacity() * size_of::<Piece>()
                    + t.nodes.capacity() * size_of::<Node>()
                    + t.runs.capacity() * size_of::<Run>(),
            )
        })
    }

    /// Where `p` lies against the region; `None` when the enclosures cannot tell (the
    /// area's sign never showed, every ray met an endpoint level with `p`, or the halving
    /// budget ran out).
    pub(crate) fn side(&self, rec: &FaceRecord, p: Point2) -> Option<UvSide> {
        let tree = self.tree.as_ref().filter(|_| self.decided)?;
        if !(p.x.is_finite() && p.y.is_finite()) {
            return None;
        }
        let dom = rec.surface.domain();
        let mut edge = false;
        if tree.w_in == 0 {
            for (d, x) in [(dom.u, p.x), (dom.v, p.y)] {
                match axis_side(d, x) {
                    UvSide::Outside => return Some(UvSide::Outside),
                    UvSide::Near => edge = true,
                    UvSide::Inside => {}
                }
            }
        }
        let Some(hull) = tree.hull else {
            // No loop: the whole domain.
            return Some(if edge { UvSide::Near } else { UvSide::Inside });
        };
        let us = lifts(dom.u, p.x, hull.min().x, hull.max().x)?;
        let vs = lifts(dom.v, p.y, hull.min().y, hull.max().y)?;
        // The face holds the point if one lift of it is in the region (`w_in` 1), or if no
        // lift is in a hole (`w_in` 0).
        let (hit, miss) = if tree.w_in == 1 {
            (UvSide::Inside, UvSide::Outside)
        } else {
            (UvSide::Outside, UvSide::Inside)
        };
        let mut budget = BUDGET;
        let (mut near, mut unknown) = (false, false);
        for &u in &us {
            for &v in &vs {
                match tree.side_at(rec, (u, v), &mut budget) {
                    Some(s) if s == hit => return Some(hit),
                    Some(UvSide::Near) => near = true,
                    Some(_) => {}
                    None => unknown = true,
                }
            }
        }
        if near {
            Some(UvSide::Near)
        } else if unknown {
            None
        } else if edge {
            Some(UvSide::Near)
        } else {
            Some(miss)
        }
    }
}

impl Tree {
    /// The pieces and tree of `rec`'s loops; `None` when a piece cannot be enclosed. A
    /// loop equal to one of `old`'s that has a run takes that run's pieces (their boxes and
    /// starts are a function of the loop alone); the second part is, for each run, the run
    /// of `old` it took.
    fn build(
        rec: &FaceRecord,
        old: Option<(&FaceRecord, &Self)>,
    ) -> Option<(Self, Vec<Option<usize>>)> {
        // `old`'s runs by their loop's first use.
        let by_first: BTreeMap<(EdgeId, bool), Vec<usize>> = old
            .map(|(r, t)| {
                let mut m: BTreeMap<_, Vec<usize>> = BTreeMap::new();
                for (i, run) in t.runs.iter().enumerate() {
                    if let Some(u) = r.loops.get(run.lp as usize).and_then(|l| l.uses.first()) {
                        m.entry((u.edge, u.sense == Sense::Forward))
                            .or_default()
                            .push(i);
                    }
                }
                m
            })
            .unwrap_or_default();
        let mut pieces = Vec::new();
        let mut runs = Vec::new();
        let mut from = Vec::new();
        for (li, lp) in rec.loops.iter().enumerate() {
            if lp.uses.iter().all(|u| matches!(u.geom, UseGeom::Point(_))) {
                // A loop of points (or none) bounds no area.
                continue;
            }
            let first = pieces.len();
            let same = old.zip(lp.uses.first()).and_then(|((r, t), u)| {
                let i = by_first
                    .get(&(u.edge, u.sense == Sense::Forward))?
                    .iter()
                    .copied()
                    .find(|&i| {
                        t.runs
                            .get(i)
                            .and_then(|run| r.loops.get(run.lp as usize))
                            .is_some_and(|l| l == lp)
                    })?;
                Some((i, t.runs.get(i)?, t))
            });
            if let Some((i, run, t)) = same {
                let li = li as u32;
                pieces.extend(
                    t.pieces
                        .get(run.first as usize..run.end as usize)?
                        .iter()
                        .map(|p| Piece {
                            src: match p.src {
                                Src::Part {
                                    us, lo, hi, rev, ..
                                } => Src::Part {
                                    lp: li,
                                    us,
                                    lo,
                                    hi,
                                    rev,
                                },
                                Src::Gap => Src::Gap,
                            },
                            ..*p
                        }),
                );
                runs.push(Run {
                    first: first as u32,
                    end: pieces.len() as u32,
                    root: 0,
                    lp: li,
                    area: run.area,
                });
                from.push(Some(i));
                continue;
            }
            let ends: Vec<(P, P)> = lp.uses.iter().map(use_ends).collect::<Option<_>>()?;
            for (ui, u) in lp.uses.iter().enumerate() {
                if let UseGeom::Pcurve { curve, interval } = &u.geom {
                    let rev = u.sense == Sense::Reversed;
                    let n = if matches!(curve, Curve2::Line(_)) {
                        1
                    } else {
                        PARTS
                    };
                    let mut cut: Vec<_> = parts(interval.min(), interval.max(), n).collect();
                    if rev {
                        cut.reverse();
                    }
                    for (a, b) in cut {
                        pieces.push(Piece {
                            start: at(curve, if rev { b } else { a })?,
                            bx: uv_box(curve.point_over(Ivl::new(a, b)))?,
                            src: Src::Part {
                                lp: li as u32,
                                us: ui as u32,
                                lo: a,
                                hi: b,
                                rev,
                            },
                        });
                    }
                }
                let end = ends.get(ui)?.1;
                let next = ends.get((ui + 1) % ends.len())?.0;
                if !same_point(end, next) {
                    pieces.push(Piece {
                        start: end,
                        bx: uv_box(hull(end, next))?,
                        src: Src::Gap,
                    });
                }
            }
            if pieces.len() > first {
                runs.push(Run {
                    first: first as u32,
                    end: pieces.len() as u32,
                    root: 0,
                    lp: li as u32,
                    area: [None; AREA_REFINE.len()],
                });
                from.push(None);
            }
        }
        let mut nodes = Vec::new();
        for run in &mut runs {
            run.root = node(&pieces, run.first, run.end, &mut nodes)?;
        }
        let hull = pieces.iter().map(|p| p.bx).reduce(Extent2d::union);
        // An area part is about the hull's centre: kept only where that is the same.
        let c = |h: Option<Extent2d>| h.map(|h| h.mid().x.to_bits());
        if old.is_none_or(|(_, t)| c(t.hull) != c(hull)) {
            for run in &mut runs {
                run.area = [None; AREA_REFINE.len()];
            }
        }
        pieces.shrink_to_fit();
        nodes.shrink_to_fit();
        runs.shrink_to_fit();
        Some((
            Self {
                pieces,
                nodes,
                runs,
                hull,
                w_in: 0,
            },
            from,
        ))
    }

    /// The sign of the chains' signed area, refined until it shows.
    fn area_sign(&mut self, rec: &FaceRecord) -> Option<Ordering> {
        for (at, k) in AREA_REFINE.into_iter().enumerate() {
            let a = self.area(rec, at, k)?;
            if a.lo > 0.0 {
                return Some(Ordering::Greater);
            }
            if a.hi < 0.0 {
                return Some(Ordering::Less);
            }
        }
        None
    }

    /// An enclosure of `sum ∮ (u - c) dv` over every chain, each curved part cut `k` more
    /// times (refinement `at`): the sum of the runs' parts in loop order, each kept in its
    /// run.
    fn area(&mut self, rec: &FaceRecord, at: usize, k: usize) -> Option<Ivl> {
        let c = Ivl::from(self.hull?.mid().x);
        let mut total = Ivl::from(0.0);
        for r in 0..self.runs.len() {
            let run = *self.runs.get(r)?;
            let part = match run.area.get(at).copied().flatten() {
                Some(a) => a,
                None => {
                    let a = self.run_area(rec, &run, c, k)?;
                    *self.runs.get_mut(r)?.area.get_mut(at)? = Some(a);
                    a
                }
            };
            total = total + part;
        }
        total.is_finite().then_some(total)
    }

    /// Run `run`'s part of [`Tree::area`]: a straight piece's integral exactly, a circular
    /// part's in closed form, any other curved part's by the mean value form
    /// `(b - a) (u(I) - c) v'(I)`.
    fn run_area(&self, rec: &FaceRecord, run: &Run, c: Ivl, k: usize) -> Option<Ivl> {
        let mut total = Ivl::from(0.0);
        for i in run.first..run.end {
            let piece = self.pieces.get(i as usize)?;
            let end = self.end_of(run, i)?;
            let straight = |a: P, b: P| ((a.x + b.x) * Ivl::from(0.5) - c) * (b.y - a.y);
            total = total
                + match piece.src {
                    Src::Gap => straight(piece.start, end),
                    Src::Part {
                        lp,
                        us,
                        lo,
                        hi,
                        rev,
                    } => {
                        let curve = pcurve(rec, lp, us)?;
                        if matches!(curve, Curve2::Line(_)) {
                            straight(piece.start, end)
                        } else if let Curve2::Circle(circle) = curve {
                            let sum = arc_area(circle, curve, lo, hi, c)?;
                            if rev { neg(sum) } else { sum }
                        } else {
                            let mut sum = Ivl::from(0.0);
                            for (a, b) in parts(lo, hi, k) {
                                let j = curve.jet_over::<Ivl, 1>(Ivl::new(a, b), Approach::Above);
                                let [d] = j.d;
                                let w = Ivl::from(b) - Ivl::from(a);
                                sum = sum + w * (j.point.x - c) * d.y;
                            }
                            if rev { neg(sum) } else { sum }
                        }
                    }
                };
        }
        total.is_finite().then_some(total)
    }

    /// The end of piece `i` of `run`: the next piece's start, round the loop.
    fn end_of(&self, run: &Run, i: u32) -> Option<P> {
        let next = if i + 1 >= run.end { run.first } else { i + 1 };
        Some(self.pieces.get(next as usize)?.start)
    }

    /// Where the lifted point `q` lies, trying the four rays in turn.
    fn side_at(&self, rec: &FaceRecord, q: (Ivl, Ivl), budget: &mut u32) -> Option<UvSide> {
        'rays: for r in 0..4 {
            let q = rot(r, q);
            let mut w = 0;
            for run in &self.runs {
                match self.walk(rec, run, run.root, r, q, budget) {
                    Step::Count(c) => w += c,
                    Step::Near => return Some(UvSide::Near),
                    Step::Undecided => continue 'rays,
                }
            }
            return Some(if w == self.w_in {
                UvSide::Inside
            } else {
                UvSide::Outside
            });
        }
        None
    }

    /// The crossings of ray `r` from `q` by the pieces under `node`.
    fn walk(
        &self,
        rec: &FaceRecord,
        run: &Run,
        node: u32,
        r: u8,
        q: (Ivl, Ivl),
        budget: &mut u32,
    ) -> Step {
        let Some(n) = self.nodes.get(node as usize) else {
            return Step::Undecided;
        };
        match classify(rot(r, box_ivl(n.bx)), q) {
            Class::Zero => Step::Count(0),
            Class::Ahead => {
                let start = self.pieces.get(n.first as usize).map(|p| p.start);
                let end = self.end_of(run, n.end - 1);
                match (start, end) {
                    (Some(s), Some(e)) => crossing(rot_p(r, s), rot_p(r, e), q),
                    _ => Step::Undecided,
                }
            }
            Class::Meets if n.right == 0 => {
                let mut sum = Step::Count(0);
                for i in n.first..n.end {
                    sum = sum.then(|| self.piece_step(rec, run, i, r, q, budget));
                }
                sum
            }
            Class::Meets => self
                .walk(rec, run, node + 1, r, q, budget)
                .then(|| self.walk(rec, run, n.right, r, q, budget)),
        }
    }

    /// The crossings of ray `r` from `q` by piece `i`.
    fn piece_step(
        &self,
        rec: &FaceRecord,
        run: &Run,
        i: u32,
        r: u8,
        q: (Ivl, Ivl),
        budget: &mut u32,
    ) -> Step {
        let (Some(piece), Some(end)) = (self.pieces.get(i as usize), self.end_of(run, i)) else {
            return Step::Undecided;
        };
        match classify(rot(r, box_ivl(piece.bx)), q) {
            Class::Zero => Step::Count(0),
            Class::Ahead => crossing(rot_p(r, piece.start), rot_p(r, end), q),
            Class::Meets => {
                let seg = match piece.src {
                    Src::Gap => Seg::Gap(piece.start, end),
                    Src::Part { lp, us, rev, .. } => match pcurve(rec, lp, us) {
                        Some(curve) => Seg::Part(curve, rev),
                        None => return Step::Undecided,
                    },
                };
                let (a, b) = match piece.src {
                    Src::Gap => (0.0, 1.0),
                    Src::Part { lo, hi, .. } => (lo, hi),
                };
                halve(&seg, a, b, r, q, DEPTH, budget)
            }
        }
    }
}

/// A piece being halved: a pcurve (walked down when `true`) or a straight gap.
enum Seg<'a> {
    Part(&'a Curve2, bool),
    Gap(P, P),
}

impl Seg<'_> {
    fn enclose(&self, t: Ivl) -> P {
        match self {
            Self::Part(c, _) => c.point_over(t),
            Self::Gap(a, b) => Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)),
        }
    }

    /// The start and end parameters of `a..b` in the walking direction.
    const fn ends(&self, a: f64, b: f64) -> (f64, f64) {
        match self {
            Self::Part(_, true) => (b, a),
            _ => (a, b),
        }
    }
}

/// The crossings of ray `r` from `q` by `seg` over `a..b`, halving where its box meets `q`.
fn halve(
    seg: &Seg<'_>,
    a: f64,
    b: f64,
    r: u8,
    q: (Ivl, Ivl),
    depth: u32,
    budget: &mut u32,
) -> Step {
    if *budget == 0 {
        return Step::Undecided;
    }
    *budget -= 1;
    let Some(bx) = uv_box(seg.enclose(Ivl::new(a, b))) else {
        return Step::Undecided;
    };
    match classify(rot(r, box_ivl(bx)), q) {
        Class::Zero => Step::Count(0),
        Class::Ahead => {
            let (s, e) = seg.ends(a, b);
            crossing(
                rot_p(r, seg.enclose(Ivl::from(s))),
                rot_p(r, seg.enclose(Ivl::from(e))),
                q,
            )
        }
        Class::Meets => {
            let m = 0.5 * (a + b);
            if depth == 0 || !(a < m && m < b) {
                return Step::Near;
            }
            halve(seg, a, m, r, q, depth - 1, budget)
                .then(|| halve(seg, m, b, r, q, depth - 1, budget))
        }
    }
}

/// What a piece or subtree adds to a winding number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Count(i32),
    /// It meets the point at the finest halving.
    Near,
    /// An endpoint is level with the point, or the budget ran out: try another ray.
    Undecided,
}

impl Step {
    /// The sum with `next`, which is not evaluated once this one cannot be summed.
    fn then(self, next: impl FnOnce() -> Self) -> Self {
        match self {
            Self::Count(a) => match next() {
                Self::Count(b) => Self::Count(a + b),
                other => other,
            },
            other => other,
        }
    }
}

enum Class {
    /// Wholly above, below or behind the ray's start: no crossing.
    Zero,
    /// Wholly ahead: the change in "at or above" between its ends.
    Ahead,
    /// Its box meets the point.
    Meets,
}

/// Where box `b` lies against the ray along `+x` from `q`.
fn classify(b: (Ivl, Ivl), q: (Ivl, Ivl)) -> Class {
    if b.1.lo > q.1.hi || b.1.hi < q.1.lo || b.0.hi < q.0.lo {
        Class::Zero
    } else if b.0.lo > q.0.hi {
        Class::Ahead
    } else {
        Class::Meets
    }
}

/// The signed crossings of the ray from `q` by a path wholly ahead of it from `s` to `e`.
fn crossing(s: P, e: P, q: (Ivl, Ivl)) -> Step {
    let above = |p: P| {
        if p.y.lo >= q.1.hi {
            Some(1)
        } else if p.y.hi < q.1.lo {
            Some(0)
        } else {
            None
        }
    };
    match (above(s), above(e)) {
        (Some(s), Some(e)) => Step::Count(e - s),
        _ => Step::Undecided,
    }
}

/// `∫ (u - c) dv` along the arc of `circle` from `t = a` to `b`, closed-form: about the
/// centre `o`, with `u - o.x = R cos φ` and `φ = φ0 ± t`,
/// `(o.x - c) (v_b - v_a) ± R² (b - a) / 2 + [(u - o.x)(v - o.y)]_a^b / 2`. Only the ends
/// are evaluated, so a thin face's arcs (radius 1e4, loops 1e-3 apart) keep their
/// cancelling areas to rounding, where the mean value form's overestimate grows as
/// `R² (b - a)` over a part and no refinement here could show the sign. The sign `±` is
/// that of `(p - o) × p'`, which is `±R²`; `None` when it does not show.
fn arc_area(circle: &Circle2, curve: &Curve2, a: f64, b: f64, c: Ivl) -> Option<Ivl> {
    let o = circle.frame().origin();
    let (ox, oy) = (Ivl::from(o.x), Ivl::from(o.y));
    let r = Ivl::from(circle.radius().mm());
    let at = |t: f64| curve.jet_over::<Ivl, 1>(Ivl::from(t), Approach::Above);
    let (ja, jb) = (at(a), at(b));
    let [d] = ja.d;
    let turn = (ja.point.x - ox) * d.y - (ja.point.y - oy) * d.x;
    let half = Ivl::from(0.5);
    let sweep = r * r * (Ivl::from(b) - Ivl::from(a)) * half;
    let sweep = if turn.lo > 0.0 {
        sweep
    } else if turn.hi < 0.0 {
        neg(sweep)
    } else {
        return None;
    };
    let corner = |p: P| (p.x - ox) * (p.y - oy);
    let total =
        (ox - c) * (jb.point.y - ja.point.y) + sweep + (corner(jb.point) - corner(ja.point)) * half;
    total.is_finite().then_some(total)
}

const fn neg(x: Ivl) -> Ivl {
    Ivl::new(-x.hi, -x.lo)
}

/// Turn `r` of four (each a rotation, so winding numbers keep): the ray along `+x` in the
/// turned plane is `+u`, `-u`, `+v`, `-v` in the face's.
const fn rot(r: u8, (x, y): (Ivl, Ivl)) -> (Ivl, Ivl) {
    match r {
        0 => (x, y),
        1 => (neg(x), neg(y)),
        2 => (y, neg(x)),
        _ => (neg(y), x),
    }
}

const fn rot_p(r: u8, p: P) -> P {
    let (x, y) = rot(r, (p.x, p.y));
    Point2::new(x, y)
}

const fn box_ivl(b: Extent2d) -> (Ivl, Ivl) {
    let (lo, hi) = (b.min(), b.max());
    (Ivl::new(lo.x, hi.x), Ivl::new(lo.y, hi.y))
}

/// The tree over pieces `first..end`; `None` past `u32` nodes.
fn node(pieces: &[Piece], first: u32, end: u32, nodes: &mut Vec<Node>) -> Option<u32> {
    let idx = u32::try_from(nodes.len()).ok()?;
    let bx = pieces
        .get(first as usize..end as usize)?
        .iter()
        .map(|p| p.bx)
        .reduce(Extent2d::union)?;
    nodes.push(Node {
        bx,
        first,
        end,
        right: 0,
    });
    if (end - first) as usize > LEAF {
        let mid = first + (end - first) / 2;
        node(pieces, first, mid, nodes)?;
        let right = node(pieces, mid, end, nodes)?;
        nodes.get_mut(idx as usize)?.right = right;
    }
    Some(idx)
}

/// The pcurve of use `us` of loop `lp`.
fn pcurve(rec: &FaceRecord, lp: u32, us: u32) -> Option<&Curve2> {
    match &rec.loops.get(lp as usize)?.uses.get(us as usize)?.geom {
        UseGeom::Pcurve { curve, .. } => Some(curve),
        UseGeom::Point(_) => None,
    }
}

/// The enclosures of where a use starts and ends, in its sense.
fn use_ends(u: &crate::record::EdgeUse) -> Option<(P, P)> {
    match &u.geom {
        UseGeom::Pcurve { curve, interval } => {
            let (lo, hi) = (at(curve, interval.min())?, at(curve, interval.max())?);
            Some(if u.sense == Sense::Reversed {
                (hi, lo)
            } else {
                (lo, hi)
            })
        }
        UseGeom::Point(p) => {
            let p = Point2::new(Ivl::from(p.x), Ivl::from(p.y));
            Some((p, p))
        }
    }
}

/// The enclosure of `curve` at `t`, `None` unless finite.
fn at(curve: &Curve2, t: f64) -> Option<P> {
    let p = curve.point_over(Ivl::from(t));
    (p.x.is_finite() && p.y.is_finite()).then_some(p)
}

/// Two enclosures of one exact point.
fn same_point(a: P, b: P) -> bool {
    a.x.lo == a.x.hi && a.y.lo == a.y.hi && a == b
}

fn hull(a: P, b: P) -> P {
    Point2::new(a.x.hull(b.x), a.y.hull(b.y))
}

/// The box of an enclosure, `None` unless finite.
fn uv_box(p: P) -> Option<Extent2d> {
    if !(p.x.is_finite() && p.y.is_finite()) {
        return None;
    }
    Extent2d::from_bounds(p.x.lo, p.y.lo, p.x.hi, p.y.hi)
}

/// One period of each closed direction, the range of each bounded one; `None` when a
/// direction is open.
fn window(s: &scx_surface::Surface) -> Option<Extent2d> {
    let d = s.domain();
    let w = |d: ParamDomain| match d {
        ParamDomain::Periodic { period, base, .. } => Some((base, base + period)),
        ParamDomain::Bounded(e) => Some((e.min(), e.max())),
        _ => None,
    };
    let (u, v) = (w(d.u)?, w(d.v)?);
    Extent2d::from_bounds(u.0, v.0, u.1, v.1)
}

/// Whether `x` is within a direction's ends: outside one, on one, or within.
fn axis_side(d: ParamDomain, x: f64) -> UvSide {
    let (lo, hi) = match d {
        ParamDomain::Bounded(e) => (e.min(), e.max()),
        ParamDomain::AtLeast { from, .. } => (from, f64::INFINITY),
        ParamDomain::AtMost { to, .. } => (f64::NEG_INFINITY, to),
        _ => return UvSide::Inside,
    };
    if x < lo || x > hi {
        UvSide::Outside
    } else if x == lo || x == hi {
        UvSide::Near
    } else {
        UvSide::Inside
    }
}

/// The lifts of `x` by whole periods that may fall in `lo..=hi`, each enclosed: `x` alone
/// on an open direction; `None` past [`MAX_LIFTS`].
fn lifts(d: ParamDomain, x: f64, lo: f64, hi: f64) -> Option<Vec<Ivl>> {
    let ParamDomain::Periodic { period, .. } = d else {
        return Some(vec![Ivl::from(x)]);
    };
    let k0 = ((lo - x) / period).floor();
    let k1 = ((hi - x) / period).ceil();
    if !(k0.is_finite() && k1.is_finite()) || k1 - k0 > MAX_LIFTS as f64 {
        return None;
    }
    Some(
        (k0 as i64..=k1 as i64)
            .map(|k| Ivl::from(x) + Ivl::from(k as f64) * Ivl::from(period))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use core::f64::consts::TAU;

    use scx_curve::{Circle2, Line2};
    use scx_num::extent1d::Extent1d;
    use scx_num::fmath;
    use scx_num::frame::{Frame, Frame2};
    use scx_num::units::Length;
    use scx_num::vector2d::Vec2;
    use scx_surface::{Cylinder, Plane, Surface};

    use super::*;
    use crate::id::{EdgeId, ShellId};
    use crate::record::{EdgeUse, LoopRecord};

    fn plane() -> Surface {
        Surface::Plane(Plane::new(Frame::WORLD))
    }

    /// A use of the segment from `a` to `b`, forward.
    fn seg(a: (f64, f64), b: (f64, f64)) -> EdgeUse {
        let line = Line2::new(Point2::new(a.0, a.1), Vec2::new(b.0 - a.0, b.1 - a.1)).unwrap();
        EdgeUse::new(
            EdgeId::from_raw(0),
            Sense::Forward,
            UseGeom::Pcurve {
                curve: Curve2::Line(line),
                interval: Extent1d::new(0.0, 1.0).unwrap(),
            },
        )
    }

    /// The polygon through `pts`, in order.
    fn polygon(pts: &[(f64, f64)]) -> LoopRecord {
        let next = pts.iter().cycle().skip(1);
        LoopRecord::new(pts.iter().zip(next).map(|(&a, &b)| seg(a, b)).collect())
    }

    /// The circle of radius `r` about the origin, run in `sense`.
    fn circle(r: f64, sense: Sense) -> LoopRecord {
        let c = Circle2::new(Frame2::WORLD, Length::from_mm(r), Sense::Forward).unwrap();
        LoopRecord::new(vec![EdgeUse::new(
            EdgeId::from_raw(0),
            sense,
            UseGeom::Pcurve {
                curve: Curve2::Circle(c),
                interval: Extent1d::new(0.0, TAU).unwrap(),
            },
        )])
    }

    fn face(surface: Surface, loops: Vec<LoopRecord>) -> FaceRecord {
        let s = ShellId::from_raw(0);
        FaceRecord::new(surface, [s, s], loops)
    }

    fn side(f: &FaceRecord, u: f64, v: f64) -> Option<UvSide> {
        Boundary::build(f).side(f, Point2::new(u, v))
    }

    const SQUARE: [(f64, f64); 4] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

    #[test]
    fn a_boundary_tree_stays_small() {
        // Research 63 §6.9 measured about 0.6 to 0.9 KB a face; a bound catches a regression
        // in the pieces per use or a capacity left over.
        let square = Boundary::build(&face(plane(), vec![polygon(&SQUARE)]));
        let (pieces, heap) = square.footprint();
        // Each side, and a gap after it: a line's ends are enclosed by intervals, not exact
        // points, so the chain closes through gaps (an open item of 63 §6.9).
        assert_eq!(pieces, 8);
        assert!(heap <= 1024, "{heap} bytes");
        let disc = Boundary::build(&face(plane(), vec![circle(1.0, Sense::Forward)]));
        let (pieces, heap) = disc.footprint();
        assert!(pieces <= 5, "{pieces} pieces");
        assert!(heap <= 1024, "{heap} bytes");
    }

    /// A clockwise square hole of side 0.1 at `(x, y)`.
    fn hole(x: f64, y: f64) -> LoopRecord {
        polygon(&[(x, y), (x, y + 0.1), (x + 0.1, y + 0.1), (x + 0.1, y)])
    }

    #[test]
    fn a_carried_boundary_is_the_one_built() {
        let outer = polygon(&[(-3.0, -3.0), (3.0, -3.0), (3.0, 3.0), (-3.0, 3.0)]);
        let disc = circle(0.5, Sense::Reversed);
        let old = face(
            plane(),
            vec![outer.clone(), hole(1.0, 1.0), disc.clone(), hole(-2.0, 1.0)],
        );
        let was = Boundary::build(&old);
        // Loops kept in another order, one dropped, one new: the same centre, so the kept
        // runs' areas carry too.
        let new = face(
            plane(),
            vec![outer, hole(-2.0, 1.0), hole(2.0, -2.0), disc.clone()],
        );
        let (got, from) = Boundary::carried(&new, Some((&old, &was)));
        assert_eq!(from, [Some(0), Some(3), None, Some(2)]);
        assert_eq!(format!("{got:?}"), format!("{:?}", Boundary::build(&new)));
        // A new outer loop moves the centre: the kept hole's pieces carry, its area is
        // found again, and the bits are still the built ones.
        let wider = polygon(&[(-3.0, -3.0), (4.0, -3.0), (4.0, 3.0), (-3.0, 3.0)]);
        let moved = face(plane(), vec![wider, disc]);
        let (got, from) = Boundary::carried(&moved, Some((&new, &got)));
        assert_eq!(from, [None, Some(3)]);
        assert_eq!(format!("{got:?}"), format!("{:?}", Boundary::build(&moved)));
    }

    #[test]
    fn carried_cells_are_the_ones_filled() {
        use crate::cells::{FaceCell, FaceCells, SurfaceBox};
        let outer = polygon(&[(-3.0, -3.0), (3.0, -3.0), (3.0, 3.0), (-3.0, 3.0)]);
        let tilted = Surface::Plane(Plane::new(
            Frame::new(
                scx_num::vector3d::Point3::new(1.0, 2.0, 3.0),
                scx_num::vector3d::Vec3::new(0.3, 0.4, 1.0),
                scx_num::vector3d::Vec3::new(1.0, 0.0, -0.3),
            )
            .unwrap(),
        ));
        let old = face(tilted.clone(), vec![outer.clone(), hole(1.0, 1.0)]);
        let _ = SurfaceBox::of(&old);
        let new = face(tilted, vec![outer, hole(1.0, 1.0), hole(-2.0, -2.0)]);
        let carried = FaceCells::carried(&old, &new);
        let _ = SurfaceBox::of(&new);
        assert!(carried.is_filled());
        assert_eq!(format!("{carried:?}"), format!("{:?}", new.cells));
        // Another surface: the boundary carries, the surface box waits for a read.
        let flat = face(plane(), new.loops.clone());
        let carried = FaceCells::carried(&old, &flat);
        let _ = Boundary::of(&flat);
        assert_eq!(format!("{carried:?}"), format!("{:?}", flat.cells));
        // Nothing filled, nothing carried.
        let unread = face(plane(), flat.loops.clone());
        assert!(!FaceCells::carried(&unread, &new).is_filled());
    }

    #[test]
    fn a_square_holds_its_inside() {
        let f = face(plane(), vec![polygon(&SQUARE)]);
        let b = Boundary::build(&f);
        assert!(b.is_bounded_region());
        let uv = b.uv_box().unwrap();
        assert!(uv.min().x <= 0.0 && uv.max().y >= 1.0 && uv.max().x < 1.0 + 1e-12);
        assert_eq!(side(&f, 0.5, 0.5), Some(UvSide::Inside));
        assert_eq!(side(&f, 0.999, 0.001), Some(UvSide::Inside));
        assert_eq!(side(&f, 2.0, 0.5), Some(UvSide::Outside));
        assert_eq!(side(&f, 0.5, -1e-9), Some(UvSide::Outside));
        // On an edge or a corner.
        assert_eq!(side(&f, 1.0, 0.5), Some(UvSide::Near));
        assert_eq!(side(&f, 0.0, 0.0), Some(UvSide::Near));
        // Level with the bottom edge: the `+u` ray runs along it, and another decides.
        assert_eq!(side(&f, -1.0, 0.0), Some(UvSide::Outside));
        assert_eq!(side(&f, 0.5, 1.0), Some(UvSide::Near));
        // Clockwise, the same loop is a hole in the whole plane.
        let mut cw = SQUARE;
        cw.reverse();
        let hole = face(plane(), vec![polygon(&cw)]);
        let hb = Boundary::build(&hole);
        assert!(!hb.is_bounded_region());
        assert_eq!(hb.uv_box(), None);
        assert_eq!(side(&hole, 0.5, 0.5), Some(UvSide::Outside));
        assert_eq!(side(&hole, 5.0, -3.0), Some(UvSide::Inside));
        assert_eq!(side(&hole, 1.0, 0.25), Some(UvSide::Near));
    }

    #[test]
    fn an_annulus_holds_its_ring() {
        let f = face(
            plane(),
            vec![circle(2.0, Sense::Forward), circle(1.0, Sense::Reversed)],
        );
        let b = Boundary::build(&f);
        assert!(b.is_bounded_region());
        let uv = b.uv_box().unwrap();
        assert!(uv.min().x <= -2.0 && uv.max().x >= 2.0 && uv.max().x < 2.5);
        for (u, v, want) in [
            (0.0, 0.0, UvSide::Outside),
            (1.5, 0.0, UvSide::Inside),
            (0.0, -1.5, UvSide::Inside),
            (1.0, 1.0, UvSide::Inside),
            (3.0, 0.0, UvSide::Outside),
            (1.5, 1.5, UvSide::Outside),
            (1.0, 0.0, UvSide::Near),
            (0.0, 2.0, UvSide::Near),
        ] {
            assert_eq!(side(&f, u, v), Some(want), "at ({u}, {v})");
        }
        // Each point agrees with the exact answer by its distance from the centre.
        for i in 0..200 {
            let t = f64::from(i) * 0.137;
            let (u, v) = (
                2.6 * fmath::cos(t) * fmath::sin(t * 0.31),
                2.6 * fmath::sin(t) * fmath::cos(t * 0.17),
            );
            let r = fmath::hypot(u, v);
            let got = side(&f, u, v);
            let want = if (r - 1.0).abs() < 1e-9 || (r - 2.0).abs() < 1e-9 {
                None
            } else if r > 1.0 && r < 2.0 {
                Some(UvSide::Inside)
            } else {
                Some(UvSide::Outside)
            };
            if let Some(want) = want {
                assert_eq!(got, Some(want), "at ({u}, {v}), radius {r}");
            }
        }
    }

    #[test]
    fn a_band_round_a_cylinder_holds_every_lift() {
        let cyl = Cylinder::new(Frame::WORLD, Length::from_mm(1.0), Sense::Forward).unwrap();
        let s = Surface::Cylinder(cyl);
        let ParamDomain::Periodic { period, .. } = s.domain().u else {
            panic!("a cylinder is closed round");
        };
        let f = face(
            s,
            vec![polygon(&[
                (0.0, 0.0),
                (period, 0.0),
                (period, 2.0),
                (0.0, 2.0),
            ])],
        );
        assert_eq!(side(&f, 1.0, 1.0), Some(UvSide::Inside));
        assert_eq!(side(&f, 1.0 + period, 1.0), Some(UvSide::Inside));
        assert_eq!(side(&f, 1.0 - 3.0 * period, 1.0), Some(UvSide::Inside));
        assert_eq!(side(&f, 1.0, 3.0), Some(UvSide::Outside));
        assert_eq!(side(&f, 1.0, 2.0), Some(UvSide::Near));
        // The seam is inside the band: one lift is on the boundary, another within.
        assert_eq!(side(&f, 0.0, 1.0), Some(UvSide::Near));
    }

    #[test]
    fn a_face_of_points_is_its_whole_domain() {
        let lone = EdgeUse::new(
            EdgeId::from_raw(0),
            Sense::Forward,
            UseGeom::Point(Point2::new(0.0, 0.0)),
        );
        let f = face(plane(), vec![LoopRecord::new(vec![lone])]);
        let b = Boundary::build(&f);
        assert_eq!((b.uv_box(), b.is_bounded_region()), (None, false));
        assert_eq!(side(&f, 3.0, 4.0), Some(UvSide::Inside));
    }

    #[test]
    fn a_loop_of_no_area_answers_nothing() {
        let f = face(plane(), vec![polygon(&[(0.0, 0.0), (1.0, 0.0)])]);
        let b = Boundary::build(&f);
        assert!(!b.is_bounded_region());
        assert_eq!(side(&f, 0.5, 0.5), None);
        // On a cylinder the uv box is still the hull with a window.
        let cyl = Cylinder::new(Frame::WORLD, Length::from_mm(1.0), Sense::Forward).unwrap();
        let g = face(
            Surface::Cylinder(cyl),
            vec![polygon(&[(0.0, 0.0), (1.0, 0.0)])],
        );
        assert_eq!(Boundary::build(&g).uv_box(), None);
    }

    #[test]
    fn a_lone_gap_closes_a_loop_that_does_not() {
        // Three sides of the square: the gap from (0, 1) back to (0, 0) closes it.
        let lp = polygon(&SQUARE);
        let open = LoopRecord::new(lp.uses.into_iter().take(3).collect());
        let f = face(plane(), vec![open]);
        assert_eq!(side(&f, 0.5, 0.5), Some(UvSide::Inside));
        assert_eq!(side(&f, -0.5, 0.5), Some(UvSide::Outside));
        assert_eq!(side(&f, 0.0, 0.5), Some(UvSide::Near));
    }
}
