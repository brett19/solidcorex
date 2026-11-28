// Written for solidcorex. Apache-2.0.
//! The section of a surface of revolution by a plane through its axis, in closed form
//! (research 74 §30, SU8; the pin's `geom/axial_section`, #17496 to #17500).
//!
//! A plane through the axis of a cylinder, cone, sphere or torus meets it in its meridians,
//! the profile turned into the plane: two lines for a cylinder, two generators from the apex
//! for a cone, one circle for a sphere and two circles for a torus. A face of the surface
//! keeps the pieces of them between the points where its boundary crosses the plane that lie
//! on the face ([`axial_section`]); [`holds_axis`] says whether the plane holds the axis
//! where the face is.
//!
//! The cutting plane is a [`Plane`] (its normal is a unit by construction), the face's box an
//! [`Extent3d`] (finite by construction), the tolerance a [`Resolution`] (above zero and
//! finite by construction), so none of the pin's "false or `None` for a number that is not
//! finite, a normal of no length or a tolerance that is not positive" survives.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `Revolved` (cylinder, cone, sphere, torus as bare numbers) | a [`Surface`] of those kinds; a plane, a NURBS surface or a procedural surface is [`AxialError::NotRevolved`] (a procedural revolution's meridian sections are not built yet) |
//! | `holds_axis -> bool` (false for a number not finite, a normal of no length, a missing section) | [`holds_axis`]: `Result<`[`Holds`]`, `[`AxialError`]`>`, [`Holds::Off`] carrying how far off the axis stands |
//! | `axial_section -> Result<Option<Vec<_>>, E>`: `None` when the plane does not hold the axis or a number is unusable | `Result<Vec<`[`AxialPiece`]`>, `[`AxialError`]`<E>>`: [`AxialError::AxisOff`], [`AxialError::AxisAcross`], [`AxialError::NotRevolved`]; `Ok(vec![])` still means the plane misses the face |
//! | a cut is put on its nearest meridian, whatever its distance | a cut farther than the section tolerance from its meridian is [`AxialError::CutOffSection`], the distance certified by [`vertex_curve_gap`] (74 §17); never assigned blindly |
//! | `tol` doubles as the plane test, the cut merge and nothing else | `tol` is the plane test and the cut merge; the cut's own allowance is `tol.xsect()` |
//! | a sphere's meridian circle starts on an arbitrary orthogonal vector | it starts on the plane's own x axis |
//!
//! An arc runs counter-clockwise about the plane's normal. A cone's meridians are its one
//! nappe's generators from the apex.

use core::convert::Infallible;
use core::fmt;

use scx_curve::{Circle, Curve, Line};
use scx_num::error::ErrorKind;
use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, UnitVec3, Vec3};

use crate::common::Axes;
use crate::gap::{Bracket, vertex_curve_gap};
use crate::{Plane, Surface};

const TURN: f64 = core::f64::consts::TAU;

/// A piece of an axial section, in the plane.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum AxialPiece {
    /// A segment of a line of a cylinder or a cone.
    Line {
        /// Where it starts.
        start: Point3,
        /// Where it ends.
        end: Point3,
    },
    /// An arc of a circle of a sphere or a torus, counter-clockwise about the plane's normal
    /// from `start` to `end`.
    Arc {
        /// The circle's centre.
        centre: Point3,
        /// Where it starts.
        start: Point3,
        /// Where it ends.
        end: Point3,
    },
    /// A whole circle of a sphere or a torus.
    Circle {
        /// The circle's centre.
        centre: Point3,
        /// Its radius.
        radius: f64,
    },
}

/// Whether a plane holds a surface's axis where a face is.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Holds {
    /// The plane holds the axis within the tolerance wherever it passes the face's box (a
    /// sphere: its centre), and is not square to it: [`axial_section`] sections the face.
    Yes,
    /// The axis stands off the plane, by this much at the worse end of the box's reach.
    Off {
        /// The larger distance of the two ends from the plane.
        by: Length,
    },
    /// The axis is square to the plane, which then cuts circles, not meridians.
    Across,
}

