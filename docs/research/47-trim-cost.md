# 47. What a UV trim costs

Date: 2026-11-26. Base: origin/main `bbb925608b`. A local benchmark with temporary instrumentation. Nothing was pushed upstream and no issue or PR was touched. The scratch commits lived only on the build server side refs, which have been deleted.

**Labels.**
- **MEASURED** means the probe measured it on the stated corpus; the precision is given beside it.
- **READ** means read from the code.
- **INFERRED** means a conclusion drawn from the measured and read facts.

**Counts and timings.**
- Counts (builds, bounds, pieces, calls) are deterministic: they were identical across reps and modes, and are exact.
- Kernel timings are from the build server (24 cores) at a 1-minute load of 1.0–4.6, below the limit of 8. They are the median of 5 reps after one warm-up. The spread (max/min) of op time was ≤ 1.02 for 74 of 78 scenarios, and ≤ 1.15 for sub-millisecond ops.

**Profiles.**
- The app's cost is the **release** number. Release is the root workspace profile the app ships with: opt 3, thin LTO, 16 codegen units. The kernel probe was built with the same flags (`--release --config profile.release.lto="thin" --config profile.release.codegen-units=16`). The app CLI was built in the root workspace with `--release`.
- `goldens` is solidcore's test profile (release plus overflow checks and debug assertions). It was run as well and is about 10% slower on trims. Every per-trim number below is release unless it is marked goldens.

**Raw data and scripts.** Raw data is under `count-kit/47-data/`:
- `release/`, `goldens/` and `release1/`: the kernel probe. Each holds `ops.tsv`, `records.tsv`, `global.tsv`, `memory.tsv` and `agg.txt`.
- `app/`: the app run (counts and the counterfactual).
- `app_t2/`: the app timing rerun at a quiet moment.
- Scripts: `agg.py`, `app_agg.py`, `scaling.py`, `app_run.sh`.

## One-page summary (actionable)

1. **A trim without a certified bound is cheap. One with a bound is expensive.**
   - **MEASURED (release, n = 2,075 trims built in kernel ops, ±3%):**
     - Without a bound: median **69 µs**, p90 144 µs, p99 0.20 ms.
     - With a bound (22 trims): median **12.8 ms**, p90 15.1 ms, max 78 ms.
   - So **1% of trims take 73% of trim time** (386 of 528 ms).
   - The app shows the same split. **MEASURED (app, quiet rerun, ±5%):**
     - 42,914 trims without a bound: median **3.3 µs**, p99 0.20 ms, 1.13 s in total.
     - 31 trims with a bound: median **49 ms**, 2.90 s in total.
     - All trims together: **4.04 s**, which is 12% of feature eval and 7.8% of 51.5 s wall.
2. **Inside the bound, the cost is the bisection.** **MEASURED.** `bisect` is 65% of all trim time in ops and 80% in `create_missing_uv_trim_curves`. Reparametrisation is 3–5%. Building the trim itself (drop and fit) is 27%.
   - Time per bound = **4.6 ms + 14.0 µs × pieces** (n = 35, R² = 0.988). On the 31 held-out app trims, actual over predicted has a median of 1.24 (range 0.58–1.58).
   - The piece count is *not* predictable from trim size and tolerance alone. **MEASURED:** the held-out median error is 1.7× and the p90 error 7.4×.
3. **Most certified-bound time is not spent making trims.** **MEASURED (kernel ops, release, ±3%).** Over all op contexts:
   - `bound.trim_gap` takes 1,852 ms. Of that, only 386 ms is inside trim creation.
   - The rest goes to three callers:
     - `cover_raised_trims`, the re-proof after joining faces: 450 ms in op and 470 ms in setup.
     - `piece_tube`, Boolean curve classification: 675 ms, nearly all of it in one import (`torii_unioned`).
     - `trim_same_parameter_deviation_bound`, the Boolean merge: 326 ms.
   - **MEASURED (app):** the same holds. Trims built inside trim frames ran 31 bounds. In total, the certified bound ran **8,742 times over 621,536 pieces**. Its biggest callers are:
     - `keep_trim_covered` (UV gap fix), 2,210 calls;
     - the Boolean merge same-parameter bound, 1,577 calls;
     - `cover_raised_trims`, 2,459 calls.
   - **MEASURED (app, quiet rerun):** the bound anywhere takes **19.4 s**. That is 38% of wall time and 5 times all trim building.
     - `keep_trim_covered`: 7.8 s.
     - Boolean merge: 6.9 s.
     - Face routine (inclusive): 8.7 s.
     - `create_missing`: 2.1 s.
     - `cover_raised_trims`: 1.6 s.
   - Section 6 has the full split.
