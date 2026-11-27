// Derived from solidcore `solidcore/crates/solidcore-record/src/step/read.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8 (its own origin: SolidCraft's `crates/step`,
// MIT OR Apache-2.0).
// Upstream record (verbatim from that file): STEP geometry entities (ISO 10303-42) to the plain
// data of [`super::geom`], from a parsed [`Exchange`] (ISO 10303-21) in one representation
// context's [`Units`].
// Changes: every refusal is a typed `P21Error` (was a `String`): `EntityCause::Missing`,
// `WrongType`, `BadParam` (with the parameter's position in its record), `Invalid` and
// `TooDeep`, or `Unsupported::Entity` for valid input not read here; so `entity`,
// `reference`, `num`, `logical`, `boolean`, `positive`, `knots`, `implied_knots`, `weights`,
// `degree`, `bspline_params`, `Reader::length`, `Reader::angle`, `Reader::point_in`,
// `Reader::point`, `Reader::ratios`, `Reader::direction`, `Reader::placement_in`,
// `Reader::placement`, `Reader::frame`, `Reader::axis1`, `Reader::vector_in`,
// `Reader::curve`, `Reader::curve_in`, `Reader::curve_2d`, `Reader::conic_scale`,
// `Reader::read_curve`, `Reader::trims`, `Reader::pcurve`, `Reader::pcurve_in`,
// `Reader::associated`, `Reader::bspline_curve`, `Reader::surface`, `Reader::surface_in`,
// `Reader::read_surface` and `Reader::bspline_surface` differ in their error paths alone.
// `Fields` (new, with `Fields::of`, `Fields::slot`, `Fields::get`, `Fields::bad`,
// `Fields::reference`, `Fields::opt_ref`, `Fields::num`, `Fields::list`, `Fields::logical`,
// `Fields::boolean`, `Fields::enumeration`) reads a record's parameters by the simple form's
// positions, so `bspline_params` no longer clones them; `number`, `logical_of`, `refuse`,
// `invalid`, `unsupported` and `Dim::count` are new. `Space` holds a `Dim` (was a `usize`),
// so `Space::model` and `Space::parameters` differ. `spec_of_type` uses `Instance::has` alone
// (it covers the simple name). A placement's entity name is checked before its location is
// read (was after; only which error comes first differs). 2D curves are cached at every
// level, the basis of a 2D trimmed curve too (was re-read each time); `Reader::curve_at`
// (new) is the one cached read. `Reader::new`, `Reader::units`, `Reader::factor`,
// `Reader::param_kinds`, `Reader::curve_param_kind`, `curve_form`, `surface_form` and
// `knot_spec` are the pin's.
//! ISO 10303-42 geometry entities read into the plain data of [`super::geom`], from a parsed
//! [`Exchange`] in one representation context's [`Units`].
//!
//! Input is hostile: every count, number and reference is checked, nesting is capped
//! ([`MAX_GEOM_DEPTH`]), counts are capped ([`MAX_CTRL`], [`MAX_DEGREE`]) and checked before
//! anything is expanded, numbers must be finite and below [`MAX_COORD`] in size, nothing is
//! allocated ahead from a count in the file, shared entities are read once, and every refusal
//! is a typed [`P21Error`], never a panic.
//!
//! The only arithmetic is unit conversion: a length read times the length unit, an angle times
//! the plane angle unit, a coordinate in a parameter space times that parameter's unit, and the
//! knot counting of B-splines given without knots. Directions keep the ratios the file writes;
//! checks that would need a geometric quantity are the kernel's when it builds from this data.
//! The checks here are on the data alone: a direction that is exactly zero in every component,
//! a vector of magnitude zero, a radius, magnitude or weight of the wrong sign, a knot vector
//! that does not fit its control points.
//!
//! Where the file is outside ISO 10303-42 in a way that has one meaning, it is read: a
//! negative torus major radius (a spindle torus, which some writers emit), a negative cone
//! semi angle (its size is checked), an enumeration value the schema does not list (read as
//! the type's unspecified value).

use std::collections::BTreeMap;
use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use super::geom::{
    Associated, Axis1, BSplineCurve, BSplineSurface, Curve, CurveForm, KnotSpec, Logical, Pcurve,
    Placement, PreferredRep, Surface, SurfaceCurveKind, SurfaceForm, Trim, TrimPreference, Vector,
    P3,
};
use crate::error::{EntityCause, P21Error, Unsupported};
use crate::exchange::{Exchange, Instance, Param};
use crate::units::Units;

/// A result whose error is a [`P21Error`].
type Result<T> = core::result::Result<T, P21Error>;

/// Geometry nesting followed (a trimmed curve of a surface curve of …). Every curve and
/// surface read checks it, so a reference cycle ends here.
pub const MAX_GEOM_DEPTH: usize = 12;
/// Most control points in one B-spline (the whole net for a surface).
pub const MAX_CTRL: usize = 1_000_000;
/// Largest coordinate or length, in file units (exclusive).
pub const MAX_COORD: f64 = 1e9;
/// Highest B-spline degree.
pub const MAX_DEGREE: usize = 32;
/// A cone's semi angle, in radians, must be larger than this in size …
const MIN_SEMI_ANGLE: f64 = 1e-12;
/// … and smaller than this (π/2 less the same margin).
const MAX_SEMI_ANGLE: f64 = FRAC_PI_2 - 1e-12;

/// What a number out of range is refused as needing.
const IN_RANGE: &str = "a finite number smaller than 1e9 in size";

/// How a parameter of a curve or surface scales from file units: angles by the angle unit,
/// lengths by the length unit, anything else (B-spline knots, ratios) not at all.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ParamKind {
    Length,
    Angle,
    Plain,
}

/// The dimension of the space an entity is read in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dim {
    /// A surface's parameter space, or a 2D placement in model space.
    Two,
    /// Model space.
    Three,
}

impl Dim {
    /// The most coordinates a point has.
    const fn count(self) -> usize {
        match self {
            Self::Two => 2,
            Self::Three => 3,
        }
    }
}

