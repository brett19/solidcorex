# Review of tangency-labs/solidcraft#20942 (head 022602ee51), fixes #20936

**Verdict: PASS WITH FIXES**

1. **Golden coverage is wrong, and the most exposed unit was not run.** `topology.shell` calls `is_inner_shell_of` directly (`goldens/src/units/topology_shell/inner.rs:82,85`), many times on one kernel after Booleans. It also calls `order_region_shells` directly (`topology_shell.rs:1228`). It is not among the 28 units. Run it, plus the units listed under F2. Then correct the body's sentence "the 28 units whose runners reach `points_in_face`, `is_inner_shell_of` or their callers".
2. **Share one generator per `is_inner_shell_of` call across its face uses, or defend the per-face restart in the body** (F1). The same applies to `topology_traverser::is_inner_shell`. The fix is small: a crate-private `points_in_face` variant that takes `&mut Rng`.
3. **Run the app and ops tests that can reach a draw:** the whole `solidcore-ops` suite and `cargo test -p solidcore-ops --test kernel_cases -- --ignored`. Also `nextest -p solidcraft-smlib`: `crates/smlib/src/core/convert.rs:2658` calls `is_inner_shell_of`, and coord rules require the app tests for a solidcore API change.
4. **Make smlib `every_call_starts_clean_on_any_kernel` test something again** (F3). Use a real draw site, as the ops test does, instead of `k.seeded_rng().next_f64()`, which is constant by construction.
5. **Fix the "threads" wording in added lines of `solidcore-ops/src/session.rs:157-158`.** The added text reads "side by side on several threads give what they give one after another". It is an old sentence carried into `+` lines by a re-wrap. Either keep the original lines untouched or drop the clause.
6. **Add to the body: an UNSURE body is now UNSURE on every call** (F4). The ops path already gave a fixed answer per call on main.

Files reviewed: 19/19.

---

## Findings, most serious first

### F1 (medium, design and quality): every face use of a shell now gets the same three dithers
`topology/shell.rs:1759` calls `points_in_face(k, b, f, 3)` once per face use. `points_in_face` makes `k.seeded_rng()` at `p2.rs:1724`, so each face use restarts the seed's sequence and gets dithers d0, d1, d2 at the same relative (u, v) slots.

On main, a call that was first on its kernel drew d0..d2 for face use 0, d3..d5 for face use 1, and so on. That covers every `solidcore-ops` call whose first draw site is this one, because `start_clean` reset the generator.

The same holds in `topology_traverser::is_inner_shell` (`topology_traverser.rs:782`, 6 or 12 samples per face use).

- **Which choice the builder made, and whether it is stated.** One generator per face use (a restart), not one per call. The body says "Each call of `points_in_face` and of `is_inner_shell_of` starts a generator at the kernel's seed". It does not say that `is_inner_shell_of`'s face uses therefore all sample at the same offsets. Its argument against mixing in ids ("depend on the order in which the B-rep was built") does not exclude a call-level generator.
- **Failure scenario.** Take a symmetric shell: a cubic cavity centred in a body with features repeated under the cube's symmetry (mirrored or patterned pockets). Its six face uses are sampled at identical relative positions. If the three rays of one face meet an edge, the symmetric rays of every other face do too, and the answer is UNSURE.
  - With independent dithers per face (main, first call), all six faces would have to be blocked: probability about p^6 instead of p.
  - Under this PR that UNSURE is deterministic, so `check_brep` and `assert_valid` fail on every call for such a body. This is a quality regression for multi-face shells, not a correctness bug. A single-face cavity, which is the issue's body, is unaffected.
- **Better option, still a pure function of the inputs.** Make one `Rng` per `is_inner_shell_of` call and draw every face use's dithers from it in face-use list order. That order is part of the B-rep value and already decides which face is tried first, so it adds no new dependence. On a fresh kernel, and on the ops path where this is the first draw, it reproduces main's samples exactly. A sub-seed from the face-use ordinal would be equivalent.
- **Ordinal-only sub-seeds** such as `seed ^ count` give nothing for one face.
- **Do not use face or entity ids**, for the builder's reason.