4. **Sweeps and revolves of splines are dominated by the re-proof bound.** **MEASURED.**
   - `spline_prism_8/32/128` sweep: the bound takes 44 / 74 / 105 ms of 47 / 82 / 122 ms of op time (90%).
   - `spline_vase_*` revolve: 75 of 85–112 ms.
   - The caller is `cover_raised_trims` → `raised_trim_bound` (p3.rs:2813), not trim creation.
   - Offsets and shells pay through the trim's own bound:
     - `offset_box_out`: 211 ms of 392.
     - `shell_cyl`: 203 ms of 275.
5. **Per-trim cost stays flat at scale. The fillet around it does not.**
   - **MEASURED.** Drilled-plate trims cost **0.100–0.109 ms each** for 4 to 64 holes, which is linear.
   - The filleted rims grow faster than linear: **13.3 → 24.6 ms per hole** of fillet op time. Trims are about 9% of that op. The face routine `create_uv_trim_curves` runs 282 → 34,944 times (about holes^1.7). That costs 110 ms at 8×8, almost all of it in the routine's own walk, not in builds.
   - **INFERRED.** This repeated walk of the face routine is worth fixing on its own.
6. **Repeats.** **MEASURED (app, exact counts).**
   - 504 of 6,739 `trim_deviation_bound` calls repeat an earlier call with identical inputs, and they hold **140,288 of 621,536 pieces (22.6%)**.
   - In the kernel corpus: 11 of 2,086 calls, covering 4,034 of 52,116 pieces (7.7%).
   - **INFERRED.** Caching these bounds on identical inputs would save that share of bisection, and nothing more.
7. **The counterfactual is cheap in builds and expensive in two STEP parts.**
   - **MEASURED (app, counts exact):** finishing every op result with all its trims needs **35,319** extra builds, against **42,945** actual top-level builds. It adds **78** bound-running trims and **841,912** pieces.
   - Of those pieces, 666,950 come from `step_rod_doubled_edge` and 139,814 from the discord fillet STEP. Without the 14 STEP inputs, the counterfactual adds about 30,000 builds and 9 bounds.
   - **INFERRED.** Eager trims would make one-off STEP imports much slower. Elsewhere they would add about 0.8 builds per actual build at about 5 µs each.
8. **Memory.** **MEASURED (allocation counter on a standing clone; about ±1 KB per body, see section 8):**
   - Trims are **24% of body allocation at the median** (p10 5%, p90 34%).
   - That is **about 850 B per trim** (p10 180, p90 1,340).
   - Of that, the NURBS data itself (knots and weighted poles) is 96 B for a line and 384 B for a 9-pole ellipse. The largest bound trims carry 2.1–8.6 KB.

**What to do, in order of measured payoff:**
- (a) Make the bisection cheaper, or skip it when a structured answer exists. This is the single dominant cost everywhere; about 83% of all calls are already answered structurally (3,552 of 4,285).
- (b) Look at the callers outside trim creation (`keep_trim_covered`, the Boolean merge same-parameter bound, `cover_raised_trims`) before touching trim creation itself.
- (c) Cache bounds on identical inputs: at most 23% of pieces in the app.
- (d) Fix the face routine's repeated walk in fillets.
- (e) Do not make trims eager.

## 1. Method

**Instrumentation.** READ/MEASURED. A scratch module, `zz_tc.rs`, plus span and count hooks. Section 9 lists the files. The data is not kept in repo counters: it goes to process-global side tables written by the probe. There are three modes:
- 0: off;
- 1: counts only, plus per-trim allocation, input fingerprints, and the counterfactual;
- 2: spans with `Instant`.

**What a trim frame is.** Each call of `get_or_create_uv_trim_curve`, and each direct `create_uv_trim_curve` call, is one trim frame. A frame records:
- its call site;
- its edge use;
- the surface, curve and trim shapes;
- the tolerances before and after;
- all counts and spans inside it;
- the raise decision.

