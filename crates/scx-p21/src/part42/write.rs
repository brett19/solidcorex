// Derived from solidcore `solidcore/crates/solidcore-step/src/write.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8 (its own origin: SolidCraft's `crates/step`,
// MIT OR Apache-2.0).
// Upstream record (verbatim from that file): Writing ISO 10303-21: a DATA section built
// entity by entity, and [`crate::geom`] curves and surfaces as ISO 10303-42 entities.
// Identical geometric entities are written once and shared, as the reader shares them;
// topological ones are distinct ([`Writer::add_distinct`]). [`topology`] writes a schema 4
// record's solids.
// Changes: only the Part 42 half of that file moves here (`GeomWriter` and its helpers; the
// Part 21 layer is `crate::write`, and `topology` and `product` go with the body mapping to
// `scx-step`). It writes `super::geom` (the reader's data) where the pin wrote its kernel's
// own geometry: an absent `axis`, `ref_direction` or `direction` is written `$`, a `VECTOR`
// from its orientation and its magnitude times its unit factor, and `CURVE_REPLICA` (no such
// curve here) is not written. `GeomWriter` and its methods become free functions over a
// `Writer` that return `Result`, refusing a value with no ISO 10303-42 form: `point`,
// `point_in`, `direction`, `direction_in`, `vector_in`, `placement`, `placement_in`,
// `axis1`, `curve`, `curve_in`, `trims`, `bspline_curve`, `surface`, `bspline_surface`,
// `pcurve_context` (now written each time and shared by `Writer::add`, was kept in a field)
// and `coords` differ; `GeomWriter::new` goes. `s`, `r`, `f`, `e`, `list`, `reals`, `ints`,
// `logical`, `boolean`, `curve_form`, `surface_form` and `knot_spec` are new here (the pin's
// take `Param` values of its own writer); `pcurve`, `pcurve_item`, `opt_ref`, `refuse` and
// `Space` are new.
//! ISO 10303-42 curves and surfaces of [`super::geom`] written into a [`Writer`], so that
//! [`super::read::Reader`] reads back the same values bit for bit.
//!
//! Lengths are written in millimetres and angles in radians: the representation context the
//! caller writes must say so. Identical entities are shared through [`Writer::add`], as the
//! reader shares them; a shared [`std::sync::Arc`] is written out again each time it is met and
//! found to be the same instances.
//!
//! A value with no ISO 10303-42 form is refused as [`Unwritable::Geometry`]: a
//! point or direction off the plane of a 2D parameter space, a placement with an axis there, a
//! surface curve or a 3D offset curve there, and a vector whose components have different
//! unit factors (one read in a parameter space with an angle on one axis and a length on the
//! other).

use super::geom::{
    Associated, Axis1, BSplineCurve, BSplineSurface, Curve, CurveForm, KnotSpec, Logical, Pcurve,
    Placement, PreferredRep, Surface, SurfaceCurveKind, SurfaceForm, Trim, TrimPreference, Vector,
    P3,
};
use crate::error::{NoPart42Form, P21Error, Refusal, Unwritable};
use crate::exchange::{Instance, Param, Record};
use crate::write::Writer;

/// A result whose error is a [`P21Error`].
type Result<T> = core::result::Result<T, P21Error>;

/// The space a value is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Space {
    /// Model space: three coordinates.
    Model,
    /// A surface's 2D parameter space: two coordinates, z = 0.
    Parameter,
}

/// Refuse geometry that has no ISO 10303-42 form.
fn refuse<T>(cause: NoPart42Form) -> Result<T> {
    Err(P21Error::Refused(Refusal::Unwritable {
        id: None,
        cause: Unwritable::Geometry(cause),
    }))
}

fn s() -> Param {
    Param::Str(String::new())
}

fn r(id: u64) -> Param {
    Param::Ref(id)
}

fn f(x: f64) -> Param {
    Param::Real(x)
}

fn e(name: &str) -> Param {
    Param::Enum(name.into())
}

fn list(v: Vec<Param>) -> Param {
    Param::List(v)
}

fn reals(v: &[f64]) -> Param {
    list(v.iter().map(|x| f(*x)).collect())
}

fn ints(v: &[usize]) -> Param {
    list(v.iter().map(|x| Param::Int(*x as i64)).collect())
}

/// A reference, or `$`.
fn opt_ref(id: Option<u64>) -> Param {
    id.map_or(Param::Unset, r)
}

