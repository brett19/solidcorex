# 22. Where the first `check_brep` time goes (blade_hub_1 / blade_hub_3 / cliff_1)

Measured on the build server (`--profile goldens`), scratch worktree `cache-chk1-time` (branch `cache-chk1-time`, off
`cache-vfgap-scratch` f5cb999796). Nothing here is committed to a PR branch or pushed to GitHub.
Counts are exact. Timings are indicative (shared server, load 3 to 6; spans-on runs cost about 5 to 8% more than
spans-off runs: blade_hub_1 L2 check 1 is 3.69 s spans off, 3.84 to 4.0 s spans on).

## 1. Answer

The 3.7 s is not the check's own checking. It is **UV trim creation, and inside it one routine: the certified
raise bound `trim_raise_distance` -> `trim_deviation_bound`** (`topology/edgeuse/p3.rs` base:1838 / 2694), which
runs an interval-arithmetic bisection (`query/curve_surface_gap.rs::trim_image_gap_bound_within`, base:3434) for
each trim it creates whose edge tolerance is under twice the sampled drop distance. On blade_hub_1, check 1 L2:

| what | spans-on ms | share of check |
|---|---|---|
| check total | 3837 to 4007 | 100% |
| `U2_trim_raise_distance` (360 calls; 100 do the bound) | 3828 | 95.5% to 99% |
| everything else the trim creation does (`create_uv_trim_curve`: surface drops, 360 calls) | 54 | 1.4% |
| tolerance pass (B3: vertex/edge/face gaps, 234 edge uses) | 4.1 | 0.1% |
| per-edge / per-vertex / per-loop walks (S4 + S5 + S3) | 76 | 1.9% |
| B4 (brep-level rest: ray casts, `is_inner_shell_of`; incl. 20 trim builds, 166 ms) | 173 | 4.3% (of which 166 ms is trim builds) |

The same trims are built **three times** in one first check: the pointer pass `A`, a second pointer pass `B2`
(`AssertValid` starts with `ValidatePointers` again), and the face walk `S2`. A and B2 do it on throwaway
`clone_standing_for_self` copies, S2 on the real body (which is why check 2 creates 0). Per pass, blade_hub_1:

| phase | trim builds | ms (spans on) |
|---|---|---|
| A: `validate_pointers_traced` (pointers.rs base:397; trims on the copy, base:468 and 480) | 120 | 1307 (build calls); phase 1329 |
| B2: `validate_pointers_excluding` inside `AssertValid` (assert_valid/brep.rs base:67) | 120 | 1303 (build calls); phase 1308 |
| S2: face walk in `sub_topology`, via `normalized_evaluate` (edgeuse/p3.rs base:3432), on the body | 100 | 1104 |
| B4: brep-level rest, same route | 20 | 170 |
| total | 360 | 3884 |

One pass over the body's missing trims is 120 builds (`create_missing_uv_trim_curves` alone: 120 trims, 1.24 s).
360 = 3 x 120. So the check pays the trim creation about 3 times; a plain first use of the body that makes the
trims (volume, `create_missing`) pays it once (1.24 to 1.36 s) and the check after it is 0.11 s.

## 2. Instrumentation (all temporary, in the scratch worktree)

New files: `src/zz_chk.rs` (span table: count, inclusive and exclusive ns, per-call durations, slow-call list,
repeat table keyed (label, entity, input fingerprint), named counters, phase tag), `tests/zz_chk1_probe.rs`
(`zz_chk_phases`, `zz_chk_timing`, `zz_chk_firstuse`; registered as a `[[test]]` in Cargo.toml). `pub mod zz_chk;` in
`src/lib.rs`. Spans (`grep -rn zz_chk src` lists every site; labels are the names in the tables):

- `topology/brep/validation/assert_valid.rs`: `check_total`, `A_validate_pointers_traced`, `B_assert_valid_with`.
- `.../assert_valid/brep.rs`: `B1_sag_object`, `B2_validate_pointers_excluding`, `B3_tolerances` (with
  `T1/T2` vertex edge/face gap, `T3` max edge/face gap per call), `B4_brep_level_rest`, `B5_sub_topology`,
  `S0..S5` per shell, face, loop, edge, vertex.
- `.../pointers.rs`: `A1/A2` (lists, live scan), `A3_vpt_check_face_all`, `A3a_clone_standing_for_self`,
  `A3b_check_face`.
