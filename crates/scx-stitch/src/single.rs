// Written for solidcorex. Apache-2.0.
//! The single-body operations (74 §4.3 step 7, ST5): [`open_edges`], [`gather`] and
//! [`sew`].
//!
//! [`sew`] stitches a body's own open edges by [`stitch`](crate::stitch)'s rules. It cuts
//! the body into *pieces*, the connected sets of faces that already share edges (an edge
//! with two or more fins joins its faces), and stitches the pieces as sheets. Each piece
//! keeps the body's ids, so operand `i` of the result's lineage names the body's own
//! entities, the faces of `pieces[i]`. The edges the body already shares, the edges of
//! three or more faces it came with among them, stay as they are: only open edges are
//! proved, joined or cut.

use std::collections::{BTreeMap, BTreeSet};

use scx_brep::op::{Cx, Done, Report};
use scx_brep::{
    Brep, Draft, EdgeId, EdgeShape, Entity, EntityKind, FaceId, RegionId, ShellId, ShellRecord,
    VertexId,
};
use scx_num::budget::{Budget, Stop};
use scx_num::units::Length;

use crate::error::{StitchDefect, StitchError, StitchUnsupported, too_large};
use crate::report::{NoItems, StitchReport, Want};
use crate::stitch::{gathered, same_document, stitched};

/// Work charged per entity read or copied.
const UNIT: u64 = 1;

/// The body's open edges: the curve edges used by exactly one face use, in id order.
///
/// An edge a face uses twice (a seam) and an edge of two or more fins are not open; a
/// collapsed edge (a pole) is not an edge to join. This is the set a [`sew`] works on, and
/// [`Want::Solid`] refuses a body that has any left.
#[must_use]
pub fn open_edges(body: &Brep) -> Vec<EdgeId> {
    body.edges()
        .filter(|e| e.fins().len() == 1 && matches!(e.shape(), EdgeShape::Curve { .. }))
        .map(scx_brep::Edge::id)
        .collect()
}

/// The bodies gathered into one, unsewn: body 0 edited in place, every other body copied in
/// beside it, so that nothing is identified or joined (use [`stitch`](crate::stitch) to
/// gather and join, or [`sew`] on the result).
///
/// Body `i` is operand `i` of the result's lineage, as for [`stitch`](crate::stitch): body
/// 0's entities keep their ids, and every entity copied from another is derived from its
/// operand's. Regions and shells are copied whole, so solids stay solids. A body given
/// twice is gathered twice, which gives the result two coincident copies. Nothing checks
/// that the bodies' solids keep clear of each other: that is the check's.
///
/// # Errors
///
/// [`StitchRefusal::OtherDocument`](crate::StitchRefusal::OtherDocument) for a body
/// finished under another resolution than `cx.doc`'s; [`StitchUnsupported::TooLarge`] past
/// the ids; [`StitchError::Exhausted`] and [`StitchError::Cancelled`] from `cx`; and
/// [`StitchError::Defect`] for the method's own failure. No bodies give an empty body.
pub fn gather(bodies: &[&Brep], cx: &mut Cx<'_>) -> Result<Done<NoItems>, StitchError> {
    gathered(bodies, cx)
}

/// A sewn body: the stitch of the pieces a body was cut into.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Sewn {
    /// The sewn body and its [`Changes`](scx_brep::Changes). Piece `i` is operand `i`;
    /// the pieces keep the input body's ids, so the lineage names the input's entities.
    pub done: Done<NoItems>,
    /// What the stitch joined, cut and left.
    pub report: StitchReport,
    /// The input body's faces in each piece, by lowest face id; piece `i` is operand `i`.
    /// One piece holding every face when the body had no open edge (nothing is cut then).
    pub pieces: Vec<Vec<FaceId>>,
    /// The result's open edges, in id order ([`open_edges`] of the result).
    pub open: Vec<EdgeId>,
    /// Each curve edge of the result the stitch cut or joined, and the input body's edge it
    /// is (a piece of): a joined edge names the input edge it kept.
    pub inputs: BTreeMap<EdgeId, EdgeId>,
}

