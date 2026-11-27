// Written for solidcorex. Apache-2.0.
//! Cells: data derived from records and cached beside them (01 §3.2, research 63 §6.9).
//!
//! A face's cells live in its record, an edge's in its geometry part, and data that crosses
//! records (vertex stars, a shell's faces, the face box tree) in the body's
//! [`BodyCells`](crate::body::BodyCells). Each cell is a `OnceLock` filled on first read by
//! a pure function of what it is derived from, so a read takes `&Brep`, two threads that
//! race to fill one store the same bits, and a cell never changes a result, only its speed.
//!
//! **A record's cells read that record alone** (63 §6.9). A face's boundary tree, uv box and
//! surface box come from its surface and pcurves; an edge's curve box from its shape. So:
//!
//! - [`Draft::face_mut`](crate::Draft::face_mut) and
//!   [`Draft::edge_mut`](crate::Draft::edge_mut) reset the cells of the record they hand
//!   out, and a record added to a draft is reset as it goes in.
//! - A write to an edge, a vertex or the adjacency resets no face's cells: nothing in them
//!   came from there. (Resetting them could not be made sound without copying: a face
//!   record shared by `Arc` with the body a draft came from can be filled by that body
//!   after the draft's reset, and the draft would see it.)
//! - A record enters a table only through an `add_*`, which resets its cells, or as a copy
//!   of a record already there (a copy-on-write, `compact`), whose fields the copy keeps. So
//!   a clone keeps the cells filled: a user's clone whose fields are written goes back in
//!   through an `add_*`, and a table copied for a write keeps every cell it can.
//!
//! The cells hold boxes, so a record grows by a pointer and a once-flag per cell, not by
//! what the cell holds; an empty cell allocates nothing.
//!
//! A cell never takes part in a comparison: two records with the same fields are equal
//! whatever their cells hold.

use std::sync::OnceLock;

use scx_curve::Curve;
use scx_num::domain::ParamDomain;
use scx_num::extent1d::Extent1d;
use scx_num::extent3d::Extent3d;
use scx_num::interval::{Ivl, round_down, round_up};
use scx_num::vector3d::Point3;
use scx_surface::Surface;

use crate::boundary::Boundary;
use crate::record::{EdgeGeom, EdgeShape, FaceRecord};

/// A cell of one face: a value derived from its record alone, kept in
/// [`FaceRecord`]'s cells and filled on first read.
pub(crate) trait FaceCell: Send + Sync + Sized + 'static {
    /// The slot that holds it.
    fn slot(cells: &FaceCells) -> &OnceLock<Box<Self>>;

    /// The value for `rec`: deterministic, a function of `rec`'s fields.
    fn fill(rec: &FaceRecord) -> Self;

    /// The value for `rec`, filled now if no read has filled it.
    fn of(rec: &FaceRecord) -> &Self {
        Self::slot(&rec.cells).get_or_init(|| Box::new(Self::fill(rec)))
    }
}

/// The cells of one face.
#[derive(Debug, Clone, Default)]
pub(crate) struct FaceCells {
    boundary: OnceLock<Box<Boundary>>,
    surface_box: OnceLock<Box<SurfaceBox>>,
}

impl FaceCells {
    /// Whether no cell is filled: a reset would change nothing.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.boundary.get().is_none() && self.surface_box.get().is_none()
    }

    /// Empties every cell.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

impl PartialEq for FaceCells {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl FaceCell for Boundary {
    fn slot(cells: &FaceCells) -> &OnceLock<Box<Self>> {
        &cells.boundary
    }

    fn fill(rec: &FaceRecord) -> Self {
        Self::build(rec)
    }
}

/// A box that holds the face's surface over its uv region: its pcurves' image, not grown
/// by any accuracy. `None` for a face that reaches infinity or a box that cannot be
/// enclosed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SurfaceBox(pub(crate) Option<Extent3d>);

impl FaceCell for SurfaceBox {
    fn slot(cells: &FaceCells) -> &OnceLock<Box<Self>> {
        &cells.surface_box
    }