/// Why an axial section cannot be made.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum AxialError<E = Infallible> {
    /// The surface is a plane or a NURBS surface, which have no axis, or a procedural
    /// surface, whose sections are not built yet.
    NotRevolved,
    /// The plane does not hold the axis ([`Holds::Off`]).
    AxisOff {
        /// How far the axis stands off the plane.
        by: Length,
    },
    /// The axis is square to the plane ([`Holds::Across`]).
    AxisAcross,
    /// A cut's coordinate is not finite.
    CutNotFinite {
        /// Its place in the list.
        index: usize,
    },
    /// A cut lies farther from the nearest meridian than the section tolerance, or the
    /// certified gap does not settle it.
    CutOffSection {
        /// Its place in the list.
        index: usize,
        /// The certified gap from the nearest meridian, which is not below the allowance.
        gap: Bracket,
    },
    /// A meridian could not be built as a curve (a radius too small to draw).
    Defect {
        /// What failed.
        reason: &'static str,
    },
    /// `on_face` failed.
    OnFace(E),
}

impl<E> AxialError<E> {
    /// The cause's kind (research 62 §10). `on_face`'s own error is the caller's: it is
    /// `Refused` here only because this crate cannot see inside it.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::NotRevolved
            | Self::AxisOff { .. }
            | Self::AxisAcross
            | Self::CutNotFinite { .. }
            | Self::CutOffSection { .. }
            | Self::OnFace(_) => ErrorKind::Refused,
            Self::Defect { .. } => ErrorKind::Defect,
        }
    }

    /// A stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NotRevolved => "axial_not_revolved",
            Self::AxisOff { .. } => "axial_axis_off_plane",
            Self::AxisAcross => "axial_axis_across_plane",
            Self::CutNotFinite { .. } => "axial_cut_not_finite",
            Self::CutOffSection { .. } => "axial_cut_off_section",
            Self::Defect { .. } => "axial_defect",
            Self::OnFace(_) => "axial_on_face",
        }
    }

    fn lift<F>(self) -> AxialError<F>
    where
        E: Into<Infallible>,
    {
        match self {
            Self::NotRevolved => AxialError::NotRevolved,
            Self::AxisOff { by } => AxialError::AxisOff { by },
            Self::AxisAcross => AxialError::AxisAcross,
            Self::CutNotFinite { index } => AxialError::CutNotFinite { index },
            Self::CutOffSection { index, gap } => AxialError::CutOffSection { index, gap },
            Self::Defect { reason } => AxialError::Defect { reason },
            Self::OnFace(e) => match e.into() {},
        }
    }
}

impl<E: fmt::Display> fmt::Display for AxialError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRevolved => f.write_str("a plane or NURBS surface has no axis to section by"),
            Self::AxisOff { by } => {
                write!(f, "the axis stands {} mm off the plane", by.mm())
            }
            Self::AxisAcross => f.write_str("the axis is square to the plane"),
            Self::CutNotFinite { index } => write!(f, "cut {index} is not finite"),
            Self::CutOffSection { index, gap } => write!(
                f,
                "cut {index} is between {} and {} mm from the section",
                gap.lower(),
                gap.upper()
            ),
            Self::Defect { reason } => write!(f, "the section failed inside: {reason}"),
            Self::OnFace(e) => write!(f, "the face test failed: {e}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for AxialError<E> {}

/// A meridian of the surface in the plane: a line `base + t dir` (from `t = 0` only for a
/// cone's, which starts at the apex), or the circle `centre + radius (cos t e1 + sin t e2)`.
#[derive(Clone, Copy, Debug)]
enum Meridian {
    Line {
        base: Point3,
        dir: Vec3,
        from_base: bool,
    },
    Circle {
        centre: Point3,
        radius: f64,
        e1: Vec3,
        e2: Vec3,
    },
}

impl Meridian {
    /// The parameter of the nearest point to `p` on the meridian (a line's clamped at its
    /// start if it has one).
    fn foot(&self, p: Point3) -> (f64, f64) {
        match *self {
            Self::Line {
                base,
                dir,
                from_base,
            } => {
                let t = (p - base).dot(dir);
                let t = if from_base { t.max(0.0) } else { t };
                ((base + dir * t).distance(p), t)
            }
            Self::Circle {
                centre,
                radius,
                e1,
                e2,
            } => {
                let d = p - centre;
                let (x, y) = (d.dot(e1), d.dot(e2));
                let theta = fmath::atan2(y, x).rem_euclid(TURN);
                let out_of_plane = d.dot(e1.cross(e2));
                (
                    fmath::hypot((x * x + y * y).sqrt() - radius, out_of_plane),
                    theta,
                )
            }
        }
    }

    fn at(&self, t: f64) -> Point3 {
        match *self {
            Self::Line { base, dir, .. } => base + dir * t,
            Self::Circle {
                centre,
                radius,
                e1,
                e2,
            } => centre + (e1 * fmath::cos(t) + e2 * fmath::sin(t)) * radius,
        }
    }

    /// The meridian's length from parameter `a` to `b`.
    fn length(&self, a: f64, b: f64) -> f64 {
        match *self {
            Self::Line { .. } => (b - a).abs(),
            Self::Circle { radius, .. } => radius * (b - a).abs(),
        }
    }

    /// The meridian as a curve with the same parameter, for the certified gap.
    fn curve(&self, n: UnitVec3) -> Result<Curve, &'static str> {
        match *self {
            Self::Line { base, dir, .. } => Line::new(base, dir)
                .map(Curve::Line)
                .map_err(|_| "a meridian line has no direction"),
            Self::Circle {
                centre, radius, e1, ..
            } => {
                let frame = Frame::from_axes(centre, e1, *n)
                    .map_err(|_| "a meridian circle has no frame")?;
                Circle::new(frame, Length::from_mm(radius), Sense::Forward)
                    .map(Curve::Circle)
                    .map_err(|_| "a meridian circle has no radius")
            }
        }
    }
}