Nested frames are recorded with their depth. Only depth-0 frames are counted as trims.

**Kernel corpus.** `tests/zz_tc_probe.rs`, 78 scenarios:
- primitives: 5;
- planar Booleans: 3;
- curved Booleans: 7;
- tangent Booleans: 4;
- fillets: constant, chain, all-12 corner, variable;
- offsets: 3;
- shells: 2;
- sweep and revolve: 2;
- spline prism and vase at 8, 32 and 128 control points, each with and without a cut;
- drilled plates of 2×2 to 8×8 holes, with and without filleted rims;
- the 28 heal-test `.smb` imports.

Each scenario ran in three modes with 1 warm-up plus 5 reps, each rep on a fresh kernel. After the op, `create_missing_uv_trim_curves` ran separately in its own context.

**App corpus.** The release `solidcraft-cli run` in one process per design, with the model cache off:
- the 63 designs of report 36: action matrix, regressions, acceptance, `.sc` fixtures;
- the 14 step-regression parts;
- `grooved_shaft` and `plate_100_holes` from step-conformance;
- the discord fillet STEP;
- `assembly_extract.stp`.

That is 81 inputs. Each ran:
- 1 warm-up and 5 × (mode 0, mode 2) for timing;
- 1 × mode 1 for counts and fingerprints;
- 1 × mode 1 with the counterfactual hook.

The counterfactual hook is in `solidcore-ops` `Body::held` and `Body::primitive`. It runs `create_missing_uv_trim_curves` on a standing clone, using the session's kernel configuration.

**Overhead.** MEASURED:
- Kernel: op time in mode 2 over mode 0 has a median of **1.006** and a p90 of 1.056 over 78 scenarios. Only sub-millisecond ops reach 1.07–1.18 (`prim_box`: 0.69 → 0.82 ms).
- Kernel counts-only (mode 1) overhead is similar.
- App: process wall time, mode 2 over mode 0, has a median of 1.014 and a p90 of 1.061 (under load, first run).

Per-trim times are not corrected for overhead. The bound is below 1% for any trim above 50 µs.

## 2. Per-trim breakdown (kernel ops, release)

MEASURED. These are depth-0 frames in the op context (including the `op2_*` contexts), 2,075 trims over 78 scenarios. Precision is ±3% per span sum. For a single trim under 10 µs it is about ±1 µs.

| | n | min | p10 | median | p90 | p99 | max | sum |
|---|---|---|---|---|---|---|---|---|
| all | 2,075 | 1.8 µs | 2.9 µs | 69 µs | 147 µs | 6.4 ms | 78 ms | 528 ms |
| bound ran | 22 | 5.4 ms | 7.4 ms | 12.8 ms | 15.1 ms | 78 ms | 78 ms | 386 ms |
| no bound | 2,053 | 1.8 µs | 2.9 µs | 69 µs | 144 µs | 0.20 ms | 1.9 ms | 142 ms |
| goldens, all | 2,075 | 2.1 µs | 3.4 µs | 77 µs | 175 µs | 7.0 ms | 90 ms | 587 ms |

**Phase shares.** Exclusive time summed over those trims, as a share of total trim time:

| Phase | Share |
|---|---|
| bisect | 65.2% |
| general drop trace (`drop_curve_base_in`) | 26.6% |
| `create_uv_trim_curve_fitted` own code | 3.7% |
| reparametrisation | 2.8% |
| seam pick | 0.4% |
| plane drop | 0.2% |
| `set_uv_trim_curve` (gap resets and tolerance raise) | 0.1% |
| `refit_raising_trim` | 0.01% |
| `other_trims_bound` | < 0.01% exclusive |
| unspanned | 0.7% |

**Counts for the same 2,075 trims.** MEASURED, exact:
- **Drop loop:** 2,075 iterations, so exactly one per trim. The loosened path never ran in ops; in `create_missing`, `drop.loosened` = 2.
- **Drop paths:**
  - `Okay` (plane): 752.
  - `DropCurveOkay`: 1,281.
  - `PickOneSeamRecovery`: 42.
  - Singular-point recovery, `trim_other_uv_curves`, short-piece removal and brute force: 0 each.
