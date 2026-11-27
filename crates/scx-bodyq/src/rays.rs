// Written for solidcorex. Apache-2.0.
//! Step 2: one ray's nearest hit, and whether it is clean enough to decide.

use core::f64::consts::PI;

use scx_brep::{Brep, EdgeId, EdgeShape, Entity, Face, FaceSide, RegionId, UvSide};
use scx_curve::{Curve, Line};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Contact, CurveSurfaceHit, Cylinder, Surface, SurfaceError, intersect_curve};

use crate::error::{BodyqDefect, BodyqError, BodyqUnsupported};
use crate::geom::{ABOVE, coincidence, recorded, segment_distance, slab};

/// The ray directions, tried in this order (72 §12.4 row 2).
///
/// Thirteen unit vectors in general position, chosen by a seeded search that maximised the
/// worst of three margins: each is at least 10.2 degrees from every coordinate plane and
/// every diagonal plane (`x = ±y`, `y = ±z`, `z = ±x`), at least 15.5 degrees from every
/// axis, face diagonal and body diagonal, and no two are within 25.6 degrees of one line
/// (so no two are opposite either). Bodies are drawn in coordinate frames, so their faces,
/// edges and symmetry planes sit in those planes and along those lines; an axis-and-diagonal
/// table is defeated by a body such as a rhombic dodecahedron whose edges and vertices lie
/// on every one of its rays from the centre. Every direction then meets a face of a
/// generic body at a clean crossing for all but a measure-zero set of points.
pub const RAYS: [Vec3; 13] = [
    Vec3::new(-0.19650924642642, 0.4478895998904253, 0.8722264742478959),
    Vec3::new(0.2837483763251629, -0.7929026206230058, -0.5392516046726274),
    Vec3::new(
        0.17781955742845118,
        0.8524331909993891,
        -0.49166844507101426,
    ),
    Vec3::new(0.845623508021843, -0.19481005267242865, 0.4969606886450855),
    Vec3::new(
        -0.8436454464865057,
        -0.48422740528147556,
        -0.23191847834304632,
    ),
    Vec3::new(
        -0.8172256389515528,
        0.5450620560334732,
        -0.18721541099169467,
    ),
    Vec3::new(0.2643857916688676, -0.7962019964165702, 0.5442081716272682),
    Vec3::new(0.5470077413943123, 0.19196595088258755, -0.8148199829142859),
    Vec3::new(
        -0.5151796350444163,
        0.22675539916300705,
        -0.8265421541493981,
    ),
    Vec3::new(
        -0.8445626306734885,
        -0.22371829144610733,
        0.4864813346288955,
    ),
    Vec3::new(0.534897432579003, 0.26859749394763544, 0.8010868385296572),
    Vec3::new(-0.458246972149871, -0.8690469633261113, -0.1864593469074155),
    Vec3::new(0.5469130804085659, 0.8004930975647344, -0.24514665657363194),
];

/// The least `|n . d|` at a clean hit: below it the ray runs so nearly along the face that
/// its side is not read from the sign.
const GRAZE: f64 = 1e-6;

/// What one ray tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cast {
    /// The ray's nearest hit is clean, or it meets nothing: the point is in this region.
    Clean(RegionId),
    /// The ray decides nothing.
    Unclean,
}

/// One place the ray meets a face.
struct Hit {
    t: f64,
    /// Coincidence at the face.
    c: f64,
    /// The region on the start's side, for a clean hit.
    clean: Option<RegionId>,
}