/// The space an entity is read in: the unit factors of its coordinates and of its direction
/// ratios, and its dimension.
#[derive(Clone, Copy, Debug)]
struct Space {
    /// Factor per coordinate of a point (and of a vector's components).
    pts: [f64; 3],
    /// Factor per direction ratio: 1 in model space, the parameter's unit in a parameter space.
    dirs: [f64; 3],
    /// Model space, or a parameter space (or 2D placement).
    dim: Dim,
}

impl Space {
    /// Model space in the length unit `l`.
    fn model(l: f64, dim: Dim) -> Self {
        Space {
            pts: [l, l, l],
            dirs: [1.0; 3],
            dim,
        }
    }

    /// A surface's parameter space whose parameters have the factors `s`.
    fn parameters(s: [f64; 3]) -> Self {
        Space {
            pts: s,
            dirs: s,
            dim: Dim::Two,
        }
    }
}

/// Reads geometry from one exchange structure, in one context's units.
pub struct Reader<'a> {
    ex: &'a Exchange,
    units: Units,
    curves: BTreeMap<u64, Arc<Curve>>,
    curves_2d: BTreeMap<(u64, [u64; 2]), Arc<Curve>>,
    surfaces: BTreeMap<u64, Arc<Surface>>,
}

/// `cause`, at `#id`.
fn refuse<T>(id: u64, cause: EntityCause) -> Result<T> {
    Err(P21Error::entity(id, cause))
}

/// An inconsistent or out-of-range value at `#id`.
fn invalid<T>(id: u64, what: &'static str) -> Result<T> {
    refuse(id, EntityCause::Invalid { what })
}

/// Valid input this reader has no mapping for: `#id`, an instance called `name`.
fn unsupported(id: u64, name: &str) -> P21Error {
    P21Error::Unsupported(Unsupported::Entity {
        id,
        name: name.to_owned(),
    })
}

fn entity(ex: &Exchange, id: u64) -> Result<&Instance> {
    ex.get(id)
        .ok_or_else(|| P21Error::entity(id, EntityCause::Missing))
}

/// A number below [`MAX_COORD`] in size, parameter `index` of `#id` (or an item of it).
fn number(id: u64, index: usize, p: Option<&Param>) -> Result<f64> {
    let Some(x) = p.and_then(Param::as_f64) else {
        return refuse(
            id,
            EntityCause::BadParam {
                index,
                expected: "a number",
            },
        );
    };
    if x.is_finite() && x.abs() < MAX_COORD {
        Ok(x)
    } else {
        refuse(
            id,
            EntityCause::BadParam {
                index,
                expected: IN_RANGE,
            },
        )
    }
}

/// A LOGICAL, if `p` is one.
fn logical_of(p: Option<&Param>) -> Option<Logical> {
    match p.and_then(Param::as_enum) {
        Some("T") => Some(Logical::True),
        Some("F") => Some(Logical::False),
        Some("U") => Some(Logical::Unknown),
        _ => None,
    }
}

/// The parameters of one record, read by the positions of the entity's simple form: position
/// `i` is `params[i + add - sub]`, and a refusal names that record position.
#[derive(Clone, Copy)]
struct Fields<'a> {
    id: u64,
    params: &'a [Param],
    add: usize,
    sub: usize,
}

impl<'a> Fields<'a> {
    /// The parameters of `e` (`#id`), as they stand.
    fn of(id: u64, e: &'a Instance) -> Self {
        Fields {
            id,
            params: e.params(),
            add: 0,
            sub: 0,
        }
    }

    /// The record position of simple-form position `i`.
    fn slot(self, i: usize) -> Option<usize> {
        (i + self.add).checked_sub(self.sub)
    }

    fn get(self, i: usize) -> Option<&'a Param> {
        self.params.get(self.slot(i)?)
    }

    /// Simple-form position `i` is not `expected`.
    fn bad(self, i: usize, expected: &'static str) -> P21Error {
        P21Error::entity(
            self.id,
            EntityCause::BadParam {
                index: self.slot(i).unwrap_or(0),
                expected,
            },
        )
    }

    fn reference(self, i: usize) -> Result<u64> {
        self.opt_ref(i)
            .ok_or_else(|| self.bad(i, "an entity reference"))
    }

    fn opt_ref(self, i: usize) -> Option<u64> {
        self.get(i).and_then(Param::as_ref_id)
    }

    fn num(self, i: usize) -> Result<f64> {
        number(self.id, self.slot(i).unwrap_or(0), self.get(i))
    }

    fn list(self, i: usize, expected: &'static str) -> Result<&'a [Param]> {
        self.get(i)
            .and_then(Param::as_list)
            .ok_or_else(|| self.bad(i, expected))
    }

    fn logical(self, i: usize) -> Result<Logical> {
        logical_of(self.get(i)).ok_or_else(|| self.bad(i, "a logical"))
    }

    fn boolean(self, i: usize) -> Result<bool> {
        self.get(i)
            .and_then(Param::as_bool)
            .ok_or_else(|| self.bad(i, "a boolean"))
    }

    fn enumeration(self, i: usize) -> Option<&'a str> {
        self.get(i).and_then(Param::as_enum)
    }
}

/// `x` when it is positive; else the refusal `what` at `#id`.
fn positive(id: u64, x: f64, what: &'static str) -> Result<f64> {
    if x > 0.0 {
        Ok(x)
    } else {
        invalid(id, what)
    }
}

fn curve_form(p: Option<&Param>) -> CurveForm {
    match p.and_then(Param::as_enum) {
        Some("POLYLINE_FORM") => CurveForm::PolylineForm,
        Some("CIRCULAR_ARC") => CurveForm::CircularArc,
        Some("ELLIPTIC_ARC") => CurveForm::EllipticArc,
        Some("PARABOLIC_ARC") => CurveForm::ParabolicArc,
        Some("HYPERBOLIC_ARC") => CurveForm::HyperbolicArc,
        _ => CurveForm::Unspecified,
    }
}

