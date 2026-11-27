// Written for solidcorex. Apache-2.0.
//! The geometry records (research 63 RE2): every curve and surface kind a body holds, written
//! once through [`Sink`] and read once through [`Source`], so binary and text share one
//! field order.
//!
//! A record stores what the type's constructor takes and nothing derived: a frame's origin,
//! x and z (its y is `z × x`), a kind's sizes and [`Sense`], a cone's half-angle (its slope
//! is computed), a NURBS curve's degree, knots, points and weights (the point count is the
//! knot count less the degree less one). Nothing is built unchecked: each read value goes
//! back through the type's own checked constructor, and a value that constructor refuses is
//! its cause, framed ([`ReadCause::Frame`], [`ReadCause::Curve`], [`ReadCause::Surface`]).
//!
//! Every record is canonical: `encode(decode(x)) == x` for every `x` that decodes, and
//! `decode(encode(v)) == v` to the bit for every value the constructors and edits build.
//! Two places need care for that:
//!
//! - A frame's axes are read back as they are ([`Frame::from_axes`]), not normalized again
//!   ([`Frame::new`] rounds them a second time and moves an ulp in most frames).
//! - A rational NURBS value keeps both its points `P` and its homogeneous points `w P`; which
//!   one its constructor computed from the other depends on how it was built
//!   ([`NurbsCurve::new`] computes `w P`, [`NurbsCurve::from_homogeneous`] computes `P`). The
//!   record stores the pair the value was built from, as a [`Form`]: the points when every
//!   `w P` is its point times its weight, rounded; else the homogeneous points when every
//!   `P` is its `w P` over its weight, rounded. A homogeneous record whose points would
//!   also pass the first test is refused ([`ReadCause::NonCanonical`]), so each value has
//!   one spelling; a value that passes neither is not written
//!   ([`WriteCause::MixedRational`]): no constructor or edit builds one.

use std::sync::Arc;

use scx_curve::arcs::Arcs;
use scx_curve::{
    AngleAxis, AngleLine2, AngleWave2, Chart, ChartKind, Circle, Circle2, Curve, Curve2,
    CurveError, Ellipse, Ellipse2, Hyperbola, Hyperbola2, Line, Line2, OnChart, Parabola,
    Parabola2,
};
use scx_num::Length;
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Frame2, Sense};
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{KnotVector, NurbsCurve};
use scx_nurbs_surface::{Net, NurbsSurface, NurbsSurfaceError, ParamAxis};
use scx_surface::{Cone, Cylinder, Plane, Sphere, Surface, SurfaceError, Torus};

use crate::error::{ReadCause, ReadError, WriteCause, WriteError};
use crate::tag::{Tag, tags};
use crate::wire::{Sink, Source};

/// A value that writes itself to a [`Sink`] and reads itself back from a [`Source`], to the
/// bit. A record nested in a larger one: no magic, no version (the enclosing record's
/// header carries them).
pub trait Record: Sized {
    /// Write the value.
    ///
    /// # Errors
    ///
    /// The sink's [`WriteError`] (a non-finite number, a list over `u32::MAX`), or one of
    /// this module's: [`WriteCause::MixedRational`], [`WriteCause::NoRecord`].
    fn put<S: Sink>(&self, w: &mut S) -> Result<(), WriteError>;

    /// Read a value, rebuilt through its type's checked constructor.
    ///
    /// # Errors
    ///
    /// The source's [`ReadError`], or the constructor's cause framed as
    /// [`ReadCause::Frame`], [`ReadCause::Curve`] or [`ReadCause::Surface`], or
    /// [`ReadCause::NonCanonical`].
    fn take<R: Source>(r: &mut R) -> Result<Self, ReadError>;
}

tags! {
    /// A [`Curve`]'s kind.
    pub(crate) enum CurveKind: "curve" {
        Line = 0, "line";
        Circle = 1, "circle";
        Ellipse = 2, "ellipse";
        Parabola = 3, "parabola";
        Hyperbola = 4, "hyperbola";
        Nurbs = 5, "nurbs";
    }
}

