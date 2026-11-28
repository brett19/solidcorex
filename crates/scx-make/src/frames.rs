// Written for solidcorex. Apache-2.0.
//! `frames` (research 75 MK7a): where along a path, and turned how, a section is carried, in
//! closed form on lines, circular arcs and helices, with the least rotation at corners.
//!
//! **A station** is a point of the path, its unit tangent and a [`Rotation`]: the rotation
//! that takes any frame at the path's start to the path's rotation-minimising frame (parallel
//! transport) at the station. It never turns about the tangent, and it does not depend on the
//! frame the start was given, so a section drawn at the start is placed at the station by
//! `at + R (q - start)`.
//!
//! The rotation on each piece:
//! - a **line**: the identity;
//! - a **circular arc** about the unit axis `n`: the rotation about `n` by the angle run;
//! - a **helix** of radius `a` and rise `b` per radian about `n`: the screw's rotation about
//!   `n` by the angle run `phi`, then about the new tangent by `psi = -b phi / sqrt(a^2 +
//!   b^2)` (the rotation-minimising normal `U = cos psi N + sin psi B` keeps `U'` along the
//!   tangent where `psi' = -tau`; Wang, Juettler, Zheng and Liu, ACM TOG 27, 2008; Guggenheimer,
//!   CAGD 6, 1989);
//! - a **corner**, tangents `t0` to `t1`: the least rotation taking one to the other, about
//!   `t0 x t1` by the angle between them. Two tangents within [`G1_TANGENTS`] are one, with
//!   no corner. A station asked as [`Stop::Corner`] is the corner's mitre plane: the rotation
//!   by half the angle, the tangent the bisector.
//!
//! Every rotation is Rodrigues' formula on [`scx_num::fmath`] (deterministic across
//! platforms), so a station's columns are orthonormal to `O(k eps)` for `k` composed turns.
//!
//! **What is not here.** Twist, end scale, orientation laws and rails are the sweep's
//! (`MK7b`); the mitre's stretch `1 / cos(angle / 2)` is [`Corner::stretch`], applied by
//! whoever mitres. A piece that is not a line, arc or helix is not a [`Piece`]: a spline path
//! needs a certified enclosure of its frame (step 5).
//!
//! Errors are a [`FrameError`], typed, apart from [`crate::MakeError`] (the sweep wraps it).

use core::fmt;

use scx_brep::Document;
use scx_num::fmath;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};

/// Two unit tangents closer than this are one: the procedural sweep's G1 rule.
pub const G1_TANGENTS: f64 = 1e-8;
/// Most pieces of a [`Path`].
pub const MAX_PIECES: usize = 1000;
/// Most turns of one arc or helix piece.
pub const MAX_TURNS: f64 = 1000.0;

/// A rotation, by its columns (the images of the axes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rotation(pub [Vec3; 3]);

impl Rotation {
    /// The identity.
    pub const IDENTITY: Self = Self([Vec3::X, Vec3::Y, Vec3::Z]);

    /// The rotation about the unit `axis` by `angle` radians, right-handed.
    #[must_use]
    pub fn about(axis: UnitVec3, angle: f64) -> Self {
        let (s, c) = fmath::sin_cos(angle);
        let n = axis.as_vec();
        let turn = |v: Vec3| v * c + n.cross(v) * s + n * (n.dot(v) * (1.0 - c));
        Self([turn(Vec3::X), turn(Vec3::Y), turn(Vec3::Z)])
    }

    /// `R v`.
    #[must_use]
    pub fn apply(&self, v: Vec3) -> Vec3 {
        let [a, b, c] = self.0;
        a * v.x + b * v.y + c * v.z
    }

    /// This rotation after `first`.
    #[must_use]
    pub fn after(&self, first: &Self) -> Self {
        Self(first.0.map(|col| self.apply(col)))
    }

