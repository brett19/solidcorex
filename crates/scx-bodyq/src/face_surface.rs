// Written for solidcorex. Apache-2.0.
//! A face's surface for callers (research 74 §34, BW11): what the face is, and on it, in the
//! face's own (u, v), its point, its unit normal and the point nearest a given point.
//!
//! The pin built a record face's surface from numbers (`surface::face_surface`: a label, a
//! NURBS form and a parameter box; blends certified on their definitions; balls patches proved
//! on their sphere), because its body held records. A body here holds the face's own
//! [`Surface`] and its uv region, so this is a view: what [`Surface`] and the face's boundary
//! lack is the pairing of the two (the face's parameter box as the search window of a nearest
//! point) and the typed answers.
//!
//! | pin (`FaceSurface`) | here |
//! |---|---|
//! | `of(record, face)`: conversion with proofs that a label lies on its NURBS form | none: a face's surface is its own; a quadric written as NURBS is recognised on request by [`FaceSurface::recognized`] (SU7) |
//! | `exact()`: label, round torus, blend, NURBS | [`FaceSurface::kind`]: the [`SurfaceKind`]; blends do not exist here yet |
//! | `domain()` | the face's uv box ([`FaceSurface::domain`]) |
//! | `normal`: `Degenerate` where the partials are parallel | `None` there |
//! | `closest_point(p, guess)`: Newton, `NotConverged` past 48 steps | the surface's own inversion: closed form on every analytic kind, certified on NURBS, within the budget |

use scx_brep::{Brep, Face, FaceId};
use scx_num::budget::{Budget, Stop};
use scx_num::domain::ParamDomain;
use scx_num::extent2d::Extent2d;
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Recognized, Surface, SurfaceKind, recognize};

use crate::error::{BodyqDefect, BodyqError, BodyqRefusal};
use crate::reach::finite;
use scx_brep::Entity;

/// The point of a face's surface nearest a given point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Closest {
    /// Its (u, v), placed in the face's uv box.
    pub uv: Point2,
    /// The point.
    pub point: Point3,
    /// Its distance from the given point.
    pub distance: Length,
}

/// A body face's surface.
#[derive(Debug, Clone, Copy)]
pub struct FaceSurface<'a> {
    face: Face<'a>,
    domain: Extent2d,
}

/// Face `face` of `body`'s surface.
///
/// # Errors
///
/// Refused for a face the body does not have; a [`Defect`](BodyqDefect::NoUvBox) for a face
/// of a finished body whose uv region cannot be boxed.
pub fn face_surface(body: &Brep, face: FaceId) -> Result<FaceSurface<'_>, BodyqError> {
    let f = body
        .face(face)
        .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: Entity::Face(face),
        }))?;
    let domain = f
        .uv_box()
        .ok_or(BodyqError::Defect(BodyqDefect::NoUvBox { face }))?;
    Ok(FaceSurface { face: f, domain })
}

impl<'a> FaceSurface<'a> {
    /// The face's surface.
    #[must_use]
    pub const fn surface(&self) -> &'a Surface {
        self.face.surface()
    }

    /// What kind of surface the face is.
    #[must_use]
    pub fn kind(&self) -> SurfaceKind {
        SurfaceKind::of(self.surface())
    }

    /// The face's parameter box in its own (u, v): a box holding its uv region.
    #[must_use]
    pub const fn domain(&self) -> Extent2d {
        self.domain
    }

    /// The point at the face's `uv`.
    ///
    /// # Errors
    ///
    /// Refused for a `uv` that is not finite.
    pub fn point(&self, uv: Point2) -> Result<Point3, BodyqError> {
        finite(uv.x)?;
        finite(uv.y)?;
        Ok(self.surface().point(uv))
    }

    /// The unit normal at `uv`, as the surface's own sense gives it: `None` where the partials
    /// are parallel (a pole or an apex).
    ///
    /// # Errors
    ///
    /// Refused for a `uv` that is not finite.
    pub fn normal(&self, uv: Point2) -> Result<Option<Vec3>, BodyqError> {
        finite(uv.x)?;
        finite(uv.y)?;
        Ok(self
            .surface()
            .normal(uv, crate::geom::ABOVE)
            .map(scx_num::math::UnitVec3::as_vec))
    }

    /// The point of the face's surface nearest `p`, within the face's parameter box, starting
    /// the search of a NURBS surface at `near` (the analytic kinds ignore it: it changes only
    /// the work).
    ///
    /// # Errors
    ///
    /// Refused for a point that is not finite. Unsupported for a NURBS surface the inversion
    /// does not take, exhausted or cancelled by `budget` and `stop`.
    pub fn closest_point(
        &self,
        p: Point3,
        near: Option<Point2>,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Closest, BodyqError> {
        crate::reach::finite_point(p)?;
        let foot = self
            .surface()
            .invert(p, near, Some(&self.domain), budget, stop)
            .map_err(|e| BodyqError::of_surface(e, self.face.id()))?
            .into_foot();
        Ok(Closest {
            uv: foot.at,
            point: foot.point,
            distance: Length::from_mm(foot.distance),
        })
    }

    /// The simplest analytic surface within `accuracy` of the face's surface over its uv box,
    /// and the proved bound of its deviation (a quadric written as a NURBS, or a cone that is
    /// a plane or a cylinder to within the accuracy), or `None` where none is proved
    /// ([`scx_surface::recognize`]).
    ///
    /// # Errors
    ///
    /// As [`FaceSurface::closest_point`].
    pub fn recognized(
        &self,
        accuracy: Accuracy,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Option<Recognized>, BodyqError> {
        // The face's box is grown by rounding past a NURBS surface's own knots: read it inside.
        let dom = self.surface().domain();
        let clip = |lo: f64, hi: f64, d: ParamDomain| match d {
            ParamDomain::Bounded(e) => (lo.max(e.min()), hi.min(e.max())),
            _ => (lo, hi),
        };
        let b = self.domain;
        let (x0, x1) = clip(b.x_min(), b.x_max(), dom.u);
        let (y0, y1) = clip(b.y_min(), b.y_max(), dom.v);
        let over = Extent2d::from_bounds(x0, y0, x1, y1).ok_or_else(|| {
            BodyqError::Defect(BodyqDefect::NoUvBox {
                face: self.face.id(),
            })
        })?;
        recognize(self.surface(), &over, accuracy, budget, stop)
            .map_err(|e| BodyqError::of_surface(e, self.face.id()))
    }
}
