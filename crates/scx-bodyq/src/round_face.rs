// Written for solidcorex. Apache-2.0.
//! What a body has on and past a round face of it (research 74 §34, BW11), read from the
//! exact body: how far the body reaches in the seat's frame, what lies past an end of the
//! face (nothing, a chamfer or a round, a neck, a counterbore, a shoulder), the largest
//! radius it reaches over a stretch, whether a stretch beside an end is empty, whether the
//! face is a whole band, and the block a face grown on its own stretch of the body is cut
//! out with.
//!
//! The pin's recipe is kept: the body is read through probes. This crate sits below
//! `scx-boolean`, so it cannot intersect the body with a ring itself: a caller answers each
//! [`Probe`] (a plain ring or disc about the seat's axis over a stretch, to be intersected with
//! the body) with what it met as an exact box in the seat's frame ([`Met`]). The map into the
//! frame is [`into_seat`], and [`extent_in`] measures the body there.
//!
//! A seat is the plane square to the axis at the face's free end, the side the face runs on
//! to, and its length ([`SeatFrame`]). "Into the face" means along the axis from the free end
//! towards the face, so below zero past the end.
//!
//! | pin (`teeth::round_face`, ops `round_face`) | here |
//! |---|---|
//! | `clear_of(r) = 1e-6 (1 + r)` | the [`Near`]'s coincidence |
//! | `same_size(r) = max(1e-3 r, 1e-3)` | `max(SAME_SIZE r, coincidence)` |
//! | `10 clear`, `1000 clear` | `RIDGE`, `BESIDE` times the coincidence |
//! | `Unread` probes read as "shut" or "not out", an unreadable extent as `None` | [`Unsupported(Probe)`](BodyqUnsupported::Probe): never taken as nothing standing there |
//! | hostile numbers answered by a shut end | refused |
//! | `MOST_PROBES` 7 | none: each reading asks at most six, by its structure |
//! | `whole_band` from 24 sampled points of an edge held as another curve | circles across the axis and lines along it are proved; any other curve is [`Band::Broken`] when two of its points differ in height and in place round the axis (a witness, so a sample refutes), else [`Band::Undecided`] |
//! | `extent_in`: a copy of the body moved by `into_seat`, then its exact box | the same: a moved copy and `tight_body_box` |

use scx_brep::op::Cx;
use scx_brep::{Brep, EdgeId, EdgeShape, FaceId};
use scx_curve::Curve;
use scx_num::frame::Frame;
use scx_num::math::Affine;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::Surface;

use crate::error::{BodyqDefect, BodyqError, BodyqRefusal, BodyqUnsupported};
use crate::reach::{Near, finite, finite_point};
use crate::tight::tight_body_box;

/// A radius is "the same" as the face's within this fraction of it, or the coincidence.
pub const SAME_SIZE: f64 = 1e-3;
/// A step or neck is clear of the face's end when it is farther than this many coincidences.
pub const RIDGE: f64 = 10.0;
/// The stretch beside an end that [`beside_clear`] reads: this many coincidences.
pub const BESIDE: f64 = 1e3;
/// Two slopes along a chamfer tie when within this fraction of its length.
pub const SLOPE_TIE: f64 = 1e-3;

/// The side the face runs on to from its free end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inward {
    /// Along the plane's normal.
    Along,
    /// Against it.
    Against,
}

impl Inward {
    const fn sign(self) -> f64 {
        match self {
            Self::Along => 1.0,
            Self::Against => -1.0,
        }
    }
}

/// A round face's seat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeatFrame {
    /// The plane square to the axis at the free end: its x is angle 0, its z the axis.
    pub plane: Frame,
    /// The side the face runs on to from the free end.
    pub inward: Inward,
    /// The length of the face along its axis.
    pub room: f64,
    /// The pick, brought onto the face's cylinder along its radius.
    pub at: Point3,
}

