// Written for solidcorex. Apache-2.0.
//! [`section`] and [`face_section`]: where a surface meets a body (BO14; 75 §2.6, §7).
//!
//! **One pipeline (R1).** The cutting plane is made a solid operand: the half-space behind it,
//! cut down to a block whose one face on the plane reaches past the body's box on every side
//! and whose other five faces lie clear of it. Stage 1 pairs only that face with the body's
//! faces, so nothing else of the block is read past its boxes.
//!
//! - **A solid's section** is [`combine`] with no cell solid and the faces kept where the
//!   body's material lies on either side of them, a piece of the plane's face or a face of the
//!   body lying in the plane: the closure of the plane's meet with the body, as a sheet (void
//!   on both sides, its edges free) of the body's document. Where the plane only touches the
//!   body (along an edge or a line of a curved face, or at a point) and meets no material,
//!   the closure is lower-dimensional: the wire and points stages 1 to 4 found, as for a
//!   sheet. A plane that both crosses the material and touches it apart from the sheet is
//!   refused as [`SectionUnsupported::TouchBesideSheet`] (a sheet with wires and points
//!   beside it is not built yet).
//! - **A sheet's section** is the wire stages 1 to 4 find ([`intersect`](crate::intersect)'s
//!   body): every curve where the plane crosses a face, every edge lying in the plane, every
//!   point where it touches.
//! - **[`face_section`]** is the second scoped to one face of either: its pairs only, so the
//!   other faces are read only where they share an edge or a vertex with it.
//!
//! The body is operand 0 of the result's lineage and the plane's block operand 1; the
//! block's face on the plane is its only face that can be an origin.

use scx_brep::op::{Cx, Done, Report};
use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, FaceId, FaceRecord, FaceSide, RegionId,
    ShellRecord, VertexId, VertexRecord,
};
use scx_curve::{Curve, Inversion, Line};
use scx_ssi::analytic::Contact;
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Plane, Surface, SurfaceKind};

use crate::boolean::{NoItems, interfere_only, staged};
use crate::cells::{Faces, Membership, Rule};
use crate::error::{BooleanDefect, BooleanError, BooleanRefusal, BooleanUnsupported};
use crate::faceface::{FaceFace, face_face};
use crate::given::Incidences;
use crate::geom::{Operands, Side};
use crate::intersect::wire;

/// Why a section is not answered yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SectionUnsupported {
    /// The cutting surface is not a plane: a section by a curved surface waits on a tool
    /// body bounded by it (its kind given).
    Surface(SurfaceKind),
    /// The body has no faces: a wire's section is points, which stage 1's face pairs do not
    /// find.
    Wire,
    /// The body has both solid regions and faces with void on both sides.
    Mixed,
    /// A face of a sheet lies in the plane: its section is the face, not a wire.
    InPlane(FaceId),
    /// The plane crosses a solid's material and also touches it (along a curve or at a
    /// point) apart from the section's sheet: the sheet with wires and points beside it is
    /// not built yet.
    TouchBesideSheet,
}

/// Why a section is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SectionRefusal {
    /// The face is not the body's.
    NoFace(FaceId),
}

/// What a body is made of, for a section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Solid,
    Sheet,
}

/// The section of `body` by the surface `by`, built in `body.edit()` (75 §2.6): for a
/// solid, the sheet where the plane meets its material, or the wire and points where it only
/// touches it; for a sheet, the wire where the plane crosses its faces and the points where
/// it touches them. A plane that misses the body gives a body of nothing.
///
/// The sheet's faces lie on the plane, each void on both sides with its edges free; its
/// edges lie on the body's faces. The wire's edges are as [`intersect`](crate::intersect)
/// writes them.
///
/// # Errors
///
/// - [`BooleanRefusal::OtherDocument`] for a body of another resolution than `cx.doc`'s.
/// - [`SectionUnsupported`] (as [`BooleanUnsupported::Section`]): a surface that is not a
///   plane, a body with no faces or of solids and sheets both, a sheet's face in the plane, a
///   solid the plane both crosses and touches apart from the sheet.
/// - The causes of [`combine`] for a solid and of [`intersect`](crate::intersect) for a
///   sheet: NURBS faces ([`BooleanUnsupported::Nurbs`]), pairs with no closed form, the
///   budget, the stop flag, defects.
pub fn section(
    body: &Brep,
    by: &Surface,
    cx: &mut Cx<'_>,
) -> Result<Done<NoItems>, BooleanError> {
    let plane = plane_of(by)?;
    let kind = kind(body)?;
    let doc = *cx.doc;
    same_document(body, &doc)?;
    let Some(tool) = tool(body, &plane, &doc)? else {
        return nothing(&doc);
    };
    match kind {
        Kind::Solid => solid(body, &tool, cx),
        Kind::Sheet => sheet(body, &tool, None, cx),
    }
}

