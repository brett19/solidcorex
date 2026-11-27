// Written for solidcorex. Apache-2.0.
//! The volume's guard against a body set inside out (research 72 §14.16).
//!
//! The estimate counts each face with the region sign (`[back solid] - [front solid]`),
//! which trusts the regions' labels. The certified sign ([`crate::outward`]) checks them
//! against the geometry. Running it on every volume would cost an enclosure per shell, so
//! the estimate is read per shell first: each face's flux is added to the shell behind it
//! and taken from the shell in front, and a shell whose sum contradicts its region by more
//! than its error (the outside's shell surely positive, or a region whose every shell is
//! surely negative) calls the certificate. The certificate then decides: the body is not
//! outward (a refusal), or the estimate missed (a defect). Never a silent pick.

use std::collections::BTreeMap;

use scx_brep::{Brep, Face, FaceSide, ShellId};
use scx_num::budget::{Budget, Stop};

use crate::enclose::outward;
use crate::error::{MassDefect, MassError};

/// The estimate's volume per shell: its sum and summed error.
#[derive(Default)]
pub(crate) struct Shells {
    sums: BTreeMap<ShellId, (f64, f64)>,
    /// Shells with a face the estimate skipped (between two solids or two voids): their
    /// sums are not their volumes.
    partial: Vec<ShellId>,
}

impl Shells {
    /// Adds a face's volume flux (about any reference: a closed shell's sum does not
    /// depend on it) and its error.
    pub(crate) fn add(&mut self, face: Face<'_>, flux: f64, error: f64) {
        let (back, front) = (
            face.shell(FaceSide::Back).id(),
            face.shell(FaceSide::Front).id(),
        );
        if back == front {
            return;
        }
        for (s, w) in [(back, 1.0), (front, -1.0)] {
            let e = self.sums.entry(s).or_insert((0.0, 0.0));
            e.0 += w * flux;
            e.1 += error;
        }
    }

    /// Marks the shells of a face the estimate skipped.
    pub(crate) fn skip(&mut self, face: Face<'_>) {
        self.partial.push(face.shell(FaceSide::Back).id());
        self.partial.push(face.shell(FaceSide::Front).id());
    }

    /// The first shell whose estimate contradicts its region, if any.
    fn contradicted(&self, body: &Brep) -> Option<ShellId> {
        let whole = |s: ShellId| !self.partial.contains(&s);
        let sum = |s: ShellId| self.sums.get(&s).copied();
        for region in body.regions() {
            let shells: Vec<ShellId> = region
                .shells()
                .map(scx_brep::Shell::id)
                .filter(|&s| whole(s))
                .collect();
            if region.is_outside() {
                if let Some(&s) = shells
                    .iter()
                    .find(|&&s| sum(s).is_some_and(|(v, e)| v - e > 0.0))
                {
                    return Some(s);
                }
            } else if region.shells().count() == shells.len()
                && let Some(&first) = shells.first()
                && shells
                    .iter()
                    .all(|&s| sum(s).is_some_and(|(v, e)| v + e < 0.0))
            {
                return Some(first);
            }
        }
        None
    }

    /// Passes a body whose estimate agrees with its regions; otherwise certifies: the
    /// certificate's refusal ([`MassRefusal::NotOutward`](crate::MassRefusal::NotOutward)),
    /// or [`MassDefect::EstimateMissed`] when the body is outward after all.
    pub(crate) fn check(
        &self,
        body: &Brep,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<(), MassError> {
        let Some(shell) = self.contradicted(body) else {
            return Ok(());
        };
        outward(body, budget, stop)?;
        Err(MassError::Defect(MassDefect::EstimateMissed { shell }))
    }
}