fn surface_form(p: Option<&Param>) -> SurfaceForm {
    match p.and_then(Param::as_enum) {
        Some("PLANE_SURF") => SurfaceForm::PlaneSurf,
        Some("CYLINDRICAL_SURF") => SurfaceForm::CylindricalSurf,
        Some("CONICAL_SURF") => SurfaceForm::ConicalSurf,
        Some("SPHERICAL_SURF") => SurfaceForm::SphericalSurf,
        Some("TOROIDAL_SURF") => SurfaceForm::ToroidalSurf,
        Some("SURF_OF_REVOLUTION") => SurfaceForm::SurfOfRevolution,
        Some("RULED_SURF") => SurfaceForm::RuledSurf,
        Some("GENERALISED_CONE") => SurfaceForm::GeneralisedCone,
        Some("QUADRIC_SURF") => SurfaceForm::QuadricSurf,
        Some("SURF_OF_LINEAR_EXTRUSION") => SurfaceForm::SurfOfLinearExtrusion,
        _ => SurfaceForm::Unspecified,
    }
}

fn knot_spec(p: Option<&Param>) -> KnotSpec {
    match p.and_then(Param::as_enum) {
        Some("UNIFORM_KNOTS") => KnotSpec::UniformKnots,
        Some("QUASI_UNIFORM_KNOTS") => KnotSpec::QuasiUniformKnots,
        Some("PIECEWISE_BEZIER_KNOTS") => KnotSpec::PiecewiseBezierKnots,
        _ => KnotSpec::Unspecified,
    }
}

/// Distinct knots and multiplicities from positions `mi` and `ki` of `f`, checked against
/// `n_ctrl` points of `degree` before anything is expanded (a file can claim multiplicities
/// that would need gigabytes).
fn knots(
    f: Fields<'_>,
    mi: usize,
    ki: usize,
    n_ctrl: usize,
    degree: usize,
) -> Result<(Vec<f64>, Vec<usize>)> {
    let m: Vec<usize> = f
        .list(mi, "a list of knot multiplicities")?
        .iter()
        .map(|x| {
            x.as_i64()
                .filter(|v| *v >= 1 && *v as usize <= degree + 1)
                .map(|v| v as usize)
                .ok_or_else(|| f.bad(mi, "knot multiplicities from 1 to the degree + 1"))
        })
        .collect::<Result<_>>()?;
    let at = f.slot(ki).unwrap_or(0);
    let raw: Vec<f64> = f
        .list(ki, "a list of knots")?
        .iter()
        .map(|x| number(f.id, at, Some(x)))
        .collect::<Result<_>>()?;
    if m.len() != raw.len() || raw.windows(2).any(|w| w.first() > w.get(1)) {
        return invalid(
            f.id,
            "knots that decrease, or not one multiplicity per knot",
        );
    }
    // Some writers list a knot twice; that is one knot with both multiplicities.
    let (mut k, mut mm): (Vec<f64>, Vec<usize>) = (Vec::new(), Vec::new());
    for (x, n) in raw.into_iter().zip(m) {
        match (k.last(), mm.last_mut()) {
            (Some(last), Some(lm)) if *last == x => *lm += n,
            _ => {
                k.push(x);
                mm.push(n);
            }
        }
    }
    let m = mm;
    if k.len() < 2 || m.iter().any(|v| *v > degree + 1) {
        return invalid(
            f.id,
            "a knot vector without two distinct knots, or a knot above the degree + 1",
        );
    }
    if m.iter().try_fold(0usize, |a, v| a.checked_add(*v)) != Some(n_ctrl + degree + 1) {
        return invalid(
            f.id,
            "knot multiplicities that do not match the control points",
        );
    }
    Ok((k, m))
}

/// Knots of a B-spline given by its type alone (`BEZIER_`, `UNIFORM_`, `QUASI_UNIFORM_CURVE`
/// and their surfaces), `#id`: the knot values are whole numbers and the multiplicities
/// counts, laid out from the number of control points and the degree (integer arithmetic
/// only), as ISO 10303-42 defines each knot type.
fn implied_knots(
    id: u64,
    spec: KnotSpec,
    n_ctrl: usize,
    degree: usize,
) -> Result<(Vec<f64>, Vec<usize>)> {
    if n_ctrl <= degree {
        return invalid(id, "no more control points than the degree");
    }
    match spec {
        KnotSpec::PiecewiseBezierKnots => {
            // Segments of `degree`, full interior multiplicity.
            let segs = (n_ctrl - 1) / degree;
            if segs * degree + 1 != n_ctrl {
                return invalid(
                    id,
                    "a Bezier spline whose control points are not the degree times the segments, plus 1",
                );
            }
            let k = (0..=segs).map(|s| s as f64).collect();
            let mut m = vec![degree; segs + 1];
            if let Some(f) = m.first_mut() {
                *f = degree + 1;
            }
            if let Some(l) = m.last_mut() {
                *l = degree + 1;
            }
            Ok((k, m))
        }
        KnotSpec::UniformKnots => {
            let n = n_ctrl + degree + 1;
            // n ≤ MAX_CTRL + MAX_DEGREE + 1, so the counts fit in i64 and convert exactly.
            let k = (0..n).map(|i| (i as i64 - degree as i64) as f64).collect();
            Ok((k, vec![1; n]))
        }
        KnotSpec::QuasiUniformKnots | KnotSpec::Unspecified => {
            // Quasi-uniform: clamped ends, uniform interior.
            let inner = n_ctrl - degree;
            let k = (0..=inner).map(|i| i as f64).collect();
            let mut m = vec![1; inner + 1];
            if let Some(f) = m.first_mut() {
                *f = degree + 1;
            }
            if let Some(l) = m.last_mut() {
                *l = degree + 1;
            }
            Ok((k, m))
        }
    }
}

/// `n` positive weights: the list `p`, parameter `index` of `#id`.
fn weights(id: u64, index: usize, p: Option<&Param>, n: usize) -> Result<Vec<f64>> {
    let Some(list) = p.and_then(Param::as_list) else {
        return refuse(
            id,
            EntityCause::BadParam {
                index,
                expected: "a list of weights",
            },
        );
    };
    let w: Vec<f64> = list
        .iter()
        .map(|w| {
            number(id, index, Some(w)).and_then(|w| {
                if w > 0.0 {
                    Ok(w)
                } else {
                    invalid(id, "a weight that is not positive")
                }
            })
        })
        .collect::<Result<_>>()?;
    if w.len() == n {
        Ok(w)
    } else {
        invalid(id, "weights that do not match the control points")
    }
}