    /// The inverse: the transpose.
    #[must_use]
    pub fn inverse(&self) -> Self {
        let [a, b, c] = self.0;
        Self([
            Vec3::new(a.x, b.x, c.x),
            Vec3::new(a.y, b.y, c.y),
            Vec3::new(a.z, b.z, c.z),
        ])
    }

    /// The largest of `| |c_i|^2 - 1 |` and `|c_i . c_j|` over the columns: how far from
    /// orthonormal.
    #[must_use]
    pub fn defect(&self) -> f64 {
        let [a, b, c] = self.0;
        [
            (a.dot(a) - 1.0).abs(),
            (b.dot(b) - 1.0).abs(),
            (c.dot(c) - 1.0).abs(),
            a.dot(b).abs(),
            b.dot(c).abs(),
            a.dot(c).abs(),
        ]
        .into_iter()
        .fold(0.0, f64::max)
    }
}

/// One piece of a path.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Piece {
    /// From `from` to `to`.
    Line {
        /// Its start.
        from: Point3,
        /// Its end.
        to: Point3,
    },
    /// From `start`, `angle` radians (positive) right-handed about `axis` through `centre`.
    Arc {
        /// A point of the axis.
        centre: Point3,
        /// The axis.
        axis: UnitVec3,
        /// Its start, off the axis.
        start: Point3,
        /// The angle run (radians, positive).
        angle: f64,
    },
    /// As an arc, rising `rise` along the axis per radian (negative: falling).
    Helix {
        /// A point of the axis.
        centre: Point3,
        /// The axis.
        axis: UnitVec3,
        /// Its start, off the axis.
        start: Point3,
        /// The angle run (radians, positive).
        angle: f64,
        /// The rise per radian.
        rise: f64,
    },
}

/// The screw a curved piece is: foot of the start on the axis, axis, radius vector at the
/// start, angle and rise.
struct Screw {
    foot: Point3,
    axis: UnitVec3,
    radial: Vec3,
    angle: f64,
    rise: f64,
}

impl Piece {
    fn screw(&self) -> Option<Screw> {
        let (centre, axis, start, angle, rise) = match *self {
            Self::Line { .. } => return None,
            Self::Arc {
                centre,
                axis,
                start,
                angle,
            } => (centre, axis, start, angle, 0.0),
            Self::Helix {
                centre,
                axis,
                start,
                angle,
                rise,
            } => (centre, axis, start, angle, rise),
        };
        let d = start - centre;
        let n = axis.as_vec();
        let radial = d - n * n.dot(d);
        Some(Screw {
            foot: start - radial,
            axis,
            radial,
            angle,
            rise,
        })
    }

    fn check(&self, tiny: f64) -> Result<(), FrameError> {
        match *self {
            Self::Line { from, to } => {
                if !(from.is_finite() && to.is_finite() && from.distance(to) > tiny) {
                    return Err(FrameError::Piece(PieceFault::Line));
                }
            }
            Self::Arc { angle, .. } | Self::Helix { angle, .. } => {
                if !(angle > 0.0 && angle / core::f64::consts::TAU <= MAX_TURNS) {
                    return Err(FrameError::Piece(PieceFault::Angle));
                }
                let finite = match *self {
                    Self::Arc { centre, start, .. } => centre.is_finite() && start.is_finite(),
                    Self::Helix {
                        centre,
                        start,
                        rise,
                        ..
                    } => centre.is_finite() && start.is_finite() && rise.is_finite(),
                    Self::Line { .. } => true,
                };
                let size = self.screw().map_or(0.0, |s| s.radial.length());
                if !(finite && size > tiny && size.is_finite()) {
                    return Err(FrameError::Piece(PieceFault::Radius));
                }
            }
        }
        Ok(())
    }

    fn length(&self) -> f64 {
        match self.screw() {
            None => match *self {
                Self::Line { from, to } => from.distance(to),
                _ => 0.0,
            },
            Some(s) => s.angle * fmath::hypot(s.radial.length(), s.rise),
        }
    }