/// Casts the ray `p + t d`, `t` in `[0, len]`, `len` reaching past the body's box.
pub(crate) fn cast(
    body: &Brep,
    p: Point3,
    d: Vec3,
    len: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Cast, BodyqError> {
    let far = p + d * len;
    let Some(span) = Extent3d::spanning(p, far) else {
        return Ok(Cast::Unclean);
    };
    // The faces whose box the ray enters, by where it enters.
    let mut faces: Vec<(f64, Face<'_>)> = body
        .faces_meeting(span)
        .filter_map(|f| match f.bounding_box() {
            Some(b) => slab(&b, p, d, len).map(|(t0, _)| (t0, f)),
            None => Some((0.0, f)),
        })
        .collect();
    faces.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id().cmp(&b.1.id())));
    let ray = BodyqError::Defect(BodyqDefect::Ray);
    let curve = Curve::Line(Line::new(p, d).map_err(|_| ray)?);
    let ce = Extent1d::new(0.0, len).ok_or(ray)?;

    let mut hits: Vec<Hit> = Vec::new();
    // The farthest t a hit could have and still matter: the nearest hit's t plus its
    // coincidence.
    let mut horizon = len;
    for (t0, face) in &faces {
        if *t0 > horizon {
            break;
        }
        stop.check()?;
        let c = coincidence(body, face.accuracy());
        let se = face
            .uv_box()
            .ok_or_else(|| BodyqError::Defect(BodyqDefect::NoUvBox { face: face.id() }))?;
        let found = intersect_curve(
            &curve,
            &ce,
            face.surface(),
            &se,
            body.resolution(),
            budget,
            stop,
        )
        .map_err(|e| BodyqError::of_surface(e, face.id()))?;
        for h in found {
            let hit = match h {
                CurveSurfaceHit::Point { t, uv, contact, .. } => {
                    let side = face.contains_uv(uv);
                    if side == Some(UvSide::Outside) {
                        continue;
                    }
                    let clean =
                        (contact == Contact::Crossing && side == Some(UvSide::Inside) && t > c)
                            .then(|| face.surface().normal(uv, ABOVE))
                            .flatten()
                            .and_then(|n| {
                                let nd = n.dot(d);
                                (nd.abs() > GRAZE).then(|| {
                                    // The natural normal points to the front: a ray running
                                    // along it starts behind the face.
                                    let from = if nd > 0.0 {
                                        FaceSide::Back
                                    } else {
                                        FaceSide::Front
                                    };
                                    face.shell(from).region().id()
                                })
                            });
                    Hit { t, c, clean }
                }
                CurveSurfaceHit::Overlap { t } => Hit {
                    t: t.min(),
                    c,
                    clean: None,
                },
            };
            horizon = horizon.min(hit.t + hit.c);
            hits.push(hit);
        }
    }
    hits.sort_by(|a, b| a.t.total_cmp(&b.t));
    let answer = match hits.first() {
        None => body.outside().id(),
        Some(first) => {
            let Some(r) = first.clean else {
                return Ok(Cast::Unclean);
            };
            if hits
                .get(1)
                .is_some_and(|h| h.t <= first.t + first.c.max(h.c))
            {
                return Ok(Cast::Unclean);
            }
            r
        }
    };
    // No edge or vertex may come within coincidence of the ray before the horizon: there
    // the faces' uv tests may both miss a crossing (a crack), or the nearest hit's face may
    // end just past it.
    let mut edges: Vec<EdgeId> = faces
        .iter()
        .filter(|(t0, _)| *t0 <= horizon)
        .flat_map(|(_, f)| f.uses().map(|u| u.edge().id()))
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let tube = Tube::new(p, d, horizon)?;
    for id in edges {
        let Some(edge) = body.edge(id) else { continue };
        stop.check()?;
        if let Some(b) = edge.bounding_box()
            && slab(&b, p, d, horizon).is_none()
        {
            continue;
        }
        for v in [edge.start(), edge.end()] {
            let cv = coincidence(body, recorded(v.accuracy()));
            if segment_distance(p, d, horizon, v.point()).0 <= cv {
                return Ok(Cast::Unclean);
            }
        }
        if let EdgeShape::Curve { curve, interval } = edge.shape() {
            let ce = coincidence(body, recorded(edge.accuracy()));
            if tube.meets(id, curve, interval, ce, body, budget, stop)? {
                return Ok(Cast::Unclean);
            }
        }
    }
    Ok(Cast::Clean(answer))
}

/// The ray's neighbourhood an edge must keep out of: a thin cylinder about it.
struct Tube {
    horizon: f64,
    frame: Frame,
}

impl Tube {
    fn new(p: Point3, d: Vec3, horizon: f64) -> Result<Self, BodyqError> {
        // The axis the ray runs least along is a hint well off it.
        let hint = if d.x.abs() <= d.y.abs() && d.x.abs() <= d.z.abs() {
            Vec3::new(1.0, 0.0, 0.0)
        } else if d.y.abs() <= d.z.abs() {
            Vec3::new(0.0, 1.0, 0.0)
        } else {
            Vec3::new(0.0, 0.0, 1.0)
        };
        let frame = Frame::new(p, d, hint).ok_or(BodyqError::Defect(BodyqDefect::Ray))?;
        Ok(Self { horizon, frame })
    }

    /// Whether the curve over `interval` comes within `c` of the ray before the horizon:
    /// it meets the cylinder of radius `c` about the ray over that stretch. A curve wholly
    /// inside that cylinder has its ends there, which the vertex test finds.
    #[allow(clippy::too_many_arguments)]
    fn meets(
        &self,
        edge: EdgeId,
        curve: &Curve,
        interval: &Extent1d,
        c: f64,
        body: &Brep,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<bool, BodyqError> {
        let ray = BodyqError::Defect(BodyqDefect::Ray);
        let cyl = Cylinder::new(self.frame, Length::from_mm(c), Sense::Forward).map_err(|_| ray)?;
        let se = Extent2d::from_bounds(0.0, 0.0, 2.0 * PI * c, self.horizon).ok_or(ray)?;
        let hits = intersect_curve(
            curve,
            interval,
            &Surface::Cylinder(cyl),
            &se,
            body.resolution(),
            budget,
            stop,
        )
        .map_err(|e| match e {
            SurfaceError::Exhausted(x) => BodyqError::of_budget(x),
            SurfaceError::Stopped => BodyqError::Cancelled,
            SurfaceError::NurbsIntersection => BodyqError::Unsupported(BodyqUnsupported::Nurbs {
                at: Entity::Edge(edge),
            }),
            _ => ray,
        })?;
        Ok(!hits.is_empty())
    }
}
