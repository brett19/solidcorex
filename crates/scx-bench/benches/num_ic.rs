// Written for solidcorex. Apache-2.0.
//! scx-num against the pin, in instructions under callgrind: `cargo bench -p scx-bench
//! --bench num_ic` (valgrind and `gungraun-runner` installed). The cases are
//! `scx_bench::num`'s, one batch each; the setup that builds a case's inputs is not counted.
//! Every case `scx_bench::num::cases()` makes is listed here (a library test checks that the
//! names exist; this list is the one to extend when a case is added).

// gungraun's macros expand to undocumented items and paths the workspace lints call
// over-qualified.
#![allow(missing_docs, unused_qualifications)]

use gungraun::prelude::*;
use scx_bench::{Case, Checksum, Side};
use std::hint::black_box;

/// The case named `name`, its inputs built.
#[expect(
    clippy::panic,
    reason = "a benchmark naming no case is a bug in this file"
)]
fn case(name: &str) -> Case {
    scx_bench::num::cases()
        .into_iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no case {name}"))
}

#[library_benchmark]
#[bench::compose(case("num/compose"))]
#[bench::compose_edge(case("num/compose/edge"))]
#[bench::contains(case("num/contains"))]
#[bench::contains_edge(case("num/contains/edge"))]
#[bench::distance(case("num/distance"))]
#[bench::distance_edge(case("num/distance/edge"))]
#[bench::extent1_fold(case("num/extent1_fold"))]
#[bench::extent1_fold_edge(case("num/extent1_fold/edge"))]
#[bench::extent1_union(case("num/extent1_union"))]
#[bench::extent1_union_edge(case("num/extent1_union/edge"))]
#[bench::extent3_fold(case("num/extent3_fold"))]
#[bench::extent3_fold_edge(case("num/extent3_fold/edge"))]
#[bench::extent3_union(case("num/extent3_union"))]
#[bench::extent3_union_edge(case("num/extent3_union/edge"))]
#[bench::inverse(case("num/inverse"))]
#[bench::inverse_edge(case("num/inverse/edge"))]
#[bench::normalize(case("num/normalize"))]
#[bench::normalize_edge(case("num/normalize/edge"))]
#[bench::point(case("num/point"))]
#[bench::point_edge(case("num/point/edge"))]
#[bench::transformed_box(case("num/transformed_box"))]
#[bench::transformed_box_edge(case("num/transformed_box/edge"))]
#[bench::vec_mix(case("num/vec_mix"))]
#[bench::vec_mix_edge(case("num/vec_mix/edge"))]
#[bench::vector(case("num/vector"))]
#[bench::vector_edge(case("num/vector/edge"))]
fn scx(case: Case) -> Checksum {
    black_box(case.run(Side::Scx))
}

#[library_benchmark]
#[bench::compose(case("num/compose"))]
#[bench::compose_edge(case("num/compose/edge"))]
#[bench::contains(case("num/contains"))]
#[bench::contains_edge(case("num/contains/edge"))]
#[bench::distance(case("num/distance"))]
#[bench::distance_edge(case("num/distance/edge"))]
#[bench::extent1_fold(case("num/extent1_fold"))]
#[bench::extent1_fold_edge(case("num/extent1_fold/edge"))]
#[bench::extent1_union(case("num/extent1_union"))]
#[bench::extent1_union_edge(case("num/extent1_union/edge"))]
#[bench::extent3_fold(case("num/extent3_fold"))]
#[bench::extent3_fold_edge(case("num/extent3_fold/edge"))]
#[bench::extent3_union(case("num/extent3_union"))]
#[bench::extent3_union_edge(case("num/extent3_union/edge"))]
#[bench::inverse(case("num/inverse"))]
#[bench::inverse_edge(case("num/inverse/edge"))]
#[bench::normalize(case("num/normalize"))]
#[bench::normalize_edge(case("num/normalize/edge"))]
#[bench::point(case("num/point"))]
#[bench::point_edge(case("num/point/edge"))]
#[bench::transformed_box(case("num/transformed_box"))]
#[bench::transformed_box_edge(case("num/transformed_box/edge"))]
#[bench::vec_mix(case("num/vec_mix"))]
#[bench::vec_mix_edge(case("num/vec_mix/edge"))]
#[bench::vector(case("num/vector"))]
#[bench::vector_edge(case("num/vector/edge"))]
fn pin(case: Case) -> Checksum {
    black_box(case.run(Side::Pin))
}

library_benchmark_group!(name = num, benchmarks = [scx, pin]);

main!(library_benchmark_groups = num);