/// How far a body reaches in a seat's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    /// The least it reaches into the face from the free end: below zero where it runs on
    /// past the free end.
    pub low: f64,
    /// The most it reaches into the face from the free end.
    pub high: f64,
    /// The most it reaches from the axis along the frame's two cross directions (a round
    /// body's radius).
    pub across: f64,
}

/// A plain ring (a disc where it has no hole) about a seat's axis, over a stretch along it:
/// what a reading asks the body to be intersected with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Probe {
    /// The plane the ring lies in: the seat's plane.
    pub plane: Frame,
    /// The radius of its hole; none for a disc.
    pub inner: Option<f64>,
    /// Its outer radius.
    pub outer: f64,
    /// Where it starts along the plane's normal, from the plane.
    pub from: f64,
    /// Where it ends along the plane's normal, from the plane: above `from`.
    pub to: f64,
}

/// What a probe met of the body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Met {
    /// Nothing: the intersection is empty.
    Nothing,
    /// A piece, as its exact box in the seat's frame ([`into_seat`]).
    Piece {
        /// The least corner of the box.
        min: [f64; 3],
        /// The greatest corner of the box.
        max: [f64; 3],
    },
    /// The intersection or its box could not be made.
    Unread,
}

/// One end of a round face, as the body has it: what lies past it.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // independent readings of one end
pub struct End {
    /// The body ends where the face ends: nothing of it lies past the end.
    pub free: bool,
    /// Nothing stands in the way past the end: no shoulder larger than a shaft, no narrowing
    /// of a bore. A free end is open.
    pub open: bool,
    /// The cuts come out of the body past this end, as a cutter runs out of the work.
    pub out: bool,
    /// How far the cuts must run on past the end to come out.
    pub through: f64,
    /// The face stops at a step: past the end the body is smaller from the very end on (a
    /// shaft's neck) or wider (a bore's counterbore).
    pub step: bool,
    /// At a shaft's step, the radius of the body just past the end.
    pub neck: f64,
    /// How far past the face's end the body's own end lies, where what lies between belongs
    /// to the face's end (a chamfer, a round). Zero otherwise.
    pub end: f64,
    /// The radius at the body's own end: the rim of a shaft's end face, the mouth of a bore.
    pub rim: f64,
    /// What lies between is a plain chamfer: straight from the rim to the face.
    pub cone: bool,
}

impl End {
    /// An end nothing is known to be past, or that something stands in the way of, on a face
    /// of radius `radius`: the cuts stop at it.
    #[must_use]
    pub const fn shut(radius: f64) -> Self {
        Self {
            free: false,
            open: false,
            out: false,
            through: 0.0,
            step: false,
            neck: radius,
            end: 0.0,
            rim: radius,
            cone: false,
        }
    }

    /// How far past the end the cuts run to come out of the body: the farther of
    /// [`End::through`] and [`End::end`] where the cuts come out, else zero.
    #[must_use]
    pub fn run_out(&self) -> f64 {
        if self.out {
            self.through.max(self.end)
        } else {
            0.0
        }
    }

    /// How far a tool that takes the whole round face runs on past this end, with `margin`
    /// to spare: through what the body has on its end and the margin, the margin past an end
    /// the body ends at, and not at all at a shoulder or a neck.
    #[must_use]
    pub fn run_past(&self, margin: f64) -> f64 {
        if self.end > 0.0 {
            self.end + margin
        } else if self.free {
            margin
        } else {
            0.0
        }
    }

    /// How far into the round face, past the end's plane, a chamfer on this end reaches once
    /// the face has been moved from the radius `from` to the radius `to`: a moved face takes
    /// the chamfer with it along the chamfer's own cone, which meets the new surface further
    /// in by its slope times the move. Negative where the face moved towards the chamfer's
    /// rim. Zero where the end has no chamfer of its own and where the numbers give no finite
    /// length (a rim on the radius `from`).
    #[must_use]
    pub fn carried(&self, from: f64, to: f64) -> f64 {
        if !(self.cone && self.end > 0.0) {
            return 0.0;
        }
        let reach = self.end * (to - from) / (from - self.rim);
        if reach.is_finite() { reach } else { 0.0 }
    }
}