/// A surface of revolution's axis and sizes, read off the kind.
enum Revolved {
    Cylinder {
        o: Point3,
        axis: UnitVec3,
        radius: f64,
    },
    Cone {
        apex: Point3,
        into: UnitVec3,
        half_angle: f64,
    },
    Sphere {
        centre: Point3,
        radius: f64,
    },
    Torus {
        o: Point3,
        axis: UnitVec3,
        major: f64,
        minor: f64,
    },
}

fn revolved(surface: &Surface) -> Result<Revolved, AxialError> {
    let unit = |a: &Axes| {
        a.z.normalized().ok_or(AxialError::Defect {
            reason: "an axis has no direction",
        })
    };
    Ok(match surface {
        Surface::Cylinder(k) => {
            let a = k.axes();
            Revolved::Cylinder {
                o: a.o,
                axis: unit(&a)?,
                radius: k.radius().mm(),
            }
        }
        Surface::Cone(k) => {
            let a = k.axes();
            // The nappe runs from the apex along the axis for a positive slope, against it
            // for a negative one (`Cone::domain`).
            let z = if k.slope() > 0.0 { a.z } else { -a.z };
            Revolved::Cone {
                apex: k.pole().point,
                into: z.normalized().ok_or(AxialError::Defect {
                    reason: "an axis has no direction",
                })?,
                half_angle: k.half_angle().abs(),
            }
        }
        Surface::Sphere(k) => Revolved::Sphere {
            centre: k.frame().origin(),
            radius: k.radius().mm(),
        },
        Surface::Torus(k) => {
            let a = k.axes();
            Revolved::Torus {
                o: a.o,
                axis: unit(&a)?,
                major: k.major().mm(),
                minor: k.minor().mm(),
            }
        }
        Surface::Plane(_) | Surface::Nurbs(_) | Surface::Procedural(_) => {
            return Err(AxialError::NotRevolved);
        }
    })
}

/// How far `p` is above the plane.
fn height(plane: &Plane, p: Point3) -> f64 {
    (p - plane.frame().origin()).dot(*plane.frame().z())
}

/// The axis through `o` along `a`, in the plane: the ends of its reach over the box, each
/// compared with the plane. Its own distance from the plane is greatest at one of them,
/// since the height is linear along the axis.
fn axis_off(plane: &Plane, over: &Extent3d, o: Point3, a: UnitVec3) -> f64 {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..8_usize {
        let pick = |bit: usize, l: f64, h: f64| if i & bit == 0 { l } else { h };
        let corner = Point3::new(
            pick(1, over.x_min(), over.x_max()),
            pick(2, over.y_min(), over.y_max()),
            pick(4, over.z_min(), over.z_max()),
        );
        let along = (corner - o).dot(*a);
        lo = lo.min(along);
        hi = hi.max(along);
    }
    height(plane, o + *a * lo)
        .abs()
        .max(height(plane, o + *a * hi).abs())
}