/// The section of `face` of `body` by the surface `by`: the wire where the plane crosses
/// the face and the points where it touches it, built as [`section`]'s sheet case over that
/// face's pairs alone. A face of a solid and of a sheet alike give a wire; a plane that
/// misses the face gives a body of nothing.
///
/// # Errors
///
/// [`SectionRefusal::NoFace`] for a face the body does not have; as [`section`] otherwise,
/// [`SectionUnsupported::InPlane`] for a face lying in the plane.
pub fn face_section(
    body: &Brep,
    face: FaceId,
    by: &Surface,
    cx: &mut Cx<'_>,
) -> Result<Done<NoItems>, BooleanError> {
    let plane = plane_of(by)?;
    if body.face(face).is_none() {
        return Err(BooleanError::Refused(BooleanRefusal::Section(
            SectionRefusal::NoFace(face),
        )));
    }
    let doc = *cx.doc;
    same_document(body, &doc)?;
    let Some(tool) = tool(body, &plane, &doc)? else {
        return nothing(&doc);
    };
    sheet(body, &tool, Some(face), cx)
}

const fn unsupported(cause: SectionUnsupported) -> BooleanError {
    BooleanError::Unsupported(BooleanUnsupported::Section(cause))
}

fn plane_of(by: &Surface) -> Result<Plane, BooleanError> {
    match by {
        Surface::Plane(p) => Ok(*p),
        other => Err(unsupported(SectionUnsupported::Surface(SurfaceKind::of(
            other,
        )))),
    }
}

fn same_document(body: &Brep, doc: &Document) -> Result<(), BooleanError> {
    if body.resolution() == doc.resolution() {
        Ok(())
    } else {
        Err(BooleanError::Refused(BooleanRefusal::OtherDocument {
            operand: Side::A.operand(),
        }))
    }
}

/// Whether `body` is solids or sheets.
fn kind(body: &Brep) -> Result<Kind, BooleanError> {
    if body.faces().len() == 0 {
        return Err(unsupported(SectionUnsupported::Wire));
    }
    let solid_side = |f: scx_brep::Face<'_>, side: FaceSide| {
        f.shell(side).region().is_solid()
    };
    let (mut solids, mut sheets) = (false, false);
    for f in body.faces() {
        if solid_side(f, FaceSide::Front) || solid_side(f, FaceSide::Back) {
            solids = true;
        } else {
            sheets = true;
        }
    }
    match (solids, sheets) {
        (true, false) => Ok(Kind::Solid),
        (false, true) => Ok(Kind::Sheet),
        _ => Err(unsupported(SectionUnsupported::Mixed)),
    }
}

/// A body of nothing, of `doc`.
fn nothing(doc: &Document) -> Result<Done<NoItems>, BooleanError> {
    let (body, changes) = Draft::new()
        .finish(doc)
        .map_err(|_| BooleanError::Defect(BooleanDefect::Build))?;
    Ok(Done::new(body, changes, Report::empty(0)))
}