/// The block a round face is grown on its own stretch of a body with: a round slab about the
/// seat's axis, wider than the body and than the grown face, over the stretch the face runs
/// along.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrowBlock {
    /// The plane the slab lies in: the seat's plane.
    pub plane: Frame,
    /// The slab's radius.
    pub radius: f64,
    /// Where it starts along the plane's normal, from the plane.
    pub from: f64,
    /// Where it ends along the plane's normal, from the plane: above `from`.
    pub to: f64,
}

/// A radius: finite and above zero.
fn radius_of(r: f64) -> Result<f64, BodyqError> {
    finite(r)?;
    if r > 0.0 {
        Ok(r)
    } else {
        Err(BodyqError::Refused(BodyqRefusal::NotPositive))
    }
}

/// The distances along the plane's normal of the stretch from `from` to `to` measured into
/// the face from its free end.
fn span(seat: &SeatFrame, from: f64, to: f64) -> (f64, f64) {
    if seat.inward.sign() > 0.0 {
        (from, to)
    } else {
        (-to, -from)
    }
}

/// The map that takes world coordinates into the seat's frame: its plane's x, y and normal,
/// its origin to zero. A rigid motion, so a body moved by it stays exact.
///
/// # Errors
///
/// Refused ([`Seat`](BodyqRefusal::Seat)) for a frame whose map has no inverse.
pub fn into_seat(seat: &SeatFrame) -> Result<Affine, BodyqError> {
    seat.plane
        .to_affine()
        .inverse()
        .ok_or(BodyqError::Refused(BodyqRefusal::Seat))
}

/// How far a body reaches in the seat's frame, from its exact box there (`min..max`, as the
/// body moved by [`into_seat`] has it).
///
/// # Errors
///
/// Refused for a number of the box that is not finite.
pub fn extent(seat: &SeatFrame, min: [f64; 3], max: [f64; 3]) -> Result<Extent, BodyqError> {
    let [min_x, min_y, min_z] = min;
    let [max_x, max_y, max_z] = max;
    let (a, b) = (min_z * seat.inward.sign(), max_z * seat.inward.sign());
    let across = min_x
        .abs()
        .max(max_x.abs())
        .max(min_y.abs())
        .max(max_y.abs());
    if a.is_finite() && b.is_finite() && across.is_finite() {
        Ok(Extent {
            low: a.min(b),
            high: a.max(b),
            across,
        })
    } else {
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    }
}

/// How far `body` reaches in the seat's frame: the body is moved into the frame (a rigid
/// motion, which keeps it exact) and boxed ([`tight_body_box`]), so a shaft that lies askew to
/// the world axes is measured as truly as one along them.
///
/// # Errors
///
/// [`Seat`](BodyqRefusal::Seat); [`Moved`](BodyqDefect::Moved) where the moved body does not
/// finish; those of [`tight_body_box`].
pub fn extent_in(body: &Brep, seat: &SeatFrame, cx: &mut Cx<'_>) -> Result<Extent, BodyqError> {
    let map = into_seat(seat)?;
    let mut draft = body.edit();
    draft
        .transform(&map, cx.doc)
        .map_err(|_| BodyqError::Defect(BodyqDefect::Moved))?;
    let (moved, _) = draft
        .finish(cx.doc)
        .map_err(|_| BodyqError::Defect(BodyqDefect::Moved))?;
    let b = tight_body_box(&moved, cx)?;
    extent(
        seat,
        [b.x_min(), b.y_min(), b.z_min()],
        [b.x_max(), b.y_max(), b.z_max()],
    )
}

/// The seat seen from the other end of its round face: the far end as the free end, the plane
/// moved there along its normal with angle 0 where it was, the face running on the other way.
///
/// # Errors
///
/// Refused for a seat whose length is not finite.
pub fn far_end(seat: &SeatFrame) -> Result<SeatFrame, BodyqError> {
    finite(seat.room)?;
    let p = &seat.plane;
    let origin = p.origin() + p.z().as_vec() * (seat.inward.sign() * seat.room);
    let plane = Frame::from_axes(origin, p.x().as_vec(), p.z().as_vec())
        .map_err(|_| BodyqError::Refused(BodyqRefusal::Seat))?;
    Ok(SeatFrame {
        plane,
        inward: match seat.inward {
            Inward::Along => Inward::Against,
            Inward::Against => Inward::Along,
        },
        room: seat.room,
        at: seat.at,
    })
}

