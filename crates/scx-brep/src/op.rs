// Written for solidcorex. Apache-2.0.
//! The calling convention and result types of an operation (01 §5.2, §6.3; research 72 §1, S2).
//!
//! - [`Cx`] bundles what an operation reads besides its bodies: the document, the
//!   [`Settings`], the [`Scratch`] memo, the [`Stop`] and a [`Trace`]. Its fields are public, so
//!   a caller splits it and hands a geometry crate only the pieces it reads (the budget and the
//!   `Stop`, the `Resolution`, `&mut Scratch`).
//! - [`Done`] is a finished operation: the new body, what it changed and a [`Report`]. The next
//!   operation reads `&done.body`.
//! - A [`Report`] is canonical: [`Report::new`] orders its parts, so equal work gives equal
//!   reports whatever order the parts were found in.

use core::any::{Any, TypeId};
use std::collections::BTreeMap;

use scx_num::budget::{Budget, Stop};
use scx_num::hash::{Rng, mix};
use scx_num::units::Accuracy;

use crate::{Brep, Changes, Document, Entity};

/// What an operation reads besides its bodies. A plain bundle: split it by its fields.
#[allow(missing_debug_implementations)] // `dyn Trace` has no `Debug`.
pub struct Cx<'a> {
    /// The document the bodies live in.
    pub doc: &'a Document,
    /// The algorithm choices, seed and budget.
    pub settings: &'a Settings,
    /// The memo of work that outlives one call.
    pub scratch: &'a mut Scratch,
    /// Cancellation, polled where the budget is metered.
    pub stop: &'a Stop,
    /// Progress and diagnostics; it cannot affect a result.
    pub trace: &'a dyn Trace,
}

impl<'a> Cx<'a> {
    /// A context over the given pieces.
    #[must_use]
    pub fn new(
        doc: &'a Document,
        settings: &'a Settings,
        scratch: &'a mut Scratch,
        stop: &'a Stop,
        trace: &'a dyn Trace,
    ) -> Self {
        Self {
            doc,
            settings,
            scratch,
            stop,
            trace,
        }
    }

    /// A fresh budget of the settings' limit, for one operation.
    #[must_use]
    pub const fn budget(&self) -> Budget {
        self.settings.budget()
    }
}

/// The choices that may change an answer. Each is part of every memo key its answer feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Settings {
    /// The random seed. Each draw is `Rng::new(mix(&[seed, op, entity]))` ([`Settings::rng`]).
    pub seed: u64,
    /// The work budget of one operation, in deterministic units.
    pub budget: u64,
}

impl Settings {
    /// The default seed and an unlimited budget.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            seed: Rng::DEFAULT_SEED,
            budget: u64::MAX,
        }
    }

    /// These settings with `seed`.
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// These settings with a budget of `limit` units.
    #[must_use]
    pub const fn with_budget(mut self, limit: u64) -> Self {
        self.budget = limit;
        self
    }

    /// A fresh budget of the limit.
    #[must_use]
    pub const fn budget(&self) -> Budget {
        Budget::new(self.budget)
    }

    /// The generator for operation `op` on entity word `entity`: fixed by the seed and the two
    /// words, never by the order of other draws.
    #[must_use]
    pub fn rng(&self, op: u64, entity: u64) -> Rng {
        Rng::new(mix(&[self.seed, op, entity]))
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

/// A memo of work that outlives one call. Entries are keyed by their type and a caller's key
/// word, held in key order, and a hit charges the work its fill recorded (01 §6.3).
#[derive(Default)]
pub struct Scratch {
    memo: BTreeMap<(TypeId, u64), Box<dyn Any + Send + Sync>>,
}

impl core::fmt::Debug for Scratch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Scratch")
            .field("entries", &self.memo.len())
            .finish()
    }
}

impl Scratch {
    /// An empty memo.
    #[must_use]
    pub fn new() -> Self {
        Self {
            memo: BTreeMap::new(),
        }
    }

    /// The entry of type `T` under `key`.
    #[must_use]
    pub fn get<T: Any + Send + Sync>(&self, key: u64) -> Option<&T> {
        self.memo
            .get(&(TypeId::of::<T>(), key))
            .and_then(|b| b.downcast_ref())
    }

    /// Stores `value` under `key`, returning the entry it replaced.
    pub fn insert<T: Any + Send + Sync>(&mut self, key: u64, value: T) -> Option<T> {
        let old = self
            .memo
            .insert((TypeId::of::<T>(), key), Box::new(value))?;
        old.downcast().ok().map(|b| *b)
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.memo.len()
    }

    /// Whether there are no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.memo.is_empty()
    }
}

/// An event an operation reports. A sink receives values and returns nothing, so it cannot
/// change a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// A named phase began.
    Phase(&'static str),
    /// `done` of `total` items are finished.
    Progress {
        /// Items finished.
        done: u64,
        /// Items in all.
        total: u64,
    },
}

/// A sink for progress and diagnostics.
pub trait Trace: Sync {
    /// Receives one event.
    fn event(&self, event: Event);
}

/// The sink that drops every event.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTrace;

impl Trace for NoTrace {
    fn event(&self, _: Event) {}
}

/// What a [`Warning`] says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum WarningKind {
    /// The entity's accuracy was raised (see the matching [`Raise`]).
    AccuracyRaised,
    /// The entity was identified with one of the other operand within coincidence.
    Coincident,
}

/// A result that is right but the caller may want to know of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Warning {
    /// The entity concerned.
    pub entity: Entity,
    /// What happened.
    pub kind: WarningKind,
}

/// An entity whose accuracy grew. Lost accuracy is never a failure (01 §5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Raise {
    /// The entity.
    pub entity: Entity,
    /// Its accuracy before.
    pub before: Accuracy,
    /// Its accuracy after.
    pub after: Accuracy,
}

