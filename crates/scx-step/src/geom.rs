// Written for solidcorex (docs/research/74-step6-idiom-prep.md §6.3, §29).
//! Part 42 geometry, as `scx-p21` reads it, into the kernel's analytic kinds: planes,
//! cylinders, cones, spheres and tori; lines, circles, ellipses, parabolas and hyperbolas.
//!
//! Every other kind is a typed [`Waits`](crate::Waits): B-splines are task SP5's, the swept
//! and offset kinds task SP6's. Nothing is approximated or recognised here (quadrics written
//! as other kinds wait for a later task). A `SURFACE_CURVE` or `SEAM_CURVE` is its 3-D
//! curve, and a `TRIMMED_CURVE` its basis: an edge is bounded by its vertices, never by the
//! file's trims, and the file's `PCURVE`s are not read (the writer gives each use its
//! exact pcurve).
//!
//! Frames follow ISO 10303-42: Z is the axis (+Z when absent), X the reference direction
//! made square to Z, and where the reference direction is absent `first_proj_axis`'s
//! default, +X, or +Y when Z lies along X.

use scx_curve::{Circle, CurveError};
use scx_curve::{Curve, Ellipse, Hyperbola, Line, Parabola};
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_p21::part42::geom::{self, Placement};
use scx_surface::{Cone, Cylinder, Plane, Sphere, Surface, SurfaceError, Torus};

use crate::import::{GeomCause, Waits};

/// Why a curve or surface entity is not built: a kind a later task reads, or one whose
/// numbers do not make the kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum NotMade {
    /// A later task's kind.
    Waits(Waits),
    /// The numbers do not make the kind.
    Bad(GeomCause),
}

fn vec(v: [f64; 3]) -> Vec3 {
    Vec3::new(v[0], v[1], v[2])
}

