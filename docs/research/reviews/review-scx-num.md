# Review: scx-num (layer 0) and the workspace skeleton, up to 3e868cb

Independent review of the move of solidcore layer 0 into `crates/scx-num`, checked against the pin
`03e16c9ee766de5b9b1ced845e5149d62ff493b8` (`sc-wt/scx-pin`). Scope: the workspace skeleton
(`Cargo.toml`, `xtask/`, lints, `README.md`, `LICENSE`, `NOTICE`), `crates/scx-num`,
`crates/scx-difftest`, and report `docs/research/52-num-move.md`, read as claims. Every file was
read from an export of commit 3e868cb, not from the working tree, because a second builder was
changing that tree during the review. Runs were on server2, in a worktree at 3e868cb
(`scx-review-3e868cb`).

**Verdict: PASS WITH FIXES.** There is no blocker. The code is a faithful move:
- No function body differs at the pin apart from paths, error names and the `&Kernel` to
  `&Tolerances` parameter.
- Every one of the 266 difftests matches bit for bit on 618,474 inputs.
- The harness can fail, and its controls show it.

The fixes are in the guard rails rather than the moved code:
- The layer check can be bypassed.
- The header check can be faked.
- Errors are compared only in part.
- Nothing checks that the pin path is really at the pin commit.
- The inputs rarely reach tolerance boundaries.
- Report 52 makes some claims that do not hold.

Scripts and their outputs are in `docs/research/reviews/scx-num-data/`:

| File | What it does |
|---|---|
| `fndiff.py`, `fndiff-out.txt` | Compares function bodies after normalisation |
| `upstream_record.py` | Checks each header's upstream record word for word |
| `branchy.py`, `branchy-out.txt` | Ranks functions by coverage regions from llvm-cov JSON |

## 1. Fidelity

**Method.** `fndiff.py` pulls every `fn` body out of the pin file and out of the moved file. It
keys each body by impl or mod plus name, and drops comments and literals. It then normalises:
- the paths (`crate::geom::`, `geom_utility::p1/p2::`, `nurbs::math::`);
- the error names (`SolidError` and `NumError`, `GeomFailure` and `NumFailure`, `geom(`/`geom_call`
  and `failed(`/`call`);
- the visibility (`pub(crate)` and `pub`);
- the parameter (`k: &Kernel`/`k.tolerances.` and `t: &Tolerances`/`t.`).

After that it compares the token streams.

**Result** (`fndiff-out.txt`):

| File | Same | Differ | Pin only | Here only |
|---|---|---|---|---|
| math | 84 | 3 | 1 | 0 |
| limits | 6 | 0 | 0 | 0 |
| vector2d | 40 | 0 | 0 | 0 |
| vector3d | 89 | 0 | 0 | 0 |
| matrix | 43 | 2 | 0 | 0 |
| extent1d | 70 | 1 | 1 | 0 |
| extent2d | 73 | 1 | 1 | 0 |
| extent3d | 94 | 0 | 2 | 0 |
| periodic_extent1d | 34 | 0 | 0 | 0 |
| certified | 27 | 0 | 0 | 2 |
| tol | 48 | 14 | 58 | 7 |
| transform | 34 | 1 | 22 | 3 |
| small_matrix | 7 | 4 | n/a | n/a |

Every "differ" entry was read by hand:
- **math** (`transformed_box`, `rigid_plane_axes`, one test): the import path of `gamma` and the
  name of the error type.
- **matrix** (tests only): `upstream_status() == Some(1001)` became
  `matches!(.., Failed)`.
- **tol**:
  - 9 functions differ only by the `k` to `t` parameter.
  - Every function that takes `&Tolerances` reads the same field it read through
    `k.tolerances` at the pin. No field is swapped, and nothing reads `zone_tol_3d` where the pin
    read `approx_tol`.
- **small_matrix**: `determinant_3x3` differs only by its `p1::` path. The tests are covered under
  "Tests" below.

No floating-point operation was reordered, no constant was retyped, no method was swapped and no
early return was moved. `certified::gamma` matches pin `nurbs/math.rs:145` word for word.

**Pin-only items** are the ones the report says stay behind:
- `Transform::of_placement`;
- the `Content` impls;
- `Extent3d::from_pseudo_box` and `circumscribe`;
- in tol, the context/brep/calc-zone, the scaled zeros of curve, volume and equation,
  `Relation`, `update_object_tolerance` and the tests that use bodies.

