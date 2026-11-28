// Written for solidcorex (docs/research/74-step6-idiom-prep.md §6.3, §29).
//! Part 42 geometry, as `scx-p21` reads it, into the kernel's analytic kinds: planes,
//! cylinders, cones, spheres and tori; lines, circles, ellipses, parabolas and hyperbolas.
//!
//! A B-spline surface goes through `scx_surface::recognize` over its whole domain and is a
//! quadric only where that proves it within the body's allowance (SP2b, 74 §38); a cone or
//! torus whose written sizes the kinds refuse goes through `recognize_written`. Every other
//! kind is a typed [`Waits`](crate::Waits): the B-splines no quadric fits are task SP5's, the
//! swept and offset kinds task SP6's. Nothing is approximated without a record. A `SURFACE_CURVE` or `SEAM_CURVE` is its 3-D
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
use scx_nurbs::KnotVector;
use scx_nurbs_surface::{Net, NurbsSurface, NurbsSurfaceError, ParamAxis};
use scx_num::budget::{Budget, Stop};
use scx_num::units::Accuracy;
use scx_p21::part42::geom::{self, Placement};
use scx_surface::{
    Cylinder, Plane, Sphere, Surface, SurfaceError, Torus, Written, recognize, recognize_written,
};

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
/// when `z` lies along X (its `dummy_gri || direction([1.0, 0.0, 0.0])`, `[0.0, 1.0, 0.0]`
/// for an axis of ratios `[1, 0, 0]` or `[-1, 0, 0]`). The placements of parts and the axes of
/// transformation operators take the same default.
pub(crate) fn default_ref(z: Vec3) -> Vec3 {
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

/// A B-spline surface's distinct knots and multiplicities as one knot vector.
fn knots(axis: ParamAxis, degree: usize, distinct: &[f64], mult: &[usize]) -> Result<KnotVector, NotMade> {
    let all = distinct
        .iter()
        .zip(mult)
        .flat_map(|(&k, &m)| core::iter::repeat_n(k, m))
        .collect();
    KnotVector::new(degree, all).map_err(|cause| {
        surface_err(SurfaceError::Nurbs(NurbsSurfaceError::BadKnots { axis, cause }))
    })
}

/// The B-spline surface a file writes, as a kernel NURBS surface.
fn nurbs(b: &geom::BSplineSurface) -> Result<NurbsSurface<3>, NotMade> {
    let bad = |e| surface_err(SurfaceError::Nurbs(e));
    let (rows, cols) = (b.points.len(), b.points.first().map_or(0, Vec::len));
    let net = Net::new(rows, cols, b.points.iter().flatten().copied().collect()).map_err(bad)?;
    let weights = match &b.weights {
        None => None,
        Some(w) => Some(Net::new(rows, cols, w.iter().flatten().copied().collect()).map_err(bad)?),
    };
    NurbsSurface::new(
        knots(ParamAxis::U, b.u_degree, &b.u_knots, &b.u_multiplicities)?,
        knots(ParamAxis::V, b.v_degree, &b.v_knots, &b.v_multiplicities)?,
        net,
        weights,
    )
    .map_err(bad)
}

/// What recognition needs: the accuracy a surface may be stated within (the body's allowance),
/// a budget for the certificate, and the stop.
pub(crate) struct Recognize<'a> {
    pub accuracy: Accuracy,
    pub budget: &'a mut Budget,
    pub stop: &'a Stop,
}

