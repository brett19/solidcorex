// Written for solidcorex. Apache-2.0.
//! `sweep` (research 75 MK7b): a profile carried along a [`Path`] by its laws, the twist,
//! end scale and orientation of a [`Shape`] or one or two guide [`Rails`], in closed form
//! (SolidCraft's `SweepControl` and `controlled_stations`, the pin's `sweep::stations`).
//!
//! **A placement** is the affine map that carries a point `q` of the profile, drawn at the
//! path's start, to `at + m (q - from)` at a station ([`Placement::place`]), `from` the
//! path's start and `m` a linear map by its columns. Without rails, `at` is the station's
//! point of the path; with rails, it is where the map takes the path's start.
//!
//! **The shape laws**, `u` the fraction of the path's arc length run and `n` the profile's
//! normal turned to point the way the path leaves it:
//! - `twist` turns the profile about `n` by `twist u`, right-handed;
//! - `end_scale` scales it about the path by `1 + (end_scale - 1) u`;
//! - then the orientation turns it: [`Orientation::Perpendicular`] by the path's
//!   rotation-minimising frame ([`frames`]); [`Orientation::Parallel`] not at all (the
//!   profile only moves), the path crossing its plane by more than the least angle asked;
//!   [`Orientation::Direction`] by the frame `[t, r, t x r]`, `r` the unit part of the
//!   direction square to the tangent `t`, carried from the start's;
//! - at a [`Stop::Corner`] the map is the mitre's: the frame's half turn, stretched by
//!   [`Corner::stretch`](crate::Corner::stretch) along `to - from` (not for a parallel
//!   profile, which is not turned).
//!
//! **Rails.** The station's plane, square to the path, meets each rail once
//! ([`crossing`](rail)). One rail: the profile turns about the path to follow the line from
//! the path to the rail, and is sized by that line's length against the start's. Two: it
//! moves with the first rail, turns with the line between them and is sized by their
//! distance. [`RailFit::Scale`] sizes it evenly, [`RailFit::Stretch`] only across. Rails set
//! the orientation, twist and scale themselves, so [`PathFrame`] holds a [`Shape`] or rails,
//! never both, and at most two rails by type.
//!
//! **The skin's stations.** [`fractions`] is SolidCraft's convention for the skin through
//! the stations, carried over and not derived: a station per degree of the tangent's turn
//! plus the twist (corners included), at least a rail's control points less one, between
//! [`MIN_SKIN_STATIONS`] and [`MAX_SKIN_STATIONS`], evenly in arc length, with every joint
//! of the path included exactly. Nothing bounds a skin's distance from the law surface
//! between them; the walls on step 5's procedural sweep kind replace the count with one the
//! kind's own bound sets.
//!
//! **The walls.** [`sweep`] builds the plain sweep ([`Shape::PLAIN`]) along lines and arcs
//! by kind, mitred at corners ([`walls`]). Any other frame it checks and places at every
//! station the skin would use, then answers [`Unsupported::NeedsKind`] with
//! [`Needed::ProceduralSweep`]: those walls are faces on step 5's procedural sweep surface,
//! which `Surface` does not have yet. That kind plugs in at the end of [`sweep`], taking the
//! profile's segments and the [`Placement`]s.

mod rail;
mod walls;

pub use rail::{Rail, RailFault};
pub use walls::SweepFaces;

use core::f64::consts::{FRAC_PI_2, PI, TAU};
use core::fmt;

use scx_brep::Document;
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_planar::Region;

use crate::Made;
use crate::error::{MakeError, Refusal, Unsupported};
use crate::frames::{MAX_TURNS, Path, Rotation, Stop, frames};

/// The fewest stations [`fractions`] gives (SolidCraft's).
pub const MIN_SKIN_STATIONS: usize = 16;
/// The most stations [`fractions`] gives (SolidCraft's).
pub const MAX_SKIN_STATIONS: usize = 2048;

