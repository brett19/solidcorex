// Written for solidcorex. Apache-2.0.
//! Pieces, pairs and the proof of apartness (research 73 §4.3, row SM4).
//!
//! A surface over its extent is cut into its **pieces**: the cells of its Bézier form (the
//! knot-span rectangles of a NURBS surface, the arc-break rectangles of an analytic kind's
//! rational image, [`Surface::to_nurbs`]), each with a certified box
//! ([`Surface::box_over`]: outward interval arithmetic, never samples). The pieces of the two
//! surfaces are paired by [`BoxTree::pairs`], and a pair is dropped only by a proof, each with
//! its rounding bounded from the arithmetic it uses (no pad and no `acos` slack):
//!
//! 1. **Boxes**: the certified boxes are more than the coincidence apart on an axis. The box
//!    tree's own test is `a.min > b.max + tol`, so it is given `tol` grown by the rounding of
//!    that sum ([`box_tolerance`]); pairs it drops are then apart by more than `tol` in the
//!    reals. A child pair made by halving is tested by subtracting in interval arithmetic.
//! 2. **Slabs**: along the unit normal of either piece at its middle, the ranges of the two
//!    pieces ([`Surface::range_along`], certified) are apart by more than `tol` times the
//!    direction's length (an interval upper bound). A NURBS piece's range is a mean-value
//!    form, second order in its size, so a flat piece seen along its normal is a thin slab.
//!
//! A pair neither proof drops **survives**. [`Candidates::halve`] splits one of its pieces in
//! two on the side that shrinks its box most (a certified box for each half) and returns the
//! children that survive. Halving ends at the resolution: a pair whose pieces are all no
//! larger than the resolution, or cannot be split in `f64`, is [`SsiError::Unresolved`] where
//! it lies, which is the one place where the resolution enters as a size.
//!
//! 3. **Field sign** (73 §4.3, item 3, row SM6a): where a side has an implicit form `f`, the
//!    polynomial `g = f(x)` over the other piece's rational Bernstein form, past the bound
//!    `|grad f|` times the coincidence (and the forms' rounding), has one strict sign: no
//!    point of that piece is within the coincidence of the side's surface
//!    ([`field::apart`](super::field::apart)).

use scx_algo::box_tree::BoxTree;
use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::interval::Ivl;
use scx_num::units::{Accuracy, Length, Resolution, coincident};
use scx_num::vector3d::Vec3;
use scx_nurbs::Approach;
use scx_nurbs_surface::ParamAxis;
use scx_surface::{Hulls, Surface};

use super::field;
use super::implicit::Implicit;
use crate::error::SsiError;

/// One piece of a surface: a uv rectangle on one Bézier cell (or a part of one) and the
/// certified box of the surface over it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    /// The piece's uv rectangle, inside the surface's extent.
    pub uv: Extent2d,
    /// A box holding the surface over `uv` ([`Surface::box_over`]).
    pub bbox: Extent3d,
    /// The slab across the piece, with its own range along it, computed once.
    slabs: [Option<Slab>; 2],
    /// The index of the piece's Bézier hull in its surface's [`Hulls`], when it is a whole
    /// patch of a NURBS surface.
    cell: Option<usize>,
}

/// A direction across a piece with the piece's own certified range along it and the
/// coincidence scaled by an upper bound of the direction's length.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Slab {
    axis: Vec3,
    own: Ivl,
    tol: f64,
}

/// A piece of the first surface with a piece of the second whose boxes and slabs do not
/// prove them apart: the surfaces may meet there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PiecePair {
    /// The piece of the first surface.
    pub a: Piece,
    /// The piece of the second surface.
    pub b: Piece,
    /// The piece [`Candidates::halve`] splits first (0 for `a`, 1 for `b`): the one whose
    /// range along the best slab axis is wider, which is what keeps the proof from closing.
    next: usize,
    /// The axis that hint came from: halving shrinks the piece's range along it fastest.
    axis: Option<Vec3>,
}