    /// Point, unit tangent and the transport from the piece's start at the fraction `f` of
    /// its length (of its angle, for a curved piece: the same).
    fn at(&self, f: f64) -> Option<(Point3, UnitVec3, Rotation)> {
        let Some(s) = self.screw() else {
            let Self::Line { from, to } = *self else {
                return None;
            };
            return Some((
                from + (to - from) * f,
                (to - from).normalized()?,
                Rotation::IDENTITY,
            ));
        };
        let phi = s.angle * f;
        let screw = Rotation::about(s.axis, phi);
        let r = screw.apply(s.radial);
        let p = s.foot + r + s.axis.as_vec() * (s.rise * phi);
        let t = (s.axis.as_vec().cross(r) + s.axis.as_vec() * s.rise).normalized()?;
        let a = s.radial.length();
        let m = if s.rise == 0.0 {
            screw
        } else {
            Rotation::about(t, -s.rise * phi / fmath::hypot(a, s.rise)).after(&screw)
        };
        Some((p, t, m))
    }

    fn ends(&self) -> Option<(Point3, UnitVec3, Point3, UnitVec3)> {
        let (p0, t0, _) = self.at(0.0)?;
        let (p1, t1, _) = self.at(1.0)?;
        Some((p0, t0, p1, t1))
    }
}

/// A corner between two pieces: where the tangent jumps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corner {
    /// The joint.
    pub at: Point3,
    /// The arriving unit tangent.
    pub from: UnitVec3,
    /// The leaving unit tangent.
    pub to: UnitVec3,
    /// The angle between them, in `(0, pi)`.
    pub angle: f64,
    /// The mitre's stretch `1 / cos(angle / 2)` along `to - from`.
    pub stretch: f64,
    /// The fraction of the path's length at which the corner is.
    pub fraction: f64,
}

/// A path measured: each piece with its start, length and the corner before it.
#[derive(Debug, Clone, PartialEq)]
struct Measured {
    piece: Piece,
    start: f64,
    length: f64,
    corner: Option<Corner>,
    /// The rotation across the corner before the piece (identity without one).
    turn: Rotation,
    half: Rotation,
}

/// A path: validated pieces end to end, each starting where the last ended.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pieces: Vec<Measured>,
    total: f64,
}

impl Path {
    /// The path of `pieces`, each starting where the last ends (within the document's
    /// resolution).
    ///
    /// # Errors
    ///
    /// [`FrameError::Pieces`] for none or more than [`MAX_PIECES`]; [`FrameError::Piece`] for
    /// a piece that is not finite or has no length, radius or positive angle;
    /// [`FrameError::Gap`] for pieces that do not meet; [`FrameError::DoublesBack`] for a
    /// corner that turns the tangent a half turn.
    pub fn new(pieces: &[Piece], doc: &Document) -> Result<Self, FrameError> {
        if pieces.is_empty() || pieces.len() > MAX_PIECES {
            return Err(FrameError::Pieces(pieces.len()));
        }
        let zone = doc.resolution().zone().mm();
        let mut out: Vec<Measured> = Vec::with_capacity(pieces.len());
        let mut run = 0.0;
        let mut last: Option<(Point3, UnitVec3)> = None;
        for (index, piece) in pieces.iter().enumerate() {
            piece.check(zone)?;
            let ends = piece.ends().ok_or(FrameError::Piece(PieceFault::Radius))?;
            let (p0, t0, p1, t1) = ends;
            let (corner, turn, half) = match last {
                None => (None, Rotation::IDENTITY, Rotation::IDENTITY),
                Some((end, t_end)) => {
                    let gap = end.distance(p0);
                    if gap > zone {
                        return Err(FrameError::Gap { index, gap });
                    }
                    meet(t_end, t0, p0, index)?
                }
            };
            let length = piece.length();
            out.push(Measured {
                piece: *piece,
                start: run,
                length,
                corner,
                turn,
                half,
            });
            run += length;
            last = Some((p1, t1));
        }
        if !(run > 0.0 && run.is_finite()) {
            return Err(FrameError::Piece(PieceFault::Line));
        }
        for m in &mut out {
            if let Some(c) = &mut m.corner {
                c.fraction = m.start / run;
            }
        }
        Ok(Self {
            pieces: out,
            total: run,
        })
    }