**Here-only items:**
- `Tolerances::default`;
- `ang_tol_rad`;
- `zone_for_size`, whose body is token-identical to `size_estimate_to_zone_tol_3d`;
- `Transform::from_matrix`;
- new tests.

**`Tolerances`** is copied from pin `kernel.rs:46-89`. Its default now uses named constants in
place of literals, and the values are the same. Section 4 compares the constants bit by bit.

**Tests.**
- SHOULD-FIX: pin test `minor_skips_row_and_column` (`geom_utility/p2.rs:1461-1464`) had a second
  assertion, `minor_3x3(&a, 3, 2) == [[1,2,4],[5,6,8],[9,10,12]]`. That assertion was dropped
  silently; `small_matrix.rs:221-232` keeps only the first. It is the only case that removes the
  last row, so put it back.
- One assertion was added: `inverse_4x4(zero).is_err()` in the mult test. That is fine.

## 2. The cut and the public API

The cut follows report 52 §1:
- the 13 moved files;
- `NOZ`, `NOW`, `UNIT` and `gamma` moved down from `nurbs/math.rs`;
- a dependency list that is empty (confirmed by `cargo metadata`: `scx-num` has
  `"dependencies":[]`).

Nothing left behind is something that layer 0 needs, and nothing moved needs a higher layer.

**Minimality.** These are NITs.
- `Extent2d` and `Extent3d` made their `min`/`max` fields private. However, `from_bounds`
  (`extent2d.rs:200`, `extent3d.rs:320`) is a public, unchecked `const` constructor. The privacy
  therefore protects no invariant: it only costs the next layer a rewrite. At the pin,
  `circumscribe` wrote the fields directly, so when it moves it must go through `from_bounds` with
  the same order of writes. Keep that in mind for the algo move. Either make the fields public
  again or document `from_bounds` as the one unchecked way in.
- `math::Transform` has a public `m` (`math.rs:172-175`). `transform::Transform` has a private
  `m` (`transform.rs:82-84`). Both match the pin, so leave them.
- `det3` was made `pub` for eval and blend, but nothing in scx-num calls it (see §3).

## 3. Harness

**It runs.** On server2, `cargo test -p scx-difftest` passes 267 tests:
- 265 diffs with 0 mismatches;
- 2 controls.

The diffs cover 618,474 inputs including the control's 1,600, or 616,874 without them. Report 52
§3 says the same.

**It can fail.**
- `controls::a_different_function_is_caught` compares `max2` with `min2`. It reports 1,410 of
  1,600 inputs as different and panics as expected.
- `signed_zero_nan_payload_and_panic_are_told_apart` separates ±0, NaN sign, one ulp and a panic.

**Inputs and branches.** I measured region coverage of scx-num during the difftest run.
`llvm-cov`, `-C instrument-coverage` on the `scx_num` and `diff` crates only, gives:
- **Overall:** 95.75 % of regions and 96.18 % of functions. matrix.rs is lowest at 88.7 %, and
  most of its gap is the error arms of the bounds-checked `get`/`put`, which are never reached.
- **Top 30 functions by region count** (`branchy-out.txt`): 14 of 30 have a region that never
  ran. After removing the unreachable `?` arms in matrix (`sv_decompose`, `jacobi`,
  `lu_decompose`, `solve_with_lu`, `transpose`, `sort_eigen_system`), 8 of 30 have a real missed
  branch:
  - `math::is_regular` `:402` (the linear part is all zero, giving `Verdict::No`) and `:407` (the
    largest entry is subnormal);
  - `periodic_extent1d::add_interval` `:269-272` (the empty-set case);
  - `extent3d::meets_ray` `:1078` (a NaN slab);
  - `extent1d::contains_periodic_value` `:872`;
  - `extent1d::clamp_vector` `:980`, `:993` (the counts path);
  - `math::centroid_and_mean_radius` `:902`, `:919`, `:927` (error arms);
  - `math::three_point_circle_2d` `:1234`.
- **Never entered:** 23 functions. 17 are `?`/closure arms in matrix and error formatting. The
  others are:
  - `math::det3`. It has no caller in scx-num, and the difftest cannot reach it because it is
    `pub(crate)` at the pin. Report 52 §3 says it is "covered through the determinants that use
    it", which is false (SHOULD-FIX in report 52).
  - `Transform::from_matrix`.
  - all of `NumError`'s `Display`.

