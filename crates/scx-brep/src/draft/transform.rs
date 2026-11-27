// Written for solidcorex. Apache-2.0.
//! A body moved by an affine map (research 63 §6.5 BR6, §6.12): [`Draft::transform`].
//!
//! Each record is mapped by its kind's own edit: a vertex's point by the map, an edge's
//! curve by `Curve::transformed` (CU5) and a face's surface by `Surface::transformed`
//! (SU7). The topology is kept: the same ids, loops, uses and senses. What follows the
//! map:
//!
//! - **Parameters.** An edge's interval moves by its curve's [`ParamShift`]. A use's
//!   pcurve is kept where its surface's [`UvMap`] is the identity and its edge's shift is
//!   none (a sphere, a torus or a NURBS face under any map it takes, a curve that keeps its
//!   parameter): `S'(p(t)) = A S(p(t)) = A C(t) = C'(t)`. Otherwise a line in uv (every
//!   planar line's and every ruling's pcurve, and a pole's segment) is mapped by the
//!   `UvMap`, an affine map of uv that keeps the line a line and its parameter; any other
//!   pcurve is made again by `pcurve_of` on the new curve and surface and moved by whole
//!   periods to start where the `UvMap` takes the old one's start, so the loop keeps its
//!   period window.
//! - **Orientation.** A map whose determinant is negative (a mirror) turns the surface's
//!   natural normal to the image of the old back side: `S_u' x S_v'` is `det(A) A^-T`
//!   times `S_u x S_v` over the `UvMap`'s determinant, positive for every kind. So each
//!   face's two sides swap, and each edge's radial order reverses (counter-clockwise about
//!   the direction becomes clockwise). The loops, use senses and pcurves stay: the uv
//!   orientation is the `UvMap`'s, which keeps it. This is the pin's mirror (the faceuse
//!   shells swap and the radial order of the edge uses reverses).
//! - **Accuracy.** A distance grows by at most the map's largest scale: for a similarity
//!   its scale, otherwise the Frobenius norm of its linear part (an upper bound). Every
//!   recorded bound is that times its reach (the recorded one, or the resolution for
//!   `None`), kept as `None` at or under the resolution. A certificate read the old
//!   geometry, so under any map but the identity it is kept as `Unproven`, scaled.
//!
//! A surface of revolution under a map that is not a similarity is refused by name
//! (`map_not_similar`, [`ErrorKind::Unsupported`]) until its refit exists (BR9).
//!
//! The transform is all or nothing: every new record is made first and written only when
//! all are, through the draft's writes, so each face's and edge's cells are reset and
//! [`Draft::finish`] reports every moved record as `replaced` with `Touch::Geometry`.

use core::fmt;
use std::sync::Arc;

use scx_curve::arcs::Arcs;
use scx_curve::{
    AngleAxis, AngleLine2, AngleWave2, Circle2, Curve2, CurveError, Ellipse2, Hyperbola2, Line2,
    ParamShift,
};
use scx_num::cert::{Basis, Recorded};
use scx_num::domain::UvDomain;
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::math::Affine;
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::Vec3;
use scx_surface::{PcurveError, Surface, SurfaceError, UvMap, pcurve_of};

use super::Draft;
use crate::document::Document;
use crate::id::{EdgeId, Entity, FaceId};
use crate::record::{EdgeShape, EdgeUse, FaceRecord, LoopRecord, UseGeom};

/// Why [`Draft::transform`] refused. The draft is unchanged.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TransformError {
    /// The map has an entry that is not finite.
    MapNotFinite,
    /// The map is not proved regular: it may flatten a direction.
    MapSingular,
    /// An edge's curve is not attached yet.
    Pending {
        /// The edge.
        edge: EdgeId,
    },
    /// A face's surface refused the map: a surface of revolution under a map that is not a
    /// similarity is [`SurfaceError::NotSimilar`], unsupported until its refit.
    Surface {
        /// The face.
        face: FaceId,
        /// The surface's refusal.
        cause: SurfaceError,
    },
    /// An edge's curve refused the map.
    Curve {
        /// The edge.
        edge: EdgeId,
        /// The curve's refusal.
        cause: CurveError,
    },
    /// A use's pcurve could not be made on the moved face.
    Pcurve {
        /// The face.
        face: FaceId,
        /// The use's edge.
        edge: EdgeId,
        /// Why `pcurve_of` refused.
        cause: PcurveError,
    },
    /// A use's pcurve is of a kind that cannot follow the face's parameters: a pole's
    /// segment that is not a line, or a pcurve that must move by whole periods and is
    /// neither a line nor an angle line.
    PcurveKind {
        /// The face.
        face: FaceId,
        /// The use's edge.
        edge: EdgeId,
    },
    /// A face's use names an edge the draft does not hold (a dangling reference, which
    /// `finish` would also refuse).
    MissingEdge {
        /// The face.
        face: FaceId,
        /// The missing edge.
        edge: EdgeId,
    },
    /// A point, an interval or an accuracy of the entity overflows under the map.
    Overflow {
        /// The entity.
        entity: Entity,
    },
}

