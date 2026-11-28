// Written for solidcorex. Apache-2.0.
//! [`bores`]: the round holes and obround slots of a body, read from its faces (research 74
//! §2.3, §33, BW6; the pin's `eval::hole_stacks::bores`).
//!
//! An inward cylindrical wall is a cylinder face whose material lies on the side away from its
//! axis and whose axis side is not material. A wall that closes round the axis has a seam, an
//! edge its own loop uses twice. A partial wall has two sides, edges that run along the axis,
//! which stand a diameter apart when it is the end of an obround slot; two partial walls of
//! one radius and extent whose openings face each other across a gap are the two ends of one
//! slot, and two that share an axis are the halves of one round hole. A countersink or chamfer
//! that opens out of a wall's end is a coaxial cone face whose narrow rim is the wall's own
//! circle: the wall's extent runs on through it.
//!
//! **Every length is the body's coincidence** ([`body_coincidence`](super::prism)): where the
//! pin held `NEAR` at a micrometre and `COAXIAL` at a millimetre for rounding in its records,
//! a body here either meets at coincidence or does not. Every angle is a length: two axes are
//! parallel when they part by no more than the coincidence over the extent they are compared
//! across, and a slot end's sides are a diameter apart within the coincidence at its radius.
//!
//! **Nothing is swallowed.** A face that is not a bore wall is left out, which is an answer. A
//! face whose box, extent or edge cannot be read is an error; the work is charged to the
//! caller's budget (one unit a face and per comparison) and the stop flag is polled with it.

use std::collections::BTreeMap;

use scx_brep::{Brep, EdgeShape, Face, FaceSide};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::vector3d::{UnitVec3, Vec3};
use scx_surface::Surface;

use super::prism::body_coincidence;
use crate::error::{BodyqDefect, BodyqError};

/// An obround slot's shape across the axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    /// The overall length, end to end.
    pub length: f64,
    /// The unit direction of the length.
    pub along: UnitVec3,
}

/// One bore of one body: a round hole, or an obround slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bore {
    /// A point on the bore's axis: a round hole's own axis; a slot's, halfway between its ends.
    pub point: scx_num::vector3d::Point3,
    /// The unit axis, either way round.
    pub axis: UnitVec3,
    /// The bore's diameter.
    pub d: f64,
    /// The wall's extent along `axis`, measured from `point`.
    pub t: [f64; 2],
    /// The slot's shape across the axis.
    pub slot: Option<Slot>,
}

/// One cylindrical wall of one face.
struct Wall {
    point: Vec3,
    axis: UnitVec3,
    radius: f64,
    t: [f64; 2],
    /// `None` when the wall closes round the axis; else the unit direction from the axis to
    /// the middle of its opening.
    opening: Option<Vec3>,
}

/// One conical face that may extend a wall.
struct Cone {
    point: Vec3,
    axis: UnitVec3,
    radius_at_origin: f64,
    slope: f64,
    t: [f64; 2],
}

/// The bores of `body`: every round hole and obround slot, with its wall's extent (a
/// countersink or chamfer at a hole's end counts towards it). A partial wall that is not the
/// end of a slot is left out: a fastener cannot follow it.
///
/// One unit of `budget` per face and per pair compared.
///
/// # Errors
///
/// [`BodyqError::Exhausted`], [`BodyqError::Cancelled`], and
/// [`BodyqDefect::NoUvBox`] for a wall or cone face whose uv extent cannot be enclosed.
pub fn bores(body: &Brep, budget: &mut Budget, stop: &Stop) -> Result<Vec<Bore>, BodyqError> {
    let c = body_coincidence(body)?;
    let mut walls = Vec::new();
    let mut cones = Vec::new();
    for face in body.faces() {
        budget.charge(1)?;
        stop.check()?;
        match face.surface() {
            Surface::Cylinder(cy) if material_outside(face, cy.sense()) => {
                if let Some(w) = wall(face, cy.frame(), cy.sense(), cy.radius().mm(), c)? {
                    walls.push(w);
                }
            }
            Surface::Cone(cn) if material_outside(face, cn.sense()) => {
                let t = extent(face)?;
                let axis = axis_of(cn.frame(), cn.sense());
                cones.push(Cone {
                    point: cn.frame().origin().to_vec(),
                    axis,
                    radius_at_origin: cn.radius().mm(),
                    slope: fmath::tan(cn.half_angle()),
                    t,
                });
            }
            _ => {}
        }
    }
    through_cones(&mut walls, &cones, c, budget, stop)?;
    let mut out = Vec::new();
    let mut used = vec![false; walls.len()];
    // Two halves of one cylinder (the sheet model cuts a hole's wall as two half faces, each
    // with two sides on one diameter and no seam) close into one round hole before slots
    // pair, so halves of two different holes cannot pair as a slot across the gap between.
    for i in 0..walls.len() {
        for j in (i + 1)..walls.len() {
            budget.charge(1)?;
            if is_used(&used, i) || is_used(&used, j) {
                continue;
            }
            let (Some(a), Some(b)) = (walls.get(i), walls.get(j)) else {
                continue;
            };
            let (Some(oa), Some(ob)) = (a.opening, b.opening) else {
                continue;
            };
            if one_cylinder(a, b, c) && (oa + ob).length() * a.radius <= c {
                mark(&mut used, i);
                mark(&mut used, j);
                out.push(round(a));
            }
        }
    }
    for i in 0..walls.len() {
        for j in (i + 1)..walls.len() {
            budget.charge(1)?;
            if is_used(&used, i) || is_used(&used, j) {
                continue;
            }
            let (Some(a), Some(b)) = (walls.get(i), walls.get(j)) else {
                continue;
            };
            let (Some(oa), Some(ob)) = (a.opening, b.opening) else {
                continue;
            };
            if let Some(bore) = slot_of(a, oa, b, ob, c) {
                mark(&mut used, i);
                mark(&mut used, j);
                out.push(bore);
            }
        }
    }
    for (i, w) in walls.iter().enumerate() {
        if w.opening.is_none() && !is_used(&used, i) {
            out.push(round(w));
        }
    }
    Ok(out)
}