impl PiecePair {
    /// The pair of two pieces, to be halved on the one with the larger box first.
    #[must_use]
    pub fn new(a: Piece, b: Piece) -> Self {
        Self {
            a,
            b,
            next: usize::from(a.bbox.max_length() < b.bbox.max_length()),
            axis: None,
        }
    }

    /// The region the pair holds: the overlap of its boxes, or their union when rounding
    /// leaves no overlap (the pair survived by the coincidence alone).
    #[must_use]
    pub fn region(&self) -> Extent3d {
        self.a
            .bbox
            .intersect(self.b.bbox)
            .unwrap_or_else(|| self.a.bbox.union(self.b.bbox))
    }

    /// The pair as the refusal that names it: [`SsiError::Unresolved`] at the middle of its
    /// [`region`](Self::region), of that region's longest side.
    #[must_use]
    pub fn unresolved(&self) -> SsiError {
        let r = self.region();
        SsiError::Unresolved {
            at: r.mid(),
            size: Length::from_mm(r.max_length()),
        }
    }
}

/// The surviving pairs of two surfaces over their extents, with what is needed to halve
/// them. Built by [`Candidates::new`]; the pairs are in the ascending order of their pieces'
/// indices (a surface's cells in u-major order), so the answer depends on nothing else.
#[derive(Debug)]
pub struct Candidates<'s> {
    surfaces: [&'s Surface; 2],
    /// The Bézier hulls of each surface, when it is a NURBS surface with certified weights.
    hulls: [Option<Hulls>; 2],
    /// The coincidence, `coincident(res, acc_a, acc_b)`, in mm.
    tol: f64,
    /// The resolution, in mm: pieces no larger are not halved.
    floor: f64,
    /// The implicit form of each surface, when it is an analytic kind.
    forms: [Option<Implicit>; 2],
    /// The pairs the proofs of the cells left, in order.
    pairs: Vec<PiecePair>,
}