- `topology/edgeuse/p3.rs`: `U0_get_or_create_build` (caller and phase noted), `U1a_create_uv_trim_curve` (with
  surface and curve kind), `U1d_surface_drop_curve`, `U1e_drop_and_trim_curve`, `U1f`, `U2_trim_raise_distance`,
  `U3_set_uv_trim_curve`, `R1..R5` inside `trim_raise_distance` / `trim_deviation_bound` (input fingerprint,
  piece count per call).
- `query/curve_surface_gap.rs`: `W0..W6` inside `trim_image_gap_bound_within`, `W4d/W4e` per piece and endpoint,
  `P1..P4` inside `piece_reparametrised`, `E1..E4` inside `reparametrised_endpoint`, piece counters, and run
  counters for the three bisections.

Not instrumented separately (not needed to pass 90%): inside `psi_over`, `curve_derivs_over`,
`NetSurface::partials`, `second_term`: these are leaf interval-arithmetic routines.

## 3. Phase tables

### blade_hub_1 L2, check 1 (spans on; exclusive ms, top leaves)

| label | calls | excl ms | per call median / max |
|---|---|---|---|
| `P4_second_term` (`second_term`, curve_surface_gap.rs base:918) | 178,181 | 867 | 5.0 us / 29 us |
| `P1_psi_over` (piece) | 193,214 | 842 | 2.9 us / 0.81 ms |
| `E4_surface_partials` (endpoint) | 113,294 | 406 | 3.7 us / 53 us |
| `P3_curve_derivs_trim` (piece) | 193,214 | 403 | 2.0 us / 0.22 ms |
| `P2_curve_derivs_curve` (piece) | 193,214 | 335 | 1.6 us / 16 us |
| `E1_psi_over` (endpoint) | 113,294 | 300 | 2.6 us / 9 us |
| `E3_curve_derivs_trim` (endpoint) | 113,294 | 196 | 1.7 us / 0.23 ms |
| `E2_curve_derivs_curve` (endpoint) | 113,294 | 148 | 1.3 us / 4 us |
| `W4d` piece, own time | 193,214 | 83 | |
| `P4c_corner_partials` (pieces over a kink) | 15,033 | 74 | 4.4 us |
| `W3_reparametrisation` incl. `W3a/W3b` (30,248 image evals, 30,048 nearest-parameter) | 100 | 68 | 0.54 ms / 2.5 ms |
| `W4e` endpoint, own time | 113,294 | 33 | |
| `R5` own, `W4_bisect_run` own | | 23 | |
| **named leaves of the bound** | | **about 3780 of 3958 total = 95%** | |
| `S4_edge` (117) + `S5_vertex` (78) + `S3_loop` (45) | | 76 | |
| `U1a_create_uv_trim_curve` (360, surface drops) | 360 | 54 | 0.12 ms / 0.62 ms |
| `A3b_check_face` own, `S2_face` own | | 33 | |
| `A3a_clone_standing_for_self` (2) | | 0.5 | |
| `B3_tolerances` (234 uses; `T1` 468, `T2` 468, `T3` 234) | | 4.1 | |

The spans table has inclusive numbers too (`W_trim_image_gap_bound_within` 165 calls, 3773 ms; `R2` main 100 calls,
3778 ms; `W4a_run_first` 1488, `W4b_run_with_cuts` 1169, `W4c_recentred_retry` 1044 ms). The bound runs 165 times:
100 "main" (the new trim) and 65 "other" (`other_trims_bound`, trims already on the edge); the 65 take 4.4 ms in
total. 65 of the 165 are answered at once by `structured_pair_bound` (W1). Not significant (MEASURED, all under
3 ms in total): `TrimPair::of` (2.4 ms), `trim_pair_surface` surface clone (0.1 ms, 165), `bisection_residual`,
`fit_tolerance`, `lowered_covers_rounding`.

### The three files and the booleans (spans on, ms, check 1 / check 2)