/// How the profile turns as it follows the path.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Orientation {
    /// Square to the path, carried by its rotation-minimising frame.
    Perpendicular,
    /// As drawn: the profile only moves along the path.
    Parallel {
        /// The least angle (radians, in `[0, pi / 2)`) the path's tangent must make with the
        /// profile's plane at every station, the caller's limit (SolidCraft's is 5 degrees).
        /// At 0, only a path that does not cross the plane is refused.
        least_crossing: f64,
    },
    /// Square to the path, its turn about the path fixed by `along`'s part square to the
    /// tangent.
    Direction {
        /// The direction.
        along: UnitVec3,
        /// The least sine (in `[0, 1)`) of the angle between the tangent and `along` at
        /// every station, the caller's limit (SolidCraft's is `1e-3`). At 0, only a path
        /// running along the direction is refused.
        least_sine: f64,
    },
}

/// The shape laws of a sweep without rails.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    /// How the profile turns.
    pub orientation: Orientation,
    /// The twist over the path (radians, finite, at most [`MAX_TURNS`] turns either way).
    pub twist: f64,
    /// The profile's scale at the path's end (finite, positive).
    pub end_scale: f64,
}

impl Shape {
    /// The plain sweep: perpendicular, no twist, scale 1.
    pub const PLAIN: Self = Self {
        orientation: Orientation::Perpendicular,
        twist: 0.0,
        end_scale: 1.0,
    };
}

/// One or two guide rails.
#[derive(Debug, Clone, PartialEq)]
pub enum Rails {
    /// One rail: the profile follows the line from the path to it.
    One(Rail),
    /// Two rails: the profile spans them, moving with the first.
    Two(Rail, Rail),
}

impl Rails {
    fn iter(&self) -> impl Iterator<Item = &Rail> {
        let (a, b) = match self {
            Self::One(a) => (a, None),
            Self::Two(a, b) => (a, Some(b)),
        };
        core::iter::once(a).chain(b)
    }
}

/// How guide rails size the profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailFit {
    /// Evenly, by the distance across.
    Scale,
    /// Only across to the rail (between the rails).
    Stretch,
}

/// How a sweep places its profile along the path: by shape laws or by rails.
#[derive(Debug, Clone, PartialEq)]
pub enum PathFrame {
    /// Orientation, twist and end scale.
    Shaped(Shape),
    /// Guide rails, which set the orientation, twist and scale themselves.
    Railed {
        /// The rails.
        rails: Rails,
        /// How they size the profile.
        fit: RailFit,
    },
}

/// Where a station places the profile (module docs): `q` goes to `at + m (q - from)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// The path's start, where the profile is drawn.
    pub from: Point3,
    /// Where `from` goes.
    pub at: Point3,
    /// The linear map, by its columns (the images of the axes).
    pub m: [Vec3; 3],
}

impl Placement {
    /// `m v`.
    #[must_use]
    pub fn apply(&self, v: Vec3) -> Vec3 {
        apply(&self.m, v)
    }

    /// Where the profile's point `q` goes.
    #[must_use]
    pub fn place(&self, q: Point3) -> Point3 {
        self.at + self.apply(q - self.from)
    }
}

/// A surface kind a construction needs that this build does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Needed {
    /// Step 5's procedural sweep surface: a profile curve carried by placements.
    ProceduralSweep,
}

impl Needed {
    /// The stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ProceduralSweep => "procedural_sweep",
        }
    }
}

/// Why a sweep's laws do not place the profile.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum LawFault {
    /// A twist not finite or past [`MAX_TURNS`] turns (radians).
    Twist(f64),
    /// An end scale not finite and positive.
    EndScale(f64),
    /// A parallel profile's least crossing angle outside `[0, pi / 2)`.
    LeastCrossing(f64),
    /// A direction orientation's least sine outside `[0, 1)`.
    LeastSine(f64),
    /// The path crosses a parallel profile's plane at no more than the least angle asked.
    Crossing {
        /// Where.
        stop: Stop,
    },
    /// The path runs along the orientation's direction, within the least sine asked.
    AlongDirection {
        /// Where.
        stop: Stop,
    },
    /// A rail does not guide a station.
    Rail {
        /// The rail: 0 or 1.
        rail: usize,
        /// Where.
        stop: Stop,
        /// What is wrong.
        fault: RailFault,
    },
    /// A rail on the path, or two rails at one point, across a station (within the
    /// resolution): the profile would shrink to nothing.
    RailsMeet {
        /// Where.
        stop: Stop,
    },
}

