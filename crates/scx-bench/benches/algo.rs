// Written for solidcorex. Apache-2.0.
//! scx-algo against the pin, timed: `cargo bench -p scx-bench --bench algo`. The cases are
//! `scx_bench::algo`'s; each is a group `algo/<case>` (`algo/<case>/edge` for its edge inputs)
//! holding the functions `scx` and `pin`.

mod common;

use criterion::{Criterion, criterion_group, criterion_main};

fn cases(c: &mut Criterion) {
    common::run(c, scx_bench::algo::cases());
}

criterion_group! {
    name = benches;
    config = common::config();
    targets = cases
}
criterion_main!(benches);