tags! {
    /// A [`Curve2`]'s kind: [`CurveKind`]'s codes and words, and the two angle curves.
    pub(crate) enum Curve2Kind: "curve2" {
        Line = 0, "line";
        Circle = 1, "circle";
        Ellipse = 2, "ellipse";
        Parabola = 3, "parabola";
        Hyperbola = 4, "hyperbola";
        Nurbs = 5, "nurbs";
        AngleLine = 6, "angle_line";
        AngleWave = 7, "angle_wave";
        OnChart = 8, "on_chart";
    }
}

tags! {
    /// A [`Chart`]'s kind.
    pub(crate) enum ChartTag: "chart" {
        Plane = 0, "plane";
        Cylinder = 1, "cylinder";
        Cone = 2, "cone";
        Sphere = 3, "sphere";
        Torus = 4, "torus";
    }
}

tags! {
    /// A [`Surface`]'s kind.
    pub(crate) enum SurfaceKind: "surface" {
        Plane = 0, "plane";
        Cylinder = 1, "cylinder";
        Cone = 2, "cone";
        Sphere = 3, "sphere";
        Torus = 4, "torus";
        Nurbs = 5, "nurbs";
    }
}

tags! {
    /// [`Sense`] on the wire.
    pub(crate) enum SenseTag: "sense" {
        Forward = 0, "forward";
        Reversed = 1, "reversed";
    }
}

tags! {
    /// [`AngleAxis`] on the wire.
    pub(crate) enum AxisTag: "angle_axis" {
        U = 0, "u";
        V = 1, "v";
    }
}

tags! {
    /// An [`Arcs`]'s sweep: a turn (its period follows) or the half turn.
    pub(crate) enum SweepTag: "sweep" {
        Turn = 0, "turn";
        Half = 1, "half";
    }
}

tags! {
    /// Which control data a NURBS record stores; see the [module](self) docs.
    pub(crate) enum Form: "nurbs_form" {
        /// No weights: the points.
        Polynomial = 0, "polynomial";
        /// The points, then the weights; read through `new`, which computes `w P`.
        Points = 1, "points";
        /// The homogeneous points `w P`, then the weights; read through
        /// `from_homogeneous`, which computes `P`.
        Homogeneous = 2, "homogeneous";
    }
}

// ---- small parts -------------------------------------------------------------------------

pub(crate) fn put_all<S: Sink>(w: &mut S, xs: &[f64]) -> Result<(), WriteError> {
    for &x in xs {
        w.f64(x)?;
    }
    Ok(())
}

fn take_n<const N: usize, R: Source>(r: &mut R) -> Result<[f64; N], ReadError> {
    let mut out = [0.0; N];
    for x in &mut out {
        *x = r.f64()?;
    }
    Ok(out)
}

pub(crate) fn put_sense<S: Sink>(w: &mut S, sense: Sense) {
    w.tag(match sense {
        Sense::Forward => SenseTag::Forward,
        Sense::Reversed => SenseTag::Reversed,
    });
}

pub(crate) fn take_sense<R: Source>(r: &mut R) -> Result<Sense, ReadError> {
    Ok(match r.tag()? {
        SenseTag::Forward => Sense::Forward,
        SenseTag::Reversed => Sense::Reversed,
    })
}

fn put_length<S: Sink>(w: &mut S, l: Length) -> Result<(), WriteError> {
    w.f64(l.mm())
}

fn take_length<R: Source>(r: &mut R) -> Result<Length, ReadError> {
    Ok(Length::from_mm(r.f64()?))
}

/// The error of a curve record that started at `at`.
fn curve_at(at: usize) -> impl FnOnce(CurveError) -> ReadError {
    move |e| ReadError::new(at, ReadCause::Curve(Box::new(e)))
}

/// The error of a surface record that started at `at`.
fn surface_at(at: usize) -> impl FnOnce(SurfaceError) -> ReadError {
    move |e| ReadError::new(at, ReadCause::Surface(Box::new(e)))
}

/// The error for a value of a kind this crate has no record for: a variant added to a
/// `#[non_exhaustive]` enum after this module.
fn no_record<S: Sink>(w: &S, record: &'static str) -> WriteError {
    WriteError::new(w.at(), WriteCause::NoRecord { record })
}

