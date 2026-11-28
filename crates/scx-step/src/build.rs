// Written for solidcorex (docs/research/74-step6-idiom-prep.md §6.3, §29).
//! The analytic build of one body: the file's topology as it shares it, each edge's and
//! vertex's accuracy raised to its certified gap, the exact pcurves, the orientation by
//! parity and the certified volume sign, and the import check.
//!
//! The steps, in order:
//!
//! 1. **Records.** Each `VERTEX_POINT` is a vertex and each `EDGE_CURVE` an edge, made the
//!    first time a face uses it: entities the file shares are shared, and nothing else is
//!    joined (two vertices at one point stay two). An edge runs its curve's way, from the
//!    vertex its `same_sense` puts first, over the parameters of the vertices' feet on the
//!    curve; on a closed curve the end is taken past the start, a whole turn when the
//!    vertices are one. On an open curve whose feet run against `same_sense`, the vertices
//!    are believed: the edge is turned and [`Note::TurnedEdge`] says so.
//! 2. **Gaps** (09 §6, R3). Each edge's curve against each face's surface that uses it,
//!    and each vertex against each edge end, is enclosed: within the zone it is `None`;
//!    beyond, the entity records the certified upper bound, and where that is past the
//!    file's stated uncertainty (or the zone when it states none) a [`Raise`] is recorded.
//!    A gap the budget leaves straddling is [`BuildError::Undecided`]: never a guess.
//! 3. **Faces.** Loops are given relative to the surface's natural normal and written by
//!    the exact pcurve writer, with a collapsed edge where a loop passes a pole. A face
//!    with an outer bound, or on a plane, whose loops prove to bound the region from inside
//!    is turned ([`Note::TurnedFace`]). Where no bound is marked outer, the loop that alone
//!    bounds a region is put first.
//! 4. **Orientation** (R4). Within each shell, two faces through an edge must use it in
//!    opposite senses about their normals; a walk from the shell's first face turns the
//!    faces that disagree (an odd cycle is [`BuildError::NotOrientable`]). A solid's shell
//!    then needs a positive certified volume sign for the region it bounds; a negative one
//!    turns the whole shell ([`Note::TurnedShell`]). Sheets take parity alone.
//! 5. **Check.** `scx_check::check` under `Scope::Import`; a body with findings is given
//!    with them.

use std::collections::BTreeMap;
use std::sync::Arc;

use scx_brep::op::Cx;
use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, FaceId, FaceRecord, FaceSide, LoopRecord,
    PcurveWriteError, RegionId, ShellId, ShellRecord, VertexId, VertexRecord,
};
use scx_check::Scope;
use scx_curve::Curve;
use scx_mass::{Sign, volume_sign};
use scx_num::budget::{Budget, Stop};
use scx_num::cert::{Basis, Cert, Claim, EdgeEnd, Method, Reads, Recorded};
use scx_num::consts::MAX_ACCURACY_RATIO;
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::Point3;
use scx_p21::Exchange;
use scx_p21::part42::read::Reader;
use scx_surface::{Aim, Bracket, GapError, Surface, curve_surface_gap, vertex_curve_gap};

use crate::geom::{self, NotMade};
use crate::import::{BuildError, Built, GeomCause, Import, Note, Unbuilt, Waits};
use crate::skeleton::{boolean, get, reference, refs, wrong};
use crate::{
    GapKind, ImportedBody, Raise, ReadOptions, ShellRole, SolidOrSheet, StepError, Uncertainty,
    read,
};

type Result<T> = core::result::Result<T, BuildError>;

fn sense(forward: bool) -> Sense {
    if forward {
        Sense::Forward
    } else {
        Sense::Reversed
    }
}

/// `uses` run the other way: reversed in order, each sense turned.
fn reversed(uses: &[(EdgeId, Sense)]) -> Vec<(EdgeId, Sense)> {
    uses.iter().rev().map(|&(e, s)| (e, s.reversed())).collect()
}