fn degree(f: Fields<'_>, i: usize) -> Result<usize> {
    f.get(i)
        .and_then(Param::as_i64)
        .filter(|d| (1..=MAX_DEGREE as i64).contains(d))
        .map(|d| d as usize)
        .ok_or_else(|| f.bad(i, "a degree from 1 to 32"))
}

/// The knot type a B-spline entity's (sub)type implies when it lists no knots.
fn spec_of_type(e: &Instance) -> KnotSpec {
    if e.has("BEZIER_CURVE") || e.has("BEZIER_SURFACE") {
        KnotSpec::PiecewiseBezierKnots
    } else if e.has("UNIFORM_CURVE") || e.has("UNIFORM_SURFACE") {
        KnotSpec::UniformKnots
    } else {
        KnotSpec::QuasiUniformKnots
    }
}

/// The parameters of a simple or complex B-spline entity `#id`, at the simple form's
/// positions: the name first, then the `B_SPLINE_*` attributes; then, from 0, the
/// `*_WITH_KNOTS` ones when it has them.
fn bspline_params<'a>(
    id: u64,
    e: &'a Instance,
    base: &str,
    with_knots: &str,
) -> Result<(Fields<'a>, Option<Fields<'a>>)> {
    if e.is_complex() {
        let b = e.record(base).ok_or_else(|| unsupported(id, e.name()))?;
        let b = Fields {
            id,
            params: b,
            add: 0,
            sub: 1,
        };
        let k = e.record(with_knots).map(|k| Fields {
            id,
            params: k,
            add: 0,
            sub: 0,
        });
        Ok((b, k))
    } else {
        let b = Fields::of(id, e);
        let n = if base == "B_SPLINE_CURVE" { 6 } else { 8 };
        let k = (e.name() == with_knots).then_some(Fields { add: n, ..b });
        Ok((b, k))
    }
}

impl<'a> Reader<'a> {
    /// A reader of `ex` whose lengths and angles are in `units`.
    pub fn new(ex: &'a Exchange, units: Units) -> Self {
        Reader {
            ex,
            units,
            curves: BTreeMap::new(),
            curves_2d: BTreeMap::new(),
            surfaces: BTreeMap::new(),
        }
    }

    /// The units the reader converts from.
    pub fn units(&self) -> Units {
        self.units
    }

    fn length(&self, f: Fields<'_>, i: usize) -> Result<f64> {
        Ok(f.num(i)? * self.units.len)
    }

    fn angle(&self, f: Fields<'_>, i: usize) -> Result<f64> {
        Ok(f.num(i)? * self.units.ang)
    }

    /// A `CARTESIAN_POINT`, each coordinate times its factor in `sp`.
    fn point_in(&self, id: u64, sp: Space) -> Result<P3> {
        let e = entity(self.ex, id)?;
        if e.name() != "CARTESIAN_POINT" {
            return refuse(
                id,
                EntityCause::WrongType {
                    expected: "CARTESIAN_POINT",
                    found: e.name().to_owned(),
                },
            );
        }
        let f = Fields::of(id, e);
        let c = f.list(1, "a list of coordinates")?;
        if c.is_empty() || c.len() > sp.dim.count() {
            return Err(f.bad(
                1,
                match sp.dim {
                    Dim::Two => "one or two coordinates",
                    Dim::Three => "one to three coordinates",
                },
            ));
        }
        let g = |i: usize| -> Result<f64> {
            if i < c.len() {
                number(id, 1, c.get(i))
            } else {
                Ok(0.0)
            }
        };
        let [sx, sy, sz] = sp.pts;
        Ok([
            g(0)? * sx,
            g(1)? * sy,
            if sp.dim == Dim::Three {
                g(2)? * sz
            } else {
                0.0
            },
        ])
    }

    /// A `CARTESIAN_POINT` in millimetres.
    pub fn point(&self, id: u64) -> Result<P3> {
        self.point_in(id, Space::model(self.units.len, Dim::Three))
    }

    /// The ratios of a `DIRECTION`, each times `s`. Refused when every ratio is exactly zero:
    /// that needs no arithmetic. A direction too short to normalise is the kernel's to refuse.
    fn ratios(&self, id: u64, s: [f64; 3], dim: Dim) -> Result<P3> {
        let e = entity(self.ex, id)?;
        if e.name() != "DIRECTION" {
            return refuse(
                id,
                EntityCause::WrongType {
                    expected: "DIRECTION",
                    found: e.name().to_owned(),
                },
            );
        }
        let f = Fields::of(id, e);
        let c = f.list(1, "a list of direction ratios")?;
        if c.is_empty() || c.len() > dim.count() {
            return Err(f.bad(
                1,
                match dim {
                    Dim::Two => "one or two direction ratios",
                    Dim::Three => "one to three direction ratios",
                },
            ));
        }
        let g = |i: usize| -> Result<f64> {
            if i < c.len() {
                number(id, 1, c.get(i))
            } else {
                Ok(0.0)
            }
        };
        let [sx, sy, sz] = s;
        let d = [
            g(0)? * sx,
            g(1)? * sy,
            if dim == Dim::Three { g(2)? * sz } else { 0.0 },
        ];
        if d == [0.0; 3] {
            return invalid(id, "a direction whose ratios are all zero");
        }
        Ok(d)
    }

    /// A `DIRECTION` in model space: its ratios as written (not normalised).
    pub fn direction(&self, id: u64) -> Result<P3> {
        self.ratios(id, [1.0; 3], Dim::Three)
    }

    /// `AXIS2_PLACEMENT_3D` in 3D, or `AXIS2_PLACEMENT_2D` (no axis) in 2D.
    fn placement_in(&self, id: u64, sp: Space) -> Result<Placement> {
        let e = entity(self.ex, id)?;
        let expected = match sp.dim {
            Dim::Three => "AXIS2_PLACEMENT_3D",
            Dim::Two => "AXIS2_PLACEMENT_2D",
        };
        if e.name() != expected {
            return refuse(
                id,
                EntityCause::WrongType {
                    expected,
                    found: e.name().to_owned(),
                },
            );
        }
        let f = Fields::of(id, e);
        let origin = self.point_in(f.reference(1)?, sp)?;
        let (axis, rf) = match sp.dim {
            Dim::Three => {
                let z = match f.opt_ref(2) {
                    Some(d) => Some(self.ratios(d, sp.dirs, sp.dim)?),
                    None => None,
                };
                (z, f.opt_ref(3))
            }
            Dim::Two => (None, f.opt_ref(2)),
        };
        let ref_direction = match rf {
            Some(d) => Some(self.ratios(d, sp.dirs, sp.dim)?),
            None => None,
        };
        Ok(Placement {
            origin,
            axis,
            ref_direction,
        })
    }