impl TransformError {
    /// The kind: the surface's, curve's or pcurve's own for theirs (so a surface of
    /// revolution under a map that is not a similarity is [`ErrorKind::Unsupported`]),
    /// [`ErrorKind::Unsupported`] for a pcurve kind, and [`ErrorKind::Refused`] otherwise.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Surface { cause, .. } => cause.kind(),
            Self::Curve { cause, .. } => cause.kind(),
            Self::Pcurve { cause, .. } => cause.kind(),
            Self::PcurveKind { .. } => ErrorKind::Unsupported,
            Self::MapNotFinite
            | Self::MapSingular
            | Self::Pending { .. }
            | Self::MissingEdge { .. }
            | Self::Overflow { .. } => ErrorKind::Refused,
        }
    }

    /// The stable name: the cause's own where there is one (`map_not_similar` for a
    /// surface of revolution under a map that is not a similarity).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::MapNotFinite => "map_not_finite",
            Self::MapSingular => "map_singular",
            Self::Pending { .. } => "edge_pending",
            Self::Surface { cause, .. } => cause.name(),
            Self::Curve { cause, .. } => cause.name(),
            Self::Pcurve { cause, .. } => cause.name(),
            Self::PcurveKind { .. } => "pcurve_kind_unmapped",
            Self::MissingEdge { .. } => "use_edge_missing",
            Self::Overflow { .. } => "transform_overflow",
        }
    }
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MapNotFinite => f.write_str("transform: the map is not finite"),
            Self::MapSingular => f.write_str("transform: the map is not proved regular"),
            Self::Pending { edge } => write!(f, "transform: edge {edge} has no curve yet"),
            Self::Surface { face, cause } => write!(f, "transform: face {face}: {cause}"),
            Self::Curve { edge, cause } => write!(f, "transform: edge {edge}: {cause}"),
            Self::Pcurve { face, edge, cause } => {
                write!(f, "transform: face {face}, use of edge {edge}: {cause}")
            }
            Self::PcurveKind { face, edge } => write!(
                f,
                "transform: face {face}, use of edge {edge}: this pcurve kind cannot follow \
                 the face's parameters"
            ),
            Self::MissingEdge { face, edge } => {
                write!(
                    f,
                    "transform: face {face} uses edge {edge}, which is not in the draft"
                )
            }
            Self::Overflow { entity } => write!(f, "transform: {entity} overflows"),
        }
    }
}

impl core::error::Error for TransformError {}

type Result<T> = core::result::Result<T, TransformError>;

/// The roundings within which the map is taken as a similarity, as the curve and surface
/// edits take it (research 63 §3.9).
const SIMILAR: f64 = 16.0 * f64::EPSILON;

/// How far the map can stretch a distance: the scale of a similarity (one within
/// [`SIMILAR`] of one, so a rigid motion leaves every bound as it was), and otherwise the
/// Frobenius norm of the linear part, which is at least its largest singular value.
fn stretch(a: &Affine) -> f64 {
    let axes = [a.x_axis(), a.y_axis(), a.z_axis()];
    let similar = axes.iter().enumerate().all(|(i, p)| {
        axes.iter().skip(i + 1).all(|q| {
            let (pp, qq, pq) = (p.dot(*p), q.dot(*q), p.dot(*q));
            (pp - qq).abs() <= SIMILAR * (pp + qq) && pq.abs() <= SIMILAR * (pp + qq)
        })
    });
    if similar {
        let k = axes.iter().map(|v| v.length()).sum::<f64>() / 3.0;
        if (k - 1.0).abs() <= SIMILAR { 1.0 } else { k }
    } else {
        axes.iter().map(|v| v.dot(*v)).sum::<f64>().sqrt()
    }
}

