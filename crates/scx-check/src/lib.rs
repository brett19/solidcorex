// Written for solidcorex. Apache-2.0.
//! The body contract (research 72 §7, task CK1): [`contract`] proves a finished body is a
//! manifold, closed, outward solid and returns it as [`Proved`], the only way to build one.
//!
//! An operation does not call it; the kernel's edge does, on what an operation returns or a
//! caller hands in (04 row 106). What the contract adds to `Draft::finish`: finish proves
//! the records are consistent (every fin has its use, each shell is connected, the sides of
//! a face agree with its edges' radial order) and accepts sheets, wires, non-manifold
//! edges, lone vertices and faces with no outer loop. The contract refuses all of those
//! and proves the orientation by the certified sign, which no estimate can contradict.
//!
//! **The tests**, in this order; the first that fails is named, at its lowest id:
//!
//! 1. **Not empty.** At least one face ([`Breach::Empty`]).
//! 2. **Every face bounds material.** One side of each face is a solid region and the other
//!    is not ([`Breach::FaceNotBounding`]): a sheet, a face inside a solid, or a face
//!    between two solids (a stack) is refused.
//! 3. **Manifold.** Every edge with a curve has exactly two fins
//!    ([`Breach::NonManifoldEdge`]); a wire edge has none. [`Contacts::Radial`] lets an edge
//!    of an even number of fins past if it passes the radial test below. A collapsed edge (a pole or an
//!    apex) is a point and bounds nothing, so it is not counted: finish proves each of its
//!    uses lies on its one vertex. No shell holds a lone vertex ([`Breach::LoneVertex`]).
//! 4. **Every face is bounded and proved.** Its region's orientation is decided by the
//!    loops' enclosed signed area ([`ContractError::Undecided`] when it is not), and it has
//!    a box ([`Breach::FaceUnbounded`]: a first loop wound the wrong way on a plane or a
//!    cylinder gives the face the whole unbounded rest of the surface).
//! 5. **Outward.** `scx_mass::outward`'s rule: each solid region has a shell whose
//!    certified volume sign (`scx_mass::volume_sign`) is positive and no shell of the
//!    outside is positive ([`Breach::NotOutward`]). The sign is proved, so a contradiction
//!    smaller than the volume estimate's error, which `scx_mass::volume`'s guard lets
//!    through, is refused.
//!
//! **Each mirror pair is certified once.** A shell's volume is the flux through its faces,
//! each counted `+1` from its back and `-1` from its front. Two shells whose weighted faces
//! are each other's negation (the two sides of one closed surface: a body's skin seen from
//! the solid and from the outside, a cavity's lining from the solid and from the void) have
//! volumes that are exact negatives, so one certificate signs both. The pairs are found from
//! the weights themselves, not assumed; a shell with no mirror is certified on its own. A
//! certificate is about half of what `scx_mass::outward`, which signs every shell, spends.
//!
//! **Radial edges** ([`Contacts`], 72 §14.37). Where a body meets itself along an edge
//! (two blocks sharing a corner line, D5), the edge has four or more fins. The default
//! [`Contacts::Refused`] refuses it; [`Contacts::Radial`] admits it when, at the middle of
//! the edge's interval with `T` the tangent, each fin's face leaves the edge in
//! `d = sense (N x T)` (`N` the face's normal at the use's pcurve), and
//!
//! - the count is even,
//! - the fins in the edge's order have strictly increasing counter-clockwise angles of `d`
//!   about `T` from the first, each gap and the wrap-round gap above `1e-9` rad (faces
//!   leaving in one direction are tangent, and the edge's order cannot be read from one
//!   sample; they are refused), and
//! - the region after each fin is solid and void by turns.
//!
//! Anything else is [`Breach::NonManifoldEdge`] (an odd count, or a count under two) or
//! [`Breach::EdgeNotRadial`] (the angles or the sectors), naming the edge. Only edges of
//! more than two fins are tested, so a body without one costs what it did.
//!
//! **Closed** follows from 2 and 3 with finish: two fins per edge, both faces bounding the
//! same solid and void (finish's radial check), leave no edge of a shell open.
//!
//! **Vertices** (CF1, research 74 §18). A body may touch itself at a vertex with no shared
//! edge (two blocks at a corner): the Boolean builds such a union on purpose (BO7), so the
//! default [`Contacts::Refused`] and [`Contacts::Radial`] admit it. [`Contacts::Manifold`]
//! refuses it: after the edges, every vertex's faces must be in one piece
//! ([`Breach::VertexNotManifold`]). It is the pass [`check`] runs (`structure`'s
//! `pinched`), reported there as a finding.
//!
//! **Not tested** here: the self-intersection and coincidence of faces, which [`check`]
//! owns.
//!
//! **Reads only.** The body is `&Brep`; the [`Proved`] holds a clone, which shares every
//! table and cell. The work goes to a fresh budget of `cx`'s settings and polls `cx.stop`.