    /// The path's arc length (mm).
    #[must_use]
    pub const fn length(&self) -> f64 {
        self.total
    }

    /// The corners, in order: corner `j` is [`Stop::Corner`]`(j)`.
    pub fn corners(&self) -> impl Iterator<Item = &Corner> {
        self.pieces.iter().filter_map(|m| m.corner.as_ref())
    }

    /// Whether the path ends where it starts (within the document's resolution).
    #[must_use]
    pub fn is_closed(&self, doc: &Document) -> bool {
        let ends = self
            .pieces
            .first()
            .zip(self.pieces.last())
            .and_then(|(a, b)| Some((a.piece.at(0.0)?.0, b.piece.at(1.0)?.0)));
        ends.is_some_and(|(a, b)| a.distance(b) <= doc.resolution().zone().mm())
    }

    /// The fractions at which the pieces after the first start (the sweep's skin takes a
    /// station at each).
    pub(crate) fn joints(&self) -> impl Iterator<Item = f64> + '_ {
        (1..self.pieces.len()).filter_map(|i| self.fraction_of_start(i))
    }

    /// The angle the unit tangent turns through along the pieces, corners not included
    /// (radians): an arc's angle, a helix's `angle a / sqrt(a^2 + b^2)`, a line's 0.
    pub(crate) fn tangent_turn(&self) -> f64 {
        self.pieces
            .iter()
            .filter_map(|m| m.piece.screw())
            .map(|s| {
                let a = s.radial.length();
                s.angle * a / fmath::hypot(a, s.rise)
            })
            .sum()
    }

    fn fraction_of_start(&self, i: usize) -> Option<f64> {
        self.pieces.get(i).map(|m| m.start / self.total)
    }
}

/// The corner (if any) where a piece leaving along `t1` follows one arriving along `t0`.
fn meet(
    t0: UnitVec3,
    t1: UnitVec3,
    at: Point3,
    index: usize,
) -> Result<(Option<Corner>, Rotation, Rotation), FrameError> {
    if (t1.as_vec() - t0.as_vec()).length() <= G1_TANGENTS {
        return Ok((None, Rotation::IDENTITY, Rotation::IDENTITY));
    }
    let cross = t0.cross(*t1);
    // past G1 the axis t0 x t1 vanishes only where the path doubles back
    let axis = cross
        .normalized()
        .ok_or(FrameError::DoublesBack { index })?;
    let angle = fmath::atan2(cross.length(), t0.dot(*t1));
    let half = 0.5 * angle;
    let corner = Corner {
        at,
        from: t0,
        to: t1,
        angle,
        stretch: 1.0 / fmath::cos(half),
        fraction: 0.0,
    };
    Ok((
        Some(corner),
        Rotation::about(axis, angle),
        Rotation::about(axis, half),
    ))
}

/// Where a station is asked.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Stop {
    /// At this fraction of the path's arc length, in `[0, 1]`. A corner's own fraction is
    /// the leaving piece's start, never its mitre.
    At(f64),
    /// At the mitre of the path's corner `j`.
    Corner(usize),
}

/// A station: a point of the path, its tangent and the rotation of the frame from the
/// path's start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Station {
    /// The point.
    pub at: Point3,
    /// The unit tangent (at a [`Stop::Corner`], the bisector of the two).
    pub tangent: UnitVec3,
    /// The rotation-minimising frame's rotation from the path's start.
    pub turn: Rotation,
}

/// The stations of `path` at `stops`, in order.
///
/// # Errors
///
/// [`FrameError::Fraction`] for a fraction outside `[0, 1]` or not a number;
/// [`FrameError::NoCorner`] for a corner the path does not have.
pub fn frames(path: &Path, stops: &[Stop]) -> Result<Vec<Station>, FrameError> {
    stops.iter().map(|&s| locate(path, s)).collect()
}