/// The sign of the linear part's determinant, for a map proved regular.
fn mirrors(a: &Affine) -> bool {
    let (x, y, z): (Vec3, Vec3, Vec3) = (a.x_axis(), a.y_axis(), a.z_axis());
    x.dot(y.cross(z)) < 0.0
}

/// The recorded bounds after the map.
struct Bounds {
    grow: f64,
    zone: Length,
    /// `None`'s image where the map stretches: one record shared by every entity.
    widened_zone: Option<Arc<Recorded>>,
}

impl Bounds {
    fn new(grow: f64, zone: Length) -> Option<Self> {
        let widened = zone * grow;
        let widened_zone = if grow > 1.0 {
            Some(Arc::new(Recorded {
                acc: Accuracy::new(widened)?,
                basis: Basis::Measured,
            }))
        } else {
            None
        };
        Some(Self {
            grow,
            zone,
            widened_zone,
        })
    }

    /// `rec`'s image: `None` where it is within the resolution; `Overflow` naming `entity`
    /// where the scaled bound is not finite.
    fn map(&self, rec: Option<&Arc<Recorded>>, entity: Entity) -> Result<Option<Arc<Recorded>>> {
        let Some(rec) = rec else {
            return Ok(self.widened_zone.clone());
        };
        let certified = matches!(rec.basis, Basis::Certified(_));
        if self.grow == 1.0 && !certified {
            return Ok(Some(Arc::clone(rec)));
        }
        let acc = rec.acc.length() * self.grow;
        if acc.mm() <= self.zone.mm() {
            return Ok(None);
        }
        let scaled = |mut c: scx_num::cert::Cert| {
            c.bound = c.bound * self.grow;
            c
        };
        let basis = match rec.basis {
            Basis::Certified(c) | Basis::Unproven(c) => Basis::Unproven(scaled(c)),
            b => b,
        };
        let acc = Accuracy::new(acc).ok_or(TransformError::Overflow { entity })?;
        Ok(Some(Arc::new(Recorded { acc, basis })))
    }

    /// The length a bound reaches: the recorded one, or the resolution.
    fn reach(&self, rec: Option<&Arc<Recorded>>) -> Length {
        rec.map_or(self.zone, |r| r.acc.length())
    }
}

/// An edge's new geometry part, and how its parameter moved.
struct EdgeImage {
    id: EdgeId,
    shape: EdgeShape,
    accuracy: Option<Arc<Recorded>>,
    shift: ParamShift,
}

