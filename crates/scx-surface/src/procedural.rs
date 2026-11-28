// Written for solidcorex. Apache-2.0.
//! [`Procedural`]: a surface held as its definition (a generator curve revolved about an
//! axis, extruded along a vector, or swept by a screw motion) and the rational net of that
//! definition, built once per value (research 75 §13.27, §13.29).
//!
//! The definition is the surface: [`Procedural::point`], [`Procedural::jet`] and
//! [`Procedural::eval`] (any [`Ranged`] scalar: `f64` and intervals) read it in closed form.
//! The net is the definition's exact NURBS image, its control data rounded once: every
//! point of the net over its knots' box is within [`Procedural::bound`] of the definition's
//! point at the same `uv` (the rounding of the control data only; each side's own
//! evaluation rounds on top). The calls that need control points (certified boxes and
//! hulls, inversion, `to_nurbs`) read the net inside its box, its bound added where the
//! answer must hold the definition ([`Procedural::net_over`]); outside the box they read
//! the definition.
//!
//! A revolution's and an extrusion's net is the definition's exact NURBS image. A sweep's
//! ([`Sweep`], its [`Motion::Screw`]) is not: its turn is cut into spans at its stations,
//! each an exact rational arc of the turn but not of the lift or the radial law, and its
//! bound adds the certified gap of that approximation ([`Screw::approx`]) to the
//! rounding, so `to_nurbs` refuses it ([`ProceduralError::Inexact`]).
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `SurfOfRevolution` / `SurfOfExtrusion`: the generator, the axis or direction, and a NURB rebuilt on demand under a per-call `NetScope` | [`Definition::Revolution`] / [`Definition::Extrusion`]: the generator's data and the net, built once in [`Procedural::new`], read by every query |
//! | the NURB built from the generator's NURB, its rounding unstated | the net built from exact control data in `f64` and, alongside, in intervals; the gap is [`Procedural::bound`] |
//! | `ProceduralSweep` beside the B-spline wall `sweep::screw` builds, riding on a `Nurbs` surface | [`Definition::Sweep`]: its own arm, the net built from the definition in [`Procedural::new`] |
//! | the generator any curve | a line, a circle or ellipse over whole quarter arcs, or a NURBS curve over its domain; a parabola or hyperbola is [`ProceduralError::Generator`] for now |
//!
//! # The uv
//!
//! A revolution's `u` is the turn about the axis in the pin's quarter-arc parameter
//! (63 D13), periodic over `[0, 1]`, `u = 0` at the generator itself; its `v` is the
//! generator's parameter. An extrusion's `u` is the generator's parameter and its `v`
//! multiplies the extrusion vector. A line's and a NURBS curve's parameter is the curve's
//! own; a circle's or ellipse's is the quarter-arc parameter of its angle (a turn over
//! `[0, 1]`, as the torus's `v`), not the curve's radians, so the net is exact in the same
//! uv.
//!
//! A sweep's `u` is the net's parameter across the turn in radians, bounded by its first and
//! last station ([`Screw::turn_at`] is the definition's angle there, equal at the stations);
//! its `v` is the generator's parameter.

use core::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2};
use core::fmt;
use std::sync::Arc;

use scx_curve::Curve;
use scx_curve::arcs::Arcs;
use scx_num::domain::{ParamDomain, UvDomain};
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::interval::Ivl;
use scx_num::jet::{CurveJet, SurfaceJet};
use scx_num::math::Affine;
use scx_num::ring::Ring;
use scx_num::trig::{Ranged, Trig};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::{Approach, KnotVector, NurbsCurve};
use scx_nurbs_surface::{Net, NurbsSurface, NurbsSurfaceError, ParamAxis};

use crate::edit::{UvMap, similar};
use crate::error::SurfaceError;
use crate::invert::{self, Closest, Inversion, Periods};

mod screw;

pub use screw::{RadialLaw, Screw};

/// The turn about a revolution's axis, and a conic generator's angle: four quarter arcs over
/// `[0, 1]`.
pub(crate) const TURN: Arcs = Arcs::turn(1.0);

/// The roundings within which a generator is taken to lie on the axis, or a line to run
/// along the extrusion.
const FLAT: f64 = 16.0 * f64::EPSILON;

/// The most quarter arcs a covering net (`to_nurbs` past the base turn) may span.
const MAX_QUARTERS: i64 = 8;

/// The most control points of a sweep's net.
const MAX_NET_POINTS: usize = 4_000_000;

/// The most Newton steps polishing a sweep's foot on its definition.
const POLISH_STEPS: usize = 8;

// ---------------------------------------------------------------- errors

/// Why a procedural surface cannot be built. Errors follow research 62 §10: a kind, a
/// stable name, a one-frame Display.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ProceduralError {
    /// The generator is a kind not built yet (a parabola or hyperbola branch). `Unsupported`.
    Generator {
        /// The kind's name.
        kind: &'static str,
    },
    /// The generator's range (or an extrusion's `v` range) is a single value, not finite,
    /// not whole quarter arcs of a conic (at most a turn), or not a NURBS curve's domain.
    /// `Refused`.
    Range {
        /// Its low end.
        min: f64,
        /// Its high end.
        max: f64,
    },
    /// The axis point, the axis or the extrusion vector is not finite, or the net's bound
    /// is not. `Refused`.
    NotFinite,
    /// The axis direction or the extrusion vector is zero. `Refused`.
    ZeroDirection,
    /// Every point of the generator lies on the axis: the revolution is a line. `Refused`.
    OnAxis,
    /// The extrusion runs along the generator's line or in the conic's plane: the surface
    /// is flat or folded. `Refused`.
    Flat,
    /// The net could not be built (overflow, for sizes near the largest double). A context
    /// frame: the cause through [`std::error::Error::source`].
    Net(NurbsSurfaceError),
    /// A radial law with no points, points of different counts, a value not finite, angles
    /// not strictly ascending, or a direction that is zero or not square to the axis.
    /// `Refused`.
    Law,
    /// A screw of no pitch and no law: that is a [`Revolution`]. `Refused`.
    ZeroPitch,
    /// The resolution needs more spans of a screw's turn, or more control points of its
    /// net, than are built. `Exhausted`.
    TooFine {
        /// The spans the turn needed when it stopped.
        spans: usize,
    },
    /// A sweep's net is not its definition's exact image: `to_nurbs` refuses it.
    /// `Unsupported`.
    Inexact {
        /// The net's bound.
        bound: f64,
    },
}

impl ProceduralError {
    /// The kind (62 §10).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Generator { .. } | Self::Inexact { .. } => ErrorKind::Unsupported,
            Self::Range { .. }
            | Self::NotFinite
            | Self::ZeroDirection
            | Self::OnAxis
            | Self::Flat
            | Self::Law
            | Self::ZeroPitch => ErrorKind::Refused,
            Self::TooFine { .. } => ErrorKind::Exhausted,
            Self::Net(e) => e.kind(),
        }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Generator { .. } => "procedural_generator_unsupported",
            Self::Range { .. } => "procedural_range",
            Self::NotFinite => "procedural_not_finite",
            Self::ZeroDirection => "procedural_zero_direction",
            Self::OnAxis => "procedural_on_axis",
            Self::Flat => "procedural_flat",
            Self::Net(_) => "procedural_net",
            Self::Law => "procedural_law",
            Self::ZeroPitch => "procedural_zero_pitch",
            Self::TooFine { .. } => "procedural_too_fine",
            Self::Inexact { .. } => "procedural_not_exact",
        }
    }
}

impl fmt::Display for ProceduralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Generator { kind } => {
                write!(
                    f,
                    "a {kind} generator is not supported for a procedural surface"
                )
            }
            Self::Range { min, max } => write!(
                f,
                "the range [{min}, {max}] is not a usable range of the generator or extrusion"
            ),
            Self::NotFinite => f.write_str("the axis, extrusion vector or net is not finite"),
            Self::ZeroDirection => f.write_str("the axis or extrusion vector is zero"),
            Self::OnAxis => f.write_str("the generator lies on the axis of revolution"),
            Self::Flat => f.write_str("the extrusion runs within the generator's line or plane"),
            Self::Net(_) => f.write_str("the procedural surface's net could not be built"),
            Self::Law => f.write_str("the radial law is not a usable law for the screw"),
            Self::ZeroPitch => f.write_str("a screw with no pitch and no law is a revolution"),
            Self::TooFine { spans } => {
                write!(
                    f,
                    "the resolution needs {spans} or more spans of the screw's turn"
                )
            }
            Self::Inexact { bound } => write!(
                f,
                "the sweep's net is within {bound} of its definition, not its exact image"
            ),
        }
    }
}

impl std::error::Error for ProceduralError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Net(e) => Some(e),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- the generator

/// A generator in closed form, evaluable over any [`Ring`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Closed {
    /// `o + d t`.
    Line { o: Point3, d: Vec3 },
    /// `o + a cos w(t) + b sin w(t)`, `w` the quarter-arc angle of [`TURN`]; `a` and `b` are
    /// conjugate semi-diameters (square and of one length for a circle).
    Conic { o: Point3, a: Vec3, b: Vec3 },
}

impl Closed {
    fn point<T: Ring>(&self, t: T, near: f64) -> Point3<T> {
        match *self {
            Self::Line { o, d } => o.map(T::lift) + d.map(T::lift) * t,
            Self::Conic { o, a, b } => {
                let (c, s) = TURN.cos_sin(t, TURN.piece(near, Approach::Above));
                o.map(T::lift) + a.map(T::lift) * c + b.map(T::lift) * s
            }
        }
    }

    fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D> {
        match *self {
            Self::Line { o, d } => CurveJet {
                point: o + d * t,
                d: core::array::from_fn(|k| if k == 0 { d } else { Vec3::splat(0.0) }),
            },
            Self::Conic { o, a, b } => {
                let tr = TURN.trig::<D>(t, side);
                let (c, s) = tr.value;
                CurveJet {
                    point: o + a * c + b * s,
                    d: core::array::from_fn(|k| {
                        let (c, s) = tr.nth(k + 1);
                        a * c + b * s
                    }),
                }
            }
        }
    }

    fn reversed(self) -> Self {
        match self {
            Self::Line { o, d } => Self::Line { o, d: -d },
            Self::Conic { o, a, b } => Self::Conic { o, a, b: -b },
        }
    }

