// Written for solidcorex (docs/01-architecture.md §8.2 C11, §10.1 item 8). Apache-2.0.
//! Where the work of a parallel site runs. A site that has `n` independent items hands
//! them to an [`Executor`]. The executor calls `f(i)` once for each `i` in `0..n`, on
//! whatever threads it has, and returns when all the calls have finished.
//!
//! An item's result never depends on the order the items run in or the thread they run
//! on: each item writes its own slot ([`map`]), and the site combines the slots in index
//! order. So every executor gives the same bits.
//!
//! [`Serial`] is the default, and the only choice on targets without threads (wasm32
//! without its threads feature). [`Scoped`] runs on a fixed number of
//! `std::thread::scope` workers. An app with its own pool (rayon, or the app's own)
//! implements the trait instead, so the library starts no threads of its own beside the
//! app's.

use core::num::NonZeroUsize;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Runs `n` independent items (C11).
pub trait Executor: Sync {
    /// Calls `f(i)` exactly once for each `i` in `0..n`, in any order and on any threads,
    /// and returns when every call has returned. A panic in `f` reaches the caller.
    fn scope(&self, n: usize, f: &(dyn Fn(usize) + Sync));
}

/// Every item on the calling thread, in index order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Serial;

impl Executor for Serial {
    fn scope(&self, n: usize, f: &(dyn Fn(usize) + Sync)) {
        for i in 0..n {
            f(i);
        }
    }
}

/// A fixed number of scoped worker threads (`std::thread::scope`), started for each call
/// and joined before it returns. The workers take the next item index from a shared
/// counter. With one worker, or at most one item, everything runs on the calling thread.
/// Where a thread cannot be started (a target without threads), the items it would have
/// taken run on the threads that did start, the calling thread among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scoped {
    workers: NonZeroUsize,
}

impl Scoped {
    /// `workers` threads per call.
    #[must_use]
    pub const fn new(workers: NonZeroUsize) -> Self {
        Self { workers }
    }

    /// The number of worker threads.
    #[must_use]
    pub const fn workers(&self) -> NonZeroUsize {
        self.workers
    }
}

impl Executor for Scoped {
    fn scope(&self, n: usize, f: &(dyn Fn(usize) + Sync)) {
        let threads = self.workers.get().min(n);
        if threads <= 1 {
            Serial.scope(n, f);
            return;
        }
        let next = AtomicUsize::new(0);
        let work = || loop {
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= n {
                break;
            }
            f(i);
        };
        std::thread::scope(|s| {
            for _ in 1..threads {
                // A target without threads (wasm32-wasip1) refuses the spawn. The
                // calling thread's own `work()` below then takes every item left.
                if std::thread::Builder::new().spawn_scoped(s, work).is_err() {
                    break;
                }
            }
            work();
        });
    }
}

/// `f(i)` for each `i` in `0..n`, run by `executor`, in index order. `None` when the
/// executor did not call some item (a broken executor).
pub fn map<T: Send + Sync>(
    executor: &dyn Executor,
    n: usize,
    f: impl Fn(usize) -> T + Sync,
) -> Option<Vec<T>> {
    let slots: Vec<OnceLock<T>> = (0..n).map(|_| OnceLock::new()).collect();
    executor.scope(n, &|i| {
        if let Some(slot) = slots.get(i) {
            let _ = slot.set(f(i));
        }
    });
    slots.into_iter().map(OnceLock::into_inner).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::hash::{Rng, mix};

    fn work(i: usize) -> u64 {
        let mut r = Rng::new(mix(&[0x5EED, i as u64]));
        (0..1000).fold(0, |h, _| h ^ r.next_u64())
    }

    #[test]
    fn every_executor_gives_the_same_answer() {
        let serial = map(&Serial, 257, work);
        assert_eq!(serial.as_ref().map(Vec::len), Some(257));
        for w in [1, 2, 3, 8, 300] {
            let ex = Scoped::new(NonZeroUsize::new(w).unwrap_or(NonZeroUsize::MIN));
            assert_eq!(map(&ex, 257, work), serial, "{w} workers");
        }
        assert_eq!(map(&Serial, 0, work), Some(Vec::new()));
    }

    #[test]
    fn each_item_runs_once() {
        let counts: Vec<AtomicUsize> = (0..100).map(|_| AtomicUsize::new(0)).collect();
        let ex = Scoped::new(NonZeroUsize::new(4).unwrap_or(NonZeroUsize::MIN));
        ex.scope(100, &|i| {
            if let Some(c) = counts.get(i) {
                c.fetch_add(1, Ordering::Relaxed);
            }
        });
        assert!(counts.iter().all(|c| c.load(Ordering::Relaxed) == 1));
    }

    #[test]
    fn a_skipped_item_is_reported() {
        struct Skips;
        impl Executor for Skips {
            fn scope(&self, n: usize, f: &(dyn Fn(usize) + Sync)) {
                (1..n).for_each(f);
            }
        }
        assert_eq!(map(&Skips, 3, work), None);
    }

    #[test]
    fn executors_are_objects() {
        let all: [&dyn Executor; 2] = [&Serial, &Scoped::new(NonZeroUsize::MIN)];
        for ex in all {
            assert_eq!(map(ex, 3, |i| i * 2), Some(vec![0, 2, 4]));
        }
    }
}
