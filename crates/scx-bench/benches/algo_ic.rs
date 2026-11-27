// Written for solidcorex. Apache-2.0.
//! scx-algo against the pin, in instructions under callgrind: `cargo bench -p scx-bench
//! --bench algo_ic` (valgrind and `gungraun-runner` installed). The cases are
//! `scx_bench::algo`'s, one batch each; the setup that builds a case's inputs is not counted.
//! Every case `scx_bench::algo::cases()` makes is listed here (a library test checks that the
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
    scx_bench::algo::cases()
        .into_iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no case {name}"))
}

#[library_benchmark]
#[bench::bernstein_roots(case("algo/bernstein_roots"))]
#[bench::bernstein_roots_edge(case("algo/bernstein_roots/edge"))]
#[bench::cubic(case("algo/cubic"))]
#[bench::cubic_edge(case("algo/cubic/edge"))]
#[bench::gsolve_intersect(case("algo/gsolve_intersect"))]
#[bench::gsolve_maximize(case("algo/gsolve_maximize"))]
#[bench::gsolve_minimize(case("algo/gsolve_minimize"))]
#[bench::gsolve_minimize_best(case("algo/gsolve_minimize_best"))]
#[bench::gsolve_minimize_large(case("algo/gsolve_minimize_large"))]
#[bench::gsolve_projected_minimize(case("algo/gsolve_projected_minimize"))]
#[bench::krawczyk1(case("algo/krawczyk1"))]
#[bench::krawczyk1_edge(case("algo/krawczyk1/edge"))]
#[bench::krawczyk2(case("algo/krawczyk2"))]
#[bench::krawczyk2_edge(case("algo/krawczyk2/edge"))]
#[bench::newton2(case("algo/newton2"))]
#[bench::newton2_edge(case("algo/newton2/edge"))]
#[bench::ode_integrate2(case("algo/ode_integrate2"))]
#[bench::ode_integrate2_edge(case("algo/ode_integrate2/edge"))]
#[bench::ode_step(case("algo/ode_step"))]
#[bench::ode_step_edge(case("algo/ode_step/edge"))]
#[bench::quadratic(case("algo/quadratic"))]
#[bench::quadratic_edge(case("algo/quadratic/edge"))]
#[bench::quadrature(case("algo/quadrature"))]
#[bench::quadrature_edge(case("algo/quadrature/edge"))]
#[bench::quartic(case("algo/quartic"))]
#[bench::quartic_edge(case("algo/quartic/edge"))]
#[bench::solve1d(case("algo/solve1d"))]
#[bench::solve1d_edge(case("algo/solve1d/edge"))]
#[bench::solve_nd(case("algo/solve_nd"))]
#[bench::solve_nd_edge(case("algo/solve_nd/edge"))]
fn scx(case: Case) -> Checksum {
    black_box(case.run(Side::Scx))
}

#[library_benchmark]
#[bench::bernstein_roots(case("algo/bernstein_roots"))]
#[bench::bernstein_roots_edge(case("algo/bernstein_roots/edge"))]
#[bench::cubic(case("algo/cubic"))]
#[bench::cubic_edge(case("algo/cubic/edge"))]
#[bench::gsolve_intersect(case("algo/gsolve_intersect"))]
#[bench::gsolve_maximize(case("algo/gsolve_maximize"))]
#[bench::gsolve_minimize(case("algo/gsolve_minimize"))]
#[bench::gsolve_minimize_best(case("algo/gsolve_minimize_best"))]
#[bench::gsolve_minimize_large(case("algo/gsolve_minimize_large"))]
#[bench::gsolve_projected_minimize(case("algo/gsolve_projected_minimize"))]
#[bench::krawczyk1(case("algo/krawczyk1"))]
#[bench::krawczyk1_edge(case("algo/krawczyk1/edge"))]
#[bench::krawczyk2(case("algo/krawczyk2"))]
#[bench::krawczyk2_edge(case("algo/krawczyk2/edge"))]
#[bench::newton2(case("algo/newton2"))]
#[bench::newton2_edge(case("algo/newton2/edge"))]
#[bench::ode_integrate2(case("algo/ode_integrate2"))]
#[bench::ode_integrate2_edge(case("algo/ode_integrate2/edge"))]
#[bench::ode_step(case("algo/ode_step"))]
#[bench::ode_step_edge(case("algo/ode_step/edge"))]
#[bench::quadratic(case("algo/quadratic"))]
#[bench::quadratic_edge(case("algo/quadratic/edge"))]
#[bench::quadrature(case("algo/quadrature"))]
#[bench::quadrature_edge(case("algo/quadrature/edge"))]
#[bench::quartic(case("algo/quartic"))]
#[bench::quartic_edge(case("algo/quartic/edge"))]
#[bench::solve1d(case("algo/solve1d"))]
#[bench::solve1d_edge(case("algo/solve1d/edge"))]
#[bench::solve_nd(case("algo/solve_nd"))]
#[bench::solve_nd_edge(case("algo/solve_nd/edge"))]
fn pin(case: Case) -> Checksum {
    black_box(case.run(Side::Pin))
}

library_benchmark_group!(name = algo, benchmarks = [scx, pin]);

main!(library_benchmark_groups = algo);
