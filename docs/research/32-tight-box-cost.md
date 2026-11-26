# 32. Why the tight body box costs 70 to 90 s on the vase bodies

Local investigation with temporary instrumentation. Scratch branch `cache-tightbox-probe` from origin/main c2e0ba3f21. Built and run on the build server, `--profile goldens`. Nothing was pushed to GitHub or the fork.

## Answer first

All of the time goes into the 4 **offset faces** of each vase body. Every other face takes well under 3 ms.

The face's `uv_domain` is the whole domain of the offset surface. The trim covers only the middle of it: on vase1 f8 the domain is [0,1]x[0,1] and the trim spans [0.19,0.92]x[0.18,0.82]. The corners of the untrimmed surface lie well beyond the face, at z = ±89.67, while the face itself stays inside ±76.2.

The cell search `face_directed_max_with` (query/face/p3.rs:1433) therefore goes wrong in four steps:

1. **No hull test.** An offset surface is not a B-spline, so `by_patch` is false (p3.rs:1456). There is no control-point hull and no monotone test.
2. **Solutions fall outside the face.** Each cell is solved on the whole surface (`cell_directed_max`, p3.rs:773). Nearly every solution lands outside the trim and classifies `Unknown`: 575,693 of 575,811 on vase2.
3. **Cells are never decided.** The solved top is above `best`, so the cell is not decided by `top.top <= best` (p3.rs:1532). The −dir solve (p3.rs:1539) rarely proves the cell lies above `best` either. Nothing else applies, so the cell is divided.
4. **Every direction runs to the limit.** Almost every direction runs to depth 16 and the 4096-division cap, about 16,385 cells each. Each cell costs 1 or 2 global solves and a classification.

At the cap, an exhausted cell without a patch takes `best = top.top` (p3.rs:1566). That is the solver's value at a point **outside the face**.

So this is not only slow. **On main, the body tight box of vase1, vase2 and vase3 is 13.47 mm too large at each z end: ±89.67 where the solid spans ±76.2.** On vase.smb it is about 10 mm too low at z min (−111.61 against −101.6 when the domain is cut to the trim). See "Accuracy".

## Instrumentation (temporary, side channel only)

- `solidcore/src/zz_prof.rs`: a thread-local span stack (inclusive and exclusive ns), named counters, log lines and a `PROTO` bit switch.
- Spans in the main code:
  - `query/brep.rs`: `brep_tight` at :265 and `brep.face_tight` at :308;
  - `query/face/p3.rs`: `dmax` at :1398, `cell` at :778, `cell.global_solve` at :827 and `cell.classify` at :855; log lines `DMAX`, `CELLS`, `SOL` and `EXH` (:1596);
  - `query/face/p2.rs`: `pc.fallback` at :1192 and `pc.loop_contains` at :1298.
- Driver: `solidcore-convert/tests/zz_tightbox_probe.rs`.
  - Each body is read, then `create_missing_uv_trim_curves` is run.
  - Each body is timed as the min of 3 calls, each on `clone_sharing_surface_caches()`.
  - The six box values are printed as f64 bits.
  - An optional `/stripped` twin has every UV trim removed and then recreated.
- Line numbers are those of the probe branch. The lines named in the answer above (p3.rs:1433 and so on) are those of main c2e0ba3f21.

## 1. Timing (MEASURED, the build server under shared load, ±10–15%)

### Vase bodies

| Body | Faces (kinds) | Edges / edge uses | Tight box, s |
|---|---|---|---|
| Fillet/vase1 | 19 (12 bspline, 4 offset, 3 plane) | 48 / 192 | 13.6–22.3 (stripped 14.4) |
| Fillet/vase2 | 11 (4 bspline, 4 offset, 3 plane) | 24 / 96 | 79.7, 89.4 (stripped 70.9) |
| Fillet/vase3 | 19 (12 bspline, 4 offset, 3 plane) | 48 / 192 | 69.8 (stripped 69.7) |
| Fillet/vase | 11 (4 bspline, 4 offset, 3 plane) | 24 / 96 | 36.3 (stripped 38.4) |
| vase_rev | 1 revolution | 3 / 8 | 0.004 |

Most vase rows are a single call, not a min of 3, because of the 30-minute cap. Repeated calls cost the same: nothing is kept between them.

### Contrast set (min of 3)

| Group | Bodies | Time |
|---|---|---|
| Primitives | box, cylinder, sphere, torus, cone | 0.00–0.002 s |
| Booleans | plate_drilled, ball_drilled, torus_ball, steinmetz | 0.005–0.034 s |
| Fillets | round_drilled, box_all_edges | 0.003–0.007 s |
| Corpus files | the other 25 | 0.0003–0.088 s (blade_hub_1 0.082, blade_hub_2 0.088, cliff_1 0.082) |

### Mass path

The mass path computes the tight box: `mass/integrator/properties.rs:3160-3162` (`estimate`). It uses `bbox.mid()` as the moment origin. Its `bbox.size()` sets `estimated = max_side²/100` (:2374), the area scale of the integration thresholds.

