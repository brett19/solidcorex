# 26 — Why classification and ray queries rebuild face trees on every call, and a reuse prototype

This is a scratch investigation. Nothing here was committed to a PR branch or pushed to GitHub.

**Base.** origin/main `ea53c33068`. #21019 was merged as `c6c0178d5c` and is an ancestor of report 25's base
`562396937b`. Since `562396937b`, the only solidcore change on main is to ops `sweep.rs`. So "main" and "#21019's
head" are the same kernel code for this question: one measurement covers both (READ). The PR branch
`cache-b3-lend` (`b3358de953`) is an older main plus the same change. I did not measure the kernel before #21019.

**Setup.** Scratch worktree `sc-wt/cache-cls-count`, branch `cache-tree-why`: report 25's 4 scratch commits, then 5
more. Built on server2 with `sc-slot.sh` (`--profile goldens`, `CARGO_PROFILE_GOLDENS_DEBUG=line-tables-only` so the
backtraces carry file:line). The built test binary ran under `nice` with `--test-threads 1`.

**Raw logs** (`logs/`):
- `26-tree-why.log`: run 1, main vs prototype A, 100 calls per cell.
- `26-tree-why-A2.log`: run 2, same as run 1.
- `26-tree-why-B.log` and `26-tree-why-B2.log`: main vs prototype A+B.
- `26-tree-why-traces.log`: 2 calls per cell, with backtraces.
- `26-proto-eq-A.log` and `26-proto-eq-B.log`: the bit-for-bit comparisons.

**Labels and timings.** Every claim is labelled MEASURED, READ or INFERRED. Counts are exact. Times are indicative
only: server2 ran at load 27-30 on 24 threads throughout, and the same cell varies up to 1.7x between runs. So every
timing comparison below is between main and the prototype **in the same run**, one after the other.

## 0. Answers first

**Why the tree is rebuilt** (READ, confirmed by MEASURED traces):
- The solve never asks the face's slot for its cache.
- Every ray or point solve gets its face cache from `query/brep/ray.rs:200` `face_surface_cache`. That calls
  `get_surface_cache_with_face` (`surface/surface/p1.rs:1119`), which, despite its name, always **makes** a new
  trimmed cache (`make_trim_srf_cache_with_face`, `p1.rs:1093`). The factory builds the tree.
- The new cache lives in `FaceCaches` (`ray/live.rs:647`), inside the `KeptFaceCaches` (`live.rs:151`) of the
  per-call `BrepQuery`/`BrepClassifier`. It is dropped when the call ends and is never put in the slot.
- The slot, `Surface`'s `TrimCacheSlot`, is a different holder with a different handle type (`EntityRef` rather than
  `CacheObject`). It is filled only by the face-query ports (`query/face/p2.rs:567` `surface_cache_of_face`), and the
  solve never reaches them.
- So a sphere whose slot holds a tree-bearing cache (`St`) still builds 1 tree per call.
- The solve's design is deliberate:
  - #685 (2026-10-23) made the solve build a fresh cache.
  - #2564 (2026-11-04) made it per `BrepQuery`: "made again after the B-rep is edited; never global" (`live.rs:148`).
  - #20546 (2026-11-21) removed the remembered cache-tree state that had mimicked upstream's reuse. The comment at
    `ray.rs:194` now reads "Upstream reuses a cache an earlier call built ... the port keeps no record of that."

**Two paths build trees on every call** (MEASURED; calls 2-100, per call):

| path | entries that hit it | trees per call on main |
|---|---|---|
| **L**, lend: `solve_point_face` (`intersect/topology_solver/p1.rs:1616`) → `lend` → `cache_mapped` (`live.rs:764`) → `prepare_surface_cache` → `face_surface_cache(get_tree = true)` | `classify_point(s)`, `face_hit_by_ray`, `fire_ray` | 1 per curved face crossed: sphere 1, torus 1, cylinder 1, mouse 2, bague 4, endcap 1 |
| **S**, state read: `global_point_solve_in` (`surface/surface/p4.rs:1761`) → `surface_cache_without_get_tree` (`live.rs:591`) → `cache_state_mapped` (`live.rs:830`) → `face_surface_cache(get_tree = false)`. Outside a Boolean operation the factory builds the tree anyway. | `face_at`, `closest_to_point`, `classify_points` (fast-drop point test) | 1 (box `classify_points10`: 2), on planes too |