    fn transformed(self, m: &Affine) -> Self {
        match self {
            Self::Line { o, d } => Self::Line {
                o: m.apply(o),
                d: m.apply_vec(d),
            },
            Self::Conic { o, a, b } => Self::Conic {
                o: m.apply(o),
                a: m.apply_vec(a),
                b: m.apply_vec(b),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Shape {
    Closed(Closed),
    Nurbs(Arc<NurbsCurve<3>>),
}

/// A generator over its range: for a conic, whole quarter arcs `[k0 / 4, k1 / 4]` of the
/// quarter-arc parameter; for a NURBS curve, its domain.
#[derive(Debug, Clone, PartialEq)]
struct Profile {
    shape: Shape,
    range: Extent1d,
}

/// The control data of a generator, or of the turn about an axis, in `T`.
struct Controls<T> {
    degree: usize,
    knots: Vec<f64>,
    pts: Vec<Point3<T>>,
    w: Vec<T>,
}

/// The cosine and sine of `k` quarter turns, exactly.
const fn quarter_turn(k: i64) -> (f64, f64) {
    match k.rem_euclid(4) {
        0 => (1.0, 0.0),
        1 => (0.0, 1.0),
        2 => (-1.0, 0.0),
        _ => (0.0, -1.0),
    }
}

/// The standard rational quarter arcs `k0..k1` of the unit circle (63 D13): breaks at the
/// exact quarter points with weight 1, corners at their sums with weight `cos(pi/4)`
/// (`root_half`, its enclosure in intervals); knots `k / 4`, doubled inside.
fn turn_controls<T: Ring>(k0: i64, k1: i64, root_half: T) -> (Vec<f64>, Vec<(T, T, T)>) {
    let at = |k: i64| k as f64 / 4.0;
    let mut knots = vec![at(k0); 3];
    let mut ctrl = Vec::with_capacity(2 * (k1 - k0).unsigned_abs() as usize + 1);
    let one = T::lift(1.0);
    for k in k0..k1 {
        let (c0, s0) = quarter_turn(k);
        let (c1, s1) = quarter_turn(k + 1);
        ctrl.push((T::lift(c0), T::lift(s0), one));
        ctrl.push((T::lift(c0 + c1), T::lift(s0 + s1), root_half));
        if k + 1 < k1 {
            knots.extend([at(k + 1); 2]);
        }
    }
    let (c, s) = quarter_turn(k1);
    ctrl.push((T::lift(c), T::lift(s), one));
    knots.extend([at(k1); 3]);
    (knots, ctrl)
}

/// The quarter arcs `t` (radians) is a whole number of, to four roundings.
fn quarters_of(t: f64) -> Option<i64> {
    let k = (t / FRAC_PI_2).round();
    let ok = k.abs() < 1e9 && (t - k * FRAC_PI_2).abs() <= 4.0 * f64::EPSILON * t.abs().max(1.0);
    ok.then_some(k as i64)
}

impl Profile {
    /// The generator `curve` over `range` (in the curve's own parameter: radians for a
    /// conic).
    fn new(curve: &Curve, range: Extent1d) -> Result<Self, ProceduralError> {
        let (min, max) = (range.min(), range.max());
        let bad = ProceduralError::Range { min, max };
        if !(min.is_finite() && max.is_finite()) || min >= max {
            return Err(bad);
        }
        let conic = |o: Point3, a: Vec3, b: Vec3| {
            let (k0, k1) = (quarters_of(min).ok_or(bad)?, quarters_of(max).ok_or(bad)?);
            if !(1..=4).contains(&(k1 - k0)) {
                return Err(bad);
            }
            let range = Extent1d::new(k0 as f64 / 4.0, k1 as f64 / 4.0).ok_or(bad)?;
            Ok(Self {
                shape: Shape::Closed(Closed::Conic { o, a, b }),
                range,
            })
        };
        match curve {
            Curve::Line(l) => Ok(Self {
                shape: Shape::Closed(Closed::Line {
                    o: l.origin(),
                    d: l.direction(),
                }),
                range,
            }),
            Curve::Circle(c) => {
                let (f, r) = (c.frame(), c.radius().mm());
                let b = r * c.sense().sign();
                conic(f.origin(), f.x().as_vec() * r, f.y().as_vec() * b)
            }
            Curve::Ellipse(e) => {
                let f = e.frame();
                let b = e.semi_y().mm() * e.sense().sign();
                conic(
                    f.origin(),
                    f.x().as_vec() * e.semi_x().mm(),
                    f.y().as_vec() * b,
                )
            }
            Curve::Nurbs(n) => {
                if !n.domain().are_same(&range) {
                    return Err(bad);
                }
                Ok(Self {
                    shape: Shape::Nurbs(Arc::clone(n)),
                    range,
                })
            }
            Curve::Parabola(_) => Err(ProceduralError::Generator { kind: "parabola" }),
            Curve::Hyperbola(_) => Err(ProceduralError::Generator { kind: "hyperbola" }),
            _ => Err(ProceduralError::Generator { kind: "curve" }),
        }
    }

    /// The quarter arcs of a conic's range.
    fn quarters(&self) -> Option<(i64, i64)> {
        match self.shape {
            Shape::Closed(Closed::Conic { .. }) => Some((
                (4.0 * self.range.min()) as i64,
                (4.0 * self.range.max()) as i64,
            )),
            _ => None,
        }
    }

    /// The direction's domain: periodic for a conic's whole turn, else the range.
    fn domain(&self) -> ParamDomain {
        match self.quarters() {
            Some((k0, k1)) if k1 - k0 == 4 => ParamDomain::periodic(1.0, self.range.min())
                .unwrap_or(ParamDomain::Bounded(self.range)),
            _ => ParamDomain::Bounded(self.range),
        }
    }

    /// The arcs a box over this direction is cut at.
    fn arcs(&self) -> Option<Arcs> {
        self.quarters().map(|_| TURN)
    }

    fn point<T: Ranged>(&self, t: T, near: f64) -> Point3<T> {
        match &self.shape {
            Shape::Closed(c) => c.point(t, near),
            Shape::Nurbs(n) => Point3::from_array(n.point_over(t, Approach::Above)),
        }
    }

    fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D> {
        match &self.shape {
            Shape::Closed(c) => c.jet(t, side),
            Shape::Nurbs(n) => match n.jet::<D>(t, side) {
                Ok((p, d)) => CurveJet {
                    point: Point3::from_array(p),
                    d: d.map(Vec3::from_array),
                },
                Err(_) => CurveJet {
                    point: Point3::new(f64::NAN, f64::NAN, f64::NAN),
                    d: [Vec3::splat(f64::NAN); D],
                },
            },
        }
    }

    /// The control data over the range, or (a conic) over the quarter arcs `quarters`.
    fn controls<T: Ring>(&self, root_half: T, quarters: Option<(i64, i64)>) -> Controls<T> {
        match &self.shape {
            Shape::Closed(Closed::Line { o, d }) => {
                let (lo, hi) = (self.range.min(), self.range.max());
                let at = |t: f64| o.map(T::lift) + d.map(T::lift) * T::lift(t);
                Controls {
                    degree: 1,
                    knots: vec![lo, lo, hi, hi],
                    pts: vec![at(lo), at(hi)],
                    w: vec![T::lift(1.0); 2],
                }
            }
            Shape::Closed(Closed::Conic { o, a, b }) => {
                let (k0, k1) = quarters.or_else(|| self.quarters()).unwrap_or((0, 4));
                let (knots, ctrl) = turn_controls(k0, k1, root_half);
                let (o, a, b) = (o.map(T::lift), a.map(T::lift), b.map(T::lift));
                Controls {
                    degree: 2,
                    knots,
                    pts: ctrl.iter().map(|&(c, s, _)| o + a * c + b * s).collect(),
                    w: ctrl.iter().map(|&(_, _, w)| w).collect(),
                }
            }
            Shape::Nurbs(n) => Controls {
                degree: n.degree(),
                knots: n.knots().to_vec(),
                pts: n
                    .points()
                    .iter()
                    .map(|p| Point3::from_array(*p).map(T::lift))
                    .collect(),
                w: n.weights().map_or_else(
                    || vec![T::lift(1.0); n.points().len()],
                    |w| w.iter().map(|&w| T::lift(w)).collect(),
                ),
            },
        }
    }

    /// The same generator run the other way, `G'(-t) = G(t)`, exactly.
    fn reversed(&self) -> Self {
        let shape = match &self.shape {
            Shape::Closed(c) => Shape::Closed(c.reversed()),
            Shape::Nurbs(n) => Shape::Nurbs(Arc::new(n.reversed())),
        };
        let range = Extent1d::new(-self.range.max(), -self.range.min()).unwrap_or(self.range);
        Self { shape, range }
    }

    /// The image under `m`: the control data mapped, the parameter kept.
    fn transformed(&self, m: &Affine) -> Result<Self, SurfaceError> {
        let shape = match &self.shape {
            Shape::Closed(c) => Shape::Closed(c.transformed(m)),
            Shape::Nurbs(n) => {
                let pts = n
                    .points()
                    .iter()
                    .map(|&p| m.apply(Point3::from_array(p)).to_array())
                    .collect();
                let w = n.weights().map(<[f64]>::to_vec);
                let c = NurbsCurve::new(n.knot_vector().clone(), pts, w)
                    .map_err(|_| SurfaceError::MapNotFinite)?;
                Shape::Nurbs(Arc::new(c))
            }
        };
        Ok(Self {
            shape,
            range: self.range,
        })
    }
}

// ---------------------------------------------------------------- definitions

/// A generator revolved a whole turn about an axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Revolution {
    profile: Profile,
    origin: Point3,
    axis: UnitVec3,
}

/// A generator extruded along a vector: `S(u, v) = G(u) + v along`.
#[derive(Debug, Clone, PartialEq)]
pub struct Extrusion {
    profile: Profile,
    along: Vec3,
    v: Extent1d,
}

/// How a sweep's profile moves. A screw now; a path motion joins as a variant.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Motion {
    /// A screw about an axis, with a radial law.
    Screw(Screw),
}

/// A generator carried by a motion: `S(u, v)` is the generator's point at `v` moved by the
/// motion at `u`.
#[derive(Debug, Clone, PartialEq)]
pub struct Sweep {
    profile: Profile,
    motion: Motion,
}

/// What a procedural surface is. Revolution, extrusion and sweep now; the other procedural
/// kinds join as variants.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Definition {
    /// A surface of revolution.
    Revolution(Revolution),
    /// A surface of linear extrusion.
    Extrusion(Extrusion),
    /// A swept surface.
    Sweep(Sweep),
}

impl Revolution {
    /// `generator` over `range` (its own parameter; radians for a circle or ellipse, whole
    /// quarter turns) revolved about the line through `origin` along `axis`, `u` turning
    /// right-handed about `axis`.
    ///
    /// # Errors
    ///
    /// [`ProceduralError::Generator`] (`Unsupported`) for a parabola or hyperbola;
    /// `Refused`: [`ProceduralError::Range`], [`ProceduralError::NotFinite`],
    /// [`ProceduralError::ZeroDirection`] for an axis too short to point, and
    /// [`ProceduralError::OnAxis`] where every control point of the generator is on the axis.
    pub fn new(
        generator: &Curve,
        range: Extent1d,
        origin: Point3,
        axis: Vec3,
    ) -> Result<Self, ProceduralError> {
        if !(origin.is_finite() && axis.is_finite()) {
            return Err(ProceduralError::NotFinite);
        }
        let axis = axis.normalized().ok_or(ProceduralError::ZeroDirection)?;
        let r = Self {
            profile: Profile::new(generator, range)?,
            origin,
            axis,
        };
        let c = r.profile.controls(FRAC_1_SQRT_2, None);
        let (mut off, mut far) = (0.0_f64, 0.0_f64);
        for &g in &c.pts {
            let w = g - origin;
            off = off.max(r.radial(w).length());
            far = far.max(w.length());
        }
        if off <= FLAT * far {
            return Err(ProceduralError::OnAxis);
        }
        Ok(r)
    }

