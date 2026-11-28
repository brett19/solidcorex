// Written for solidcorex. Apache-2.0.
//! Whether a face of a body is a prism's planar cap, and whether a prism cut at a face of a
//! target is kept (research 74 §34, BW11): the cap tests on plain planes, the point and face
//! on a plane, and the section's edges lying on the target's face, each curve on the face's
//! surface and inside its trim, proved.
//!
//! | pin (`geom::plane_height`, ops `cap`) | here |
//! |---|---|
//! | squareness "the cross of the normals under `1e-9`" | one line over the reach ([`Near`]) |
//! | the same way "dot above `1 - 1e-9`" | one line over the reach, and the dot positive |
//! | `edges_lie_on_face`: the surface at sampled points, the trim by classifying each part | [`curve_lies_on_face`]: the surface by the certified gap (`curve_surface_gap`), the trim by the face's own boundary test over enclosing cells of the curve's uv image, or by lying within the tolerance of one of the face's own edges. A curve neither proved on nor off is [`Lies::Undecided`], never taken as on |
//! | `section_tolerance`, `cap_tolerance` per call, "never over a quarter of the distance" | the [`Near`]'s coincidence |
//! | a NURBS target face | [`Unsupported`](BodyqUnsupported::Nurbs) (the gap over a NURBS surface is pending in `scx-surface`) |

use scx_brep::{Brep, EdgeId, EdgeShape, Entity, FaceId, UvSide};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::interval::Ivl;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Aim, Bracket, GapError, Short, curve_curve_gap, curve_surface_gap};
use scx_surface::{Surface, pcurve_of};

use crate::error::{BodyqError, BodyqRefusal, BodyqUnsupported};
use crate::reach::{Near, finite, finite_point, unit3};

/// A plane: a point on it and its normal (any nonzero length).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaneAt {
    /// A point on the plane.
    pub origin: Point3,
    /// Its normal.
    pub normal: Vec3,
}

/// Whether `face` is square to the direction `normal` (either way round) over the reach and
/// lies at `height` along it from `origin`, within coincidence.
///
/// # Errors
///
/// Refused for a value that is not finite, or a normal of no length.
pub fn is_square_plane_at_height(
    face: &PlaneAt,
    origin: Point3,
    normal: Vec3,
    height: Length,
    near: &Near,
) -> Result<bool, BodyqError> {
    finite_point(face.origin)?;
    finite_point(origin)?;
    let height = finite(height.mm())?;
    let (fa, na) = (unit3(face.normal)?, unit3(normal)?);
    Ok(near.one_line(fa, na) && near.within((face.origin - origin).dot(na) - height))
}

/// Whether `point` lies within coincidence of the plane at `height` along the unit direction
/// `normal` from the world origin.
///
/// # Errors
///
/// Refused for a value that is not finite, or a normal of no length.
pub fn is_point_on_plane(
    point: Point3,
    normal: Vec3,
    height: Length,
    near: &Near,
) -> Result<bool, BodyqError> {
    finite_point(point)?;
    let height = finite(height.mm())?;
    let n = unit3(normal)?;
    Ok(near.within(point.to_vec().dot(n) - height))
}

/// Whether a planar face with normal `face_normal`, whose boundary has the corners `corners`,
/// lies on the plane at `height` along `normal` from the world origin: the normals point the
/// same way (one line over the reach, the dot positive) and every corner is on the plane. The
/// normals alone do not settle it on a large face, which is why the corners count.
///
/// # Errors
///
/// Refused for a value that is not finite, a normal of no length, or no corners
/// ([`Corners`](BodyqRefusal::Corners)).
pub fn is_face_on_plane(
    face_normal: Vec3,
    corners: &[Point3],
    normal: Vec3,
    height: Length,
    near: &Near,
) -> Result<bool, BodyqError> {
    if corners.is_empty() {
        return Err(BodyqError::Refused(BodyqRefusal::Corners));
    }
    let (fa, na) = (unit3(face_normal)?, unit3(normal)?);
    let mut all = true;
    for c in corners {
        all &= is_point_on_plane(*c, na, height, near)?;
    }
    Ok(fa.dot(na) > 0.0 && near.one_line(fa, na) && all)
}