    /// An `AXIS2_PLACEMENT_3D` in millimetres.
    pub fn placement(&self, id: u64) -> Result<Placement> {
        self.placement_in(id, Space::model(self.units.len, Dim::Three))
    }

    /// A placement in 3D space (an assembly's or a mapped item's): `AXIS2_PLACEMENT_3D`, or an
    /// `AXIS2_PLACEMENT_2D` as the placement in the XY plane it describes.
    pub fn frame(&self, id: u64) -> Result<Placement> {
        let dim = if entity(self.ex, id)?.name() == "AXIS2_PLACEMENT_2D" {
            Dim::Two
        } else {
            Dim::Three
        };
        self.placement_in(id, Space::model(self.units.len, dim))
    }

    /// An `AXIS1_PLACEMENT` in millimetres.
    pub fn axis1(&self, id: u64) -> Result<Axis1> {
        let e = entity(self.ex, id)?;
        if e.name() != "AXIS1_PLACEMENT" {
            return refuse(
                id,
                EntityCause::WrongType {
                    expected: "AXIS1_PLACEMENT",
                    found: e.name().to_owned(),
                },
            );
        }
        let f = Fields::of(id, e);
        let origin = self.point(f.reference(1)?)?;
        let direction = match f.opt_ref(2) {
            Some(d) => Some(self.direction(d)?),
            None => None,
        };
        Ok(Axis1 { origin, direction })
    }

    /// A `VECTOR`: its orientation's ratios and its magnitude as written, with the factors of
    /// `sp`. Refused when the magnitude is negative or zero.
    fn vector_in(&self, id: u64, sp: Space) -> Result<Vector> {
        let e = entity(self.ex, id)?;
        if e.name() != "VECTOR" {
            return refuse(
                id,
                EntityCause::WrongType {
                    expected: "VECTOR",
                    found: e.name().to_owned(),
                },
            );
        }
        let f = Fields::of(id, e);
        let orientation = self.ratios(f.reference(1)?, [1.0; 3], sp.dim)?;
        let magnitude = f.num(2)?;
        if magnitude < 0.0 {
            return invalid(id, "a vector magnitude that is negative");
        }
        if magnitude == 0.0 {
            return invalid(id, "a vector of magnitude zero");
        }
        Ok(Vector {
            orientation,
            magnitude,
            scale: sp.pts,
        })
    }

    /// A 3D curve entity.
    pub fn curve(&mut self, id: u64) -> Result<Arc<Curve>> {
        self.curve_in(id, 0)
    }

    fn curve_in(&mut self, id: u64, depth: usize) -> Result<Arc<Curve>> {
        self.curve_at(id, Space::model(self.units.len, Dim::Three), depth)
    }

    /// A curve in `sp`, read once per id and space.
    fn curve_at(&mut self, id: u64, sp: Space, depth: usize) -> Result<Arc<Curve>> {
        if sp.dim == Dim::Three {
            if let Some(c) = self.curves.get(&id) {
                return Ok(c.clone());
            }
            let c = Arc::new(self.read_curve(id, sp, depth)?);
            self.curves.insert(id, c.clone());
            Ok(c)
        } else {
            let [sx, sy, _] = sp.pts;
            let key = (id, [sx.to_bits(), sy.to_bits()]);
            if let Some(c) = self.curves_2d.get(&key) {
                return Ok(c.clone());
            }
            let c = Arc::new(self.read_curve(id, sp, depth)?);
            self.curves_2d.insert(key, c.clone());
            Ok(c)
        }
    }

    /// A 2D curve in a surface's parameter space, scaled by the surface's parameter units.
    fn curve_2d(
        &mut self,
        id: u64,
        kinds: (ParamKind, ParamKind),
        depth: usize,
    ) -> Result<Arc<Curve>> {
        let s = [self.factor(kinds.0), self.factor(kinds.1), 1.0];
        self.curve_at(id, Space::parameters(s), depth)
    }

    fn factor(&self, k: ParamKind) -> f64 {
        match k {
            ParamKind::Length => self.units.len,
            ParamKind::Angle => self.units.ang,
            ParamKind::Plain => 1.0,
        }
    }

    /// The factor of the radii of conic `#id` (called `name`): its space's, which must be the
    /// same on both axes (a conic in a parameter space with an angle on one axis and a length
    /// on the other is not a conic in millimetres and radians).
    fn conic_scale(id: u64, name: &str, sp: Space) -> Result<f64> {
        let [sx, sy, _] = sp.pts;
        if sx != sy {
            return Err(unsupported(id, name));
        }
        Ok(sx)
    }