impl Draft {
    /// Moves the whole body by `a`, in place: every vertex, edge and face mapped by its
    /// kind's edit, the topology kept, and under a mirror each face's sides swapped and each
    /// edge's radial order reversed (module docs). Recorded bounds scale with the map, the
    /// resolution taken from `doc` for those within it.
    ///
    /// The identity writes nothing. A [`Draft::finish`] after it reports every record the
    /// map moved as replaced with [`Touch::Geometry`](crate::Touch::Geometry).
    ///
    /// # Errors
    ///
    /// [`TransformError`], leaving the draft unchanged: a map not finite or not proved
    /// regular, an edge with no curve, and a record whose edit refuses the map, among them
    /// a surface of revolution under a map that is not a similarity (`map_not_similar`,
    /// unsupported until its refit).
    pub fn transform(&mut self, a: &Affine, doc: &Document) -> Result<()> {
        if *a == Affine::IDENTITY {
            return Ok(());
        }
        if !a.is_finite() {
            return Err(TransformError::MapNotFinite);
        }
        if !a.is_regular(0.0).is_yes() {
            return Err(TransformError::MapSingular);
        }
        if let Some(&edge) = self.pending.first() {
            return Err(TransformError::Pending { edge });
        }
        let mirror = mirrors(a);
        let zone = doc.resolution().zone();
        let bounds = Bounds::new(stretch(a), zone).ok_or(TransformError::MapNotFinite)?;

        // Make every new record first: nothing is written unless all are made.
        let vertices = self
            .t
            .vertices
            .iter()
            .map(|(id, v)| {
                let overflow = || TransformError::Overflow { entity: id.into() };
                let point = a.apply(v.point);
                if !point.is_finite() {
                    return Err(overflow());
                }
                let accuracy = bounds.map(v.accuracy.as_ref(), id.into())?;
                Ok((id, point, accuracy))
            })
            .collect::<Result<Vec<_>>>()?;
        let edges = self
            .t
            .edges
            .iter()
            .map(|(id, g)| {
                let overflow = || TransformError::Overflow { entity: id.into() };
                let (shape, shift) = match &g.shape {
                    EdgeShape::Curve { curve, interval } => {
                        let (curve, shift) = curve
                            .transformed(a)
                            .map_err(|cause| TransformError::Curve { edge: id, cause })?;
                        let interval = shift.extent(*interval).ok_or_else(overflow)?;
                        (EdgeShape::Curve { curve, interval }, shift)
                    }
                    EdgeShape::Collapsed => (EdgeShape::Collapsed, ParamShift::NONE),
                };
                let accuracy = bounds.map(g.accuracy.as_ref(), id.into())?;
                Ok(EdgeImage {
                    id,
                    shape,
                    accuracy,
                    shift,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let edge_image = |id: EdgeId| {
            edges
                .binary_search_by_key(&id, |e| e.id)
                .ok()
                .and_then(|i| edges.get(i))
        };
        let faces = self
            .t
            .faces
            .iter()
            .map(|(id, f)| {
                let (surface, uv) = f
                    .surface
                    .transformed(a)
                    .map_err(|cause| TransformError::Surface { face: id, cause })?;
                let [[p, q], [r, s]] = uv.matrix();
                let swap = mirror != (p * s - q * r < 0.0);
                let loops = f
                    .loops
                    .iter()
                    .map(|l| {
                        let uses = l
                            .uses
                            .iter()
                            .map(|u| {
                                let image =
                                    edge_image(u.edge).ok_or(TransformError::MissingEdge {
                                        face: id,
                                        edge: u.edge,
                                    })?;
                                let bound = bounds.map(u.bound.as_ref(), u.edge.into())?;
                                // The pcurve is made to the larger of the use's and
                                // the edge's bounds: either can part it from the surface.
                                let reach = bounds
                                    .reach(bound.as_ref())
                                    .max(bounds.reach(image.accuracy.as_ref()));
                                // `pcurve_of` reads the approximation tolerance, half the zone.
                                // A recorded bound is above the zone, so the fallback to the
                                // document's resolution never fires.
                                let res = Resolution::new(reach * 2.0)
                                    .unwrap_or_else(|| doc.resolution());
                                let geom = use_image(id, u, image, &surface, &uv, res)?;
                                let mut out = EdgeUse::new(u.edge, u.sense, geom);
                                out.bound = bound;
                                Ok(out)
                            })
                            .collect::<Result<Vec<_>>>()?;
                        Ok(LoopRecord::new(uses))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let sides = if swap {
                    [f.sides[1], f.sides[0]]
                } else {
                    f.sides
                };
                Ok((id, FaceRecord::new(surface, sides, loops)))
            })
            .collect::<Result<Vec<_>>>()?;

        // Write them, through the writes that log each record and reset its cells.
        for (id, point, accuracy) in vertices {
            if let Some(v) = self.vertex_mut(id) {
                v.point = point;
                v.accuracy = accuracy;
            }
        }
        for e in edges {
            if let Some(g) = self.edge_mut(e.id) {
                g.shape = e.shape;
                g.accuracy = e.accuracy;
            }
            if mirror {
                self.touched.fins(&self.t, e.id);
            }
            if mirror && let Some(adj) = self.t.adjacency.get_mut(e.id) {
                adj.fins.reverse();
            }
        }
        for (id, record) in faces {
            self.put_face(id, record);
        }
        Ok(())
    }

    /// Puts `record` in the face's place as [`Draft::face_mut`] writes it, logged, with fresh
    /// cells, but without copying the record it replaces.
    fn put_face(&mut self, id: FaceId, record: FaceRecord) {
        self.touched.face(&self.t, id);
        if let Some(face) = self.t.faces.get_mut(id) {
            self.log.face_written(id, face);
            *face = Arc::new(record);
        }
    }
}

/// A use's image in its moved face's uv (module docs, Parameters).
fn use_image(
    face: FaceId,
    u: &EdgeUse,
    edge: &EdgeImage,
    surface: &Surface,
    uv: &UvMap,
    res: Resolution,
) -> Result<UseGeom> {
    let kind = || TransformError::PcurveKind { face, edge: u.edge };
    let (curve, interval) = match &u.geom {
        UseGeom::Point(p) => return Ok(UseGeom::Point(uv.apply(*p))),
        UseGeom::Pcurve { curve, interval } => (curve, *interval),
    };
    let unshifted = edge.shift == ParamShift::NONE;
    if unshifted && near_identity(uv) {
        return Ok(u.geom.clone());
    }
    if unshifted && let Curve2::Line(l) = curve {
        let [[p, q], [r, s]] = uv.matrix();
        let d = l.direction();
        let line = Line2::new(
            uv.apply(l.origin()),
            Vec2::new(p * d.x + q * d.y, r * d.x + s * d.y),
        )
        .map_err(|cause| TransformError::Curve {
            edge: u.edge,
            cause,
        })?;
        return Ok(UseGeom::Pcurve {
            curve: Curve2::Line(line),
            interval,
        });
    }
    if unshifted && let Some(mapped) = mapped(curve, uv, &surface.domain()) {
        return Ok(UseGeom::Pcurve {
            curve: mapped,
            interval,
        });
    }
    let EdgeShape::Curve {
        curve: moved,
        interval: over,
    } = &edge.shape
    else {
        return Err(kind());
    };
    let made = pcurve_of(moved, surface, *over, res).map_err(|cause| TransformError::Pcurve {
        face,
        edge: u.edge,
        cause,
    })?;
    let at = uv.apply(start(curve, interval, u.sense));
    let made = aligned(made, *over, u.sense, at, &surface.domain()).ok_or_else(kind)?;
    Ok(UseGeom::Pcurve {
        curve: made,
        interval: *over,
    })
}

/// `uv` applied to `curve` where the image is of the same kind with the same parameter
/// (FU1, 63 §6.12 open question 2), for an edge whose parameter the map kept: then the image
/// pcurve is `uv ∘ p`, exactly as for a line, and needs no `pcurve_of`.
///
/// - A circle, an ellipse or a hyperbola's branch under a uniform scale `(k, k)`, `k > 0`
///   (a plane under a similarity): the centre mapped, the semi-axes times `k`, the axes and
///   sense kept.
/// - An angle line or an angle wave under a diagonal map (a cylinder's `(k, k)`, a cone's
///   `(1, k)`): the angle coordinate keeps its angle, on the new surface's arcs, when its
///   scale is the ratio of the new period to the old to [`SIMILAR`] and it has no offset; the
///   other coordinate is mapped as a line's.
///
/// `None` for anything else (a parabola, whose parameter a scale stretches, so its edge is
/// shifted; a NURBS pcurve; a map with a shear), which `pcurve_of` makes again.
fn mapped(curve: &Curve2, uv: &UvMap, domain: &UvDomain) -> Option<Curve2> {
    let [[p, q], [r, s]] = uv.matrix();
    if q != 0.0 || r != 0.0 {
        return None;
    }
    let [cu, cv] = uv.offset();
    let uniform = p == s && p > 0.0;
    match curve {
        Curve2::Circle(c) if uniform => {
            let frame = c.frame().moved_to(uv.apply(c.frame().origin()))?;
            Circle2::new(frame, c.radius() * p, c.sense())
                .ok()
                .map(Curve2::Circle)
        }
        Curve2::Ellipse(e) if uniform => {
            let frame = e.frame().moved_to(uv.apply(e.frame().origin()))?;
            Ellipse2::new(frame, e.semi_x() * p, e.semi_y() * p, e.sense())
                .ok()
                .map(Curve2::Ellipse)
        }
        Curve2::Hyperbola(h) if uniform => {
            let frame = h.frame().moved_to(uv.apply(h.frame().origin()))?;
            Hyperbola2::new(frame, h.semi_x() * p, h.semi_y() * p, h.sense())
                .ok()
                .map(Curve2::Hyperbola)
        }
        Curve2::AngleLine(l) => {
            let (scale, shift, other, at, period) = match l.axis() {
                AngleAxis::U => (p, cu, s, cv, domain.u.period()),
                AngleAxis::V => (s, cv, p, cu, domain.v.period()),
            };
            let arcs = rescaled(l.arcs(), scale, shift, period)?;
            AngleLine2::new(
                arcs,
                l.axis(),
                l.start(),
                l.sense(),
                other * l.other() + at,
                other * l.rate(),
            )
            .ok()
            .map(Curve2::AngleLine)
        }
        Curve2::AngleWave(w) => {
            let arcs = rescaled(w.arcs(), p, cu, domain.u.period())?;
            let (ac, asin) = w.amplitudes();
            AngleWave2::new(
                arcs,
                w.start(),
                w.sense(),
                s * w.mean() + cv,
                s * ac,
                s * asin,
            )
            .ok()
            .map(Curve2::AngleWave)
        }
        _ => None,
    }
}

/// The arcs of the new surface's angle direction, where the map scales that coordinate by
/// `scale` with no offset, and that is the ratio of its new period to the old (a turn), or
/// one (the sphere's half turn, which has no period).
fn rescaled(arcs: Arcs, scale: f64, shift: f64, period: Option<f64>) -> Option<Arcs> {
    if shift != 0.0 {
        return None;
    }
    if !arcs.is_turn() {
        return (scale == 1.0 && period.is_none()).then_some(arcs);
    }
    let period = period?;
    ((scale * arcs.length() - period).abs() <= SIMILAR * period).then(|| Arcs::turn(period))
}

/// Whether `uv` is the identity to [`SIMILAR`]: a rigid motion's, whose scale is the mean
/// of lengths each within rounding of one. The pcurve kept under it is off by at most
/// `16 eps |uv|`, under the rounding of evaluating the surface there; made again by
/// `pcurve_of` it would be no nearer, at twenty times the cost.
fn near_identity(uv: &UvMap) -> bool {
    let [[p, q], [r, s]] = uv.matrix();
    let near = |x: f64, to: f64| (x - to).abs() <= SIMILAR;
    near(p, 1.0) && near(s, 1.0) && near(q, 0.0) && near(r, 0.0) && uv.offset() == [0.0, 0.0]
}

/// Where a use in `sense` over `interval` starts on `curve`.
fn start(curve: &Curve2, interval: Extent1d, sense: Sense) -> Point2 {
    match sense {
        Sense::Forward => curve.point(interval.min()),
        Sense::Reversed => curve.point(interval.max()),
    }
}

/// `curve` moved by whole periods of `domain` so that a use in `sense` over `interval`
/// starts at `at`; `None` for a kind that cannot move.
fn aligned(
    curve: Curve2,
    interval: Extent1d,
    sense: Sense,
    at: Point2,
    domain: &UvDomain,
) -> Option<Curve2> {
    let gap = at - start(&curve, interval, sense);
    let whole = |g: f64, p: Option<f64>| p.map_or(0.0, |p| (g / p).round());
    let (pu, pv) = (domain.u.period(), domain.v.period());
    let m = [whole(gap.x, pu), whole(gap.y, pv)];
    if m == [0.0, 0.0] {
        return Some(curve);
    }
    let d = Vec2::new(m[0] * pu.unwrap_or(0.0), m[1] * pv.unwrap_or(0.0));
    match curve {
        Curve2::Line(l) => Line2::new(l.origin() + d, l.direction())
            .ok()
            .map(Curve2::Line),
        Curve2::AngleLine(l) => {
            // The angle coordinate moves by whole turns, the other by the period itself.
            let (turns, other) = match l.axis() {
                AngleAxis::U => (m[0], d.y),
                AngleAxis::V => (m[1], d.x),
            };
            AngleLine2::new(
                l.arcs(),
                l.axis(),
                l.start() + turns * core::f64::consts::TAU,
                l.sense(),
                l.other() + other,
                l.rate(),
            )
            .ok()
            .map(Curve2::AngleLine)
        }
        // The chart's periods are the domain's.
        Curve2::OnChart(o) => o.shifted(m).ok().map(Curve2::from),
        _ => None,
    }
}