impl LawFault {
    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Twist(_) => "twist",
            Self::EndScale(_) => "end_scale",
            Self::LeastCrossing(_) => "least_crossing",
            Self::LeastSine(_) => "least_sine",
            Self::Crossing { .. } => "crossing",
            Self::AlongDirection { .. } => "along_direction",
            Self::Rail { .. } => "rail",
            Self::RailsMeet { .. } => "rails_meet",
        }
    }
}

impl fmt::Display for LawFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Twist(t) => write!(
                f,
                "the twist {t} rad is not finite and at most {MAX_TURNS} turns"
            ),
            Self::EndScale(s) => write!(f, "the end scale {s} is not finite and positive"),
            Self::LeastCrossing(a) => {
                write!(f, "the least crossing angle {a} rad is outside [0, pi / 2)")
            }
            Self::LeastSine(s) => write!(f, "the least sine {s} is outside [0, 1)"),
            Self::Crossing { stop } => write!(
                f,
                "the path crosses the parallel profile's plane by no more than the least angle at {stop:?}"
            ),
            Self::AlongDirection { stop } => {
                write!(
                    f,
                    "the path runs along the orientation's direction at {stop:?}"
                )
            }
            Self::Rail { rail, stop, fault } => {
                write!(f, "rail {rail} {} at {stop:?}", fault.says())
            }
            Self::RailsMeet { stop } => write!(
                f,
                "a rail meets the path, or the two rails meet, at {stop:?}: the profile shrinks to nothing"
            ),
        }
    }
}

fn refused(fault: LawFault) -> MakeError {
    MakeError::Refused(Refusal::Law(fault))
}

/// `m v` for a map by its columns.
fn apply(m: &[Vec3; 3], v: Vec3) -> Vec3 {
    let [a, b, c] = *m;
    a * v.x + b * v.y + c * v.z
}

const AXES: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];

/// Checks the values of `frame` that no station is needed for, `tiny` the resolution.
fn check(frame: &PathFrame, tiny: f64) -> Result<(), MakeError> {
    match frame {
        PathFrame::Shaped(shape) => {
            let twist = shape.twist;
            if !(twist.is_finite() && twist.abs() <= MAX_TURNS * TAU) {
                return Err(refused(LawFault::Twist(twist)));
            }
            let scale = shape.end_scale;
            if !(scale.is_finite() && scale > 0.0) {
                return Err(refused(LawFault::EndScale(scale)));
            }
            match shape.orientation {
                Orientation::Perpendicular => {}
                Orientation::Parallel { least_crossing } => {
                    if !(0.0..FRAC_PI_2).contains(&least_crossing) {
                        return Err(refused(LawFault::LeastCrossing(least_crossing)));
                    }
                }
                Orientation::Direction { least_sine, .. } => {
                    if !(0.0..1.0).contains(&least_sine) {
                        return Err(refused(LawFault::LeastSine(least_sine)));
                    }
                }
            }
        }
        PathFrame::Railed { rails, .. } => {
            for (rail, r) in rails.iter().enumerate() {
                r.check(tiny).map_err(|fault| {
                    refused(LawFault::Rail {
                        rail,
                        stop: Stop::At(0.0),
                        fault,
                    })
                })?;
            }
        }
    }
    Ok(())
}

