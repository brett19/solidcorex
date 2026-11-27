// Written for solidcorex.
//! The read: units, uncertainty and the entity plan of every body, with no geometry yet.

use std::collections::BTreeSet;

use scx_p21::product::{self, BodyKind, ItemKind, ProductStructure};
use scx_p21::units::Units;
use scx_p21::{Exchange, protocol};

use crate::error::{Pending, StepError};
use crate::report::{AssumedUnits, ImportReport, ImportWarning, SkipReason, Skipped};
use crate::skeleton::{self, ShellPlan};
use crate::units::{self, Resolved, UnitsFault};

/// What the caller decides about a read.
///
/// Every field starts at the option that assumes nothing and changes nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct ReadOptions {
    /// The units to use for a representation context that names none (or only one of the two).
    /// With `None`, such a context is [`StepError::NoUnits`]. The file's own units always win.
    pub assume_units: Option<Units>,
    /// Join open edges through `scx-stitch` (SP4). Until then, `true` is
    /// [`StepError::Pending`].
    pub close_gaps: bool,
}

impl ReadOptions {
    /// These options, assuming `units` where a context names none.
    #[must_use]
    pub const fn assuming_units(mut self, units: Units) -> Self {
        self.assume_units = Some(units);
        self
    }

    /// These options, closing gaps through stitching (SP4).
    #[must_use]
    pub const fn closing_gaps(mut self) -> Self {
        self.close_gaps = true;
        self
    }
}

/// Whether a body is a solid or a sheet (01: `scx-step` reads both).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolidOrSheet {
    /// `MANIFOLD_SOLID_BREP` or `BREP_WITH_VOIDS`.
    Solid,
    /// `SHELL_BASED_SURFACE_MODEL`.
    Sheet,
}

/// One body of the file: what SP2 to SP4 turn into a `Brep`.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedBody {
    /// The body's entity id.
    pub entity: u64,
    /// Its name.
    pub name: String,
    /// Solid or sheet.
    pub kind: SolidOrSheet,
    /// The first representation (entity id, in id order) that lists it.
    pub representation: u64,
    /// The units and uncertainty of that representation's context.
    pub context: Resolved,
    /// Its shells: a solid's outer shell first, then its voids.
    pub shells: Vec<ShellPlan>,
}

/// The result of a read.
///
/// SP2 adds each body's `Brep` and its findings to [`ImportedBody`]; the rest is as built here.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    /// The bodies, in representation then item order.
    pub bodies: Vec<ImportedBody>,
    /// The file's product structure (SP8 turns it into the tree of placed instances).
    pub products: ProductStructure,
    /// The bodies not in `bodies`, each with its reason.
    pub skipped: Vec<Skipped>,
    /// What else the read has to say.
    pub report: ImportReport,
}

/// Read the solids and sheets of an exchange.
///
/// A file whose representation context names no length unit or plane angle unit is refused
/// unless [`ReadOptions::assume_units`] supplies them; the units used are in
/// [`ImportedBody::context`], and an assumption is in [`ImportReport::assumed`]. A body whose
/// entities are not what its kind needs is skipped with its reason; the rest are read.
pub fn read(ex: &Exchange, opts: &ReadOptions) -> Result<Imported, StepError> {
    if opts.close_gaps {
        return Err(StepError::Pending(Pending::CloseGaps));
    }
    if opts.assume_units.is_some_and(|u| !units::assumable(u)) {
        return Err(StepError::BadAssumedUnits);
    }
    let declared = protocol::of_exchange(ex);
    let products = product::read(ex)?;
    let mut bodies = Vec::new();
    let mut skipped = Vec::new();
    let mut warnings: Vec<ImportWarning> = declared
        .doubt
        .iter()
        .cloned()
        .map(ImportWarning::Protocol)
        .collect();
    let mut assumed = Vec::new();
    let mut seen = BTreeSet::new();
    for rep in &products.representations {
        let listed: Vec<_> = rep
            .items
            .iter()
            .filter_map(|i| match i.kind {
                ItemKind::Body(k) if seen.insert(i.entity) => Some((i, k)),
                _ => None,
            })
            .collect();
        if listed.is_empty() {
            continue;
        }
        let context = units::resolve(ex, rep.context, opts.assume_units).map_err(|f| match f {
            UnitsFault::Missing(missing) => StepError::NoUnits {
                representation: rep.entity,
                missing,
            },
            UnitsFault::Conflicting(which) => StepError::ConflictingUnits {
                context: rep.context.unwrap_or(rep.entity),
                which,
            },
        })?;
        if context.assumed.any() {
            assumed.push(AssumedUnits {
                representation: rep.entity,
                units: context.units,
                which: context.assumed,
            });
        }
        if context.several_uncertainties {
            warnings.push(ImportWarning::SeveralUncertainties {
                representation: rep.entity,
            });
        }
        for (item, kind) in listed {
            if kind == BodyKind::FacetedBrep {
                skipped.push(Skipped {
                    entity: item.entity,
                    why: SkipReason::BodyKind("FACETED_BREP".to_owned()),
                });
                continue;
            }
            match skeleton::body(ex, item.entity, kind) {
                Ok((shape, shells)) => bodies.push(ImportedBody {
                    entity: item.entity,
                    name: item.name.clone(),
                    kind: shape,
                    representation: rep.entity,
                    context,
                    shells,
                }),
                Err(e) => skipped.push(Skipped {
                    entity: item.entity,
                    why: SkipReason::Malformed(Box::new(e)),
                }),
            }
        }
    }
    if bodies.is_empty() && skipped.is_empty() {
        warnings.push(ImportWarning::NoBodies);
    }
    Ok(Imported {
        bodies,
        products,
        skipped,
        report: ImportReport {
            declared,
            warnings,
            assumed,
            raised: Vec::new(),
        },
    })
}
