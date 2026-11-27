// Written for solidcorex. Apache-2.0.
//! The body record (research 63 RE3): a [`Brep`]'s tables by id, each record's constructor
//! inputs and nothing derived, read back through a [`Draft`] and [`Draft::finish`].
//!
//! - **What is stored.** Every table in kind order: regions (from id 1: the outside is the
//!   one every [`Draft::new`] starts with), shells, faces, edges, vertices. Ids are kept,
//!   deleted slots as runs ([`Sink::slots`]). A shell is its region, wires and lone
//!   vertices; a face its surface, its two sides and its loops of uses; an edge its shape,
//!   ends, accuracy and fins in radial order; a vertex its point and accuracy. Each pcurve
//!   is stored exactly as written (63 D10), so a decoded body never refits.
//! - **What is not.** Cells, which are rebuilt on demand; the face list of a shell and the
//!   edges of a vertex, which views find; the resolution, which is the reader's
//!   [`Document`]'s, as at every `finish`.
//! - **Reading checks everything.** The tables are read whole, the input must end there,
//!   and then the draft is finished. A body `finish` refuses is
//!   [`ReadCause::Invalid`] with its cause, so every decoded body is a valid one, and a
//!   dangling id or a loop off its edges keeps its typed cause. No record can name a
//!   pending curve, so bytes never reach the `MissingCurve` defect (63 D8).
//!
//! The fins are stored, not left to [`Draft::add_face`]: their order is the radial order, a
//! constructor input the faces do not fix (BR10 accepts any cyclic shift, and the sides
//! decide only where the sectors differ). The set is checked by `finish` against the uses
//! ([`StructureCause::UseWithoutFin`], [`StructureCause::FinWithoutUse`],
//! [`StructureCause::RepeatedFin`]).
//!
//! [`StructureCause::UseWithoutFin`]: scx_brep::StructureCause::UseWithoutFin
//! [`StructureCause::FinWithoutUse`]: scx_brep::StructureCause::FinWithoutUse
//! [`StructureCause::RepeatedFin`]: scx_brep::StructureCause::RepeatedFin

use std::sync::Arc;

use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, EntityKind, FaceId, FaceRecord,
    Fin, LoopRecord, RegionId, ShellId, ShellRecord, TableFull, UseGeom, VertexId, VertexRecord,
};
use scx_curve::{Curve, Curve2};
use scx_num::cert::{Basis, Cert, Claim, EdgeEnd, MAX_READS, Method, Reads, Recorded};
use scx_num::extent1d::Extent1d;
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_surface::Surface;

use crate::binary::{Reader, Writer};
use crate::error::{ReadCause, ReadError, WriteError};
use crate::geom::{Record, put_all, put_sense, take_sense};
use crate::tag::tags;
use crate::text::{TextReader, TextWriter};
use crate::wire::{Magic, Sink, Source};

/// The magic of a body record: `SCXB`.
pub const BODY: Magic = Magic(*b"SCXB");

/// How much a body record may make its reader allocate beyond its own bytes.
///
/// A live record takes bytes on the wire, but a run of deleted slots is a few bytes for any
/// number of slots, each of which the decoded body holds. So each table's length is capped:
/// over the cap is [`ReadCause::TooMany`], kind `Exhausted`, before anything is allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// The most slots in any one table, live and deleted, the outside region counted.
    pub slots: u32,
}

impl Limits {
    /// The default cap: 2^20 slots per table, which bounds a decode's memory past its input
    /// to about 0.3 GB at worst (every table a run of deleted slots).
    pub const DEFAULT: Self = Self { slots: 1 << 20 };