/// A body read through its probes.
struct Reader<'s, 'p> {
    seat: &'s SeatFrame,
    probe: &'p mut dyn FnMut(&Probe) -> Met,
    clear: f64,
}

impl Reader<'_, '_> {
    /// What the body has within the ring from `inner` (none where it is not above zero) to
    /// `outer` over the stretch `from..to` measured into the face. A ring that is no ring
    /// (radii not finite or out of order, a stretch not finite or of no length) is not
    /// asked, and not read: [`Unsupported(Probe)`](BodyqUnsupported::Probe).
    fn met(&mut self, inner: f64, outer: f64, from: f64, to: f64) -> Result<Met, BodyqError> {
        let (a, b) = span(self.seat, from, to);
        let inner = (inner > 0.0).then_some(inner);
        let ring = outer.is_finite()
            && outer > 0.0
            && inner.is_none_or(|r| r.is_finite() && r < outer)
            && a.is_finite()
            && b.is_finite()
            && a < b;
        if !ring {
            return Err(BodyqError::Unsupported(BodyqUnsupported::Probe));
        }
        match (self.probe)(&Probe {
            plane: self.seat.plane,
            inner,
            outer,
            from: a,
            to: b,
        }) {
            Met::Unread => Err(BodyqError::Unsupported(BodyqUnsupported::Probe)),
            m => Ok(m),
        }
    }

    fn piece(&self, min: [f64; 3], max: [f64; 3]) -> Result<Extent, BodyqError> {
        extent(self.seat, min, max)
    }

    /// How far towards the body's end (up to `zone` past the face's end) what the body has
    /// within `rho` of the axis reaches; 0 where it has nothing there.
    fn reaches(&mut self, rho: f64, zone: f64) -> Result<f64, BodyqError> {
        let clear = self.clear;
        match self.met(0.0, rho, -(zone + clear), -clear)? {
            Met::Piece { min, max } => Ok((-self.piece(min, max)?.low).max(0.0).min(zone)),
            _ => Ok(0.0),
        }
    }

    /// The largest radius of the body between `from` and `to` past the end, inside `far`; 0
    /// where it has nothing there.
    fn radius_in(&mut self, far: f64, from: f64, to: f64) -> Result<f64, BodyqError> {
        match self.met(0.0, far, from, to)? {
            Met::Piece { min, max } => Ok(self.piece(min, max)?.across),
            _ => Ok(0.0),
        }
    }
}

fn same_size(radius: f64, near: &Near) -> f64 {
    (SAME_SIZE * radius).max(near.coincidence().mm())
}

