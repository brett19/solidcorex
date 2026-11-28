// Written for solidcorex. Apache-2.0.
//! [`faces_on_one_surface`]: the faces a face continues into across the edges where the body
//! proves one surface goes on (research 74 §46, the pin's `faces_on_one_surface`).
//!
//! **One surface** is ST5's proof (research 74 §42.5), shared through
//! [`scx_surface::plane_bound`]: two faces lie on one surface when their surfaces are `==`
//! (the same kind, the same bits, on one frame: a cylinder split into two faces), or when
//! both are planes whose normals agree within [`CREASE`](scx_surface::CREASE) and each face's
//! box lies within `Resolution::approx` of the other's plane. Quadrics on different frames,
//! and every other pair, are [`NotOneSurface::Kind`]; planes facing each other are
//! [`NotOneSurface::Opposite`].
//!
//! **Across an edge** (the pin's rule, a growth across edges, not a scan of all pairs): from a
//! face, an edge leads to the face on its other side when it is a curve edge of exactly two
//! fins, on two different faces of the same shells, run in opposite senses (so the outward
//! side continues), and the faces are on one surface. A face reaches only the faces its own
//! edges name: the proof never looks at a pair that shares no edge, and each edge is decided
//! once whichever face reaches it first.
//!
//! **Every face is on the seed's surface.** A face is also proved against the surface of the
//! seed its growth came from (one more proof a face, [`NotCrossed::Drift`] when it fails), so
//! a chain of planes each within the resolution of the next but drifting overall stops where
//! the accumulated bound is crossed, as `merge_faces` does by proving each member against the
//! group's plane.
//!
//! The answer is every face reached, the seeds first in the order given and then each face as
//! it is reached, and every edge that did not lead on, with the reason ([`Stopped`]). An edge
//! the caller names in `keep` is not crossed and not reported (a split line the caller wants
//! kept apart).

use std::collections::{BTreeMap, BTreeSet};

use scx_brep::{Brep, EdgeId, EdgeShape, Entity, FaceId};
use scx_num::budget::{Budget, Stop};
use scx_num::extent3d::Extent3d;
use scx_num::units::Length;
use scx_surface::{NotNear, Surface, plane_bound};

use crate::error::{BodyqError, BodyqRefusal};

const UNIT: u64 = 1;

/// Why two faces are not proved to lie on one surface.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NotOneSurface {
    /// The surfaces are not `==` and not both planes: only planes are compared within the
    /// resolution (a cylinder on two frames is not proved one cylinder here).
    Kind,
    /// Two planes whose normals differ by more than [`scx_surface::CREASE`]: faces that
    /// meet at an angle.
    Crease,
    /// Two planes whose normals point against each other.
    Opposite,
    /// Two planes that differ by more than `Resolution::approx` somewhere over a face's box;
    /// the larger of the two bounds.
    Apart(Length),
    /// A face reaches infinity or cannot be boxed, so its extent is not known.
    NoBox,
}

/// Why an edge did not lead from one face to the next.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NotCrossed {
    /// The edge does not have exactly two fins: a face of the other side is not named
    /// (one fin) or not one (three or more, a partition between solids). Carries the count.
    Fins(usize),
    /// The two faces bound different shells.
    Shells,
    /// The two fins run the same way: the faces do not continue each other's outward side.
    Sense,
    /// The faces are not proved to lie on one surface.
    Surface(NotOneSurface),
    /// The faces are on one surface within the resolution (the edge itself would lead on),
    /// but the face across is not on the surface of `seed`, the seed the growth came from:
    /// a chain of planes each within the resolution of the last has drifted. `why` is the
    /// proof against the seed's surface (the same reasons as [`Surface`](Self::Surface)).
    Drift {
        /// The seed whose surface the face across was proved against.
        seed: FaceId,
        /// How the face across is off the seed's surface.
        why: NotOneSurface,
    },
}

/// An edge of a reached face that did not lead on.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Stopped {
    /// The edge.
    pub edge: EdgeId,
    /// The reached face that came to it first.
    pub from: FaceId,
    /// The face on its other side, when the edge has exactly two fins on two faces.
    pub to: Option<FaceId>,
    /// Why it was not crossed.
    pub why: NotCrossed,
}

/// The faces on one surface with the seeds, and the edges that stopped the growth.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct OneSurface {
    /// The seeds (deduplicated, in the order given), then each face as it is reached.
    pub faces: Vec<FaceId>,
    /// Every curve edge of a reached face that did not lead on, once each, in the order
    /// found. An edge on a reached face's seam or collapsed at a pole is not an edge between
    /// two faces and is not listed.
    pub stopped: Vec<Stopped>,
}