/// The placements of a profile whose plane's normal is `normal`, swept along `path` by
/// `frame`, at `stops` in order (module docs).
///
/// # Errors
///
/// - [`Refusal::Path`] for a fraction outside `[0, 1]` or a corner the path does not have.
/// - [`Refusal::Law`] for a twist, end scale or least angle or sine out of range, a parallel
///   profile the path does not cross by the least angle, a path along the orientation's
///   direction, a rail not a finite line, arc or clamped NURBS curve, a rail not crossing a
///   station's plane exactly once (short of it, touching it, crossing it twice, lying in it,
///   too near to tell), or rails meeting each other or the path.
/// - [`Unsupported::RailsAtCorner`] for rails along a path with a corner.
pub fn placements(
    path: &Path,
    normal: UnitVec3,
    frame: &PathFrame,
    stops: &[Stop],
    doc: &Document,
) -> Result<Vec<Placement>, MakeError> {
    let tiny = doc.resolution().zone().mm();
    check(frame, tiny)?;
    let [first] = *frames(path, &[Stop::At(0.0)])?.as_slice() else {
        return Err(MakeError::Refused(Refusal::Path(
            crate::frames::FrameError::Pieces(0),
        )));
    };
    match frame {
        PathFrame::Shaped(shape) => stops
            .iter()
            .map(|&stop| shaped(path, normal, shape, first.at, first.tangent, stop))
            .collect(),
        PathFrame::Railed { rails, fit } => railed(path, rails, *fit, first.at, stops, tiny),
    }
}

/// One station's placement by the shape laws.
fn shaped(
    path: &Path,
    normal: UnitVec3,
    shape: &Shape,
    s0: Point3,
    t0: UnitVec3,
    stop: Stop,
) -> Result<Placement, MakeError> {
    let station = frames(path, &[stop])?;
    let Some(st) = station.first() else {
        return Err(MakeError::Refused(Refusal::Path(
            crate::frames::FrameError::Pieces(0),
        )));
    };
    let corner = match stop {
        Stop::Corner(j) => path.corners().nth(j).copied(),
        _ => None,
    };
    let u = match (stop, corner) {
        (_, Some(c)) => c.fraction,
        (Stop::At(u), None) => u,
        _ => 0.0,
    };
    let axis = if normal.dot(*t0) < 0.0 {
        -normal
    } else {
        normal
    };
    let t = st.tangent;
    let turn: [Vec3; 3] = match shape.orientation {
        Orientation::Perpendicular => st.turn.0,
        Orientation::Parallel { least_crossing } => {
            // the angle between the tangent and the plane is asin(t . axis)
            if t.dot(*axis) <= fmath::sin(least_crossing) {
                return Err(refused(LawFault::Crossing { stop }));
            }
            Rotation::IDENTITY.0
        }
        Orientation::Direction { along, least_sine } => {
            let frame_at = |t: UnitVec3| -> Option<[Vec3; 3]> {
                let across = along.as_vec() - t.as_vec() * t.dot(*along);
                if across.length() <= least_sine {
                    return None;
                }
                let r = across.normalized()?;
                Some([t.as_vec(), r.as_vec(), t.cross(*r)])
            };
            let along_fault = || refused(LawFault::AlongDirection { stop });
            let f0 = frame_at(t0).ok_or_else(along_fault)?;
            let fi = frame_at(t).ok_or_else(along_fault)?;
            // fi f0^T
            AXES.map(|e| apply(&fi, Vec3::new(f0[0].dot(e), f0[1].dot(e), f0[2].dot(e))))
        }
    };
    let (sin, cos) = fmath::sin_cos(shape.twist * u);
    let scale = 1.0 + (shape.end_scale - 1.0) * u;
    let n = axis.as_vec();
    let shape_of = |v: Vec3| {
        let along = n * v.dot(n);
        let r = (v - along) * scale;
        along + r * cos + n.cross(r) * sin
    };
    let mut m = AXES.map(|e| apply(&turn, shape_of(e)));
    let parallel = matches!(shape.orientation, Orientation::Parallel { .. });
    if let (Some(c), false) = (corner, parallel)
        && let Some(b) = (c.to.as_vec() - c.from.as_vec()).normalized()
    {
        let b = b.as_vec();
        for col in &mut m {
            *col += b * ((c.stretch - 1.0) * col.dot(b));
        }
    }
    Ok(Placement {
        from: s0,
        at: st.at,
        m,
    })
}

