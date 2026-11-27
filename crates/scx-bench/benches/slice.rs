// Written for solidcorex. Apache-2.0.
//! The step-4 slice against the pin, timed: `cargo bench -p scx-bench --bench slice`. The
//! cases are `slice_cases`'; each is a group `slice/<phase>/<recipe>` holding the
//! functions `scx` and `pin`. Each case is a whole body operation, so far fewer samples than
//! the per-input targets take.

mod common;
#[path = "common/slice_cases.rs"]
mod slice_cases;

use criterion::{Criterion, criterion_group, criterion_main};
use std::time::Duration;

fn cases(c: &mut Criterion) {
    // `SLICE_FILTER=<substring>` builds and runs only the cases whose name holds it (the
    // setup of every case is slow); criterion's own filter then applies as usual.
    let filter = std::env::var("SLICE_FILTER").unwrap_or_default();
    common::run(c, slice_cases::cases_matching(&filter));
}

criterion_group! {
    name = benches;
    config = common::config()
        .measurement_time(Duration::from_secs(5))
        .sample_size(10);
    targets = cases
}
criterion_main!(benches);