fn logical(l: Logical) -> Param {
    e(match l {
        Logical::True => "T",
        Logical::False => "F",
        Logical::Unknown => "U",
    })
}

fn boolean(b: bool) -> Param {
    e(if b { "T" } else { "F" })
}

fn curve_form(f: CurveForm) -> Param {
    e(match f {
        CurveForm::PolylineForm => "POLYLINE_FORM",
        CurveForm::CircularArc => "CIRCULAR_ARC",
        CurveForm::EllipticArc => "ELLIPTIC_ARC",
        CurveForm::ParabolicArc => "PARABOLIC_ARC",
        CurveForm::HyperbolicArc => "HYPERBOLIC_ARC",
        CurveForm::Unspecified => "UNSPECIFIED",
    })
}

fn surface_form(f: SurfaceForm) -> Param {
    e(match f {
        SurfaceForm::PlaneSurf => "PLANE_SURF",
        SurfaceForm::CylindricalSurf => "CYLINDRICAL_SURF",
        SurfaceForm::ConicalSurf => "CONICAL_SURF",
        SurfaceForm::SphericalSurf => "SPHERICAL_SURF",
        SurfaceForm::ToroidalSurf => "TOROIDAL_SURF",
        SurfaceForm::SurfOfRevolution => "SURF_OF_REVOLUTION",
        SurfaceForm::RuledSurf => "RULED_SURF",
        SurfaceForm::GeneralisedCone => "GENERALISED_CONE",
        SurfaceForm::QuadricSurf => "QUADRIC_SURF",
        SurfaceForm::SurfOfLinearExtrusion => "SURF_OF_LINEAR_EXTRUSION",
        SurfaceForm::Unspecified => "UNSPECIFIED",
    })
}

fn knot_spec(k: KnotSpec) -> Param {
    e(match k {
        KnotSpec::UniformKnots => "UNIFORM_KNOTS",
        KnotSpec::QuasiUniformKnots => "QUASI_UNIFORM_KNOTS",
        KnotSpec::PiecewiseBezierKnots => "PIECEWISE_BEZIER_KNOTS",
        KnotSpec::Unspecified => "UNSPECIFIED",
    })
}

/// The coordinates of `p` in `sp`: refused in a parameter space unless z is 0.
fn coords(p: P3, sp: Space) -> Result<Param> {
    let [x, y, z] = p;
    match sp {
        Space::Model => Ok(reals(&p)),
        Space::Parameter if z == 0.0 => Ok(reals(&[x, y])),
        Space::Parameter => refuse(NoPart42Form::OffParameterPlane),
    }
}

/// A `CARTESIAN_POINT` in millimetres.
pub fn point(w: &mut Writer, p: P3) -> Result<u64> {
    point_in(w, p, Space::Model)
}

fn point_in(w: &mut Writer, p: P3, sp: Space) -> Result<u64> {
    let c = coords(p, sp)?;
    w.add(Instance::simple("CARTESIAN_POINT", vec![s(), c]))
}

fn direction_in(w: &mut Writer, d: P3, sp: Space) -> Result<u64> {
    let c = coords(d, sp)?;
    w.add(Instance::simple("DIRECTION", vec![s(), c]))
}

/// A `DIRECTION` in model space, its ratios as given.
pub fn direction(w: &mut Writer, d: P3) -> Result<u64> {
    direction_in(w, d, Space::Model)
}

/// A `VECTOR`: its orientation, and its magnitude in millimetres (its magnitude times its unit
/// factor, which must be the same for every component of `sp`).
fn vector_in(w: &mut Writer, v: &Vector, sp: Space) -> Result<u64> {
    let [a, b, c] = v.scale;
    if a != b || (sp == Space::Model && b != c) {
        return refuse(NoPart42Form::MixedUnitVector);
    }
    let d = direction_in(w, v.orientation, sp)?;
    w.add(Instance::simple(
        "VECTOR",
        vec![s(), r(d), f(v.magnitude * a)],
    ))
}

fn placement_in(w: &mut Writer, p: &Placement, sp: Space) -> Result<u64> {
    let o = point_in(w, p.origin, sp)?;
    let x = match p.ref_direction {
        Some(d) => Some(direction_in(w, d, sp)?),
        None => None,
    };
    match sp {
        Space::Parameter => {
            if p.axis.is_some() {
                return refuse(NoPart42Form::AxisInParameterSpace);
            }
            w.add(Instance::simple(
                "AXIS2_PLACEMENT_2D",
                vec![s(), r(o), opt_ref(x)],
            ))
        }
        Space::Model => {
            let z = match p.axis {
                Some(d) => Some(direction_in(w, d, sp)?),
                None => None,
            };
            w.add(Instance::simple(
                "AXIS2_PLACEMENT_3D",
                vec![s(), r(o), opt_ref(z), opt_ref(x)],
            ))
        }
    }
}