**What the prototype saves and whether any answer moved** (MEASURED):
- **Prototype A** serves path L from the slot. It is about 30 lines, behind a probe switch (section 4).
- **Prototype B** adds path S, with one changed condition.
- Tree builds on calls 2-100 fell from 1-4 per call to **0 in every cell** under A+B (80 cells, 10 bodies or
  states × 8 calls).
- Typical warm call time (calls 2-100, same run):
  - `classify_point`: sphere 738 → 333 us, torus 1161 → 390, mouse 1195 → 393, bague 1296 → 873,
    cylinder 387 → 293.
  - `fire_ray`: sphere 561 → 139, endcap 778 → 170.
  - With B, `face_at` and `closest_to_point`: sphere 819 → 86, torus 1285 → 101.
- What it costs: path L now makes 2 whole-cache copies per face per call (slot → solve copy, then the lend's copy)
  instead of 1 tree + 1 copy. A copy costs 20-64 us; a tree costs 100-800 us.
- **No answer moved.** For each of A and A+B separately:
  - 1,080 comparisons: 450 entries × 3 calls (cold, warm, third) + 630 later-query comparisons in report 25's C2
    pattern.
  - Plus 80 cells × 100 calls in `tree_why`.
  - All were identical bit for bit (floats as shortest round-trip text, volume as bits, check report as Debug).
- Call 1 is not faster under A, except on bodies whose slot was filled before the call (section 5).

## 1. Instrumentation (scratch only)

Paths are relative to `solidcore/crates/solidcore/src`. Report 25's counters (its section 1) are kept as they were.

| what | where |
|---|---|
| timers `BASE_TREE_NS`, `TRIM_TREE_IN_NS`, `PREPARE_NS`, `MAP_HANDLES_NS`, `BREP_CACHE_NS`, `LEND_NEW_NS`; counters `PROTO_SLOT_SERVED`, `PROTO_FALLBACK`, `CURVE_TREE_BUILDS`, `CURVE_CACHE_BUILDS`; `Timer` guard; `kind_name` | `zz_probe_counts.rs:60-86` |
| backtrace capture `trace(tag)` (keeps kernel/ops/test frames as `fn@file:line`, skips cache-internal frames), `take_traces`, `set_trace`, `set_proto`, `proto_b()` (env `ZZ_PROTO_B`) | `zz_probe_counts.rs:136-230` |
| tree build: time and trace `base_tree kind=<surface> copy_live=<bool>` | `surface/surface_cache/p3.rs:990-997` (`build_tree_base_with`) |
| trimmed tree build (boundary implant included): time | `surface/trim_srf_cache.rs:808` (`build_tree_in`) |
| solve prepare: time and trace | `query/brep/ray.rs:178-179` |
| **prototype switch** | `query/brep/ray.rs:222-248` |
| whole-cache copy: time and trace | `surface/surface_cache/p1.rs:1981-1982` (`map_handles`) |
| `BrepCache` build: time and trace | `query/brep.rs:712-713` |
| new lend: time | `query/brep/ray/live.rs:794` |
| slot new build: trace | `query/face/p2.rs:619` |
| slot state reader `zz_slot_state` (`E`mpty, `C`hecked out, `S`tored + `t`ree / `n`o tree, `m`odified, bound to `f`ace) | `query/face/p2.rs:1090-1112` |
| curve tree / curve cache builds | `curve/curve_cache.rs:922`, `curve/curve/p5.rs:350` |
| tests `tree_why` (env `ZZ_BODY`, `ZZ_REPS`) and `proto_eq` | `solidcore-ops/tests/zz_cls_probe.rs:600-760` |

## 2. Q1 — the per-call tree, as a table

These are calls 2-100 on an unchanged body. The traces are from call 2 (MEASURED in `26-tree-why-traces.log`).

| | path L (lend) | path S (state read) |
|---|---|---|
| which tree | `SurfaceCache` base tree + trimmed tree with the face boundary implanted (`build_tree_with_face`, `trim_srf_cache.rs:770`) | the same tree, built by the factory |
| builder | `make_trim_srf_cache_with_face` (`surface/surface/p1.rs:1093`, tree at `:1107`), via `get_surface_cache_with_face` (`:1119`) | the same |
| caller | `ray.rs:264` ← `prepare_surface_cache` (`ray.rs:172`) ← `cache_mapped` (`live.rs:764`) ← `cache` (`live.rs:742`) ← `lend` (`live.rs:795`) ← `lend_surface_cache` (`live.rs:574`) ← `lent_cache_from_context` (`surface/surface/p8.rs:1594`) ← line intersect (`p8.rs:2109`) ← `solve_point_face` (`topology_solver/p1.rs:1616`) ← `BrepClassifier::classify` (`query/brep.rs:902`) / `first_face_hit` (`brep.rs:771`) | `ray.rs:264` ← `cache_state_mapped` (`live.rs:830`) ← `surface_cache_without_get_tree` (`live.rs:591`) ← `global_point_solve_in` (`surface/surface/p4.rs:1761`) ← `solve_point_face` (`topology_solver/p1.rs:1789`) ← `classify` (`brep.rs:903`) |
| on which object | a new `TrimSrfCache<CacheObject>` over `clone_for_evaluation()` of the face surface (`ray.rs:211`). The B-rep it reads is the standing copy (read entries) or the body itself under its snapshot (`with_body` entries). | the same |
| where it is held | `FaceCaches.caches[face]` (`live.rs:647`), and one `map_handles` copy as `Arc` in `FaceCaches.lent` (`live.rs:653`) | `FaceCaches.caches[face]`, with no tree requested. A later lend of that face takes it only if it already has a tree (`live.rs:753-760`). |
| fate at the end of the call | dropped with the `BrepQuery`/classifier (`KeptFaceCaches`, `live.rs:151`); **never stored in the slot** | the same |

Per-call counts on main, calls 2-100 (MEASURED, run 1). Each cell gives trees / whole-cache copies / new lends; "0"
means none of them.

| body (state) | classify_point | classify_points10 | face_at | hit_ray / fire_ray | closest_to_point | bounds, point_on_face |
|---|---|---|---|---|---|---|
| box (proved) | 0 | 2 / 0 / 0 | 1 / 0 / 0 | 0 | 1 / 0 / 0 | 0 |
| sphere (fresh, proved) | 1 / 1 / 1 | 1 / 1 / 1 | 1 / 0 / 0 | 1 / 1 / 1 | 1 / 0 / 0 | 0 |
| torus (proved) | 1 / 1 / 1 | 1 / 1 / 1 | 1 / 0 / 0 | 1 / 1 / 1 | 1 / 0 / 0 | 0 |
| cylinder (proved) | 1 / 1 / 1 | 1 / 1 / 1 | 1 / 0 / 0 | 1 / 1 / 1 | 1 / 0 / 0 | 0 |
| fillet_box (proved) | 0 | 2 / 0 / 0 | 1 / 0 / 0 | 0 | 1 / 0 / 0 | 0 |
| c_mouse (proved) | 2 / 2 / 2 | 2 / 2 / 2 | 1 / 0 / 0 | 2 / 2 / 2 | 1 / 0 / 0 | 0 |
| c_bague (proved) | 4 / 4 / 4 | 4 / 4 / 4 | 0 | 4 / 4 / 4 | 0 | 0 |
| c_endcap (proved; fresh for rays) | 0 | 0 | 0 | 1 / 1 / 1 | 0 | 0 |

**Surface kinds of the rebuilt trees** (MEASURED trace tags): BSpline, Sphere, Torus, Plane, Cone, SurfOfExtrusion.
Path S also builds trees on planes (box, fillet_box).

**The sphere.** Its slot holds a stored cache with a tree but not bound to the face (`St`), both fresh and proved
(MEASURED). Neither path reads the slot:
- `face_surface_cache` goes straight to the factory (READ, `ray.rs:200-275`).
- `get_surface_cache_with_face` is a "get or make" in upstream's terms, but the port only makes (READ, `p1.rs:1119-1132`).
- The one reader of the slot's state, `point_classify_with_topology` (`query/face/p2.rs` ~1150), uses the slot cache
  only if it is already `Stored`, and does not build one.

## 3. Q2 — why slots stay empty on spline, planar and conic faces

**What fills a slot** (READ): only `surface_cache_of_face`'s `Empty` branch (`query/face/p2.rs:615-640`). It builds the
cache and keeps it. It is reached through `face_surface_cache` and `owned_surface_cache` (`p2.rs:433`, `:455`) from:
- `intersect/point_class.rs:2160` (fast-drop test)
- `topology/brep/p9.rs:3059` (edge drop)
- `topology/brep/p1.rs:1943` (point solve without a guess)
- `topology/edgeuse/p3.rs:335` (binormal)
- `boolean/topology_intersector/p1.rs:4621`
- offset `edgeuse.rs:701`

None of these is on the classification, ray or `face_at` path; MEASURED `slot_new_build` traces: 0 across all 80 cells.

**Slot state per face, before call 1** (MEASURED, `slots_before`):

| body | slots |
|---|---|
| box | `E` ×6 |
| cylinder | `E` ×3 |
| c_mouse | `E` ×4 |
| c_bague | `E` ×4 |
| sphere, torus | `St` |
| fillet_box | `E,Snmf,E,Snmf,Snmf,Snmf,E` (stored blend faces, modified, no tree) |
| c_endcap | `Snf` |

So a face's slot holds a cache only if some construction-time operation reached one of those callers on that face.

**History** (READ, `git log -S`):
- `fd645a8986`/`53641687a5` (#685, 2026-10-23): `prepare_surface_cache`. The solve builds its own cache.
- `84927204a0` (#697, the same day): adds the face-bound slot (`TrimCacheSlot`) for the face-query ports.
- `e88b8f1a73` (#2564, refs #2494, 2026-11-04): `KeptFaceCaches`. Kept across ray solves of one `BrepQuery` only,
  "never global".
- `a6e102b277` (#20546, for #20054, 2026-11-21): removed `CacheTreeState`/`note_surface_cache`, so that no answer
  depends on what ran before. That deleted the solve's last link to a cache built earlier
  (`let get_tree = get_tree || surface.cache_tree_state() == Built`).
- `15843734ae` (#20694, for #20179): the fast-drop point test follows the face, not the cache's history.

The solve and the slot were two holders from the start. The project has since removed every way a solve's answer
could depend on an earlier call. Keeping the solve's tree in the slot was never done, so no decision was ever made
against it. The policy it would have to meet is "no answer depends on what ran before" (INFERRED from the commit
messages above).

## 4. Q3 — cost of a tree, a copy and a lend

Main, calls 2-100, run 1, per unit (MEASURED, indicative):

| body | trimmed tree build (whole `build_tree_in`) | of which base tree | one whole-cache copy (`map_handles`) | new lend (tree + copy) | `BrepCache` | standing copy |
|---|---|---|---|---|---|---|
| box (plane, path S) | 11 us | 3 | n/a | n/a | 10-11 | 17-19 |
| fillet_box | 23 | 3 | n/a | n/a | 16 | 28 |
| cylinder | 98-110 | 37 | 21 | 131-137 | 4 | 11 |
| bague (spline) | 141-157 | 58-62 | 31-35 | 175-196 | 7 | 17 |
| mouse (spline) | 225-324 | 51-80 | 20-22 | 333-350 | 10-12 | 18 |
| sphere | 430-452 | 337-359 | 35-42 | 473-497 | 2 | 8 |
| endcap (spline) | 684-701 | 437-444 | 42 | 730-748 | 2-3 | 5 |
| torus | 776-808 | 630-663 | 54-64 | 877 | 3 | 8 |

So on curved faces the tree is **85-95 %** of a new lend, and the copy is 5-15 %. On a warm `classify_point` the
tree is about 60 % of the call on sphere (444 of 738 us) and torus, and about 27 % on mouse per face (2 faces: about
54 %). The `BrepCache` and the standing copy are small, as in report 25.

## 5. Q5 — the prototype and its measurements

**Prototype A** (`query/brep/ray.rs:222-248`, scratch, behind `zz_probe_counts::PROTO`):
- In `face_surface_cache`, when `get_tree` and the B-rep is not being edited, not in a Boolean operation, and the
  face's surface owner is not cleared, the solve's cache is served by `query::face::p2::solve_surface_cache_of_face`
  (`p2.rs:480`).
- That takes the slot cache (or builds and keeps one), binds it to the face with its current zone tolerance
  (`p2.rs:538-541`) and builds its tree if missing (`face_cache_tree`). It then returns a `map_handles` copy in the
  solve's handle space, with surface handle `CacheObject::Surface(face)`.
- On `None` (no trimmed cache could be made) it falls back to today's path. Fallbacks: **0** in every cell.

**Prototype B** (env `ZZ_PROTO_B`): the same with `get_tree == false` too. The state read is then answered by the
slot's tree-bearing cache.

**Tree builds and copies per call, calls 2-100** (MEASURED; every cell):
- A: path L trees → 0. Each path-L face makes 2 copies (it made 1). Path S is unchanged.
- A+B: every tree build → 0 in all 80 cells. Each path-S face makes 1 copy (it made a tree).
- New lends: 1 per face per call, unchanged. The lend holder is still per call.
- `PROTO_SLOT_SERVED` equals the number of faces per call.

**Time** (MEASURED, calls 2-100 median, us; main vs prototype in the same run):

| cell | main | A | main | A+B |
|---|---|---|---|---|
| | run 1 | run 1 | run B2 | run B2 |
| sphere proved `classify_point` | 738 | 333 | 1279 | 550 |
| torus proved `classify_point` | 1161 | 390 | 1915 | 629 |
| cylinder proved `classify_point` | 387 | 293 | 678 | 462 |
| c_mouse proved `classify_point` | 1195 | 393 | 1967 | 665 |
| c_bague proved `classify_point` | 1296 | 873 | 2591 | 950 |
| c_mouse proved `classify_points10` | 1948 | 1145 | 3233 | 2026 |
| sphere proved `fire_ray` | 561 | 139 | 903 | 213 |
| c_endcap proved `fire_ray` | 778 | 170 | 1054 | 159 |
| sphere proved `face_at` | 459 | 486 | 819 | **86** |
| torus proved `face_at` | 816 | 810 | 1285 | **101** |
| sphere proved `closest_to_point` | 460 | 455 | 725 | **87** |
| box proved `face_at` (plane) | 45 | 45 | 69 | 54 |
| control: sphere `bounds` (untouched) | 899 | 880 | 1544 | 1465 |
| control: box `classify_point` | 36 | 36 | 52 | 52 |

Run B2 ran at higher load than run 1: its `bounds` control is 1.7x run 1. Run 2 (`26-tree-why-A2.log`) repeats run 1's
ratios within ±15 %.

**Call 1** (MEASURED, indicative). Call 1 is the cold call on a fresh instance (a new instance per prototype setting).
- On proved bodies the proof (`valid_and_outward_failure`) runs classification with the switch on, so under the
  prototype the slots are already filled before call 1 (`slots_before` = `Stf`). Call 1 then mostly costs the first
  `BrepCache` and curve caches. For example, c_mouse `classify_point` call 1: main 2553 us, A 1486 us (run 1).
- For a body nobody has touched, the cost moves to call 1, with nothing saved: endcap fresh `fire_ray` call 1 built
  its tree in the slot (main 72.5 ms with traces, A 77.5 ms). Calls 2-100 then built none.
- The sphere's slot already held an unbound tree (`St`), so under A call 1 only implanted the face boundary
  (`TRIM_TREE_IN` 87-110 us, no base tree) instead of building the whole tree.
- Under traces, call-1 times include backtrace cost; the untraced call-1 values are in the `WHYREP` rows.

**Bit-for-bit equality** (MEASURED):
- `proto_eq`, run once with A and once with A+B: 10 bodies × 3 states (fresh, proved, record round trip) ×
  15 entries, each 3 calls (cold, warm, third). That is **450 entries, 1,350 call results**: every one identical to
  main.
- Plus the later-query comparison: **630 comparisons** (report 25's C2 pattern, 10 × 3 × 3 prior calls × 7 later
  queries). The prior call ran with the prototype; the later query ran on a fresh body with the prototype off, so
  any state the prototype left in the slot would show. All identical.
- Totals: `EQTOTAL same=1080 diff=0` for A and for A+B.
- `tree_why` also compared all 100 outputs of each of the 80 cells with main's (`eq_main=1` everywhere, A, A+B, both
  runs).

**What it does not change** (MEASURED): the standing copy, the snapshot, `BrepCache` (1 per call), and the new lend
per face per call (still per `BrepQuery`). A copy of the slot cache is still made per call: the slot's `EntityRef`
cache cannot be read in the solve's `CacheObject` space without a `map_handles` copy (READ, `p2.rs:489-492`).

## 6. Q4 — is keeping the tree across calls exactly equivalent?

**What the kept cache depends on** (READ):
- the face surface's geometry (the copy in the cache);
- the face's loops: the implanted boundary and its UV trim curves, made during the build;
- the face's zone tolerance (`bind_face`, refreshed on every fetch at `p2.rs:538-541`, so a raised tolerance is
  picked up: verified by read);
- the kernel's cache tolerances and options (`cache_tolerances(k, surface)`, `p1.rs:1099`);
- the fixes in force (`brep_ray_classifies_current_trims` only while editing).

**Reset sites** (READ):
- Surface edited: `edit_notify` on `PreEdit` → `delete_stored_trim_cache` (`surface/surface/p2.rs:5388`, `:5396`);
  likewise a transform at `p2.rs:3731`.
- Face edited: `topology/face/p3.rs:510` → `mark_trim_cache_face_modified` (`p2.rs:1450`). A checked-out cache gets
  the mark when it is stored again (`p2.rs:1420`). The next fetch drops a modified cache unless editing is on
  (`query/face/p2.rs:589`).
- Explicit deletes: `query/face/p2.rs:600`, `:638`; `ray.rs:510`; `p2.rs:2967`.

**Equivalence, item by item:**

| claim | status |
|---|---|
| On an unchanged body, the slot cache answers exactly as a fresh build, for every entry and body tested | **verified** (MEASURED: 2 × 1,080 comparisons + 80 × 100 calls, 0 differences) |
| A raised face tolerance is picked up | **verified by read** (`bind_face` on every fetch) |
| A modified cache is reused while editing | **true and excluded**: the prototype runs only when `!editing_enabled()` and not in a Boolean operation (READ) |
| Every edit that changes a face's trims (edge curve, vertex tolerance, loop change) sends `PreEdit` to that face | **assumed**: not traced, and the brief scope did not cover it. This is the main staleness risk. |
| The slot is not keyed by kernel tolerances or fixes, unlike the per-call `FaceCacheKey` (`live.rs:152`). A body queried under two kernels with different tolerances or fixes would reuse a cache built under the first. | **assumed safe only if a body never changes kernels**; not tested (INFERRED risk) |
| The slot is shared with `clone_for_evaluation` copies (standing copy, snapshot). A cache stored from a copy serves the body. | **verified for the tested bodies** (`SLOT_STORE_FROM_COPY` > 0 in many cells; answers unchanged) |
| Errors: `solve_surface_cache_of_face` surfaces some errors that today's `ignored_trim_cache_result` (`p1.rs:1127`) ignores | **assumed rare**; 0 differences and 0 fallbacks measured, but no failing-build body was in the set |
| Shipping this needs a policy decision: an answer now depends on whether a cache was kept. #20054/#20179 removed exactly that kind of dependence, and the equality runs only show it does not change answers on these bodies. | INFERRED |

## 7. Q6 — bodies without trims re-make them on the copy every call

**Confirmed** (MEASURED). `c_endcap` fresh (0 of 2 uses trimmed) with `classify_point`, `classify_points10`,
`face_at` or `bounds`:
- every one of 100 calls makes 1 UV trim (`UV_TRIM_CREATIONS=1`, `UV_TRIM_ON_COPY=1`), 1 edge-use routine and 2 curve
  trees, on the standing copy;
- the copy is discarded, so the body stays without trims;
- about 41 ms per call (median 40.8-43.6 ms in run 1, 44-78 ms in loaded runs), with 0 surface tree builds.

The `with_body` entries (`fire_ray`, `hit_ray`, `point_on_face`, `closest_to_point`) make the trim on the body itself in
call 1 (`UV_TRIM_ON_BODY=1`, 67-72 ms). The snapshot rule keeps a successful edit (`session.rs:688-708`, `kept`), so
calls 2-100 cost 25-780 us. Report 25 found the same for `ball_minus_corner` fresh; I did not re-measure it here.

**What the ops layer does to give a body its trims** (READ):
- `Body::to_record` (`solidcore-ops/src/body.rs:495`) runs under `with_record` → `with_body`. It calls the converter's
  `to_record` (`solidcore-convert/src/brep/record.rs:130`), whose first line is
  `create_missing_uv_trim_curves(k, b)?` (`record.rs:131`; kernel `topology/brep/p2.rs:504`).
- Every `with_body` entry (`session.rs:688`) keeps the trims the kernel makes during the call.
- `read_else_edit` / `read_else_make` (`session.rs:724`, `measure.rs:63`; used by mass properties, area and similar)
  fall back to the editing form when the read answers `PreparationRequired`, and keep its trims.
- The read entries (`read_body`/`read_held`, `session.rs:749`) never keep them: classification, `face_at` and
  `bounds` run on the standing copy.
- No other ops code calls `create_missing_uv_trim_curves` (READ, grep).
- Per the standing decision that a read never creates trims, no fix was prototyped.

## 8. Q7 — who repeats queries on an unchanged body in the app

From a read-only sweep of `crates/` (READ, spot-checked):
- **No kernel query per frame or per mouse move.** Picking, hover and snapping use mesh indexes
  (`crates/ui-egui/src/pick_index.rs`, `crates/viewport/src/pick.rs`; neither imports solidcore beyond
  `solidcore_record::Vec3`).
- **Joint-origin snap glyphs** (`ui-egui/src/dialogs_assembly.rs:2315`, drawn each frame) do reach the kernel
  (`snap_of` → `kernel::cylinder_face_at`, `:999`) for a non-planar face. They are memoised per world, doc and
  selection in `picked_snap` (`:1163-1180`), and the hovered snap is recomputed only when the hovered item changes
  (`:2325-2329`).
- **Quick Measure** is cached by selection, doc pointer and `session.revision` (`ui-egui/src/quick_measure.rs:32`, `:72`).
- **Joint drag with contact sets** calls `body_distance` once per drag step on newly placed bodies
  (`engine/src/cmd/contact_sweep.rs:450`). The placement changes every step, so these are not repeats of an unchanged
  body.
- **Headless `view.pick`** (`engine/src/cmd/view_pick.rs:87`) runs `fire_ray` once per pixel and body in one
  command. That is many queries on one unchanged body within one call sequence, and the clearest app user of
  warm-call speed.
- **Feature evaluation and name resolution** (`crates/doc/src/eval.rs`, `naming.rs`, `name_lookup.rs`) run kernel
  queries per evaluation. Evaluations are cached by `EvalCache` (`eval.rs:1398`).

INFERRED: repeated identical-body queries in the app come in bursts within one command (`view.pick`, naming
lookups, measure of a new selection), not across frames. The win from keeping trees is per burst.

## 9. Not measured

- The kernel before #21019 (`c6c0178d5c^`). #21019 is already on main, so main and its head are the same here.
- Memory held by trees kept in slots for the body's lifetime.
- Whether every trim-changing edit notifies the face (section 6).
- A body queried under kernels with different tolerances or fixes.
- Bodies outside the 10 in `zz_cls_probe` (no NIST or large-assembly parts).
- Timings on a quiet machine. Every run was at load 27-30.
- The app-level effect: no app run was made.
- `ball_minus_corner` was not in `tree_why`'s body list (it was in `proto_eq`).