/// Origin, x, z: nine numbers, read back through [`Frame::from_axes`].
impl Record for Frame {
    fn put<S: Sink>(&self, w: &mut S) -> Result<(), WriteError> {
        let (o, x, z) = (self.origin(), *self.x(), *self.z());
        put_all(w, &[o.x, o.y, o.z, x.x, x.y, x.z, z.x, z.y, z.z])
    }

    fn take<R: Source>(r: &mut R) -> Result<Self, ReadError> {
        let at = r.at();
        let [ox, oy, oz, xx, xy, xz, zx, zy, zz] = take_n::<9, _>(r)?;
        Self::from_axes(
            Point3::new(ox, oy, oz),
            Vec3::new(xx, xy, xz),
            Vec3::new(zx, zy, zz),
        )
        .map_err(|e| ReadError::new(at, ReadCause::Frame(e)))
    }
}

/// Origin, x: four numbers, read back through [`Frame2::from_axis`].
impl Record for Frame2 {
    fn put<S: Sink>(&self, w: &mut S) -> Result<(), WriteError> {
        let (o, x) = (self.origin(), *self.x());
        put_all(w, &[o.x, o.y, x.x, x.y])
    }

    fn take<R: Source>(r: &mut R) -> Result<Self, ReadError> {
        let at = r.at();
        let [ox, oy, xx, xy] = take_n::<4, _>(r)?;
        Self::from_axis(Point2::new(ox, oy), Vec2::new(xx, xy))
            .map_err(|e| ReadError::new(at, ReadCause::Frame(e)))
    }
}

fn put_arcs<S: Sink>(w: &mut S, arcs: Arcs) -> Result<(), WriteError> {
    if arcs.is_turn() {
        w.tag(SweepTag::Turn);
        w.f64(arcs.length())
    } else {
        w.tag(SweepTag::Half);
        Ok(())
    }
}

fn take_arcs<R: Source>(r: &mut R) -> Result<Arcs, ReadError> {
    match r.tag()? {
        SweepTag::Turn => {
            let at = r.at();
            Arcs::try_turn(r.f64()?).map_err(curve_at(at))
        }
        SweepTag::Half => Ok(Arcs::half()),
    }
}

fn put_axis<S: Sink>(w: &mut S, axis: AngleAxis) {
    w.tag(match axis {
        AngleAxis::U => AxisTag::U,
        AngleAxis::V => AxisTag::V,
    });
}

fn take_axis<R: Source>(r: &mut R) -> Result<AngleAxis, ReadError> {
    Ok(match r.tag()? {
        AxisTag::U => AngleAxis::U,
        AxisTag::V => AngleAxis::V,
    })
}

// ---- NURBS -------------------------------------------------------------------------------

/// Whether each `wp[i]` is `p[i]` times `w[i]`, rounded, to the bit: what `new` computes.
fn products<const D: usize>(p: &[[f64; D]], w: &[f64], wp: &[[f64; D]]) -> bool {
    p.iter().zip(w).zip(wp).all(|((p, &w), wp)| {
        p.iter()
            .zip(wp)
            .all(|(&x, &h)| (x * w).to_bits() == h.to_bits())
    })
}

/// Whether each `p[i]` is `wp[i]` over `w[i]`, rounded, to the bit: what
/// `from_homogeneous` computes.
fn quotients<const D: usize>(p: &[[f64; D]], w: &[f64], wp: &[[f64; D]]) -> bool {
    p.iter().zip(w).zip(wp).all(|((p, &w), wp)| {
        p.iter()
            .zip(wp)
            .all(|(&x, &h)| (h / w).to_bits() == x.to_bits())
    })
}

/// The form a rational value with points `p`, weights `w` and homogeneous points `wp` is
/// written in, or `None` when neither rebuilds it.
fn rational_form<const D: usize>(p: &[[f64; D]], w: &[f64], wp: &[[f64; D]]) -> Option<Form> {
    if products(p, w, wp) {
        Some(Form::Points)
    } else if quotients(p, w, wp) {
        Some(Form::Homogeneous)
    } else {
        None
    }
}

