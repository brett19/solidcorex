# 35 - Design T: fit each trim to the budget it has left

Local prototype and measurement only. Nothing was pushed to GitHub or the fork, and nothing was filed or commented.

- Branch: `cache-fitbudget-a`, local, off `cache-trimsplit-a` (50a78ecab7). Head: 145b456b8d.
- Server side ref: `push/cache-fitbudget-a`. Server worktree: `sc-wt/cache-trimsplit-a`, reset to that ref.
- Labels: MEASURED (counted on server2), READ (from the code), INFERRED (reasoned).
- Raw logs are in `scratch35/`:
  - `t35-full.log`: the full run, 298 body variants, 674 s.
  - `t35-diag.log`: the rerun with refusal diagnostics. It hit the 30-minute cap at 271 of 298 variants because the tight-G timing is slow.
- Golden logs are on server2 at `sc-wt/g35-<unit>-<base|T>.log`.

## Verdict

**Sound where it accepts, but not yet adoptable.** The idea is right, and the following measurements support it:

- T never raises a tolerance, and its trims do not depend on the order they are made in.
- Every trim it accepts carries a certified bound B ≤ L.
- It leaves adjacent trims' ends within the vertex tolerance with no gap pass.
- It removes all 4 `check` failures that main's settle causes.
- Its answers never differ from main's by more than 2.2e-6 relative in body area or 1.8e-9 in volume.

**Why it is not ready (MEASURED).**

- **40 distinct trims are refused that main builds.** Only 1 of them is a true floor.
  - The refusals turn into refused answers: 407 classifications, 109 mass keys and 100 rays (MSvT).
  - They also turn into golden rows: 7 broken, plus status 0→1001 in mass.integrator.
  - Valid input must not become a refusal.
- **It costs 3.3x main's creation time** (125 s against 38 s, excluding the probe-only variable_round diagnostics). 76% of that is the certified G bound.
- **Its G for offset and procedural-blend surfaces is sampled, not certified,** so it is not fully pure.

To become adoptable, T needs three things, and each is a contained piece of work:

1. A tight certified edge-to-plane gap (closed form for a planar face). This removes 8 refusals.
2. A certified gap and trim bound on OffsetSurface and ProceduralBlend. This addresses 24.
3. A fitter that can reach about 1e-6 without the loosened path. This addresses 6.

With those three in place, the remainder is 1 FLOOR and 1 BOUND-SLACK.

## Design as built (READ)

**Code locations.** `zz_fit.rs` (new, scratch, `#[doc(hidden)]`), with hooks in `topology/edgeuse/p3.rs` and the four drop-library distance checks.

**Budget.**
- L = tol_edge + tol_face.
- t = min(fit_main, (L − Ĝ)/1.5). The safety factor is 1.5. An uncapped mode (t = (L − Ĝ)/1.5) exists but was not run in full.

**Ĝ.**
- Where it exists, Ĝ is the certified `edge_face_gap_bound` in its within-test form, with within = L − 1.5·fit_main.
- For OffsetSurface, ProceduralBlend and any other case where that returns None, Ĝ = 1.1 × sampled `calc_max_edge_face_gap`. This is **not a bound**.
- If Ĝ ≥ L, T refuses with GAP.

**Build.**
- T builds in Lazy mode with fit = t. Lazy mode refuses the loosened (ii ≥ 2) path, interval recovery and brute force.
- The drop's distance-only checks use max(L, t). Its fit checks keep t, which decouples the drop's dual-role tolerance.

**Accept.**
- B comes from `trim_deviation_bound`, or from `raised_trim_bound` (NURB plus slack) for a UV Line trim.
- B ≤ L accepts. B = None refuses with NO-PROOF.
- B > L, or a failed build, halves t. T makes at most 3 refits, then refuses with BOUND or BUILD.
- T attaches the trim with a 0.0 raise. It never raises a tolerance and never calls `set_interval`.

**Refusal.** A refusal carries the edge use, G, D, B and L in a `Note`. In the golden hook it becomes a `LazyTrimRefusal` error.

**Golden switch.** `ZZ_FIT_T=1` routes `get_or_create_uv_trim_curve` through T.

## (a) Acceptance (MEASURED)

**Counts over 6,841 edge-use notes.**
- 6,735 OK, every one on the first try. No refit was ever needed to accept.
- 0 tolerance raises, and 0 other value changes.
- On the same bodies, main's `create_missing` raised 507 edge and 712 vertex tolerances. That is 41 bodies, with a median largest ratio of 1.99 and a maximum of 3.46. T avoids all of these.

**Refusals: 40 distinct trims.** This collapses mate pairs, O/RO and duplicate files.