    /// A point of the axis.
    #[must_use]
    pub const fn origin(&self) -> Point3 {
        self.origin
    }

    /// The axis: `u` turns right-handed about it.
    #[must_use]
    pub const fn axis(&self) -> UnitVec3 {
        self.axis
    }

    /// The generator's range in the surface's `v`.
    #[must_use]
    pub const fn range(&self) -> Extent1d {
        self.profile.range
    }

    /// The part of `w` square to the axis.
    fn radial<T: Ring>(&self, w: Vec3<T>) -> Vec3<T> {
        let z = self.axis.as_vec().map(T::lift);
        w - z * w.dot(z)
    }

    /// `g` turned by the angle whose cosine and sine are `(c, s)`:
    /// `g + r (c - 1) + (Z x r) s`, `r` the radial part of `g - O`. Exactly `g` at `(1, 0)`,
    /// and linear in `(c, s)`, so a quarter arc's corner `(1, 1)` gives its control point.
    fn at<T: Ring>(&self, c: T, s: T, g: Point3<T>) -> Point3<T> {
        let z = self.axis.as_vec().map(T::lift);
        let r = self.radial(g - self.origin.map(T::lift));
        g + r * (c - T::lift(1.0)) + z.cross(r) * s
    }
}

impl Extrusion {
    /// `generator` over `range` (its own parameter; radians for a circle or ellipse, whole
    /// quarter turns) swept along `along` for `v` in `v`: `S(u, v) = G(u) + v along`.
    ///
    /// # Errors
    ///
    /// [`ProceduralError::Generator`] (`Unsupported`) for a parabola or hyperbola;
    /// `Refused`: [`ProceduralError::Range`] (either range), [`ProceduralError::NotFinite`],
    /// [`ProceduralError::ZeroDirection`] for a zero vector, and [`ProceduralError::Flat`]
    /// for a line generator along `along` or a conic whose plane holds it.
    pub fn new(
        generator: &Curve,
        range: Extent1d,
        along: Vec3,
        v: Extent1d,
    ) -> Result<Self, ProceduralError> {
        if !along.is_finite() {
            return Err(ProceduralError::NotFinite);
        }
        let (v0, v1) = (v.min(), v.max());
        if !(v0.is_finite() && v1.is_finite()) || v0 >= v1 {
            return Err(ProceduralError::Range { min: v0, max: v1 });
        }
        let len = along.length();
        if len == 0.0 {
            return Err(ProceduralError::ZeroDirection);
        }
        let profile = Profile::new(generator, range)?;
        let flat = match profile.shape {
            Shape::Closed(Closed::Line { d, .. }) => {
                d.cross(along).length() <= FLAT * d.length() * len
            }
            Shape::Closed(Closed::Conic { a, b, .. }) => {
                let n = a.cross(b);
                n.dot(along).abs() <= FLAT * n.length() * len
            }
            Shape::Nurbs(_) => false,
        };
        if flat {
            return Err(ProceduralError::Flat);
        }
        Ok(Self { profile, along, v })
    }

    /// The extrusion vector: `S_v`.
    #[must_use]
    pub const fn along(&self) -> Vec3 {
        self.along
    }

    /// The generator's range in the surface's `u`.
    #[must_use]
    pub const fn range(&self) -> Extent1d {
        self.profile.range
    }

    /// The range of `v`.
    #[must_use]
    pub const fn v_range(&self) -> Extent1d {
        self.v
    }

    fn at<T: Ring>(&self, v: T, g: Point3<T>) -> Point3<T> {
        g + self.along.map(T::lift) * v
    }
}

impl Sweep {
    /// `generator` over `range` (its own parameter; radians for a circle or ellipse, whole
    /// quarter turns) carried by `motion`: `S(u, v)` the generator's point at `v` moved by
    /// the motion at `u`.
    ///
    /// # Errors
    ///
    /// [`ProceduralError::Generator`] (`Unsupported`) for a parabola or hyperbola;
    /// `Refused`: [`ProceduralError::Range`], and [`ProceduralError::OnAxis`] where every
    /// control point of the generator is on a screw's axis and no law moves it off;
    /// [`ProceduralError::TooFine`] (`Exhausted`) for a net of more than 4 000 000 control
    /// points.
    pub fn new(
        generator: &Curve,
        range: Extent1d,
        motion: impl Into<Motion>,
    ) -> Result<Self, ProceduralError> {
        let profile = Profile::new(generator, range)?;
        let motion = motion.into();
        let c = profile.controls(FRAC_1_SQRT_2, None);
        match &motion {
            Motion::Screw(s) => {
                let k = s.axis().as_vec();
                let (mut off, mut far) = (0.0_f64, 0.0_f64);
                for &g in &c.pts {
                    let w = g - s.origin();
                    off = off.max((w - k * w.dot(k)).length());
                    far = far.max(w.length());
                }
                let moved = s.law().is_some_and(|l| !l.is_nought());
                if off <= FLAT * far && !moved {
                    return Err(ProceduralError::OnAxis);
                }
                let rows = 2 * s.spans() + 1;
                if rows.saturating_mul(c.pts.len()) > MAX_NET_POINTS {
                    return Err(ProceduralError::TooFine { spans: s.spans() });
                }
            }
        }
        Ok(Self { profile, motion })
    }

    /// The motion.
    #[must_use]
    pub const fn motion(&self) -> &Motion {
        &self.motion
    }

    /// The generator's range in the surface's `v`.
    #[must_use]
    pub const fn range(&self) -> Extent1d {
        self.profile.range
    }

    fn screw(&self) -> &Screw {
        match &self.motion {
            Motion::Screw(s) => s,
        }
    }
}

impl From<Screw> for Motion {
    fn from(s: Screw) -> Self {
        Self::Screw(s)
    }
}

impl From<Sweep> for Definition {
    fn from(s: Sweep) -> Self {
        Self::Sweep(s)
    }
}

impl From<Revolution> for Definition {
    fn from(r: Revolution) -> Self {
        Self::Revolution(r)
    }
}

impl From<Extrusion> for Definition {
    fn from(e: Extrusion) -> Self {
        Self::Extrusion(e)
    }
}

// ---------------------------------------------------------------- the net

/// A net's control data in `T`: rows along `u`, row-major.
struct NetData<T> {
    ku: (usize, Vec<f64>),
    kv: (usize, Vec<f64>),
    cols: usize,
    pts: Vec<Point3<T>>,
    w: Vec<T>,
}

/// The definition's control net in `T`, over its range or over the quarter arcs `uq`, `vq`
/// of its periodic directions.
fn net_data<T: Trig>(
    def: &Definition,
    root_half: T,
    uq: Option<(i64, i64)>,
    vq: Option<(i64, i64)>,
) -> NetData<T> {
    match def {
        Definition::Revolution(r) => {
            let (k0, k1) = uq.unwrap_or((0, 4));
            let (knots, turn) = turn_controls(k0, k1, root_half);
            let g = r.profile.controls(root_half, vq);
            let mut pts = Vec::with_capacity(turn.len() * g.pts.len());
            let mut w = Vec::with_capacity(pts.capacity());
            for &(c, s, wu) in &turn {
                for (&p, &wv) in g.pts.iter().zip(&g.w) {
                    pts.push(r.at(c, s, p));
                    w.push(wu * wv);
                }
            }
            NetData {
                ku: (2, knots),
                kv: (g.degree, g.knots),
                cols: g.pts.len(),
                pts,
                w,
            }
        }
        Definition::Extrusion(e) => {
            let g = e.profile.controls(root_half, uq);
            let (v0, v1) = (e.v.min(), e.v.max());
            let mut pts = Vec::with_capacity(2 * g.pts.len());
            let mut w = Vec::with_capacity(pts.capacity());
            for (&p, &wu) in g.pts.iter().zip(&g.w) {
                pts.push(e.at(T::lift(v0), p));
                pts.push(e.at(T::lift(v1), p));
                w.extend([wu, wu]);
            }
            NetData {
                ku: (g.degree, g.knots),
                kv: (1, vec![v0, v0, v1, v1]),
                cols: 2,
                pts,
                w,
            }
        }
        Definition::Sweep(s) => {
            let g = s.profile.controls(root_half, vq);
            let (knots, pts, w) = s.screw().rows(&g.pts, &g.w);
            NetData {
                ku: (2, knots),
                kv: (g.degree, g.knots),
                cols: g.pts.len(),
                pts,
                w,
            }
        }
    }
}

/// The net in `f64` and the bound of its gap from the exact net, over the given quarter
/// arcs (`None`: the range).
///
/// With `P~, w~` the rounded control data and `P, W` the exact, the net's point is a
/// convex combination of the `P~` (with the weights `N w~`) inside its knots' box, so
/// `|S~ - S| <= delta + eps / (1 - eps) D`, for `delta` the largest `|P~ - P|`, `eps` the
/// largest `|w~ / W - 1|` and `D` a bound on the exact points' spread (research 75 §13.27).
/// The effective `P~` is the net's own homogeneous point over its weight, so the rounding of
/// `w~ P~` is in `delta`. All of it in outward intervals, the upper end taken. A sweep's
/// exact net is within [`Screw::approx`] of its definition, which the bound adds.
fn build(
    def: &Definition,
    uq: Option<(i64, i64)>,
    vq: Option<(i64, i64)>,
) -> Result<(NurbsSurface<3>, f64), ProceduralError> {
    let f = net_data::<f64>(def, FRAC_1_SQRT_2, uq, vq);
    let x = net_data::<Ivl>(def, Ivl::from(0.5).sqrt(), uq, vq);
    let rational = f.w.iter().any(|&w| w != 1.0);
    let (mut d2, mut eps) = (Ivl::from(0.0), 0.0_f64);
    let mut hull: Option<[Ivl; 3]> = None;
    for ((p, &w), (q, &wx)) in f.pts.iter().zip(&f.w).zip(x.pts.iter().zip(&x.w)) {
        let eff = |c: f64| {
            if rational {
                Ivl::from(c * w) / Ivl::from(w)
            } else {
                Ivl::from(c)
            }
        };
        let e = [eff(p.x) - q.x, eff(p.y) - q.y, eff(p.z) - q.z];
        let e2 = e.iter().fold(Ivl::from(0.0), |s, c| {
            let m = Ivl::from(c.mag());
            s + m * m
        });
        d2 = Ivl::new(d2.lo.max(e2.lo), d2.hi.max(e2.hi));
        eps = eps.max((Ivl::from(w) / wx - Ivl::from(1.0)).mag());
        let qa = [q.x, q.y, q.z];
        hull = Some(match hull {
            None => qa,
            Some(h) => [h[0].hull(qa[0]), h[1].hull(qa[1]), h[2].hull(qa[2])],
        });
    }
    let delta = Ivl::from(d2.hi).sqrt();
    let spread = hull.map_or_else(
        || Ivl::from(0.0),
        |h| {
            h.iter().fold(Ivl::from(0.0), |s, c| {
                let wdt = Ivl::from(c.hi) - Ivl::from(c.lo);
                s + wdt * wdt
            })
        },
    );
    let dm = Ivl::from(spread.hi).sqrt() + delta + delta;
    let e = Ivl::from(eps);
    let approx = match def {
        Definition::Sweep(s) => s.screw().approx(),
        _ => 0.0,
    };
    let bound = (delta + e / (Ivl::from(1.0) - e) * dm + Ivl::from(approx)).hi;
    if !(bound.is_finite() && eps < 1.0) {
        return Err(ProceduralError::NotFinite);
    }
    let net_err = ProceduralError::Net;
    let knots = |axis, (deg, k): (usize, Vec<f64>)| {
        KnotVector::new(deg, k)
            .map_err(|cause| net_err(NurbsSurfaceError::BadKnots { axis, cause }))
    };
    let ku = knots(ParamAxis::U, f.ku)?;
    let kv = knots(ParamAxis::V, f.kv)?;
    let rows = f.pts.len() / f.cols.max(1);
    let pts =
        Net::new(rows, f.cols, f.pts.iter().map(|p| p.to_array()).collect()).map_err(net_err)?;
    let w = if rational {
        Some(Net::new(rows, f.cols, f.w).map_err(net_err)?)
    } else {
        None
    };
    let net = NurbsSurface::new(ku, kv, pts, w).map_err(net_err)?;
    Ok((net, bound))
}

// ---------------------------------------------------------------- the surface

/// A procedural surface: its [`Definition`] and the definition's exact net, built once
/// (research 75 §13.27). See the module docs for which calls read which.
#[derive(Debug, Clone, PartialEq)]
pub struct Procedural {
    def: Definition,
    net: NurbsSurface<3>,
    bound: f64,
}

/// A closed-form view of a revolution or extrusion whose generator is a line or conic: its
/// formula over any [`Ring`] (the enclosure's ring of values with partials among them).
pub(crate) enum ClosedForm<'a> {
    Revolution(&'a Revolution, Closed),
    Extrusion(&'a Extrusion, Closed),
}

impl ClosedForm<'_> {
    pub(crate) fn eval<T: Ring>(&self, u: T, v: T, near: Point2) -> Point3<T> {
        match *self {
            Self::Revolution(r, shape) => {
                let (c, s) = TURN.cos_sin(u, TURN.piece(near.x, Approach::Above));
                r.at(c, s, shape.point(v, near.y))
            }
            Self::Extrusion(e, shape) => e.at(v, shape.point(u, near.x)),
        }
    }
}

fn contains(outer: &Extent2d, inner: &Extent2d) -> bool {
    let (a, b) = (outer.min(), outer.max());
    let (c, d) = (inner.min(), inner.max());
    a.x <= c.x && a.y <= c.y && d.x <= b.x && d.y <= b.y
}

impl Procedural {
    /// The surface of `def`, its net built and bounded.
    ///
    /// # Errors
    ///
    /// [`ProceduralError::NotFinite`] where the bound is not finite, and
    /// [`ProceduralError::Net`] where the net cannot be built (overflow).
    pub fn new(def: impl Into<Definition>) -> Result<Self, ProceduralError> {
        let def = def.into();
        let (net, bound) = build(&def, None, None)?;
        Ok(Self { def, net, bound })
    }