    /// The cap of `slots` per table.
    #[must_use]
    pub const fn new(slots: u32) -> Self {
        Self { slots }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The body's binary record.
///
/// # Errors
///
/// [`WriteError`] when a value cannot be written: a certificate bound that is not finite
/// ([`WriteCause::NonFinite`](crate::WriteCause::NonFinite): [`Cert::bound`] is not checked
/// at construction), a geometry kind with no record yet
/// ([`WriteCause::NoRecord`](crate::WriteCause::NoRecord)), or a rational NURBS value
/// neither form keeps ([`WriteCause::MixedRational`](crate::WriteCause::MixedRational)).
pub fn encode(body: &Brep) -> Result<Vec<u8>, WriteError> {
    let mut w = Writer::new();
    put_body(body, &mut w)?;
    Ok(w.finish())
}

/// The body's text record: the binary record's fields, one word each.
///
/// # Errors
///
/// As [`encode`].
pub fn write_text(body: &Brep) -> Result<String, WriteError> {
    let mut w = TextWriter::new();
    put_body(body, &mut w)?;
    Ok(w.finish())
}

/// The body a binary record holds, finished in `doc`.
///
/// # Errors
///
/// [`ReadError`] when the bytes are not a body record, when a value does not build (its
/// constructor's cause), when a table is over `limits`, or when the body does not finish
/// ([`ReadCause::Invalid`], with the [`FinishCause`](scx_brep::FinishCause)).
pub fn decode(bytes: &[u8], doc: &Document, limits: Limits) -> Result<Brep, ReadError> {
    take_body(Reader::new(bytes), bytes.len(), *doc, limits)
}

/// The body a text record holds, finished in `doc`.
///
/// # Errors
///
/// As [`decode`].
pub fn read_text(text: &str, doc: &Document, limits: Limits) -> Result<Brep, ReadError> {
    take_body(TextReader::new(text), text.len(), *doc, limits)
}

// ---- tags --------------------------------------------------------------------------------

tags! {
    /// [`EdgeShape`] on the wire.
    enum ShapeKind: "edge_shape" {
        Curve = 0, "curve";
        Collapsed = 1, "collapsed";
    }
}

tags! {
    /// [`UseGeom`] on the wire.
    enum UseKind: "use_geom" {
        Pcurve = 0, "pcurve";
        Point = 1, "point";
    }
}

tags! {
    /// [`Basis`] on the wire.
    enum BasisKind: "basis" {
        Measured = 0, "measured";
        Certified = 1, "certified";
        Unproven = 2, "unproven";
        Stated = 3, "stated";
    }
}

tags! {
    /// [`Claim`] on the wire, the edge end folded in.
    enum ClaimKind: "claim" {
        PcurveOnSurface = 0, "pcurve_on_surface";
        CurveToIntersection = 1, "curve_to_intersection";
        NetToDefinition = 2, "net_to_definition";
        EndsAtStart = 3, "ends_at_start";
        EndsAtEnd = 4, "ends_at_end";
        Outward = 5, "outward";
    }
}

tags! {
    /// [`Method`] on the wire.
    enum MethodKind: "method" {
        Interval = 0, "interval";
        Bernstein = 1, "bernstein";
        Taylor = 2, "taylor";
        Krawczyk = 3, "krawczyk";
    }
}

// ---- writing -----------------------------------------------------------------------------

/// The ids of a table of `slots` slots.
fn ids(slots: usize) -> core::ops::Range<u32> {
    0..u32::try_from(slots).unwrap_or(u32::MAX)
}

fn put_body<S: Sink>(b: &Brep, w: &mut S) -> Result<(), WriteError> {
    w.header(BODY);
    // Regions from id 1: the outside is the draft's own, void by construction.
    let regions = ids(b.region_slots()).skip(1).map(|i| {
        b.region(RegionId::from_raw(i))
            .map(scx_brep::Region::record)
    });
    w.slots(regions, |w, r| {
        w.bool(r.solid);
        Ok(())
    })?;
    let shells =
        ids(b.shell_slots()).map(|i| b.shell(ShellId::from_raw(i)).map(scx_brep::Shell::record));
    w.slots(shells, put_shell)?;
    let faces =
        ids(b.face_slots()).map(|i| b.face(FaceId::from_raw(i)).map(scx_brep::Face::record));
    w.slots(faces, put_face)?;
    let edges = ids(b.edge_slots()).map(|i| b.edge(EdgeId::from_raw(i)));
    w.slots(
        edges.collect::<Vec<_>>().iter().map(Option::as_ref),
        |w, e| put_edge(w, e.geometry(), &e.adjacency().fins),
    )?;
    let vertices = ids(b.vertex_slots()).map(|i| {
        b.vertex(VertexId::from_raw(i))
            .map(scx_brep::Vertex::record)
    });
    w.slots(vertices, |w, v| {
        put_point3(w, v.point)?;
        put_recorded(w, v.accuracy.as_deref())
    })
}

fn put_ids<S: Sink>(w: &mut S, ids: impl ExactSizeIterator<Item = u32>) -> Result<(), WriteError> {
    w.count(ids.len())?;
    for id in ids {
        w.u32(id);
    }
    Ok(())
}

fn put_shell<S: Sink>(w: &mut S, s: &ShellRecord) -> Result<(), WriteError> {
    w.u32(s.region.raw());
    put_ids(w, s.wires.iter().map(|e| e.raw()))?;
    put_ids(w, s.lone_vertices.iter().map(|v| v.raw()))
}

fn put_face<S: Sink>(w: &mut S, f: &FaceRecord) -> Result<(), WriteError> {
    f.surface.put(w)?;
    w.u32(f.sides[0].raw());
    w.u32(f.sides[1].raw());
    w.list(&f.loops, |w, lp| {
        w.line();
        w.list(&lp.uses, |w, u| {
            w.line();
            put_use(w, u)
        })
    })
}

fn put_use<S: Sink>(w: &mut S, u: &EdgeUse) -> Result<(), WriteError> {
    w.u32(u.edge.raw());
    put_sense(w, u.sense);
    match &u.geom {
        UseGeom::Pcurve { curve, interval } => {
            w.tag(UseKind::Pcurve);
            curve.put(w)?;
            put_interval(w, *interval)?;
        }
        UseGeom::Point(p) => {
            w.tag(UseKind::Point);
            put_all(w, &[p.x, p.y])?;
        }
    }
    put_recorded(w, u.bound.as_deref())
}

fn put_edge<S: Sink>(w: &mut S, g: &EdgeGeom, fins: &[Fin]) -> Result<(), WriteError> {
    match &g.shape {
        EdgeShape::Curve { curve, interval } => {
            w.tag(ShapeKind::Curve);
            curve.put(w)?;
            put_interval(w, *interval)?;
        }
        EdgeShape::Collapsed => w.tag(ShapeKind::Collapsed),
    }
    w.u32(g.ends[0].raw());
    w.u32(g.ends[1].raw());
    put_recorded(w, g.accuracy.as_deref())?;
    w.list(fins, |w, fin| {
        w.u32(fin.face.raw());
        put_sense(w, fin.sense);
        Ok(())
    })
}

fn put_point3<S: Sink>(w: &mut S, p: Point3) -> Result<(), WriteError> {
    put_all(w, &[p.x, p.y, p.z])
}

fn put_interval<S: Sink>(w: &mut S, e: Extent1d) -> Result<(), WriteError> {
    put_all(w, &[e.min(), e.max()])
}

fn put_recorded<S: Sink>(w: &mut S, rec: Option<&Recorded>) -> Result<(), WriteError> {
    let Some(rec) = rec else {
        w.bool(false);
        return Ok(());
    };
    w.bool(true);
    w.f64(rec.acc.length().mm())?;
    match rec.basis {
        Basis::Measured => w.tag(BasisKind::Measured),
        Basis::Stated => w.tag(BasisKind::Stated),
        Basis::Certified(c) => {
            w.tag(BasisKind::Certified);
            put_cert(w, &c)?;
        }
        Basis::Unproven(c) => {
            w.tag(BasisKind::Unproven);
            put_cert(w, &c)?;
        }
    }
    Ok(())
}

fn put_cert<S: Sink>(w: &mut S, c: &Cert) -> Result<(), WriteError> {
    w.tag(match c.claim {
        Claim::PcurveOnSurface => ClaimKind::PcurveOnSurface,
        Claim::CurveToIntersection => ClaimKind::CurveToIntersection,
        Claim::NetToDefinition => ClaimKind::NetToDefinition,
        Claim::EndsAtVertex(EdgeEnd::Start) => ClaimKind::EndsAtStart,
        Claim::EndsAtVertex(EdgeEnd::End) => ClaimKind::EndsAtEnd,
        Claim::Outward => ClaimKind::Outward,
    });
    w.f64(c.bound.mm())?;
    w.tag(match c.method {
        Method::Interval => MethodKind::Interval,
        Method::Bernstein => MethodKind::Bernstein,
        Method::Taylor => MethodKind::Taylor,
        Method::Krawczyk => MethodKind::Krawczyk,
    });
    w.list(c.reads.keys(), |w, k| {
        w.u64(*k);
        Ok(())
    })
}

// ---- reading -----------------------------------------------------------------------------

/// A live edge as read: its geometry and its fins.
type EdgeEntry = (EdgeGeom, Vec<Fin>);

/// `end` is the input's length: where a refusal of the body as a whole is placed.
fn take_body<R: Source>(
    mut r: R,
    end: usize,
    doc: Document,
    limits: Limits,
) -> Result<Brep, ReadError> {
    // Every version `header` accepts (1 to FORMAT_VERSION = 1) is this layout; a second
    // version gets its own arm here, at the record boundary (63 §7.2).
    let _version = r.header(BODY)?;
    let regions = r.slots(limits.slots.saturating_sub(1), Source::bool)?;
    let shells = r.slots(limits.slots, take_shell)?;
    let faces = r.slots(limits.slots, take_face)?;
    let edges = r.slots(limits.slots, take_edge)?;
    let vertices = r.slots(limits.slots, take_vertex)?;
    r.finish()?;

    let full = |_: TableFull| {
        ReadError::new(
            end,
            ReadCause::TooMany {
                count: u32::MAX,
                limit: limits.slots,
            },
        )
    };
    let mut d = Draft::new();
    fill(&mut d, EntityKind::Region, regions, |d, solid| {
        d.add_region(solid).map(drop)
    })
    .map_err(full)?;
    fill(&mut d, EntityKind::Shell, shells, |d, s| {
        d.add_shell(s).map(drop)
    })
    .map_err(full)?;
    fill(&mut d, EntityKind::Face, faces, |d, f| {
        d.add_face(f).map(drop)
    })
    .map_err(full)?;
    fill(&mut d, EntityKind::Edge, edges, |d, (g, fins)| {
        let id = d.add_edge(g)?;
        // The stored radial order replaces whatever the faces appended.
        if let Some(adj) = d.adjacency_mut(id) {
            adj.fins = fins;
        }
        Ok(())
    })
    .map_err(full)?;
    fill(&mut d, EntityKind::Vertex, vertices, |d, v| {
        d.add_vertex(v).map(drop)
    })
    .map_err(full)?;
    match d.finish(&doc) {
        Ok((body, _)) => Ok(body),
        Err(unfinished) => Err(ReadError::new(
            end,
            ReadCause::Invalid(Box::new(unfinished.cause)),
        )),
    }
}

/// Adds a table's records to `d` in id order, its deleted slots as skipped ids.
fn fill<T>(
    d: &mut Draft,
    kind: EntityKind,
    slots: Vec<Option<T>>,
    mut add: impl FnMut(&mut Draft, T) -> Result<(), TableFull>,
) -> Result<(), TableFull> {
    let mut run = 0u32;
    for slot in slots {
        match slot {
            None => run += 1,
            Some(record) => {
                d.skip(kind, run)?;
                run = 0;
                add(d, record)?;
            }
        }
    }
    d.skip(kind, run)
}

fn take_ids<R: Source, I>(r: &mut R, id: impl Fn(u32) -> I) -> Result<Vec<I>, ReadError> {
    r.list(|r| r.u32().map(&id))
}

fn take_shell<R: Source>(r: &mut R) -> Result<ShellRecord, ReadError> {
    let mut s = ShellRecord::new(RegionId::from_raw(r.u32()?));
    s.wires = take_ids(r, EdgeId::from_raw)?;
    s.lone_vertices = take_ids(r, VertexId::from_raw)?;
    Ok(s)
}

fn take_face<R: Source>(r: &mut R) -> Result<FaceRecord, ReadError> {
    let surface = Surface::take(r)?;
    let sides = [ShellId::from_raw(r.u32()?), ShellId::from_raw(r.u32()?)];
    let loops = r.list(|r| {
        r.line();
        r.list(|r| {
            r.line();
            take_use(r)
        })
        .map(LoopRecord::new)
    })?;
    Ok(FaceRecord::new(surface, sides, loops))
}

fn take_use<R: Source>(r: &mut R) -> Result<EdgeUse, ReadError> {
    let edge = EdgeId::from_raw(r.u32()?);
    let sense = take_sense(r)?;
    let geom = match r.tag()? {
        UseKind::Pcurve => {
            let curve = Curve2::take(r)?;
            UseGeom::Pcurve {
                curve,
                interval: take_interval(r)?,
            }
        }
        UseKind::Point => UseGeom::Point(Point2::new(r.f64()?, r.f64()?)),
    };
    let mut u = EdgeUse::new(edge, sense, geom);
    u.bound = take_recorded(r)?;
    Ok(u)
}

fn take_edge<R: Source>(r: &mut R) -> Result<EdgeEntry, ReadError> {
    let shape = match r.tag()? {
        ShapeKind::Curve => {
            let curve = Curve::take(r)?;
            EdgeShape::Curve {
                curve,
                interval: take_interval(r)?,
            }
        }
        ShapeKind::Collapsed => EdgeShape::Collapsed,
    };
    let ends = [VertexId::from_raw(r.u32()?), VertexId::from_raw(r.u32()?)];
    let mut g = EdgeGeom::new(shape, ends);
    g.accuracy = take_recorded(r)?;
    let fins = r.list(|r| {
        let face = FaceId::from_raw(r.u32()?);
        Ok(Fin::new(face, take_sense(r)?))
    })?;
    Ok((g, fins))
}

fn take_vertex<R: Source>(r: &mut R) -> Result<VertexRecord, ReadError> {
    let mut v = VertexRecord::new(Point3::new(r.f64()?, r.f64()?, r.f64()?));
    v.accuracy = take_recorded(r)?;
    Ok(v)
}

fn take_interval<R: Source>(r: &mut R) -> Result<Extent1d, ReadError> {
    let at = r.at();
    let (min, max) = (r.f64()?, r.f64()?);
    Extent1d::new(min, max).ok_or_else(|| ReadError::new(at, ReadCause::BadInterval))
}

fn take_recorded<R: Source>(r: &mut R) -> Result<Option<Arc<Recorded>>, ReadError> {
    if !r.bool()? {
        return Ok(None);
    }
    let at = r.at();
    let bound = r.f64()?;
    // `Accuracy::new` reads -0 as 0, which would write back as 0: a second spelling.
    if bound.to_bits() == (-0.0f64).to_bits() {
        return Err(ReadError::new(
            at,
            ReadCause::NonCanonical { record: "accuracy" },
        ));
    }
    let acc = Accuracy::new(Length::from_mm(bound))
        .ok_or_else(|| ReadError::new(at, ReadCause::BadAccuracy))?;
    let basis = match r.tag()? {
        BasisKind::Measured => Basis::Measured,
        BasisKind::Stated => Basis::Stated,
        BasisKind::Certified => Basis::Certified(take_cert(r)?),
        BasisKind::Unproven => Basis::Unproven(take_cert(r)?),
    };
    Ok(Some(Arc::new(Recorded { acc, basis })))
}

fn take_cert<R: Source>(r: &mut R) -> Result<Cert, ReadError> {
    let claim = match r.tag()? {
        ClaimKind::PcurveOnSurface => Claim::PcurveOnSurface,
        ClaimKind::CurveToIntersection => Claim::CurveToIntersection,
        ClaimKind::NetToDefinition => Claim::NetToDefinition,
        ClaimKind::EndsAtStart => Claim::EndsAtVertex(EdgeEnd::Start),
        ClaimKind::EndsAtEnd => Claim::EndsAtVertex(EdgeEnd::End),
        ClaimKind::Outward => Claim::Outward,
    };
    let bound = Length::from_mm(r.f64()?);
    let method = match r.tag()? {
        MethodKind::Interval => Method::Interval,
        MethodKind::Bernstein => Method::Bernstein,
        MethodKind::Taylor => Method::Taylor,
        MethodKind::Krawczyk => Method::Krawczyk,
    };
    let at = r.at();
    let count = r.u32()?;
    if count as usize > MAX_READS {
        return Err(ReadError::new(at, ReadCause::BadReads { count }));
    }
    let mut keys = [0u64; MAX_READS];
    let keys = keys.get_mut(..count as usize).unwrap_or_default();
    for k in keys.iter_mut() {
        *k = r.u64()?;
    }
    let reads =
        Reads::new(keys).ok_or_else(|| ReadError::new(at, ReadCause::BadReads { count }))?;
    Ok(Cert {
        claim,
        bound,
        method,
        reads,
    })
}