- **Plane drops:** line 416, other 336.
- **General traces:** 1,324 traced. Inside them:
  - 1,202 `get_next_point` calls;
  - 1,154 span passes and 156 failed spans that were subdivided;
  - 47 `flush_curve` calls;
  - the exact cylinder-section shortcut was taken only 16 times.
- **Raise:**
  - 2,070 calls; 2,048 skipped as already covered.
  - 22 ran the own bound, and all 22 also ran `other_trims_bound`. Every one decided `lowered`.
  - The edge tolerance was raised 2 times.
- **Gap resets:** 4,140, which is 2 per trim.
- **UV gap fix inside trim frames:** 0.

**What decides whether the bound runs.** READ and MEASURED. `trim_raise_distance` skips the bound when the trim's fit is already covered by its tolerance (`raise.skip_covered`).

So the bound runs only when the built trim's distance to the surface is *not* within the edge tolerance. Without the bound, the edge would have to be raised:
- **Line trims** take `line_trim_bound` (a cheap closed form);
- **everything else** takes `trim_deviation_bound`, then `other_trims_bound` for the other trims of the edge.

All 22 op cases were NURBS trims on curved or B-spline surfaces, and all decided `lowered`: the bound proved a lower raise than the fit estimate.

Earlier reports already established two related facts:
- Plane/Line/Line trims never bound (report 28).
- Structured bounds answer 3,552 of 4,285 `trim_gap` calls (83%) at a median of about 4 µs (`bound.structured` 16 ms over 4,285 calls).

**`create_missing_uv_trim_curves` afterwards.** MEASURED. It ran on 78 bodies and built 148 trims in 134 ms. The 13 trims that ran a bound took 129 ms of that (median 11.3 ms). The other 135 had a median of 3.8 µs. Nearly all of the cost is in `offset_box_out` (88 ms, 21 trims) and three imports (`cyl_cyl_2191` 31 ms, `vase2` 10 ms, `vase3` 10 ms).

**Allocation per trim.** MEASURED in mode 1 by the counting allocator. A Plane/Line trim allocates about 3.8–5.3 KB in total, including transient allocations. The per-kind figures are in `release/records.tsv`, field `alloc_b`.

## 3. By surface × curve × path × bound (kernel, release)

MEASURED. Ops plus `create_missing`, depth-0 trims. Control points and bytes are medians of the built trim; pieces is the median number of bisection pieces.

| surface | edge curve | path | bound | n | median | p95 | max | sum | ctrl | bytes | pieces |
|---|---|---|---|---|---|---|---|---|---|---|---|
| BSpline 2×2 | Ellipse | DropCurve | yes | 2 | 77.6 ms | 78.2 ms | 78.2 ms | 155 ms | 193 | 7,752 | 5,232 |
| Cone (cylinders) | Ellipse | DropCurve | – | 1,308 | 95 µs | 156 µs | 183 µs | 137 ms | 2 | 96 | 0 |
| Sphere | BSpline | DropCurve | yes | 11 | 13.6 ms | 15.1 ms | 15.1 ms | 134 ms | 52 | 2,112 | 420 |
| Sphere | Ellipse | DropCurve | yes | 8 | 12.6 ms | 15.1 ms | 15.1 ms | 96 ms | 52 | 2,112 | 373 |
| BSpline | BSpline | DropCurve | yes | 8 | 10.8 ms | 13.1 ms | 13.1 ms | 82 ms | 34 | 1,392 | 373 |
| Cone | Line | SeamPick | – | 531 | 97 µs | 105 µs | 170 µs | 52 ms | 2 | 96 | 0 |
| Cone | BSpline | DropCurve | yes | 2 | 14.4 ms | 14.7 ms | 14.7 ms | 29 ms | 40 | 1,632 | 548 |
| SurfOfRevolution | Ellipse | DropCurve | – | 258 | 76 µs | 96 µs | 261 µs | 20 ms | 2 | 96 | 0 |
| Cone | BSpline | DropCurve | – | 32 | 187 µs | 1.93 ms | 1.94 ms | 13 ms | 37 | 1,512 | 0 |
| Plane | Ellipse | Okay | – | 1,058 | 7.2 µs | 20.7 µs | 73 µs | 10 ms | 9 | 384 | 0 |
| Plane | BSpline | DropAndTrim / Okay | yes | 4 | 5.0 ms | 5.1 ms | 5.1 ms | 20 ms | 4 | 192 | 288 |
| Plane | Line | Okay | – | 1,315 | 2.7 µs | 10.8 µs | 19.6 µs | 4.7 ms | 2 | 96 | 0 |

