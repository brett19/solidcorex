// Written for solidcorex (docs/research/74-step6-idiom-prep.md §6, §19, §29).
//! STEP import and export of solids and sheets, on [`scx_p21`].
//!
//! The read (task SP1) decides everything before geometry; the analytic build (task SP2,
//! [`build`] and [`import`]) makes each body from its plan.
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
//! - **The product tree** ([`ProductTree`]) is the file's assembly expanded into placed
//!   instances: every use of a part ([`Instance`]) and every body with the map that places it in
//!   the world frame ([`PlacedBody`], [`World`]). A placement the file does not give, or gives
//!   unusably, is [`World::Unplaced`] with its cause; nothing is placed by guess.
//!
//! - **The build** ([`build`], [`import`]) makes a [`Built`] body of planes, quadrics and tori
//!   bounded by lines and conics: draft, then each enclosed gap past the file's uncertainty
//!   raised and recorded ([`Raise`]), exact pcurves, parity and the certified volume sign,
//!   and the import check. Every other kind is a typed [`BuildError::Pending`] ([`Waits`]).
//!   Each body is built once; the product tree places it. A B-spline surface (or a cone or
//!   torus written with sizes the kinds refuse) that is a plane, cylinder, cone, sphere or
//!   torus within the body's allowance is built as that quadric, and the proved deviation is
//!   recorded ([`Recognition`], [`Note::Recognized`]); one that is not stays pending.
//!
//! `scx-step` never heals. The raises the build makes to enclosed gaps are recorded
//! ([`ImportReport::raised`]); gap closing is `scx-stitch`'s and is requested with
//! [`ReadOptions::closing_gaps`], which is [`StepError::Pending`] until SP4.

mod build;
mod error;
mod geom;
mod import;
mod products;
mod read;
mod report;
mod skeleton;
pub mod units;

pub use build::{build, import};
pub use error::{Pending, StepError};
pub use import::{BuildError, Built, GeomCause, Import, Note, Recognition, Unbuilt, Waits};
pub use products::{Instance, Part, PlacedBody, ProductTree, Unplaced, Use, World};
pub use read::{Imported, ImportedBody, ReadOptions, SolidOrSheet, read};
pub use report::{AssumedUnits, GapKind, ImportReport, ImportWarning, Raise, SkipReason, Skipped};
pub use skeleton::{FacePlan, ShellPlan, ShellRole};
pub use units::{MissingUnits, Resolved, Uncertainty};