**SHOULD-FIX: boundary inputs.** The 34 edge values reach the absolute-zero thresholds exactly
(for example 1e-12 against 0). They do not reach these boundaries:
- the scaled-zero boundaries (`size * EFF_ZERO`);
- the coincidence boundaries (`zone`, `zone/2`, `2*zone`);
- the zone-table edges inside the difftest (`zone_for_size` has them only in its unit test).

That is where a moved `<` turned into `<=` would hide. For each tolerance-taking function, add a
pool that holds {t, next_up(t), next_down(t)}, and for scaled zeros the same around `size*EFF_ZERO`.
Also add a zero matrix, a subnormal-scaled matrix and an empty periodic interval, which cover the
branches listed above.

**SHOULD-FIX: errors are compared only in part.** `facade.rs:299-320` compares these fields:
- `kind`;
- `operation`;
- `failure()` by `Debug`;
- the cause chain.

It does not compare the `InvalidInput` `reason` or the `Display` text. Coverage confirms that
`Display` never runs. A reason string attached to the wrong check would pass. Add
`self.to_string()` to both fingerprints; the strings were copied word for word, so they should
match. NaN payloads inside `failure()` are also lost through `Debug`, which is a NIT: fingerprint
the f64 fields of the failure by bits.

**The 4 functions with no diff.** These are `next_up`, `next_down`, `det3` and `from_matrix`.
- `next_up`/`next_down` have an independent bit-layout property test. That is enough.
- `det3` is a pure three-line expression and body-identical to the pin (fndiff). Its risk is low,
  but it has no test at all. Add a unit test against an integer determinant (NIT).
- `from_matrix` is new and unit-tested.

