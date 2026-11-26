//! A counting global allocator: total bytes allocated, bytes live, number of allocations.
//! Process-wide atomics; read deltas around single-threaded work.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// The allocator. Registered by this crate as the global allocator.
pub struct Counting;

static ALLOCATED: AtomicU64 = AtomicU64::new(0);
static FREED: AtomicU64 = AtomicU64::new(0);
static COUNT: AtomicU64 = AtomicU64::new(0);

// SAFETY: forwards every call to `System` unchanged; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(l.size() as u64, Relaxed);
        COUNT.fetch_add(1, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        FREED.fetch_add(l.size() as u64, Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(l.size() as u64, Relaxed);
        COUNT.fetch_add(1, Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        // counted as a fresh allocation of n and a free of the old size
        ALLOCATED.fetch_add(n as u64, Relaxed);
        FREED.fetch_add(l.size() as u64, Relaxed);
        COUNT.fetch_add(1, Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// A reading of the counters.
#[derive(Clone, Copy, Debug)]
pub struct Reading {
    pub allocated: u64,
    pub freed: u64,
    pub count: u64,
}

/// The counters now.
pub fn read() -> Reading {
    Reading {
        allocated: ALLOCATED.load(Relaxed),
        freed: FREED.load(Relaxed),
        count: COUNT.load(Relaxed),
    }
}

impl Reading {
    /// Bytes allocated since `earlier`.
    pub fn allocated_since(&self, earlier: &Reading) -> u64 {
        self.allocated - earlier.allocated
    }
    /// Net bytes still live that were allocated since `earlier` (may be negative).
    pub fn live_since(&self, earlier: &Reading) -> i64 {
        (self.allocated - earlier.allocated) as i64 - (self.freed - earlier.freed) as i64
    }
    /// Allocations since `earlier`.
    pub fn count_since(&self, earlier: &Reading) -> u64 {
        self.count - earlier.count
    }
}