/// What lies past the seat's free end of a round face of radius `radius` (a `bore`'s wall,
/// else the outside of a shaft), on a body whose extent in the seat's frame is `body`;
/// `reach` is the radius the cuts reach (the bottoms of a shaft's tooth spaces, of a hub's in
/// a bore). For the far end, the seat seen from there ([`far_end`]).
///
/// The body is asked through `probe` (at most six times): plain rings and discs over the
/// stretch past the end, and the exact boxes of what they meet. Anything in the way (a
/// shoulder, a bore that narrows) shuts the end. Past an open end: how far the cuts must run
/// to come out; a step from the very end on (a shaft's neck, a bore's counterbore); and a short
/// stretch belonging to the end (a chamfer or a round), whose slope at two places along it
/// tells a plain chamfer from a round.
///
/// # Errors
///
/// Refused for a radius not finite and above zero. Unsupported
/// ([`Probe`](BodyqUnsupported::Probe)) where a probe came back [`Met::Unread`]: the end is not
/// taken as shut then, the caller chooses ([`End::shut`]).
pub fn past_end(
    seat: &SeatFrame,
    radius: f64,
    bore: bool,
    reach: f64,
    body: &Extent,
    near: &Near,
    probe: &mut dyn FnMut(&Probe) -> Met,
) -> Result<End, BodyqError> {
    let radius = radius_of(radius)?;
    finite(reach)?;
    let shut = End::shut(radius);
    let zone = -body.low;
    let same = same_size(radius, near);
    if !zone.is_finite() {
        return Err(BodyqError::Refused(BodyqRefusal::NotFinite));
    }
    if zone <= same {
        return Ok(End {
            free: true,
            open: true,
            out: true,
            ..shut
        });
    }
    let clear = near.coincidence().mm();
    let mut reader = Reader { seat, probe, clear };
    let far = radius + body.across + 1.0;
    // Anything in the way: a shoulder larger than the shaft, a bore that narrows.
    let blocked = if bore {
        reader.met(0.0, radius - clear, -(zone + clear), -clear)?
    } else {
        reader.met(radius + clear, far, -(zone + clear), -clear)?
    };
    if blocked != Met::Nothing {
        return Ok(shut);
    }
    // As far as the body has anything at the radius the cuts reach.
    let cut = if bore {
        reader.met(0.0, reach + clear, -(zone + clear), -clear)?
    } else {
        reader.met((reach - clear).max(clear), far, -(zone + clear), -clear)?
    };
    let stands = match cut {
        Met::Piece { min, max } => -reader.piece(min, max)?.low,
        _ => 0.0,
    };
    let (out, through) = if stands.is_finite() && stands <= radius {
        (true, stands.max(0.0).min(zone))
    } else {
        (false, 0.0)
    };
    let open = End {
        open: true,
        out,
        through,
        ..shut
    };
    let ridge = RIDGE * clear;
    // The slope of a straight chamfer is told from a round by where it is at two places.
    let near_to = |a: f64, b: f64| (a - b).abs() <= SLOPE_TIE * zone + ridge;
    if bore {
        let wider = zone / 16.0;
        let first = reader.reaches(radius + wider, zone)?;
        if first <= ridge {
            // Wider from the very end on: a counterbore.
            return Ok(End { step: true, ..open });
        }
        if zone > radius || first >= zone - ridge {
            // A long stretch, or one hardly wider at the body's end than the bore: not an
            // entry to carry.
            return Ok(open);
        }
        // A straight chamfer that is `wider` across where it is `first` long is this far
        // across at the end.
        let across = wider * zone / first;
        if across > 2.0 * zone + same {
            return Ok(open);
        }
        let mut cone = true;
        for k in [0.5, 0.9] {
            cone &= near_to(reader.reaches(radius + k * across, zone)?, k * zone);
        }
        return Ok(End {
            end: zone,
            rim: if cone { radius + across } else { radius },
            cone,
            ..open
        });
    }
    let thin = (0.05 * zone).min(BESIDE * clear).max(clear);
    let beside = reader.radius_in(far, -(thin + clear), -clear)?;
    if beside < radius - ridge {
        // Smaller from the very end on: a neck.
        return Ok(End {
            step: true,
            neck: beside,
            ..open
        });
    }
    if zone > radius {
        return Ok(open);
    }
    // The body in the last of its length: where a straight chamfer comes to, its rim lies
    // this far in.
    let last = reader.radius_in(far, -(zone + clear), -zone + thin)?;
    let across = (radius - last) * zone / (zone - thin);
    if !(across >= -same && across <= 2.0 * zone + same) {
        return Ok(open);
    }
    let mut cone = across > ridge;
    if cone {
        for k in [0.5, 0.1] {
            cone &= near_to(
                reader.radius_in(far, -(k * zone + thin), -k * zone)?,
                radius - k * across,
            );
        }
    }
    Ok(End {
        end: zone,
        rim: if cone { radius - across } else { last },
        cone,
        ..open
    })
}