**SHOULD-FIX: nothing checks the pin commit.** `scx-difftest/Cargo.toml:15` is a path dependency on
`../../../sc-wt/scx-pin/...`, and the commit appears only in a comment. On server2 the worktree is
at 03e16c9e and clean, which I checked by hand. If someone moves or dirties it, the difftest
compares against a different solidcore and still reports PASS. Either:
- add a `build.rs` that reads that worktree's HEAD with `git rev-parse` and `git status
  --porcelain`, and fails unless it is `PIN` and clean; or
- make it a git dependency with `rev = "03e16c9e…"`.

**Compiler, flags and profile.**
- Both sides build in one cargo invocation, so they share one rustc (1.99.1, `58cf8f9c3`,
  LLVM 23.1.1, the same as the pin's `rust-toolchain.toml`).
- No `RUSTFLAGS` and no `target-cpu` are set anywhere.
- The pin's `[profile]` sections (dev opt 1, release opt 3 with thin LTO) do not apply to a
  dependency, so the pin is built with solidcorex's profile. That is correct for a difftest.
  Rust does not contract FP, so the opt level does not change results.
- NIT: the README says to run the difftest with `--release`. That profile has no debug assertions
  and no overflow checks, so it compares fewer panic outcomes than dev does. My runs used dev. Run
  it in dev (or both), and say which in the README.

## 4. The new parts

**Constants** (`consts.rs`, `tol.rs`), compared bit for bit with the pin:
- `ZONE_DEFAULT` = 1e-5 (pin `geom/tol.rs:209`, `kernel.rs:70`).
- `EPS_LEN` = `EPS_REL` = `EFF_ZERO` = 1e-12.
- `EPS_PARAM` = 1.2e-8, and `EFF_ZERO_PARAM` is now defined from it.
- `MAX_ACCURACY_RATIO` = 1000.
- `UNINIT_TOL` = `UNDEF_DOUBLE` = 1.23456e-61, the same bits as the pin's literal. NIT: this
  change is not listed in tol.rs's header under Changes.
- Every value in `Tolerances::default` is the same as at the pin.

**SHOULD-FIX: two meanings share one constant.** `tol.rs:138` defines `LARGE_SMALL_SIZE_RATIO =
MAX_ACCURACY_RATIO`. The two names stand for different things:
- `LARGE_SMALL_SIZE_RATIO` is the `Tolerances` default that drives the small-topology zone.
- `MAX_ACCURACY_RATIO` is the validity limit. At the pin, heal/edgeuse uses it as such and cites
  Test #59.

They are both 1000 today. Tying them together means a future change to the validity limit
silently moves a tolerance default. Give `LARGE_SMALL_SIZE_RATIO` its own literal `1000.0`, and
let a test assert that the two are equal for now.

**The 5e-7 zone** (`mesh_reconstruction/topology.rs:64`, `const ZONE: f64 = 5e-7`) is a separate,
deliberate constant. It is the `zone_tol_3d` given to bodies rebuilt from STL, together with
`approx = ZONE*0.5` and `COINCIDENT = 1e-9`. It is not a copy of 1e-5 and not an upstream bug.
Report 52 §4 lists it, says it "uses a different value", and then says "These are replaced as
their layers move". SHOULD-FIX (report): split it out explicitly as "not a copy; keep it", so that
the mesh move does not swap it for `ZONE_DEFAULT`. I confirmed every other location in that list
at the pin.

**`NumError`** (`error.rs`) keeps only the variants that the moved files construct. Two points
about how it was derived:
- `kind()` maps `Disjoint` and `PointOutsideExtent` to `Failed`. At the pin these reach `Failed`
  through the `of_status` fallback, so the result is the same, and the difftest compares `kind`.
- Upstream status codes are not kept. Only the matrix test relied on them, and it was rewritten.

**SHOULD-FIX: a derived file under a "written for" header.** `error.rs` opens with "Written for
solidcorex", but its `Display` strings and doc comments are copied word for word from pin
`error/geom.rs`. That makes it a modified file in the sense of Apache-2.0 §4(b). Give it the
derivation header: moved from `error.rs`/`error/geom.rs` at the pin, with that file's upstream
record and Changes ("only the variants layer 0 builds; renamed").

**Newtypes** (`units.rs`): `Length`, `Accuracy`, `Resolution`, `AngleTol` and `ParamTol`, as in
09 §7.1.
- Nothing uses them yet, and they have `pub` fields.
- 09's `coincident(res, a, b)` was not added. Report 52 §5 says so on purpose, which is fine for
  this step.

## 5. The rules

I tried to break each check.

**`layers`** has holes:
- **SHOULD-FIX: dependencies outside the workspace are not checked.** `xtask/src/main.rs:179`
  checks only dependencies whose names are workspace members. I demonstrated this on server2 in a
  scratch copy (`/tmp/scxbreak-1`). I added
  `solidcore = { path = ".../sc-wt/scx-pin/solidcore/crates/solidcore" }` to
  `crates/scx-num/Cargo.toml`, and **`cargo xtask layers` printed "layers: 3 crates in order"**.

  So the one dependency this repository exists to avoid, a library on the old kernel, is not
  caught, and nothing enforces "scx-num has no external dependencies". Fix: have `layers` reject
  any non-member dependency of a library unless an allow-list in `layers.txt` names it. Also
  reject `solidcore` by name everywhere outside `[difftest]`.
- NIT: a glob in `members` breaks the parse (`:105-118`). Fail with a message in that case.
- NIT: `split('#')` (`:79`, `:127`) cuts a value that contains `#`. It does no harm in today's
  manifests.
- `layers` already catches an upward dev-dependency and a crate missing from `layers.txt`.

**`headers`** can be faked:
- **SHOULD-FIX.** `header_ok` (`:231-239`) accepts any opening comment that contains "written for
  solidcorex", and it never compares the upstream record with the source.
  - The upstream records are in fact correct. `upstream_record.py` finds all 13 moved files
    word for word, except `small_matrix.rs`, which drops p2's "(enums from
    `source/SMLib/inc/SmTypes.h`)" (NIT).
  - However, the check that fails the build would not notice if they were wrong. Have `headers`
    read the named pin file through `git show PIN:<path>` and compare the quoted record. A file
    whose body appears in the pin (for example a fndiff hit) should not be allowed a "written for"
    header.
- NIT: `rust_files` skips every directory called `bin` (`:210`). That would silently exempt
  `src/bin/*.rs`.
- The 06 rule "a derivation header in an MIT crate fails the build" is not implemented. That is
  fine while there is no MIT crate, but record it as owed.

**`fast`:**
- `fast` runs clippy on the default members only (`:295`), so scx-difftest is not linted. I ran
  `cargo clippy -p scx-difftest --all-targets -D warnings` by hand: it is clean, in 35 s.