impl<'s> Candidates<'s> {
    /// The cell pairs of `a` over `ae` and `b` over `be` that boxes and slabs leave.
    ///
    /// Charges one unit for the construction and one per pair the box tree reports, polling
    /// `stop` after the first charge and per pair (the budget first, as the global solver).
    ///
    /// # Errors
    ///
    /// [`SsiError::Budget`] and [`SsiError::Stopped`]; and [`SsiError::Unresolved`] over the
    /// whole of both extents (as [`whole_pair`]) when a cell's box cannot be enclosed (a
    /// value not finite).
    pub fn new(
        a: &'s Surface,
        ae: &Extent2d,
        b: &'s Surface,
        be: &Extent2d,
        acc: [Accuracy; 2],
        res: Resolution,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Self, SsiError> {
        budget.charge(1)?;
        stop.check()?;
        let tol = coincident(res, acc[0], acc[1]).mm();
        let mut out = Self {
            surfaces: [a, b],
            hulls: [a.hulls(), b.hulls()],
            tol,
            floor: res.zone().mm(),
            forms: [Implicit::of(a), Implicit::of(b)],
            pairs: Vec::new(),
        };
        let (Some(pa), Some(pb)) = (out.cells(0, ae), out.cells(1, be)) else {
            return Err(whole_pair(a, ae, b, be));
        };
        let boxes = |ps: &[Piece]| ps.iter().map(|p| p.bbox).collect::<Vec<_>>();
        let (ba, bb) = (boxes(&pa), boxes(&pb));
        let reach = ba
            .iter()
            .chain(&bb)
            .fold(0.0_f64, |m, x| m.max(magnitude(x)));
        let tree_a = BoxTree::new(&ba);
        let tree_b = BoxTree::new(&bb);
        let found = tree_a.pairs(&tree_b, box_tolerance(tol, reach));
        let mut kept = Vec::new();
        for (i, j) in found {
            budget.charge(1)?;
            stop.check()?;
            let (Some(&p), Some(&q)) = (pa.get(i), pb.get(j)) else {
                continue;
            };
            let pair = PiecePair::new(p, q);
            if let Some((next, axis)) = out.prove(&pair) {
                kept.push(PiecePair { next, axis, ..pair });
            }
        }
        out.pairs = kept;
        Ok(out)
    }

    /// The pairs the cells' proofs left, in order.
    #[must_use]
    pub fn pairs(&self) -> &[PiecePair] {
        &self.pairs
    }

    /// The piece of the first surface over `uv`, with its certified box and slabs (as the
    /// pieces of [`Candidates::pairs`] have them). `None` when a box cannot be enclosed.
    #[must_use]
    pub fn piece_of_a(&self, uv: Extent2d) -> Option<Piece> {
        self.piece(0, uv, None)
    }

    /// The piece of the second surface over `uv` (see [`Candidates::piece_of_a`]).
    #[must_use]
    pub fn piece_of_b(&self, uv: Extent2d) -> Option<Piece> {
        self.piece(1, uv, None)
    }

    /// The coincidence the proofs use, in mm.
    #[must_use]
    pub const fn tolerance(&self) -> f64 {
        self.tol
    }

    /// Whether the two pieces of `pair` are proved apart by more than the coincidence: by
    /// their boxes, then by the slab across either's normal, then by the sign of either
    /// side's implicit field over the other's piece (an analytic side's). `false` is "not
    /// proved", never "meet".
    #[must_use]
    pub fn apart(&self, pair: &PiecePair) -> bool {
        self.prove(pair).is_none()
    }

    /// `None` when the pair is proved apart (boxes, then the slab across either piece's unit
    /// normal at its middle, then the field sign); else the piece to halve next, from the axis along which the
    /// ranges overlap least (a heuristic, never a proof).
    fn prove(&self, pair: &PiecePair) -> Option<(usize, Option<Vec3>)> {
        if boxes_apart(&pair.a.bbox, &pair.b.bbox, self.tol) {
            return None;
        }
        // The best axis so far: its overlap (negative: a gap) and the wider piece.
        let mut best: Option<(f64, usize, Vec3)> = None;
        for (k, (p, q)) in [(&pair.a, &pair.b), (&pair.b, &pair.a)]
            .into_iter()
            .enumerate()
        {
            for slab in p.slabs.into_iter().flatten() {
                let Some(other) = self.range(1 - k, q, slab.axis) else {
                    continue;
                };
                let (ra, rb) = if k == 0 {
                    (slab.own, other)
                } else {
                    (other, slab.own)
                };
                // The gap along the axis is at most the distance when its length is at most
                // 1: the tolerance is scaled by an upper bound of that length.
                let gap_up = Ivl::from(rb.lo) - Ivl::from(ra.hi);
                let gap_down = Ivl::from(ra.lo) - Ivl::from(rb.hi);
                if gap_up.lo > slab.tol || gap_down.lo > slab.tol {
                    return None;
                }
                let overlap = ra.hi.min(rb.hi) - ra.lo.max(rb.lo);
                let wider = usize::from(rb.hi - rb.lo > ra.hi - ra.lo);
                if overlap.is_finite() && best.is_none_or(|(o, _, _)| overlap < o) {
                    best = Some((overlap, wider, slab.axis));
                }
            }
        }
        for (f, form) in self.forms.iter().enumerate() {
            let (Some(form), Some(s)) = (form, self.surfaces.get(1 - f)) else {
                continue;
            };
            let piece = if f == 0 { &pair.b } else { &pair.a };
            if field::apart(form, s, &piece.uv, &piece.bbox, self.tol) {
                return None;
            }
        }
        Some(best.map_or((pair.next, None), |(_, w, n)| (w, Some(n))))
    }

    /// The resolution, in mm: pieces no larger are not halved.
    pub(crate) const fn floor(&self) -> f64 {
        self.floor
    }

    /// A certified range of surface `side` over `p` along `dir`: the Bézier hull of its
    /// patch when `p` is one, else [`Surface::range_along`].
    fn range(&self, side: usize, p: &Piece, dir: Vec3) -> Option<Ivl> {
        if let (Some(h), Some(k)) = (self.hulls.get(side)?.as_ref(), p.cell)
            && let Some(hull) = h.get(k)
        {
            return Some(hull.range_along(dir));
        }
        self.surfaces.get(side)?.range_along(dir, p.uv)
    }

    /// The piece of surface `side` over `uv` with its box and slab; `cell` is the index of
    /// its patch's hull when the piece is the whole patch.
    fn piece(&self, side: usize, uv: Extent2d, cell: Option<usize>) -> Option<Piece> {
        let s = *self.surfaces.get(side)?;
        let hull = cell.and_then(|k| self.hulls.get(side)?.as_ref()?.get(k));
        let bbox = match hull.and_then(scx_surface::Hull::bbox) {
            Some(b) => b,
            None => s.box_over(uv)?,
        };
        self.piece_in(side, uv, bbox, cell)
    }

    /// [`Self::piece`] with the box given.
    fn piece_in(
        &self,
        side: usize,
        uv: Extent2d,
        bbox: Extent3d,
        cell: Option<usize>,
    ) -> Option<Piece> {
        let s = *self.surfaces.get(side)?;
        let hull = cell.and_then(|k| self.hulls.get(side)?.as_ref()?.get(k));
        let mut piece = Piece {
            uv,
            bbox,
            slabs: [None; 2],
            cell,
        };
        let normal = s
            .normal(uv.mid(), [Approach::Above; 2])
            .map(scx_num::vector3d::UnitVec3::as_vec);
        let axes = [hull.and_then(scx_surface::Hull::axis).or(normal), normal];
        let slab = |axis: Option<Vec3>| {
            let n = axis?;
            let own = self.range(side, &piece, n)?;
            let len = Vec3::new(Ivl::from(n.x), Ivl::from(n.y), Ivl::from(n.z))
                .norm()
                .hi;
            Some(Slab {
                axis: n,
                own,
                tol: (Ivl::from(self.tol) * Ivl::from(len)).hi,
            })
        };
        // The second slab is the surface's normal at the middle, when the net's own differs.
        let second = if axes[0] == axes[1] { None } else { axes[1] };
        piece.slabs = [slab(axes[0]), slab(second)];
        Some(piece)
    }

    /// The children of `pair` that survive the proofs: its larger piece (by box) halved on
    /// the side that shrinks it most, each half paired with the other piece. Zero children
    /// is a proof that the pair holds no meeting.
    ///
    /// Charges one unit per child pair and polls `stop` after each charge.
    ///
    /// # Errors
    ///
    /// [`SsiError::Unresolved`] where `pair` lies when it cannot be halved (every piece no
    /// larger than the resolution, or not splittable in `f64`); [`SsiError::Budget`] and
    /// [`SsiError::Stopped`].
    pub fn halve(
        &self,
        pair: &PiecePair,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Vec<PiecePair>, SsiError> {
        // The hinted piece first; the other when it cannot be split.
        let order = if pair.next == 0 { [0, 1] } else { [1, 0] };
        for which in order {
            let piece = if which == 0 { &pair.a } else { &pair.b };
            if piece.bbox.max_length() <= self.floor {
                continue;
            }
            let Some(halves) = self.split(which, piece, pair.axis) else {
                continue;
            };
            let mut out = Vec::with_capacity(2);
            for h in halves {
                budget.charge(1)?;
                stop.check()?;
                let child = if which == 0 {
                    PiecePair::new(h, pair.b)
                } else {
                    PiecePair::new(pair.a, h)
                };
                if let Some((next, axis)) = self.prove(&child) {
                    out.push(PiecePair {
                        next,
                        axis,
                        ..child
                    });
                }
            }
            return Ok(out);
        }
        Err(pair.unresolved())
    }

    /// Halves `pair` and its survivors, round by round, until none survives (`Ok(None)`:
    /// the pair is proved to hold no meeting) or one cannot be halved or the surviving front
    /// holds more than `front` pairs, which names the first such pair (`Ok(Some(pair))`: not
    /// proved either way). The width limit is a policy of effort, not of proof: a pair that
    /// is apart by a small margin keeps a narrow front, and one the surfaces cross keeps
    /// widening it. The same inputs give the same pair.
    ///
    /// # Errors
    ///
    /// [`SsiError::Budget`] and [`SsiError::Stopped`].
    pub fn settle(
        &self,
        pair: PiecePair,
        front: usize,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Option<PiecePair>, SsiError> {
        let mut now = vec![pair];
        while !now.is_empty() {
            if now.len() > front {
                return Ok(now.into_iter().next());
            }
            let mut next = Vec::new();
            for p in &now {
                match self.halve(p, budget, stop) {
                    Ok(children) => next.extend(children),
                    // The floor: this pair is undecided where it lies.
                    Err(SsiError::Unresolved { .. }) => return Ok(Some(*p)),
                    Err(e) => return Err(e),
                }
            }
            now = next;
        }
        Ok(None)
    }
}

/// The refusal of a pair left undecided over the whole of both extents: `Unresolved` at
/// the middle of the overlap of the surfaces' certified boxes (or of their union when they
/// do not overlap), of that box's longest side. A box that cannot be enclosed (a value not
/// finite) is an undecided region of unbounded size, named at the middle of the two
/// extents' middle points.
pub(crate) fn whole_pair(a: &Surface, ae: &Extent2d, b: &Surface, be: &Extent2d) -> SsiError {
    if let (Some(xa), Some(xb)) = (a.box_over(*ae), b.box_over(*be)) {
        let region = xa.intersect(xb).unwrap_or_else(|| xa.union(xb));
        SsiError::Unresolved {
            at: region.mid(),
            size: Length::from_mm(region.max_length()),
        }
    } else {
        let (pa, pb) = (a.point(ae.mid()), b.point(be.mid()));
        SsiError::Unresolved {
            at: pa + (pb - pa) * 0.5,
            size: Length::from_mm(f64::INFINITY),
        }
    }
}

/// The largest coordinate magnitude of a box.
fn magnitude(b: &Extent3d) -> f64 {
    [
        b.x_min(),
        b.y_min(),
        b.z_min(),
        b.x_max(),
        b.y_max(),
        b.z_max(),
    ]
    .iter()
    .fold(0.0_f64, |m, x| m.max(x.abs()))
}

/// The tolerance to give [`BoxTree::pairs`] so that every pair it drops is apart by more than
/// `tol` in the reals. Its test is `a.min > b.max + t`, and the rounded sum differs from the
/// real one by at most `2^-53 (|b.max| + t)`; with `reach` the largest coordinate of any box
/// and `EPSILON = 2^-52`, `t = tol + 4 EPSILON (reach + tol)` (rounded up) exceeds `tol` by
/// more than that error, so `a.min > fl(b.max + t)` gives `a.min > b.max + tol`. The same
/// holds for `b.min - t`.
fn box_tolerance(tol: f64, reach: f64) -> f64 {
    let pad = (Ivl::from(4.0 * f64::EPSILON) * (Ivl::from(reach) + Ivl::from(tol))).hi;
    (Ivl::from(tol) + Ivl::from(pad)).hi
}

/// Whether two boxes are apart by more than `tol` on some axis, by interval subtraction
/// (the difference rounded down, compared with `tol`).
pub(crate) fn boxes_apart(a: &Extent3d, b: &Extent3d, tol: f64) -> bool {
    let gap = |lo: f64, hi: f64| (Ivl::from(lo) - Ivl::from(hi)).lo > tol;
    gap(a.x_min(), b.x_max())
        || gap(a.y_min(), b.y_max())
        || gap(a.z_min(), b.z_max())
        || gap(b.x_min(), a.x_max())
        || gap(b.y_min(), a.y_max())
        || gap(b.z_min(), a.z_max())
}

impl Candidates<'_> {
    /// Surface `side`'s pieces over `extent`: the cells of its Bézier form with their
    /// certified boxes, u-major. The extent itself as the one cell when the form cannot be
    /// built (the box then is still certified, by the surface). `None` when a box cannot be
    /// enclosed.
    fn cells(&self, side: usize, extent: &Extent2d) -> Option<Vec<Piece>> {
        let s = *self.surfaces.get(side)?;
        let (us, vs) = match s.to_nurbs(extent) {
            Ok(n) => (
                breaks(
                    &n,
                    ParamAxis::U,
                    extent.x_interval().min(),
                    extent.x_interval().max(),
                ),
                breaks(
                    &n,
                    ParamAxis::V,
                    extent.y_interval().min(),
                    extent.y_interval().max(),
                ),
            ),
            Err(_) => (
                vec![extent.x_min(), extent.x_max()],
                vec![extent.y_min(), extent.y_max()],
            ),
        };
        let hulls = self.hulls.get(side)?.as_ref();
        let mut out = Vec::with_capacity(us.len().saturating_sub(1) * vs.len().saturating_sub(1));
        for u in us.windows(2) {
            for v in vs.windows(2) {
                let (&[u0, u1], &[v0, v1]) = (u.first_chunk()?, v.first_chunk()?);
                let uv = Extent2d::from_bounds(u0, v0, u1, v1)?;
                // The hull serves a piece that is a whole patch; a part of one is enclosed
                // by the surface (tighter than the patch's hull).
                let cell = hulls
                    .and_then(|h| h.containing(&uv))
                    .filter(|(_, h)| h.uv() == uv)
                    .map(|(k, _)| k);
                out.push(self.piece(side, uv, cell)?);
            }
        }
        Some(out)
    }

    /// A piece's two halves on the uv side that narrows them most, each with its certified
    /// box: by their range along `axis` where there is one (the direction the proof that is
    /// failing looks along), else by their boxes' longest side. `None` when no side splits
    /// (a middle equal to an end) or a box cannot be enclosed. The first half is the lower.
    fn split(&self, side: usize, p: &Piece, axis: Option<Vec3>) -> Option<[Piece; 2]> {
        let s = *self.surfaces.get(side)?;
        let (lo, hi) = (p.uv.min(), p.uv.max());
        let halves = |across_u: bool| -> Option<[(Extent2d, Extent3d); 2]> {
            let (a, b) = if across_u { (lo.x, hi.x) } else { (lo.y, hi.y) };
            let m = 0.5 * (a + b);
            if !(m > a && m < b) {
                return None;
            }
            let (e0, e1) = if across_u {
                (
                    Extent2d::from_bounds(lo.x, lo.y, m, hi.y)?,
                    Extent2d::from_bounds(m, lo.y, hi.x, hi.y)?,
                )
            } else {
                (
                    Extent2d::from_bounds(lo.x, lo.y, hi.x, m)?,
                    Extent2d::from_bounds(lo.x, m, hi.x, hi.y)?,
                )
            };
            Some([(e0, s.box_over(e0)?), (e1, s.box_over(e1)?)])
        };
        let worst = |h: &[(Extent2d, Extent3d); 2]| {
            let width = |&(uv, bbox): &(Extent2d, Extent3d)| {
                axis.and_then(|n| s.range_along(n, uv))
                    .map_or_else(|| bbox.max_length(), |r| r.hi - r.lo)
            };
            width(&h[0]).max(width(&h[1]))
        };
        let chosen = match (halves(true), halves(false)) {
            (Some(u), Some(v)) => Some(if worst(&v) < worst(&u) { v } else { u }),
            (Some(h), None) | (None, Some(h)) => Some(h),
            (None, None) => None,
        }?;
        let make = |(uv, bbox): (Extent2d, Extent3d)| self.piece_in(side, uv, bbox, None);
        Some([make(chosen[0])?, make(chosen[1])?])
    }
}

/// The distinct knots of a direction within `[lo, hi]`, with `lo` and `hi` themselves as the
/// ends (the form is over the extent, so its domain is that).
fn breaks(n: &scx_nurbs_surface::NurbsSurface<3>, axis: ParamAxis, lo: f64, hi: f64) -> Vec<f64> {
    let mut out = vec![lo];
    for (k, _) in n.knot_vector(axis).breakpoints() {
        if k > lo && k < hi && out.last().is_some_and(|&l| k > l) {
            out.push(k);
        }
    }
    out.push(hi);
    out
}