fn is_used(used: &[bool], i: usize) -> bool {
    used.get(i).copied().unwrap_or(true)
}

fn mark(used: &mut [bool], i: usize) {
    if let Some(u) = used.get_mut(i) {
        *u = true;
    }
}

/// The axis a surface's `v` runs along: the frame's z, turned by the sense.
fn axis_of(frame: &Frame, sense: scx_num::frame::Sense) -> UnitVec3 {
    if sense.sign() > 0.0 {
        frame.z()
    } else {
        -frame.z()
    }
}

fn round(w: &Wall) -> Bore {
    Bore {
        point: w.point.to_point(),
        axis: w.axis,
        d: 2.0 * w.radius,
        t: w.t,
        slot: None,
    }
}

/// Whether a face's material lies on the side away from its axis and nothing on the axis side.
fn material_outside(face: Face<'_>, sense: scx_num::frame::Sense) -> bool {
    let away = if sense.sign() > 0.0 {
        FaceSide::Front
    } else {
        FaceSide::Back
    };
    face.shell(away).region().is_solid() && !face.shell(away.other()).region().is_solid()
}

/// The face's extent along its surface's axis: the `v` span of its uv box.
fn extent(face: Face<'_>) -> Result<[f64; 2], BodyqError> {
    let b = face
        .uv_box()
        .ok_or_else(|| BodyqError::Defect(BodyqDefect::NoUvBox { face: face.id() }))?;
    let v = b.y_interval();
    Ok([v.min(), v.max()])
}

/// The wall of one cylinder face: `None` when it does not close round the axis and is not a
/// slot's end (two sides on one diameter, with an opening between them).
fn wall(
    face: Face<'_>,
    frame: &Frame,
    sense: scx_num::frame::Sense,
    radius: f64,
    c: f64,
) -> Result<Option<Wall>, BodyqError> {
    let axis = axis_of(frame, sense);
    let point = frame.origin().to_vec();
    let t = extent(face)?;
    let mut uses: BTreeMap<scx_brep::EdgeId, usize> = BTreeMap::new();
    let mut edges = Vec::new();
    for u in face.uses() {
        let e = u.edge();
        let n = uses.entry(e.id()).or_insert(0);
        *n += 1;
        if *n == 1 {
            edges.push(e);
        }
    }
    let seam = uses.values().any(|n| *n >= 2);
    let mut sides = Vec::new();
    let mut rim = None;
    for e in edges {
        if uses.get(&e.id()).copied().unwrap_or(0) >= 2 {
            continue; // the seam, in its two uses
        }
        // A collapsed edge is no edge of a bore's wall.
        let EdgeShape::Curve { curve, interval } = e.shape() else {
            return Ok(None);
        };
        let (a, b, mid) = (
            curve.point(interval.min()).to_vec(),
            curve.point(interval.max()).to_vec(),
            curve.point(interval.mid()).to_vec(),
        );
        let along = (b - a).dot(*axis);
        let perp = (b - a) - axis * along;
        if perp.length() <= c {
            if along.abs() > c {
                sides.push(a); // an edge along the axis: its end carries its angle
            }
        } else if rim.is_none() {
            // An edge round the axis, between the sides: its middle lies in the covered arc.
            rim = Some(mid);
        }
    }
    if seam {
        return Ok(Some(Wall {
            point,
            axis,
            radius,
            t,
            opening: None,
        }));
    }
    // Exactly two sides on one diameter: the end of an obround slot.
    let (Some(&first), Some(&second), None) = (sides.first(), sides.get(1), sides.get(2)) else {
        return Ok(None);
    };
    let (x, y) = (*frame.x(), *frame.y());
    let angle = |p: Vec3| {
        let d = p - point;
        fmath::atan2(d.dot(y), d.dot(x))
    };
    let spread = (angle(first) - angle(second)).abs();
    let delta = spread.min(core::f64::consts::TAU - spread);
    if (delta - core::f64::consts::PI).abs() * radius > c {
        return Ok(None);
    }
    let Some(mid) = rim else {
        return Ok(None);
    };
    let radial = mid - point - axis * (mid - point).dot(*axis);
    let Some(radial) = radial.normalized() else {
        return Ok(None);
    };
    Ok(Some(Wall {
        point,
        axis,
        radius,
        t,
        opening: Some(-*radial),
    }))
}

