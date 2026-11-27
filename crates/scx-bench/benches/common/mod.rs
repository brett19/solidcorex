// Written for solidcorex. Apache-2.0.
//! The criterion runner: each case a group holding the functions `scx` and `pin`, each
//! timed iteration one batch, throughput per input (docs/02-testing.md §8.1).

use criterion::{Criterion, Throughput};
use scx_bench::{Case, Side};
use std::hint::black_box;
use std::time::Duration;

/// The harness settings for every target: enough samples for a tight interval, short
/// enough that a target runs in minutes.
pub fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(100)
}

/// Every case of `cases` as its own group.
pub fn run(c: &mut Criterion, cases: Vec<Case>) {
    for case in cases {
        let mut g = c.benchmark_group(&case.name);
        g.throughput(Throughput::Elements(case.len as u64));
        g.bench_function("scx", |b| b.iter(|| black_box(case.run(Side::Scx))));
        g.bench_function("pin", |b| b.iter(|| black_box(case.run(Side::Pin))));
        g.finish();
    }
}