/// The form, then the points or homogeneous points, then the weights of a rational value.
fn put_control<S: Sink, const D: usize>(
    w: &mut S,
    points: &[[f64; D]],
    rational: Option<(&[f64], &[[f64; D]])>,
) -> Result<(), WriteError> {
    w.line();
    let Some((weights, weighted)) = rational else {
        w.tag(Form::Polynomial);
        return put_points(w, points);
    };
    let form = rational_form(points, weights, weighted)
        .ok_or_else(|| WriteError::new(w.at(), WriteCause::MixedRational { record: "nurbs" }))?;
    w.tag(form);
    put_points(
        w,
        if form == Form::Points {
            points
        } else {
            weighted
        },
    )?;
    w.line();
    put_all(w, weights)
}

fn put_points<S: Sink, const D: usize>(w: &mut S, points: &[[f64; D]]) -> Result<(), WriteError> {
    w.line();
    for p in points {
        put_all(w, p)?;
    }
    Ok(())
}

/// `n` points, grown as they are read: a corrupt count costs no more than the input holds.
fn take_points<R: Source, const D: usize>(r: &mut R, n: usize) -> Result<Vec<[f64; D]>, ReadError> {
    r.line();
    let mut out = Vec::with_capacity(n.min(r.remaining()));
    for _ in 0..n {
        out.push(take_n::<D, _>(r)?);
    }
    Ok(out)
}

fn take_weights<R: Source>(r: &mut R, n: usize) -> Result<Vec<f64>, ReadError> {
    r.line();
    let mut out = Vec::with_capacity(n.min(r.remaining()));
    for _ in 0..n {
        out.push(r.f64()?);
    }
    Ok(out)
}

fn put_knots<S: Sink>(w: &mut S, knots: &KnotVector) -> Result<(), WriteError> {
    let degree = u32::try_from(knots.degree()).map_err(|_| {
        WriteError::new(
            w.at(),
            WriteCause::TooLong {
                len: knots.degree(),
            },
        )
    })?;
    w.u32(degree);
    w.line();
    w.list(knots.knots(), |w, &k| w.f64(k))
}

/// The degree and the knots, unchecked: the caller builds the [`KnotVector`] and frames
/// its error.
fn take_knots<R: Source>(r: &mut R) -> Result<(usize, Vec<f64>), ReadError> {
    let degree = r.u32()? as usize;
    r.line();
    let knots = r.list(Source::f64)?;
    Ok((degree, knots))
}

/// Degree, knots, form, then the points or homogeneous points and the weights.
impl<const D: usize> Record for NurbsCurve<D> {
    fn put<S: Sink>(&self, w: &mut S) -> Result<(), WriteError> {
        put_knots(w, self.knot_vector())?;
        put_control(w, self.points(), self.weights().zip(self.weighted_points()))
    }

    fn take<R: Source>(r: &mut R) -> Result<Self, ReadError> {
        let at = r.at();
        let (degree, knots) = take_knots(r)?;
        let knots = KnotVector::new(degree, knots)
            .map_err(|e| ReadError::new(at, ReadCause::Curve(Box::new(CurveError::Nurbs(e)))))?;
        let n = knots.point_count();
        r.line();
        let form: Form = r.tag()?;
        let data = take_points::<R, D>(r, n)?;
        let nurbs = |e| CurveError::Nurbs(e);
        match form {
            Form::Polynomial => Self::new(knots, data, None).map_err(|e| curve_at(at)(nurbs(e))),
            Form::Points => {
                let weights = take_weights(r, n)?;
                Self::new(knots, data, Some(weights)).map_err(|e| curve_at(at)(nurbs(e)))
            }
            Form::Homogeneous => {
                let weights = take_weights(r, n)?;
                let c = Self::from_homogeneous(knots, data, weights)
                    .map_err(|e| curve_at(at)(nurbs(e)))?;
                match (c.weights(), c.weighted_points()) {
                    (Some(w), Some(wp)) if !products(c.points(), w, wp) => Ok(c),
                    _ => Err(ReadError::new(
                        at,
                        ReadCause::NonCanonical { record: "nurbs" },
                    )),
                }
            }
        }
    }
}