fn made(entity: u64, n: NotMade) -> BuildError {
    match n {
        NotMade::Waits(waits) => BuildError::Pending { entity, waits },
        NotMade::Bad(cause) => BuildError::Geometry { entity, cause },
    }
}

/// An edge as the build holds it.
struct EdgeIn {
    entity: u64,
    id: EdgeId,
    curve: Curve,
    interval: Extent1d,
    ends: [VertexId; 2],
}

/// A face as the build holds it.
struct FaceIn {
    entity: u64,
    /// Its shell, by index in the body's plan.
    shell: usize,
    surface: Surface,
    /// Whether the face's normal (as built) is the surface's natural normal.
    natural: bool,
    /// Whether the file gives it a `FACE_OUTER_BOUND`, or it is on a plane.
    bounded: bool,
    /// Whether a bound is marked `FACE_OUTER_BOUND`.
    marked: bool,
    /// Its loops in the file's senses about the face's normal, the outer bound first, with
    /// a collapsed edge at each joint at a pole.
    loops: Vec<Vec<(EdgeId, Sense)>>,
    id: Option<FaceId>,
}

struct Builder<'a> {
    ex: &'a Exchange,
    body: &'a ImportedBody,
    doc: &'a Document,
    reader: Reader<'a>,
    draft: Draft,
    vertices: Vec<(u64, VertexId, Point3)>,
    vertex_at: BTreeMap<u64, (VertexId, Point3)>,
    edges: Vec<EdgeIn>,
    edge_at: BTreeMap<u64, (EdgeId, bool)>,
    faces: Vec<FaceIn>,
    notes: Vec<Note>,
    raised: Vec<Raise>,
}

/// Builds `body` from `ex` in `cx`'s document. The body is built once; the product tree
/// places it.
///
/// # Errors
///
/// [`BuildError`]: a kind a later task reads ([`BuildError::Pending`]), an entity or
/// geometry the body cannot be made of, a gap the budget leaves undecided, a shell that
/// cannot be oriented, or records that do not finish.
pub fn build(ex: &Exchange, body: &ImportedBody, cx: &mut Cx<'_>) -> Result<Built> {
    let mut b = Builder {
        ex,
        body,
        doc: cx.doc,
        reader: Reader::new(ex, body.context.units),
        draft: Draft::new(),
        vertices: Vec::new(),
        vertex_at: BTreeMap::new(),
        edges: Vec::new(),
        edge_at: BTreeMap::new(),
        faces: Vec::new(),
        notes: Vec::new(),
        raised: Vec::new(),
    };
    for (k, shell) in body.shells.iter().enumerate() {
        for plan in &shell.faces {
            let f = b.face(plan.entity, plan.surface, plan.same_sense, k)?;
            b.faces.push(f);
        }
    }
    let mut budget = cx.budget();
    b.gaps(&mut budget, cx.stop)?;
    let shells = b.shells()?;
    b.write_faces(&shells)?;
    b.orient(&shells)?;
    let body_out = b.finish(&shells, &mut budget, cx.stop)?;
    let findings = scx_check::check(&body_out, cx, Scope::Import).map_err(BuildError::Check)?;
    b.raised.sort_by_key(|r| r.entity);
    Ok(Built {
        entity: body.entity,
        name: body.name.clone(),
        kind: body.kind,
        body: body_out,
        raised: b.raised,
        notes: b.notes,
        findings,
        faces: b
            .faces
            .iter()
            .filter_map(|f| f.id.map(|id| (f.entity, id)))
            .collect(),
        edges: b.edges.iter().map(|e| (e.entity, e.id)).collect(),
        vertices: b.vertices.iter().map(|&(s, v, _)| (s, v)).collect(),
    })
}