/// Whether two axes part by no more than `c` over `reach`, either way round.
pub(crate) fn parallel(a: Vec3, b: Vec3, reach: f64, c: f64) -> bool {
    a.cross(b).length() * reach <= c
}

fn span(t: [f64; 2]) -> f64 {
    t[1] - t[0]
}

/// Whether two partial walls are two halves of one cylinder: one axis line, one radius, one
/// extent.
fn one_cylinder(a: &Wall, b: &Wall, c: f64) -> bool {
    if !parallel(*a.axis, *b.axis, span(a.t).max(span(b.t)), c) || (a.radius - b.radius).abs() > c {
        return false;
    }
    let gap = b.point - a.point;
    let along = gap.dot(*a.axis);
    if (gap - a.axis * along).length() > c {
        return false;
    }
    let (u0, u1) = if b.axis.dot(*a.axis) >= 0.0 {
        (b.t[0], b.t[1])
    } else {
        (-b.t[1], -b.t[0])
    };
    (a.t[0] - (u0 + along)).abs() <= c && (a.t[1] - (u1 + along)).abs() <= c
}

/// The slot two facing partial walls make: its centre, axis, extent and shape.
fn slot_of(a: &Wall, oa: Vec3, b: &Wall, ob: Vec3, c: f64) -> Option<Bore> {
    if !parallel(*a.axis, *b.axis, span(a.t).max(span(b.t)), c) || (a.radius - b.radius).abs() > c {
        return None;
    }
    let gap = b.point - a.point;
    let along = gap.dot(*a.axis);
    let across = gap - a.axis * along;
    let dir = across.normalized()?; // halves on one axis are not a slot
    let (u0, u1) = if b.axis.dot(*a.axis) >= 0.0 {
        (b.t[0], b.t[1])
    } else {
        (-b.t[1], -b.t[0])
    };
    if (a.t[0] - (u0 + along)).abs() > c || (a.t[1] - (u1 + along)).abs() > c {
        return None;
    }
    // The openings face each other along the line between the axes.
    if (oa - *dir).length() * a.radius > c || (ob + *dir).length() * a.radius > c {
        return None;
    }
    Some(Bore {
        point: (a.point + across * 0.5).to_point(),
        axis: a.axis,
        d: 2.0 * a.radius,
        t: a.t,
        slot: Some(Slot {
            length: across.length() + 2.0 * a.radius,
            along: dir,
        }),
    })
}

/// Walls through their cones: a countersink or a chamfer at a wall's end, whose narrow rim is
/// the wall's own circle, extends the wall's extent to the cone's far end.
fn through_cones(
    walls: &mut [Wall],
    cones: &[Cone],
    c: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(), BodyqError> {
    for w in walls.iter_mut() {
        for cone in cones {
            budget.charge(1)?;
            stop.check()?;
            if !parallel(*w.axis, *cone.axis, span(w.t).max(span(cone.t)), c) {
                continue;
            }
            let off = cone.point - w.point;
            let along = off.dot(*w.axis);
            if (off - w.axis * along).length() > c {
                continue;
            }
            let map = |x: f64| (cone.point + cone.axis * x - w.point).dot(*w.axis);
            let (m0, m1) = (map(cone.t[0]), map(cone.t[1]));
            let (lo, hi) = (m0.min(m1), m0.max(m1));
            let radius_at = |t: f64| {
                let s = (w.point + w.axis * t - cone.point).dot(*cone.axis);
                cone.radius_at_origin + s * cone.slope
            };
            // A countersink or chamfer opens out: the cone's far end is no narrower than its
            // rim at the wall. A cone that narrows away (a drill point) is the bore's own
            // closed end, not an opening.
            if (radius_at(w.t[0]) - w.radius).abs() <= c
                && radius_at(lo) >= radius_at(w.t[0]) - c
                && (hi - w.t[0]).abs() <= c
                && lo < w.t[0]
            {
                w.t[0] = lo;
            } else if (radius_at(w.t[1]) - w.radius).abs() <= c
                && radius_at(hi) >= radius_at(w.t[1]) - c
                && (lo - w.t[1]).abs() <= c
                && hi > w.t[1]
            {
                w.t[1] = hi;
            }
        }
    }
    Ok(())
}
