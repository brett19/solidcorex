// Written for solidcorex. Apache-2.0.
//! The step-4 slice against the pin, in instructions under callgrind: `cargo bench -p scx-bench
//! --bench slice_ic` (valgrind and `gungraun-runner` installed). The cases are
//! `slice_cases`', one batch each; the setup that builds a case's inputs is not counted.
//! Only the named case is built (`slice_cases::case`), so a benchmark builds one
//! recipe, not all; the list is `slice_cases::names()`.

// gungraun's macros expand to undocumented items and paths the workspace lints call
// over-qualified.
#![allow(missing_docs, unused_qualifications)]

#[path = "common/slice_cases.rs"]
mod slice_cases;

use gungraun::prelude::*;
use scx_bench::{Case, Checksum, Side};
use std::hint::black_box;

/// The case named `name`, its inputs built.
#[expect(
    clippy::panic,
    reason = "a benchmark naming no case is a bug in this file"
)]
fn case(name: &str) -> Case {
    slice_cases::case(name).unwrap_or_else(|| panic!("no case {name}"))
}

#[library_benchmark]
#[bench::make_block_3x4x5(case("slice/make/block-3x4x5"))]
#[bench::make_cylinder_r2_h10(case("slice/make/cylinder-r2-h10"))]
#[bench::make_cone_frustum_3_1(case("slice/make/cone-frustum-3-1"))]
#[bench::make_sphere_r5(case("slice/make/sphere-r5"))]
#[bench::make_torus_5_1_5(case("slice/make/torus-5-1.5"))]
#[bench::make_extrude_d(case("slice/make/extrude-d"))]
#[bench::build_box_cyl_hole(case("slice/build/box-cyl-hole"))]
#[bench::parts_box_cyl_hole(case("slice/parts/box-cyl-hole"))]
#[bench::mass_box_cyl_hole(case("slice/mass/box-cyl-hole"))]
#[bench::mass_full_box_cyl_hole(case("slice/mass_full/box-cyl-hole"))]
#[bench::classify_box_cyl_hole(case("slice/classify/box-cyl-hole"))]
#[bench::tess_box_cyl_hole(case("slice/tess/box-cyl-hole"))]
#[bench::wire_box_cyl_hole(case("slice/wire/box-cyl-hole"))]
#[bench::build_plate_4(case("slice/build/plate-4"))]
#[bench::parts_plate_4(case("slice/parts/plate-4"))]
#[bench::mass_plate_4(case("slice/mass/plate-4"))]
#[bench::mass_full_plate_4(case("slice/mass_full/plate-4"))]
#[bench::classify_plate_4(case("slice/classify/plate-4"))]
#[bench::tess_plate_4(case("slice/tess/plate-4"))]
#[bench::wire_plate_4(case("slice/wire/plate-4"))]
#[bench::build_plate_12(case("slice/build/plate-12"))]
#[bench::parts_plate_12(case("slice/parts/plate-12"))]
#[bench::mass_plate_12(case("slice/mass/plate-12"))]
#[bench::mass_full_plate_12(case("slice/mass_full/plate-12"))]
#[bench::classify_plate_12(case("slice/classify/plate-12"))]
#[bench::tess_plate_12(case("slice/tess/plate-12"))]
#[bench::wire_plate_12(case("slice/wire/plate-12"))]
#[bench::build_gear_60(case("slice/build/gear-60"))]
#[bench::parts_gear_60(case("slice/parts/gear-60"))]
#[bench::mass_gear_60(case("slice/mass/gear-60"))]
#[bench::mass_full_gear_60(case("slice/mass_full/gear-60"))]
#[bench::classify_gear_60(case("slice/classify/gear-60"))]
#[bench::tess_gear_60(case("slice/tess/gear-60"))]
#[bench::wire_gear_60(case("slice/wire/gear-60"))]
fn scx(case: Case) -> Checksum {
    black_box(case.run(Side::Scx))
}

#[library_benchmark]
#[bench::make_block_3x4x5(case("slice/make/block-3x4x5"))]
#[bench::make_cylinder_r2_h10(case("slice/make/cylinder-r2-h10"))]
#[bench::make_cone_frustum_3_1(case("slice/make/cone-frustum-3-1"))]
#[bench::make_sphere_r5(case("slice/make/sphere-r5"))]
#[bench::make_torus_5_1_5(case("slice/make/torus-5-1.5"))]
#[bench::make_extrude_d(case("slice/make/extrude-d"))]
#[bench::build_box_cyl_hole(case("slice/build/box-cyl-hole"))]
#[bench::parts_box_cyl_hole(case("slice/parts/box-cyl-hole"))]
#[bench::mass_box_cyl_hole(case("slice/mass/box-cyl-hole"))]
#[bench::mass_full_box_cyl_hole(case("slice/mass_full/box-cyl-hole"))]
#[bench::classify_box_cyl_hole(case("slice/classify/box-cyl-hole"))]
#[bench::tess_box_cyl_hole(case("slice/tess/box-cyl-hole"))]
#[bench::wire_box_cyl_hole(case("slice/wire/box-cyl-hole"))]
#[bench::build_plate_4(case("slice/build/plate-4"))]
#[bench::parts_plate_4(case("slice/parts/plate-4"))]
#[bench::mass_plate_4(case("slice/mass/plate-4"))]
#[bench::mass_full_plate_4(case("slice/mass_full/plate-4"))]
#[bench::classify_plate_4(case("slice/classify/plate-4"))]
#[bench::tess_plate_4(case("slice/tess/plate-4"))]
#[bench::wire_plate_4(case("slice/wire/plate-4"))]
#[bench::build_plate_12(case("slice/build/plate-12"))]
#[bench::parts_plate_12(case("slice/parts/plate-12"))]
#[bench::mass_plate_12(case("slice/mass/plate-12"))]
#[bench::mass_full_plate_12(case("slice/mass_full/plate-12"))]
#[bench::classify_plate_12(case("slice/classify/plate-12"))]
#[bench::tess_plate_12(case("slice/tess/plate-12"))]
#[bench::wire_plate_12(case("slice/wire/plate-12"))]
#[bench::build_gear_60(case("slice/build/gear-60"))]
#[bench::parts_gear_60(case("slice/parts/gear-60"))]
#[bench::mass_gear_60(case("slice/mass/gear-60"))]
#[bench::mass_full_gear_60(case("slice/mass_full/gear-60"))]
#[bench::classify_gear_60(case("slice/classify/gear-60"))]
#[bench::tess_gear_60(case("slice/tess/gear-60"))]
#[bench::wire_gear_60(case("slice/wire/gear-60"))]
fn pin(case: Case) -> Checksum {
    black_box(case.run(Side::Pin))
}

library_benchmark_group!(name = slice, benchmarks = [scx, pin]);

main!(library_benchmark_groups = slice);