/// Whether a curve lies on a face.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Lies {
    /// Proved: on the face's surface within the tolerance, and inside its trim.
    Yes,
    /// Proved off the surface by at least this.
    OffSurface {
        /// A lower bound on the curve's largest distance from the surface.
        by: Length,
    },
    /// Proved to have a stretch wholly outside the face's trim.
    OutsideTrim,
    /// Neither: the enclosures at the cuts the work allowed could not tell.
    Undecided,
}

/// Cells one trim test may halve a piece to: past it the piece is undecided.
const MAX_DEPTH: u32 = 40;

fn of_gap(r: Result<Bracket, GapError>, face: FaceId) -> Result<Option<Bracket>, BodyqError> {
    match r {
        Ok(b) => Ok(Some(b)),
        Err(GapError::Undecided { why, .. }) => match why {
            Short::Exhausted(x) => Err(BodyqError::of_budget(x)),
            Short::Stopped => Err(BodyqError::Cancelled),
            _ => Ok(None),
        },
        Err(GapError::Pending(_)) => Err(BodyqError::Unsupported(BodyqUnsupported::Nurbs {
            at: Entity::Face(face),
        })),
        Err(GapError::NotFinite) => Err(BodyqError::Refused(BodyqRefusal::NotFinite)),
        Err(_) => Err(BodyqError::Refused(BodyqRefusal::Tolerance)),
    }
}

/// Whether `curve` over `over` lies on face `face` of `target`, within `tol`: on its surface
/// (the certified gap) and inside its trim.
///
/// The trim is proved one of two ways. Where the curve is within `tol` of one of the face's
/// own edges all along it, it lies on the face's boundary. Otherwise its uv image is enclosed
/// piece by piece and each piece's cell is read against the face
/// ([`side_of_cell`](scx_brep::Face::side_of_cell)): every cell `Inside` proves it, a cell
/// `Outside` refutes it, and a cell on the boundary is halved until the work or the depth
/// ends, then [`Lies::Undecided`]. Each gap point and each cell charges one unit.
///
/// # Errors
///
/// Refused for a face the target does not have or a `tol` that is negative or not finite.
/// Unsupported for a NURBS target face. Exhausted or cancelled by `budget` and `stop`.
pub fn curve_lies_on_face(
    target: &Brep,
    face: FaceId,
    curve: &Curve,
    over: Extent1d,
    tol: Length,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Lies, BodyqError> {
    let tol = finite(tol.mm())?;
    if tol < 0.0 {
        return Err(BodyqError::Refused(BodyqRefusal::Tolerance));
    }
    let f = target
        .face(face)
        .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: Entity::Face(face),
        }))?;
    let surface = f.surface();
    // A procedural face is refused with the NURBS ones (its gap would be its net's).
    if matches!(surface, Surface::Nurbs(_) | Surface::Procedural(_)) {
        return Err(BodyqError::Unsupported(BodyqUnsupported::Nurbs {
            at: Entity::Face(face),
        }));
    }
    let Some(gap) = of_gap(
        curve_surface_gap(curve, &over, surface, Aim::Against(tol), budget, stop),
        face,
    )?
    else {
        return Ok(Lies::Undecided);
    };
    match gap.exceeds(tol) {
        Some(true) => {
            return Ok(Lies::OffSurface {
                by: Length::from_mm(gap.lower()),
            });
        }
        Some(false) => {}
        None => return Ok(Lies::Undecided),
    }
    // On the face's boundary: within `tol` of one of its own edges all along.
    for u in f.uses() {
        stop.check()?;
        let edge = u.edge();
        let EdgeShape::Curve {
            curve: ec,
            interval,
        } = edge.shape()
        else {
            continue;
        };
        let g = of_gap(
            curve_curve_gap(curve, &over, ec, interval, Aim::Against(tol), budget, stop),
            face,
        )?;
        if g.is_some_and(|g| g.exceeds(tol) == Some(false)) {
            return Ok(Lies::Yes);
        }
    }
    // Inside the trim: the uv image of the curve in cells.
    let Ok(pcurve) = pcurve_of(curve, surface, over, target.resolution()) else {
        return Ok(Lies::Undecided);
    };
    let mut stack = vec![(over.min(), over.max(), 0_u32)];
    let mut undecided = false;
    while let Some((lo, hi, depth)) = stack.pop() {
        budget.charge(1)?;
        stop.check()?;
        let p = pcurve.point_over(Ivl::new(lo, hi));
        let Some(cell) = Extent2d::from_bounds(p.x.lo, p.y.lo, p.x.hi, p.y.hi) else {
            undecided = true;
            continue;
        };
        match f.side_of_cell(cell) {
            Some(UvSide::Inside) => {}
            Some(UvSide::Outside) => return Ok(Lies::OutsideTrim),
            Some(UvSide::Near) | None => {
                let mid = 0.5 * (lo + hi);
                if depth >= MAX_DEPTH || !(lo < mid && mid < hi) {
                    undecided = true;
                } else {
                    stack.push((mid, hi, depth + 1));
                    stack.push((lo, mid, depth + 1));
                }
            }
        }
    }
    Ok(if undecided {
        Lies::Undecided
    } else {
        Lies::Yes
    })
}