/// The largest radius a body reaches in the stretch `from..to` (measured from the seat's free
/// end into a face of radius `radius`) outside the radius `above`; `above` itself where it has
/// nothing there or the stretch is empty. `body` is the body's extent in the seat's frame; the
/// body is asked through `probe` once at most: what a ring over the stretch meets.
///
/// # Errors
///
/// Refused for a radius or `above` that is not a number. Unsupported
/// ([`Probe`](BodyqUnsupported::Probe)) where what the ring meets could not be read, a stretch
/// that is not finite among them: never taken as nothing standing there.
pub fn stands_over(
    seat: &SeatFrame,
    radius: f64,
    body: &Extent,
    above: f64,
    from: f64,
    to: f64,
    near: &Near,
    probe: &mut dyn FnMut(&Probe) -> Met,
) -> Result<f64, BodyqError> {
    radius_of(radius)?;
    finite(above)?;
    let clear = near.coincidence().mm();
    if to.partial_cmp(&from) != Some(core::cmp::Ordering::Greater) || body.across <= above + clear {
        return Ok(above);
    }
    let mut reader = Reader { seat, probe, clear };
    match reader.met(above + clear, body.across + 1.0, from, to)? {
        Met::Piece { min, max } => Ok(reader.piece(min, max)?.across.max(above)),
        _ => Ok(above),
    }
}

/// Whether a body leaves the stretch just past the seat's end (`BESIDE` coincidences) empty
/// within `within` of the axis: a chamfer that wide at a bore's step cuts into no wall there.
/// The body is asked through `probe` once: what a disc over the stretch meets.
///
/// # Errors
///
/// Refused for a radius or `within` that is not finite and above zero. Unsupported
/// ([`Probe`](BodyqUnsupported::Probe)) where what the disc meets could not be read.
pub fn beside_clear(
    seat: &SeatFrame,
    radius: f64,
    within: f64,
    near: &Near,
    probe: &mut dyn FnMut(&Probe) -> Met,
) -> Result<bool, BodyqError> {
    radius_of(radius)?;
    radius_of(within)?;
    let clear = near.coincidence().mm();
    let mut reader = Reader { seat, probe, clear };
    Ok(matches!(
        reader.met(0.0, within, -(BESIDE * clear + clear), -clear)?,
        Met::Nothing
    ))
}

/// Whether the round face of radius `radius` on the seat is a whole band of the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Band {
    /// Every face of the body on the seat's cylinder is bounded only by circles across the
    /// axis on that cylinder (its ends) and by straight lines along the axis (seams).
    Whole,
    /// This edge of this face on the cylinder is proved neither: a hole across a shaft, a
    /// flat, a keyway or a set screw's hole in a hub that opens in the face leaves such edges.
    /// A circle or line that is not a rim or a seam, or a curve with points at two heights
    /// and two places round the axis, proves it.
    Broken {
        /// The face on the cylinder.
        face: FaceId,
        /// Its edge that is no end and no seam.
        edge: EdgeId,
    },
    /// No edge is proved not to bound a band, but this one is a curve (a spline, an ellipse)
    /// that is not proved a rim or a seam, nor refuted as both.
    Undecided {
        /// The face on the cylinder.
        face: FaceId,
        /// The edge.
        edge: EdgeId,
    },
    /// No face of the body lies on the seat's cylinder.
    Absent,
}

