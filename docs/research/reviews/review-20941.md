# Review of tangency-labs/solidcraft#20941 (head 79c5ade77f, branch `cache-a4-latch`, Fixes #18916)

**Verdict: PASS WITH FIXES.** The code change is correct and complete. The golden rows are exactly what the body says they are. Fix these before the PR is marked ready:

1. **Certificate ledger (blocks ready: two goldens lib tests go red).** Update `solidcore/crates/goldens/src/certify/`:
   - `trim_tolerance_fillet_uv_main.jsonl`: remove the `fillet_point_step` key from its 3 rows (all 3 carry it).
   - `trim_tolerance.rs` `FILLET_UV_CHANGES`: update all **six** hashes (`old_hash` and `new_hash` for each of the three cases), not three.
   - `trim_tolerance_main.tsv`: update the three unit digests and add a `#` line saying why.

   This is not a CI-only test: `independent_golden_changes_cannot_hide_other_changes_or_tolerance_rows` is a quick, ordinary `#[test]` and fails as well. The steps are in F1.
2. **Leftover references to the latch.**
   - `solidcore/crates/solidcore/tests/wr_621_half_degree_crease_rounds.rs:172`: the doc says "(which leaves the solver's step latch unset)", but the assert it described was removed.
   - `solidcore/docs/failure-codes-and-reproduction.md:70`: lists "the fillet step latch" as state the kernel keeps between calls.
   - `docs/rfcs/0016-failure-signatures.md:68`: says the same. Optional, because it is a draft RFC.
3. **PR body: reconcile the issue's numbers.** The body says the issue's counts "match", but only two of the three do. On main, executive_p3 recorded 1e-3 in **10** rows, while the issue says 15. The body also never addresses the issue's "the rest move geometry by <= 1.1e-12" class, which came from the MEASURED survey in #17401. Add the mechanism argument from F2, which shows why only executive_p3's variable-radius cases could move at all.
4. **PR body: "Other units: ARGUED, left to CI".** Either cite #17401's all-unit survey (MEASURED, 283 units: with this flag on alone, only these three units moved), or run the units listed in F3. The "golden units the change can move" rule asks for one of the two.
5. **`Test change:` line.**
   - Give the reason, not only the list. For example: "the removed assertions and the golden `fillet_point_step` key observed the step latch, which this PR deletes with the flag's off path; every other expected value is unchanged."
   - Add the dropped latch assertion in `the_kernel_after_a_panic_is_configured_as_before` (`solidcore-ops/src/session/tests.rs`).
   - Also list the three dropped `fillet_point_step.get()` asserts in `p4.rs`, and the dropped `k2` half of `variable_radius_crossing_latches_small_step`, which is now renamed `..._reaches_step_solve`.
6. **Body hygiene.**
   - The body's list of removals leaves out `Kernel::latched_fillet_point_step`. The addendum does name it.
   - State the local gate that ran: fmt, clippy, `cargo test -p solidcore`, and the goldens of the three units, as solidcore/AGENTS.md §4 "Finish" asks.
   - The rustfmt commit comes after the expectations commit. Put the ledger update in the same package, so the expectations and ledger commit is the last commit.

## Findings, most serious first

