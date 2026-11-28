// Written for solidcorex. Apache-2.0.
//! Certified repair of bodies (research 75, tasks RP1-RP8): [`heal`] with items per defect;
//! later, sewing within a gap, hole patches and analytic rebuild, none needing Booleans.
//!
//! - [`heal`] does each [`Item`] only when every edit it makes is certified within the
//!   accuracies the body records, so the result is valid by construction and no accuracy
//!   grows. RP1 builds the short-edge [`Cluster`]: vertices within coincidence joined by
//!   edges, merged at one point, on lines and planes exactly. RP2 re-ends NURBS edges by
//!   their end control points and cuts their [`EmptyPieces`], certified on the check's gaps.
//! - Items are in order and an [`OnItemFailure`] says what a failing one does (01 §5.2).
//! - Errors are [`RepairError`] in 01 §5.1's shape; an item's cause is a [`HealCause`].

mod error;
mod heal;

pub use error::{HealCause, RepairCause, RepairDefect, RepairError};
pub use heal::{Cluster, EmptyPieces, Item, OnItemFailure, heal, items};