/// Reads `ex` and builds each body it lists, once. A body that cannot be built is
/// [`Unbuilt`] with its cause, and the rest are built; the report's
/// [`raised`](crate::ImportReport::raised) holds every built body's raises in entity order.
///
/// # Errors
///
/// [`StepError`] where the read refuses the file.
pub fn import(
    ex: &Exchange,
    opts: &ReadOptions,
    cx: &mut Cx<'_>,
) -> core::result::Result<Import, StepError> {
    let mut read = read(ex, opts)?;
    let mut built = Vec::new();
    let mut unbuilt = Vec::new();
    for body in &read.bodies {
        match build(ex, body, cx) {
            Ok(b) => {
                read.report.raised.extend(b.raised.iter().copied());
                built.push(b);
            }
            Err(why) => unbuilt.push(Unbuilt {
                entity: body.entity,
                why,
            }),
        }
    }
    read.report.raised.sort_by_key(|r| r.entity);
    Ok(Import {
        read,
        built,
        unbuilt,
    })
}

/// The shells of each plan shell: the shell on the side its faces point out to, and the one
/// on the other side.
struct ShellPair {
    out: ShellId,
    inside: ShellId,
    /// The shell whose volume sign must be positive (a solid's), or `None` for a sheet.
    signed: Option<ShellId>,
}