mod error;
mod findings;
mod structure;

use std::collections::BTreeMap;

use scx_brep::op::Cx;
use scx_brep::{Brep, Edge, EdgeShape, FaceId, FaceSide, ShellId, UseGeom};
use scx_mass::{MassError, MassRefusal, Sign};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::Sense;
use scx_num::vector3d::Vec3;
use scx_nurbs::Approach;

pub use error::{Breach, CheckError, ContractError};
pub use findings::{Finding, Findings, Scope, Spent, Undecided, check};
pub use structure::{Class, Pinch};

/// What [`contract`] does with an edge of more than two fins (a body meeting itself along
/// a line, D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum Contacts {
    /// Refuse it ([`Breach::NonManifoldEdge`]).
    #[default]
    Refused,
    /// Admit an edge whose fins are in strict radial order with solid and void sectors by
    /// turns (see the crate doc); refuse any other.
    Radial,
    /// As [`Contacts::Refused`], and refuse a vertex whose faces are in more than one piece
    /// (two solids touching at a corner): the body is a manifold at every vertex too.
    Manifold,
}

/// A body that [`contract`] proved: manifold, closed and outward. Only `contract` builds it.
#[derive(Debug, Clone)]
pub struct Proved(Brep);

impl Proved {
    /// The body.
    #[must_use]
    pub const fn body(&self) -> &Brep {
        &self.0
    }

    /// The body, given up.
    #[must_use]
    pub fn into_body(self) -> Brep {
        self.0
    }
}

impl AsRef<Brep> for Proved {
    fn as_ref(&self) -> &Brep {
        &self.0
    }
}

/// Proves `body` keeps the contract (see the crate doc) and returns it as [`Proved`];
/// `contacts` says whether an edge of more than two fins may pass.
///
/// # Errors
///
/// [`ContractError::Broken`] naming the first test the body fails, at its lowest id;
/// [`ContractError::Undecided`] for a face whose region the loops' area cannot sign;
/// [`ContractError::Mass`] when the certified sign cannot be finished (a sign still
/// undecided at its cap, the budget, the stop flag).
pub fn contract(body: &Brep, cx: &Cx<'_>, contacts: Contacts) -> Result<Proved, ContractError> {
    structure(body, contacts).map_err(ContractError::Broken)?;
    for f in body.faces() {
        if !f.is_region_proved() {
            return Err(ContractError::Undecided { face: f.id() });
        }
        if f.bounding_box().is_none() {
            return Err(ContractError::Broken(Breach::FaceUnbounded {
                face: f.id(),
            }));
        }
    }
    let mut budget = cx.budget();
    outward(body, &mut budget, cx.stop).map_err(|e| match e {
        MassError::Refused(MassRefusal::NotOutward { shell }) => {
            ContractError::Broken(Breach::NotOutward { shell })
        }
        e => ContractError::Mass(e),
    })?;
    Ok(Proved(body.clone()))
}

/// Test 5, `scx_mass::outward`'s rule in its order (regions, then their shells, by id), with
/// each shell's sign certified once per mirror pair (see the crate doc).
fn outward(body: &Brep, budget: &mut Budget, stop: &Stop) -> Result<(), MassError> {
    let mut signs = Signs::new(body);
    for region in body.regions() {
        let mut positive = false;
        let mut first = None;
        for s in region.shells() {
            first = first.or_else(|| Some(s.id()));
            if signs.of(s.id(), budget, stop)? == Sign::Positive {
                if region.is_outside() {
                    return Err(MassError::Refused(MassRefusal::NotOutward {
                        shell: s.id(),
                    }));
                }
                positive = true;
            }
        }
        if !region.is_outside()
            && !positive
            && let Some(shell) = first
        {
            return Err(MassError::Refused(MassRefusal::NotOutward { shell }));
        }
    }
    Ok(())
}