The remaining 18 rows are all under 4 ms in sum (see `release/agg.txt`).

**INFERRED.**
- The non-bound cost is set by the drop path:
  - Plane: 3–7 µs.
  - General trace: 40–200 µs. This applies to cylinders, cones, spheres, revolutions and B-splines, and is about 25 times the plane cost.
- Cylinder rims (Cone/Ellipse) are the most common curved trim: 1,308 trims and 137 ms. They go through the general trace, not the exact cylinder-section shortcut, which ran only 16 times. A closed-form UV line for a circle on a cylinder would remove most of that 95 µs.

## 4. Scaling

- **Time against pieces.** MEASURED (release, n = 35 trims that ran a bound, kernel). The time of the raise span is **4.56 ms + 13.997 µs × pieces**, R² = 0.988. Goldens gives 4.36 ms + 16.2 µs × pieces (R² = 0.991).
  - Time per piece by surface:
    - Sphere and B-spline 5×3: 27–29 µs.
    - Plane 2×2: 17 µs.
    - B-spline 2×2 with an ellipse: 15 µs.
    - Cone 9×2: 26 µs.
  - Each piece does 0.47 endpoint evaluations. The endpoint cache answers 76% of endpoint requests (27,224 hits against 8,392 evaluations).
- **Pieces against trim size.** MEASURED. A fit on kernel trims gives pieces ≈ 1.13 × spans^0.41 × (length/tolerance)^0.40.
  - Train: median error 1.34×, p90 2.7×.
  - **Held-out test on the 31 app trims that ran a bound:** median error 1.66×, p90 7.4×. The worst case is `step_rod_doubled_edge`: 98,304 pieces against 3,575 predicted (27×).
  - **INFERRED.** The piece count depends on how close the trim lies to its tolerance (the bound is driven toward `lowered`), not just on its size. There is no geometry-only predictor.
- **Held-out test of the time model.** MEASURED on the 31 app trims that ran a bound, using the quiet rerun (`app_t2/heldout_time.txt`, raise span against the kernel-fitted model). The ratio of actual to predicted has a median of **1.24** and ranges 0.58–1.58.
  - It holds within 10% for the five largest trims (8,152–98,304 pieces). One of them is the 1.49 s STEP trim, predicted at 1.38 s.
  - The model under-predicts mid-sized trims by about 25%. Those trims are on planes and cones with longer edges.
  - **INFERRED.** Pieces explain the time to within about ±50%. The intercept and slope depend on the surface type.
- **Control points.** MEASURED. For the spline prism sweep, the re-proof bound takes 44 / 74 / 105 ms at 8 / 32 / 128 profile points, with 1,708 / 2,724 / 3,032 pieces. That is sublinear in control points. Trim-frame time stays at 0.3–0.9 ms.

## 5. Per operation (kernel, release)

MEASURED (op context, which includes the `op2_*` second op; ±3%):
- "trim frames" is time inside trim creation.
- "all bound in op" is every `trim_gap` call in the op, including the ones inside trim frames.
- The two columns overlap.