    fn read_curve(&mut self, id: u64, sp: Space, depth: usize) -> Result<Curve> {
        if depth > MAX_GEOM_DEPTH {
            return refuse(
                id,
                EntityCause::TooDeep {
                    limit: MAX_GEOM_DEPTH,
                },
            );
        }
        let e = entity(self.ex, id)?;
        if e.is_complex() {
            return self.bspline_curve(id, e, sp);
        }
        let f = Fields::of(id, e);
        let name = e.name();
        match name {
            "LINE" => {
                let origin = self.point_in(f.reference(1)?, sp)?;
                let vector = self.vector_in(f.reference(2)?, sp)?;
                Ok(Curve::Line { origin, vector })
            }
            "CIRCLE" => {
                let k = Self::conic_scale(id, name, sp)?;
                let position = self.placement_in(f.reference(1)?, sp)?;
                Ok(Curve::Circle {
                    position,
                    radius: positive(id, f.num(2)? * k, "a circle radius that is not positive")?,
                })
            }
            "ELLIPSE" => {
                let k = Self::conic_scale(id, name, sp)?;
                let position = self.placement_in(f.reference(1)?, sp)?;
                let what = "an ellipse semi axis that is not positive";
                Ok(Curve::Ellipse {
                    position,
                    semi_axis_1: positive(id, f.num(2)? * k, what)?,
                    semi_axis_2: positive(id, f.num(3)? * k, what)?,
                })
            }
            "HYPERBOLA" => {
                let k = Self::conic_scale(id, name, sp)?;
                let position = self.placement_in(f.reference(1)?, sp)?;
                let what = "a hyperbola semi axis that is not positive";
                Ok(Curve::Hyperbola {
                    position,
                    semi_axis: positive(id, f.num(2)? * k, what)?,
                    semi_imag_axis: positive(id, f.num(3)? * k, what)?,
                })
            }
            "PARABOLA" => {
                let k = Self::conic_scale(id, name, sp)?;
                let position = self.placement_in(f.reference(1)?, sp)?;
                let focal_dist = f.num(2)? * k;
                if focal_dist == 0.0 {
                    return invalid(id, "a parabola focal distance of zero");
                }
                Ok(Curve::Parabola {
                    position,
                    focal_dist,
                })
            }
            "B_SPLINE_CURVE_WITH_KNOTS"
            | "BEZIER_CURVE"
            | "UNIFORM_CURVE"
            | "QUASI_UNIFORM_CURVE" => self.bspline_curve(id, e, sp),
            "POLYLINE" => {
                let ids = f.list(1, "a list of 2 to 1 000 000 points")?;
                if ids.len() < 2 || ids.len() > MAX_CTRL {
                    return Err(f.bad(1, "a list of 2 to 1 000 000 points"));
                }
                let pts = ids
                    .iter()
                    .map(|c| {
                        let r = c.as_ref_id().ok_or_else(|| f.bad(1, "point references"))?;
                        self.point_in(r, sp)
                    })
                    .collect::<Result<_>>()?;
                Ok(Curve::Polyline(pts))
            }
            "TRIMMED_CURVE" => {
                let basis = self.curve_at(f.reference(1)?, sp, depth + 1)?;
                // A trimmed curve's parameters are its basis's: angles on a conic, also through
                // an offset, trimmed or surface curve of one; plain numbers otherwise.
                let kind = Self::curve_param_kind(&basis);
                let trim_1 = self.trims(f, 2, kind, sp)?;
                let trim_2 = self.trims(f, 3, kind, sp)?;
                let sense = f.boolean(4)?;
                let preference = match f.enumeration(5) {
                    Some("CARTESIAN") => TrimPreference::Cartesian,
                    Some("PARAMETER") => TrimPreference::Parameter,
                    _ => TrimPreference::Unspecified,
                };
                Ok(Curve::Trimmed {
                    basis,
                    trim_1,
                    trim_2,
                    sense,
                    preference,
                })
            }
            "SURFACE_CURVE" | "SEAM_CURVE" | "INTERSECTION_CURVE" | "BOUNDED_SURFACE_CURVE"
                if sp.dim == Dim::Three =>
            {
                let kind = match name {
                    "SEAM_CURVE" => SurfaceCurveKind::Seam,
                    "INTERSECTION_CURVE" => SurfaceCurveKind::Intersection,
                    "BOUNDED_SURFACE_CURVE" => SurfaceCurveKind::BoundedSurface,
                    _ => SurfaceCurveKind::Surface,
                };
                let curve_3d = self.curve_in(f.reference(1)?, depth + 1)?;
                let items = f.list(2, "a list of associated geometry")?;
                if items.len() > 2 {
                    return invalid(id, "a surface curve with more than two associated items");
                }
                let mut associated = Vec::new();
                for item in items {
                    let r = item
                        .as_ref_id()
                        .ok_or_else(|| f.bad(2, "associated geometry references"))?;
                    associated.push(self.associated(r, depth + 1)?);
                }
                let master = match f.enumeration(3) {
                    Some("PCURVE_S1") => PreferredRep::PcurveS1,
                    Some("PCURVE_S2") => PreferredRep::PcurveS2,
                    _ => PreferredRep::Curve3d,
                };
                Ok(Curve::SurfaceCurve {
                    kind,
                    curve_3d,
                    associated,
                    master,
                })
            }
            "OFFSET_CURVE_3D" if sp.dim == Dim::Three => {
                let basis = self.curve_in(f.reference(1)?, depth + 1)?;
                let distance = self.length(f, 2)?;
                let self_intersect = f.logical(3)?;
                let ref_direction = self.direction(f.reference(4)?)?;
                Ok(Curve::OffsetCurve3d {
                    basis,
                    distance,
                    self_intersect,
                    ref_direction,
                })
            }
            n => Err(unsupported(id, n)),
        }
    }

    /// The trims at position `i` of trimmed curve `f`, parameters in units of `kind`.
    fn trims(&self, f: Fields<'_>, i: usize, kind: ParamKind, sp: Space) -> Result<Vec<Trim>> {
        let items = f.list(i, "a list of one or two trims")?;
        if items.is_empty() || items.len() > 2 {
            return Err(f.bad(i, "a list of one or two trims"));
        }
        let at = f.slot(i).unwrap_or(0);
        items
            .iter()
            .map(|t| match t {
                Param::Ref(r) => Ok(Trim::Point(self.point_in(*r, sp)?)),
                Param::Typed { name, value } if name == "PARAMETER_VALUE" => Ok(Trim::Parameter(
                    number(f.id, at, Some(value))? * self.factor(kind),
                )),
                Param::Real(_) | Param::Int(_) => Ok(Trim::Parameter(
                    number(f.id, at, Some(t))? * self.factor(kind),
                )),
                _ => Err(f.bad(i, "trims that are points or parameter values")),
            })
            .collect()
    }

    /// A `PCURVE`: its surface, and the one 2D curve of its `DEFINITIONAL_REPRESENTATION` in
    /// the surface's parameter units.
    pub fn pcurve(&mut self, id: u64) -> Result<Pcurve> {
        self.pcurve_in(id, 0)
    }