### F2 (medium, evidence): the golden and test coverage claim is incomplete
Every runner makes its own `Kernel::default()` per case (`goldens/src/lib.rs:61-66`), but a case is not one draw. Booleans draw inside the case:
- region regrouping (`brep/p3.rs:3943 regroup_split_region`);
- `order_region_shells` (`shell.rs:424`);
- `split_closed` (`brep/p6.rs:1616`);
- `merge_shells` (`brep/p7.rs:693`);
- the enclosing-shell sort (`brep/enclosing.rs:42`);
- the glue's `test_closure_split_region_attempt`;
- `assert_valid`'s inner-shell test (`validation/assert_valid/brep.rs:164`).

Any draw after the first in a case now gets different samples. "No row moved" in the 28 run units is MEASURED by the builder and plausible: a sample change only matters when a ray meets an edge. But these units can reach a draw and were not run:

- **`topology.shell`**: a direct caller of `is_inner_shell_of` and `order_region_shells`, with several calls per kernel. This is the unit most likely to move.
- `heal.stitch_p2`. Stitch results depend on the dither: `plan/upstream-issues/ND-1--addendum-2026-11-01-stitch-closure-split-confirmed-on-bull.md`.
- `topology.brep_p3`, `topology.brep_p4`, `topology.brep_p6`, `topology.brep_p7`, `topology.brep_planar_section`, `topology.api_files`, `topology.brep_constructor`.
- `boolean.merge_p4`, `boolean.topology_intersector_p1`.
- `offset.offset_executive_p1`, `sweep.api_brep`, `sweep.topology_sweep`.
- `blend.fillet_executive_p1`, `blend.fillet_executive_p2`, `blend.fillet_executive_p3`. The fillet cone stream in ND-493 depended on draws.
- Outside goldens:
  - the `solidcore-ops` kernel cases (`--test kernel_cases -- --ignored`) and the rest of the ops tests (only `session::` and `body::` were run);
  - `solidcraft-smlib` (the convert.rs caller);
  - the `solidcore` integration binaries other than `offset`. `cargo test -p solidcore` is the rule; the body lists only fmt, clippy, panic sites and wasm, so it is unclear whether the full `-p solidcore` suite ran.

Fallback that settles it: `cargo run --profile goldens -p goldens -- --all --jobs 0 --goldens`.

### F3 (low-medium, test strength): the smlib pool test became a tautology
At `crates/smlib/src/tests.rs:802`, `k.seeded_rng().next_f64()` is a function of `k.seed` alone and nothing moves it. The test now only checks that pool kernels share a seed, and its doc ("the same draws of random rays") overstates it. The ops counterpart (`session/tests.rs:79 sampled`) goes through `points_in_face`, which is the right model.

`a_solid_test_of_an_operand_moves_no_random_draw` is now trivially true (no shared state is left to move). It is harmless as a guard and needs no change.

### F4 (low, body): what a user sees for an always-UNSURE body
**Verified:** the cavity body's fixed TRUE holds with margin.

**Inferred** from the code and a Python replay of `Rng`'s SplitMix64 at `Rng::DEFAULT_SEED`:
- The first three dithers are +0.4036, +0.3228 and -0.2892. The bound is ±0.4167.
- On the sphere the three samples are one per iso-u meridian, with `v_norm = (1 + d)/2`.
- Assuming v is close to linear in latitude, the ray slopes |tan(lat)| are 0.735, 0.556 and 0.488.
- The notch-edge thresholds `h/6` for the opposite notches are 0.390, 0.412 and 0.435.
- So all three rays clear the edges and the first decides by a wide margin. This matches main's measured 25% UNSURE rate (about p = 0.63 per ray, a cutoff near |d| ≈ 0.26).

A nearby body is UNSURE for ever: the same body with notch half-heights of about 4.45 instead of 2.27 blocks all three fixed rays. Main failed that body on roughly 94% of calls, the PR on 100%. Both are "Inner shell is not geometrically inside its containing outerShell" from `check_brep`, so the user gets an invalid-body report or a refused operation.