/// The u degree and knots, the v degree and knots, the form, then the net row by row (u
/// index major) and its weights.
impl<const D: usize> Record for NurbsSurface<D> {
    fn put<S: Sink>(&self, w: &mut S) -> Result<(), WriteError> {
        put_knots(w, self.knot_vector(ParamAxis::U))?;
        w.line();
        put_knots(w, self.knot_vector(ParamAxis::V))?;
        let rational = self
            .weights()
            .zip(self.weighted_points())
            .map(|(w, wp)| (w.points(), wp.points()));
        put_control(w, self.points().points(), rational)
    }

    fn take<R: Source>(r: &mut R) -> Result<Self, ReadError> {
        let at = r.at();
        let fail = |e: NurbsSurfaceError| {
            ReadError::new(at, ReadCause::Surface(Box::new(SurfaceError::Nurbs(e))))
        };
        let axis_knots = |r: &mut R, axis| {
            let (degree, knots) = take_knots(r)?;
            KnotVector::new(degree, knots)
                .map_err(|cause| fail(NurbsSurfaceError::BadKnots { axis, cause }))
        };
        let u = axis_knots(r, ParamAxis::U)?;
        r.line();
        let v = axis_knots(r, ParamAxis::V)?;
        let (rows, cols) = (u.point_count(), v.point_count());
        // Each count is below the input's length, so the product does not overflow a
        // 64-bit `usize`; on a narrower one an overflow is more than the input holds.
        let n = rows
            .checked_mul(cols)
            .ok_or_else(|| ReadError::new(r.at(), ReadCause::Truncated))?;
        r.line();
        let form: Form = r.tag()?;
        let data = Net::new(rows, cols, take_points::<R, D>(r, n)?).map_err(fail)?;
        let weights = match form {
            Form::Polynomial => None,
            Form::Points | Form::Homogeneous => {
                Some(Net::new(rows, cols, take_weights(r, n)?).map_err(fail)?)
            }
        };
        match (form, weights) {
            (Form::Homogeneous, Some(weights)) => {
                let s = Self::from_homogeneous(u, v, data, weights).map_err(fail)?;
                match (s.weights(), s.weighted_points()) {
                    (Some(w), Some(wp))
                        if !products(s.points().points(), w.points(), wp.points()) =>
                    {
                        Ok(s)
                    }
                    _ => Err(ReadError::new(
                        at,
                        ReadCause::NonCanonical { record: "nurbs" },
                    )),
                }
            }
            (_, weights) => Self::new(u, v, data, weights).map_err(fail),
        }
    }
}

// ---- curves ------------------------------------------------------------------------------

/// The kind's tag, then its fields: a line's origin and direction; a conic's frame, sizes
/// and sense; a NURBS curve's record.
impl Record for Curve {
    fn put<S: Sink>(&self, w: &mut S) -> Result<(), WriteError> {
        match self {
            Self::Line(l) => {
                w.tag(CurveKind::Line);
                let (o, d) = (l.origin(), l.direction());
                put_all(w, &[o.x, o.y, o.z, d.x, d.y, d.z])
            }
            Self::Circle(c) => {
                w.tag(CurveKind::Circle);
                c.frame().put(w)?;
                put_length(w, c.radius())?;
                put_sense(w, c.sense());
                Ok(())
            }
            Self::Ellipse(e) => {
                w.tag(CurveKind::Ellipse);
                e.frame().put(w)?;
                put_length(w, e.semi_x())?;
                put_length(w, e.semi_y())?;
                put_sense(w, e.sense());
                Ok(())
            }
            Self::Parabola(p) => {
                w.tag(CurveKind::Parabola);
                p.frame().put(w)?;
                put_length(w, p.focal())?;
                put_sense(w, p.sense());
                Ok(())
            }
            Self::Hyperbola(h) => {
                w.tag(CurveKind::Hyperbola);
                h.frame().put(w)?;
                put_length(w, h.semi_x())?;
                put_length(w, h.semi_y())?;
                put_sense(w, h.sense());
                Ok(())
            }
            Self::Nurbs(n) => {
                w.tag(CurveKind::Nurbs);
                w.line();
                n.put(w)
            }
            _ => Err(no_record(w, CurveKind::RECORD)),
        }
    }