/// The surface a face names, and where it is not the kind the file wrote, the proved deviation
/// of the surface from the file's.
pub(crate) fn surface(
    s: &geom::Surface,
    rec: &mut Recognize<'_>,
) -> Result<(Surface, Option<Length>), NotMade> {
    let mm = Length::from_mm;
    let exact = |s| Ok((s, None));
    // A kind written with sizes its constructor refuses is the kind it stands for, within
    // the deviation `recognize_written` proves.
    let written = |w: Written, rec: &Recognize<'_>| {
        let r = recognize_written(&w, rec.accuracy).map_err(surface_err)?;
        let moved = r.deviation.mm() > 0.0;
        Ok((r.surface, moved.then_some(r.deviation)))
    };
    match s {
        geom::Surface::Plane { position } => exact(Surface::Plane(Plane::new(placed(position)?))),
        geom::Surface::Cylinder { position, radius } => exact(Surface::Cylinder(
            Cylinder::new(placed(position)?, mm(*radius), Sense::Forward).map_err(surface_err)?,
        )),
        geom::Surface::Cone {
            position,
            radius,
            semi_angle,
        } => {
            let w = Written::Cone {
                frame: placed(position)?,
                radius: mm(*radius),
                half_angle: *semi_angle,
                sense: Sense::Forward,
            };
            let (surf, dev) = written(w, rec)?;
            // An angle of zero is the cylinder the file means; that is a recognition too.
            Ok((surf, dev.or_else(|| (*semi_angle == 0.0).then_some(Length::ZERO))))
        }
        geom::Surface::Sphere { position, radius } => exact(Surface::Sphere(
            Sphere::new(placed(position)?, mm(*radius), Sense::Forward).map_err(surface_err)?,
        )),
        geom::Surface::Torus {
            position,
            major_radius,
            minor_radius,
        } => {
            let w = Written::Torus {
                frame: placed(position)?,
                major: mm(*major_radius),
                minor: mm(*minor_radius),
                sense: Sense::Forward,
            };
            let (surf, dev) = written(w, rec)?;
            let sphere = matches!(surf, Surface::Sphere(_));
            Ok((surf, dev.or_else(|| sphere.then_some(Length::ZERO))))
        }
        geom::Surface::BSpline(b) => {
            let n = nurbs(b)?;
            let over = n.domain();
            let given = Surface::Nurbs(std::sync::Arc::new(n));
            match recognize(&given, &over, rec.accuracy, rec.budget, rec.stop).map_err(surface_err)? {
                Some(r) => Ok((r.surface, Some(r.deviation))),
                None => Err(NotMade::Waits(Waits::BSplineSurface)),
            }
        }
        geom::Surface::Revolution { .. } => Err(NotMade::Waits(Waits::Revolution)),
        geom::Surface::Extrusion { .. } => Err(NotMade::Waits(Waits::Extrusion)),
        geom::Surface::Offset { .. } => Err(NotMade::Waits(Waits::OffsetSurface)),
        geom::Surface::DegenerateTorus { .. } => Err(NotMade::Waits(Waits::DegenerateTorus)),
        geom::Surface::RectangularTrimmed { .. } => Err(NotMade::Waits(Waits::TrimmedSurface)),
        _ => Err(NotMade::Waits(Waits::OtherSurface)),
    }
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

    // ---- SP2b: quadrics written as other kinds (74 §38)

    use scx_num::extent2d::Extent2d;
    use scx_p21::part42::geom::{KnotSpec, Logical, SurfaceForm};

    fn accuracy(mm: f64) -> Accuracy {
        Accuracy::new(Length::from_mm(mm)).unwrap()
    }

    /// `n` as a file writes it: rows along u, distinct knots and multiplicities.
    fn written_bspline(n: &NurbsSurface<3>) -> geom::BSplineSurface {
        let net = n.points();
        let rows = |f: &dyn Fn(usize, usize) -> [f64; 3]| -> Vec<Vec<[f64; 3]>> {
            (0..net.nrows())
                .map(|i| (0..net.ncols()).map(|j| f(i, j)).collect())
                .collect()
        };
        let pts = net.points();
        let cols = net.ncols();
        let knots = |a: ParamAxis| {
            let kv = n.knot_vector(a);
            let (k, m): (Vec<f64>, Vec<usize>) = kv.breakpoints().unzip();
            (kv.degree(), k, m)
        };
        let (ud, uk, um) = knots(ParamAxis::U);
        let (vd, vk, vm) = knots(ParamAxis::V);
        geom::BSplineSurface {
            u_degree: ud,
            v_degree: vd,
            points: rows(&|i, j| pts[i * cols + j]),
            weights: n.weights().map(|w| {
                (0..net.nrows())
                    .map(|i| (0..cols).map(|j| w.points()[i * cols + j]).collect())
                    .collect()
            }),
            u_knots: uk,
            u_multiplicities: um,
            v_knots: vk,
            v_multiplicities: vm,
            form: SurfaceForm::Unspecified,
            u_closed: Logical::Unknown,
            v_closed: Logical::Unknown,
            self_intersect: Logical::Unknown,
            knot_spec: KnotSpec::Unspecified,
        }
    }

    fn through(
        s: &Surface,
        over: (f64, f64, f64, f64),
        acc: f64,
        bump: Option<f64>,
    ) -> Result<(Surface, Option<Length>), NotMade> {
        let ext = Extent2d::from_bounds(over.0, over.2, over.1, over.3).unwrap();
        let n = s.to_nurbs(&ext).unwrap();
        let mut b = written_bspline(&n);
        if let Some(d) = bump {
            b.points[2][2][0] += d;
        }
        let mut rec = Recognize {
            accuracy: accuracy(acc),
            budget: &mut Budget::unlimited(),
            stop: &Stop::new(),
        };
        surface(&geom::Surface::BSpline(Box::new(b)), &mut rec)
    }

    fn frame3() -> Frame {
        Frame::new(
            Point3::new(1.5, -2.0, 0.7),
            Vec3::new(0.3, -0.4, 0.86),
            Vec3::new(1.0, 0.2, 0.0),
        )
        .unwrap()
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }

    #[test]
    fn exact_bspline_forms_of_each_quadric_are_recognised_with_their_sizes() {
        let mm = Length::from_mm;
        let f = frame3();
        let cases: [(Surface, (f64, f64, f64, f64)); 5] = [
            (Surface::Plane(Plane::new(f)), (-1.0, 2.0, 0.0, 3.0)),
            (
                Surface::Cylinder(Cylinder::new(f, mm(2.5), Sense::Forward).unwrap()),
                (0.2, 5.0, -1.0, 4.0),
            ),
            (
                Surface::Cone(scx_surface::Cone::new(f, mm(2.0), 0.3, Sense::Forward).unwrap()),
                (0.0, 3.0, 0.5, 3.0),
            ),
            (
                Surface::Sphere(Sphere::new(f, mm(3.0), Sense::Forward).unwrap()),
                (0.1, 3.0, -1.0, 1.0),
            ),
            (
                Surface::Torus(Torus::new(f, mm(5.0), mm(1.5), Sense::Forward).unwrap()),
                (0.0, 2.0, 0.0, 4.0),
            ),
        ];
        for (truth, over) in cases {
            let (got, dev) = through(&truth, over, 1e-6, None).unwrap();
            let dev = dev.unwrap().mm();
            assert!((0.0..1e-9).contains(&dev), "{truth:?}: {dev}");
            match (&truth, &got) {
                (Surface::Plane(a), Surface::Plane(b)) => {
                    close((b.frame().origin() - a.frame().origin()).dot(a.frame().z().as_vec()), 0.0);
                    close(a.frame().z().as_vec().dot(b.frame().z().as_vec()), 1.0);
                }
                (Surface::Cylinder(a), Surface::Cylinder(b)) => {
                    close(a.radius().mm(), b.radius().mm());
                    close(a.frame().z().as_vec().dot(b.frame().z().as_vec()).abs(), 1.0);
                }
                (Surface::Cone(a), Surface::Cone(b)) => {
                    close(a.half_angle().abs(), b.half_angle().abs());
                }
                (Surface::Sphere(a), Surface::Sphere(b)) => {
                    close(a.radius().mm(), b.radius().mm());
                    close((a.frame().origin() - b.frame().origin()).length(), 0.0);
                }
                (Surface::Torus(a), Surface::Torus(b)) => {
                    close(a.major().mm(), b.major().mm());
                    close(a.minor().mm(), b.minor().mm());
                }
                _ => panic!("{truth:?} recognised as {got:?}"),
            }
        }
    }

    #[test]
    fn a_bspline_beyond_coincidence_with_a_quadric_stays_pending() {
        let cyl = Surface::Cylinder(
            Cylinder::new(frame3(), Length::from_mm(2.5), Sense::Forward).unwrap(),
        );
        let over = (0.2, 5.0, -1.0, 4.0);
        // A control point moved 1e-9 is within 1e-6; moved 1e-3 it is not.
        assert!(through(&cyl, over, 1e-6, Some(1e-9)).is_ok());
        assert_eq!(
            through(&cyl, over, 1e-6, Some(1e-3)).map(|_| ()),
            Err(NotMade::Waits(Waits::BSplineSurface))
        );
    }

    #[test]
    fn a_cone_of_angle_zero_is_the_cylinder_and_a_spindle_torus_a_sphere() {
        let position = Placement {
            origin: [0.0, 0.0, 0.0],
            axis: None,
            ref_direction: None,
        };
        let mut rec = Recognize {
            accuracy: accuracy(1e-3),
            budget: &mut Budget::unlimited(),
            stop: &Stop::new(),
        };
        let (s, dev) = surface(
            &geom::Surface::Cone {
                position,
                radius: 5.0,
                semi_angle: 0.0,
            },
            &mut rec,
        )
        .unwrap();
        assert!(matches!(&s, Surface::Cylinder(c) if c.radius().mm() == 5.0));
        assert_eq!(dev, Some(Length::ZERO));
        let torus = |major| geom::Surface::Torus {
            position,
            major_radius: major,
            minor_radius: 2.0,
        };
        // Within the accuracy: the sphere of the minor radius, moved by at most the major.
        let (s, dev) = surface(&torus(5e-4), &mut rec).unwrap();
        assert!(matches!(&s, Surface::Sphere(c) if c.radius().mm() == 2.0));
        assert_eq!(dev, Some(Length::from_mm(5e-4)));
        // Beyond it: the torus's own refusal, never a sphere.
        assert!(matches!(
            surface(&torus(0.5), &mut rec),
            Err(NotMade::Bad(GeomCause::Surface(SurfaceError::SelfIntersecting { .. })))
        ));
        // An ordinary cone and torus are not recognitions.
        let cone = geom::Surface::Cone {
            position,
            radius: 5.0,
            semi_angle: 0.4,
        };
        assert_eq!(surface(&cone, &mut rec).unwrap().1, None);
        assert_eq!(surface(&torus(5.0), &mut rec).unwrap().1, None);
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
        let mut rec = Recognize {
            accuracy: Accuracy::new(Length::from_mm(1e-3)).unwrap(),
            budget: &mut Budget::unlimited(),
            stop: &Stop::new(),
        };
        assert_eq!(
            surface(&s, &mut rec).map(|_| ()),
            Err(NotMade::Waits(Waits::OffsetSurface))
        );
    }
}