fn locate(path: &Path, stop: Stop) -> Result<Station, FrameError> {
    let (u, corner) = match stop {
        Stop::At(u) if (0.0..=1.0).contains(&u) => (u, None),
        Stop::At(u) => return Err(FrameError::Fraction(u)),
        Stop::Corner(j) => {
            let i = path
                .pieces
                .iter()
                .enumerate()
                .filter(|(_, m)| m.corner.is_some())
                .nth(j)
                .map(|(i, _)| i)
                .ok_or(FrameError::NoCorner(j))?;
            // piece `i` is where the walk stops: it starts at or before this fraction
            (path.fraction_of_start(i).unwrap_or(0.0), Some(i))
        }
    };
    let mut transport = Rotation::IDENTITY;
    let count = path.pieces.len();
    for (i, m) in path.pieces.iter().enumerate() {
        let before = m.turn.after(&transport);
        if corner == Some(i) {
            let (p, t1, _) = m.piece.at(0.0).ok_or(FrameError::NoCorner(i))?;
            let t0 = m.corner.map_or(t1, |c| c.from);
            let bisector = (t0.as_vec() + t1.as_vec())
                .normalized()
                .ok_or(FrameError::NoCorner(i))?;
            return Ok(Station {
                at: p,
                tangent: bisector,
                turn: m.half.after(&transport),
            });
        }
        let on = i + 1 == count || path.fraction_of_start(i + 1).is_some_and(|next| u < next);
        if on {
            let f = if m.length > 0.0 {
                ((u * path.total - m.start) / m.length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (at, tangent, local) = m.piece.at(f).ok_or(FrameError::NoCorner(i))?;
            return Ok(Station {
                at,
                tangent,
                turn: local.after(&before),
            });
        }
        let (_, _, whole) = m.piece.at(1.0).ok_or(FrameError::NoCorner(i))?;
        transport = whole.after(&before);
    }
    Err(FrameError::Pieces(0))
}

/// What is wrong with one piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PieceFault {
    /// A line not finite, or not longer than the resolution.
    Line,
    /// An angle not positive, or past [`MAX_TURNS`] turns.
    Angle,
    /// A curved piece not finite, or starting on its axis (within the resolution).
    Radius,
}

/// Why no frames were made.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum FrameError {
    /// The path has this many pieces: none, or more than [`MAX_PIECES`].
    Pieces(usize),
    /// A piece is not valid.
    Piece(PieceFault),
    /// Piece `index` starts `gap` mm from where the last ended.
    Gap {
        /// The piece.
        index: usize,
        /// The distance (mm).
        gap: f64,
    },
    /// The path turns back on itself at the start of piece `index`.
    DoublesBack {
        /// The piece after the corner.
        index: usize,
    },
    /// A fraction outside `[0, 1]` or not a number.
    Fraction(f64),
    /// A corner the path does not have.
    NoCorner(usize),
}

impl FrameError {
    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Pieces(_) => "pieces",
            Self::Piece(_) => "piece",
            Self::Gap { .. } => "gap",
            Self::DoublesBack { .. } => "doubles_back",
            Self::Fraction(_) => "fraction",
            Self::NoCorner(_) => "no_corner",
        }
    }
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pieces(n) => write!(f, "a path of {n} pieces (1 to {MAX_PIECES} are taken)"),
            Self::Piece(p) => write!(f, "a piece that is not valid: {p:?}"),
            Self::Gap { index, gap } => {
                write!(f, "piece {index} starts {gap} mm from the end of the last")
            }
            Self::DoublesBack { index } => {
                write!(f, "the path doubles back at the start of piece {index}")
            }
            Self::Fraction(u) => write!(f, "the fraction {u} is outside [0, 1]"),
            Self::NoCorner(j) => write!(f, "the path has no corner {j}"),
        }
    }
}

impl core::error::Error for FrameError {}