/// A shell's faces with their weights: `+1` from the back, `-1` from the front, a face with
/// both sides in the shell cancelling (as `scx_mass` weighs a shell's volume).
type Weights = BTreeMap<FaceId, i32>;

/// The certified signs asked so far, and each shell's mirror.
struct Signs<'a> {
    body: &'a Brep,
    mirror: BTreeMap<ShellId, ShellId>,
    known: BTreeMap<ShellId, Sign>,
}

impl<'a> Signs<'a> {
    /// Pairs each shell with the shell whose weights are its own negated, if there is one.
    fn new(body: &'a Brep) -> Self {
        let weights: BTreeMap<ShellId, Weights> = body
            .shells()
            .map(|s| {
                let mut w = Weights::new();
                for (f, side) in s.faces() {
                    *w.entry(f.id()).or_insert(0) += match side {
                        FaceSide::Back => 1,
                        FaceSide::Front => -1,
                    };
                }
                w.retain(|_, x| *x != 0);
                (s.id(), w)
            })
            .collect();
        let mut mirror = BTreeMap::new();
        for (&id, w) in &weights {
            // The candidate: the shell on the other side of the shell's first face.
            let Some(f) = w.keys().next().and_then(|&f| body.face(f)) else {
                continue;
            };
            let other = [FaceSide::Back, FaceSide::Front]
                .map(|side| f.shell(side).id())
                .into_iter()
                .find(|&o| o != id);
            if let Some(o) = other
                && let Some(ow) = weights.get(&o)
                && ow.len() == w.len()
                && w.iter().all(|(face, x)| ow.get(face) == Some(&-x))
            {
                mirror.insert(id, o);
            }
        }
        Self {
            body,
            mirror,
            known: BTreeMap::new(),
        }
    }

    /// The shell's certified sign: its mirror's negated when that is known, else certified.
    fn of(&mut self, shell: ShellId, budget: &mut Budget, stop: &Stop) -> Result<Sign, MassError> {
        if let Some(&s) = self.known.get(&shell) {
            return Ok(s);
        }
        if let Some(&s) = self.mirror.get(&shell).and_then(|m| self.known.get(m)) {
            return Ok(negated(s));
        }
        let s = scx_mass::volume_sign(self.body, shell, budget, stop)?;
        self.known.insert(shell, s);
        Ok(s)
    }
}

const fn negated(s: Sign) -> Sign {
    match s {
        Sign::Positive => Sign::Negative,
        Sign::Negative => Sign::Positive,
        Sign::Zero => Sign::Zero,
    }
}

/// Tests 1 to 3 and the vertex test: the records alone.
fn structure(body: &Brep, contacts: Contacts) -> Result<(), Breach> {
    if body.faces().len() == 0 {
        return Err(Breach::Empty);
    }
    for f in body.faces() {
        let solid = |side| f.shell(side).region().is_solid();
        if solid(FaceSide::Front) == solid(FaceSide::Back) {
            return Err(Breach::FaceNotBounding { face: f.id() });
        }
    }
    for e in body.edges() {
        let fins = e.fins().len();
        if fins == 2 || matches!(e.shape(), EdgeShape::Collapsed) {
            continue;
        }
        if contacts == Contacts::Radial && fins > 2 && fins % 2 == 0 {
            if !radial(e) {
                return Err(Breach::EdgeNotRadial { edge: e.id() });
            }
        } else {
            return Err(Breach::NonManifoldEdge { edge: e.id(), fins });
        }
    }
    for s in body.shells() {
        if let Some(v) = s.lone_vertices().next() {
            return Err(Breach::LoneVertex { vertex: v.id() });
        }
    }
    if contacts == Contacts::Manifold
        && let Some(p) = structure::pinched(body).first()
    {
        return Err(Breach::VertexNotManifold {
            vertex: p.vertex,
            pieces: p.pieces,
        });
    }
    Ok(())
}

/// Two directions closer than this, in radians, are one: the faces are tangent.
const TIED: f64 = 1e-9;

