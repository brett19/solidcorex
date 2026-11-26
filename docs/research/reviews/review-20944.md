# Review of tangency-labs/solidcraft#20944 (head ed9b77d369, fixes #20938)

**Verdict: PASS WITH FIXES**

1. **Measure and state the same-body change (major, needed before ready).** The six moved readers now read the body's own estimate. On main they read whatever was last written to the context, and every `new_brep` with `USE_DEFAULT` (nearly every operation) reset it to the default. So for a body that stores its own estimate (scaled past the 0.5..500 band, or given an estimate by heal), any operation that comes after another body was created now sees, for example, 1e-4 where main and upstream saw 1e-5. Neither the inventory nor the tests cover this direction. The body only argues the cross-body direction. Add a test of the history (scale Y by 1000, create another body, then heal Y / Boolean Y), compare it against main, and add the results to the body. If any result moves, it needs the same proof as the goldens.
2. **Relabel the goldens claim.** "PROVEN mechanically from each row's own columns" is a check over a diff, which is MEASURED. What can be PROVEN is narrower: "the new values are the defaults", by the setter signature (it no longer takes `&mut Kernel`) plus the committed unit-test assertions. Say which part is which.
3. **Stale module doc.** `solidcore/crates/solidcore/src/topology/vertex/corner.rs:27-29` still says the corner approximation tolerance is `context_approx_tol_3d(k, Some(&k.context), ..)`, "as the B-rep constructor reads it". Both sites now use `brep_size_zone_tol_3d`.
4. **Evidence gaps in the body.** Name the one unit that failed in the 282/283 full goldens run, and say why it is unrelated. List the two rewritten unit tests in the expectations commit's message or table. They sit in the code commit 26cbcf9, not in the single final expectations commit ed9b77d. Merge-commit policy forbids rewriting a pushed branch, so documenting this is enough.

Files reviewed: 12/12.

## Findings, most serious first

### F1 (major, evidence): same-body result change not inventoried

Upstream reference (`upstream/source/SMLib/inc/SmBrep.h:723-732`, `src/SmBrep.cpp:519-520`, `src/SmContext.cpp:201-219`):
- The B-rep setters forward to the B-rep's context.
- The `SmBrep` constructor calls both setters, so every new B-rep resets its context to the default.
- Upstream therefore has the same cross-body leak among the B-reps of one model context.
- Main was a faithful port of that (one `k.context`).

The PR deliberately departs from it. That is the right design for #13672 and #20938, but it changes two things, not one:
- (a) **Cross-body** (the fix): X no longer sees Y's estimate.
- (b) **Same-body** (unstated): a body Y that stores a non-default estimate. On main, the context held Y's estimate only until the next `new_brep(USE_DEFAULT)`. Callers verified to pass `USE_DEFAULT`: `boolean/merge/p3.rs:988,1211`, `topology/brep/local_operation.rs:326`, `topology/brep/planar_section.rs:880`, and the primitive constructors. After that, every moved reader returned the default zone tolerance for Y. With the PR they return Y's own value (for example 1e-4 for a 10-cube scaled by 1000).

The commit message says this much: "In a history where the body itself last wrote the context this is the value they read before". That is true, but other histories are the common case, and the PR body does not mention them.

Concrete scenarios (inferred; not run, because I cannot build):
- **Heal of a scaled body** (app `crates/kernel` repair / tolerance_heal → heal), with Y scaled by 1000 and any body created since:
  - `EdgeProps` at `heal/heal_data/p2.rs:508-519` and `VertexProps` at `:753-764` take `def_zone_tol_3d` = 1e-4 now, 1e-5 on main.
  - `FaceProps` at `heal/face_props.rs:1867` likewise.
  - The edges' own tolerances are 1e-4 after the transform (`p5.rs:3712-3725`). On main they could therefore be flagged `bad_large_zone_tol_3d`; with the PR they are not.
  - The healed tolerances may end the same, because `fix_tol_sizes` uses `brep_calc_*`, which reads `b.tolerance`. The flags, the problem lists and the heal report can differ.
- **Boolean with a scaled operand**, merged in place (the result is an operand; a `Brep` clone carries `model_size_estimate`, `topology.rs:910`). `face_corner_to_move_to` (`boolean/merge/p1.rs:933`) → `calc_corner_from_faces` (`corner.rs:342`) switches to its parallel-face branch at `max_parallel_face_gap > approx_tol_3d`. That threshold is now 5e-5, against 5e-6 on main. This only matters for a vertex outside xsect tolerance with nearly parallel faces, so it is rare but possible.
- **Crease split** (`topology/brep/p8.rs:920`, through `trimming_tools.rs:335,658`) on a scaled body: the split tolerance becomes 1e-4.