    /// The definition.
    #[must_use]
    pub const fn definition(&self) -> &Definition {
        &self.def
    }

    /// The exact net: the definition's NURBS image over the domain's base box, its control
    /// data rounded once.
    #[must_use]
    pub const fn net(&self) -> &NurbsSurface<3> {
        &self.net
    }

    /// The bound of the net's gap from the definition at the same `uv`, inside the net's
    /// knots' box: the rounding of the control data, not of either evaluation.
    #[must_use]
    pub const fn bound(&self) -> f64 {
        self.bound
    }

    /// The net and its bound, for a query over `uv` that reads control points: `Some` where
    /// `uv` lies in the net's knots' box (the bound holds there), `None` past it, where the
    /// query reads the definition.
    #[must_use]
    pub fn net_over(&self, uv: &Extent2d) -> Option<(&NurbsSurface<3>, f64)> {
        contains(&self.net.domain(), uv).then_some((&self.net, self.bound))
    }

    /// The closed form of a revolution or extrusion of a line or conic; `None` for a NURBS
    /// generator and for a sweep (its angle is not a ring's: [`Procedural::eval`] reads it).
    pub(crate) fn closed_form(&self) -> Option<ClosedForm<'_>> {
        let shape = match &self.profile().shape {
            Shape::Closed(c) => *c,
            Shape::Nurbs(_) => return None,
        };
        match &self.def {
            Definition::Revolution(r) => Some(ClosedForm::Revolution(r, shape)),
            Definition::Extrusion(e) => Some(ClosedForm::Extrusion(e, shape)),
            Definition::Sweep(_) => None,
        }
    }

    fn profile(&self) -> &Profile {
        match &self.def {
            Definition::Revolution(r) => &r.profile,
            Definition::Extrusion(e) => &e.profile,
            Definition::Sweep(s) => &s.profile,
        }
    }

    /// The arcs a box is cut at in each direction (`None`: no breaks), each arc in `per`.
    pub(crate) fn cuts(&self, per: usize) -> ((Option<Arcs>, usize), (Option<Arcs>, usize)) {
        let g = self.profile().arcs();
        match &self.def {
            Definition::Revolution(_) => ((Some(TURN), per), (g, per)),
            Definition::Extrusion(_) => ((g, per), (None, 1)),
            Definition::Sweep(_) => ((None, per), (g, per)),
        }
    }

    /// `S(u, v)` from the definition.
    #[must_use]
    pub fn point(&self, uv: Point2) -> Point3 {
        self.eval(uv.x, uv.y, uv)
    }

    /// `S(u, v)` from the definition in any [`Trig`] scalar (`f64`, an enclosure in
    /// [`Ivl`]), with the arcs that hold `near`.
    #[must_use]
    pub fn eval<T: Trig>(&self, u: T, v: T, near: Point2) -> Point3<T> {
        match &self.def {
            Definition::Revolution(r) => {
                let (c, s) = TURN.cos_sin(u, TURN.piece(near.x, Approach::Above));
                r.at(c, s, r.profile.point(v, near.y))
            }
            Definition::Extrusion(e) => e.at(v, e.profile.point(u, near.x)),
            Definition::Sweep(s) => s.screw().eval(u, s.profile.point(v, near.y)),
        }
    }

    /// The partials `S^(i,j)` for every `i + j <= D`, from the definition; at a break,
    /// `side` picks the piece in each direction.
    #[must_use]
    pub fn jet<const D: usize>(&self, uv: Point2, side: [Approach; 2]) -> SurfaceJet<Vec3, D> {
        let zero = Vec3::splat(0.0);
        match &self.def {
            Definition::Revolution(r) => {
                let tu = TURN.trig::<D>(uv.x, side[0]);
                let g = r.profile.jet::<D>(uv.y, side[1]);
                let z = r.axis.as_vec();
                let w0 = g.point - r.origin;
                SurfaceJet::from_fn(|i, j| {
                    let gj = match j.checked_sub(1) {
                        None => w0,
                        Some(k) => g.d.get(k).copied().unwrap_or(zero),
                    };
                    let rj = r.radial(gj);
                    let qj = z.cross(rj);
                    let (c, s) = tu.nth(i);
                    if i == 0 {
                        let base = if j == 0 { g.point.to_vec() } else { gj };
                        base + rj * (c - 1.0) + qj * s
                    } else {
                        rj * c + qj * s
                    }
                })
            }
            Definition::Extrusion(e) => {
                let g = e.profile.jet::<D>(uv.x, side[0]);
                SurfaceJet::from_fn(|i, j| match (i, j) {
                    (0, 0) => g.point.to_vec() + e.along * uv.y,
                    (0, 1) => e.along,
                    (i, 0) => g.d.get(i - 1).copied().unwrap_or(zero),
                    _ => zero,
                })
            }
            Definition::Sweep(sw) => {
                let s = sw.screw();
                let g = sw.profile.jet::<D>(uv.y, side[1]);
                SurfaceJet::from_fn(|i, j| {
                    let pj = match j.checked_sub(1) {
                        None => g.point - s.origin(),
                        Some(k) => g.d.get(k).copied().unwrap_or(zero),
                    };
                    s.partial(uv.x, side[0], i, j, pj)
                })
            }
        }
    }

    /// The unit normal along `S_u x S_v`, from the pieces `side` picks; `None` where the
    /// cross product is too short to point (the generator meeting the axis).
    #[must_use]
    pub fn normal(&self, uv: Point2, side: [Approach; 2]) -> Option<UnitVec3> {
        let j = self.jet::<1>(uv, side);
        let (su, sv) = (j.get(1, 0)?, j.get(0, 1)?);
        su.cross(sv).normalized()
    }

    /// The domain: a revolution's `u` periodic over `[0, 1]`; a generator's direction
    /// periodic for a conic's whole turn, else its range; an extrusion's `v` its range.
    #[must_use]
    pub fn domain(&self) -> UvDomain {
        match &self.def {
            Definition::Revolution(r) => UvDomain::new(
                ParamDomain::periodic(1.0, 0.0).unwrap_or(ParamDomain::Unbounded),
                r.profile.domain(),
            ),
            Definition::Extrusion(e) => {
                UvDomain::new(e.profile.domain(), ParamDomain::Bounded(e.v))
            }
            Definition::Sweep(s) => {
                UvDomain::new(ParamDomain::Bounded(s.screw().turn()), s.profile.domain())
            }
        }
    }

    /// The exact NURBS image over `over`, in the same uv: the net restricted to `over`
    /// where `over` lies in its box; past it along a periodic direction, a net built over
    /// the quarter arcs that cover `over` (at most two turns), restricted.
    ///
    /// # Errors
    ///
    /// `Refused`: [`SurfaceError::EmptyExtent`], and [`SurfaceError::OutsideDomain`] past a
    /// bounded direction or over more than a period; [`SurfaceError::Procedural`] or
    /// [`SurfaceError::Nurbs`] only on overflow; [`SurfaceError::Procedural`] with
    /// [`ProceduralError::Inexact`] (`Unsupported`) for a sweep, whose net is not exact.
    pub fn to_nurbs(&self, over: &Extent2d) -> Result<NurbsSurface<3>, SurfaceError> {
        if matches!(self.def, Definition::Sweep(_)) {
            return Err(SurfaceError::Procedural(ProceduralError::Inexact {
                bound: self.bound,
            }));
        }
        if let Some((n, _)) = self.net_over(over) {
            return crate::to_nurbs::restrict(n, over);
        }
        let have = self.net.domain();
        let d = self.domain();
        let cover = |axis, want: Extent1d, pd: ParamDomain, have: Extent1d| {
            let (min, max) = (want.min(), want.max());
            if min == max {
                return Err(SurfaceError::EmptyExtent { axis, at: min });
            }
            if have.min() <= min && max <= have.max() {
                return Ok(None);
            }
            let outside = SurfaceError::OutsideDomain { axis, min, max };
            match pd {
                ParamDomain::Periodic { period, .. } if max - min <= period => {
                    let (k0, k1) = ((4.0 * min).floor(), (4.0 * max).ceil());
                    if !(k0.abs() < 1e9 && k1.abs() < 1e9) || k1 - k0 > MAX_QUARTERS as f64 {
                        return Err(outside);
                    }
                    Ok(Some((k0 as i64, k1 as i64)))
                }
                _ => Err(outside),
            }
        };
        let uq = cover(ParamAxis::U, over.x_interval(), d.u, have.x_interval())?;
        let vq = cover(ParamAxis::V, over.y_interval(), d.v, have.y_interval())?;
        let (n, _) = build(&self.def, uq, vq).map_err(SurfaceError::Procedural)?;
        crate::to_nurbs::restrict(&n, over)
    }