fn point(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

/// ISO 10303-42's `first_proj_axis` default for an absent reference direction: +X, or +Y
/// when `z` lies along X.
fn default_ref(z: Vec3) -> Vec3 {
    if z.cross(Vec3::X).normalized().is_some() {
        Vec3::X
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    }
}

/// An `AXIS2_PLACEMENT_3D` as a frame; `None` for a direction of no length or a reference
/// direction along the axis.
pub(crate) fn frame(p: &Placement) -> Option<Frame> {
    let z = p
        .axis
        .map_or(Some(Vec3::Z), |a| Some(vec(a).normalized()?.as_vec()))?;
    let hint = p.ref_direction.map_or_else(|| default_ref(z), vec);
    Frame::new(point(p.origin), z, hint)
}

/// `frame` turned half a turn about its z axis: x and y negated.
fn half_turn(p: &Placement) -> Option<Frame> {
    let f = frame(p)?;
    Frame::new(f.origin(), f.z().as_vec(), -f.x().as_vec())
}

fn placed(p: &Placement) -> Result<Frame, NotMade> {
    frame(p).ok_or(NotMade::Bad(GeomCause::Placement))
}

fn curve_err(e: CurveError) -> NotMade {
    NotMade::Bad(GeomCause::Curve(e))
}

fn surface_err(e: SurfaceError) -> NotMade {
    NotMade::Bad(GeomCause::Surface(e))
}

/// The analytic curve an edge's geometry names.
pub(crate) fn curve(c: &geom::Curve) -> Result<Curve, NotMade> {
    Ok(match c {
        geom::Curve::Line { origin, vector } => {
            Curve::Line(Line::new(point(*origin), vec(vector.orientation)).map_err(curve_err)?)
        }
        geom::Curve::Circle { position, radius } => Curve::Circle(
            Circle::new(placed(position)?, Length::from_mm(*radius), Sense::Forward)
                .map_err(curve_err)?,
        ),
        geom::Curve::Ellipse {
            position,
            semi_axis_1,
            semi_axis_2,
        } => Curve::Ellipse(
            Ellipse::new(
                placed(position)?,
                Length::from_mm(*semi_axis_1),
                Length::from_mm(*semi_axis_2),
                Sense::Forward,
            )
            .map_err(curve_err)?,
        ),
        geom::Curve::Hyperbola {
            position,
            semi_axis,
            semi_imag_axis,
        } => Curve::Hyperbola(
            Hyperbola::new(
                placed(position)?,
                Length::from_mm(*semi_axis),
                Length::from_mm(*semi_imag_axis),
                Sense::Forward,
            )
            .map_err(curve_err)?,
        ),
        // `origin + f t^2 X + 2 f t Y`: a negative `f` is the parabola of `|f|` opening
        // along -X, its y term along -Y, so the same points at the same `t`.
        geom::Curve::Parabola {
            position,
            focal_dist,
        } => {
            let f = if *focal_dist < 0.0 {
                half_turn(position).ok_or(NotMade::Bad(GeomCause::Placement))?
            } else {
                placed(position)?
            };
            Curve::Parabola(
                Parabola::new(f, Length::from_mm(focal_dist.abs()), Sense::Forward)
                    .map_err(curve_err)?,
            )
        }
        geom::Curve::SurfaceCurve { curve_3d, .. } => curve(curve_3d)?,
        geom::Curve::Trimmed { basis, .. } => curve(basis)?,
        geom::Curve::BSpline(_) => return Err(NotMade::Waits(Waits::BSplineCurve)),
        geom::Curve::Polyline(_) => return Err(NotMade::Waits(Waits::Polyline)),
        geom::Curve::OffsetCurve3d { .. } => return Err(NotMade::Waits(Waits::OffsetCurve)),
        _ => return Err(NotMade::Waits(Waits::OtherCurve)),
    })
}

/// The analytic surface a face names.
pub(crate) fn surface(s: &geom::Surface) -> Result<Surface, NotMade> {
    let mm = Length::from_mm;
    Ok(match s {
        geom::Surface::Plane { position } => Surface::Plane(Plane::new(placed(position)?)),
        geom::Surface::Cylinder { position, radius } => Surface::Cylinder(
            Cylinder::new(placed(position)?, mm(*radius), Sense::Forward).map_err(surface_err)?,
        ),
        geom::Surface::Cone {
            position,
            radius,
            semi_angle,
        } => Surface::Cone(
            Cone::new(placed(position)?, mm(*radius), *semi_angle, Sense::Forward)
                .map_err(surface_err)?,
        ),
        geom::Surface::Sphere { position, radius } => Surface::Sphere(
            Sphere::new(placed(position)?, mm(*radius), Sense::Forward).map_err(surface_err)?,
        ),
        geom::Surface::Torus {
            position,
            major_radius,
            minor_radius,
        } => Surface::Torus(
            Torus::new(
                placed(position)?,
                mm(*major_radius),
                mm(*minor_radius),
                Sense::Forward,
            )
            .map_err(surface_err)?,
        ),
        geom::Surface::BSpline(_) => return Err(NotMade::Waits(Waits::BSplineSurface)),
        geom::Surface::Revolution { .. } => return Err(NotMade::Waits(Waits::Revolution)),
        geom::Surface::Extrusion { .. } => return Err(NotMade::Waits(Waits::Extrusion)),
        geom::Surface::Offset { .. } => return Err(NotMade::Waits(Waits::OffsetSurface)),
        geom::Surface::DegenerateTorus { .. } => {
            return Err(NotMade::Waits(Waits::DegenerateTorus));
        }
        geom::Surface::RectangularTrimmed { .. } => {
            return Err(NotMade::Waits(Waits::TrimmedSurface));
        }
        _ => return Err(NotMade::Waits(Waits::OtherSurface)),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]

    use super::*;

    #[test]
    fn an_absent_reference_direction_is_first_proj_axis_default() {
        let p = Placement {
            origin: [1.0, 2.0, 3.0],
            axis: Some([0.0, 0.0, 2.0]),
            ref_direction: None,
        };
        let f = frame(&p).unwrap();
        assert_eq!(f.x().as_vec(), Vec3::X);
        let along_x = Placement {
            axis: Some([-3.0, 0.0, 0.0]),
            ..p
        };
        let f = frame(&along_x).unwrap();
        assert_eq!(f.x().as_vec(), Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(f.z().as_vec(), Vec3::new(-1.0, 0.0, 0.0));
    }

    #[test]
    fn a_negative_focal_distance_keeps_the_points() {
        let position = Placement {
            origin: [0.0, 0.0, 0.0],
            axis: None,
            ref_direction: None,
        };
        let c = curve(&geom::Curve::Parabola {
            position,
            focal_dist: -2.0,
        })
        .unwrap();
        for t in [-1.5, 0.0, 0.5, 2.0] {
            let p = c.point(t);
            let want = Point3::new(-2.0 * t * t, -4.0 * t, 0.0);
            assert!((p - want).length() < 1e-12, "{t}: {p:?}");
        }
    }

    #[test]
    fn splines_and_swept_kinds_wait_by_name() {
        let position = Placement {
            origin: [0.0, 0.0, 0.0],
            axis: None,
            ref_direction: None,
        };
        let s = geom::Surface::Offset {
            basis: std::sync::Arc::new(geom::Surface::Plane { position }),
            distance: 1.0,
            self_intersect: geom::Logical::False,
        };
        assert_eq!(surface(&s), Err(NotMade::Waits(Waits::OffsetSurface)));
    }
}