MEASURED on vase1: mass took 14.65 s, and 14.59 s of that was its one `brep_tight` call.

### Callers

**Kernel:**
- `api_queries::brep_bounding_box(tight)` (:884);
- face `calculate_bounding_box_in(tight)` (`query/face/p1.rs:604`);
- `blend/fillet_corner/topology/end_smooth.rs:339`, which takes a face's tight box;
- `prism/measure.rs:924`, a scale for the prism certificate;
- `sweep/extrude.rs:152`, in tests only.

**solidcore-ops:**
- `measure.rs:688` `Body::bounds()`;
- `Body::mass_properties` (about :625), through `brep_mass_properties_estimated(_prepared)`.

**smlib:**
- `core/measure.rs:113` and `:166`.

**App:**
- `crates/kernel/src/body.rs:1593` `Body::exact_bounds()` keeps its result per record. A failure is not kept.
- `crates/kernel/src/measure.rs:107` `measure_mass` calls the smlib mass properties, which compute the tight box inside the kernel, **and** `exact_bounds()`. So a first mass measure pays for the tight box twice.

**User actions that reach it:**
- the measure and mass properties commands;
- body bounds and envelope in `crates/doc` (eval, eval_frame, envelope, drawing dimensions, frame trims);
- picking the nearest body (`body_at_picks_nearest_body_using_exact_bounds`);
- the reach and pre-checks of Boolean tools (`kernel/ops.rs:82` and `:118`);
- split face, emboss and blend.

## 2. Where the time goes (MEASURED, one profiled call)

### vase2: 82.4 s, nearly all in named spans

| Span | Calls | Exclusive s | Share |
|---|---|---|---|
| `cell.global_solve` (`global_point_solve_starts`, Signed3dDirectedMaximize) | 575,811 | 73.07 | 88.6% |
| `pc.loop_contains` (classification fallback, loop walk) | 575,811 | 5.88 | 7.1% |
| `pc.fallback` own (vertex test, surface copy) | 575,811 | 1.54 | 1.9% |
| `cell` own and `cell.eval` | 575,811 | 1.41 | 1.7% |

**Leaf routines: 99.3%.** Edges, the loose box, planarity and the trim cache each take less than 0.05 s.

The trim cache never answers (`pc.tsc_none` = 575,811), so every classification takes the fallback.

### vase1: 19.1 s

- The global solve takes 59.8%, the loop walk 25.6% and the fallback 7.9%.
- 469,995 cells were solved, in 295,348 cells over 96 directions.
- Exhausted: 57,688 at depth and 100 at the cell cap.

### Slowest faces

All the slow faces are offset faces. On every one: degree 0x0, no net, 2x2 knots, open both ways, 1 loop, 4 edges, tolerance 1e-5.

| Body | Face | Domain | Time, s |
|---|---|---|---|
| vase2 | f8 | [0,1]² | 37.3 |
| vase2 | f6 | [0,1]² | 36.3 |
| vase2 | f10 | [0,1]² | 4.6 |
| vase2 | f7 | [0,1]² | 4.2 |
| vase1 | f8 | [0,1]² | 6.17 |
| vase1 | f7 | [0,0.937]x[3,4] | 4.87 |
| vase1 | f10 | [0,1]x[2,3] | 4.08 |
| vase1 | f9 | [0,0.937]x[1,2] | 3.96 |

The next slowest faces are B-spline: vase1 f15 is degree 3x3 with a 320x5 net and takes 0.003 s, and vase1 f11 takes 0.002 s.

### How the time grows

The time is about (offset faces) × (directions that miss) × min(cells reached, ~16,385) × (cost per solve).

- On vase2, 22 of 24 offset directions hit the cap. On vase1, 18 of 24.
- The cost per cell differs by surface: about 370 µs on vase2 f6 and f8, about 50 µs on f7 and f10.
- The time does not grow with the body's face or edge count. It is set by the cap times the per-solve cost of the base surface.

## 3. Repeated work (MEASURED)

- **Within one call, on the same face:**
  - vase2: 575,811 classifications over 357,563 distinct (face, uv) points, at most 25 times one point;
  - vase1: 469,995 over 278,633, at most 20.
- **Why points repeat:** the solver's "solutions" are mostly cell corners (`s.start`). Neighbouring cells share corners, and the 6 directions solve the same cells again.
- **The −dir solve:** it runs on 60% of cells (215,327 of 360,484 on vase2).
- **Across calls:** nothing is kept. Each call repeats everything, and the mass path and `bounds` each compute the box again.
- **In the app:** `measure_mass` computes it twice on first use, as listed under item 1.

The edge boxes computed again for each face cost less than 0.01 s here (88 calls): not a factor.

## 4. Algorithm and upstream

### Upstream

`SmFace::CalculateTightBoundingBox` (upstream SmFace.cpp:959) makes 6 global solves over the whole domain and classifies each solution. Only `SM_PC_FACE` points count. Its NOTES say the tight box is expensive and should be computed once and kept.

### The fork (WR-588, #1799) adds a cell search

