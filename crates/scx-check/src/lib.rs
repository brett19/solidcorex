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
//!    ([`Breach::NonManifoldEdge`]); a wire edge has none. A collapsed edge (a pole or an
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
//! **Closed** follows from 2 and 3 with finish: two fins per edge, both faces bounding the
//! same solid and void (finish's radial check), leave no edge of a shell open.
//!
//! **Not tested** (open in 72 §14.22): vertex manifoldness (two solids touching at one
//! vertex pass), and the self-intersection and coincidence of faces, which the full check
//! of 72 step 6 owns.
//!
//! **Reads only.** The body is `&Brep`; the [`Proved`] holds a clone, which shares every
//! table and cell. The work goes to a fresh budget of `cx`'s settings and polls `cx.stop`.

mod error;

use std::collections::BTreeMap;

use scx_brep::op::Cx;
use scx_brep::{Brep, EdgeShape, FaceId, FaceSide, ShellId};
use scx_mass::{MassError, MassRefusal, Sign};
use scx_num::budget::{Budget, Stop};

pub use error::{Breach, ContractError};

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

/// Proves `body` keeps the contract (see the crate doc) and returns it as [`Proved`].
///
/// # Errors
///
/// [`ContractError::Broken`] naming the first test the body fails, at its lowest id;
/// [`ContractError::Undecided`] for a face whose region the loops' area cannot sign;
/// [`ContractError::Mass`] when the certified sign cannot be finished (a sign still
/// undecided at its cap, the budget, the stop flag).
pub fn contract(body: &Brep, cx: &Cx<'_>) -> Result<Proved, ContractError> {
    structure(body).map_err(ContractError::Broken)?;
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

/// Tests 1 to 3: the records alone.
fn structure(body: &Brep) -> Result<(), Breach> {
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
        if fins != 2 && !matches!(e.shape(), EdgeShape::Collapsed) {
            return Err(Breach::NonManifoldEdge { edge: e.id(), fins });
        }
    }
    for s in body.shells() {
        if let Some(v) = s.lone_vertices().next() {
            return Err(Breach::LoneVertex { vertex: v.id() });
        }
    }
    Ok(())
}