/// A body's open edges stitched, by [`stitch`](crate::stitch)'s rules (74 §4.3 step 7).
///
/// The body is cut into pieces (see the [module](self)): the connected sets of faces that
/// share edges already. The pieces are stitched as sheets, so vertices of different
/// pieces within coincidence are one vertex, and open edges of different pieces with the
/// same ends within coincidence of each other (certified) are one edge. Whatever the
/// stitch proves not joined is [`Sewn::open`], never guessed. A body with no open edge is
/// returned as it is (one piece, empty changes), unless `want` is [`Want::Solid`] and the
/// body is a sheet to orient.
///
/// # Errors
///
/// - [`StitchError`] as [`stitch`](crate::stitch) gives for its pieces: a piece with two
///   vertices within coincidence of each other (a sheet folded back on itself, whose own
///   open edges meet) is
///   [`StitchRefusal::VerticesWithinCoincidence`](crate::StitchRefusal::VerticesWithinCoincidence),
///   naming the piece as the operand; an open edge left for `want` [`Want::Solid`] is
///   [`StitchRefusal::NotSolid`](crate::StitchRefusal::NotSolid).
/// - [`StitchUnsupported::Loose`] for a body with a wire edge or a lone vertex: a piece
///   holds faces only, and sewing would lose them.
pub fn sew(body: &Brep, want: Want, cx: &mut Cx<'_>) -> Result<Sewn, StitchError> {
    sewn(body, want, None, cx)
}

/// [`sew`] within `gap`: open edges and their vertices reach at least `gap`, so ends and
/// open edges of different pieces that far apart are one (RP5's sew, 75 §7).
///
/// Vertices identified are raised, `Measured`, to the farthest end the merge moves, as in
/// [`stitch`](crate::stitch). Of a joined pair the curve kept is the stitch's (the smaller
/// reach, then the lower midpoint); its uses on the faces of the piece dropped are written
/// as at coincidence, exactly where the curve lies on the face, otherwise as the curve's
/// projection with the certified gap as the use's bound (`Certified`,
/// [`Claim::PcurveOnSurface`](scx_num::cert::Claim::PcurveOnSurface), through
/// [`Draft::write_pcurves_projecting`](scx_brep::Draft::write_pcurves_projecting)). The
/// kept edge's accuracy is then raised, `Measured`, to the largest such gap where that is
/// beyond its reach, and is in the report's raises. Nothing else changes: the body's
/// resolution, its other edges and their accuracies. Whatever is not within `gap` stays
/// open ([`Sewn::open`]).
///
/// # Errors
///
/// As [`sew`]; [`EdgePending::Fit`](crate::EdgePending::Fit) for a joined pair whose kept
/// curve has no projection onto a dropped face the pcurve writer can give. Two vertices of one
/// piece within `gap` of each other are
/// [`StitchRefusal::VerticesWithinCoincidence`](crate::StitchRefusal::VerticesWithinCoincidence).
/// `gap` must be finite and above zero; otherwise the result is [`sew`]'s.
pub fn sew_within(
    body: &Brep,
    gap: Length,
    want: Want,
    cx: &mut Cx<'_>,
) -> Result<Sewn, StitchError> {
    let g = gap.mm();
    sewn(body, want, (g.is_finite() && g > 0.0).then_some(g), cx)
}

fn sewn(body: &Brep, want: Want, gap: Option<f64>, cx: &mut Cx<'_>) -> Result<Sewn, StitchError> {
    same_document(&[body], cx)?;
    for shell in body.shells() {
        let rec = shell.record();
        if let Some(&w) = rec.wires.first() {
            return Err(StitchError::Unsupported(StitchUnsupported::Loose {
                at: Entity::Edge(w),
            }));
        }
        if let Some(&v) = rec.lone_vertices.first() {
            return Err(StitchError::Unsupported(StitchUnsupported::Loose {
                at: Entity::Vertex(v),
            }));
        }
    }
    let mut budget = cx.budget();
    let open = open_edges(body);
    let solid = body.regions().any(scx_brep::Region::is_solid);
    if open.is_empty() && (want == Want::Sheets || solid) {
        let (sewn, changes) = body
            .edit()
            .finish(cx.doc)
            .map_err(|u| StitchError::Defect(StitchDefect::Finish(Box::new(u.cause))))?;
        let faces: Vec<FaceId> = sewn.faces().map(scx_brep::Face::id).collect();
        return Ok(Sewn {
            done: Done::new(sewn, changes, Report::empty(budget.used())),
            report: StitchReport::default(),
            pieces: if faces.is_empty() {
                Vec::new()
            } else {
                vec![faces]
            },
            open,
            inputs: BTreeMap::new(),
        });
    }
    let pieces = pieces(body, &mut budget, cx.stop)?;
    let mut sheets = Vec::with_capacity(pieces.len());
    for faces in &pieces {
        budget.charge(UNIT)?;
        sheets.push(piece(body, faces, cx)?);
    }
    let refs: Vec<&Brep> = sheets.iter().collect();
    let (stitched, names) = stitched(&refs, want, gap, cx)?;
    let open = open_edges(&stitched.done.body);
    let inputs = names
        .into_iter()
        .filter(|(e, _)| stitched.done.body.edge(*e).is_some())
        .map(|(e, (_, input))| (e, input))
        .collect();
    Ok(Sewn {
        done: stitched.done,
        report: stitched.report,
        pieces,
        open,
        inputs,
    })
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while let Some(&p) = parent.get(i) {
        if p == i {
            break;
        }
        let up = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(i) {
            *slot = up;
        }
        i = up;
    }
    i
}