/// Whether the fins of `edge` (an even number, over two) leave it in strict radial order
/// with solid and void sectors by turns (see the crate doc).
fn radial(edge: Edge<'_>) -> bool {
    let EdgeShape::Curve { curve, interval } = edge.shape() else {
        return false;
    };
    let s = 0.5 * (interval.min() + interval.max());
    let Some(tan) = curve.tangent_dir(s, Approach::Above) else {
        return false;
    };
    let tan = tan.as_vec();
    let mut fins = Vec::with_capacity(edge.fins().len());
    for u in edge.uses() {
        let UseGeom::Pcurve { curve: pc, .. } = u.geom() else {
            return false;
        };
        let Some(n) = u.face().surface().normal(pc.point(s), [Approach::Above; 2]) else {
            return false;
        };
        let Some(d) = (n.as_vec().cross(tan) * u.sense().sign()).normalized() else {
            return false;
        };
        // The sector after the fin: the front of a forward use, the back of a reversed one.
        let side = match u.sense() {
            Sense::Forward => FaceSide::Front,
            Sense::Reversed => FaceSide::Back,
        };
        fins.push((d.as_vec(), u.face().shell(side).region().is_solid()));
    }
    is_radial(tan, &fins)
}

/// The test of the crate doc on each fin's direction `d` and whether the sector after it is
/// solid, in the edge's order, about the unit tangent `tan`.
fn is_radial(tan: Vec3, fins: &[(Vec3, bool)]) -> bool {
    let Some(&(e1, _)) = fins.first() else {
        return false;
    };
    let e2 = tan.cross(e1);
    let mut last = 0.0;
    for &(d, _) in fins.iter().skip(1) {
        let a = fmath::atan2(d.dot(e2), d.dot(e1));
        let a = if a < 0.0 {
            a + core::f64::consts::TAU
        } else {
            a
        };
        // A direction behind the first is a second turn; one at or before the last is out
        // of order; one within `TIED` of its neighbour is tangent to it.
        if !(a - last > TIED && core::f64::consts::TAU - a > TIED) {
            return false;
        }
        last = a;
    }
    fins.windows(2)
        .all(|w| matches!(w, [(_, x), (_, y)] if x != y))
        && matches!((fins.first(), fins.last()), (Some((_, x)), Some((_, y))) if x != y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Vec3 = Vec3::new(0.0, 0.0, 1.0);

    fn dir(angle: f64) -> Vec3 {
        Vec3::new(fmath::cos(angle), fmath::sin(angle), 0.0)
    }

    fn fins(angles: &[f64]) -> Vec<(Vec3, bool)> {
        angles
            .iter()
            .enumerate()
            .map(|(i, &a)| (dir(a), i % 2 == 0))
            .collect()
    }

    const Q: f64 = core::f64::consts::FRAC_PI_2;

    #[test]
    fn four_fins_in_order_with_sectors_by_turns_pass() {
        assert!(is_radial(
            T,
            &fins(&[0.3, 0.3 + Q, 0.3 + 2.0 * Q, 0.3 + 3.0 * Q])
        ));
        // Any rotation of the list, and a start behind the wrap.
        assert!(is_radial(
            T,
            &fins(&[5.0, 5.0 + Q, 5.0 + 2.0 * Q, 5.0 + 3.0 * Q])
        ));
        assert!(is_radial(T, &fins(&[0.0, 0.5, 3.0, 3.5, 4.0, 5.0])));
    }

    #[test]
    fn a_swapped_or_reversed_order_is_refused() {
        assert!(!is_radial(T, &fins(&[0.0, 2.0 * Q, Q, 3.0 * Q])));
        assert!(!is_radial(T, &fins(&[0.0, 3.0 * Q, 2.0 * Q, Q])));
        assert!(!is_radial(T, &fins(&[0.0, 0.5, 3.0, 2.0, 4.0, 5.0])));
    }

    #[test]
    fn tangent_fins_are_refused() {
        assert!(!is_radial(T, &fins(&[0.0, Q, Q + 1e-11, 3.0 * Q])));
        assert!(!is_radial(
            T,
            &fins(&[0.0, Q, 2.0 * Q, core::f64::consts::TAU - 1e-11])
        ));
        assert!(!is_radial(T, &fins(&[0.0, 0.0, 2.0 * Q, 3.0 * Q])));
    }

    #[test]
    fn sectors_that_do_not_alternate_are_refused() {
        let mut f = fins(&[0.0, Q, 2.0 * Q, 3.0 * Q]);
        f[1].1 = true;
        assert!(!is_radial(T, &f));
        // Alternating over the list but not across the wrap: an odd count.
        assert!(!is_radial(T, &fins(&[0.0, Q, 2.0 * Q])));
    }
}