/// The map that turns `along` (unit) onto `along_to`, `across` (unit, square to `along`) onto
/// `across_to` sized by `k`, and the third way to follow, sized by `k` or kept (SolidCraft's
/// `rail_frame`).
fn rail_map(
    along: Vec3,
    across: Vec3,
    along_to: Vec3,
    across_to: Vec3,
    k: f64,
    fit: RailFit,
) -> [Vec3; 3] {
    let k_other = match fit {
        RailFit::Scale => k,
        RailFit::Stretch => 1.0,
    };
    let (w0, w1) = (along.cross(across), along_to.cross(across_to));
    AXES.map(|v| {
        along_to * v.dot(along) + across_to * (k * v.dot(across)) + w1 * (k_other * v.dot(w0))
    })
}

/// The placements by guide rails (module docs).
fn railed(
    path: &Path,
    rails: &Rails,
    fit: RailFit,
    s0: Point3,
    stops: &[Stop],
    tiny: f64,
) -> Result<Vec<Placement>, MakeError> {
    if path.corners().next().is_some() {
        return Err(MakeError::Unsupported(Unsupported::RailsAtCorner));
    }
    // the anchor, and the line across from it square to the tangent, at a stop
    let across_at = |stop: Stop| -> Result<(UnitVec3, Point3, Vec3), MakeError> {
        let station = frames(path, &[stop])?;
        let Some(st) = station.first() else {
            return Err(MakeError::Refused(Refusal::Path(
                crate::frames::FrameError::Pieces(0),
            )));
        };
        let mut hits = Vec::with_capacity(2);
        for (rail, r) in rails.iter().enumerate() {
            let hit = rail::crossing(r, st.at, st.tangent, tiny)
                .map_err(|fault| refused(LawFault::Rail { rail, stop, fault }))?;
            hits.push(hit);
        }
        let (anchor, v) = match *hits.as_slice() {
            [r] => (st.at, r - st.at),
            [a, b, ..] => (a, b - a),
            [] => (st.at, Vec3::ZERO),
        };
        let t = st.tangent.as_vec();
        Ok((st.tangent, anchor, v - t * v.dot(t)))
    };
    let start = Stop::At(0.0);
    let (t0, a0, d0) = across_at(start)?;
    let u0 = match d0.normalized() {
        Some(u) if d0.length() > tiny => u,
        _ => return Err(refused(LawFault::RailsMeet { stop: start })),
    };
    stops
        .iter()
        .map(|&stop| {
            let (t, anchor, d) = across_at(stop)?;
            let ud = match d.normalized() {
                Some(u) if d.length() > tiny => u,
                _ => return Err(refused(LawFault::RailsMeet { stop })),
            };
            let k = d.length() / d0.length();
            let m = rail_map(t0.as_vec(), u0.as_vec(), t.as_vec(), ud.as_vec(), k, fit);
            // the anchor's start lands on the anchor
            Ok(Placement {
                from: s0,
                at: anchor - apply(&m, a0 - s0),
                m,
            })
        })
        .collect()
}