impl Builder<'_> {
    fn vertex(&mut self, id: u64) -> Result<(VertexId, Point3)> {
        if let Some(&v) = self.vertex_at.get(&id) {
            return Ok(v);
        }
        let e = get(self.ex, id)?;
        let p = e
            .record("VERTEX_POINT")
            .ok_or_else(|| wrong(e, id, "VERTEX_POINT"))?;
        let at = reference(id, p, 1)?;
        let q = self.reader.point(at).map_err(|x| x.within(id))?;
        let point = Point3::new(q[0], q[1], q[2]);
        let v = self.draft.add_vertex(VertexRecord::new(point))?;
        self.vertices.push((id, v, point));
        self.vertex_at.insert(id, (v, point));
        Ok((v, point))
    }

    fn foot(curve: &Curve, p: Point3, entity: u64) -> Result<f64> {
        curve
            .invert(p, None)
            .map(|i| i.foot().at)
            .map_err(|e| BuildError::Geometry {
                entity,
                cause: GeomCause::Curve(e),
            })
    }

    fn edge(&mut self, id: u64) -> Result<(EdgeId, bool)> {
        if let Some(&e) = self.edge_at.get(&id) {
            return Ok(e);
        }
        let e = get(self.ex, id)?;
        let p = e
            .record("EDGE_CURVE")
            .ok_or_else(|| wrong(e, id, "EDGE_CURVE"))?;
        let (v1, v2, g) = (
            reference(id, p, 1)?,
            reference(id, p, 2)?,
            reference(id, p, 3)?,
        );
        let same = boolean(id, p, 4)?;
        let (a, b) = (self.vertex(v1)?, self.vertex(v2)?);
        let read = self.reader.curve(g).map_err(|x| x.within(id))?;
        let curve = geom::curve(&read).map_err(|n| made(g, n))?;
        let (mut s, mut t) = if same { (a, b) } else { (b, a) };
        let mut flipped = !same;
        let mut t0 = Self::foot(&curve, s.1, g)?;
        let mut t1 = Self::foot(&curve, t.1, g)?;
        if let Some(period) = curve.domain().period() {
            t1 = if s.0 == t.0 {
                t0 + period
            } else {
                t0 + (t1 - t0).rem_euclid(period)
            };
        } else if t1 < t0 {
            core::mem::swap(&mut s, &mut t);
            core::mem::swap(&mut t0, &mut t1);
            flipped = !flipped;
            self.notes.push(Note::TurnedEdge { edge: id });
        }
        let interval = Extent1d::new(t0, t1)
            .filter(|_| t1 > t0)
            .ok_or(BuildError::Degenerate { edge: id })?;
        let ends = [s.0, t.0];
        let eid = self.draft.add_edge(EdgeGeom::new(
            EdgeShape::Curve {
                curve: curve.clone(),
                interval,
            },
            ends,
        ))?;
        self.edges.push(EdgeIn {
            entity: id,
            id: eid,
            curve,
            interval,
            ends,
        });
        self.edge_at.insert(id, (eid, flipped));
        Ok((eid, flipped))
    }

    /// A face's surface and loops, in the file's senses about the face's normal.
    fn face(&mut self, id: u64, surface: u64, same_sense: bool, shell: usize) -> Result<FaceIn> {
        let e = get(self.ex, id)?;
        let p = e
            .record("ADVANCED_FACE")
            .or_else(|| e.record("FACE_SURFACE"))
            .ok_or_else(|| wrong(e, id, "a face with a surface"))?;
        let bounds = refs(id, p, 1)?;
        let read = self.reader.surface(surface).map_err(|x| x.within(id))?;
        let surf = geom::surface(&read).map_err(|n| made(surface, n))?;
        let mut outer = None;
        let mut loops = Vec::with_capacity(bounds.len());
        for bid in bounds {
            let be = get(self.ex, bid)?;
            let (bp, is_outer) = if let Some(bp) = be.record("FACE_OUTER_BOUND") {
                (bp, true)
            } else if let Some(bp) = be.record("FACE_BOUND") {
                (bp, false)
            } else {
                return Err(wrong(be, bid, "a face bound").within(id).into());
            };
            let lid = reference(bid, bp, 1)?;
            let orientation = boolean(bid, bp, 2)?;
            let le = get(self.ex, lid)?;
            if le.record("VERTEX_LOOP").is_some() {
                return Err(BuildError::Pending {
                    entity: id,
                    waits: Waits::VertexLoop,
                });
            }
            let lp = le
                .record("EDGE_LOOP")
                .ok_or_else(|| wrong(le, lid, "EDGE_LOOP"))?;
            let mut uses = Vec::new();
            for oid in refs(lid, lp, 1)? {
                let oe = get(self.ex, oid)?;
                let op = oe
                    .record("ORIENTED_EDGE")
                    .ok_or_else(|| wrong(oe, oid, "ORIENTED_EDGE"))?;
                let ec = reference(oid, op, 3)?;
                let along = boolean(oid, op, 4)?;
                // The edge runs from the `EDGE_CURVE`'s second vertex to its first when
                // `flipped`.
                let (edge, flipped) = self.edge(ec)?;
                uses.push((edge, sense(along != flipped)));
            }
            if !orientation {
                uses = reversed(&uses);
            }
            if is_outer && outer.is_none() {
                outer = Some(loops.len());
            }
            loops.push(uses);
        }
        if let Some(o) = outer {
            let first = loops.remove(o);
            loops.insert(0, first);
        }
        let bounded = outer.is_some() || matches!(surf, Surface::Plane(_));
        Ok(FaceIn {
            entity: id,
            shell,
            surface: surf,
            natural: same_sense,
            bounded,
            marked: outer.is_some(),
            loops,
            id: None,
        })
    }

    /// The gap's upper bound, or `None` within the zone.
    fn gap(
        zone: f64,
        entity: u64,
        kind: GapKind,
        mut run: impl FnMut(Aim) -> core::result::Result<Bracket, GapError>,
    ) -> Result<Option<f64>> {
        let fail = |e: GapError| match e {
            GapError::Undecided { reached, .. } => BuildError::Undecided {
                entity,
                gap: kind,
                reached,
            },
            GapError::Pending(_) => BuildError::Pending {
                entity,
                waits: Waits::NurbsGap,
            },
            cause => BuildError::Gap { entity, cause },
        };
        let first = run(Aim::Against(zone)).map_err(fail)?;
        if first.exceeds(zone) == Some(false) {
            return Ok(None);
        }
        let b = run(Aim::Width(0.5 * zone)).map_err(fail)?;
        Ok(Some(b.upper().max(first.lower())))
    }

    /// Records `upper` as `entity`'s accuracy, and the raise where it is past `allowance`.
    fn record(
        &mut self,
        entity: u64,
        kind: GapKind,
        upper: f64,
        allowance: f64,
        zone: f64,
        claim: Claim,
    ) -> Option<Arc<Recorded>> {
        if upper > allowance {
            self.raised.push(Raise {
                entity,
                gap: kind,
                before: allowance,
                after: upper,
            });
        }
        let ratio = upper / zone;
        if ratio > MAX_ACCURACY_RATIO {
            self.notes.push(Note::PastRatio { entity, ratio });
        }
        let bound = Length::from_mm(upper);
        Accuracy::new(bound).map(|acc| {
            Arc::new(Recorded {
                acc,
                basis: Basis::Certified(Cert {
                    claim,
                    bound,
                    method: Method::Interval,
                    reads: Reads::NONE,
                }),
            })
        })
    }

    /// Each edge against each face that uses it, each vertex against each edge end.
    fn gaps(&mut self, budget: &mut Budget, stop: &Stop) -> Result<()> {
        let zone = self.doc.resolution().zone().mm();
        let allowance = match self.body.context.uncertainty {
            Uncertainty::Stated(u) => u.max(zone),
            Uncertainty::Absent => zone,
        };
        let mut on: BTreeMap<EdgeId, Vec<usize>> = BTreeMap::new();
        for (fi, f) in self.faces.iter().enumerate() {
            for &(e, _) in f.loops.iter().flatten() {
                let faces = on.entry(e).or_default();
                if !faces.contains(&fi) {
                    faces.push(fi);
                }
            }
        }
        let edges: Vec<(u64, EdgeId, Curve, Extent1d)> = self
            .edges
            .iter()
            .map(|e| (e.entity, e.id, e.curve.clone(), e.interval))
            .collect();
        for (entity, id, curve, interval) in edges {
            let mut worst: Option<f64> = None;
            let surfaces = on
                .get(&id)
                .into_iter()
                .flatten()
                .filter_map(|&fi| self.faces.get(fi).map(|f| &f.surface));
            for s in surfaces {
                let g = Self::gap(zone, entity, GapKind::EdgeToFace, |aim| {
                    curve_surface_gap(&curve, &interval, s, aim, budget, stop)
                })?;
                worst = match (worst, g) {
                    (Some(w), Some(g)) => Some(w.max(g)),
                    (w, g) => w.or(g),
                };
            }
            if let Some(g) = worst {
                let rec = self.record(
                    entity,
                    GapKind::EdgeToFace,
                    g,
                    allowance,
                    zone,
                    Claim::CurveToIntersection,
                );
                if let Some(edge) = self.draft.edge_mut(id) {
                    edge.accuracy = rec;
                }
            }
        }
        // Each vertex's entity and point, and each edge end at it.
        let at: BTreeMap<VertexId, (u64, Point3)> =
            self.vertices.iter().map(|&(s, v, p)| (v, (s, p))).collect();
        let mut ends: Vec<(VertexId, u64, Point3, Curve, f64, EdgeEnd)> = Vec::new();
        for e in &self.edges {
            let [v0, v1] = e.ends;
            let both = [
                (v0, e.interval.min(), EdgeEnd::Start),
                (v1, e.interval.max(), EdgeEnd::End),
            ];
            for (v, t, end) in both {
                if let Some(&(entity, p)) = at.get(&v) {
                    ends.push((v, entity, p, e.curve.clone(), t, end));
                }
            }
        }
        // The widest gap past the zone at each vertex, with the end that has it.
        let mut worst: BTreeMap<VertexId, (u64, f64, EdgeEnd)> = BTreeMap::new();
        for (v, entity, p, curve, t, end) in ends {
            let g = Self::gap(zone, entity, GapKind::VertexToEdge, |_| {
                vertex_curve_gap(p, &curve, t)
            })?;
            if let Some(g) = g
                && worst.get(&v).is_none_or(|&(_, w, _)| g > w)
            {
                worst.insert(v, (entity, g, end));
            }
        }
        for (v, (entity, g, end)) in worst {
            let rec = self.record(
                entity,
                GapKind::VertexToEdge,
                g,
                allowance,
                zone,
                Claim::EndsAtVertex(end),
            );
            if let Some(vx) = self.draft.vertex_mut(v) {
                vx.accuracy = rec;
            }
        }
        Ok(())
    }

    /// The shells of the body's regions, one pair per plan shell.
    fn shells(&mut self) -> Result<Vec<ShellPair>> {
        let mut out = Vec::with_capacity(self.body.shells.len());
        match self.body.kind {
            SolidOrSheet::Solid => {
                let outside = self.draft.add_shell(ShellRecord::new(RegionId::OUTSIDE))?;
                let solid = self.draft.add_region(true)?;
                for plan in &self.body.shells {
                    out.push(if plan.role == ShellRole::Void {
                        let void = self.draft.add_region(false)?;
                        let v = self.draft.add_shell(ShellRecord::new(void))?;
                        let lining = self.draft.add_shell(ShellRecord::new(solid))?;
                        ShellPair {
                            out: v,
                            inside: lining,
                            signed: Some(v),
                        }
                    } else {
                        let m = self.draft.add_shell(ShellRecord::new(solid))?;
                        ShellPair {
                            out: outside,
                            inside: m,
                            signed: Some(m),
                        }
                    });
                }
            }
            SolidOrSheet::Sheet => {
                for _ in &self.body.shells {
                    let s = self.draft.add_shell(ShellRecord::new(RegionId::OUTSIDE))?;
                    out.push(ShellPair {
                        out: s,
                        inside: s,
                        signed: None,
                    });
                }
            }
        }
        Ok(out)
    }

    /// The loop records of `loops` (about the face's normal) about the surface's natural
    /// normal, adding a collapsed edge at each joint at a pole (and where the writer finds
    /// one the distance test missed).
    fn records(
        draft: &mut Draft,
        doc: Document,
        ends: &BTreeMap<EdgeId, [VertexId; 2]>,
        face: &mut FaceIn,
    ) -> Result<Vec<LoopRecord>> {
        // A collapsed edge at every joint at a pole: the pole's segment in uv is where the
        // loop meets the pole's line, a whole turn when the uses either side leave it at
        // the same `u` (a face bounded by its seam alone), which no gap would show.
        for lp in 0..face.loops.len() {
            while let Some((at, v)) = Self::pole_joint(draft, doc, ends, face, lp) {
                let c = draft.add_edge(EdgeGeom::new(EdgeShape::Collapsed, [v, v]))?;
                if let Some(l) = face.loops.get_mut(lp) {
                    l.insert(at, (c, Sense::Forward));
                }
            }
        }
        loop {
            let about: Vec<Vec<(EdgeId, Sense)>> = if face.natural {
                face.loops.clone()
            } else {
                face.loops.iter().map(|l| reversed(l)).collect()
            };
            match draft.exact_loops(&face.surface, &about, &doc) {
                Ok(r) => return Ok(r),
                Err(PcurveWriteError::PoleNeedsEdge { lp, at }) => {
                    // The vertex between use `at - 1` and use `at` of the loop as written,
                    // which is the same joint of the file's loop.
                    let (lp_in, at_in) = if face.natural {
                        (lp, at)
                    } else {
                        let n = face.loops.get(lp).map_or(0, Vec::len);
                        (lp, (n - at) % n.max(1))
                    };
                    let uses = face.loops.get(lp_in).ok_or(BuildError::Pcurve {
                        face: face.entity,
                        cause: PcurveWriteError::PoleNeedsEdge { lp, at },
                    })?;
                    let &(e, s) =
                        uses.get(at_in % uses.len().max(1))
                            .ok_or(BuildError::Pcurve {
                                face: face.entity,
                                cause: PcurveWriteError::PoleNeedsEdge { lp, at },
                            })?;
                    let v = ends
                        .get(&e)
                        .map(|v| if s == Sense::Forward { v[0] } else { v[1] })
                        .ok_or(BuildError::Pcurve {
                            face: face.entity,
                            cause: PcurveWriteError::PoleNeedsEdge { lp, at },
                        })?;
                    let c = draft.add_edge(EdgeGeom::new(EdgeShape::Collapsed, [v, v]))?;
                    if let Some(l) = face.loops.get_mut(lp_in) {
                        l.insert(at_in, (c, Sense::Forward));
                    }
                }
                Err(cause) => {
                    return Err(match cause {
                        PcurveWriteError::Winds { .. } => BuildError::Pending {
                            entity: face.entity,
                            waits: Waits::Seamless,
                        },
                        PcurveWriteError::NeedsFit { .. } => BuildError::Pending {
                            entity: face.entity,
                            waits: Waits::PcurveFit,
                        },
                        cause => BuildError::Pcurve {
                            face: face.entity,
                            cause,
                        },
                    });
                }
            }
        }
    }

    /// The first joint of loop `lp` (index of the use after it, and its vertex) at a pole of
    /// the face's surface with no collapsed edge on either side.
    fn pole_joint(
        draft: &Draft,
        doc: Document,
        ends: &BTreeMap<EdgeId, [VertexId; 2]>,
        face: &FaceIn,
        lp: usize,
    ) -> Option<(usize, VertexId)> {
        let poles: Vec<Point3> = match &face.surface {
            Surface::Sphere(s) => s.poles().iter().map(|p| p.point).collect(),
            Surface::Cone(c) => vec![c.pole().point],
            _ => return None,
        };
        let uses = face.loops.get(lp)?;
        let collapsed = |e: EdgeId| !ends.contains_key(&e);
        let zone = doc.resolution().zone().mm();
        let n = uses.len();
        (0..n).find_map(|at| {
            let &(e, s) = uses.get(at)?;
            let &(before, _) = uses.get((at + n - 1) % n)?;
            if collapsed(e) || collapsed(before) {
                return None;
            }
            let [a, z] = *ends.get(&e)?;
            let v = if s == Sense::Forward { a } else { z };
            let vx = draft.vertex(v)?;
            let reach = zone + vx.accuracy().map_or(0.0, |r| r.acc.length().mm());
            poles
                .iter()
                .any(|&p| (vx.point() - p).length() <= reach)
                .then_some((at, v))
        })
    }

    /// Writes every face, turning one whose loops prove it inside out.
    fn write_faces(&mut self, shells: &[ShellPair]) -> Result<()> {
        let ends: BTreeMap<EdgeId, [VertexId; 2]> =
            self.edges.iter().map(|e| (e.id, e.ends)).collect();
        for f in &mut self.faces {
            let pair = shells
                .get(f.shell)
                .ok_or(BuildError::NotOrientable { shell: 0 })?;
            let sides = [pair.out, pair.inside];
            let mut records = Self::records(&mut self.draft, *self.doc, &ends, f)?;
            if f.bounded
                && FaceRecord::new(f.surface.clone(), sides, records.clone()).is_bounded_region()
                    == Some(false)
            {
                f.natural = !f.natural;
                records = Self::records(&mut self.draft, *self.doc, &ends, f)?;
                self.notes.push(Note::TurnedFace { face: f.entity });
            }
            if !f.marked && records.len() > 1 {
                // No bound is marked outer: on a plane the outer loop is the one that alone
                // bounds a region.
                let outer = records.iter().position(|r| {
                    FaceRecord::new(f.surface.clone(), sides, vec![r.clone()]).is_bounded_region()
                        == Some(true)
                });
                if let Some(o) = outer.filter(|&o| o > 0) {
                    let first = records.remove(o);
                    records.insert(0, first);
                }
            }
            f.id = Some(
                self.draft
                    .add_face(FaceRecord::new(f.surface.clone(), sides, records))?,
            );
        }
        Ok(())
    }

    /// Parity per shell, then each face's sides.
    fn orient(&mut self, shells: &[ShellPair]) -> Result<()> {
        // Each edge's uses: (face, forward about the face's normal).
        let mut uses: BTreeMap<EdgeId, Vec<(usize, bool)>> = BTreeMap::new();
        for (fi, f) in self.faces.iter().enumerate() {
            for &(e, s) in f.loops.iter().flatten() {
                uses.entry(e).or_default().push((fi, s == Sense::Forward));
            }
        }
        if let Some(e) = self
            .edges
            .iter()
            .find(|e| uses.get(&e.id).is_some_and(|u| u.len() > 2))
        {
            return Err(BuildError::Pending {
                entity: e.entity,
                waits: Waits::NonManifold,
            });
        }
        let shell_of: Vec<usize> = self.faces.iter().map(|f| f.shell).collect();
        let mut next: BTreeMap<usize, Vec<(usize, bool)>> = BTreeMap::new();
        for u in uses.values() {
            if let [(f, a), (g, b)] = u[..]
                && f != g
                && shell_of.get(f) == shell_of.get(g)
            {
                // Consistent when the two uses run opposite ways; otherwise one face turns.
                let turn = a == b;
                next.entry(f).or_default().push((g, turn));
                next.entry(g).or_default().push((f, turn));
            }
        }
        let mut turned: BTreeMap<usize, bool> = BTreeMap::new();
        for (start, &k) in shell_of.iter().enumerate() {
            if turned.contains_key(&start) {
                continue;
            }
            turned.insert(start, false);
            let mut queue = std::collections::VecDeque::from([start]);
            while let Some(f) = queue.pop_front() {
                let Some(&tf) = turned.get(&f) else { continue };
                for &(g, turn) in next.get(&f).into_iter().flatten() {
                    let want = tf != turn;
                    match turned.get(&g) {
                        None => {
                            turned.insert(g, want);
                            queue.push_back(g);
                        }
                        Some(&t) if t != want => {
                            return Err(BuildError::NotOrientable {
                                shell: self.shell_entity(k),
                            });
                        }
                        Some(_) => {}
                    }
                }
            }
        }
        for (fi, f) in self.faces.iter().enumerate() {
            let (Some(plan), Some(pair)) = (self.body.shells.get(f.shell), shells.get(f.shell))
            else {
                continue;
            };
            let Some(&flip) = turned.get(&fi) else {
                continue;
            };
            let out_front = (f.natural != flip) != plan.reversed;
            let sides = if out_front {
                [pair.out, pair.inside]
            } else {
                [pair.inside, pair.out]
            };
            if let Some(r) = f.id.and_then(|id| self.draft.face_mut(id)) {
                r.sides = sides;
            }
        }
        Ok(())
    }

    /// The file entity of the plan's `k`th shell.
    fn shell_entity(&self, k: usize) -> u64 {
        self.body.shells.get(k).map_or(0, |s| s.entity)
    }

    /// Finishes the draft and turns each solid shell whose volume sign is negative.
    fn finish(&mut self, shells: &[ShellPair], budget: &mut Budget, stop: &Stop) -> Result<Brep> {
        let draft = core::mem::take(&mut self.draft);
        let (body, _) = draft
            .finish(self.doc)
            .map_err(|u| BuildError::Unfinished(u.cause))?;
        let mut turn = Vec::new();
        for (k, pair) in shells.iter().enumerate() {
            let Some(shell) = pair.signed else { continue };
            let entity = self.shell_entity(k);
            match volume_sign(&body, shell, budget, stop) {
                Ok(Sign::Positive) => {}
                Ok(Sign::Negative) => turn.push(k),
                Ok(Sign::Zero) => return Err(BuildError::Flat { shell: entity }),
                Err(cause) => {
                    return Err(BuildError::Mass {
                        shell: entity,
                        cause,
                    });
                }
            }
        }
        if turn.is_empty() {
            return Ok(body);
        }
        let mut draft = body.into_draft();
        for &k in &turn {
            for f in self.faces.iter().filter(|f| f.shell == k) {
                if let Some(r) = f.id.and_then(|id| draft.face_mut(id)) {
                    r.sides = [r.side(FaceSide::Back), r.side(FaceSide::Front)];
                }
            }
            let shell = self.shell_entity(k);
            self.notes.push(Note::TurnedShell { shell });
        }
        let (body, _) = draft
            .finish(self.doc)
            .map_err(|u| BuildError::Unfinished(u.cause))?;
        Ok(body)
    }
}