### F1. Certificate ledger left stale (verified from code; the CI failure is inferred)
- `trim_tolerance.rs:1347-1371` pins each of the three `FILLET_UV_CHANGES` rows by FNV-1a 64 of its canonical row (`fnv1a64(to_canonical_string(v))`). `put_back_fillet_uv_changes` (`:1380`) checks the expected file's row against `new_hash`. All three rows lost their key in this PR, so every check fails, and the first error is "changed X, row Y".
- `trim_tolerance_fillet_uv_main.jsonl` holds main's original three rows. All 3 still carry `fillet_point_step` (`grep -c` = 3). `independent_golden_changes_cannot_hide_other_changes_or_tolerance_rows` (`:1444`) is a cheap ordinary `#[test]` with no kernel replay. It `expect`s `put_back` to succeed and then compares the result to this file, so it fails as well. The builder's open items do not mention this test.
- `the_rows_are_the_whole_golden_diff_from_main` (`:1563`) is also an ordinary `#[test]`, not `#[ignore]`. It runs under `cargo test -p goldens --lib` locally as well as in CI, so the claim that "it runs only in CI" is inaccurate. It walks the units in BTreeMap order, so it fails at `blend.fillet_executive_p3` first.
- The consistent fix:
  - Remove the key from the 3 rows of `trim_tolerance_fillet_uv_main.jsonl`, using the same edit as the expectations commit. Then set `old_hash` to the hash of each stripped original row, and `new_hash` to the hash of each row of the PR's expected file.
  - Set the three `.tsv` digests.
  - Leaving the originals with the key would make the restored executive file a mix of rows with and without it, which is wrong.
- **Regeneration needs no golden runs.** Every value is FNV-1a over canonical JSON of committed text:
  - **Six row hashes and the two simple digests:** write a throwaway local test (do not commit it) in `certify/trim_tolerance.rs` that prints:
    - `crate::rng::fnv1a64(&crate::json::to_canonical_string(&v))` for each row of the stripped `trim_tolerance_fillet_uv_main.jsonl`, and for the three cases in `expected/blend.fillet_executive_p3.jsonl`;
    - `expected_digest(&text, &[], &mut |_| Err(..))` for `expected/blend.fillet_intersector.jsonl` and `expected/blend.fillet_solver_p4.jsonl`. Neither unit has ROWS or put-backs, so its tsv digest is simply the digest of its expected file. The ignored `main_expected_digests` test does the same if `MAIN_EXPECTED_DIR` points at a directory holding just those two files: `MAIN_EXPECTED_DIR=<dir> MAIN_SHA=x MAIN_EXPECTED_OUT=<file> cargo test --profile goldens -p goldens --lib certify::trim_tolerance::main_expected_digests -- --ignored --exact`.
  - **executive_p3 digest:** it is computed after the FILLET_UV and ROWS put-backs. Once the hashes above are in, run `cargo test --profile goldens -p goldens --lib certify::trim_tolerance::the_rows_are_the_whole_golden_diff_from_main -- --exact`. It panics with `blend.fillet_executive_p3: <got>, main <want>`; `<got>` is the new digest. Re-run once more to confirm the other two. It replays only the blocked-key ROWS cases of the units before it, so it is not a full goldens run.
  - Then run `independent_golden_changes_cannot_hide_other_changes_or_tolerance_rows` and confirm it passes.
- **"Expect CI to fail" is not acceptable before ready.** "Validate, then publish" requires a complete package, and the "Red CI on a ready PR" rule exists because a red ready PR costs a cycle. The values can be computed deterministically and cheaply, so they belong in the PR now.

### F2. "Zero geometry rows moved" vs the issue's estimate (row identity verified; the mechanism argument is verified from code; the reconciliation with #17401 is inferred)
- **Rows (verified on all 2,736, not a sample):**
  - I parsed the merge-base (41e5136c21, identical to the current origin/main for these three files) and HEAD expected files. In every changed row, removing `fillet_point_step` from the base row gives exactly the HEAD row. A textual check agrees: a `sed` that strips the key from the base file is byte-identical to the HEAD file in all three units (`cmp` clean).
  - Changed rows: executive_p3 793/794, intersector 1292/1293, solver_p4 651/652. That is every row except the header, and it equals the body's 793 / 1292 / 651, total 2,736.
  - No row at HEAD still has the key.
  - Values the base rows recorded: solver_p4 has 124 at 1e-3 and 527 at 1e-5; intersector has 5 at 1e-3 and 1287 at 1e-5; executive_p3 has 10 at 1e-3 and 783 at 1e-5.
