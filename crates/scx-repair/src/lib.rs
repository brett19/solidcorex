// Written for solidcorex. Apache-2.0.
//! Certified repair of bodies (research 75, tasks RP1-RP8): [`heal`] with items per defect,
//! [`sew`] within a gap; later, hole patches and analytic rebuild, none needing Booleans.
//!
//! - [`heal`] does each [`Item`] only when every edit it makes is certified within the
//!   accuracies the body records, so the result is valid by construction and no accuracy
//!   grows. RP1 builds the short-edge [`Cluster`]: vertices within coincidence joined by
//!   edges, merged at one point, on lines and planes exactly.
//! - [`sew`] makes one of each pair of open edges within a gap, over `scx-stitch` (RP5): the
//!   kept edge's accuracy is the certified gap, recorded; a pair past it is
//!   [`SewCause::TooFar`].
//! - Items are in order and an [`OnItemFailure`] says what a failing one does (01 §5.2).
//! - Errors are [`RepairError`] in 01 §5.1's shape; an item's cause is a [`HealCause`].

mod error;
mod heal;
mod sew;

pub use error::{HealCause, RepairCause, RepairDefect, RepairError, SewCause};
pub use heal::{Cluster, Item, OnItemFailure, heal, items};
pub use sew::sew;