It prunes in four ways:
- (1) by the hull of the patch's control points (B-spline faces open both ways only);
- (2) when the solved top is at or below `best`;
- (3) when the solution is inside the face, by raising `best`;
- (4) when the −dir solve proves the whole cell lies above `best`, or by a monotone patch.

Otherwise it divides the cell, up to depth 16 and 4096 divisions.

### Where the vase bodies defeat it

- **(1) and (4)'s monotone test** need a B-spline patch. Offset faces have none.
- **(2) fails** because the face's start box comes from its vertices and edges, while the surface outside the trim rises higher. In vase1 f8 +z, `in` = 76.2 and the surface corner is 89.67.
- **(3) almost never fires.** Interior solutions are rare: 112 of 575,811 classified Face.
- **(4)'s −dir test** proves little near the trim.

So the search runs to the cap. The exhausted rule then adopts an off-face value (p3.rs:1566).

## 5. Candidates

| # | Candidate | Basis | Expected effect | Same bits? |
|---|---|---|---|---|
| A | Cut the search domain to the face's trim box (UV box of the outer-loop trims, padded 1e-6) | MEASURED (prototype, PROTO=4) | vase1 13.6 → 0.036 s; vase2 79.7 → 10.6 s; vase3 69.8 → 10.6 s; vase 36.3 → 17.7 s | **No.** See below. |
| B | Seed each face's directions with the body box so far (PROTO=2) | INFERRED, not run | `best` starts at 76.2 or above, but the off-face tops at 89.67 stay above it, so the search still exhausts. Little gain on the vases. | Unknown |
| C | Skip a direction whose loose surface box is at or below `best` (PROTO=1) | INFERRED, not run | The loose box covers the off-face corners, so it never skips on the vases | Same when it skips |
| D | Keep the body tight box per body, dropped on write (report 30, section 6.5 fix) | INFERRED | Removes the repeats across calls: `bounds` after `mass`, and the double call in `measure_mass` | Yes, by construction |
| E | Answer classification from the trim cache on offset faces (now always `tsc_none`) | INFERRED | At most the classification share: 9% on vase2, 33% on vase1 | Only if the answers match |
| F | Classify each (face, uv) once per call, as a map | MEASURED as counts only | Saves about 38% of classifications, which is about 3% of time on vase2 | Yes |

The global solve is 88.6% of vase2's time. Only candidates that cut the **number of cells** matter there, which is A (or a hull bound for offset surfaces).

### Prototype A result: the box differs, so I stopped

As the brief asks, I stopped and did not tune it. The z bounds change on every vase body. The x and y bounds of vase1, vase2 and vase3 are bit-identical. vase_rev is identical.

| Body | main z min / max | A z min / max |
|---|---|---|
| vase1 | −89.6703842 / 89.6703842 | −76.2 / 76.2001236 |
| vase2 | −89.6703842 / 89.6703842 | −76.2 / 76.2001903 |
| vase3 | −89.6703842 / 89.6703842 | −76.2 / 76.2001879 / 76.200185 (stripped / as-is) |
| vase | −111.607282 / 85.0366639 | −101.6 / 82.3281835 |

On vase.smb, x min, y max and z max also differ in the last bits, for example x min −57.15 against −57.149999999999…

A's result also differs between the stripped and as-is twins in the last digits. Its exhausted cells still adopt solver values (76.2001236 against the true 76.2), so A is not exact either. The 28-file and Boolean/fillet bit-for-bit comparisons were therefore not run for A.

## 6. Accuracy and what the callers need

### Ground truth (MEASURED)

I sampled each offset face on a 129×129 grid over its trim box, keeping only points that classify Face, Edge or Vertex: 10k to 13k points per face.

- vase1, vase2 and vase3: the offset faces span z [−57.15, 76.2000] and no more. The other faces reach −76.2.
- So the true body z range is ±76.2, and main's ±89.67 is the untrimmed offset surface's corner (POS f8 (0,0) = z −89.67).
- On vase.smb, the sampled offset faces reach z −88.90 at most. Main's −111.61 is again an off-face corner.

### Effect on the callers

- **Mass:** uses `mid` (unchanged in z on vase1, vase2 and vase3, which are symmetric) and `size`. A size of 179.3 instead of 152.4 makes `estimated` area 1.38 times larger, which loosens the integration thresholds. Mass bits depend on this box.
- **Mass needs only an origin and a scale.** The vertex and edge box, or the loose box, would do. That is INFERRED, and it changes mass bits.
- **Picking, Boolean reach and envelope** need a box that contains the body. A loose box is enough and is conservative.
- **Exact bounds the user sees** (measure, drawing dimensions) need the true box. Main's answer is wrong on the vase bodies today.

Callers were not changed.

## Not measured

- Prototypes B, C, D, E and F were not run. The 28-file and Boolean/fillet bit-for-bit check was not run for A, because A differed on the first bodies.
- Why vase2's f6 and f8 cost 7 times more per solve than f7 and f10 (a different base surface; not profiled inside the solver).
- Mass timing on vase2 and vase3 (vase1 only).
- The app-level double call was read from code, not timed.

## Housekeeping

