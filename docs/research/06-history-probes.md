# 06 — History probes (local tests, never pushed)

Written by code reading only: nothing here was compiled or run. Every probe prints
`PROBE <name>: SAME|DIFFER|SKIPPED ...` and asserts nothing.

## Files and how to run

| File | Probes |
|---|---|
| `solidcore/crates/solidcore/tests/zz_cache_history_probe.rs` | 1A, 1B (two tests), 2, 3, 4 (three bodies), 5, 6 (three bodies) |
| `solidcore/crates/solidcore-ops/tests/zz_cache_history_probe.rs` | 7 (pool), 6 at ops level (`to_record` first) |

**Both crates set `autotests = false`**, so a new file in `tests/` is not a test target by
itself, and I was not allowed to edit `Cargo.toml` or `tests/*/main.rs`. Before running, add
locally (uncommitted) to each crate's `Cargo.toml`:

```toml
[[test]]
name = "zz_cache_history_probe"
path = "tests/zz_cache_history_probe.rs"
```

Then:

```
cargo test --profile goldens -p solidcore     --test zz_cache_history_probe -- --nocapture --test-threads 1
cargo test --profile goldens -p solidcore-ops --test zz_cache_history_probe -- --nocapture --test-threads 1
```

(Alternative without a manifest edit: one `#[path = "../zz_cache_history_probe.rs"] mod zz_cache_history_probe;`
line in `tests/topology/main.rs` / `tests/ops/main.rs`, run with `--test topology zz_cache_history_probe::`
/ `--test ops zz_cache_history_probe::`. For the ops file prefer the separate target: the pool probe
wants to be the first user of the process's kernel pool.)

While the files exist unlisted, `every_test_file_is_built` (in `tests/topology/main.rs` and
`tests/ops/main.rs`) fails; the manifest entry above satisfies it. Filter output with
`| grep '^PROBE'`.

## Probes

### 1A `probe_1a_points_in_face_depends_on_earlier_draws`
- Mechanism: `rand_dither` (`topology/face/p2.rs:1654`), called per sample at `p2.rs:1741-1745`
  (rectangular branch) and `p2.rs:1834` (trimmed branch) when `point_count > 1`.
- Scenario: same body and face, `points_in_face(.., 7)` on a fresh kernel and on a kernel that made
  one such call on an unrelated box; also the same call twice on one kernel.
- DIFFER proves: the sample UVs of a public query depend on how many draws the kernel made before.
- Expectation: **DIFFER, near certain** (any face that yields samples).

### 1B `probe_1b_inner_shell_answer_depends_on_earlier_draws`, `probe_1b_check_brep_validity_depends_on_earlier_draws`
- Mechanism: `is_inner_shell_of` (`topology/shell.rs:1589`) casts 3 rays per face use from
  `points_in_face(.., 3)` (`shell.rs:1683`); a ray whose later hit is not a face is refused
  (`shell.rs:1803-1812`); when all are refused the answer is `SM_UNSURE` (`shell.rs:1943`). The
  geometric answer itself is draw-independent (the shell stack balances), so the only draw-decided
  outcome is TRUE/FALSE versus UNSURE. `check_brep` test 12 fails unless the answer is TRUE
  (`validation/assert_valid/brep.rs:154`, `seams.rs:86`).
- Scenario: a cube with a spherical cavity (one face, so 3 rays in all). A scout kernel samples
  the cavity face to learn the fixed azimuths of the sample columns and the spread of elevations;
  the body then gets one box notch per azimuth (and its antipode) whose vertical corner EDGE stands
  on the ray's path for about 60 % of the dither range. Expected: about one call in five has all
  three rays refused. The call is repeated 40 times on one kernel and body; then replayed on a
  second kernel and an untouched identical body after unrelated `points_in_face` calls on another
  box bring the generator to the position of the call that answered differently. The second test
  repeats `check_brep(Level0)` 16 times.
- DIFFER proves: containment and **validity of a body** depend on earlier unrelated sampling calls.
- Expectation: **likely DIFFER but unverified geometry**. Assumptions that can break it: the boolean
  makes a cavity from a ball strictly inside (the cube/cube case is tested in
  `issue_1473_inner_shell.rs`, the ball is not); the ray solver reports a hit on the notch edge as
  an Edge (not two Face hits); sample columns have fixed azimuths (the probe prints them and SKIPs
  if there are more than 8). If the repeat line says SAME with all TRUE, the edge hit is not being
  refused; all UNSURE means the notch is too tall (the scout line prints slopes and height).
- Other callers not probed: `brep/enclosing.rs:88`, `brep/p3.rs:3966` (same function);
  `topology_traverser.rs:748` (`is_inner_shell`, also `points_in_face`).

### 2 `probe_2_fillet_step_latch`
- Mechanism: `Kernel::fillet_point_step` (`kernel.rs:820-827`), latched at
  `blend/fillet_solver/p4.rs:4030-4041` by the first point that takes finite differences. That
  branch runs only when `offset_radii_can_change()` or the point is a tangent point, so a
  constant-radius fillet latches only at a tangent point (always `1e-3`), and a variable-radius
  fillet latches at its first point (`1e-5` unless tangent).
- Target: `variable_radius_fillet` 1→2 on a fresh 100×100×10 slab; result = whole body snapshot
  (vertex points, tolerances, intervals) plus volume bits.
- Histories, default kernel: constant round first (expect no latch, SAME); the same variable round
  first (SAME); `latch_fillet_point_step(1e-5)` (SAME); `latch_fillet_point_step(1e-3)`
  (**expect DIFFER**: the public seam standing for an earlier tangent-point fillet).
- Histories, every fix but `fillet_point_step_per_point`: the tangent-point round of
  `tests/wr603_trace_from_tangent_point.rs` first (a real earlier fillet; **DIFFER if it latches
  `1e-3`**, the `/latch` line says what it latched; unverified that it works with that one fix off),
  and the two seams.
- DIFFER proves: the same fillet gives different geometry depending on which fillet ran first.

### 3 `probe_3_context_scalars_written_by_another_body`
- Mechanism: `Brep::set_this_model_size_estimate` writes `k.context` (`topology/brep/p1.rs:237`).
- Finding on "does a public path leave a non-default value": **yes, the scaling transform**
  (`topology/brep/p5.rs:3692-3708`, reached from `mirror::transform_matrix(.., scale_tolerances = true)`,
  which is what ops' `Body::transform` always passes, `solidcore-ops/src/body.rs:341-349`). It stays
  until the next `new_brep` (`topology/brep/p3.rs:664`; every primitive and most result bodies pass
  `USE_DEFAULT`, which puts the default back). Body copies do not write the context
  (`brep/p3.rs:750`). Heal's `fix_tol_sizes` (`heal/heal_data/p2.rs:3055-3067`) also writes it; not probed.
- Readers found: the memo keys (`curve/curve/p5.rs:407-423`, `query/brep/ray/live.rs:130-134`: a
  miss, not a result), `Brep::this_model_size_estimate(k, true)` for a body storing `USE_DEFAULT`
  (`brep/p1.rs:198-205`), heal (`heal/heal_data/p2.rs:507,751,3055`), crease splitting in
  `brep/p8.rs:918`, and `cache_kernel` copies.
- Probe: context values after create_box / move / scale×1000 / reads / next create_box; then the
  same answers about a slab made before the scale, on a fresh kernel and on the scaled one:
  `tol::brep_model_size_estimate`, tolerance, volume, area, check lines, UV classification grid, a
  round and its resulting body.
- Expectation: `3.context/after_scale_1000...` **DIFFER** (size ≈ 17320, zone 1e-4);
  `after_the_next_create_box` SAME; in `answers_about_a_body_made_before_the_scale` the line
  `size brep_model_size_estimate(X)` **DIFFER**, the rest probably SAME (no ordinary operation
  reads the context scalars). A DIFFER on any other key proves a result leak.

### 4 `probe_4_trim_cache_history_{bar_into_post,ball_less_corner,plate_with_hole}`
- Mechanism: `surface_cache_of_face` (`query/face/p2.rs:556-642`) keeps the first cache built;
  `classify_by_trim_cache` (`p2.rs:1030-1058`) answers from it only with a finished tree, else the
  loop walk answers with other tolerance rules (`query/brep/ray.rs:480-486`).
- Scenario: `face_classify_uv` on a 13×13 grid of every face's UV domain (crowded to 1e-8 of the
  borders), on an identical fresh body after each of 14 histories: volume, area, check level 2,
  body point classification, cache lease, lease+tree, first lease while `editing_enabled`
  (report 04 §3.6 item 2), while `doing_boolean` (item 3), on a `clone_sharing_surface_caches` copy
  (item 9), a standing copy moved (`brep/mirror.rs:349`), tree then standing copy moved, a move of
  the body rolled back by `Brep::with_rollback`, tree then rolled-back move, editing lease then tree.
  Also: the grid twice on one body, and a full body snapshot before/after each history
  (`4.bodywrite/...`: tolerances, intervals, which edge uses have UV trims, gap memos).
- Expectation: `4.bodywrite/*` **DIFFER for most reads** (lazy trims and gap memos, by design;
  the `by kind` map separates `eu_has_uv_trim` from `edge_tol`/`vertex_tol`, the interesting ones).
  `4.classify/*`: unknown; most likely SAME except possibly at the near-border points. A DIFFER is a
  direct defect: same UV, same face, different class by cache history.
- Not covered: item 1 (first lookup while the owner is cleared: no public way to be inside
  `create_uv_trim_curve`), item 5 (nested lease), the `tree_requested` bit (#20179, left out as asked).

### 5 `probe_5_stale_gap_memos_after_set_interval`
- Mechanism: `edge::set_interval` (`topology/edge.rs:394-397`) notifies nothing;
  `edge_ccw_edge_gap` (`topology/edgeuse/p1.rs:945-959`) and `vertex_edge_gap`
  (`topology/vertexuse.rs:687-705`) recompute only when not initialised.
- Scenario: body 1 caches the gaps, shrinks one edge's interval by 10 % each end, asks again;
  identical body 2 gets the edit without having cached. Then `check_brep(Level2)` on both and a
  body snapshot; a forced recalculation is printed for reference.
- Expectation: **DIFFER** on `edge_ccw_edge_gap` (stale ≈0 against ≈0.1·length). `vertex_edge_gap`
  DIFFER unless the boolean that built the body already initialised that memo on both bodies (then
  SAME, and the `/forced_recalculation` line shows the stale value). `max_edge_face_gap` is a control.
  A DIFFER in `check_brep_after_the_edit` proves the stale memo changes the validity report.

### 6 `probe_6_reads_then_mass_{bar_into_post,ball_less_corner,rounded_slab}` and ops `probe_6_record_conversion_as_the_first_read`
- Mechanism: reads create trims and may raise tolerances (report 02 §1.5, §2).
- Kernel scenario: volume (1e-6, 1e-10) and area alone, against the same after a first read
  (check, point classification, cache leases in the four modes, rolled-back move); tolerances and
  the whole body afterwards; and check after mass against check alone.
- Ops scenario (`to_record` is in `solidcore-convert`, which the kernel crate cannot use):
  measures alone / after `to_record` / after `check`; the record's JSON before and after measures;
  measures twice.
- Expectation: unknown; a DIFFER in `6.reads/*` or `6.tolerances/*` is the defect "a read changes
  what the next read answers". `6.whole_body/*` will likely DIFFER on memo fields only.

### 7 ops `probe_0_pool_same_calls_after_unrelated_calls`
- Pool facts (`solidcore-ops/src/session.rs`): `start_clean` (236-239) resets the generator and the
  blend station memo. Left as they were: memo caches and counters, the context scalars, and the
  fillet latch, which is the process-wide `STEP` `OnceLock` (66, 209-213) and can never be replayed
  in one process. The app pool (`crates/smlib/src/core.rs:155-166`) also resets the latch and the UB
  list, and likewise leaves the context.
- Scenario: two identical sets of bodies made first; a battery (round, variable round, boolean,
  point classes, check, records as JSON, volumes) on the first; unrelated calls (scale by 1000,
  volume, check, another round, a point class, scale again); the battery on the second.
- Expectation: SAME (the only state found to leak, the context size, has no ordinary reader). A
  DIFFER is a pool-level defect as the adapter's doc states it.
- Not feasible at ops level: the latch order (one process, one latch; and the fix
  `fillet_point_step_per_point` removes it); reading the pooled kernel's state (no public access).

## Compile confidence

Checked against source: every imported path, and the signatures of `points_in_face`,
`is_inner_shell_of`, `check_brep`/`BrepCheck::{is_valid,lines}`, `fillet_edges`,
`variable_radius_fillet`, `latch_fillet_point_step`, `transform_matrix`, `face_classify_uv`,
`face_surface_cache`/`face_cache_tree`/`note_face_surface_cache`/`release`, `with_rollback`,
`clone_standing_for_self`, `clone_sharing_surface_caches`, the gap getters, `set_interval`,
`brep_volume_default`, `brep_area`, the `tol` functions, the context getters, arena `iter()`/`ids()`,
record field names; ops `blend`, `fillet_variable`, `boolean`, `make_box`, `make_cylinder`,
`Body::{transform,volume,check,classify_point,to_record}`.

Kernel file: about 80 % that it builds first time. Least sure:
- closures coerced to `fn` pointers in the typed arrays of `probe_1a` (should infer);
- `BrepQuery::of_brep(b).point_3d_classify(k, b, ..)` with `k: &mut Kernel` (copied from
  `tests/brep_query_cache_lifetime.rs`, where `k` is `&Kernel`);
- `Option::is_none_or` (needs Rust 1.82; the crate already uses it);
- `Point3::distance`, `Curve::evaluate(k, t, 0, true, false)` (copied from `wr603` test);
- borrow shape of `b.with_rollback(|b| transform_matrix(k, b, ..), |_| false)`.

Ops file: about 75 %. Least sure: `mod common;` resolving to `tests/common/mod.rs` (as every other
ops test does), `serde_json::to_string` on the record (it derives `Serialize`), `Blended`/`CheckReport`/
`Error` `Debug` (all derive it).

Runtime risks: a kernel panic (debug assertions are on in the goldens profile) fails only that test;
4 and 6 run 15 and 9 body builds each with a few thousand classifications, 1B about 25 booleans.