/// The prism a part was cut from and where it was cut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrismCut {
    /// A point on the prism's start plane.
    pub origin: Point3,
    /// The direction the prism runs along (any nonzero length).
    pub axis: Vec3,
    /// The height along `axis` of the start cap.
    pub start: Length,
    /// The height of the far end, where the unbounded prism was cut off.
    pub far: Length,
}

/// Why a part reaches past the face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reach {
    /// Its far cap is still there.
    FarCap,
    /// Its section is empty: nothing shows the piece ends at the face.
    EmptySection,
    /// This edge of its section is proved not on the target's face.
    SectionOff {
        /// The edge of the part.
        edge: EdgeId,
    },
}

/// What becomes of one part of a prism cut at a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PrismPart {
    /// The part does not start at the prism's start: not the piece asked for.
    Dropped,
    /// The part reaches past the face.
    ReachesPast(Reach),
    /// The part starts at the start and ends at the face.
    Kept,
    /// This edge of the section could not be proved on or off the face.
    Undecided {
        /// The edge of the part.
        edge: EdgeId,
    },
}

/// What becomes of `part`, one piece of the prism `cut` cut at face `face` of `target`:
/// [`PrismPart::Dropped`] unless one of its `planar` faces is the start cap; else
/// [`ReachesPast`](PrismPart::ReachesPast) when one is the far cap, when the `section` (edges
/// of the part, the face the cut made) is empty, or when an edge of it is proved not on the
/// target's face; else [`Kept`](PrismPart::Kept) when every edge is proved on it.
///
/// # Errors
///
/// As [`curve_lies_on_face`], and [`NoEntity`](BodyqRefusal::NoEntity) for a section edge the
/// part does not have; a collapsed section edge is [`Collapsed`](BodyqRefusal::Collapsed).
#[allow(clippy::too_many_arguments)]
pub fn prism_part(
    target: &Brep,
    face: FaceId,
    part: &Brep,
    planar: &[PlaneAt],
    section: &[EdgeId],
    cut: &PrismCut,
    near: &Near,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<PrismPart, BodyqError> {
    let cap_at = |h: Length| -> Result<bool, BodyqError> {
        for p in planar {
            if is_square_plane_at_height(p, cut.origin, cut.axis, h, near)? {
                return Ok(true);
            }
        }
        Ok(false)
    };
    if !cap_at(cut.start)? {
        return Ok(PrismPart::Dropped);
    }
    if cap_at(cut.far)? {
        return Ok(PrismPart::ReachesPast(Reach::FarCap));
    }
    if section.is_empty() {
        return Ok(PrismPart::ReachesPast(Reach::EmptySection));
    }
    let mut undecided = None;
    for &id in section {
        let e = part
            .edge(id)
            .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
                entity: Entity::Edge(id),
            }))?;
        let EdgeShape::Curve { curve, interval } = e.shape() else {
            return Err(BodyqError::Refused(BodyqRefusal::Collapsed { edge: id }));
        };
        match curve_lies_on_face(
            target,
            face,
            curve,
            *interval,
            near.coincidence(),
            budget,
            stop,
        )? {
            Lies::Yes => {}
            Lies::OffSurface { .. } | Lies::OutsideTrim => {
                return Ok(PrismPart::ReachesPast(Reach::SectionOff { edge: id }));
            }
            Lies::Undecided => undecided = undecided.or(Some(id)),
        }
    }
    Ok(match undecided {
        Some(edge) => PrismPart::Undecided { edge },
        None => PrismPart::Kept,
    })
}