| class | n | op ms | trims | trim frames ms (share) | all bound in op ms (share) | create_missing after, ms |
|---|---|---|---|---|---|---|
| primitives | 5 | 6.7 | 42 | 2.9 (44%) | 0.4 (6%) | 0.0 |
| planar Booleans | 3 | 1.9 | 17 | 0.1 (4%) | 0 | 0.1 |
| curved Booleans | 7 | 78.8 | 47 | 4.4 (6%) | 4.4 (6%) | 0.0 |
| tangent Booleans | 4 | 12.2 | 9 | 0.7 (5%) | 2.2 (18%) | 0.1 |
| spline Booleans (cuts) | 6 | 207 | 30 | 6.2 (3%) | 3.3 (2%) | 0.0 |
| constant fillets | 2 | 16.4 | 48 | 1.1 (7%) | 0 | 0 |
| corner fillet (all 12 edges) | 1 | 96.0 | 112 | 7.7 (8%) | 0 | 0 |
| variable fillet | 1 | 30.0 | 14 | 0.7 (2%) | 0 | 0 |
| offsets | 3 | 413 | 321 | 242 (59%) | 237 (57%) | 82 |
| shells | 2 | 251 | 140 | 158 (63%) | 230 (92%) | 0 |
| sweep (seam pin) | 1 | 1.4 | 5 | 0.4 (29%) | 0 | 0 |
| spline sweeps | 3 | 244 | 51 | 1.6 (1%) | 222 (91%) | 0.1 |
| revolve (box profile) | 1 | 1.9 | 13 | 0.8 (40%) | 0.1 (5%) | 0 |
| spline revolves | 3 | 273 | 18 | 2.3 (1%) | 227 (83%) | 0 |
| drilled plates (op and fillet) | 8 | 3,085 | 1,200 | 99 (3%) | 252 (8%) | 0.1 |
| imports (`.smb`) | 28 | 4,001 | 8 | 0.1 (0%) | 674 (17%) | 51 |
| **all** | 78 | 8,719 | 2,075 | 528 (6.1%) | 1,852 (21%) | 133 |

**Notes.**
- Stitching was not driven separately. Stitching is part of the imports here; no separate stitch op is in the corpus.
- Importing builds no trims: the `.smb` files carry them.
- The 674 ms in imports is `piece_tube` in Boolean curve classification on `torii_unioned` (282 calls).
- Plain / certified / gap-fix split. MEASURED:
  - Plain: 2,053 trims, 142 ms.
  - Certified: 22 trims, 386 ms.
  - Gap fix: zero UV gap fixes ran inside trim frames.
  - The face routine `create_uv_trim_curves` ran 53,356 times for 417 ms inclusive, 114 ms of it exclusive. Of those calls, 34,944 came from the 8×8 filleted plate.

**Larger inputs.** MEASURED (goldens and release agree within 10%):

| holes | drill op | trims | ms per trim | fillet op | per hole | face-routine calls |
|---|---|---|---|---|---|---|
| 2×2 | 14.7 ms | 8 | 0.109 | 53 ms | 13.3 ms | 282 |
| 4×4 | 58.0 ms | 32 | 0.102 | 250 ms | 15.6 ms | 2,678 |
| 6×6 | 131 ms | 72 | 0.101 | 708 ms | 19.7 ms | 11,794 |
| 8×8 | 237 ms | 128 | 0.100 | 1,575 ms | 24.6 ms | 34,944 |

- **Drilling:** cost per trim is flat. The certified same-parameter bound from the Boolean merge is 128 calls and 67 ms at 8×8, 28% of the drill op, and also linear.
- **Fillet:** the op grows faster than linear.
  - Trim builds in the fillet: 384 builds, 31 ms.
  - The face routine: 110 ms, about 9% of the op.
  - **INFERRED.** The rest of the super-linear growth is outside trims.
- **Long spline body:** see section 4. At 128 points the trims themselves stay under 1 ms.
- **Largest STEP parts:** see section 6.

## 6. App: regeneration of 81 inputs (release CLI)

MEASURED counts, which are exact.

- **Trim frames:**
  - 42,945 top-level trim frames.
  - 31 of them ran a certified bound.
  - Builds by call site:
    - `brep/p2.rs:533` (`create_missing_uv_trim_curves`): 15,908.
    - `face/p3.rs:194` (face routine): 12,789.
    - `edgeuse/p3.rs:3714`: 4,473.
    - `loopuse.rs:162`: 4,537.
    - `edgeuse/p3.rs:4091`: 4,191.
    - `brep/p5.rs:1665`: 292.
    - `fillet trace_reach.rs:201`: 294.
    - `mass/enclosure.rs:2568`: 354.
    - Others: 107.
- **Certified-bound family, any caller:**
  - `trim_gap` ran 8,742 times over 621,536 pieces.
  - `trim_deviation_bound` was called 6,739 times.

**Calls by feature kind.** Bound calls and pieces from mode 1; `zz:start` is file open or import, and `zz:measure` is `inspect.measure`.