| class | trims | where | G sampled / G tight certified / L | what it would need |
|---|---|---|---|---|
| FLOOR | 1 | blade_hub_1 = cliff_1 eu316 (BSpline on SurfOfRevolution) | 4.13e-4 / 4.59e-4 / 4.54e-4 | nothing in a trim can pass; the edge tolerance itself is short (a raise, which T forbids) |
| G-SLACK, plane | 4 | hair_off_pin_diff ×2 (Ellipse), case_ext, case_int (BSpline) | ~1e-15 / 1.1e-2 to 1.9e-2 / 3e-5 to 7.7e-5 | a tight curve-to-plane gap (exact for a plane); then they pass easily |
| G-SLACK, BSpline | 4 | test8 | 3.51e-4 / 1.47 / 3.86e-4 | a tighter BSpline-BSpline gap bound; even then only 9% of L is left |
| OFFSET (no certified G) | 20 | vase1 ×8, vase2 ×4, vase3 ×8 | sampled exactly L/1.1 | a certified offset-surface gap bound, and a gain below 1.1 |
| BUILD (fit near L) | 6 | bottle ×2 (G/L 0.80, 0.82), open_shell ×3 (0.70, 0.91), vase3 ×1 (0.91) | drop gave no curve / start or end off the vertex at t ≈ 1e-6 | a fitter that can work at t ≈ 1e-6 without the loosened path |
| BOUND-SLACK | 1 | shell_fillet2 eu24 | D 7.4e-5, B = inf, L 2.0e-4 | a tighter trim bound for this BSpline pair (report 34: B/D = 29) |
| NO-PROOF | 4 | variable_round (ProceduralBlend) | D ≤ 1.1e-6 (≤ 2.3% of L) | a ProceduralBlend trim bound |

**Correction to the expected classes.**
- The FLOOR edge is blade_hub_1/cliff_1 eu316, not the bottle. The bottle edges are BUILD: report 34's FIT-NEAR edges, which the loosened path builds.
- The 1 BOUND-SLACK and 4 NO-PROOF match the expectation.
- Two new classes appear because T needs a G bound at all: G-SLACK (8) and OFFSET (20).

INFERRED: every OFFSET edge has a sampled G of exactly L/1.1. Their tolerances were set from the gap with main's 1.1 gain, so T's same 1.1 gain lands exactly on L. Any edge whose tolerance came from an earlier raise is refused by construction if its G is sampled.

## (b) T against main (MEASURED)

Counted over trims missing at the start.

**Agreement.**
- 6,412 are identical to main's. 323 differ.
- The maximum 3D distance between the two trims' images is 2.78e-3, on revolution_mold stripped. That edge's L is larger than this.

**Control-point increase.**
- The mode is +3 (170 trims), and the tail reaches +231 and +408.
- The cap bound (t < fit_main) on 684 edge uses.
- Trim memory: 2,224,016 B (main) against 2,252,160 B (T), which is +1.3%. The worst bodies are revolution_mold, revolution2, nurb_box1, vase1/2/3, torii_unioned, box_extrusion2 and bottle.

**Tolerances.** Unchanged under T: 0 edge and 0 vertex raises, against main's 507 and 712.

## (c) Order independence (MEASURED)

Edge-use order was tried reversed and shuffled (LCG seed 0xfeed), on 298 variants × 2 orders. There were 0 differing trims and 0 value diffs.

## (d) The gap pass (MEASURED)

**T with no pass.**
- 23,352 adjacent trim ends were checked.
- 0 exceed the vertex tolerance, whether measured as the UV/3D distance between the two ends or as end to vertex.
- The largest ratio of apart to vtol is 0.646.

**T plus the face pass (`s_settle_with`, on a fresh T body).**
- 211 trim moves, of which 99 are listed per move.
- 53 created: these are the trims T refused, which the pass then builds main's way.
- **40 edge and 54 vertex raises, all on the bodies T refused:** bottle, blade_hub_1/cliff_1, vase1/2/3 and open_shell.

**Main plus the face pass.** 235 moves, 11 edge and 4 vertex raises, with a maximum move of 2.17.

**Per move (T plus pass).**
- 43 moves are explained by the vertex sitting off the edge end by about the move.
- 40 are below 1e-12.
- 16 others are at most 6.5e-6, where the vertex is off the surface by up to 3.9e-6 (bottle, open_shell) or the move is noise below 1e-8.

**INFERRED.** With T, the pass is only needed for two things:
- the refused trims, where it also brings back main's raises;
- sub-tolerance tidying.

No pass is needed for vertex consistency.

## (e) Answers against main's settled body (MEASURED)

**Control.** The P family on main's own settled body (MSvPS) gives 0 differences in every family, so the differences below are real.

**Same / differ / refused, counted as per-key rows.**

| family | MSvT (T, no pass) | MSvTG (T + pass) |
|---|---|---|
| box (no vase) | 283 / 0 / 9 refused | 292 / 0 / 0 |
| cls (200 pts) | 58,693 / 0 / 407 refused | 59,100 / 0 / 0 |
| ray (20) | 5,774 / 86 / 100 refused | 5,864 / 96 / 0 |
| closest | 8,940 / 0 / 0 | 8,940 / 0 / 0 |
| mass | 5,499 / 574 / 109 refused | 5,658 / 524 / 0 |
| check | 294 / 4 / 0 | 294 / 4 / 0 |

