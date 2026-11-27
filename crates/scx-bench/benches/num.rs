// Written for solidcorex. Apache-2.0.
//! scx-num against the pin, timed: `cargo bench -p scx-bench --bench num`. The cases are
//! `scx_bench::num`'s; each is a group `num/<case>` (`num/<case>/edge` for its edge inputs)
//! holding the functions `scx` and `pin`.

mod common;

use criterion::{Criterion, criterion_group, criterion_main};

fn cases(c: &mut Criterion) {
    common::run(c, scx_bench::num::cases());
}

criterion_group! {
    name = benches;
    config = common::config();
    targets = cases
}
criterion_main!(benches);