    /// The nearest point of the surface to `p`, placed against `domain` (`None`: the net's
    /// box): the net's foot, and for a sweep (whose net is not exact) that foot polished on
    /// the definition by Newton steps within the net's box, each kept only where it comes
    /// nearer.
    pub(crate) fn invert(
        &self,
        p: Point3,
        near: Option<Point2>,
        domain: Option<&Extent2d>,
        budget: &mut scx_num::budget::Budget,
        stop: &scx_num::budget::Stop,
    ) -> Result<Inversion<Point2>, SurfaceError> {
        let inv = crate::nurbs_invert::invert(&self.net, p, near, domain, budget, stop)?;
        if !matches!(self.def, Definition::Sweep(_)) {
            return Ok(inv);
        }
        let start = inv.foot().at;
        if !(start.x.is_finite() && start.y.is_finite()) {
            return Ok(inv);
        }
        let uv = self.polish(p, start);
        let at = |uv: Point2| invert::foot(p, uv, self.point(uv));
        Ok(invert::invert(
            Closest::One(at(uv)),
            Periods::NONE,
            domain,
            at,
        ))
    }

    /// Newton on `|S - p|^2` from `uv`, within the net's box.
    fn polish(&self, p: Point3, mut uv: Point2) -> Point2 {
        let side = [Approach::Above, Approach::Above];
        let bx = self.net.domain();
        let mut best = (self.point(uv) - p).length_squared();
        for _ in 0..POLISH_STEPS {
            let j = self.jet::<2>(uv, side);
            let get = |i, k| j.get(i, k).unwrap_or(Vec3::splat(0.0));
            let (d, su, sv) = (get(0, 0) - p.to_vec(), get(1, 0), get(0, 1));
            let (suu, suv, svv) = (get(2, 0), get(1, 1), get(0, 2));
            let (gu, gv) = (su.dot(d), sv.dot(d));
            let (a, b, c) = (
                su.dot(su) + suu.dot(d),
                su.dot(sv) + suv.dot(d),
                sv.dot(sv) + svv.dot(d),
            );
            let det = a * c - b * b;
            if !(det.is_finite() && det > 0.0) {
                break;
            }
            let (du, dv) = ((c * gu - b * gv) / det, (a * gv - b * gu) / det);
            let next = bx.clamp_point(Point2::new(uv.x - du, uv.y - dv));
            let dist = (self.point(next) - p).length_squared();
            if dist.is_nan() || dist >= best {
                break;
            }
            (uv, best) = (next, dist);
        }
        uv
    }

    /// The same surface run the other way along `axis`, `S'(-u, v) = S(u, v)` (or in `v`),
    /// exactly: a revolution's turn by negating its axis, a generator's direction by the
    /// generator reversed, an extrusion's `v` by its vector negated, a screw's turn by its
    /// axis, stations and law's angles negated ([`Screw`]'s gap kept). The net is the
    /// original's reversed ([`NurbsSurface::reversed`], exact), with the same bound.
    #[must_use]
    pub fn reversed(&self, axis: ParamAxis) -> (Self, UvMap) {
        let def = match (&self.def, axis) {
            (Definition::Revolution(r), ParamAxis::U) => Definition::Revolution(Revolution {
                axis: -r.axis,
                ..r.clone()
            }),
            (Definition::Revolution(r), ParamAxis::V) => Definition::Revolution(Revolution {
                profile: r.profile.reversed(),
                ..r.clone()
            }),
            (Definition::Extrusion(e), ParamAxis::U) => Definition::Extrusion(Extrusion {
                profile: e.profile.reversed(),
                ..e.clone()
            }),
            (Definition::Extrusion(e), ParamAxis::V) => Definition::Extrusion(Extrusion {
                along: -e.along,
                v: Extent1d::new(-e.v.max(), -e.v.min()).unwrap_or(e.v),
                ..e.clone()
            }),
            (Definition::Sweep(s), ParamAxis::U) => Definition::Sweep(Sweep {
                motion: Motion::Screw(s.screw().reversed()),
                ..s.clone()
            }),
            (Definition::Sweep(s), ParamAxis::V) => Definition::Sweep(Sweep {
                profile: s.profile.reversed(),
                ..s.clone()
            }),
        };
        let s = Self {
            def,
            net: self.net.reversed(axis),
            bound: self.bound,
        };
        (s, UvMap::negating(axis))
    }

    /// The image under `a`, on the same uv ([`UvMap::IDENTITY`]): an extrusion under any
    /// affine map (its generator's control data and vector mapped), a revolution or a sweep
    /// under a similarity (its axis the image's, turned for a mirror so `u` keeps its points;
    /// a screw's pitch and law scaled, its stations kept). The net is built again.
    ///
    /// # Errors
    ///
    /// [`SurfaceError::MapNotFinite`]; [`SurfaceError::NotSimilar`] (`Unsupported`) for a
    /// revolution or sweep under a map that is not a similarity; [`SurfaceError::Procedural`] for an
    /// image the definition refuses (flattened by the map).
    pub fn transformed(&self, a: &Affine) -> Result<(Self, UvMap), SurfaceError> {
        if !a.is_finite() {
            return Err(SurfaceError::MapNotFinite);
        }
        let axes = [a.x_axis(), a.y_axis(), a.z_axis()];
        let det = axes[0].cross(axes[1]).dot(axes[2]);
        let def = match &self.def {
            Definition::Revolution(r) => {
                if !similar(&axes) {
                    return Err(SurfaceError::NotSimilar);
                }
                let z = a.apply_vec(r.axis.as_vec()) * det.signum();
                Definition::Revolution(Revolution {
                    profile: r.profile.transformed(a)?,
                    origin: a.apply(r.origin),
                    axis: z.normalized().ok_or(SurfaceError::MapFlattens)?,
                })
            }
            Definition::Extrusion(e) => {
                let along = a.apply_vec(e.along);
                if along.length() == 0.0 {
                    return Err(SurfaceError::MapFlattens);
                }
                Definition::Extrusion(Extrusion {
                    profile: e.profile.transformed(a)?,
                    along,
                    v: e.v,
                })
            }
            Definition::Sweep(sw) => {
                if !similar(&axes) {
                    return Err(SurfaceError::NotSimilar);
                }
                let s = sw.screw();
                let scale = axes[0].length();
                let z = a.apply_vec(s.axis().as_vec()) * det.signum();
                let screw = s
                    .transformed(a.apply(s.origin()), z, scale, det < 0.0, |d| a.apply_vec(d))
                    .map_err(SurfaceError::Procedural)?;
                let profile = sw.profile.transformed(a)?;
                Definition::Sweep(Sweep {
                    profile,
                    motion: Motion::Screw(screw),
                })
            }
        };
        let s = Self::new(def).map_err(SurfaceError::Procedural)?;
        Ok((s, UvMap::IDENTITY))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Surface, Torus};
    use scx_curve::{Circle, Ellipse, Line};
    use scx_num::budget::{Budget, Stop};
    use scx_num::fmath;
    use scx_num::frame::{Frame, Sense};
    use scx_num::hash::Rng;
    use scx_num::units::{Length, Resolution};
    use scx_nurbs::KnotVector;

    const ABOVE: [Approach; 2] = [Approach::Above, Approach::Above];

    fn ext(a: f64, b: f64) -> Extent1d {
        Extent1d::new(a, b).unwrap()
    }

    fn line(a: Point3, b: Point3) -> Curve {
        Curve::Line(Line::through(a, b).unwrap())
    }

    fn rev(g: &Curve, range: Extent1d) -> Procedural {
        Procedural::new(Revolution::new(g, range, Point3::ORIGIN, Vec3::Z).unwrap()).unwrap()
    }

    /// The pin's `cone()`: the line (2, 0, 0) to (1, 0, 3) about the z axis.
    fn cone() -> Procedural {
        rev(
            &line(Point3::new(2.0, 0.0, 0.0), Point3::new(1.0, 0.0, 3.0)),
            ext(0.0, 1.0),
        )
    }