| feature kind | bound calls | pieces |
|---|---|---|
| Combine | 1,008 | 219,232 |
| zz:measure | 214 | 135,696 |
| Thread | 302 | 109,501 |
| zz:start | 4 | 101,196 |
| Loft | 273 | 49,352 |
| Revolve | 42 | 36,183 |
| Shell | 1,244 | 28,353 |
| Extrude | 3,456 | 27,895 |
| Sweep | 210 | 16,096 |
| Fillet | 168 | 11,093 |
| Mirror | 610 | 10,383 |
| RectangularPattern | 380 | 9,943 |
| Chamfer | 6 | 7,968 |
| Thicken | 54 | 7,016 |
| others | under 650 together | about 4,500 |

**Time split.** MEASURED in the quiet rerun `app_t2/`:
- Conditions: release CLI, mode 2, the median of 5 reps per design after a warm-up. The 1-minute load was 2.3–7.9 (median 3.2, p90 7.8).
- Precision: the rep spread (max/min) of process wall time is ≤ 1.05 for most designs. It reaches 1.1–1.3 only for runs under 40 ms.
- Overhead: mode 2 over mode 0, median 1.011, p90 1.045.

**Totals.**
- Process wall time (mode 0): 51.5 s over 81 inputs.
- Feature eval: 33.6 s.
- **Top-level trim frames:** 42,945, taking **4.04 s**.
  - That is 12.0% of feature eval, or 7.8% of wall time.
  - The per-trim median is **3.3 µs** (p90 33 µs, p99 221 µs).
- **The 31 trims that ran a bound:** 2.90 s, with a median of 48.9 ms.
  - One of them is a 1.50 s trim at `step_rod_doubled_edge` open (Cone with a 1,399-pole B-spline, 98,304 pieces, `raise_to_bound`).
  - Ten others are 50–125 ms trims in `oblique_cuts_through_chamfered_ring` (Plane with a 7-pole B-spline, 1,584–8,600 pieces).
- **The other 42,914 trims:** 1.13 s, median 3.3 µs, p99 199 µs.

**Certified bound anywhere** (the `bound.trim_gap` label, the leaf that every caller ends in, so it is not double-counted): **19.4 s, which is 38% of process wall time and 5 times all trim building.** By caller (inclusive spans; callers nest):

| caller | calls | time |
|---|---|---|
| `trim_deviation_bound` (all callers) | 6,739 | 12.3 s |
| face routine `create_uv_trim_curves` (inclusive) | 27,100 | 8.7 s |
| `keep_trim_covered` (UV gap fix) | 2,210 | 7.8 s |
| Boolean merge same-parameter bound | 1,577 | 6.9 s |
| `create_missing_uv_trim_curves` | 782 | 2.1 s |
| `raise_joined_edge` / `cover_raised_trims` | 2,459 | 1.6 s |
| `cover_joined_faces` | 3,842 | 0.34 s |
| `cover_moved_edge` | 265 | 0.25 s |

**Designs where the bound dominates.** These are bound time against feature eval; zz:start and measure are not part of feature eval.
- `fins_joined_into_their_hub`: 7.5 s against 8.7 s. Its trims take 48 ms.
- `revolve-360-cut`: 2.3 s against 4.0 s.
- `loft_join_carved_top_on_slab`: 1.7 s against 2.0 s.
- `step_discord` fillet: 1.3 s at measure.
- `oblique_cuts_through_chamfered_ring`: 1.03 s against 1.05 s, through trims.
- `front_triangle_hull_triangle`: 0.86 s against 1.15 s.
- `closed_sweep_torus_cut`: 0.71 s against 1.9 s.

**Trim time by feature kind:**
- zz:start (STEP open): 1.65 s.
- Extrude: 0.52 s (16% of Extrude eval).
- Revolve: 0.47 s (44%).
- Measure: 0.46 s.
- Fillet: 0.30 s (7%).
- Chamfer: 0.17 s (38%).
- Everything else is under 0.15 s.

**Trim time by call site:**
- `brep/p2.rs:533` (`create_missing`): 2.00 s.
- `face/p3.rs:194`: 0.80 s.
- `edgeuse/p3.rs:3714`: 0.67 s.
- `loopuse.rs:162`: 0.34 s.
- `edgeuse/p3.rs:4091`: 0.16 s.

The first, high-load run (`app/`, load about 25) gave 7.7 s of trim time and 30 s of bound time. Its times are inflated by 1.5–1.9 and are kept only for its counts.