/// A solid's section: [`combine`] keeping the faces with `body`'s material on a side, or,
/// where it keeps none, the wire of what stages 1 to 4 found.
fn solid(body: &Brep, tool: &Brep, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError> {
    let never = |_: Membership| false;
    // A piece between cells `m` and `o` is kept when it lies on the block's face (or is a
    // face of the body coincident with it) and the body is on a side of it: the two cells
    // together are in both operands, and no side is in the block alone or in both at once
    // with the other side in the block (a piece of the body's face inside the block).
    let closure = |m: Membership, o: Membership| {
        m.union(o) == Membership::BOTH && !m.intersection(o).contains(1)
    };
    let rule = Rule {
        solid: &never,
        faces: Faces::Where(&closure),
    };
    let (done, (found, ff)) = staged(body, tool, &rule, &Incidences::NONE, cx)?;
    let ops = Operands { a: body, b: tool };
    if done.body.faces().len() == 0 {
        let (out, changes) = wire(ops, &found, &ff, *cx.doc)?;
        return Ok(Done::new(out, changes, done.report));
    }
    let zone = cx.doc.resolution().zone().mm();
    if beside(ops, &done.body, &ff, zone)? {
        return Err(unsupported(SectionUnsupported::TouchBesideSheet));
    }
    Ok(done)
}

/// Whether stage 4 found a place the plane only touches the body (a tangent curve, an edge
/// lying in the plane, a touch point) that `sheet` holds on none of its edges or vertices
/// within `zone`, judged at the curve's middle or the point.
fn beside(
    ops: Operands<'_>,
    sheet: &Brep,
    ff: &FaceFace,
    zone: f64,
) -> Result<bool, BooleanError> {
    let mut at = Vec::new();
    for s in &ff.sections {
        if s.contact == Contact::Tangent {
            at.push(s.curve.point(s.t.mid()));
        }
    }
    for l in &ff.lying {
        let edge = ops
            .of(l.side)
            .edge(l.edge)
            .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
        if let EdgeShape::Curve { curve, .. } = edge.shape() {
            at.push(curve.point(l.t.mid()));
        }
    }
    at.extend(ff.touches.iter().map(|t| t.at));
    for p in at {
        if !holds(sheet, p, zone) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether `p` lies within `zone` of a vertex or an edge of `sheet`.
fn holds(sheet: &Brep, p: Point3, zone: f64) -> bool {
    sheet.vertices().any(|v| (v.point() - p).length() <= zone)
        || sheet.edges().any(|e| match e.shape() {
            EdgeShape::Curve { curve, interval } => curve
                .invert(p, Some(&interval))
                .is_ok_and(|i| matches!(i, Inversion::Inside(f) if f.distance <= zone)),
            _ => false,
        })
}

/// A sheet's (or, with `only`, one face's) section: the wire stages 1 to 4 find.
fn sheet(
    body: &Brep,
    tool: &Brep,
    only: Option<FaceId>,
    cx: &mut Cx<'_>,
) -> Result<Done<NoItems>, BooleanError> {
    let mut budget = cx.budget();
    let ops = Operands { a: body, b: tool };
    let found = interfere_only(ops, only, &mut budget, cx.stop)?;
    let ff = face_face(ops, &found, &mut budget, cx.stop)?;
    if let Some(c) = ff.coincident.first() {
        return Err(unsupported(SectionUnsupported::InPlane(c.faces[0])));
    }
    let (out, changes) = wire(ops, &found, &ff, *cx.doc)?;
    Ok(Done::new(out, changes, Report::empty(budget.used())))
}

/// The block behind `plane` whose face on it reaches past `body`'s box on every side and
/// whose other faces lie clear of the box, or `None` where the plane misses the box.
fn tool(body: &Brep, plane: &Plane, doc: &Document) -> Result<Option<Brep>, BooleanError> {
    let Some(bx) = body.bounding_box() else {
        return Ok(None);
    };
    let frame = plane.frame();
    let n: Vec3 = frame.z().as_vec();
    let c = bx.mid();
    let h = bx.max_distance_to_point(c);
    let d = (c - frame.origin()).dot(n);
    let zone = doc.resolution().zone().mm();
    if d.abs() > h + zone {
        return Ok(None);
    }
    // Every point of the box lies within `h` of `c`: within `h` of `o` along the plane and
    // within `|d| + h` of it across. Twice those clear the box by `h` at least.
    let o = c - n * d;
    let reach = 2.0 * h.max(zone);
    let depth = 2.0 * (d.abs() + h).max(zone);
    block(o, frame, reach, depth, doc).map(Some)
}

/// One face of the block: its corners (bits `uvw`, set at the far end of that axis),
/// counter-clockwise about the outward normal, and that normal's axis and sign.
struct Role {
    ring: [u8; 4],
    axis: usize,
    outward: f64,
}

/// The block's faces; the first is the one on the plane (`w` at its far end).
const ROLES: [Role; 6] = [
    Role {
        ring: [0b001, 0b101, 0b111, 0b011],
        axis: 2,
        outward: 1.0,
    },
    Role {
        ring: [0b000, 0b010, 0b110, 0b100],
        axis: 2,
        outward: -1.0,
    },
    Role {
        ring: [0b000, 0b100, 0b101, 0b001],
        axis: 1,
        outward: -1.0,
    },
    Role {
        ring: [0b100, 0b110, 0b111, 0b101],
        axis: 0,
        outward: 1.0,
    },
    Role {
        ring: [0b110, 0b010, 0b011, 0b111],
        axis: 1,
        outward: 1.0,
    },
    Role {
        ring: [0b010, 0b000, 0b001, 0b011],
        axis: 0,
        outward: -1.0,
    },
];

fn defect<E>(_: E) -> BooleanError {
    BooleanError::Defect(BooleanDefect::Draft)
}

/// The block `[-reach, reach]² x [-depth, 0]` in the frame at `o` with `frame`'s axes.
fn block(
    o: Point3,
    frame: &Frame,
    reach: f64,
    depth: f64,
    doc: &Document,
) -> Result<Brep, BooleanError> {
    let axes: [Vec3; 3] = [frame.x().as_vec(), frame.y().as_vec(), frame.z().as_vec()];
    let lo = [-reach, -reach, -depth];
    let hi = [reach, reach, 0.0];
    let point = |bits: u8| -> Point3 {
        let mut p = o;
        for (k, ((axis, lo), hi)) in axes.iter().zip(lo).zip(hi).enumerate() {
            let t = if bits >> (2 - k) & 1 == 1 { hi } else { lo };
            p = p + *axis * t;
        }
        p
    };
    let mut draft = Draft::new();
    let outside = draft
        .add_shell(ShellRecord::new(RegionId::OUTSIDE))
        .map_err(defect)?;
    let region = draft.add_region(true).map_err(defect)?;
    let inner = draft.add_shell(ShellRecord::new(region)).map_err(defect)?;
    let mut verts: [Option<VertexId>; 8] = [None; 8];
    let mut lines: Vec<((VertexId, VertexId), EdgeId)> = Vec::new();
    let unit = Extent1d::new(0.0, 1.0).ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
    for role in &ROLES {
        let mut ring = Vec::with_capacity(4);
        for bits in role.ring {
            let cell = verts
                .get_mut(usize::from(bits))
                .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
            ring.push(match cell {
                Some(v) => *v,
                None => {
                    let v = draft
                        .add_vertex(VertexRecord::new(point(bits)))
                        .map_err(defect)?;
                    *cell = Some(v);
                    v
                }
            });
        }
        let mut uses = Vec::with_capacity(4);
        for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            let (lo_v, hi_v, sense) = if a < b {
                (a, b, Sense::Forward)
            } else {
                (b, a, Sense::Reversed)
            };
            let e = if let Some(&(_, e)) = lines.iter().find(|(k, _)| *k == (lo_v, hi_v)) {
                e
            } else {
                let p = draft.vertex(lo_v).map(|v| v.point()).ok_or_else(|| defect(()))?;
                let q = draft.vertex(hi_v).map(|v| v.point()).ok_or_else(|| defect(()))?;
                let line = Line::new(p, q - p).map_err(defect)?;
                let e = draft
                    .add_edge(EdgeGeom::new(
                        EdgeShape::Curve {
                            curve: Curve::Line(line),
                            interval: unit,
                        },
                        [lo_v, hi_v],
                    ))
                    .map_err(defect)?;
                lines.push(((lo_v, hi_v), e));
                e
            };
            uses.push((e, sense));
        }
        let [p0, p1, ..] = role.ring.map(point);
        let normal = *axes.get(role.axis).ok_or_else(|| defect(()))? * role.outward;
        let plane = Frame::new(p0, normal, p1 - p0).ok_or_else(|| defect(()))?;
        let surface = Surface::Plane(Plane::new(plane));
        let loops = draft.exact_loops(&surface, &[uses], doc).map_err(defect)?;
        draft
            .add_face(FaceRecord::new(surface, [outside, inner], loops))
            .map_err(defect)?;
    }
    draft
        .finish(doc)
        .map(|(b, _)| b)
        .map_err(|_| BooleanError::Defect(BooleanDefect::Build))
}