/// The meridians, or why the plane has none.
fn meridians(
    surface: &Surface,
    plane: &Plane,
    over: &Extent3d,
    tol: Resolution,
) -> Result<Vec<Meridian>, AxialError> {
    let f = plane.frame();
    let n = f.z();
    let into_plane = |p: Point3| p - *n * height(plane, p);
    let tol = tol.zone().mm();
    // The axis through `o` along `a`, in the plane: its point, its unit direction there, and
    // the unit direction square to it in the plane (counter-clockwise from it about `n`).
    let axial = |o: Point3, a: UnitVec3| -> Result<(Point3, Vec3, Vec3), AxialError> {
        let by = axis_off(plane, over, o, a);
        if by > tol {
            return Err(AxialError::AxisOff {
                by: Length::from_mm(by),
            });
        }
        let e1 = (*a - *n * a.dot(*n))
            .normalized()
            .ok_or(AxialError::AxisAcross)?;
        Ok((into_plane(o), *e1, n.cross(*e1)))
    };
    let pair = |make: &dyn Fn(f64) -> Meridian| vec![make(1.0), make(-1.0)];
    Ok(match revolved(surface)? {
        Revolved::Cylinder { o, axis, radius } => {
            let (o, e1, e2) = axial(o, axis)?;
            pair(&|s| Meridian::Line {
                base: o + e2 * (s * radius),
                dir: e1,
                from_base: false,
            })
        }
        Revolved::Cone {
            apex,
            into,
            half_angle,
        } => {
            let (o, e1, e2) = axial(apex, into)?;
            let (sin, cos) = fmath::sin_cos(half_angle);
            pair(&|s| Meridian::Line {
                base: o,
                dir: e1 * cos + e2 * (s * sin),
                from_base: true,
            })
        }
        Revolved::Sphere { centre, radius } => {
            let by = height(plane, centre).abs();
            if by > tol {
                return Err(AxialError::AxisOff {
                    by: Length::from_mm(by),
                });
            }
            vec![Meridian::Circle {
                centre: into_plane(centre),
                radius,
                e1: *f.x(),
                e2: *f.y(),
            }]
        }
        Revolved::Torus {
            o,
            axis,
            major,
            minor,
        } => {
            let (o, e1, e2) = axial(o, axis)?;
            pair(&|s| Meridian::Circle {
                centre: o + e2 * (s * major),
                radius: minor,
                e1,
                e2,
            })
        }
    })
}

/// Whether the plane holds the axis of `surface` within `tol` wherever the axis passes the
/// box `over` (the face's), or for a sphere its centre: whether [`axial_section`] sections a
/// face there.
///
/// # Errors
///
/// [`AxialError::NotRevolved`] for a plane or a NURBS surface.
pub fn holds_axis(
    surface: &Surface,
    plane: &Plane,
    over: &Extent3d,
    tol: Resolution,
) -> Result<Holds, AxialError> {
    match meridians(surface, plane, over, tol) {
        Ok(_) => Ok(Holds::Yes),
        Err(AxialError::AxisOff { by }) => Ok(Holds::Off { by }),
        Err(AxialError::AxisAcross) => Ok(Holds::Across),
        Err(e) => Err(e),
    }
}

/// The pieces of one meridian between `cuts` (parameters, sorted), each with whether it lies
/// on the face (`on_face` at its middle), joined where neighbours both do.
fn trimmed<E>(
    m: &Meridian,
    cuts: &[f64],
    on_face: &mut impl FnMut(Point3) -> Result<bool, E>,
) -> Result<Vec<AxialPiece>, E> {
    // The spans between the cuts: on a line, between neighbours (a face is bounded, so it ends
    // at a cut each way); on a circle, round it from each to the next.
    let mut spans: Vec<(f64, f64)> = cuts
        .iter()
        .zip(cuts.iter().skip(1))
        .map(|(a, b)| (*a, *b))
        .collect();
    let circle = matches!(m, Meridian::Circle { .. });
    if circle {
        match (cuts.first(), cuts.last()) {
            (Some(first), Some(last)) => spans.push((*last, *first + TURN)),
            _ => spans.push((0.0, TURN)),
        }
    }
    let mut kept = Vec::with_capacity(spans.len());
    for &(a, b) in &spans {
        kept.push(on_face(m.at(0.5 * (a + b)))?);
    }
    // Joined runs of kept spans; on a circle a run may wrap past the last cut to the first.
    let mut runs: Vec<(f64, f64)> = Vec::new();
    let mut open: Option<(f64, f64)> = None;
    for (&(a, b), &keep) in spans.iter().zip(&kept) {
        match (keep, open.as_mut()) {
            (true, Some(run)) => run.1 = b,
            (true, None) => open = Some((a, b)),
            (false, _) => runs.extend(open.take()),
        }
    }
    let whole = circle && !kept.is_empty() && kept.iter().all(|k| *k);
    if let Some(run) = open {
        match runs.first_mut() {
            Some(first) if circle && kept.first() == Some(&true) => first.0 = run.0 - TURN,
            _ => runs.push(run),
        }
    }
    Ok(match *m {
        Meridian::Circle { centre, radius, .. } if whole => {
            vec![AxialPiece::Circle { centre, radius }]
        }
        Meridian::Circle { centre, .. } => runs
            .into_iter()
            .map(|(a, b)| AxialPiece::Arc {
                centre,
                start: m.at(a),
                end: m.at(b),
            })
            .collect(),
        Meridian::Line { .. } => runs
            .into_iter()
            .map(|(a, b)| AxialPiece::Line {
                start: m.at(a),
                end: m.at(b),
            })
            .collect(),
    })
}