/// An `AXIS2_PLACEMENT_3D` in millimetres.
pub fn placement(w: &mut Writer, p: &Placement) -> Result<u64> {
    placement_in(w, p, Space::Model)
}

/// An `AXIS1_PLACEMENT` in millimetres.
pub fn axis1(w: &mut Writer, a: &Axis1) -> Result<u64> {
    let o = point(w, a.origin)?;
    let d = match a.direction {
        Some(d) => Some(direction(w, d)?),
        None => None,
    };
    w.add(Instance::simple(
        "AXIS1_PLACEMENT",
        vec![s(), r(o), opt_ref(d)],
    ))
}

/// A 3D curve.
pub fn curve(w: &mut Writer, c: &Curve) -> Result<u64> {
    curve_in(w, c, Space::Model)
}

fn curve_in(w: &mut Writer, c: &Curve, sp: Space) -> Result<u64> {
    match c {
        Curve::Line { origin, vector } => {
            let p = point_in(w, *origin, sp)?;
            let v = vector_in(w, vector, sp)?;
            w.add(Instance::simple("LINE", vec![s(), r(p), r(v)]))
        }
        Curve::Circle { position, radius } => {
            let p = placement_in(w, position, sp)?;
            w.add(Instance::simple("CIRCLE", vec![s(), r(p), f(*radius)]))
        }
        Curve::Ellipse {
            position,
            semi_axis_1,
            semi_axis_2,
        } => {
            let p = placement_in(w, position, sp)?;
            w.add(Instance::simple(
                "ELLIPSE",
                vec![s(), r(p), f(*semi_axis_1), f(*semi_axis_2)],
            ))
        }
        Curve::Hyperbola {
            position,
            semi_axis,
            semi_imag_axis,
        } => {
            let p = placement_in(w, position, sp)?;
            w.add(Instance::simple(
                "HYPERBOLA",
                vec![s(), r(p), f(*semi_axis), f(*semi_imag_axis)],
            ))
        }
        Curve::Parabola {
            position,
            focal_dist,
        } => {
            let p = placement_in(w, position, sp)?;
            w.add(Instance::simple(
                "PARABOLA",
                vec![s(), r(p), f(*focal_dist)],
            ))
        }
        Curve::BSpline(b) => bspline_curve(w, b, sp),
        Curve::Polyline(pts) => {
            let mut ids = Vec::new();
            for p in pts {
                ids.push(r(point_in(w, *p, sp)?));
            }
            w.add(Instance::simple("POLYLINE", vec![s(), list(ids)]))
        }
        Curve::Trimmed {
            basis,
            trim_1,
            trim_2,
            sense,
            preference,
        } => {
            let b = curve_in(w, basis, sp)?;
            let t1 = trims(w, trim_1, sp)?;
            let t2 = trims(w, trim_2, sp)?;
            let pref = e(match preference {
                TrimPreference::Cartesian => "CARTESIAN",
                TrimPreference::Parameter => "PARAMETER",
                TrimPreference::Unspecified => "UNSPECIFIED",
            });
            w.add(Instance::simple(
                "TRIMMED_CURVE",
                vec![s(), r(b), t1, t2, boolean(*sense), pref],
            ))
        }
        Curve::SurfaceCurve { .. } | Curve::OffsetCurve3d { .. } if sp == Space::Parameter => {
            refuse(NoPart42Form::SpaceCurveInParameterSpace)
        }
        Curve::SurfaceCurve {
            kind,
            curve_3d,
            associated,
            master,
        } => {
            let c3 = curve_in(w, curve_3d, Space::Model)?;
            let mut items = Vec::new();
            for a in associated {
                items.push(r(match a {
                    Associated::Pcurve(pc) => pcurve(w, pc)?,
                    Associated::Surface(sf) => surface(w, sf)?,
                }));
            }
            let name = match kind {
                SurfaceCurveKind::Surface => "SURFACE_CURVE",
                SurfaceCurveKind::Seam => "SEAM_CURVE",
                SurfaceCurveKind::Intersection => "INTERSECTION_CURVE",
                SurfaceCurveKind::BoundedSurface => "BOUNDED_SURFACE_CURVE",
            };
            let m = e(match master {
                PreferredRep::Curve3d => "CURVE_3D",
                PreferredRep::PcurveS1 => "PCURVE_S1",
                PreferredRep::PcurveS2 => "PCURVE_S2",
            });
            w.add(Instance::simple(name, vec![s(), r(c3), list(items), m]))
        }
        Curve::OffsetCurve3d {
            basis,
            distance,
            self_intersect,
            ref_direction,
        } => {
            let b = curve_in(w, basis, Space::Model)?;
            let d = direction(w, *ref_direction)?;
            w.add(Instance::simple(
                "OFFSET_CURVE_3D",
                vec![s(), r(b), f(*distance), logical(*self_intersect), r(d)],
            ))
        }
    }
}