    fn fill(rec: &FaceRecord) -> Self {
        let b = Boundary::of(rec);
        let Some(whole) = b.uv_box().and_then(|uv| rec.surface.box_over(uv)) else {
            return Self(None);
        };
        // A ruled surface is straight along `v`, so each point of a bounded region lies on
        // a segment between two points of its boundary, and the face lies in the hull of
        // its boundary's image. The pieces' own boxes, mapped, are usually much tighter
        // than the uv box mapped (a triangle on a tilted plane, a band on a cone).
        //
        // Round a whole period of a cylinder or cone the hull is no tighter than the uv box
        // mapped (every angle is on the boundary), and costs a box per piece: skipped.
        let ruled = match &rec.surface {
            Surface::Plane(_) => true,
            Surface::Cylinder(_) | Surface::Cone(_) => !spans_a_period(&rec.surface, b),
            _ => false,
        };
        if ruled && b.is_bounded_region() {
            let hull = b
                .piece_boxes()
                .map(|uv| rec.surface.box_over(uv))
                .try_fold(None, |acc: Option<Extent3d>, bx| {
                    let bx = bx?;
                    Some(Some(acc.map_or(bx, |a| a.union(bx))))
                })
                .flatten();
            if let Some(tight) = hull.and_then(|h| h.intersect(whole)) {
                return Self(Some(tight));
            }
        }
        Self(Some(whole))
    }
}

/// Whether the face's uv box runs round a whole period in `u`.
fn spans_a_period(s: &Surface, b: &Boundary) -> bool {
    match (s.domain().u, b.uv_box()) {
        (ParamDomain::Periodic { period, .. }, Some(uv)) => uv.max().x - uv.min().x >= period,
        _ => false,
    }
}

/// The cells of one edge's geometry part.
#[derive(Debug, Clone, Default)]
pub(crate) struct EdgeCells {
    curve_box: OnceLock<Option<Box<Extent3d>>>,
}

impl EdgeCells {
    /// Whether no cell is filled.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.curve_box.get().is_none()
    }

    /// Empties every cell.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

impl PartialEq for EdgeCells {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// How many parts an edge's curve is enclosed in, each by one interval evaluation: the
/// box's overshoot is first order in a part's length (research 63 §6.9's table).
const EDGE_PARTS: usize = 8;

/// A box holding the edge's curve over its interval, not grown by any accuracy; `None` for
/// a collapsed edge or a curve that cannot be enclosed. Filled on first read.
pub(crate) fn curve_box(geom: &EdgeGeom) -> Option<Extent3d> {
    geom.cells
        .curve_box
        .get_or_init(|| match &geom.shape {
            EdgeShape::Curve { curve, interval } => enclose_curve(curve, *interval).map(Box::new),
            EdgeShape::Collapsed => None,
        })
        .as_deref()
        .copied()
}

/// The hull of the interval enclosures of `curve` over `EDGE_PARTS` parts of `interval`.
fn enclose_curve(curve: &Curve, interval: Extent1d) -> Option<Extent3d> {
    let n = if matches!(curve, Curve::Line(_)) {
        1
    } else {
        EDGE_PARTS
    };
    parts(interval.min(), interval.max(), n)
        .map(|(a, b)| point_box(curve.point_over(Ivl::new(a, b))))
        .try_fold(None, |acc: Option<Extent3d>, bx| {
            let bx = bx?;
            Some(Some(acc.map_or(bx, |a| a.union(bx))))
        })
        .flatten()
}

/// `[lo, hi]` cut into `n` parts, in order, the cuts monotone and the last end `hi`.
pub(crate) fn parts(lo: f64, hi: f64, n: usize) -> impl Iterator<Item = (f64, f64)> {
    let n = if hi > lo { n.max(1) } else { 1 };
    let at = move |i: usize| {
        if i == n {
            hi
        } else {
            (lo + (hi - lo) * (i as f64 / n as f64)).clamp(lo, hi)
        }
    };
    (0..n).map(move |i| (at(i), at(i + 1)))
}

/// The box of an enclosure, `None` unless finite.
pub(crate) fn point_box(p: Point3<Ivl>) -> Option<Extent3d> {
    if !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
        return None;
    }
    Extent3d::from_bounds(p.x.lo, p.y.lo, p.z.lo, p.x.hi, p.y.hi, p.z.hi)
}

/// `bx` with every side moved out by `d` (at least 0), rounded outward so the grown box
/// still holds every point within `d` of `bx`.
pub(crate) fn grown(bx: Extent3d, d: f64) -> Option<Extent3d> {
    let (lo, hi) = (bx.min(), bx.max());
    let down = |x: f64| round_down(x - d);
    let up = |x: f64| round_up(x + d);
    Extent3d::from_bounds(
        down(lo.x),
        down(lo.y),
        down(lo.z),
        up(hi.x),
        up(hi.y),
        up(hi.z),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_cover_the_interval() {
        let p: Vec<_> = parts(0.0, 1.0, 4).collect();
        assert_eq!(p.len(), 4);
        assert_eq!(p.first().map(|x| x.0), Some(0.0));
        assert_eq!(p.last().map(|x| x.1), Some(1.0));
        assert!(p.iter().zip(p.iter().skip(1)).all(|(a, b)| a.1 == b.0));
        assert_eq!(parts(2.0, 2.0, 8).collect::<Vec<_>>(), [(2.0, 2.0)]);
    }

    #[test]
    fn growing_rounds_outward() {
        let b = Extent3d::from_bounds(0.1, 0.2, 0.3, 0.4, 0.5, 0.6);
        let g = b.and_then(|b| grown(b, 0.1));
        assert!(g.is_some_and(|g| g.min().x <= 0.1 - 0.1 && g.max().z >= 0.6 + 0.1));
    }
}