    fn take<R: Source>(r: &mut R) -> Result<Self, ReadError> {
        let kind: CurveKind = r.tag()?;
        let at = r.at();
        match kind {
            CurveKind::Line => {
                let [ox, oy, oz, dx, dy, dz] = take_n::<6, _>(r)?;
                Line::new(Point3::new(ox, oy, oz), Vec3::new(dx, dy, dz))
                    .map(Self::Line)
                    .map_err(curve_at(at))
            }
            CurveKind::Circle => {
                let frame = Frame::take(r)?;
                let radius = take_length(r)?;
                let sense = take_sense(r)?;
                Circle::new(frame, radius, sense)
                    .map(Self::Circle)
                    .map_err(curve_at(at))
            }
            CurveKind::Ellipse => {
                let frame = Frame::take(r)?;
                let (a, b) = (take_length(r)?, take_length(r)?);
                let sense = take_sense(r)?;
                Ellipse::new(frame, a, b, sense)
                    .map(Self::Ellipse)
                    .map_err(curve_at(at))
            }
            CurveKind::Parabola => {
                let frame = Frame::take(r)?;
                let focal = take_length(r)?;
                let sense = take_sense(r)?;
                Parabola::new(frame, focal, sense)
                    .map(Self::Parabola)
                    .map_err(curve_at(at))
            }
            CurveKind::Hyperbola => {
                let frame = Frame::take(r)?;
                let (a, b) = (take_length(r)?, take_length(r)?);
                let sense = take_sense(r)?;
                Hyperbola::new(frame, a, b, sense)
                    .map(Self::Hyperbola)
                    .map_err(curve_at(at))
            }
            CurveKind::Nurbs => {
                r.line();
                Ok(Self::Nurbs(Arc::new(NurbsCurve::take(r)?)))
            }
        }
    }
}

/// As [`Curve`] in the plane, and the angle curves: the arcs, then the fields in their
/// constructor's order.
impl Record for Curve2 {
    fn put<S: Sink>(&self, w: &mut S) -> Result<(), WriteError> {
        match self {
            Self::Line(l) => {
                w.tag(Curve2Kind::Line);
                let (o, d) = (l.origin(), l.direction());
                put_all(w, &[o.x, o.y, d.x, d.y])
            }
            Self::Circle(c) => {
                w.tag(Curve2Kind::Circle);
                c.frame().put(w)?;
                put_length(w, c.radius())?;
                put_sense(w, c.sense());
                Ok(())
            }
            Self::Ellipse(e) => {
                w.tag(Curve2Kind::Ellipse);
                e.frame().put(w)?;
                put_length(w, e.semi_x())?;
                put_length(w, e.semi_y())?;
                put_sense(w, e.sense());
                Ok(())
            }
            Self::Parabola(p) => {
                w.tag(Curve2Kind::Parabola);
                p.frame().put(w)?;
                put_length(w, p.focal())?;
                put_sense(w, p.sense());
                Ok(())
            }
            Self::Hyperbola(h) => {
                w.tag(Curve2Kind::Hyperbola);
                h.frame().put(w)?;
                put_length(w, h.semi_x())?;
                put_length(w, h.semi_y())?;
                put_sense(w, h.sense());
                Ok(())
            }
            Self::Nurbs(n) => {
                w.tag(Curve2Kind::Nurbs);
                w.line();
                n.put(w)
            }
            Self::AngleLine(a) => {
                w.tag(Curve2Kind::AngleLine);
                put_arcs(w, a.arcs())?;
                put_axis(w, a.axis());
                w.f64(a.start())?;
                put_sense(w, a.sense());
                put_all(w, &[a.other(), a.rate()])
            }
            Self::AngleWave(a) => {
                w.tag(Curve2Kind::AngleWave);
                put_arcs(w, a.arcs())?;
                w.f64(a.start())?;
                put_sense(w, a.sense());
                let (c, s) = a.amplitudes();
                put_all(w, &[a.mean(), c, s])
            }
            Self::OnChart(o) => {
                w.tag(Curve2Kind::OnChart);
                w.line();
                o.curve().put(w)?;
                o.chart().put(w)?;
                let [a, b] = o.reference();
                put_all(w, &[o.over().min(), o.over().max(), a.x, a.y, b.x, b.y])
            }
            _ => Err(no_record(w, Curve2Kind::RECORD)),
        }
    }