**Which bodies refuse.** Every refused answer is on a body with a refused trim:
- variable_round, hair_off_pin_diff, test8, shell_fillet2, cliff_1, vase1/2/3, bottle.

**Size of the differences.**
- The per-key relative difference in mass reaches 1.8, but only on near-zero moments: vase2, torii_unioned, vase3, bottle.
- Body area differs by at most 2.2e-6 relative (torii_unioned, nurb_box1 stripped). Volume differs by at most 1.8e-9 (nurb_box1).
- T plus pass: area at most 4.9e-6 (vase1 stripped), volume at most 6.8e-10.

**Analytic closeness.**
- **`shared_face_overlap1e-6` union.** T area 839.999994, main 840.000000, analytic 839.999976 (report 33). T is closer.
  - Volume is identical at 1359.999964.
  - T plus pass equals main exactly.
  - T's trims here are identical to main's, as built and stripped. Both passes move 6 trims by up to 1e-6, with no raises.
- **shell_cyl.** Analytic area 798π = 2506.990938. Main 2506.990840, T 2506.990840 (they differ by 4.7e-9). The integration error dominates, so the two cannot be told apart.
- **slot_through_holes.** Area differs by 2e-13 relative.

## (f) Cost (MEASURED, full run, wall time per test thread on server2, 6 threads)

**Excluding variable_round.**
- Main's creation takes 38.4 s; T takes 125.5 s (3.3x).
- The G bound is 95.3 s (76%), the B bound 28.3 s (23%), and the builds 1.0 s (1%).
- T computes 6,691 certified bounds, against 875 for main.

**variable_round.** It adds 180 s to T, of which only 1.7 s is the design. The rest is the probe's D measurement on refusal (`zz_split::analyze` on a ProceduralBlend), which is diagnostics.

**Per class.** Mean creation time per body:

| class | main | T | G share |
|---|---|---|---|
| kernel primitives | 0.4 ms | 84 ms | 60% |
| corpus | 622 ms | 1,444 ms | 76% |

**Worst bodies.**

| body | main | T | G |
|---|---|---|---|
| vase1 stripped | 3.0 s | 8.5 s | 7.3 s |
| cliff_1 / blade_hub_1 stripped | 3.4 s | 8.6 s | 6.9 s |
| vase3 stripped | 2.3 s | 6.1 s | 5.2 s |

**Within-test form against the tight form.**
- Within-test G: 125.5 s for 6,116 calls in the diagnostic run (20.5 ms each).
- Tight G (`within = [-inf, inf]`): 1,208.6 s for 6,212 calls (195 ms each).
- **The within-test form is about 10x cheaper, and T cannot afford the tight form.**

## (g) check_brep (MEASURED)

**Vanished.** 4 failures that main's settled body has disappear under T:
- nurb_box1 and box_extrusion2 stripped: "Edge tolerance relative to brep tolerance", caused by main's raises.
- vase1 and vase3 stripped: "2D BSplineCurve kink at interior knot", from main's loosened fits.

**New.** None, with or without the pass.

## Golden units with `ZZ_FIT_T=1`, forwards, no bless (MEASURED)

The baseline (switch off) passes every unit.

| unit | cases | changed | broken (refusals) | note |
|---|---|---|---|---|
| topology.edgeuse_p3 | 5,611 | 72 | 3 (bound exceeds) | adversarial b11/b25/b31/b33 trim values |
| topology.face_p3 | 2,407 | 62 | 4 (build failed) | Endcap loop orientation and uv-trim rows; sphere_diff_3cylinders |
| topology.trimming_tools | 3,305 | 1 | 0 | gen0885: a different block count (799 → 889) |
| mass.integrator | 1,628 | 32 | 0 | ≥10 issue1435_multispan rows go status 0 → 1001 (a refusal) |
| topology.assert_valid | 965 | 15 | 0 | violated_or_updated 1 → 0 (nothing to raise), mutation6 rows |
| topology.brep_validation | 69 | 0 | 0 | |

## What remains impure

- **The gap pass.**
  - Under T it is not needed for consistency at vertices (d).
  - It still moves trims by up to 6.5e-6 against vertex-off-surface and vertex-off-edge-end offsets.
  - It still raises on any trim T refused, because it builds those main's way.
- **The sampled G on OffsetSurface and ProceduralBlend.** It uses a 1.1 gain, which is not a bound.
- **The thread-local FIT/DIST overrides.** These are prototype plumbing only. An adopted form would pass t and L as arguments.

## Not reached

- The uncapped mode was not run on the full set.
- The diagnostic rerun stopped at 271 of 298 variants (the 30-minute cap), so shell_fillet2 is missing from it. Its numbers above come from the full run.
- No bodies were reached beyond the prototype's set.
