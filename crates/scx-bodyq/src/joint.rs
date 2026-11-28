// Written for solidcorex. Apache-2.0.
//! At a vertex where edges meet (research 74 §34, BW11): the direction an edge leaves it
//! in ([`leaving`]), how far one edge turns from running straight on into another there
//! ([`turn_at`]), whether an edge is convex there ([`convex_at`]), and the angle between two
//! planar directions where two curves of a path meet ([`joint_turn`]).
//!
//! | pin (`query::edge_turn`) | here |
//! |---|---|
//! | `leaving`, `leaving_derivative`: `Option<Vec3>` | [`leaving`]: [`Leaving`], the cases named |
//! | `turn_at`: `Option<f64>` | [`turn_at`]: [`Turn`] |
//! | `convex_at`: `Option<bool>`, `None` for tangent faces, undecided and no material alike | [`convex_at`]: [`Convexity`], each case its own |
//! | `joint_turn`: a refusal by `Vec2::angle_between`'s absolute zero | [`joint_turn`]: a direction of no length is [`NoDirection`](BodyqRefusal::NoDirection) at any scale |
//! | `turn_to_parallel` | BW9's measure map |
//!
//! **Direction and turn** are evaluations, not proofs: the curve's first derivative on the
//! side of the vertex, so a NURBS curve kinked at a knot reads its own end, and the angle is
//! `atan2(|a x b|, a . b)`, good to rounding over the whole range (no `acos` near 0 or pi).
//!
//! **Convexity** is a proof. Each face's normal into the material is enclosed by the
//! closed-form normal field of `tangent_all_along` (BW4), the edge's direction by the curve's
//! jet over the end parameter, and the answer comes only when the sign of the enclosure is
//! the same on both faces' reading; faces within the caller's smooth angle of each other have
//! no convexity ([`Convexity::Tangent`]).

use scx_brep::{Brep, EdgeId, EdgeShape, FaceId, FaceSide, VertexId};
use scx_curve::Curve;
use scx_num::fmath;
use scx_num::interval::Ivl;
use scx_num::vector3d::Vec3;
use scx_nurbs::Approach;

use crate::along::Field;
use crate::error::{BodyqError, BodyqRefusal, BodyqUnsupported};
use scx_brep::Entity;

/// The angle in radians, in `[0, pi]`, between the planar directions `before` (the first
/// curve's end tangent) and `after` (the next curve's start tangent): 0 where a path runs
/// straight on, pi where it doubles back. The directions need not be unit.
///
/// # Errors
///
/// Refused: [`NotFinite`](BodyqRefusal::NotFinite) for a coordinate that is not, and
/// [`NoDirection`](BodyqRefusal::NoDirection) for a direction of no length, however small
/// its coordinates.
pub fn joint_turn(before: [f64; 2], after: [f64; 2]) -> Result<f64, BodyqError> {
    let scaled = |d: [f64; 2]| -> Result<[f64; 2], BodyqError> {
        if !d.iter().all(|c| c.is_finite()) {
            return Err(BodyqError::Refused(BodyqRefusal::NotFinite));
        }
        let big = d[0].abs().max(d[1].abs());
        if big == 0.0 {
            return Err(BodyqError::Refused(BodyqRefusal::NoDirection));
        }
        Ok([d[0] / big, d[1] / big])
    };
    let (a, b) = (scaled(before)?, scaled(after)?);
    let cross = a[0] * b[1] - a[1] * b[0];
    let dot = a[0] * b[0] + a[1] * b[1];
    Ok(fmath::atan2(cross.abs(), dot))
}

/// Why an edge has no direction at its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Missing {
    /// Both ends are the vertex: which end is meant is not said.
    Closed,
    /// The edge is collapsed to a point.
    Collapsed,
    /// The curve's tangent there has no length.
    Stationary,
}

/// The direction an edge leaves a vertex in.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Leaving {
    /// The curve's unit tangent from inside the edge, turned to point away from the vertex.
    Direction(Vec3),
    /// There is none.
    Missing(Missing),
}

/// An edge's end at a vertex: the curve, the end's parameter, and whether it is the start.
struct EndAt<'a> {
    curve: &'a Curve,
    t: f64,
    at_start: bool,
}