    fn take<R: Source>(r: &mut R) -> Result<Self, ReadError> {
        let kind: Curve2Kind = r.tag()?;
        let at = r.at();
        match kind {
            Curve2Kind::Line => {
                let [ox, oy, dx, dy] = take_n::<4, _>(r)?;
                Line2::new(Point2::new(ox, oy), Vec2::new(dx, dy))
                    .map(Self::Line)
                    .map_err(curve_at(at))
            }
            Curve2Kind::Circle => {
                let frame = Frame2::take(r)?;
                let radius = take_length(r)?;
                let sense = take_sense(r)?;
                Circle2::new(frame, radius, sense)
                    .map(Self::Circle)
                    .map_err(curve_at(at))
            }
            Curve2Kind::Ellipse => {
                let frame = Frame2::take(r)?;
                let (a, b) = (take_length(r)?, take_length(r)?);
                let sense = take_sense(r)?;
                Ellipse2::new(frame, a, b, sense)
                    .map(Self::Ellipse)
                    .map_err(curve_at(at))
            }
            Curve2Kind::Parabola => {
                let frame = Frame2::take(r)?;
                let focal = take_length(r)?;
                let sense = take_sense(r)?;
                Parabola2::new(frame, focal, sense)
                    .map(Self::Parabola)
                    .map_err(curve_at(at))
            }
            Curve2Kind::Hyperbola => {
                let frame = Frame2::take(r)?;
                let (a, b) = (take_length(r)?, take_length(r)?);
                let sense = take_sense(r)?;
                Hyperbola2::new(frame, a, b, sense)
                    .map(Self::Hyperbola)
                    .map_err(curve_at(at))
            }
            Curve2Kind::Nurbs => {
                r.line();
                Ok(Self::Nurbs(Arc::new(NurbsCurve::take(r)?)))
            }
            Curve2Kind::AngleLine => {
                let arcs = take_arcs(r)?;
                let axis = take_axis(r)?;
                let start = r.f64()?;
                let sense = take_sense(r)?;
                let [other, rate] = take_n::<2, _>(r)?;
                AngleLine2::new(arcs, axis, start, sense, other, rate)
                    .map(Self::AngleLine)
                    .map_err(curve_at(at))
            }
            Curve2Kind::AngleWave => {
                let arcs = take_arcs(r)?;
                let start = r.f64()?;
                let sense = take_sense(r)?;
                let [mean, c, s] = take_n::<3, _>(r)?;
                AngleWave2::new(arcs, start, sense, mean, c, s)
                    .map(Self::AngleWave)
                    .map_err(curve_at(at))
            }
            Curve2Kind::OnChart => {
                r.line();
                let curve = Curve::take(r)?;
                let chart = Chart::take(r)?;
                let at_over = r.at();
                let [min, max, ax, ay, bx, by] = take_n::<6, _>(r)?;
                let over = Extent1d::new(min, max)
                    .ok_or_else(|| ReadError::new(at_over, ReadCause::BadInterval))?;
                let reference = [Point2::new(ax, ay), Point2::new(bx, by)];
                OnChart::new(curve, chart, over, reference)
                    .map(Self::from)
                    .map_err(curve_at(at))
            }
        }
    }
}

/// The kind's tag, the frame, the sense, then the kind's size (a cylinder's radius, a
/// torus's major radius; the others have none).
impl Record for Chart {
    fn put<S: Sink>(&self, w: &mut S) -> Result<(), WriteError> {
        let (tag, size) = match self.kind() {
            ChartKind::Plane => (ChartTag::Plane, None),
            ChartKind::Cylinder { radius } => (ChartTag::Cylinder, Some(radius)),
            ChartKind::Cone => (ChartTag::Cone, None),
            ChartKind::Sphere => (ChartTag::Sphere, None),
            ChartKind::Torus { major } => (ChartTag::Torus, Some(major)),
            _ => return Err(no_record(w, ChartTag::RECORD)),
        };
        w.tag(tag);
        self.frame().put(w)?;
        put_sense(w, self.sense());
        match size {
            Some(l) => put_length(w, l),
            None => Ok(()),
        }
    }

