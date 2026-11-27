// Written for solidcorex. Apache-2.0.
//! The work budget and the stop flag of an iterating routine (01 §6.3).
//!
//! A geometry routine that iterates takes `&mut Budget` and `&Stop` and polls both at the
//! same points, inside its loops, not only between items.
//!
//! - **[`Budget`]** counts work in deterministic units (solver branches, iterations,
//!   marching steps), never time. An operation holds one total: each item charges what it
//!   did, in item-index order, and the operation stops at the first charge that takes the
//!   running sum past the limit ([`Exhausted`]). A memo or cell hit charges the work recorded
//!   when it was filled, so the stop point does not move with cache warmth (04, 11-27).
//! - **[`Stop`]** is the caller's cancellation, set from any thread. It changes no result:
//!   a routine that finishes while the flag is clear returns the same bits as with no flag,
//!   and one that sees it set returns [`Stopped`] (kind `Cancelled`), never a partial answer
//!   as if complete.
//!
//! The global solver's own bounds have the same meaning: `SolveOptions::max_branches` is a
//! budget of `max_branches` branches, of which a solve completes that many and fails on the
//! next, as a [`Budget`] of that limit charged one per branch does; and
//! `BranchSolver::cancelled` is [`Stop::is_set`], asked at every branch after the budget.

use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::error::ErrorKind;

/// An operation's work budget: a limit and the work charged so far, in deterministic units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Budget {
    limit: u64,
    used: u64,
}

impl Budget {
    /// A budget of `limit` units, none used.
    #[must_use]
    pub const fn new(limit: u64) -> Self {
        Self { limit, used: 0 }
    }

    /// A budget that never runs out (`u64::MAX` units).
    #[must_use]
    pub const fn unlimited() -> Self {
        Self::new(u64::MAX)
    }

    /// The limit.
    #[must_use]
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    /// The work charged so far. Past the limit after the charge that ran out.
    #[must_use]
    pub const fn used(&self) -> u64 {
        self.used
    }

    /// The units left before the budget runs out: 0 once it has.
    #[must_use]
    pub const fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.used)
    }

    /// Charges `work` units. The work is recorded even when it runs out, so [`used`] is the
    /// work done (saturating at `u64::MAX`).
    ///
    /// [`used`]: Budget::used
    ///
    /// # Errors
    ///
    /// [`Exhausted`] when the running sum passes the limit. A limit of `n` admits `n`
    /// charges of one unit and refuses the next.
    pub const fn charge(&mut self, work: u64) -> Result<(), Exhausted> {
        self.used = self.used.saturating_add(work);
        if self.used > self.limit {
            return Err(Exhausted { limit: self.limit });
        }
        Ok(())
    }
}

/// A budget ran out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Exhausted {
    /// The budget's limit.
    pub limit: u64,
}

impl Exhausted {
    /// [`ErrorKind::Exhausted`]: the caller gives more, or stops.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        ErrorKind::Exhausted
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        "budget_exhausted"
    }
}

impl fmt::Display for Exhausted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the work budget of {} units ran out", self.limit)
    }
}

impl std::error::Error for Exhausted {}

/// The caller's cancellation flag, shared by reference (`&Stop` is `Send` and `Sync`).
///
/// Setting it changes no result except by ending a routine with [`Stopped`]: a routine
/// reads it only to stop, never to choose a path or a value.
#[derive(Debug, Default)]
pub struct Stop {
    set: AtomicBool,
}

impl Stop {
    /// A clear flag.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            set: AtomicBool::new(false),
        }
    }

    /// Asks every routine polling the flag to stop. It stays set.
    pub fn set(&self) {
        self.set.store(true, Ordering::Relaxed);
    }

    /// Whether the flag is set.
    #[must_use]
    pub fn is_set(&self) -> bool {
        self.set.load(Ordering::Relaxed)
    }

    /// `Err(Stopped)` once the flag is set.
    ///
    /// # Errors
    ///
    /// [`Stopped`] when the flag is set.
    pub fn check(&self) -> Result<(), Stopped> {
        if self.is_set() { Err(Stopped) } else { Ok(()) }
    }
}

/// The caller stopped the routine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stopped;

impl Stopped {
    /// [`ErrorKind::Cancelled`].
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        ErrorKind::Cancelled
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        "stopped"
    }
}

impl fmt::Display for Stopped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the caller stopped the operation")
    }
}

impl std::error::Error for Stopped {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_limit_of_n_admits_n_units_and_refuses_the_next() {
        let mut b = Budget::new(3);
        for left in [2, 1, 0] {
            assert_eq!(b.charge(1), Ok(()));
            assert_eq!(b.remaining(), left);
        }
        assert_eq!(b.charge(1), Err(Exhausted { limit: 3 }));
        assert_eq!((b.used(), b.remaining()), (4, 0));
    }

    #[test]
    fn a_charge_records_its_work_and_saturates() {
        let mut b = Budget::new(10);
        assert_eq!(b.charge(7), Ok(()));
        assert_eq!(b.charge(7), Err(Exhausted { limit: 10 }));
        assert_eq!(b.used(), 14);
        let mut u = Budget::unlimited();
        assert_eq!(u.charge(u64::MAX), Ok(()));
        assert_eq!(u.charge(1), Ok(()));
        assert_eq!(u.used(), u64::MAX);
        assert_eq!(Budget::new(0).charge(0), Ok(()));
        assert_eq!(Budget::new(0).charge(1), Err(Exhausted { limit: 0 }));
    }

    #[test]
    fn stop_is_clear_until_set_and_stays_set() {
        let s = Stop::new();
        assert!(!s.is_set());
        assert_eq!(s.check(), Ok(()));
        s.set();
        assert!(s.is_set());
        assert_eq!(s.check(), Err(Stopped));
        assert!(!Stop::default().is_set());
    }

    #[test]
    fn the_causes_have_kinds_and_names() {
        assert_eq!(Exhausted { limit: 1 }.kind(), ErrorKind::Exhausted);
        assert_eq!(Stopped.kind(), ErrorKind::Cancelled);
        assert_eq!(Exhausted { limit: 1 }.name(), "budget_exhausted");
        assert_eq!(Stopped.name(), "stopped");
        assert_eq!(
            Exhausted { limit: 5 }.to_string(),
            "the work budget of 5 units ran out"
        );
    }

    #[test]
    fn stop_is_shared_across_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<Stop>();
        let s = Stop::new();
        std::thread::scope(|t| {
            t.spawn(|| s.set());
        });
        assert!(s.is_set());
    }
}