/// The section of a face on `surface` by `plane`, when the plane holds the surface's axis
/// ([`holds_axis`] over `over`, a box around the face). `cuts` are the points where the
/// face's boundary crosses the plane; each lies on a meridian, which it splits, and must lie
/// within `tol.xsect()` of it (certified, [`AxialError::CutOffSection`]). A piece between
/// cuts is the face's when `on_face` says its middle is. Lines start and end at the cuts'
/// feet on them, so on the face's edges; arcs run counter-clockwise about the plane's
/// normal; a circle the face keeps all of is whole. Cuts within `tol` of each other along a
/// meridian are one (an edge's end and its neighbour's start, a vertex). `Ok(vec![])` when
/// the plane misses the face.
///
/// The pieces come meridian by meridian in the order the kind has them (a cylinder, cone or
/// torus: the one on the plane's left of the axis first, `n x e1` with `e1` the axis's
/// direction in the plane), and along each in parameter order, so a run twice is the same to
/// the bit.
///
/// # Errors
///
/// [`AxialError::NotRevolved`], [`AxialError::AxisOff`] and [`AxialError::AxisAcross`] where
/// [`holds_axis`] is not [`Holds::Yes`]; [`AxialError::CutNotFinite`] and
/// [`AxialError::CutOffSection`] for a cut; [`AxialError::OnFace`] for `on_face`'s error.
pub fn axial_section<E>(
    surface: &Surface,
    plane: &Plane,
    over: &Extent3d,
    tol: Resolution,
    cuts: &[Point3],
    mut on_face: impl FnMut(Point3) -> Result<bool, E>,
) -> Result<Vec<AxialPiece>, AxialError<E>> {
    let lines = meridians(surface, plane, over, tol).map_err(AxialError::lift)?;
    let (tol_len, allowance) = (tol.zone().mm(), tol.xsect().mm());
    // Each cut on the meridian it lies on (the nearest: every point of the boundary is on
    // the surface, so where it crosses the plane is on a meridian), and certified to lie
    // there.
    let mut on: Vec<Vec<f64>> = vec![Vec::new(); lines.len()];
    for (index, p) in cuts.iter().enumerate() {
        if !p.is_finite() {
            return Err(AxialError::CutNotFinite { index });
        }
        let nearest = lines
            .iter()
            .map(|m| m.foot(*p))
            .enumerate()
            .min_by(|(_, a), (_, b)| a.0.total_cmp(&b.0));
        let Some((i, (_, t))) = nearest else {
            continue;
        };
        let m = lines.get(i).ok_or(AxialError::Defect {
            reason: "a nearest meridian is missing",
        })?;
        let curve = m
            .curve(plane.frame().z())
            .map_err(|reason| AxialError::Defect { reason })?;
        let gap =
            vertex_curve_gap(*p, &curve, t).map_err(|_| AxialError::CutNotFinite { index })?;
        if gap.exceeds(allowance) != Some(false) {
            return Err(AxialError::CutOffSection { index, gap });
        }
        if let Some(ts) = on.get_mut(i) {
            ts.push(t);
        }
    }
    let mut out = Vec::new();
    for (m, ts) in lines.iter().zip(&mut on) {
        if let Meridian::Line {
            from_base: true, ..
        } = m
        {
            ts.push(0.0);
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|b, a| m.length(*a, *b) <= tol_len);
        if let (Meridian::Circle { .. }, Some(first), Some(last)) = (m, ts.first(), ts.last())
            && ts.len() > 1
            && m.length(*last, *first + TURN) <= tol_len
        {
            ts.pop();
        }
        out.extend(trimmed(m, ts, &mut on_face).map_err(AxialError::OnFace)?);
    }
    Ok(out)
}