    fn pcurve_in(&mut self, id: u64, depth: usize) -> Result<Pcurve> {
        let e = entity(self.ex, id)?;
        if e.name() != "PCURVE" {
            return refuse(
                id,
                EntityCause::WrongType {
                    expected: "PCURVE",
                    found: e.name().to_owned(),
                },
            );
        }
        let f = Fields::of(id, e);
        let surface = self.surface_in(f.reference(1)?, depth)?;
        let rep_id = f.reference(2)?;
        let rep = entity(self.ex, rep_id)?;
        if rep.name() != "DEFINITIONAL_REPRESENTATION" {
            return refuse(
                rep_id,
                EntityCause::WrongType {
                    expected: "DEFINITIONAL_REPRESENTATION",
                    found: rep.name().to_owned(),
                },
            );
        }
        let rf = Fields::of(rep_id, rep);
        let items = rf.list(1, "a list of representation items")?;
        let [item] = items else {
            return invalid(rep_id, "a pcurve representation without exactly one item");
        };
        let c = item
            .as_ref_id()
            .ok_or_else(|| rf.bad(1, "a curve reference"))?;
        let kinds = Self::param_kinds(&surface);
        let curve = self.curve_2d(c, kinds, depth)?;
        Ok(Pcurve { surface, curve })
    }

    fn associated(&mut self, id: u64, depth: usize) -> Result<Associated> {
        if entity(self.ex, id)?.name() == "PCURVE" {
            Ok(Associated::Pcurve(self.pcurve_in(id, depth)?))
        } else {
            Ok(Associated::Surface(self.surface_in(id, depth)?))
        }
    }

    /// How a surface's (u, v) parameters scale from file units. Recursion follows the
    /// surface's own basis and profile links, which reading bounded by [`MAX_GEOM_DEPTH`].
    fn param_kinds(s: &Surface) -> (ParamKind, ParamKind) {
        use ParamKind::{Angle, Length, Plain};
        match s {
            Surface::Plane { .. } => (Length, Length),
            Surface::Cylinder { .. } | Surface::Cone { .. } => (Angle, Length),
            Surface::Sphere { .. } | Surface::Torus { .. } | Surface::DegenerateTorus { .. } => {
                (Angle, Angle)
            }
            Surface::Revolution { profile, .. } => (Angle, Self::curve_param_kind(profile)),
            Surface::Extrusion { profile, .. } => (Self::curve_param_kind(profile), Plain),
            Surface::BSpline(_) => (Plain, Plain),
            Surface::Offset { basis, .. } | Surface::RectangularTrimmed { basis, .. } => {
                Self::param_kinds(basis)
            }
        }
    }

    /// How a curve's parameter scales from file units (bounded like [`Self::param_kinds`]).
    fn curve_param_kind(c: &Curve) -> ParamKind {
        match c {
            Curve::Circle { .. } | Curve::Ellipse { .. } => ParamKind::Angle,
            Curve::Trimmed { basis, .. } | Curve::OffsetCurve3d { basis, .. } => {
                Self::curve_param_kind(basis)
            }
            Curve::SurfaceCurve { curve_3d, .. } => Self::curve_param_kind(curve_3d),
            _ => ParamKind::Plain,
        }
    }

    fn bspline_curve(&mut self, id: u64, e: &Instance, sp: Space) -> Result<Curve> {
        let (base, with_knots) =
            bspline_params(id, e, "B_SPLINE_CURVE", "B_SPLINE_CURVE_WITH_KNOTS")?;
        let degree = degree(base, 1)?;
        let ids = base.list(2, "a list of control points")?;
        if ids.len() > MAX_CTRL {
            return invalid(id, "a B-spline with more than 1 000 000 control points");
        }
        let points: Vec<P3> = ids
            .iter()
            .map(|c| {
                let r = c
                    .as_ref_id()
                    .ok_or_else(|| base.bad(2, "control point references"))?;
                self.point_in(r, sp)
            })
            .collect::<Result<_>>()?;
        let (knots, multiplicities, knot_spec) = if let Some(k) = with_knots {
            let (kv, m) = knots(k, 0, 1, points.len(), degree)?;
            (kv, m, knot_spec(k.get(2)))
        } else {
            let spec = spec_of_type(e);
            let (kv, m) = implied_knots(id, spec, points.len(), degree)?;
            (kv, m, spec)
        };
        let weights = match e.record("RATIONAL_B_SPLINE_CURVE") {
            Some(r) => Some(weights(id, 0, r.first(), points.len())?),
            None => None,
        };
        Ok(Curve::BSpline(Box::new(BSplineCurve {
            degree,
            points,
            weights,
            knots,
            multiplicities,
            form: curve_form(base.get(3)),
            closed: logical_of(base.get(4)).unwrap_or(Logical::Unknown),
            self_intersect: logical_of(base.get(5)).unwrap_or(Logical::Unknown),
            knot_spec,
        })))
    }

    /// A surface entity.
    pub fn surface(&mut self, id: u64) -> Result<Arc<Surface>> {
        self.surface_in(id, 0)
    }

    fn surface_in(&mut self, id: u64, depth: usize) -> Result<Arc<Surface>> {
        if let Some(s) = self.surfaces.get(&id) {
            return Ok(s.clone());
        }
        if depth > MAX_GEOM_DEPTH {
            return refuse(
                id,
                EntityCause::TooDeep {
                    limit: MAX_GEOM_DEPTH,
                },
            );
        }
        let s = Arc::new(self.read_surface(id, depth)?);
        self.surfaces.insert(id, s.clone());
        Ok(s)
    }