/// An item an operation dropped, in a mode that skips failed items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped<C> {
    /// The item's index.
    pub item: usize,
    /// The round of the skip fixpoint that dropped it.
    pub round: u32,
    /// Why.
    pub cause: C,
}

/// What an operation says about its result besides the body.
#[derive(Debug, Clone, PartialEq)]
pub struct Report<C> {
    /// Warnings, in entity then kind order.
    pub warnings: Vec<Warning>,
    /// Dropped items, in item then round order.
    pub skipped: Vec<Skipped<C>>,
    /// Accuracy raises, in entity order.
    pub raised: Vec<Raise>,
    /// The budget used, in deterministic units.
    pub work: u64,
}

impl<C> Report<C> {
    /// A report of `work` units with nothing to say.
    #[must_use]
    pub const fn empty(work: u64) -> Self {
        Self {
            warnings: Vec::new(),
            skipped: Vec::new(),
            raised: Vec::new(),
            work,
        }
    }

    /// A report of the given parts in canonical order: whatever order they were found in.
    /// Equal keys keep their given order.
    #[must_use]
    pub fn new(
        mut warnings: Vec<Warning>,
        mut skipped: Vec<Skipped<C>>,
        mut raised: Vec<Raise>,
        work: u64,
    ) -> Self {
        warnings.sort();
        skipped.sort_by_key(|s| (s.item, s.round));
        raised.sort_by_key(|r| r.entity);
        Self {
            warnings,
            skipped,
            raised,
            work,
        }
    }

    /// Whether the operation had nothing to report besides its work.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty() && self.skipped.is_empty() && self.raised.is_empty()
    }
}

/// A finished operation.
#[derive(Debug, Clone)]
pub struct Done<C> {
    /// The new body; the next operation reads `&done.body`.
    pub body: Brep,
    /// What the operation changed.
    pub changes: Changes,
    /// What it says besides.
    pub report: Report<C>,
}

impl<C> Done<C> {
    /// A finished operation.
    #[must_use]
    pub const fn new(body: Brep, changes: Changes, report: Report<C>) -> Self {
        Self {
            body,
            changes,
            report,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Draft, FaceId, VertexId};

    fn step(body: &Brep, doc: Document) -> Done<()> {
        let (body, changes) = body.edit().finish(&doc).expect("an empty edit finishes");
        Done::new(body, changes, Report::empty(1))
    }

    #[test]
    fn construction_and_chaining() {
        let doc = Document::default();
        let settings = Settings::new().with_seed(7).with_budget(100);
        let mut scratch = Scratch::new();
        let stop = Stop::default();
        let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
        assert_eq!(cx.budget().limit(), 100);
        assert_eq!(cx.budget().used(), 0);
        // A caller splits the bundle and hands out the pieces.
        let Cx {
            settings,
            scratch,
            stop,
            ..
        } = cx;
        let mut budget = settings.budget();
        assert!(stop.check().is_ok());
        scratch.insert(3, 9_u32);
        assert_eq!(scratch.get::<u32>(3), Some(&9));
        assert_eq!(scratch.get::<u64>(3), None);
        assert_eq!(scratch.insert(3, 10_u32), Some(9));
        assert_eq!(budget.limit(), 100);
        let _ = &mut budget;

        let (first, changes) = Draft::new().finish(&doc).expect("an empty draft finishes");
        let a = Done::<()>::new(first, changes, Report::empty(0));
        let b = step(&a.body, doc);
        let c = step(&b.body, doc);
        assert_eq!(c.report.work, 1);
        assert!(c.report.is_clean());
        assert!(c.changes.is_empty());
    }

    #[test]
    fn settings_draws_depend_on_inputs_only() {
        let s = Settings::new().with_seed(5);
        let mut a = s.rng(1, 2);
        let mut b = s.rng(1, 2);
        let mut c = s.rng(1, 3);
        let (x, y, z) = (a.next_u64(), b.next_u64(), c.next_u64());
        assert_eq!(x, y);
        assert_ne!(x, z);
        assert_eq!(Settings::default(), Settings::new());
    }

    #[test]
    fn report_order_is_canonical() {
        let f = |i| Entity::Face(FaceId::from_raw(i));
        let v = |i| Entity::Vertex(VertexId::from_raw(i));
        let w = |e, kind| Warning { entity: e, kind };
        let sk = |item, round| Skipped {
            item,
            round,
            cause: item * 10 + round as usize,
        };
        let r = |e, a| Raise {
            entity: e,
            before: Accuracy::default(),
            after: a,
        };
        let acc = Accuracy::default();
        let one = Report::new(
            vec![
                w(v(1), WarningKind::Coincident),
                w(f(2), WarningKind::AccuracyRaised),
                w(f(1), WarningKind::Coincident),
            ],
            vec![sk(2, 0), sk(1, 1), sk(1, 0)],
            vec![r(v(0), acc), r(f(3), acc)],
            4,
        );
        let two = Report::new(
            vec![
                w(f(1), WarningKind::Coincident),
                w(v(1), WarningKind::Coincident),
                w(f(2), WarningKind::AccuracyRaised),
            ],
            vec![sk(1, 0), sk(1, 1), sk(2, 0)],
            vec![r(f(3), acc), r(v(0), acc)],
            4,
        );
        assert_eq!(one, two);
        assert_eq!(one.warnings[0].entity, f(1));
        assert_eq!(
            one.skipped.iter().map(|s| s.cause).collect::<Vec<_>>(),
            [10, 11, 20]
        );
        assert_eq!(one.raised[0].entity, f(3));
        assert!(!one.is_clean());
    }
}