- `fast` does not run xtask's own tests.

Both are NITs. Add the difftest to a CI-tier clippy, and `-p xtask` to `fast`.

**Lints.**
- Workspace `unsafe_code = forbid` and `missing_docs` warn.
- Clippy pedantic warns. `unwrap`/`expect`/`panic`/`indexing_slicing`/`todo`/`unimplemented`/`dbg`
  are denied, and allowed only in tests through `clippy.toml`.
- The crate-wide `#![allow]` in the difftest `main.rs` is acceptable for a test crate.

**Licence** (Apache-2.0 §4):
- (a) The licence is included. `LICENSE` holds the terms; it drops the pin LICENSE's first line
  (the NVIDIA copyright) and the appendix. The NVIDIA notice is carried in `NOTICE`, so §4(c) is
  met. NIT: keep that line in LICENSE as solidcore does.
- (b) Changed files are marked, by the "Changes:" lines, except error.rs (see §4).
- (c) Copyright and attribution notices are kept. The pin's source files carry no per-file
  copyright lines.
- (d) `NOTICE` carries solidcore's notice and the upstream notice, and adds its own
  modifications paragraph. NIT: it drops solidcore's pointer to `ATTRIBUTION.md` and the
  third-party paragraph ("none apply"). Neither is required, but the ATTRIBUTION reference is
  useful.

I spot-checked headers on 18 files: 13 moved, 4 written here in scx-num, and the xtask main. All
are present and in the correct form, with the two exceptions above (error.rs, and small_matrix's
record).

## 6. Independence of the property-test oracles

`crates/scx-num/tests/properties.rs` has 10 tests. None of them calls the code under test as its
own oracle:
- **Exact integer oracles** (i128 Leibniz for 4x4, cross/dot/triple products): independent.
- **Unimodular inverses**, where `A·A⁻¹` must equal I exactly: independent.
- **The IEEE bit layout** for `next_up`/`next_down`: independent.
- **Integer translation round trips, the `min`/`max` box and interval oracles, and the linear
  residual computed from the raw entries**: independent.

Weak spots (NIT):
- `zone_follows_the_size_table` writes the table from the same source the function was ported
  from. This is a transcription check, not an independent oracle, which is acceptable.
- `unitize_gives_a_parallel_unit_vector` checks its parallel direction through `u.cross(..)`,
  which is the code under test. The cross product itself is checked exactly elsewhere, so this
  is acceptable.
- The `contains_*` checks skip inputs within 1e-6 of a side. That excludes exactly the
  tolerance boundaries noted in §3; the difftest has to cover them.

## 7. Runs on server2

All runs used `sc-slot`, `SC_BG=1` and `nice -n 10`, at 3e868cb. Logs are in
`scx-review/logs/`.

| Step | Result | Wall time |
|---|---|---|
| `cargo xtask fast` (warm target) | pass (fmt, clippy, layers, headers, 128 unit + 10 property) | 1.6 s, then 0.2 s |
| `cargo test -p scx-num` | 128 + 10 pass | 0.07 s |
| `cargo test -p xtask` | 2 pass | 0.13 s |
| `cargo build --target wasm32-unknown-unknown -p scx-num` | builds | 0.4 s (warm) |
| `cargo test -p scx-difftest --no-run` (dev) | builds | 83 s |
| difftest run (dev) | 267 pass, 0 mismatches, 618,474 inputs | 0.07 s of test time |
| `cargo clippy -p scx-difftest --all-targets -D warnings` | clean | 35 s |
| coverage build and run (scx_num and diff instrumented) | 267 pass | about 90 s |

The "cold" `fast` run was not cold: the worktree target already had a build in it. Treat 1.6 s as
the incremental figure.

**SHOULD-FIX: determinism on wasm is not shown.** The wasm32 target builds, but nothing runs on
it. scx-num calls `sqrt`, `sin`, `cos`, `atan2`, `acos` and `powi`/`powf`:
- `sqrt` is correctly rounded everywhere.
- The others call glibc libm on native and Rust's `libm` on wasm, so the bits can differ in the
  last place.

Bit-identity with the pin is shown only on x86_64 Linux. Until 07 §7 rule 9 (an owner decision)
is settled, either say this in 05-status, or run the property tests under
`wasm32-wasip1` + wasmtime and record which tests' bits differ.