That is no worse in kind:
- through `solidcore-ops`, main already gave one fixed answer per call (`start_clean`);
- repeatable failure is the issue's goal, and better than intermittent failure.

The underlying weakness, three rays and then UNSURE, is upstream's and out of scope. The body should still say plainly that "a body that is UNSURE is now UNSURE on every call", and a follow-up issue for the inner-shell test's robustness is worth filing.

### F5 (low, wording rule): "threads" in added lines
`solidcore-ops/src/session.rs:157-158`, as added: "so calls side by side on several threads give what they give one after another". It was carried from main by a re-wrap, but it is in the diff. There are no other hits for thread, parallel or concurrency in the added lines, commit messages or body. There are no local paths or session links, and no `#NNNN` in the added code or docs.

### F6 (low, process): the test rewrites sit in the code commit
"Validate, then publish" asks for the code with the original tests first, then ONE expectations commit. Here the 8 rewritten tests are inside 7bbdf11e9b. That is unavoidable in part, because the old tests use the removed `Kernel::rng` and cannot compile. The body should say so in one line.

Commit 022602ee51 (`let mut k1`) is a compile fix to that commit's test. The builder's test runs must therefore have been on a later state; confirm.

### F7 (informational): the pool no longer guards the seed
`start_clean` used to force `Rng::new(DEFAULT_SEED)` whatever the kernel held. Now a closure that wrote `k.seed` would leak to later calls on that pooled kernel. Nothing writes it today: the only writes are the scratch copies `tk.seed = k.seed`. Optionally reset `k.seed = KernelOptions::default().seed` in both `start_clean`s, or leave it as is.

### F8 (informational): stale `kernel.rng` in plan records
`plan/findings/general--general.md` and several `plan/upstream-issues/*` files still mention `kernel.rng`. Records are never edited and a PR touches no `plan/` file, so no action is needed in this PR.

---

## Checks that passed

1. **Correctness of the fix (verified).** The only draw sites in non-test kernel code are `p2.rs:1687 rand_dither` and `shell.rs:1791-1807`, by grep for `next_f64` / `next_u64` / `next_signed_unit`. Each now draws from a call-local `k.seeded_rng()`. No reads of `k.rng` remain anywhere in `solidcore/` or `crates/` (git grep). For wire and vertex shells, `cnt = 1`, so one generator per call is exactly one per sequence. Each call is a pure function of the B-rep, the shells or face, the count and the seed. The order-dependence question is F1.
2. **Removed machinery (verified).**
   - Glue (`api_brep.rs:528-544`): it still saves and restores the snapshot and `upstream_ub_passed`; only the rng was dropped. Its test still asserts the fingerprint and the UB list.
   - ops `start_clean`: still clears `blend_station_memo`.
   - smlib `start_clean`: still clears `blend_station_memo`, `fillet_point_step` and `upstream_ub_passed`.
   - ops `undrawn` restored only the rng.
   - `Expect::from_proof` (`body.rs:567-574`) keeps the same short-circuit and error propagation.
   - The five scratch kernels copy the seed.
   - Public API: `Kernel::rng` is gone and `Rng` stays exported. The only consumers were in smlib and ops, both updated. There is no `rng:` struct literal elsewhere.
   - `KernelOptions::seed` is still observable: the seed-7 branch of `points_in_face_repeats_its_points_whatever_was_sampled_before`, and `every_seeded_rng_starts_at_the_kernels_seed`.