/// The end `vertex` is of `edge`; what is not one is `Err(Ok(why))` where the edge has no
/// direction there, `Err(Err(_))` where the request is refused.
fn end_of(
    body: &Brep,
    edge: EdgeId,
    vertex: VertexId,
) -> Result<EndAt<'_>, Result<Missing, BodyqError>> {
    let e = body
        .edge(edge)
        .ok_or(Err(BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: Entity::Edge(edge),
        })))?;
    let ends = e.ends();
    if !ends.contains(&vertex) {
        return Err(Err(BodyqError::Refused(BodyqRefusal::NotAnEnd {
            edge,
            vertex,
        })));
    }
    let EdgeShape::Curve { curve, interval } = e.shape() else {
        return Err(Ok(Missing::Collapsed));
    };
    if ends[0] == ends[1] {
        return Err(Ok(Missing::Closed));
    }
    let at_start = ends[0] == vertex;
    let t = if at_start {
        interval.min()
    } else {
        interval.max()
    };
    Ok(EndAt { curve, t, at_start })
}

/// The unit direction edge `edge` leaves its end `vertex` in: its curve's tangent there from
/// inside the edge (the span above the start, below the end), turned to point away from
/// `vertex`.
///
/// # Errors
///
/// Refused for an edge the body does not have or a vertex that is not an end of it.
pub fn leaving(body: &Brep, edge: EdgeId, vertex: VertexId) -> Result<Leaving, BodyqError> {
    let EndAt { curve, t, at_start } = match end_of(body, edge, vertex) {
        Ok(x) => x,
        Err(Ok(why)) => return Ok(Leaving::Missing(why)),
        Err(Err(e)) => return Err(e),
    };
    let side = if at_start {
        Approach::Above
    } else {
        Approach::Below
    };
    let [d] = curve.jet::<1>(t, side).d;
    let d = if at_start { d } else { -d };
    Ok(match d.length_and_unit() {
        Some((_, u)) => Leaving::Direction(u.as_vec()),
        None => Leaving::Missing(Missing::Stationary),
    })
}

/// How far two edges turn at a shared vertex.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Turn {
    /// The angle in radians, in `[0, pi]`, between the direction `e` arrives at the vertex in
    /// and the one `f` leaves it in: 0 where `f` continues `e` tangentially.
    Angle(f64),
    /// `edge`, the first of the two without a direction, has none there.
    Undefined {
        /// The edge.
        edge: EdgeId,
        /// Why.
        why: Missing,
    },
}

/// How far edge `f` turns at their shared end `vertex` from running straight on from edge `e`.
///
/// # Errors
///
/// As [`leaving`], for either edge.
pub fn turn_at(body: &Brep, e: EdgeId, f: EdgeId, vertex: VertexId) -> Result<Turn, BodyqError> {
    let mut dirs = [Vec3::ZERO; 2];
    for (slot, edge) in dirs.iter_mut().zip([e, f]) {
        match leaving(body, edge, vertex)? {
            Leaving::Direction(d) => *slot = d,
            Leaving::Missing(why) => return Ok(Turn::Undefined { edge, why }),
        }
    }
    let arrives = -dirs[0];
    let leaves = dirs[1];
    Ok(Turn::Angle(fmath::atan2(
        arrives.cross(leaves).length(),
        arrives.dot(leaves),
    )))
}

/// Whether an edge is convex at an end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Convexity {
    /// The faces meet at less than 180 degrees inside the material, proved.
    Convex,
    /// The faces meet at more than 180 degrees inside the material, proved.
    Concave,
    /// The faces' normals into the material are proved within the smooth angle of each
    /// other: tangent faces have no convexity.
    Tangent,
    /// The faces part by more than the smooth angle (or may), but the side is not proved:
    /// the enclosures hold both signs.
    Undecided,
    /// A face of the edge does not bound material on exactly one side (both sides solid, or
    /// neither): a sheet or a face inside a solid has no inside corner.
    NoMaterial {
        /// The face.
        face: FaceId,
    },
    /// The edge's end is not told from its other (both are the vertex), or it is collapsed.
    NoEnd(Missing),
}

/// A face at the edge's end: its unit normal into the material and the direction from the
/// edge into the face, enclosed.
type Reading = (Vec3<Ivl>, Vec3<Ivl>);

/// The tangent of `a`, enclosed (the library's `tan` is within a few ulps).
fn tan_ivl(a: f64) -> Ivl {
    let t = fmath::tan(a);
    let mut k = Ivl::outward_ulp(t, t);
    for _ in 0..3 {
        k = Ivl::outward_ulp(k.lo, k.hi);
    }
    k
}

fn neg(v: Vec3<Ivl>) -> Vec3<Ivl> {
    let z = Ivl::from(0.0);
    Vec3::new(z - v.x, z - v.y, z - v.z)
}