- **How the runners ran on main (verified):**
  - Every case of the three units runs on a fresh `Kernel::default()` or `crate::runner_kernel()`, both of which have the flag off. executive_p3's `rust()` calls `rust_with_fixes(.., &UpstreamFixes::default())`.
  - The latch was only *recorded* after the case (`k.fillet_point_step.get().unwrap_or(1e-5)`). It was never seeded on the Rust side.
  - So removing the recording hides nothing. The harness changes in the diff only remove the `with_point_step` / `rust_point_step` plumbing. `with_point_step` skipped deferred and rejected outputs, and its removal changes no other field.
- **When can flag-off and per-point steps differ?** The step is read only inside `if ops.offset_radii_can_change()? || point_type == TangentPoint` (`p4.rs:4026`). A constant-radius solver therefore uses the step only at tangent points, which take 1e-3 under both regimes. A difference needs a **variable-offset solver evaluating both a crossing and a tangent point on one kernel**. Per unit:
  - **solver_p4 `point_values` cases:** one `compute_point_values` call per case (`rust_point_case`), so there is one point and one step, and the latched step equals the point's own. `calc_geom` uses `fd: false`, a constant-radius solver. **Cannot move.**
  - **intersector:** the solver is `ConstantRadiusAssistedFs` with no fd switch (grep). Every FD point is a tangent point. **Cannot move.**
  - **executive_p3:** the 10 rows that recorded 1e-3 are all `stubs4-degenerate-*-r0.5` constant-radius fillets, which cannot move. Only the variable-radius cases (`attributed-variable-box-*`, `issue920-*-variable-constant-law-*`, `upstream-TestSmVariableRadiusFillet`, `radius-supported-variable-*`) could move, and only if they meet a tangent point after a crossing. The builder MEASURED that none did. I did not re-run it, but nothing argues against it.
  - So "0 geometry rows" is plausible, and for two of the three units it follows from the code.
- **The issue's numbers.** The issue's "~140 (124/15/5), 129 latch-only, rest <= 1.1e-12" comes from #17401's survey, which compared against the blessed set. The survey's moved solver_p4 *point* rows are impossible by the argument above. The likeliest explanation is that blessed lines lag a fresh run within the compare tolerance; `trim_tolerance.rs:1324-1330` documents exactly this lag. That explanation is inferred. Either way the PR body should say this. Its current line, "124 solver_p4 rows and 5 intersector rows recorded 1e-3, which matches the issue's counts", is silent on executive's 15 vs 10 and on the <= 1.1e-12 class (fix 3).
- **CI exposure:** none for these units. The goldens compare allows 1e-9 relative, so a 1e-12 move would pass against the committed rows anyway.

### F3. Other consumers of flag-off fillets (verified where stated)
- **App and rig: unaffected (verified).**
  - SolidCraft runs `UpstreamFixes::all()` (`crates/smlib/src/core.rs:75`).
  - The kernel rig sets every flag (`solidcore-ops/tests/kernel_cases.rs:1715` ff., `UpstreamFixes::NAMES`).
  - On main, the flag-on branch never read the latch or the pool's `STEP` cell (old `p4.rs:4023-4027`).
  - So solidcore-ops sessions, the smlib pool and the rig have identical results.
- **Goldens units that run fillets on default fixes and could mix point types.** These are the units with variable-radius fillets on one kernel:
  - `blend.fillet_custom_solver` (variable and law solvers), the most likely;
  - `blend.fillet_executive_p1` and `blend.fillet_executive_p2`;
  - `blend.feature_executive`;
  - `blend.fillet_corner_p3`;
  - `corpus.parity`.

  #17401's survey (MEASURED, all 283 units, at add215f56) found that the flag alone moved only the three units in this PR. That is good evidence that none of these move, but it predates many generic-blend changes. Either cite it in the body, or run these units: `cargo run --profile goldens -p goldens -- --unit blend.fillet_custom_solver --unit blend.fillet_executive_p1 --unit blend.fillet_executive_p2 --unit blend.feature_executive --unit corpus.parity --goldens`.