/// Where a skin through the sweep's stations puts them, as fractions of the path's arc
/// length in increasing order (module docs; SolidCraft's convention).
///
/// # Errors
///
/// [`Refusal::Law`] for a twist or end scale out of range or a rail of no shape, as
/// [`placements`].
pub fn fractions(path: &Path, frame: &PathFrame, doc: &Document) -> Result<Vec<f64>, MakeError> {
    check(frame, doc.resolution().zone().mm())?;
    let twist = match frame {
        PathFrame::Shaped(s) => s.twist.abs(),
        PathFrame::Railed { .. } => 0.0,
    };
    let turn = twist + path.tangent_turn() + path.corners().map(|c| c.angle).sum::<f64>();
    let rail_points = match frame {
        PathFrame::Shaped(_) => 0,
        PathFrame::Railed { rails, .. } => {
            rails.iter().map(Rail::control_points).max().unwrap_or(0)
        }
    };
    let by_turn = (turn / (PI / 180.0)).ceil();
    let by_turn = if by_turn.is_finite() && by_turn >= 0.0 {
        by_turn as usize
    } else {
        MAX_SKIN_STATIONS
    };
    let n = by_turn
        .max(rail_points.saturating_sub(1))
        .clamp(MIN_SKIN_STATIONS, MAX_SKIN_STATIONS);
    let mut out: Vec<f64> = (0..=n).map(|j| j as f64 / n as f64).collect();
    out.extend(path.joints().skip(1).map(|(_, at, _)| at));
    out.sort_by(f64::total_cmp);
    out.dedup();
    Ok(out)
}

/// Sweeps `profile`, placed in `plane`'s `x, y` plane, along `path` by `frame`.
///
/// The plain sweep ([`Shape::PLAIN`]: perpendicular, no twist, scale 1) builds its walls by
/// kind (module [`walls`]): `plane`'s origin is the path's start and its z axis the start's
/// tangent. Any other frame (a twist, an end scale, another orientation, rails) is checked in
/// full and every station [`fractions`] gives is placed, so a refusal comes before the answer
/// that its walls are not built yet: they are faces on step 5's procedural sweep kind.
///
/// # Errors
///
/// - [`Refusal::Resolution`] when the profile was built at a resolution other than `doc`'s.
/// - The plain sweep's: [`Refusal::Placement`] when `plane` is not at the path's start along
///   its tangent; [`Refusal::TightArc`], [`Refusal::MitresMeet`], [`Refusal::ArcTurn`] and
///   [`Refusal::ClosedTurned`] when the swept profile would meet itself or its ends not
///   meet, at the first piece that does; [`Refusal::Path`] for a closed path that doubles
///   back where it closes; [`Unsupported::HelixPiece`], [`Unsupported::CornerOnArc`],
///   [`Unsupported::Segment`] for a profile segment that is not a line, arc or circle, and
///   [`Unsupported::SpindleTorus`] for a profile arc whose circle crosses an arc piece's
///   axis outside it; a [`Defect`](crate::Defect) when the body does not finish, or
///   [`MakeError::Full`].
/// - Any other frame's: those of [`placements`] at every station of [`fractions`], then
///   [`Unsupported::NeedsKind`] with [`Needed::ProceduralSweep`].
pub fn sweep(
    profile: &Region,
    plane: &Frame,
    path: &Path,
    frame: &PathFrame,
    doc: &Document,
) -> Result<Made<SweepFaces>, MakeError> {
    if *frame == PathFrame::Shaped(Shape::PLAIN) {
        return walls::build(profile, plane, path, doc);
    }
    if profile.resolution() != doc.resolution() {
        return Err(MakeError::Refused(Refusal::Resolution {
            region: profile.resolution(),
            document: doc.resolution(),
        }));
    }
    let stops: Vec<Stop> = fractions(path, frame, doc)?
        .into_iter()
        .map(Stop::At)
        .collect();
    let _placed = placements(path, plane.z(), frame, &stops, doc)?;
    // Step 5 plugs in here: one wall per profile segment on the procedural sweep surface of
    // that segment carried by `_placed` (its own bound, not `fractions`, setting the
    // stations), joined along the profile's vertices' curves, and the two planar caps (the
    // end cap the profile placed by the last placement), into `SweepFaces` as the plain
    // sweep's walls fill it (`[piece][loop][segment]`).
    Err(MakeError::Unsupported(Unsupported::NeedsKind {
        kind: Needed::ProceduralSweep,
    }))
}