3. **Tests (read; the revert was not re-run by me).**
   - The three integration tests build the issue's exact body: a 20 mm cube, an r=2 ball cavity, and notches whose corner edges sit at r=6 on the sample azimuths.
   - Reasoned failure on main: each call drew new dithers, and about 25% of calls had all three rays below the edge slope.
   - Placement: `tests/issue_1473_inner_shell.rs` is an existing file and already the home of the `IsInnerShellOf` regressions, so adding to it does not break "never a new per-bug file". It compiles into the `offset` binary (`tests/offset/main.rs:50`) because that binary groups "shells" by name, a pre-existing misfiling. The `topology` binary would be the better home, but that is not required here.
   - Rewritten tests:
     - `p2` (2 tests): stronger. They now assert every point equals its grid slot plus dither n, instead of a draw count.
     - `kernel` (1 test): equivalent.
     - `api_brep` (1 test): the rng assertion was removed; the UB-list and fingerprint assertions stay.
     - `issue_2015` (1 test): `k_raw.rng.next_u64() == k_typed.rng.next_u64()` was removed and not replaced. It is moot, since no state is left.
     - ops (2 tests): still meaningful (`sampled`).
     - smlib (1 test): weakened (F3).
4. **Body.**
   - `Fixes #20936` is present.
   - `Changes a default: no` is exact (a kernel result).
   - The `Test change:` line is present and its reason is valid (the API no longer exists).
   - Labels: the "PROVEN for the rectangular branch" claim is backed by committed assertions (correct). The cavity's TRUE is ARGUED (correct). The goldens and revert results are MEASURED.
   - Mislabels: none. The one false claim is the coverage claim (F2).
5. **Scope.** Nothing outside the stated scope. The doc edits (AGENTS.md, agent-style.md, module docs, the goldens unit's doc) only rename the source of randomness.

## Files in the diff

| File | Checked |
|---|---|
| crates/smlib/src/core.rs | rng reset removed; the other resets kept; doc updated |
| crates/smlib/src/tests.rs | test now tautological (F3) |
| solidcore/AGENTS.md | rule text only; accurate |
| goldens/src/units/topology_face_p2.rs | doc only; the unit still records only draw-independent values (could now record full points, optional) |
| solidcore-ops/src/body.rs | `undrawn` removal; `from_proof` semantics unchanged |
| solidcore-ops/src/session.rs | reset removed; "threads" in re-wrapped `+` lines (F5); seed not guarded (F7) |
| solidcore-ops/src/session/tests.rs | rewritten through `points_in_face`; meaningful |
| blend/fillet_executive/p1.rs | seed copy |
| blend/fillet_solver/p1.rs | seed copy plus comment |
| blend/fillet_solver/p2.rs | seed copy |
| boolean/api_brep.rs | glue keeps its snapshot and UB list; test keeps the UB and fingerprint asserts |
| boolean/merge/p4/kernel_adapter.rs | `cache_kernel` copies the seed |
| kernel.rs | `seed` field, `seeded_rng` (`const`, `&self`), docs, tests; commit 3 is a compile fix |
| topology/edgeuse/p3.rs | seed copy |
| topology/face/p2.rs | per-call rng; branch dithers unchanged; tests stronger |
| topology/shell.rs | per-call rng for wire and vertex rays; face uses restart via `points_in_face` (F1) |
| tests/issue_1473_inner_shell.rs | issue body; geometry and margin checked (F4); placement acceptable |
| tests/issue_2015_helpers.rs | rng assert removed; result assert kept |
| docs/agent-style.md | wording |

## Could not verify without a build

- **Golden units not run** (F2): `cargo run --profile goldens -p goldens -- --unit topology.shell --goldens`, and likewise for each unit listed in F2, or `--all --jobs 0`.
- **ops kernel cases:** `cargo test -p solidcore-ops --test kernel_cases -- --ignored`, and `cargo test -p solidcore-ops`.
- **App tests:** `cargo nextest run -p solidcraft-smlib`.
- **Revert:** that the 4 new tests fail on the base. I reasoned it from the code and it agrees with the issue's 30/10 measurement, but I did not run it.
- **Exact dither-to-latitude mapping** on the rational sphere (F4 margins). Settle it by printing `points.pv` in `sample_ray_azimuths` on the default seed.
- **F1 effect size:** run `is_inner_shell_of` on a symmetric multi-face cavity body, once per face-use restart and once with a call-level rng.