/// A `PCURVE`: its surface, a `DEFINITIONAL_REPRESENTATION` of its 2D curve in the 2D
/// parameter-space context, and the pcurve.
pub fn pcurve(w: &mut Writer, pc: &Pcurve) -> Result<u64> {
    let surf = surface(w, &pc.surface)?;
    let c2 = curve_in(w, &pc.curve, Space::Parameter)?;
    let ctx = pcurve_context(w)?;
    let rep = pcurve_item(w, c2, ctx)?;
    w.add(Instance::simple("PCURVE", vec![s(), r(surf), r(rep)]))
}

/// The `DEFINITIONAL_REPRESENTATION` of 2D curve `#c2` in context `#ctx`.
fn pcurve_item(w: &mut Writer, c2: u64, ctx: u64) -> Result<u64> {
    w.add(Instance::simple(
        "DEFINITIONAL_REPRESENTATION",
        vec![s(), list(vec![r(c2)]), r(ctx)],
    ))
}

/// The 2D parameter-space context pcurves refer to (written once: [`Writer::add`] shares it).
fn pcurve_context(w: &mut Writer) -> Result<u64> {
    w.add(Instance::complex(vec![
        Record::new("GEOMETRIC_REPRESENTATION_CONTEXT", vec![Param::Int(2)]),
        Record::new("PARAMETRIC_REPRESENTATION_CONTEXT", vec![]),
        Record::new(
            "REPRESENTATION_CONTEXT",
            vec![Param::Str("2D SPACE".into()), s()],
        ),
    ]))
}

fn trims(w: &mut Writer, t: &[Trim], sp: Space) -> Result<Param> {
    let mut items = Vec::new();
    for t in t {
        items.push(match t {
            Trim::Point(p) => r(point_in(w, *p, sp)?),
            Trim::Parameter(x) => Param::typed("PARAMETER_VALUE", f(*x)),
        });
    }
    Ok(list(items))
}