| body, level | check total 1 / 2 | A | B2 | B3 | B4 | B5 (S2 face) | U2 raise bound | trims made 1 / 2 |
|---|---|---|---|---|---|---|---|---|
| blade_hub_1 L2 | 4007 / 35.7 | 1329 | 1308 / 0 | 4.1 | 173 | 1192 (1116) | 3828 | 360 / 0 |
| blade_hub_1 L0 | 3951 / 32 | 1310 | 1291 / 0 | 4.0 | 171 | 1175 (1099) | 3781 | 360 / 0 |
| blade_hub_3 L2 | 4329 / 37.9 | 1451 | 1409 / 0 | 5.7 | 208 | 1256 (1167) | 4128 | 450 / 0 |
| blade_hub_3 L0 | 4302 / 33.4 | 1431 | 1409 / 0 | 5.8 | 207 | 1249 (1161) | 4109 | 450 / 0 |
| cliff_1 L2 | 3908 / 35.1 | 1296 | 1277 / 0 | 3.9 | 169 | 1162 (1087) | 3736 | 360 / 0 |
| cliff_1 L0 | 3919 / 30.8 | 1296 | 1282 / 0 | 3.9 | 171 | 1166 (1091) | 3755 | 360 / 0 |
| ball_less_corner L2 | 9.9 / 7.1 | 0.9 | 0.26 | 0.27 | 0.8 | 7.6 (5.2) | 0.001 | 18 / 0 |
| bar_into_post L2 | 10.3 / 3.1 | 1.0 | 0.41 | 0.50 | 0.5 | 7.9 (0.9) | 0.000 | 15 / 0 |

Level 0 equals Level 2 within noise: the dominant work is in the pointer passes and the face walk, which run at
both. In check 2 the B2 pass is skipped (the skip is taken: faces are whole) and no trim is made. The two Boolean
bodies make 15 to 18 trims and none reaches the bound (`R` spans never run: the edge tolerance already covers twice
the drop distance), so their check is 10 ms; they do not show this cost.

All three files have the same shape: 100 (blade_hub_1, cliff_1) or 102 (blade_hub_3) bound runs for 360 / 450
builds, so the breakdown is the same: A ~34% / B2 ~33% / S2 ~29% / B4 ~4% of the trim time.

### Timings (spans off, runs = 5, min / median, seconds)

| body | L2 check 1 | L2 check 2 | L0 check 1 | L0 check 2 |
|---|---|---|---|---|
| blade_hub_1 | 3.688 / 3.692 | 0.0323 / 0.0324 | 3.687 / 3.731 | 0.0286 / 0.0292 |
| blade_hub_3 | 4.106 / 4.131 | 0.0349 / 0.0360 | 4.050 / 4.055 | 0.0309 / 0.0311 |
| cliff_1 | 3.672 / 3.677 | 0.0320 / 0.0323 | 3.659 / 3.670 | 0.0282 / 0.0284 |
| ball_less_corner | 0.0094 / 0.0095 | 0.0069 / 0.0069 | 0.0095 / 0.0095 | 0.0068 / 0.0069 |
| bar_into_post | 0.0099 / 0.0101 | 0.0029 / 0.0029 | 0.0098 / 0.0098 | 0.0028 / 0.0028 |

## 4. The dominant routine: calls, entities, repeats, distribution

`trim_deviation_bound`, "main" calls (blade_hub_1, check 1; the other two files are the same within 5%):

- MEASURED: 100 calls (102 on blade_hub_3) over 49 distinct edge uses. The input fingerprint is (edge use, hash of the
  trim curve, of the edge curve, of the surface kind data, fit tolerance, `enough`, `over`). **51 of the 100 calls
  repeat an earlier call's exact fingerprint**, 1967 ms of the 3823 ms (the later calls' time). blade_hub_3: 54 of
  102, 2132 of 4105 ms. cliff_1: 51 of 100, 1920 of 3732 ms. By phase: A 34 calls, B2 34, S2 26, B4 6 (A and B2 are
  the same 34 edge uses). The 65 "other" calls are 35 repeats of 30 distinct, 4 ms: negligible.
- MEASURED: trim builds in total 360 over 172 distinct edge uses, none twice in the same phase.
- MEASURED: time spread. Per main call: median 33 to 34 ms, max 122 to 128 ms; 100 calls = 3623 ms in the slow
  list, the 10 slowest = 956 ms (26%). So many calls share it; it is not one pathological call. Pieces per call:
  median 1846, max 4980, total 193,214. By kind (all 100): 72 calls on `SurfOfRevolution` face with BSpline edge curve and
  BSpline trim, 3249 ms, 171,852 pieces; 28 calls on `Cone` faces, same curve kinds, 374 ms, 21,362 pieces.
- 10 slowest (blade_hub_1, check 1, ms, edge use, face, edge, surface / curve / trim kind, pieces):