/// The body's faces in connected sets: two faces are connected when an edge with two or
/// more fins has both. Each set in id order, the sets by their lowest face.
fn pieces(body: &Brep, budget: &mut Budget, stop: &Stop) -> Result<Vec<Vec<FaceId>>, StitchError> {
    let mut parent: Vec<usize> = (0..body.face_slots()).collect();
    for e in body.edges() {
        budget.charge(UNIT)?;
        let mut fins = e.fins();
        let Some(first) = fins.next() else { continue };
        for fin in fins {
            let (a, b) = (
                find(&mut parent, first.face.index()),
                find(&mut parent, fin.face.index()),
            );
            if a != b
                && let Some(slot) = parent.get_mut(a.max(b))
            {
                *slot = a.min(b);
            }
        }
    }
    stop.check()?;
    let mut sets: BTreeMap<usize, Vec<FaceId>> = BTreeMap::new();
    for f in body.faces() {
        sets.entry(find(&mut parent, f.id().index()))
            .or_default()
            .push(f.id());
    }
    Ok(sets.into_values().collect())
}

/// How far a table has been filled, to keep ids.
struct Slot(u32);

impl Slot {
    /// Leaves the ids before `want` unused, then takes `want`.
    fn take(&mut self, d: &mut Draft, kind: EntityKind, want: u32) -> Result<(), StitchError> {
        if want > self.0 {
            d.skip(kind, want - self.0).map_err(too_large)?;
        }
        self.0 = want + 1;
        Ok(())
    }
}

/// The sheet of `faces` of `body`: those faces, the edges and vertices they use and the
/// shells and regions they lie in, each keeping its id in the body (the ids between are
/// left unused), with the body's radial order on each edge.
fn piece(body: &Brep, faces: &[FaceId], cx: &Cx<'_>) -> Result<Brep, StitchError> {
    let defect = || StitchError::Defect(StitchDefect::Draft);
    let mut shells: BTreeSet<ShellId> = BTreeSet::new();
    let mut edges: BTreeSet<EdgeId> = BTreeSet::new();
    for &f in faces {
        let face = body.face(f).ok_or_else(defect)?;
        shells.extend(face.record().sides);
        edges.extend(face.uses().map(|u| u.record().edge));
    }
    let mut vertices: BTreeSet<VertexId> = BTreeSet::new();
    for &e in &edges {
        vertices.extend(body.edge(e).ok_or_else(defect)?.geometry().ends);
    }
    let mut regions: BTreeSet<RegionId> = BTreeSet::new();
    for &s in &shells {
        regions.insert(body.shell(s).ok_or_else(defect)?.record().region);
    }
    regions.remove(&RegionId::OUTSIDE);
    let mut d = Draft::new();
    let mut at = Slot(1);
    for &r in &regions {
        at.take(&mut d, EntityKind::Region, r.raw())?;
        let solid = body.region(r).ok_or_else(defect)?.is_solid();
        d.add_region(solid).map_err(too_large)?;
    }
    let mut at = Slot(0);
    for &s in &shells {
        at.take(&mut d, EntityKind::Shell, s.raw())?;
        let region = body.shell(s).ok_or_else(defect)?.record().region;
        d.add_shell(ShellRecord::new(region)).map_err(too_large)?;
    }
    let mut at = Slot(0);
    for &v in &vertices {
        at.take(&mut d, EntityKind::Vertex, v.raw())?;
        let rec = body.vertex(v).ok_or_else(defect)?.record().clone();
        d.add_vertex(rec).map_err(too_large)?;
    }
    let mut at = Slot(0);
    for &e in &edges {
        at.take(&mut d, EntityKind::Edge, e.raw())?;
        let geom = body.edge(e).ok_or_else(defect)?.geometry().clone();
        d.add_edge(geom).map_err(too_large)?;
    }
    let mut at = Slot(0);
    for &f in faces {
        at.take(&mut d, EntityKind::Face, f.raw())?;
        let rec = body.face(f).ok_or_else(defect)?.record().clone();
        d.add_face(rec).map_err(too_large)?;
    }
    for &e in &edges {
        let fins: Vec<_> = body.edge(e).ok_or_else(defect)?.fins().collect();
        d.adjacency_mut(e).ok_or_else(defect)?.fins = fins;
    }
    d.finish(cx.doc)
        .map(|(sheet, _)| sheet)
        .map_err(|u| StitchError::Defect(StitchDefect::Finish(Box::new(u.cause))))
}