fn bspline_curve(w: &mut Writer, b: &BSplineCurve, sp: Space) -> Result<u64> {
    let mut ctrl = Vec::new();
    for p in &b.points {
        ctrl.push(r(point_in(w, *p, sp)?));
    }
    let common = vec![
        Param::Int(b.degree as i64),
        list(ctrl),
        curve_form(b.form),
        logical(b.closed),
        logical(b.self_intersect),
    ];
    let knots = vec![
        ints(&b.multiplicities),
        reals(&b.knots),
        knot_spec(b.knot_spec),
    ];
    match &b.weights {
        None => {
            let mut p = vec![s()];
            p.extend(common);
            p.extend(knots);
            w.add(Instance::simple("B_SPLINE_CURVE_WITH_KNOTS", p))
        }
        Some(wt) => w.add(Instance::complex(vec![
            Record::new("BOUNDED_CURVE", vec![]),
            Record::new("B_SPLINE_CURVE", common),
            Record::new("B_SPLINE_CURVE_WITH_KNOTS", knots),
            Record::new("CURVE", vec![]),
            Record::new("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
            Record::new("RATIONAL_B_SPLINE_CURVE", vec![reals(wt)]),
            Record::new("REPRESENTATION_ITEM", vec![s()]),
        ])),
    }
}

/// A surface.
pub fn surface(w: &mut Writer, sf: &Surface) -> Result<u64> {
    match sf {
        Surface::Plane { position } => {
            let p = placement(w, position)?;
            w.add(Instance::simple("PLANE", vec![s(), r(p)]))
        }
        Surface::Cylinder { position, radius } => {
            let p = placement(w, position)?;
            w.add(Instance::simple(
                "CYLINDRICAL_SURFACE",
                vec![s(), r(p), f(*radius)],
            ))
        }
        Surface::Cone {
            position,
            radius,
            semi_angle,
        } => {
            let p = placement(w, position)?;
            w.add(Instance::simple(
                "CONICAL_SURFACE",
                vec![s(), r(p), f(*radius), f(*semi_angle)],
            ))
        }
        Surface::Sphere { position, radius } => {
            let p = placement(w, position)?;
            w.add(Instance::simple(
                "SPHERICAL_SURFACE",
                vec![s(), r(p), f(*radius)],
            ))
        }
        Surface::Torus {
            position,
            major_radius,
            minor_radius,
        } => {
            let p = placement(w, position)?;
            w.add(Instance::simple(
                "TOROIDAL_SURFACE",
                vec![s(), r(p), f(*major_radius), f(*minor_radius)],
            ))
        }
        Surface::DegenerateTorus {
            position,
            major_radius,
            minor_radius,
            select_outer,
        } => {
            let p = placement(w, position)?;
            w.add(Instance::simple(
                "DEGENERATE_TOROIDAL_SURFACE",
                vec![
                    s(),
                    r(p),
                    f(*major_radius),
                    f(*minor_radius),
                    boolean(*select_outer),
                ],
            ))
        }
        Surface::BSpline(b) => bspline_surface(w, b),
        Surface::Revolution { profile, axis } => {
            let c = curve(w, profile)?;
            let a = axis1(w, axis)?;
            w.add(Instance::simple(
                "SURFACE_OF_REVOLUTION",
                vec![s(), r(c), r(a)],
            ))
        }
        Surface::Extrusion { profile, vector } => {
            let c = curve(w, profile)?;
            let v = vector_in(w, vector, Space::Model)?;
            w.add(Instance::simple(
                "SURFACE_OF_LINEAR_EXTRUSION",
                vec![s(), r(c), r(v)],
            ))
        }
        Surface::Offset {
            basis,
            distance,
            self_intersect,
        } => {
            let b = surface(w, basis)?;
            w.add(Instance::simple(
                "OFFSET_SURFACE",
                vec![s(), r(b), f(*distance), logical(*self_intersect)],
            ))
        }
        Surface::RectangularTrimmed {
            basis,
            u1,
            u2,
            v1,
            v2,
            u_sense,
            v_sense,
        } => {
            let b = surface(w, basis)?;
            w.add(Instance::simple(
                "RECTANGULAR_TRIMMED_SURFACE",
                vec![
                    s(),
                    r(b),
                    f(*u1),
                    f(*u2),
                    f(*v1),
                    f(*v2),
                    boolean(*u_sense),
                    boolean(*v_sense),
                ],
            ))
        }
    }
}

fn bspline_surface(w: &mut Writer, b: &BSplineSurface) -> Result<u64> {
    let mut rows = Vec::new();
    for row in &b.points {
        let mut ids = Vec::new();
        for p in row {
            ids.push(r(point(w, *p)?));
        }
        rows.push(list(ids));
    }
    let common = vec![
        Param::Int(b.u_degree as i64),
        Param::Int(b.v_degree as i64),
        list(rows),
        surface_form(b.form),
        logical(b.u_closed),
        logical(b.v_closed),
        logical(b.self_intersect),
    ];
    let knots = vec![
        ints(&b.u_multiplicities),
        ints(&b.v_multiplicities),
        reals(&b.u_knots),
        reals(&b.v_knots),
        knot_spec(b.knot_spec),
    ];
    match &b.weights {
        None => {
            let mut p = vec![s()];
            p.extend(common);
            p.extend(knots);
            w.add(Instance::simple("B_SPLINE_SURFACE_WITH_KNOTS", p))
        }
        Some(wt) => w.add(Instance::complex(vec![
            Record::new("BOUNDED_SURFACE", vec![]),
            Record::new("B_SPLINE_SURFACE", common),
            Record::new("B_SPLINE_SURFACE_WITH_KNOTS", knots),
            Record::new("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
            Record::new(
                "RATIONAL_B_SPLINE_SURFACE",
                vec![list(wt.iter().map(|row| reals(row)).collect())],
            ),
            Record::new("REPRESENTATION_ITEM", vec![s()]),
            Record::new("SURFACE", vec![]),
        ])),
    }
}