Coverage: I found no golden unit, kernel case or integration test that scales a body past the 500 diagonal and then heals it, takes a Boolean of it or splits it.
- Every scaled `transform_matrix(.., true)` call in `solidcore/crates/solidcore/tests` is a rotation (tol_scale 1, so no estimate is set). Checked: wr707:406, issue_2656:795/1037, wr696:140, issue_2826, contact_between_bodies, fillet_round_failure_notes_status.
- The `issue_1570`/`wr507` scales do no follow-up operation.
- The kernel cases `cov-scale-*` scale by 2 on small bodies, so the zone tolerance is unchanged.

So the 19 units the builder chose, and even the full goldens run, would not exercise (b). Its absence from the moved rows is not evidence that nothing changed.

### F2 (minor, labelling): PROVEN used for a diff check
The PR body says "(PROVEN mechanically from each row's own columns)". The rules ("Validate, then publish" 5; Proof-package self-check 5) require a committed assertion for PROVEN.
- The row-by-row "old equals the value just set" is MEASURED. I re-ran it myself, see the table below.
- "The new value equals the configured default" is effectively PROVEN by the type signature (the setters take no kernel) together with `p1.rs:4283-4289` and `tol.rs:1455-1469`.

### F3 (minor): stale doc in `corner.rs:27-29` (see fix 3)

### F4 (minor, process)
- The unit-test assertion rewrites are in the code commit, not in the one final expectations commit.
- The claim commit eeba6ff carries only a `Co-Authored-By: Claude Fable 5.2` trailer. Other commits have the required Opus 5.5 trailer.
- Neither point is worth rewriting history for.

### Checked and clean
- **Heal within one call (item 6).** After `fix_tol_sizes` sets the estimate (`p2.rs:3066-3075`), the rest of that function uses `brep_calc_face/edge/vertex_zone_tol_3d`, which read `b.tolerance` and not the context.
- **Later heal passes.** Prop recomputation on main read `context = this_model_size` when a set happened. With the PR it reads `b`'s stored value, the same number. When no set happened (this_est == brep_est), the results diverge only in F1's histories.
- **The `this_est == context_est → USE_DEFAULT` branch** compares with the configured default, which is right.
- **No missed reader.** No path computes for a body with a body in hand and still reads the context: see the table. All remaining `k.context` scalar reads are configuration reads or memo/proof keys.
- **Upstream sanity.** The upstream setters and constructor shown in F1 are as the PR says, apart from the "one model's" framing.

## Item 1: every reader of the context's three scalars

| Site | What it reads | Class |
|---|---|---|
| `topology/brep/p1.rs:203` `Brep::this_model_size_estimate(k,true)` | context estimate for a default-storing body | configuration, correct |
| `topology/brep/p1.rs:217` `this_large_small_size_ratio(k,true)` | context ratio | configuration, correct |
| `geom/tol.rs:357` `context_model_size_estimate` → `heal_data/p2.rs:3058` | configured default, compared with the body's | configuration, correct (F1 does not apply) |
| `geom/tol.rs:393,412,426,587,597,618` context-form tol fns | only callers with `Some(&k.context)`: `curve_cache.rs:601`, `brep_size_zone_tol_3d`, goldens `geom_context` (own contexts), `tol.rs` tests; all others pass `None` (system) | configuration |
| `curve/curve_cache.rs:601` fallback approx tol (curve without owner tolerance) | context zone | configuration, correct (no body in hand) |
| `geom/tol.rs:672` `brep_size_zone_tol_3d` (new) default branch | context zone for a default-storing body | configuration, correct |
| `heal/face_props.rs:1867` | was context, now `brep_size_zone_tol_3d(b)` | body's own (F1(b)) |
| `heal/heal_data/p2.rs:508,512` EdgeProps | was context, now body | body's own (F1(b)) |
| `heal/heal_data/p2.rs:753,757` VertexProps | was context, now body | body's own (F1(b)) |
| `topology/brep/p8.rs:920` crease split | was context, now body | body's own (F1(b)) |
| `topology/vertex/corner.rs:342` corner approx tol | was context, now body | body's own (F1(b)); doc stale (F3) |
| `topology/brep/p3.rs:669` `new_brep` tolerance | was context after self-write, now body | body's own, identical on main except a caller-configured context (tests only) |
| `curve/curve/p5.rs:413-418` `CurveMemoKey::of_kernel` | bits of all three | memo key; now constant unless configured (more hits, never wrong) |
| `query/brep/ray/live.rs:129-134` `FaceCacheKey` | bits of all three | memo key; same |
| `solidcore-ops/src/body.rs:84-93` `Outward` `Settings` | bits of all three | proof key; see item 5 |
| `geom/context.rs:542-544` `metadata_copy` (Boolean cache kernels) | copies all three | copy; correct |
| `blend/fillet_executive/session/insertion/live.rs:747-785` | test of a configured context surviving a fillet | test; still holds (no B-rep op writes the context now) |
| goldens `topology_brep_p1.rs:1100-1107` | context after a setter | the 141 moved rows |
| goldens `geom_context.rs` | its own `Context` values | configuration, unchanged |
| `crates/smlib` (`sweep.rs:482`, `measure.rs:193`), solidcore-ops (`surface.rs`, `curve_plane.rs`, `curve_fit.rs`, `sweep.rs`, `network.rs`), `api_queries.rs`, `hermite_curve.rs`, `ellipse/p2.rs`, `edgeuse/p1.rs`, `brep/sheet.rs` | `context_*_tol_3d(k, None, ..)` = system tolerance | not a context read |