    /// The pin's `bezier()`: a cubic, not planar.
    fn bezier() -> Curve {
        let k = KnotVector::new(3, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]).unwrap();
        let pts = vec![
            [2.0, 0.0, 0.0],
            [2.5, 0.4, 0.5],
            [3.2, -0.2, 1.7],
            [2.7, 0.3, 3.0],
        ];
        Curve::Nurbs(Arc::new(NurbsCurve::new(k, pts, None).unwrap()))
    }

    fn grid() -> Vec<Point2> {
        let mut out = Vec::new();
        for i in 0..=24 {
            for j in 0..=10 {
                out.push(Point2::new(f64::from(i) / 24.0, f64::from(j) / 10.0));
            }
        }
        out
    }

    fn near(a: Point3, b: Point3, tol: f64) -> bool {
        (a - b).length() <= tol
    }

    fn samples(p: &Procedural) -> Vec<Point2> {
        let e = p.net().domain();
        let (lo, hi) = (e.min(), e.max());
        grid()
            .into_iter()
            .map(|t| Point2::new(lo.x + (hi.x - lo.x) * t.x, lo.y + (hi.y - lo.y) * t.y))
            .collect()
    }

    /// Ported from the pin's `canonical_line_is_a_cone`: the line's revolution is the cone
    /// `x^2 + y^2 = (2 - z / 3)^2`, point for point, the quarter turn half way up at
    /// (0, 1.5, 1.5).
    #[test]
    fn a_line_revolved_is_a_cone() {
        let s = cone();
        let p = s.point(Point2::new(0.25, 0.5));
        assert!(near(p, Point3::new(0.0, 1.5, 1.5), 1e-15), "{p:?}");
        for uv in grid() {
            let p = s.point(uv);
            let rho = (p.x * p.x + p.y * p.y).sqrt();
            assert!((rho - (2.0 - p.z / 3.0)).abs() <= 1e-14, "{uv:?}");
            assert!((p.z - 3.0 * uv.y).abs() <= 1e-15);
            // The angle is the torus's quarter-arc turn of u.
            let want = TURN.angle(uv.x);
            let got = fmath::atan2(p.y, p.x).rem_euclid(core::f64::consts::TAU);
            let d = (got - want).abs();
            assert!(
                d <= 1e-13 || (d - core::f64::consts::TAU).abs() <= 1e-13,
                "{uv:?}"
            );
        }
        assert_eq!(s.domain().v, ParamDomain::Bounded(ext(0.0, 1.0)));
    }

    /// Ported from the pin's `sweep_iso_curve_is_a_circle`: the iso line along `u` at
    /// `v = 0.5` is the circle of radius 1.5 at height 1.5, and along `v` the generator.
    #[test]
    fn the_sweep_iso_line_is_a_circle() {
        let s = cone();
        for i in 0..=40 {
            let p = s.point(Point2::new(f64::from(i) / 40.0, 0.5));
            assert!((p.z - 1.5).abs() <= 1e-15);
            assert!(((p.x * p.x + p.y * p.y).sqrt() - 1.5).abs() <= 1e-14);
            let g = s.point(Point2::new(0.0, f64::from(i) / 40.0));
            let t = f64::from(i) / 40.0;
            assert!(near(g, Point3::new(2.0 - t, 0.0, 3.0 * t), 1e-15));
        }
    }

    /// Ported from the pin's `generator_rotation_and_equality`: the cubic turned a quarter
    /// is at (0, 2, 0) at `v = 0`; a copy is equal, a reversal is not.
    #[test]
    fn a_generator_turns_and_a_copy_is_equal() {
        let s = rev(&bezier(), ext(0.0, 1.0));
        let p = s.point(Point2::new(0.25, 0.0));
        assert!(near(p, Point3::new(0.0, 2.0, 0.0), 1e-15), "{p:?}");
        assert_eq!(s.clone(), s);
        let (r, _) = s.reversed(ParamAxis::U);
        assert_ne!(r, s);
        assert_eq!(r.reversed(ParamAxis::U).0, s);
        // Every point is the generator's point turned about z by the quarter-arc angle.
        let Curve::Nurbs(g) = bezier() else {
            unreachable!()
        };
        for uv in grid() {
            let q = g.point(uv.y, Approach::Above).unwrap();
            let (c, sn) = TURN.cos_sin(uv.x, TURN.piece(uv.x, Approach::Above));
            let want = Point3::new(q[0] * c - q[1] * sn, q[0] * sn + q[1] * c, q[2]);
            assert!(near(s.point(uv), want, 1e-14), "{uv:?}");
        }
    }

    /// An off-axis circle revolved is the torus, to the rounding.
    #[test]
    fn an_off_axis_circle_revolved_is_a_torus() {
        let (big, small) = (3.0, 1.25);
        let f = Frame::new(Point3::new(big, 0.0, 0.0), -Vec3::Y, Vec3::X).unwrap();
        // x = X, y = Z: the tube's circle from the outer equator, upward.
        assert!(near(
            f.y().as_vec().to_point(),
            Point3::new(0.0, 0.0, 1.0),
            1e-15
        ));
        let c = Curve::Circle(Circle::new(f, Length::from_mm(small), Sense::Forward).unwrap());
        let s = rev(&c, ext(0.0, core::f64::consts::TAU));
        let t = Torus::new(
            Frame::WORLD,
            Length::from_mm(big),
            Length::from_mm(small),
            Sense::Forward,
        )
        .unwrap();
        for uv in grid() {
            assert!(near(s.point(uv), t.point(uv), 1e-14), "{uv:?}");
            let n = s.normal(uv, ABOVE).unwrap();
            assert!((*n - *t.normal(uv)).length() <= 1e-12, "{uv:?}");
        }
        assert!(matches!(s.domain().v, ParamDomain::Periodic { .. }));
    }

    /// A skew line revolved is the hyperboloid `x^2 + y^2 - (z tan a)^2 = r^2`.
    #[test]
    fn a_skew_line_revolved_is_a_hyperboloid() {
        let (r, a) = (1.5_f64, 0.6_f64);
        let g = Curve::Line(
            Line::new(
                Point3::new(r, 0.0, 0.0),
                Vec3::new(0.0, fmath::sin(a), fmath::cos(a)),
            )
            .unwrap(),
        );
        let s = rev(&g, ext(-2.0, 2.0));
        for uv in grid() {
            let uv = Point2::new(uv.x, 4.0 * uv.y - 2.0);
            let p = s.point(uv);
            let lhs = p.x * p.x + p.y * p.y - fmath::powi(p.z * fmath::tan(a), 2);
            assert!((lhs - r * r).abs() <= 1e-13, "{uv:?} {lhs}");
        }
    }

    /// An ellipse extruded is the elliptic cylinder `x^2 / a^2 + y^2 / b^2 = 1`.
    #[test]
    fn an_ellipse_extruded_is_an_elliptic_cylinder() {
        let (a, b) = (3.0, 2.0);
        let e = Ellipse::new(
            Frame::WORLD,
            Length::from_mm(a),
            Length::from_mm(b),
            Sense::Forward,
        )
        .unwrap();
        let along = Vec3::new(0.5, 0.0, 4.0);
        let d = Extrusion::new(
            &Curve::Ellipse(e),
            ext(0.0, core::f64::consts::TAU),
            along,
            ext(0.0, 1.0),
        )
        .unwrap();
        let s = Procedural::new(d).unwrap();
        for uv in grid() {
            let p = s.point(uv);
            let q = p - along * uv.y;
            assert!((q.x * q.x / (a * a) + q.y * q.y / (b * b) - 1.0).abs() <= 1e-14);
            assert!(q.z.abs() <= 1e-15);
            let j = s.jet::<2>(uv, ABOVE);
            assert_eq!(j.get(0, 1).unwrap(), along);
            assert_eq!(j.get(1, 1).unwrap(), Vec3::splat(0.0));
        }
    }

    fn all() -> Vec<Procedural> {
        let circle = |o: Point3, z: Vec3, r: f64| {
            let f = Frame::new(o, z, Vec3::new(0.3, 1.0, 0.2)).unwrap();
            Curve::Circle(Circle::new(f, Length::from_mm(r), Sense::Reversed).unwrap())
        };
        let mut out = vec![
            cone(),
            rev(&bezier(), ext(0.0, 1.0)),
            rev(
                &circle(Point3::new(4.0, 1.0, 2.0), Vec3::new(0.2, 1.0, 0.1), 1.0),
                ext(-FRAC_PI_2, core::f64::consts::PI),
            ),
        ];
        let e = Extrusion::new(
            &bezier(),
            ext(0.0, 1.0),
            Vec3::new(1.0, -2.0, 0.5),
            ext(-1.0, 2.0),
        );
        out.push(Procedural::new(e.unwrap()).unwrap());
        let e = Extrusion::new(
            &circle(Point3::new(1e3, -2e3, 5e2), Vec3::Z, 250.0),
            ext(0.0, core::f64::consts::TAU),
            Vec3::new(0.0, 1.0, 30.0),
            ext(0.0, 1.0),
        );
        out.push(Procedural::new(e.unwrap()).unwrap());
        let r = Revolution::new(
            &line(Point3::new(1.0, 2.0, 3.0), Point3::new(-1.0, 5.0, 4.0)),
            ext(-0.5, 2.0),
            Point3::new(0.5, 0.25, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        );
        out.push(Procedural::new(r.unwrap()).unwrap());
        out
    }

    fn scale(p: &Procedural) -> f64 {
        p.net()
            .points()
            .points()
            .iter()
            .flatten()
            .fold(1.0_f64, |m, x| m.max(x.abs()))
    }

    /// The net is the definition to its bound (plus each side's own evaluation rounding),
    /// and the bound is a few roundings of the size.
    #[test]
    fn the_net_is_the_definition_within_its_bound() {
        for s in all() {
            let sc = scale(&s);
            assert!(
                s.bound() > 0.0 && s.bound() <= 64.0 * f64::EPSILON * sc,
                "{}",
                s.bound()
            );
            for uv in samples(&s) {
                let n = s.net().point(uv.to_array(), ABOVE).unwrap();
                let gap = (Point3::from_array(n) - s.point(uv)).length();
                assert!(gap <= s.bound() + 64.0 * f64::EPSILON * sc, "{uv:?} {gap}");
            }
        }
    }

    /// The jet's partials are the definition's: central differences of the point.
    #[test]
    fn the_jet_is_the_derivative_of_the_point() {
        for s in all() {
            let sc = scale(&s);
            for uv in samples(&s).into_iter().step_by(7) {
                let j = s.jet::<2>(uv, ABOVE);
                // The jet's point is the same formula by another evaluation order.
                let p0 = j.get(0, 0).unwrap().to_point();
                assert!(
                    p0.distance(s.point(uv)) <= 8.0 * f64::EPSILON * sc,
                    "{uv:?}"
                );
                let h = 1e-6;
                for (i, d) in [(1, 0), (0, 1)] {
                    let e = Point2::new(h * f64::from(i), h * f64::from(d));
                    let fd = (s.point(Point2::new(uv.x + e.x, uv.y + e.y))
                        - s.point(Point2::new(uv.x - e.x, uv.y - e.y)))
                        * (0.5 / h);
                    let got = j.get(i as usize, d as usize).unwrap();
                    assert!((fd - got).length() <= 1e-6 * sc * 10.0, "{uv:?} {i}{d}");
                }
            }
        }
    }

    /// The certified box holds the definition's points, inside the net's box and past it.
    #[test]
    fn boxes_and_hulls_hold_the_definition() {
        let mut rng = Rng::new(0x9e0c);
        for s in all() {
            let surf = Surface::from(s.clone());
            let e = s.net().domain();
            let (lo, hi) = (e.min(), e.max());
            for k in 0..20 {
                let pick = |rng: &mut Rng, a: f64, b: f64| a + (b - a) * rng.next_f64();
                let (u0, v0) = (pick(&mut rng, lo.x, hi.x), pick(&mut rng, lo.y, hi.y));
                let (u1, v1) = (pick(&mut rng, u0, hi.x), pick(&mut rng, v0, hi.y));
                // Every fifth box reaches past the net's box (a periodic `u` past a turn).
                let past = if k % 5 == 0 { 0.6 } else { 0.0 };
                let uv = Extent2d::from_bounds(u0, v0, u1 + past, v1).unwrap();
                let b = surf.box_over(uv).unwrap();
                let a = surf.range_along(Vec3::new(0.3, -1.0, 0.7), uv).unwrap();
                for i in 0..=6 {
                    for j in 0..=6 {
                        let t = Point2::new(
                            u0 + (u1 + past - u0) * f64::from(i) / 6.0,
                            v0 + (v1 - v0) * f64::from(j) / 6.0,
                        );
                        let p = s.point(t);
                        let (m, n) = (b.min(), b.max());
                        assert!(
                            m.x <= p.x
                                && p.x <= n.x
                                && m.y <= p.y
                                && p.y <= n.y
                                && m.z <= p.z
                                && p.z <= n.z,
                            "{t:?}"
                        );
                        let d = Vec3::new(0.3, -1.0, 0.7).dot(p.to_vec());
                        assert!(a.lo <= d && d <= a.hi, "{t:?}");
                    }
                }
            }
            let h = surf.hulls().unwrap();
            for uv in samples(&s) {
                let at = Extent2d::from_bounds(uv.x, uv.y, uv.x, uv.y).unwrap();
                // A cell is half open: the net's top edge is in none.
                let Some((_, cell)) = h.containing(&at) else {
                    let top = s.net().domain().max();
                    assert!(uv.x == top.x || uv.y == top.y, "{uv:?}");
                    continue;
                };
                let p = s.point(uv);
                let b = cell.bbox().unwrap();
                let (lo, hi) = (b.min(), b.max());
                assert!(
                    lo.x <= p.x
                        && p.x <= hi.x
                        && lo.y <= p.y
                        && p.y <= hi.y
                        && lo.z <= p.z
                        && p.z <= hi.z,
                    "{uv:?}"
                );
            }
        }
    }

    /// `to_nurbs` is the definition over the box, inside the net's box and past it along a
    /// periodic `u`; a bounded direction past its range is refused.
    #[test]
    fn to_nurbs_inside_and_past_the_net() {
        for s in all() {
            let e = s.net().domain();
            let (lo, hi) = (e.min(), e.max());
            let sc = scale(&s);
            let inside = Extent2d::from_bounds(
                lo.x + 0.1 * (hi.x - lo.x),
                lo.y,
                lo.x + 0.8 * (hi.x - lo.x),
                lo.y + 0.5 * (hi.y - lo.y),
            )
            .unwrap();
            let mut boxes = vec![inside];
            if matches!(s.definition(), Definition::Revolution(_)) {
                boxes.push(Extent2d::from_bounds(0.6, lo.y, 1.4, hi.y).unwrap());
                boxes.push(Extent2d::from_bounds(-0.3, lo.y, 0.2, hi.y).unwrap());
            }
            for b in boxes {
                let n = s.to_nurbs(&b).unwrap();
                assert!(n.domain().are_same(&b));
                for t in grid() {
                    let uv = Point2::new(
                        b.min().x + (b.max().x - b.min().x) * t.x,
                        b.min().y + (b.max().y - b.min().y) * t.y,
                    );
                    let p = Point3::from_array(n.point(uv.to_array(), ABOVE).unwrap());
                    assert!(near(p, s.point(uv), 1e-13 * sc), "{uv:?}");
                }
            }
            let past = Extent2d::from_bounds(lo.x, lo.y, hi.x, hi.y + 1.0).unwrap();
            if !matches!(s.domain().v, ParamDomain::Periodic { .. }) {
                let err = s.to_nurbs(&past).unwrap_err();
                assert_eq!(err.name(), "extent_outside_domain");
            }
        }
    }

    /// Reversal is exact and its own inverse; a transform maps every point.
    #[test]
    fn reversal_and_transform_map_every_point() {
        let maps = [
            Affine::rotation(Point3::new(1.0, 2.0, 0.5), Vec3::new(0.3, 1.0, -0.2), 0.7).unwrap(),
            Affine::uniform_scaling(Point3::new(-1.0, 0.0, 2.0), 2.5).unwrap(),
            Affine::reflection(Point3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 0.5, 0.0)).unwrap(),
        ];
        let shear = Affine::from_axes(
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.7, 1.0, 0.0),
            Vec3::new(0.0, 0.2, 1.5),
            Point3::new(1.0, 1.0, 1.0),
        );
        for s in all() {
            let sc = scale(&s);
            for axis in [ParamAxis::U, ParamAxis::V] {
                let (r, m) = s.reversed(axis);
                assert_eq!(r.reversed(axis).0, s);
                for uv in samples(&s) {
                    assert!(
                        near(r.point(m.apply(uv)), s.point(uv), 1e-15 * sc),
                        "{uv:?}"
                    );
                }
            }
            let mut all_maps = maps.to_vec();
            if matches!(s.definition(), Definition::Extrusion(_)) {
                all_maps.push(shear);
            } else {
                assert_eq!(s.transformed(&shear).unwrap_err().name(), "map_not_similar");
            }
            for a in &all_maps {
                let (t, m) = s.transformed(a).unwrap();
                assert_eq!(m, UvMap::IDENTITY);
                for uv in samples(&s).into_iter().step_by(5) {
                    let want = a.apply(s.point(uv));
                    assert!(near(t.point(uv), want, 1e-13 * sc * 3.0), "{uv:?}");
                }
            }
        }
    }

    /// Inversion reads the net: a point on the surface inverts to itself within the bound.
    #[test]
    fn inversion_finds_points_on_the_surface() {
        for s in all() {
            let surf = Surface::from(s.clone());
            let sc = scale(&s);
            for uv in samples(&s).into_iter().step_by(17) {
                let p = s.point(uv);
                let inv = surf
                    .invert(p, None, None, &mut Budget::new(1_000_000), &Stop::new())
                    .unwrap();
                let f = inv.foot();
                assert!(f.distance <= 1e-9 * sc, "{uv:?} {}", f.distance);
            }
        }
    }

    #[test]
    fn refusals_are_typed() {
        let l = line(Point3::new(0.0, 0.0, 0.0), Point3::new(0.0, 0.0, 1.0));
        let on = Revolution::new(&l, ext(0.0, 1.0), Point3::ORIGIN, Vec3::Z).unwrap_err();
        assert_eq!(
            (on.name(), on.kind()),
            ("procedural_on_axis", ErrorKind::Refused)
        );
        let z = Revolution::new(&l, ext(0.0, 1.0), Point3::ORIGIN, Vec3::splat(0.0)).unwrap_err();
        assert_eq!(z, ProceduralError::ZeroDirection);
        let flat = Extrusion::new(&l, ext(0.0, 1.0), Vec3::new(0.0, 0.0, 2.0), ext(0.0, 1.0));
        assert_eq!(flat.unwrap_err(), ProceduralError::Flat);
        let c =
            Curve::Circle(Circle::new(Frame::WORLD, Length::from_mm(1.0), Sense::Forward).unwrap());
        let part = Revolution::new(&c, ext(0.0, 1.0), Point3::new(5.0, 0.0, 0.0), Vec3::Y);
        assert!(matches!(part.unwrap_err(), ProceduralError::Range { .. }));
        let b = bezier();
        let short = Revolution::new(&b, ext(0.0, 0.5), Point3::ORIGIN, Vec3::Z).unwrap_err();
        assert_eq!(short.name(), "procedural_range");
        let e = SurfaceError::Procedural(short);
        assert_eq!(
            (e.kind(), e.name()),
            (ErrorKind::Refused, "procedural_failed")
        );
        assert!(std::error::Error::source(&e).is_some());
        assert!(!ProceduralError::Flat.to_string().is_empty());
    }

    // ------------------------------------------------------------ the screw sweep

    fn screw(o: Point3, k: Vec3, pitch: f64, turn: Extent1d, law: Option<RadialLaw>) -> Screw {
        Screw::new(o, k, pitch, turn, law, Resolution::DEFAULT).unwrap()
    }

    fn swept(g: &Curve, range: Extent1d, m: Screw) -> Procedural {
        Procedural::new(Sweep::new(g, range, m).unwrap()).unwrap()
    }

    fn the_screw(p: &Procedural) -> &Screw {
        let Definition::Sweep(s) = p.definition() else {
            unreachable!()
        };
        let Motion::Screw(m) = s.motion();
        m
    }

    /// A line with a law, a cubic about a skew axis, and a coil: the circle in a plane of
    /// the axis, two whole turns.
    fn sweeps() -> Vec<Procedural> {
        let law = RadialLaw::new(Vec3::X, vec![0.0, 2.0], vec![0.0, 0.5]).unwrap();
        let a = swept(
            &line(Point3::new(4.0, 0.0, 0.0), Point3::new(5.0, 0.0, 1.0)),
            ext(0.0, 1.0),
            screw(Point3::ORIGIN, Vec3::Z, 1.5, ext(0.0, 3.0), Some(law)),
        );
        let b = swept(
            &bezier(),
            ext(0.0, 1.0),
            screw(
                Point3::new(0.5, 0.25, -1.0),
                Vec3::new(1.0, 1.0, 1.0),
                -2.0,
                ext(-1.0, 2.0),
                None,
            ),
        );
        let f = Frame::new(Point3::new(6.0, 0.0, 0.0), Vec3::Y, Vec3::X).unwrap();
        let c = Curve::Circle(Circle::new(f, Length::from_mm(1.5), Sense::Forward).unwrap());
        let coil = swept(
            &c,
            ext(0.0, core::f64::consts::TAU),
            screw(
                Point3::ORIGIN,
                Vec3::Z,
                3.0,
                ext(0.0, 2.0 * core::f64::consts::TAU),
                None,
            ),
        );
        vec![a, b, coil]
    }

    /// The screw of a point is the helix, and of a line along the axis the cylinder: on the
    /// line (3, 0, 0) to (3, 0, 1) about z with pitch `pi`, `S(u, v)` is
    /// `(3 cos theta, 3 sin theta, v + theta / 2)`, `theta` the definition's angle at `u`, the
    /// stations' angles their own.
    #[test]
    fn a_point_screwed_is_the_helix_and_a_line_the_cylinder() {
        let m = screw(
            Point3::ORIGIN,
            Vec3::Z,
            core::f64::consts::PI,
            ext(0.0, 5.0),
            None,
        );
        let s = swept(
            &line(Point3::new(3.0, 0.0, 0.0), Point3::new(3.0, 0.0, 1.0)),
            ext(0.0, 1.0),
            m,
        );
        let m = the_screw(&s);
        for &t in m.stations() {
            assert_eq!(m.turn_at(t), t);
        }
        let mut last = f64::NEG_INFINITY;
        for i in 0..=2000 {
            let u = 5.0 * f64::from(i) / 2000.0;
            let th = m.turn_at(u);
            assert!(th > last && (th - u).abs() <= 0.1, "{u}");
            last = th;
            for j in 0..=4 {
                let v = f64::from(j) / 4.0;
                let p = s.point(Point2::new(u, v));
                let (sn, c) = fmath::sin_cos(th);
                let want = Point3::new(3.0 * c, 3.0 * sn, v + 0.5 * th);
                assert!(near(p, want, 1e-14), "{u} {v} {p:?}");
                assert!(((p.x * p.x + p.y * p.y).sqrt() - 3.0).abs() <= 1e-14);
            }
        }
        assert_eq!(s.domain().u, ParamDomain::Bounded(ext(0.0, 5.0)));
        assert_eq!(s.domain().v, ParamDomain::Bounded(ext(0.0, 1.0)));
        assert!(s.closed_form().is_none());
    }

    /// The law moves the radius: the line's foot at (4, 0, 0) is at radius `4 + 0.5 theta / 2`
    /// on the law's first stretch and `4.5` past it.
    #[test]
    fn the_law_moves_the_radius() {
        let s = sweeps().swap_remove(0);
        let m = the_screw(&s);
        for i in 0..=300 {
            let u = 3.0 * f64::from(i) / 300.0;
            let th = m.turn_at(u);
            let p = s.point(Point2::new(u, 0.0));
            let want = 4.0 + m.law().unwrap().offset(th);
            assert!(
                ((p.x * p.x + p.y * p.y).sqrt() - want).abs() <= 1e-14,
                "{u}"
            );
            assert!(
                (p.z - 1.5 * th / core::f64::consts::TAU).abs() <= 1e-14,
                "{u}"
            );
        }
        assert_eq!(m.law().unwrap().offset(1.0), 0.25);
        assert_eq!(m.law().unwrap().offset(2.5), 0.5);
    }

    /// The net is the definition within the screw's gap plus the net's rounding; the gap is
    /// inside the resolution's approximation and reached (a dense look at a widest span finds
    /// at least half of it).
    #[test]
    fn the_net_is_the_screw_within_its_gap() {
        let goal = Resolution::DEFAULT.approx().mm();
        for s in sweeps() {
            let m = the_screw(&s);
            let sc = scale(&s);
            // The sweep's net rounds its trigonometry at every station.
            let round = 256.0 * f64::EPSILON * sc;
            assert!(m.approx() > 0.0 && m.approx() <= goal, "{}", m.approx());
            assert!(
                s.bound() >= m.approx() && s.bound() <= m.approx() + round,
                "{} {} {round}",
                s.bound(),
                m.approx()
            );
            let st = m.stations();
            let widest = st.windows(2).enumerate().fold((0, 0.0_f64), |b, (i, w)| {
                if w[1] - w[0] > b.1 {
                    (i, w[1] - w[0])
                } else {
                    b
                }
            });
            let mut seen = 0.0_f64;
            let e = s.net().domain();
            let vr = ext(e.min().y, e.max().y);
            for i in 0..m.spans() {
                let dense = if i == widest.0 { 400 } else { 4 };
                for a in 0..=dense {
                    let u = st[i] + (st[i + 1] - st[i]) * f64::from(a) / f64::from(dense);
                    for b in 0..=6 {
                        let uv = Point2::new(u, vr.min() + vr.length() * f64::from(b) / 6.0);
                        let n = s.net().point(uv.to_array(), ABOVE).unwrap();
                        let gap = (Point3::from_array(n) - s.point(uv)).length();
                        assert!(gap <= s.bound() + round, "{uv:?} {gap} {}", s.bound());
                        seen = seen.max(gap);
                    }
                }
            }
            assert!(seen >= 0.5 * m.approx(), "{seen} {}", m.approx());
        }
    }

    /// Whether `u` is clear of every station by `h` (the turn's second partials step there).
    fn clear(m: &Screw, u: f64, h: f64) -> bool {
        m.stations().iter().all(|&s| (s - u).abs() > h)
    }

    /// The jet's partials are central differences of the point (first order) and of the
    /// first-order jet (second), away from the stations; at a station the side picks the span.
    #[test]
    fn the_sweep_jet_is_the_derivative_of_the_point() {
        let h = 1e-6;
        for s in sweeps() {
            let sc = scale(&s);
            let m = the_screw(&s).clone();
            let diff = |uv: Point2, du: bool, of: (usize, usize)| -> Vec3 {
                let e = if du {
                    Point2::new(h, 0.0)
                } else {
                    Point2::new(0.0, h)
                };
                let at = |q: Point2| s.jet::<1>(q, ABOVE).get(of.0, of.1).unwrap();
                (at(Point2::new(uv.x + e.x, uv.y + e.y)) - at(Point2::new(uv.x - e.x, uv.y - e.y)))
                    * (0.5 / h)
            };
            for uv in samples(&s).into_iter().step_by(7) {
                // The generator's knots step its second partials too (a quarter arc's).
                let vk = s.net().knots(ParamAxis::V);
                if !clear(&m, uv.x, 4.0 * h) || vk.iter().any(|&k| (k - uv.y).abs() <= 4.0 * h) {
                    continue;
                }
                let j = s.jet::<2>(uv, ABOVE);
                let p0 = j.get(0, 0).unwrap().to_point();
                assert!(
                    p0.distance(s.point(uv)) <= 16.0 * f64::EPSILON * sc,
                    "{uv:?}"
                );
                let checks = [
                    ((1, 0), true, (0, 0)),
                    ((0, 1), false, (0, 0)),
                    ((2, 0), true, (1, 0)),
                    ((1, 1), false, (1, 0)),
                    ((0, 2), false, (0, 1)),
                ];
                for (want, du, of) in checks {
                    let fd = diff(uv, du, of);
                    let got = j.get(want.0, want.1).unwrap();
                    assert!(
                        (fd - got).length() <= 1e-5 * sc,
                        "{uv:?} {want:?} {fd:?} {got:?}"
                    );
                }
            }
            // At an inner station the two sides are each their span's.
            if let Some(&t) = m.stations().get(1) {
                let e = s.net().domain();
                let vr = ext(e.min().y, e.max().y);
                let uv = Point2::new(t, vr.min() + 0.3 * vr.length());
                for side in [Approach::Above, Approach::Below] {
                    let j = s.jet::<1>(uv, [side, Approach::Above]);
                    let off = if side == Approach::Above { 1e-7 } else { -1e-7 };
                    let fd = (s.point(Point2::new(t + off, uv.y)) - s.point(uv)) * (1.0 / off);
                    assert!(
                        (fd - j.get(1, 0).unwrap()).length() <= 1e-5 * sc,
                        "{side:?}"
                    );
                }
            }
        }
    }

    /// The certified box and the range along a direction hold the definition's points.
    #[test]
    fn sweep_boxes_hold_the_definition() {
        let mut rng = Rng::new(0x5c3e);
        let dir = Vec3::new(0.3, -1.0, 0.7);
        for s in sweeps() {
            let surf = Surface::from(s.clone());
            let e = s.net().domain();
            let (lo, hi) = (e.min(), e.max());
            for k in 0..20 {
                let pick = |rng: &mut Rng, a: f64, b: f64| a + (b - a) * rng.next_f64();
                let (u0, v0) = (pick(&mut rng, lo.x, hi.x), pick(&mut rng, lo.y, hi.y));
                let (u1, v1) = if k % 4 == 0 {
                    (u0, v0)
                } else {
                    (pick(&mut rng, u0, hi.x), pick(&mut rng, v0, hi.y))
                };
                let uv = Extent2d::from_bounds(u0, v0, u1, v1).unwrap();
                let b = surf.box_over(uv).unwrap();
                let a = surf.range_along(dir, uv).unwrap();
                for i in 0..=8 {
                    for j in 0..=8 {
                        let t = Point2::new(
                            u0 + (u1 - u0) * f64::from(i) / 8.0,
                            v0 + (v1 - v0) * f64::from(j) / 8.0,
                        );
                        let p = s.point(t);
                        let (m, n) = (b.min(), b.max());
                        assert!(
                            m.x <= p.x
                                && p.x <= n.x
                                && m.y <= p.y
                                && p.y <= n.y
                                && m.z <= p.z
                                && p.z <= n.z,
                            "{t:?}"
                        );
                        let d = dir.dot(p.to_vec());
                        assert!(a.lo <= d && d <= a.hi, "{t:?}");
                    }
                }
            }
        }
    }

    /// The net is not the definition, so a sweep's exact NURBS is refused by name.
    #[test]
    fn a_sweep_has_no_exact_nurbs() {
        for s in sweeps() {
            let e = s.net().domain();
            let err = s.to_nurbs(&e).unwrap_err();
            assert_eq!(
                (err.name(), err.kind()),
                ("procedural_failed", ErrorKind::Unsupported)
            );
            let SurfaceError::Procedural(ProceduralError::Inexact { bound }) = err else {
                unreachable!("{err:?}")
            };
            assert_eq!(bound, s.bound());
        }
    }

    /// Reversal is exact and its own inverse; a similarity maps every point; a shear is
    /// refused.
    #[test]
    fn sweep_reversal_and_transform_map_every_point() {
        let maps = [
            Affine::rotation(Point3::new(1.0, 2.0, 0.5), Vec3::new(0.3, 1.0, -0.2), 0.7).unwrap(),
            Affine::uniform_scaling(Point3::new(-1.0, 0.0, 2.0), 2.5).unwrap(),
            Affine::reflection(Point3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 0.5, 0.0)).unwrap(),
        ];
        let shear = Affine::from_axes(
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.7, 1.0, 0.0),
            Vec3::new(0.0, 0.2, 1.5),
            Point3::new(1.0, 1.0, 1.0),
        );
        for s in sweeps() {
            let sc = scale(&s);
            for axis in [ParamAxis::U, ParamAxis::V] {
                let (r, m) = s.reversed(axis);
                assert_eq!(r.reversed(axis).0, s);
                for uv in samples(&s) {
                    assert!(
                        near(r.point(m.apply(uv)), s.point(uv), 8.0 * f64::EPSILON * sc),
                        "{axis:?} {uv:?}"
                    );
                }
            }
            assert_eq!(s.transformed(&shear).unwrap_err().name(), "map_not_similar");
            for a in &maps {
                let (t, m) = s.transformed(a).unwrap();
                assert_eq!(m, UvMap::IDENTITY);
                for uv in samples(&s).into_iter().step_by(5) {
                    let want = a.apply(s.point(uv));
                    assert!(near(t.point(uv), want, 3e-13 * sc), "{uv:?}");
                }
            }
        }
    }

    /// A point on the sweep inverts to itself, to the definition, not to the net's gap.
    #[test]
    fn sweep_inversion_finds_points_on_the_surface() {
        for s in sweeps() {
            let surf = Surface::from(s.clone());
            let sc = scale(&s);
            for uv in samples(&s).into_iter().step_by(17) {
                let p = s.point(uv);
                let inv = surf
                    .invert(p, None, None, &mut Budget::new(1_000_000), &Stop::new())
                    .unwrap();
                let f = inv.foot();
                assert!(f.distance <= 1e-9 * sc, "{uv:?} {}", f.distance);
            }
        }
    }

    #[test]
    fn sweep_refusals_are_typed() {
        let m = screw(Point3::ORIGIN, Vec3::Z, 1.0, ext(0.0, 1.0), None);
        let on = line(Point3::new(0.0, 0.0, 0.0), Point3::new(0.0, 0.0, 1.0));
        let e = Sweep::new(&on, ext(0.0, 1.0), m.clone()).unwrap_err();
        assert_eq!(
            (e.name(), e.kind()),
            ("procedural_on_axis", ErrorKind::Refused)
        );
        // A law that moves the axis line off it is a sweep.
        let law = RadialLaw::new(Vec3::X, vec![0.0, 1.0], vec![1.0, 2.0]).unwrap();
        let moved = screw(Point3::ORIGIN, Vec3::Z, 1.0, ext(0.0, 1.0), Some(law));
        assert!(Sweep::new(&on, ext(0.0, 1.0), moved).is_ok());
        let zero = Screw::new(
            Point3::ORIGIN,
            Vec3::Z,
            0.0,
            ext(0.0, 1.0),
            None,
            Resolution::DEFAULT,
        );
        assert_eq!(zero.unwrap_err().name(), "procedural_zero_pitch");
        let tilt =
            RadialLaw::new(Vec3::new(1.0, 0.0, 1.0), vec![0.0, 1.0], vec![0.0, 1.0]).unwrap();
        let tilted = Screw::new(
            Point3::ORIGIN,
            Vec3::Z,
            1.0,
            ext(0.0, 1.0),
            Some(tilt),
            Resolution::DEFAULT,
        );
        assert_eq!(tilted.unwrap_err(), ProceduralError::Law);
        let fine = Resolution::new(Length::from_mm(1e-12)).unwrap();
        let many = Screw::new(Point3::ORIGIN, Vec3::Z, 1e3, ext(0.0, 1e3), None, fine).unwrap_err();
        assert_eq!(
            (many.name(), many.kind()),
            ("procedural_too_fine", ErrorKind::Exhausted)
        );
        let e = SurfaceError::Procedural(ProceduralError::Inexact { bound: 1e-7 });
        assert_eq!(e.kind(), ErrorKind::Unsupported);
        assert!(!ProceduralError::ZeroPitch.to_string().is_empty());
    }
}