/// Whether edge `edge` is convex at its end `vertex`: the faces on its two sides meet at less
/// than 180 degrees inside the material there.
///
/// The side is `w_b . m_a`, with `m_a` face a's unit normal into the material and `w_b` the
/// direction from the edge into face b square to it (face b's natural normal crossed with the
/// edge's direction, turned by its use's sense: a face lies to the left of its loop). Positive
/// is convex. Both readings (`w_b . m_a` and `w_a . m_b`) are enclosed and must not disagree.
///
/// `smooth` (radians, in `(0, pi / 2)`) is the caller's angle below which two faces are
/// tangent: its normals into the material within it have no convexity.
///
/// # Errors
///
/// Refused for an edge the body does not have, a vertex that is not an end of it, a smooth
/// angle out of range ([`Tolerance`](BodyqRefusal::Tolerance)), an edge with fewer than two
/// faces. Unsupported for more than two faces or a NURBS face.
pub fn convex_at(
    body: &Brep,
    edge: EdgeId,
    vertex: VertexId,
    smooth: f64,
) -> Result<Convexity, BodyqError> {
    if !(smooth > 0.0 && smooth < core::f64::consts::FRAC_PI_2) {
        return Err(BodyqError::Refused(BodyqRefusal::Tolerance));
    }
    let EndAt { curve, t, at_start } = match end_of(body, edge, vertex) {
        Ok(x) => x,
        Err(Ok(why)) => return Ok(Convexity::NoEnd(why)),
        Err(Err(e)) => return Err(e),
    };
    let at = Entity::Edge(edge);
    let e = body
        .edge(edge)
        .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity { entity: at }))?;
    let fins: Vec<_> = e.fins().collect();
    let [fa, fb] = match fins.as_slice() {
        [a, b] => [*a, *b],
        [] | [_] => return Err(BodyqError::Refused(BodyqRefusal::OneSided { edge })),
        _ => {
            return Err(BodyqError::Unsupported(BodyqUnsupported::NonManifold {
                at,
            }));
        }
    };
    let side = if at_start {
        Approach::Above
    } else {
        Approach::Below
    };
    let jet = curve.jet_over::<Ivl, 1>(Ivl::from(t), side);
    let p = Vec3::new(jet.point.x, jet.point.y, jet.point.z);
    let tangent = jet.d[0];

    // Per face: its unit normal into the material, and the direction from the edge into the
    // face. A face with no material side ends the answer.
    let reading = |fin: scx_brep::Fin| -> Result<Result<Reading, Convexity>, BodyqError> {
        let face = body
            .face(fin.face)
            .ok_or(BodyqError::Refused(BodyqRefusal::NoEntity {
                entity: Entity::Face(fin.face),
            }))?;
        let (front, back) = (
            face.shell(FaceSide::Front).region().is_solid(),
            face.shell(FaceSide::Back).region().is_solid(),
        );
        if front == back {
            return Ok(Err(Convexity::NoMaterial { face: fin.face }));
        }
        let Some((field, sense)) = Field::of(face.surface()) else {
            return Err(BodyqError::Unsupported(BodyqUnsupported::Nurbs {
                at: Entity::Face(fin.face),
            }));
        };
        let Some(foot) = field.foot::<Ivl>(p) else {
            return Ok(Err(Convexity::Undecided));
        };
        let natural = foot.n0 * Ivl::from(sense);
        let into = if front { natural } else { neg(natural) };
        let run = if fin.sense == scx_num::frame::Sense::Forward {
            tangent
        } else {
            neg(tangent)
        };
        Ok(Ok((into, natural.cross(run))))
    };
    let (m_a, w_a) = match reading(fa)? {
        Ok(x) => x,
        Err(c) => return Ok(c),
    };
    let (m_b, w_b) = match reading(fb)? {
        Ok(x) => x,
        Err(c) => return Ok(c),
    };

    // Within the smooth angle of each other: tangent, no convexity.
    let (c, d) = (m_a.cross(m_b).norm(), m_a.dot(m_b));
    let tan = tan_ivl(smooth);
    let parted = d.hi <= 0.0 || c.lo > (tan * Ivl::from(d.hi)).hi;
    let within = d.lo > 0.0 && c.hi < (tan * Ivl::from(d.lo)).lo;
    if within {
        return Ok(Convexity::Tangent);
    }
    if !parted {
        return Ok(Convexity::Undecided);
    }
    let (ab, ba) = (w_b.dot(m_a), w_a.dot(m_b));
    let verdict = |x: Ivl| {
        if x.lo > 0.0 {
            Some(true)
        } else if x.hi < 0.0 {
            Some(false)
        } else {
            None
        }
    };
    Ok(match (verdict(ab), verdict(ba)) {
        (Some(x), Some(y)) if x != y => Convexity::Undecided,
        (Some(true), _) | (_, Some(true)) => Convexity::Convex,
        (Some(false), _) | (_, Some(false)) => Convexity::Concave,
        (None, None) => Convexity::Undecided,
    })
}