/// Whether the round face of radius `radius` on the seat is a whole band of `body`.
///
/// A face lies on the seat's cylinder when it is a cylinder of that radius whose axis is the
/// seat's within coincidence over the seat's length. An edge of it bounds a band when it is a
/// circle of that radius about the axis, across it, or a straight line along the axis, each
/// within coincidence.
///
/// # Errors
///
/// Refused for a radius not finite and above zero, or a seat whose length is not above zero.
pub fn whole_band(
    body: &Brep,
    seat: &SeatFrame,
    radius: f64,
    near: &Near,
) -> Result<Band, BodyqError> {
    let radius = radius_of(radius)?;
    let room = radius_of(seat.room)?;
    let k = seat.plane.z().as_vec();
    let o = seat.plane.origin();
    finite_point(o)?;
    let axis_reach = near.over(scx_num::units::Length::from_mm(room))?;
    let off_axis = |p: Point3, dir: Vec3| -> (f64, f64) {
        let d = p - o;
        (dir.cross(k).length(), (d - k * d.dot(k)).length())
    };
    let on_cylinder = |s: &Surface| match s {
        Surface::Cylinder(c) => {
            let (lean, off) = off_axis(c.frame().origin(), c.frame().z().as_vec());
            axis_reach.within(lean * room)
                && near.within(off)
                && near.within(c.radius().mm() - radius)
        }
        _ => false,
    };
    let mut undecided = None;
    let mut any = false;
    for face in body.faces() {
        if !on_cylinder(face.surface()) {
            continue;
        }
        any = true;
        for u in face.uses() {
            let edge = u.edge();
            let EdgeShape::Curve { curve, interval } = edge.shape() else {
                continue;
            };
            let verdict = match curve {
                Curve::Circle(c) => {
                    let (lean, off) = off_axis(c.frame().origin(), c.frame().z().as_vec());
                    let ring = near.over(c.radius())?;
                    Some(
                        ring.within(lean * c.radius().mm())
                            && near.within(off)
                            && near.within(c.radius().mm() - radius),
                    )
                }
                Curve::Line(_) => {
                    let (a, b) = (edge.start().point(), edge.end().point());
                    let chord = b - a;
                    let len = chord.length();
                    Some(len > near.coincidence().mm() && near.within(chord.cross(k).length()))
                }
                // Any other curve (an ellipse, a NURBS): not proved a rim or a seam. It is
                // refuted as both by witnesses: points of it at different heights (no rim)
                // and at different places round the axis (no seam).
                _ => {
                    let at = |t: f64| {
                        let d = curve.point(t) - o;
                        let h = d.dot(k);
                        (h, d - k * h)
                    };
                    let [lo, hi] = [interval.min(), interval.max()];
                    let pts = [at(lo), at(0.5 * (lo + hi)), at(hi)];
                    let mut rises = false;
                    let mut turns = false;
                    for (i, (h, a)) in pts.iter().enumerate() {
                        for (g, b) in pts.iter().skip(i + 1) {
                            rises |= !near.within(h - g);
                            turns |= !near.within((*a - *b).length());
                        }
                    }
                    (rises && turns).then_some(false)
                }
            };
            match verdict {
                Some(true) => {}
                Some(false) => {
                    return Ok(Band::Broken {
                        face: face.id(),
                        edge: edge.id(),
                    });
                }
                None => undecided = undecided.or_else(|| Some((face.id(), edge.id()))),
            }
        }
    }
    Ok(match (any, undecided) {
        (false, _) => Band::Absent,
        (true, Some((face, edge))) => Band::Undecided { face, edge },
        (true, None) => Band::Whole,
    })
}

/// The block that takes the stretch of a body a round face of radius `radius` runs along out
/// of it, for the face to be grown by `by` there on its own: a round slab wider than the body
/// (whose extent in the seat's frame is `body`) and than the grown face, from `before` past
/// the free end to `after` past the far end.
///
/// # Errors
///
/// Refused for a number that is not finite, a radius not above zero, or a stretch that comes
/// to no length.
pub fn grow_block(
    seat: &SeatFrame,
    radius: f64,
    body: &Extent,
    by: f64,
    before: f64,
    after: f64,
) -> Result<GrowBlock, BodyqError> {
    let radius = radius_of(radius)?;
    let slab = body.across + radius + by.abs() + 1.0;
    let (from, to) = span(seat, -before, seat.room + after);
    for x in [body.across, by, before, after, seat.room, slab, from, to] {
        finite(x)?;
    }
    if !(slab > 0.0 && from < to) {
        return Err(BodyqError::Refused(BodyqRefusal::NotPositive));
    }
    Ok(GrowBlock {
        plane: seat.plane,
        radius: slab,
        from,
        to,
    })
}