| ms | eu | face | edge | surface | curve | trim | pieces |
|---|---|---|---|---|---|---|---|
| 127.4 | 304 | 28 | 78 | SurfOfRevolution | BSpline | BSpline | 4980 |
| 126.4 | 304 | 28 | 78 | SurfOfRevolution | BSpline | BSpline | 4980 |
| 126.0 | 304 | 28 | 78 | SurfOfRevolution | BSpline | BSpline | 4980 |
| 110.2 | 308 | 28 | 80 | SurfOfRevolution | BSpline | BSpline | 4212 |
| 109.5 | 308 | 28 | 80 | SurfOfRevolution | BSpline | BSpline | 4212 |
| 109.4 | 309 | 28 | 80 | SurfOfRevolution | BSpline | BSpline | 4212 |
| 73.8 | 116 | 11 | 51 | SurfOfRevolution | BSpline | BSpline | 4284 |
| 73.4 | 117 | 11 | 51 | SurfOfRevolution | BSpline | BSpline | 4284 |
| 72.6 | 116 | 11 | 51 | SurfOfRevolution | BSpline | BSpline | 4284 |
| 66.6 | 80 | 7 | 38 | SurfOfRevolution | BSpline | BSpline | 3752 |

Edge use 304 appears three times with the same result (5.00473503854439e-5): that is A, B2 and S2.