    fn take<R: Source>(r: &mut R) -> Result<Self, ReadError> {
        let tag: ChartTag = r.tag()?;
        let at = r.at();
        let frame = Frame::take(r)?;
        let sense = take_sense(r)?;
        let kind = match tag {
            ChartTag::Plane => ChartKind::Plane,
            ChartTag::Cylinder => ChartKind::Cylinder {
                radius: take_length(r)?,
            },
            ChartTag::Cone => ChartKind::Cone,
            ChartTag::Sphere => ChartKind::Sphere,
            ChartTag::Torus => ChartKind::Torus {
                major: take_length(r)?,
            },
        };
        Chart::new(frame, sense, kind).map_err(curve_at(at))
    }
}

// ---- surfaces ----------------------------------------------------------------------------

/// The kind's tag, then its frame, sizes and sense (a plane has a frame only), or a NURBS
/// surface's record.
impl Record for Surface {
    fn put<S: Sink>(&self, w: &mut S) -> Result<(), WriteError> {
        match self {
            Self::Plane(p) => {
                w.tag(SurfaceKind::Plane);
                p.frame().put(w)
            }
            Self::Cylinder(c) => {
                w.tag(SurfaceKind::Cylinder);
                c.frame().put(w)?;
                put_length(w, c.radius())?;
                put_sense(w, c.sense());
                Ok(())
            }
            Self::Cone(c) => {
                w.tag(SurfaceKind::Cone);
                c.frame().put(w)?;
                put_length(w, c.radius())?;
                w.f64(c.half_angle())?;
                put_sense(w, c.sense());
                Ok(())
            }
            Self::Sphere(s) => {
                w.tag(SurfaceKind::Sphere);
                s.frame().put(w)?;
                put_length(w, s.radius())?;
                put_sense(w, s.sense());
                Ok(())
            }
            Self::Torus(t) => {
                w.tag(SurfaceKind::Torus);
                t.frame().put(w)?;
                put_length(w, t.major())?;
                put_length(w, t.minor())?;
                put_sense(w, t.sense());
                Ok(())
            }
            Self::Nurbs(n) => {
                w.tag(SurfaceKind::Nurbs);
                w.line();
                n.put(w)
            }
            _ => Err(no_record(w, SurfaceKind::RECORD)),
        }
    }

    fn take<R: Source>(r: &mut R) -> Result<Self, ReadError> {
        let kind: SurfaceKind = r.tag()?;
        let at = r.at();
        match kind {
            SurfaceKind::Plane => Ok(Self::Plane(Plane::new(Frame::take(r)?))),
            SurfaceKind::Cylinder => {
                let frame = Frame::take(r)?;
                let radius = take_length(r)?;
                let sense = take_sense(r)?;
                Cylinder::new(frame, radius, sense)
                    .map(Self::Cylinder)
                    .map_err(surface_at(at))
            }
            SurfaceKind::Cone => {
                let frame = Frame::take(r)?;
                let radius = take_length(r)?;
                let half_angle = r.f64()?;
                let sense = take_sense(r)?;
                Cone::new(frame, radius, half_angle, sense)
                    .map(Self::Cone)
                    .map_err(surface_at(at))
            }
            SurfaceKind::Sphere => {
                let frame = Frame::take(r)?;
                let radius = take_length(r)?;
                let sense = take_sense(r)?;
                Sphere::new(frame, radius, sense)
                    .map(Self::Sphere)
                    .map_err(surface_at(at))
            }
            SurfaceKind::Torus => {
                let frame = Frame::take(r)?;
                let (major, minor) = (take_length(r)?, take_length(r)?);
                let sense = take_sense(r)?;
                Torus::new(frame, major, minor, sense)
                    .map(Self::Torus)
                    .map_err(surface_at(at))
            }
            SurfaceKind::Nurbs => {
                r.line();
                Ok(Self::Nurbs(Arc::new(NurbsSurface::take(r)?)))
            }
        }
    }
}