**Counterfactual: "every op result ends with all its trims."** MEASURED (mode 1, exact counts; wall time first run, under load):
- 35,319 extra builds against 42,945 actual.
- 78 extra trims that ran a bound, with 841,912 extra pieces.
- The worst design is `step_rod_doubled_edge`: 50 builds, 22 bounds, 666,950 pieces, about 26 s under load.
- Next is `step_discord-2026-10-30-fillet`: 84 builds, 36 bounds, 139,814 pieces.
- The worst non-STEP design is `oblique_cuts_through_chamfered_ring`: 272 builds, 9 bounds, 19,958 pieces.
- Without the STEP inputs, there are 9 extra bounds over 63 designs.

This differs from report 36 (86,700 extra builds, 0 bounds) for two reasons:
- The hook here is narrower: `Body::held` and `Body::primitive` only.
- The corpus adds STEP parts, and they hold all the extra bounds.

## 7. Repeated work

- **MEASURED (exact).** A repeat is a `trim_deviation_bound` call whose input fingerprint (curve, surface, trim, interval, floor, enough, over, budget) was seen earlier in the same process.
  - App: **504 of 6,739 calls repeat** an earlier call, with **140,288 of 621,536 pieces (22.6%)**.
  - Kernel: 11 of 2,086 calls; 4,034 of 52,116 pieces (7.7%); 111 ms of 1,437 ms when joined to the mode-2 times.
- **MEASURED.** No trim was ever rebuilt over an existing one: `had_trim` was false in all 4,856 kernel frames.
- **Not measured:** whether bounds in different callers (`keep_trim_covered` against Boolean merge) re-prove the same pair with a different budget. Only exact-input repeats are counted.
- **INFERRED.** At most about 23% of bisection in the app is avoidable by caching on identical inputs. The remaining 77% is first-time work.

## 8. Memory

MEASURED with the counting allocator in the probe binary. The method: the bytes allocated by `clone_standing_for_self`, with trims and with all trims stripped. The per-body precision is about ±1 KB. Two small bodies (sphere and torus) come out slightly negative, which is noise from the strip itself.

- **Trim share of body allocation:** median **24.4%**, p10 5.2%, p90 34.0% (78 bodies).
- **Bytes per trim** (allocation delta divided by the number of trims): median **847 B**, p10 179, p90 1,336.
- **NURBS payload per trim** (8 × (knots + weighted poles)):
  - median 273 B;
  - Line 96 B;
  - plane ellipse (9 poles) 384 B;
  - sphere bound trims (52 poles) 2,112 B;
  - B-spline-surface ellipse (193 poles) 7,752 B;
  - maximum 8.6 KB.
- **Largest bodies:**
  - `plate_8x8_drilled_filleted`: 3.31 MB with trims, 2.77 MB without; 664 trims; 16%.
  - `blade_hub_3` import: 756 KB against 496 KB; 252 trims; 34%.

## 9. What was not reached, and caveats

- **App timing.** The first app run ended at load 25 on 24 cores, because another agent was running a build. Its counts are exact; its times are inflated. The timing rerun is in `app_t2/`. It ran at a 1-minute load of 2.3–7.9 after waiting 620 s for the load to fall below 8. It covered modes 0 and 2 only. The counterfactual wall times in section 6 are still from the high-load run.
- **Stitching** has no separate scenario, and **variable fillets** have one scenario only.
- **Loft** is only in the app corpus.
- Per-trim allocation was counted (`alloc_b`) but not analysed by kind beyond the Plane/Line spot values.
- **Instrumented files** (scratch, deleted):
  - `zz_tc.rs`;
  - `edgeuse/p1.rs` and `edgeuse/p3.rs`;
  - `face/p2.rs` and `face/p3.rs`;
  - `brep/p2.rs`;
  - `query/curve_surface_gap.rs`;
  - `surface_drop_lib/p2.rs` and `p3.rs`;
  - `plane/p1.rs`;
  - `curve_class/p3.rs`;
  - `solidcore-ops` `body.rs` and `session.rs` (`fresh` made `pub(crate)`);
  - `crates/doc/src/eval.rs`;
  - `apps/solidcraft-cli/src/main.rs`;
  - the probe test.
- In the scratch copy the workspace lint `unsafe_code` was "deny", so the test's counting allocator could compile. The crate itself kept `forbid`.