Inside each bound (blade_hub_1, 100 runs, MEASURED): three full bisections (`W4a` first run, `W4b` run with cuts at
the correspondence's knots, `W4c` recentred run), 69,366 + 72,028 + 51,820 = 193,214 pieces. The cuts run lowered
the bound in 67 of 100, the recentred run in 55 of 100, and the bound ended at or under `enough` in 0 of 100, so
those retries are not skippable without changing a result. 36,383 endpoint values are cached after run 1 and
shared with run 2 (the recentred run has its own cache).

## 5. Is it the check's own work? (step 3 and 4)

Fresh read each time, then the step, then check 1 (L2) and check 2. Seconds, min / median over 5 runs, trims made in
brackets. Timings are indicative.

| body | variant | read | step | check 1 | check 2 |
|---|---|---|---|---|---|
| blade_hub_1 | plain read, check | 0.221 / 0.225 | - | 3.753 / 3.790 (360) | 0.034 / 0.034 |
| | (a) `create_missing_uv_trim_curves` | 0.224 / 0.226 | 1.234 / 1.238 (120) | 0.111 / 0.111 (0) | 0.034 / 0.034 |
| | (b) rebuilding read | 0.107 / 0.107 | - | 0.111 / 0.111 (0) | 0.033 / 0.034 |
| | (c) volume (`brep_volume`, 1e-6) | 0.225 / 0.226 | 1.362 / 1.362 (120) | 0.110 / 0.111 (0) | 0.033 / 0.034 |
| | Level 0 check, then L2 | 0.225 / 0.226 | 3.776 / 3.780 (360) | 0.034 / 0.034 | 0.034 / 0.034 |
| blade_hub_3 | plain | 0.268 / 0.269 | - | 4.161 / 4.189 (450) | 0.036 / 0.037 |
| | (a) create_missing | 0.268 / 0.270 | 1.362 / 1.366 (150) | 0.128 / 0.128 (0) | 0.037 / 0.037 |
| | (b) rebuilding read | 0.120 / 0.120 | - | 0.127 / 0.128 (0) | 0.036 / 0.036 |
| | (c) volume | 0.267 / 0.269 | 1.505 / 1.507 (150) | 0.127 / 0.128 (0) | 0.036 / 0.036 |
| cliff_1 | plain | 0.226 / 0.227 | - | 3.808 / 3.812 (360) | 0.034 / 0.034 |
| | (a) create_missing | 0.227 / 0.228 | 1.242 / 1.243 (120) | 0.112 / 0.112 (0) | 0.034 / 0.035 |
| | (b) rebuilding read | 0.108 / 0.108 | - | 0.112 / 0.112 (0) | 0.034 / 0.034 |
| | (c) volume | 0.227 / 0.227 | 1.373 / 1.374 (120) | 0.111 / 0.112 (0) | 0.034 / 0.035 |

Reading (MEASURED): the work is trim creation that the check happens to pay, not a check-only cost. After the
trims exist the first check costs 0.11 to 0.13 s (about 80 ms above the steady 0.033 s: gap and tree caches warming,
not measured further). The check makes the trims three times (360 = 3 x 120; 450 = 3 x 150), a first use that
makes them once pays 1.24 to 1.5 s. The rebuilding read makes them in 0.107 s of total read time, because a file
import takes the sampled raise (`Kernel::with_sampled_trim_raise`, kernel.rs:1026, read at edgeuse/p3.rs base:1852)
and never runs the certified bound; the plain read (0.225 s) makes none (it checks and keeps the file's trims).
So the difference between "trims from a read" (0.1 s) and "trims from a check" (1.24 s per pass) is exactly the
certified bound: `prove_lazy_trim_raise` is true for any call outside a file import.

## 6. What the dominant routine does per call, and candidate savings

Per `get_or_create_uv_trim_curve` (edgeuse/p3.rs base:1706) that builds: `create_uv_trim_curve` (base:483: surface
and curve copies, 1 to 4 drops, 0.12 ms median here), then `trim_raise_distance` (base:1838). That returns at once
unless the edge tolerance is under twice the sampled distance (base:1852); otherwise it calls `trim_deviation_bound`
(base:2694), which copies the surface for evaluation (`trim_pair_surface`, base:2764) and calls
`trim_image_gap_bound_within` (curve_surface_gap.rs base:3434): build the pair data (`TrimPair::of`), try the
structured bound, build the reparametrisation `psi` (30 points, nearest-parameter each, W3), then up to three
bisection runs (`with_psi` base:3551: first run base:3553, run with cuts base:3564, `recentred_retry` base:3570), each
piece evaluating `psi_over`, two `curve_derivs_over`, `second_term`, and for new endpoints `psi_over`, two
`curve_derivs_over`, `NetSurface::partials` (`piece_reparametrised` base:862, `reparametrised_endpoint` base:819,
cached by `cached_trim_endpoint` base:839).

Candidates, each with its count and a label. I have not tested any change.

1. **The same trim is created and proved three times per first check (A, B2, S2).** MEASURED: 360 builds for 120
   trims on the body; 51 of 100 bound runs repeat an earlier run's exact inputs, 1.97 of 3.82 s (spans on) is in the
   repeating calls; A and B2 builds are on copies (`clone_standing_for_self`, pointers.rs base:480) that are
   discarded; S2/B4 build the same trims on the body. Where the saving would be (INFERRED, would need a proof that the
   results are equal): (a) the second pointer pass `B2` re-checks what `A` just checked, 1.3 s of 3.9 s; it is skipped in
   check 2 and not in check 1 only because `faces_whole` is false when a face had no trims
   (assert_valid.rs base:464); (b) A's trims on the copy are the ones S2 later makes on the body, with equal inputs
   in about 17 of the 32 S2/B4 runs (by subtraction: 100 runs, 49 distinct, 51 repeats, with A = B2 = 34; not logged per pair). Whichever path is taken, saving needs the
   result of check 1 unchanged, including which trims end on the body and the raised tolerances.
2. **Within one bound, the cuts run recomputes pieces of the first run.** MEASURED (counts and time of the piece
   routine): run 2 evaluates 72,028 pieces, 21,954 (30%) have the same (a, b) as a run 1 piece; those 21,954 take
   248 ms of the 3.84 s check (6.5%, spans on). INFERRED that their bound is the same: a piece bound is a function of
   (a, b), `psi`, `cn`, `wn`, `net`, all the same in the two runs (the endpoint values are already shared through
   `endpoints`, base:3552). Not skippable: the three runs themselves (the cuts run lowered the bound in 67 of 100,
   the recentred in 55 of 100).
3. **A bound that the structured path answers is already skipped** (65 of 165 calls answered by W1 at 0.9 ms total).
   Nothing to save there.
4. Not a saving (MEASURED small): the surface clone per call (0.1 ms over 165), pair data (2.4 ms), reparametrisation
   (68 ms, 1.7%), tolerance pass (4 ms), per-surface cache builds (3 base tree builds and 3 trimmed-tree builds in the
   whole check, per the existing counters), drops (46 ms for 360 calls).

Not a defect of the leaf code: about 3.78 s of 3.96 is in piece/endpoint interval evaluations. They cost 2 to 9 us
each; there are 306,508 of them (193,214 pieces + 113,294 new endpoints).

## 7. Scratch state added (nothing removed from the existing scratch worktree)

Scratch branches, logs and their clean-up are recorded outside this repository.