- **Kernel tests on `Kernel::default()` with variable rounds, which could move if they hit a tangent point:** `wr696_default_variable_rounds`, `wr646_variable_round_follows_law`, `wr300_variable_rounds_meet_at_law_radii`, `fillet_variable_radius_closes_onto_vertex`, `issue_2706_point_radius_law`, `fork_64e5632_variable_round_definitions`, `wr685_*`, `variable_round_law_corner_refused`. `cargo test -p solidcore` settles them, and the body does not say it ran.

### F4. Leftover references (verified by `git grep` at HEAD)
- Code and docs (fix 2):
  - `tests/wr_621_half_degree_crease_rounds.rs:172` (doc of the test whose latch assert was removed);
  - `solidcore/docs/failure-codes-and-reproduction.md:70`;
  - `docs/rfcs/0016-failure-signatures.md:68`.
- Under `solidcore/plan/`, merged records that are append-only and should **not** be edited (the new addendum supersedes them):
  - `ND-3--a-function-local-static-fixed-by-the-first-call.md:10`;
  - `ND-3--addendum-2026-10-27-*`;
  - `ND-3--addendum-2026-10-31-*`;
  - `findings/blend.fillet_solver_p4--harness.md:12` (describes seeding `Kernel::fillet_point_step`);
  - the corner_p3, b47 and geom.local_solve_nd findings;
  - `closure.md:778` (it describes upstream's static, which is correct).
- No remaining hits in code for `fillet_point_step`, `latch_fillet_point_step`, `process_fillet_point_step`, `latched_fillet_point_step`, `fillet_point_step_per_point`, `static_step` or `sdStaticStep`. The only hits are in new explanatory comments in `p4.rs` and `kernel.rs`.
- Nothing in `fixes.json`, `goldens/src/solidcraft.rs`, `*.toml`, `*.py`, `*.sh` or `*.yml`.
- The flag field disappears because `build.rs` generates the fields from `src/kernel/fixes/*.md`, and the PR deletes that file.
- `crates/smlib` has no preset line to delete: the preset is `UpstreamFixes::all()`. This is correct, so the brief's "preset line" item is moot.

### F5. The new regression test (verified by reading; "fails on main" is inferred)
- `the_step_is_the_points_own_whatever_the_kernel_evaluated_before` sits in `p4.rs` `mod tests_c6`, an existing module of the right area. It uses no removed API, so it compiles against main.
- What happens on main with the flag off:
  - On the first kernel, a crossing (variable solver, floor against wall) latches 1e-5. The tangent point that follows takes 1e-5, so the ratio check `tangent / crossing == 100` fails.
  - On the second kernel, a tangent point latches 1e-3 first, so the crossing that follows gets 1e-3, and `assert_eq!(.., crossing)` fails.
  - This reproduces history dependence for the stated reason. The first guess is recorded before the retry with the bigger step, so that retry does not mask it.
- It is a unit-level test of `ComputePointValues`, not a whole fillet run twice. That is adequate for this mechanism.
- There is no test that the pools no longer share a latch. That is enforced only by compilation, because the fields are gone, which is sufficient.

### F6. The removals: no second job lost (verified)
- `smlib::core::start_clean` keeps resetting the rng, the station memo and `upstream_ub_passed`.
- `session::fresh` still sets the fixes.
- `cache_kernel` still copies the fixes and the memo flag.
- `kernel.rs` drops the `OnceLock` import. `Cell` is still used by the counters (`:181`), and `session.rs` still needs `OnceLock` (`:209`).
- The session panic test keeps its other three checks.

### F7. Proof package and labels
- **Counts:** the body's per-unit counts and total equal the diff (F2).
- **Commits:** there is one expectations commit, 51f0e5702b, but the rustfmt commit 79c5ade77f comes after it (fix 6).
- **Labels:**
  - "Key removed: PROVEN by the compiler" is acceptable. It would be more precise to cite the goldens compare as the assertion.
  - "Bit-identical: MEASURED" is labelled correctly.
  - "Issue counts: ARGUED" is labelled correctly but its content is incomplete (F2).
- **ND-3 addendum:**
  - `plan/upstream-issues/ND-3--addendum-2026-11-22-flag-removed-fillet-point-step-per-point.md` is a new file with the `Updates: ND-3` header. It follows the WR-621 addendum's naming and creates a new file rather than editing the record.
  - It contains no `#NNNN`, no local paths and no concurrency motive.
  - Its counts match the diff.

### F8. Body and hygiene (verified)
- The body has `Changes a default: no` exactly, `Fixes #18916`, and a `Test change:` line in plain text (its content needs fix 5).
- No local paths and no session link in the body or the commits.
- Searching the added lines, the commit messages, the body and the addendum for thread, parallel, concurren, "side by side", `/home` and `claude.ai`:
  - No motive appears anywhere.
  - The only hits are the deleted test name `calls_side_by_side_latch_one_step`, which the body names as a removed test, and deleted doc lines in `session.rs`.

## File coverage (25/25)

| file | checked |
|---|---|
| crates/smlib/src/core.rs | module doc trimmed; `start_clean` loses only the latch reset; preset is `all()` |
| goldens expected blend.fillet_executive_p3.jsonl | 793 rows, all key-only (parsed and byte-checked) |
| goldens expected blend.fillet_intersector.jsonl | 1292 rows, all key-only |
| goldens expected blend.fillet_solver_p4.jsonl | 651 rows, all key-only |
| goldens units blend_fillet_executive_p3.rs | `POINT_STEP`, `rust_point_step`, `with_point_step` removed; `rust_with_fixes` returns `output()`; default fixes |
| .../attributed.rs | `match run()`, no other change |
| .../end_to_end.rs | 4 sites; checkpoint prefix path preserved |
| .../preview.rs | doc and return value only |
| .../surface_bodies.rs | `match run()` |
| .../two_offset_chamfer.rs | `output()` directly |
| goldens units blend_fillet_intersector.rs | constant-radius solver only; key and stale C++ comments removed |
| goldens units blend_fillet_solver_p4.rs | one call per point case; key removed |
| solidcore-ops/src/session.rs | `STEP` and the doc bullet removed; `fresh` still sets fixes; `OnceLock` still used |
| solidcore-ops/src/session/tests.rs | two latch tests deleted; panic test loses one tuple field (add to the Test change line) |
| solidcore/src/blend/fillet_solver/p4.rs | step is per point; docs updated; c6 test asserts trimmed; new test (F5) |
| boolean/merge/p4/kernel_adapter.rs | two field copies removed |
| solidcore/src/kernel.rs | fields, methods, table row, tests and `OnceLock` import removed; new module-doc paragraph is accurate |
| kernel/fixes/fillet_point_step_per_point.md | deleted, which removes the generated field |
| tests/issue_18166..., issue_1945..., issue_2020..., issue_2260..., wr650... | one flag entry each; lists stay sorted |
| tests/wr_621_half_degree_crease_rounds.rs | assert removed; doc at :172 is stale (F4) |
| plan/upstream-issues/ND-3--addendum-2026-11-22-... | new file, rules met (F7) |

## Could not verify without a build
- That the new test fails on main: apply it to main's `p4.rs` and run `cargo test -p solidcore --lib blend::fillet_solver::p4::tests_c6::the_step_is_the_points_own_whatever_the_kernel_evaluated_before`.
- That the branch's goldens output is byte-identical to main's (MEASURED by the builder): `cargo run --profile goldens -p goldens -- --unit blend.fillet_executive_p3 --unit blend.fillet_intersector --unit blend.fillet_solver_p4 --goldens`. Only executive_p3's variable-radius cases can differ (F2).
- Other flag-off units and tests (F3): the goldens command listed there and `cargo test -p solidcore`.
- The new ledger values (F1 commands).