/// `seeds` and every face of `body` joined to them across an edge not in `keep`, as the
/// [module](self) says.
///
/// Work is charged to `budget` (one unit for each edge read and one for each proof) and
/// `stop` is checked before each edge; the same body and seeds charge the same work.
///
/// # Errors
///
/// [`BodyqError::Refused`] with [`NoEntity`](BodyqRefusal::NoEntity) for a seed the body
/// does not hold; [`BodyqError::Exhausted`] and [`BodyqError::Cancelled`].
pub fn faces_on_one_surface(
    body: &Brep,
    seeds: &[FaceId],
    keep: &[EdgeId],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<OneSurface, BodyqError> {
    let mut out = OneSurface::default();
    let mut seen: BTreeSet<FaceId> = BTreeSet::new();
    for &f in seeds {
        if body.face(f).is_none() {
            return Err(BodyqError::Refused(BodyqRefusal::NoEntity {
                entity: Entity::Face(f),
            }));
        }
        if seen.insert(f) {
            out.faces.push(f);
        }
    }
    let keep: BTreeSet<EdgeId> = keep.iter().copied().collect();
    let approx = body.resolution().approx().mm();
    let mut boxes: BTreeMap<FaceId, Option<Extent3d>> = BTreeMap::new();
    let mut decided: BTreeSet<EdgeId> = BTreeSet::new();
    // The seed each reached face's growth came from (a seed is its own), and the faces already
    // proved off a seed's surface (so a face two edges lead to is proved once).
    let mut root: BTreeMap<FaceId, FaceId> = out.faces.iter().map(|&f| (f, f)).collect();
    let mut drifted: BTreeMap<(FaceId, FaceId), NotOneSurface> = BTreeMap::new();
    let mut next = 0;
    while let Some(&f) = out.faces.get(next) {
        next += 1;
        let Some(face) = body.face(f) else { continue };
        for u in face.uses() {
            stop.check()?;
            let edge = u.edge();
            let id = edge.id();
            if keep.contains(&id) || !decided.insert(id) {
                continue;
            }
            budget.charge(UNIT)?;
            if !matches!(edge.shape(), EdgeShape::Curve { .. }) {
                continue;
            }
            let fins: Vec<_> = edge.fins().collect();
            let [f0, f1] = match fins.as_slice() {
                [a, b] => [*a, *b],
                other => {
                    out.stopped.push(Stopped {
                        edge: id,
                        from: f,
                        to: None,
                        why: NotCrossed::Fins(other.len()),
                    });
                    continue;
                }
            };
            if f0.face == f1.face {
                continue;
            }
            let to = if f0.face == f { f1.face } else { f0.face };
            let why = if body.face(f0.face).map(|x| x.record().sides)
                != body.face(f1.face).map(|x| x.record().sides)
            {
                Some(NotCrossed::Shells)
            } else if f0.sense == f1.sense {
                Some(NotCrossed::Sense)
            } else {
                budget.charge(UNIT)?;
                one_surface(body, f, to, approx, &mut boxes)
                    .err()
                    .map(NotCrossed::Surface)
            };
            let why = match why {
                None if !seen.contains(&to) => {
                    let seed = root.get(&f).copied().unwrap_or(f);
                    let off = if let Some(&e) = drifted.get(&(seed, to)) {
                        Some(e)
                    } else {
                        budget.charge(UNIT)?;
                        let e = on_seed(body, seed, to, approx, &mut boxes).err();
                        if let Some(e) = e {
                            drifted.insert((seed, to), e);
                        }
                        e
                    };
                    off.map(|why| NotCrossed::Drift { seed, why })
                }
                other => other,
            };
            match why {
                Some(why) => out.stopped.push(Stopped {
                    edge: id,
                    from: f,
                    to: Some(to),
                    why,
                }),
                None => {
                    if seen.insert(to) {
                        out.faces.push(to);
                        let seed = root.get(&f).copied().unwrap_or(f);
                        root.insert(to, seed);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Whether faces `a` and `b` lie on one surface within `approx`.
fn one_surface(
    body: &Brep,
    a: FaceId,
    b: FaceId,
    approx: f64,
    boxes: &mut BTreeMap<FaceId, Option<Extent3d>>,
) -> Result<(), NotOneSurface> {
    let (Some(fa), Some(fb)) = (body.face(a), body.face(b)) else {
        return Err(NotOneSurface::NoBox);
    };
    let (sa, sb) = (fa.surface(), fb.surface());
    if sa == sb {
        return Ok(());
    }
    let (Surface::Plane(pa), Surface::Plane(pb)) = (sa, sb) else {
        return Err(NotOneSurface::Kind);
    };
    let ba = face_box(body, a, boxes).ok_or(NotOneSurface::NoBox)?;
    let bb = face_box(body, b, boxes).ok_or(NotOneSurface::NoBox)?;
    let near = |e: NotNear| match e {
        NotNear::Opposite => NotOneSurface::Opposite,
        _ => NotOneSurface::Crease,
    };
    let dev = plane_bound(pa, pb, &ba)
        .map_err(near)?
        .max(plane_bound(pb, pa, &bb).map_err(near)?);
    if dev <= approx {
        Ok(())
    } else {
        Err(NotOneSurface::Apart(Length::from_mm(dev)))
    }
}

/// Whether face `to` lies on the surface of `seed` within `approx`: one proof, `to`'s box
/// against the seed's plane (the seed's own box is not needed, a surface is unbounded).
fn on_seed(
    body: &Brep,
    seed: FaceId,
    to: FaceId,
    approx: f64,
    boxes: &mut BTreeMap<FaceId, Option<Extent3d>>,
) -> Result<(), NotOneSurface> {
    let (Some(fs), Some(ft)) = (body.face(seed), body.face(to)) else {
        return Err(NotOneSurface::NoBox);
    };
    let (ss, st) = (fs.surface(), ft.surface());
    if ss == st {
        return Ok(());
    }
    let (Surface::Plane(ps), Surface::Plane(pt)) = (ss, st) else {
        return Err(NotOneSurface::Kind);
    };
    let bt = face_box(body, to, boxes).ok_or(NotOneSurface::NoBox)?;
    let dev = plane_bound(pt, ps, &bt).map_err(|e| match e {
        NotNear::Opposite => NotOneSurface::Opposite,
        _ => NotOneSurface::Crease,
    })?;
    if dev <= approx {
        Ok(())
    } else {
        Err(NotOneSurface::Apart(Length::from_mm(dev)))
    }
}

fn face_box(
    body: &Brep,
    f: FaceId,
    boxes: &mut BTreeMap<FaceId, Option<Extent3d>>,
) -> Option<Extent3d> {
    *boxes
        .entry(f)
        .or_insert_with(|| body.face(f).and_then(scx_brep::Face::bounding_box))
}