Missed cases: none (every computation that has a body in hand now reads the body's value). Because of that, F1(b) is a result change by design, not a missed reader.

## Item 2: same-body traces
- **Boolean of scaled Y:** reaches `calc_corner_from_faces` through merge (F1). New value: body's 1e-4. Main: 1e-5 after any intervening `new_brep`, Y's value if none.
- **Fillet of Y:** reaches no moved reader directly that I found, other than `new_brep` for temporaries, which are default-storing, so unchanged. Inferred.
- **Mass properties:** no moved reader.
- **`check_brep` / AssertValid:** vertex test 1 goes through `assert_valid/seams.rs:169` (gap against `vertex1_tolerance`) and does not call `calc_corner_from_faces`, so it is unchanged. Verified by reading.
- **Cache builds (`of_kernel`, `FaceCacheKey`, `cache_tolerances`):** the keys only become more stable; the cached builds read no moved reader.
- **Heal of Y:** changed (F1).

Settled only by new tests; there is no existing one, see F1.

## Item 3: moved rows (verified by script over `git diff origin/main...HEAD`)
- 141 rows changed, all in `topology.brep_p1.jsonl`, the same 141 case names on both sides.
- No change in `error`, `ints`, `topology` or `deferred`.
- Classes:
  - 96 `set_est` rows move out[4] and out[5].
  - 15 `set_est` rows move out[4] only (the body's zone tolerance is already 1e-5).
  - 30 `set_ratio` rows move out[2].
  - These match the body's 96/15/30.
- The claim was checked on all 141 rows, not a sample:
  - `set_est` rows: old out[4] == out[2] (the stored estimate) and old out[5] == out[0] (the returned zone tolerance); new = 50.0 and 1e-5.
  - `set_ratio` rows: old out[2] == out[0]; new = 1000.0.
- The body's own columns are unchanged in every row.
- Samples checked by eye: `cone_set_est_0.3`, `toruswvy_set_est_1e-09`, `box_with_large_fillet_set_est_3000000.0`, `sphere_set_est_0.0` (0.0 → 50/1e-5), `Endcap_set_est_-3.0`, `surf_rev_set_est_100000.0`, `network_set_est`, `wire_line_set_est`, `cone_set_est_0.001`, `vase_rev_set_est_0.3`, `surf_extrude_set_est_100000.0`, and 13 `set_ratio` rows (2.5 and 1e6 → 1000).
- One expectations commit (ed9b77d) carries the table. Labelling issue: F2.

## Item 4: tests
- **Rewritten tests are re-pointed and strengthened, not weakened.**
  - `p1.rs` size-setter test: still asserts the returned tolerance and `b.tolerance()`. The context's 1000 / 7.0 become `USE_DEFAULT` plus zone 1e-5. Added: a defaulted B-rep follows a caller-set context, and `b` keeps its own value.
  - `tol.rs::brep_model_size_estimate_and_ratio_chase_the_context`: the same, plus `brep_size_zone_tol_3d` and `brep_size_calc_gap_zone_tol_3d` on `b` and on another B-rep.
- **New tests**, in `tests/wr507_508_transform_matrix_tolerances.rs`, the existing transform-tolerance file (placement OK):
  - `scaling_one_body_leaves_the_size_estimate_of_another` is the issue's exact case: a 100×100×10 X and a 10-cube Y scaled by 1000 with `scale_tolerances = true`. On main it would fail at `assert_eq!(of_x(&k), x_before)`, since the estimate is 17320.5. It uses only APIs that exist on main.
  - `healing_a_body_after_scaling_another_is_as_on_a_fresh_kernel`: on main, `fix_tol_sizes` sees the leaked 17320 and stores 50, so it fails at the comparison with `fresh`.
  - Revert-and-run is the builder's MEASURED claim. I could not run it.
  - Neither test covers F1(b).

## Item 5: scope and API
- **Public API change:** `Brep::set_this_model_size_estimate` / `set_this_large_small_size_ratio` and `geom::tol::set_brep_*` drop `k`. Added: `brep_size_zone_tol_3d` and `brep_size_calc_gap_zone_tol_3d`.
- **Consumers:** a repository-wide grep over solidcore, solidcore-ops, solidcore-convert, the other solidcore crates, goldens, examples, `crates/` and `crates/smlib` finds no other caller. All updated.
- **The geom_tol runner's `mut` removal** follows from the API change (otherwise `unused_mut` fails clippy `-D warnings`). It is in scope.
- **`Outward` `Settings`:** its context component is now constant unless the caller configures the kernel.
  - A proof stays valid across another body's transform where main dropped it.
  - That is sound: the validity check reads no moved reader (vertex test 1 does not use `calc_corner_from_faces`), and the body's own estimate is part of the B-rep, guarded by `BodyGuard`.
  - No stale proof is kept wrongly.

## Item 6: heal
Covered under "Checked and clean". Within one `heal_brep` there is no dependence on the context value `fix_tol_sizes` wrote, beyond what now comes from `b` itself.

## Item 7: PR body, commits, diff
- `Fixes #20938`: present.
- `Changes a default: no`: exact, alone on its line.
- `Test change:` is a bullet line. The pr-checks regex allows bullets, and the "Test change check" passed.
- No local paths, no session links.
- Search of the diff, commit messages and body for thread / parallel / concurrency / local paths / session links: one hit, `has_parallel_faces` in a context line of `corner.rs` (geometric, not a motive). Clean.
- Labels: F2. "ARGUED, no reproduction" for the crease/corner reads is correct. The body omits F1(b).

## File table

| File | Checked |
|---|---|
| goldens/expected/topology.brep_p1.jsonl | 141 rows by script; classes and old==set on all rows |
| goldens/src/units/geom_tol.rs | new signatures; the `mut` drop is required by the API change |
| goldens/src/units/topology_brep_p1.rs | new signatures; context columns read after the setter (source of the moved rows) |
| solidcore/src/geom/tol.rs | new fns equal main's `context_zone_tol_3d` for a body that wrote the context itself (`USE_DEFAULT` and `UNINIT_TOL` branches); docs; test strengthened |
| solidcore/src/heal/face_props.rs | reader moved to body (F1) |
| solidcore/src/heal/heal_data/p2.rs | Edge/VertexProps moved (F1); `fix_tol_sizes` branch logic still right; imports |
| solidcore/src/topology/brep/p1.rs | setters write only the body; getters unchanged; test re-pointed and strengthened |
| solidcore/src/topology/brep/p3.rs | `new_brep` tolerance equal to main except a configured context (tests only); unused import dropped |
| solidcore/src/topology/brep/p5.rs | both context writes removed (setter forward + explicit `set_context_model_size_estimate`) |
| solidcore/src/topology/brep/p8.rs | crease zone tolerance moved to body (F1) |
| solidcore/src/topology/vertex/corner.rs | approx tol = `approx_tol3d(0.5 * body zone)`, same formula as `context_approx_tol_3d`; module doc stale (F3) |
| solidcore/tests/wr507_508_transform_matrix_tolerances.rs | two regression tests; issue's exact case; fail on main by reading |

## Could not verify without a build
- That both new tests fail on main and pass on the head (`cargo test -p solidcore --test wr507_508_transform_matrix_tolerances`).
- F1(b) results. A new test would settle it: scale a 10-cube by 1000, `create_box` another body, then
  - (i) `heal_brep(.., FixTolsizes)` on Y, comparing face/edge/vertex props flags and tolerances on main vs head;
  - (ii) `boolean_union(Y, box)` volume and check_brep Level 2 on main vs head.
- Goldens units that exercise the moved readers, to confirm nothing beyond `topology.brep_p1` moves: `heal.heal_data_p2`, `heal.face_props`, `topology.brep_p8`, `topology.vertex`, `topology.assert_valid`, `boolean.merge_p1`, `topology.brep_p3`, `topology.brep_mirror`, `geom.tol`, `geom.context`.
- The identity of the 1/283 failing unit in the builder's full run.
- Clippy `--workspace --all-targets`, including the goldens crate.

Verified = read in code or computed by script over the diff. Inferred = everything in F1's scenarios and the fillet trace.