## 8. Lessons for the plan and the next moves

1. **01/03, the oracle.** The token-normalised body diff (`fndiff.py`) caught everything the
   difftest would catch at this layer, and more: it found the dropped test assertion, which no
   difftest can see. Make it a standing step of every move. Commit a script like it, run it in
   CI over each moved file, and have it print only the diffs that remain after normalisation.
   The reviewer then reads a list a few dozen lines long instead of whole files.
2. **02, testing.** Change "every public function is diffed" to "every public function is diffed,
   and its boundary pool is listed". Report region coverage of the moved crate during the difftest
   with every move. The coverage recipe is in `scx-review/cov.sh`: the profiler runtime is linked
   only when the *final* test crate is also instrumented, so instrument `diff` as well as the
   library. Without that, no profile is written.
3. **07 §7, determinism.** The wasm question can only grow: `nurbs` and `algo` use far more
   transcendental functions. Settle rule 9 before the nurbs move, or every bit-identity claim
   there will be x86-only.
4. **09, tolerances.** The single-source constants work. Keep a "not a copy" list next to the
   "copies to replace" list (5e-7 belongs on it), and do not alias two named policies to one
   constant just because their values are equal today.
5. **Intervals into num** (in progress by the other builder). These are new code with no pin
   counterpart, so the difftest does not apply. Their oracles need to be independent:
   - exact rational or double-double arithmetic, as the recent commits do;
   - plus directed-rounding checks at `next_up`/`next_down`.

   Check `layers` by hand until the non-member hole is closed, because an interval crate is
   exactly the kind of thing someone pulls a dependency in for.
6. **algo.** This move carries `circumscribe` and `from_pseudo_box`, which write the private
   extent fields. Rebuild them through `from_bounds` and keep the write order. Diff them, with
   boundary pools.
7. **nurbs.** Keep the `operation` strings as the pin path (as layer 0 did:
   `"geom::vector3d::Vec3::unitize"`). They are part of the compared fingerprint, and they make a
   moved error traceable to its pin site. Add `Display` to the fingerprint before this move: there
   are many more error sites there.

## Findings by class

**BLOCKER:** none.

**SHOULD-FIX:**
1. `layers` passes a library that depends on a non-member crate, including solidcore itself.
   This was demonstrated: `xtask/src/main.rs:179`.
2. `headers` accepts any "written for solidcorex" comment and never checks the upstream record
   against the pin: `:231-239`.
3. `error.rs` is derived from pin `error/geom.rs` (its strings are copied word for word) but
   carries a "written for" header. Give it a derivation header (§4(b)).
4. The difftest does not compare the `InvalidInput` reason or the `Display` text: `facade.rs:299-320`.
5. Nothing verifies that `sc-wt/scx-pin` is at 03e16c9e and clean. Add a build-time check or a git
   rev dependency.
6. The input pools have no boundary values at tolerance and scaled-zero thresholds. There are
   eight real missed branches in the top 30, listed in §3.
7. The second assertion of the pin test `minor_skips_row_and_column`,
   `minor_3x3(&a, 3, 2)`, was dropped: `small_matrix.rs:221-232`.
8. `LARGE_SMALL_SIZE_RATIO = MAX_ACCURACY_RATIO` ties a tolerance default to a validity limit:
   `tol.rs:138`.
9. Report 52 §4 should mark the 5e-7 mesh zone as a separate constant, not a copy to replace.
   §3's claim that `det3` is "covered through the determinants" is false: nothing calls it.
10. Bit-identity on wasm32 is not shown, because transcendental functions go through libm. Record
    this, or test it.

**NIT:**
- The extent fields are private even though a public unchecked `from_bounds` exists.
- `det3` has no test.
- `UNINIT_TOL` is missing from tol.rs's Changes, and vector2d's "paths only" leaves out the error
  type.
- `small_matrix.rs` drops p2's "(enums from SmTypes.h)" from its upstream record.
- In xtask, a glob in `members` or a `#` in a value breaks the parse, and the `bin` skip could
  exempt `src/bin` files.
- `fast` skips clippy on the difftest and xtask's own tests.
- The README suggests `--release` for the difftest. Dev compares more panic outcomes.
- NaN payloads in `failure()` are compared through `Debug`.
- LICENSE drops the NVIDIA line, and NOTICE drops the ATTRIBUTION pointer.
