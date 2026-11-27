// Written for solidcorex (docs/research/74-step6-idiom-prep.md §6, §19).
//! STEP import and export of solids and sheets, on [`scx_p21`].
//!
//! This is the skeleton (task SP1): everything a read decides before it builds geometry.
//!
//! - **Units** ([`units`]) are the file's. A representation context that names no length unit
//!   or no plane angle unit is refused ([`StepError::NoUnits`]) unless the caller assumes them
//!   with [`ReadOptions::assuming_units`], and the assumption is recorded in the
//!   [`ImportReport`]. Nothing defaults to millimetres.
//! - **Uncertainty** is the representation context's own `UNCERTAINTY_MEASURE_WITH_UNIT`
//!   ([`Uncertainty`]); with none it is [`Uncertainty::Absent`], which SP2 resolves to the
//!   document's resolution. It is never taken from another context or from "the last measure
//!   in the file".
//! - **[`Imported`]** is the result: one [`ImportedBody`] per solid or sheet, the file's
//!   product structure, the bodies skipped with their reason, and the report.
//! - **[`StepError`]** is typed; every cause has a kind and a stable name.
//! - **The entity plan** ([`ShellPlan`], [`FacePlan`]) is the skeleton of a body: its shells
//!   and their faces as entity ids, with the surface kind the file names. SP2 to SP4 fill it
//!   with geometry and topology.
//!
//! `scx-step` never heals. The report has a field for the raises SP2 makes to enclosed gaps
//! ([`ImportReport::raised`]); gap closing is `scx-stitch`'s and is requested with
//! [`ReadOptions::closing_gaps`], which is [`StepError::Pending`] until SP4.

mod error;
mod read;
mod report;
mod skeleton;
pub mod units;

pub use error::{Pending, StepError};
pub use read::{Imported, ImportedBody, ReadOptions, SolidOrSheet, read};
pub use report::{AssumedUnits, GapKind, ImportReport, ImportWarning, Raise, SkipReason, Skipped};
pub use skeleton::{FacePlan, ShellPlan, ShellRole};
pub use units::{MissingUnits, Resolved, Uncertainty};