    fn read_surface(&mut self, id: u64, depth: usize) -> Result<Surface> {
        let e = entity(self.ex, id)?;
        if e.is_complex() {
            return self.bspline_surface(id, e);
        }
        let f = Fields::of(id, e);
        let position = |r: &Self| r.placement(f.reference(1)?);
        match e.name() {
            "PLANE" => Ok(Surface::Plane {
                position: position(self)?,
            }),
            "CYLINDRICAL_SURFACE" => Ok(Surface::Cylinder {
                position: position(self)?,
                radius: positive(
                    id,
                    self.length(f, 2)?,
                    "a cylinder radius that is not positive",
                )?,
            }),
            "CONICAL_SURFACE" => {
                let radius = self.length(f, 2)?;
                let semi_angle = self.angle(f, 3)?;
                if radius < 0.0
                    || !(semi_angle.abs() > MIN_SEMI_ANGLE && semi_angle.abs() < MAX_SEMI_ANGLE)
                {
                    return invalid(
                        id,
                        "a cone with a negative radius or a semi angle outside (0, π/2) in size",
                    );
                }
                Ok(Surface::Cone {
                    position: position(self)?,
                    radius,
                    semi_angle,
                })
            }
            "SPHERICAL_SURFACE" => Ok(Surface::Sphere {
                position: position(self)?,
                radius: positive(
                    id,
                    self.length(f, 2)?,
                    "a sphere radius that is not positive",
                )?,
            }),
            "TOROIDAL_SURFACE" | "DEGENERATE_TOROIDAL_SURFACE" => {
                let major_radius = self.length(f, 2)?;
                let minor_radius = positive(
                    id,
                    self.length(f, 3)?,
                    "a torus minor radius that is not positive",
                )?;
                // A negative major radius (outside the schema, which some writers use for a
                // spindle torus) is the same formula: its tube on the other side of the axis.
                if !major_radius.is_finite() {
                    return invalid(id, "a torus major radius that is not finite");
                }
                let position = position(self)?;
                if e.name() == "DEGENERATE_TOROIDAL_SURFACE" {
                    let select_outer = f.boolean(4)?;
                    Ok(Surface::DegenerateTorus {
                        position,
                        major_radius,
                        minor_radius,
                        select_outer,
                    })
                } else {
                    Ok(Surface::Torus {
                        position,
                        major_radius,
                        minor_radius,
                    })
                }
            }
            "B_SPLINE_SURFACE_WITH_KNOTS"
            | "BEZIER_SURFACE"
            | "UNIFORM_SURFACE"
            | "QUASI_UNIFORM_SURFACE" => self.bspline_surface(id, e),
            "SURFACE_OF_REVOLUTION" => {
                let profile = self.curve_in(f.reference(1)?, depth + 1)?;
                let axis = self.axis1(f.reference(2)?)?;
                Ok(Surface::Revolution { profile, axis })
            }
            "SURFACE_OF_LINEAR_EXTRUSION" => {
                let profile = self.curve_in(f.reference(1)?, depth + 1)?;
                let vector =
                    self.vector_in(f.reference(2)?, Space::model(self.units.len, Dim::Three))?;
                Ok(Surface::Extrusion { profile, vector })
            }
            "OFFSET_SURFACE" => {
                let basis = self.surface_in(f.reference(1)?, depth + 1)?;
                let distance = self.length(f, 2)?;
                let self_intersect = f.logical(3)?;
                Ok(Surface::Offset {
                    basis,
                    distance,
                    self_intersect,
                })
            }
            "RECTANGULAR_TRIMMED_SURFACE" => {
                let basis = self.surface_in(f.reference(1)?, depth + 1)?;
                let (ku, kv) = Self::param_kinds(&basis);
                let (fu, fv) = (self.factor(ku), self.factor(kv));
                Ok(Surface::RectangularTrimmed {
                    u1: f.num(2)? * fu,
                    u2: f.num(3)? * fu,
                    v1: f.num(4)? * fv,
                    v2: f.num(5)? * fv,
                    u_sense: f.boolean(6)?,
                    v_sense: f.boolean(7)?,
                    basis,
                })
            }
            n => Err(unsupported(id, n)),
        }
    }

    fn bspline_surface(&mut self, id: u64, e: &Instance) -> Result<Surface> {
        let (base, with_knots) =
            bspline_params(id, e, "B_SPLINE_SURFACE", "B_SPLINE_SURFACE_WITH_KNOTS")?;
        let (u_degree, v_degree) = (degree(base, 1)?, degree(base, 2)?);
        let rows = base.list(3, "a list of control point rows")?;
        let mut points: Vec<Vec<P3>> = Vec::new();
        let mut total = 0usize;
        for r in rows {
            let r = r
                .as_list()
                .ok_or_else(|| base.bad(3, "a list of control point rows"))?;
            total = total.saturating_add(r.len());
            if total > MAX_CTRL {
                return invalid(
                    id,
                    "a B-spline surface with more than 1 000 000 control points",
                );
            }
            points.push(
                r.iter()
                    .map(|c| {
                        let p = c
                            .as_ref_id()
                            .ok_or_else(|| base.bad(3, "control point references"))?;
                        self.point(p)
                    })
                    .collect::<Result<_>>()?,
            );
        }
        let nu = points.len();
        let nv = points.first().map_or(0, Vec::len);
        if nu == 0 || nv == 0 || points.iter().any(|r| r.len() != nv) {
            return invalid(id, "a B-spline control net that is empty or ragged");
        }
        let ((u_knots, u_multiplicities), (v_knots, v_multiplicities), knot_spec) =
            if let Some(k) = with_knots {
                (
                    knots(k, 0, 2, nu, u_degree)?,
                    knots(k, 1, 3, nv, v_degree)?,
                    knot_spec(k.get(4)),
                )
            } else {
                let spec = spec_of_type(e);
                (
                    implied_knots(id, spec, nu, u_degree)?,
                    implied_knots(id, spec, nv, v_degree)?,
                    spec,
                )
            };
        let weights = match e.record("RATIONAL_B_SPLINE_SURFACE") {
            Some(r) => {
                let Some(w) = r.first().and_then(Param::as_list) else {
                    return refuse(
                        id,
                        EntityCause::BadParam {
                            index: 0,
                            expected: "a list of weight rows",
                        },
                    );
                };
                if w.len() != nu {
                    return invalid(id, "weights that do not match the control net");
                }
                Some(
                    w.iter()
                        .map(|row| weights(id, 0, Some(row), nv))
                        .collect::<Result<_>>()?,
                )
            }
            None => None,
        };
        Ok(Surface::BSpline(Box::new(BSplineSurface {
            u_degree,
            v_degree,
            points,
            weights,
            u_knots,
            u_multiplicities,
            v_knots,
            v_multiplicities,
            form: surface_form(base.get(4)),
            u_closed: logical_of(base.get(5)).unwrap_or(Logical::Unknown),
            v_closed: logical_of(base.get(6)).unwrap_or(Logical::Unknown),
            self_intersect: logical_of(base.get(7)).unwrap_or(Logical::Unknown),
            knot_spec,
        })))
    }
}
