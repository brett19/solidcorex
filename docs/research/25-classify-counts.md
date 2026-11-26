# 25 — Point classification, bounds and ray queries: measured counts (settles U1-U9 of report 24)

Scratch measurement, nothing committed or pushed. Base: origin/main 562396937b (includes #21107). Scratch worktree
`sc-wt/cache-cls-count` (local, branch `cache-cls-count`, 3 scratch commits) and the same on server2. Built and run on
server2 through `sc-slot.sh` (`--profile goldens`, one run per cell, shared machine: times are indicative only).
Raw logs: `logs/25-cls-per_call.log` (846 rows), `logs/25-cls-c2.log` (629 comparisons), `logs/25-cls-repeated.log`.
Marks: MEASURED unless stated.

## 0. Answers first

1. **Copy share of call time.** The standing copy (`clone_standing_for_self`, timed inside the call) costs 3-24 us per
   body (standalone: box 13-20, plate+hole 19-22, fillet box 18-21, sphere 3, torus 4, mouse 10-12, bague 10-12).
   As a share of the call, time-weighted over the 10 bodies and 3 states, cold call:
   **1 point 1.6 %, 10 points 0.5 %, 1,000 points 0.0 % (max 0.5 %)**; `bounds` 0.3 %; `face_at` 0.8 %.
   It matters only where the call itself is tiny: planar bodies. One point on box / plate+hole / filleted box:
   **35 %** (47 us call, 17 us copy), `face_at` there 20-25 %, 10 points 11-14 %, 1,000 points 0.2-0.5 %.
   On every body with a curved spline/sphere/torus face it is 0.1-7 %. Snapshots of the `with_body` entries
   (`face_hit_by_ray`, `fire_ray`, distances, `point_on_face`...): mean 0.7 % cold, 2-4 % warm, but
   **`point_on_face` and `face_normal_at_point` warm: 35-38 %** (28 us call, 11 us snapshot).
2. **Prepared tests.** P1 (every edge use has a trim) holds at entry on **28 of 30 body-states** (cold). It fails
   only for `ball_minus_corner` fresh (16 of 28 uses) and the spline `c_endcap` fresh (0 of 2); after the proof or the record round
   trip it holds everywhere, so P1 is the common case, not the exception (kernel constructors, booleans, fillets and
   file reads all leave trims). P2 (no classification tree to build) holds in the same 28. **P3 as defined in the report
   (slot holds a reusable cache with its tree for every face) holds in only 4 of 30 body-states cold (sphere and torus,
   fresh and proved) and 66 of 450 calls warm.** Planar and conic faces (box, cylinder, plate, fillet box) never leave a
   cache in the slot, so P3 never holds for them even after any number of calls. So P1+P2+P3 all hold for **60 of 450
   cold calls (13 %) and 66 of 450 warm (15 %)**, all of them sphere/torus (plus 2 endcap record). P1+P2 alone hold for
   420 of 450 cold calls (93 %). If the read form needs P3 as defined it will rarely run (spheres and tori only).
3. **C2.** 629 comparisons (10 bodies x 3 states x 3 prior calls x 7 later queries: `classify_points` 200 mixed
   points, `bounds`, `volume`, `check_brep` (All), `face_hit_by_ray`, `fire_ray`, `face_distances`), later query on a
   fresh body vs after a classification / bounds / face_at call: **0 differences, bit for bit** (floats compared as
   shortest round-trip text, volume as bits, check report as Debug). 224 of the comparisons had prior calls that stored
   caches from a copy (up to 301 stores per `classify_points` call), 28 where the copy also built trims the body does
   not have (`ball_minus_corner` fresh: 6 trims made on the copy, body stays 16 of 28; `c_endcap` fresh: 1). No effect
   observed. Caveat below: the store counter includes value-preserving take-and-return.
4. **Prototype of the read form: skipped** (a read-only twin of the face services is not cheap). Proxy, from today's
   path on the 126 calls whose P1+P2+P3 hold at entry: 0 made a trim, 0 raised a tolerance, 0 had a missing
   slot cache in the classification path (`SC_EMPTY_BUILD` 0, `SC_MODIFIED_DROP` 0); but **76 of the 126 built a tree
   anyway** in the solve (`BASE_TREE_BUILDS` 1-4 per call), including spheres whose slot cache has a tree. So a read
   form that refuses on a cache miss "the way the report defines P3" would refuse these; what the solve builds is not
   the slot's tree (see U7). Not measured why.
5. **Repeated picks (100 calls, unchanged body): the per-call rebuild is the cost, not the copy.** See section 5:
   on spline/sphere/torus bodies every call rebuilds 1 face tree per face used (spheres 1, mouse 2, bague 4), 1
   `BrepCache`, 1 whole-cache copy per face and 1 lend per face; on planar bodies 1 `BrepCache`, 1 standing copy, and
   nothing else. Two bodies in a state without trims (`c_endcap` fresh, `ball_minus_corner` fresh) re-make their
   trims on the copy on **every** call and never keep them (endcap: 41 ms per `classify_point`, 100 of 100 calls).

## 1. Instrumentation (scratch, local patch on the scratch worktree)

Counters in `solidcore/crates/solidcore/src/zz_probe_counts.rs` (new), `pub mod` in `lib.rs:47`. Bumps (file:line in
the scratch tree):

| What | Where |
|---|---|
| standing copy, count + ns + call site | `topology.rs:1346-1350` (`standing_copy`), sites in `query/brep.rs:1118` (LazyBrep::get_mut) and `:1159` (Target::mutable) |
| copy made by classifier / by tight box | `query/brep.rs:1156` `COPY_BY_TARGET`, `:1115` `COPY_BY_LAZY`; both set `COPY_LIVE` |
| snapshot (with_rollback), count + ns | `topology.rs:1447-1451` |
| `brep_copy`, `clone_sharing_surface_caches`, `clone_for_trial` | `topology/brep/p3.rs:1870`, `topology.rs:1284`, `:1393` (all 0 in these entries) |
| UV trim created, on the real body vs a copy (address compare with the probe's `REAL`) | `topology/edgeuse/p3.rs:500-504`; edge-use routine `:1744`; face routine `topology/face/p3.rs:1304` |
| tree builds | `surface/surface_cache/p3.rs:989`, `surface/trim_srf_cache.rs:754,768,807` (+ `TREE_BUILD_ON_COPY` `:809`) |
| whole-cache copies | `surface/surface_cache/p1.rs:1795` (clone), `:1979` (map_handles, with site), `surface/trim_srf_cache.rs:477` |
| slot take / store, store while a copy exists, store with tree | `surface/surface/p2.rs:1387,1410-1415` |
| `BrepCache` build | `query/brep.rs:711` |
| solve cache prepare, lends | `query/brep/ray.rs:177`; `query/brep/ray/live.rs:791` shared, `:796` owned, `:799` new |
| W13/W14 | `live.rs:517` (`with_face_entity_mut`), `:404` (`classifies_on_face`), retry at 100x |
| classification cache lookup outcome | `query/face/p2.rs:592,599,603,618` |
| P tests | shims `zz_builds_tree` and `zz_solve_cache_ready` in `query/face/p2.rs` (next to `classification_builds_tree`); `missing_uv_trim` is public |

Probe: `solidcore/crates/solidcore-ops/tests/zz_cls_probe.rs` (own `[[test]]`), driving `Body::bounds`, `classify_point(s)`,
`face_at`, `face_hit_by_ray`, `fire_ray`, `face_distances`, `closest_to_point`, `point_on_face`,
`face_normal_at_point`, `volume`, `check`. Bodies: box 30x20x10, cylinder r6 h20, sphere r10, torus 30/8, plate 60x40x10
with a r6 hole, sphere r15 less a 20-cube corner, box with one r3 edge fillet (the fillet is a cone/cylinder face), and the corpus
files `Endcap.smb` (1 BSpline face), `Mouse.smb` (BSpline + 2 extrusion faces + plane), `Fillet/bague.smb` (4 BSpline
faces). States: fresh = kernel B-rep wrapped by `Body::from_kernel`; proved = after `valid_and_outward_failure`
(what `finish_proved` runs); record = `to_record` then `from_record(trusted)`. Each call runs cold (new body) then warm
(second identical call on the same body). 1 point: one call per kind (inside, outside, on a face, on an edge, 1e-5 from a
vertex); 10 and 1,000: the five kinds interleaved. Points were chosen on a scratch body (inside/outside by
classification of a 600-point scatter; face points as ray hits; edge points via `edge_middle`; vertex points offset
1e-5). The sheet `c_endcap` has no inside points.

## 2. Counts per call (cold, summed over 10 bodies x 3 states = 30 calls; the 1-point row is 150 calls)

| call | calls with standing copy | snapshots | trims on body / on copy | tree builds | `BrepCache` builds | cache copies (`map_handles`) | lends new / shared | slot stores (of which with a copy live) |
|---|---|---|---|---|---|---|---|---|
| `bounds` | 27 (via `get_mut`) | 0 | 0 / 1 | 0 | 0 | 0 | 0 / 0 | 142 (142) |
| `classify_point` (150) | 150 (via `Target::mutable`) | 0 | 0 / 2 | 80 | 150 | 54 | 54 / 0 | 60 (60) |
| `classify_points` 10 | 30 | 0 | 0 / 1 | 48 | 30 | 30 | 30 / 84 | 84 (84) |
| `classify_points` 1,000 | 30 | 0 | 0 / 7 | 96 | 30 | 39 | 39 / 10,873 | 7,698 (7,698) |
| `face_at` | 30 | 0 | 0 / 1 | 23 | 30 | 0 | 0 / 0 | 28 (28) |
| `face_hit_by_ray` | 0 | 30 | 1 / 0 | 36 | 30 | 36 | 36 / 0 | 28 (0) |
| `fire_ray` | 0 | 30 | 1 / 0 | 36 | 30 | 36 | 36 / 0 | 28 (0) |
| `face_distances` (10 pts) | 0 | 30 | 1 / 0 | 23 | 0 | 0 | 0 / 0 | 246 (0) |
| `closest_to_point` | 0 | 30 | 1 / 0 | 23 | 0 | 0 | 0 / 0 | 28 (0) |
| `point_on_face`, `face_normal_at_point` | 0 | 30 each | 1 / 0 | 23 | 0 | 0 | 0 / 0 | 30 (0) |

The 1 trim "on body" in the `with_body` rows is `c_endcap` fresh (0 to 2 trims, and 1 tolerance changed on the body); the
"on copy" counts are `c_endcap` fresh and `ball_minus_corner` fresh. Over all 846 rows: `BREP_COPY_FN`, `clone_sharing`,
`clone_for_trial`, surface-cache clones, trimmed-cache `map_handles`, `make_surface_cache`, `get_surface_cache`, `CHECKED_OUT_BUILD`:
all 0. `with_face_entity_mut` 0, `classifies_on_face` 0, 100x retry 0. So `bounds` copies at the first classified extremum (27 of 30 body-states; the box needs none) and
each classifier call copies exactly once however many points it holds (1 copy for 1,000 points, as the classifier is shared).

Tree builds are not once per body: `classify_points` on bague builds 4 per call, every call (section 5). In the
1,000-point row the lends are 10,873 shared (Arc lends within the call) against 39 new.

Copy time (in-call, mean of cold calls, us) and share of the call, per body (1 point / 10 / 1,000 / bounds):

| body | 1 pt | 10 pts | 1,000 pts | bounds |
|---|---|---|---|---|
| box | 47 us, copy 17 (35 %) | 120, 17 (14 %) | 3,314, 17 (0.5 %) | 15, 0 |
| cylinder | 136, 10 (7.5 %) | 692, 10 (1.5 %) | 66,236, 8 (0.0 %) | 1,726, 9 (0.5 %) |
| sphere | 261, 5 (1.8 %) | 1,222, 5 (0.4 %) | 59,661, 4 | 948, 5 (0.5 %) |
| torus | 576, 6 (1.1 %) | 1,507, 6 | 80,799, 6 | 1,938, 6 |
| plate+hole | 67, 24 (36 %) | 220, 23 (11 %) | 13,413, 22 (0.2 %) | 1,349, 23 (1.7 %) |
| ball less corner | 309, 15 (4.7 %) | 1,342, 15 (1.1 %) | 71,447, 13 | 968, 14 (1.5 %) |
| filleted box | 65, 22 (35 %) | 191, 21 (11 %) | 7,855, 21 (0.3 %) | 282, 21 (7.6 %) |
| endcap | 5,576, 4 | 14,189, 4 | 54,730, 4 | 16,384, 3 |
| mouse | 621, 17 (2.7 %) | 2,529, 17 (0.7 %) | 139,924, 15 | 5,128, 15 |
| bague | 899, 16 (1.8 %) | 3,342, 16 | 189,052, 14 | 5,066, 14 |

(The endcap row is dominated by the 41 ms trim creation on the copy for the fresh state; its proved/record rows are
0.1-1 ms.)

## 3. P1 / P2 / P3 at entry

Cold, per body and state (P1 P2 P3; trims have/uses):

| body | fresh | proved | record |
|---|---|---|---|
| box | 110 (48/48) | 110 | 110 |
| cylinder | 110 (12/12) | 110 | 110 |
| sphere | 111 (4/4) | 111 | 110 |
| torus | 111 (8/8) | 111 | 110 |
| plate+hole | 110 (60/60) | 110 | 110 |
| ball less corner | **010 (16/28)** | 110 | 110 |
| filleted box | 110 (60/60) | 110 | 110 |
| endcap | **010 (0/2)** | 110 | 110 |
| mouse | 110 | 110 | 110 |
| bague | 110 | 110 | 110 |

Fractions: P1 28/30 body-states; P1+P2 28/30; P1+P2+P3 4/30 (sphere, torus x fresh, proved). Over all calls: P1+P2+P3
60/450 cold, 66/450 warm; P1+P2 420/450 cold and 426/450 warm.
P3 stays false after calls on planar and conic bodies because no trimmed cache is stored for those faces (slot empty),
and for bague/mouse spline faces (slot empty too: the cache the classification builds there is not left in the slot).

What passes in the app: the body an application holds after a boolean, fillet or file read is "proved/record"-like:
P1 and P2 true, P3 true only for sphere/torus.

## 4. C2 (U3)

Method in section 0 item 3; 629 comparisons, 0 DIFF, in all three states, including the 28 comparisons where the
prior call made trims on a copy the body lacks (`ball_minus_corner` fresh, `c_endcap` fresh) and the 224 that stored
caches from a copy. Slot stores per prior call from a live copy: `classify_points` 1,000 points 250-301 on the bodies
that store at all (planar boxes 0 to 30; most of the count is value-preserving take-and-return on the same face's
cache once per lend). `P_cls1000` on the bague stores caches: later `check`, `volume` and rays equal. Not shown: a
case where a copy-built cache was built against copy-only trims AND then used by a later call on a body that still lacks
those trims. The two bodies with missing trims (endcap and ball fresh) are exactly that case: the later `classify_points`
re-made the trims itself, and no later query differed. Not measured: separate "new cache stored" vs "same cache returned".

## 5. Repeated picks (100 identical calls, unchanged body, state "proved"; `classify_point` also fresh and record)

| body/call | call 1 us | median 2-100 us | copy us (median) | rebuilt on every call 2-100 |
|---|---|---|---|---|
| box `classify_point` | 46 | 32 | 13 (41 %) | 1 standing copy, 1 `BrepCache` |
| box `face_hit_by_ray` | 37 | 27 | snapshot 14 | 1 snapshot, 1 `BrepCache` |
| cylinder `classify_point` | 475 | 401 | 7 (2 %) | 1 tree, 1 cache copy, 1 lend, 1 `BrepCache`, 1 copy |
| sphere `classify_point` | 792 | 708 | 3 | 1 tree, 1 cache copy, 1 lend, 1 `BrepCache`, 1 slot take+store, 1 copy |
| torus `bounds` | 1,990 | 1,829 | 4 | (as above) |
| plate+hole `bounds` | 1,390 | 1,219 | 20 (1.6 %) | |
| mouse `classify_point` | 1,324 | 1,120 | 13 (1.2 %) | 2 trees, 2 cache copies, 2 lends, 1 `BrepCache` |
| bague `classify_point` | 1,619 | 1,299 | 12 (0.9 %) | 4 trees, 4 cache copies, 4 lends, 1 `BrepCache` |
| bague `face_hit_by_ray` | 1,203 | 907 | snapshot 12 | 4 trees, 4 cache copies, 4 lends, 1 snapshot |
| `c_endcap` fresh `classify_point` | 41,078 | 40,673 | 5 | trim made on the copy every call (1), never kept; body stays 0 of 2 |
| `ball_minus_corner` fresh `classify_point` | 938 | 787 | 9 | 6 trims made on the copy every call; body stays 16 of 28 |

Call 2-100 is only 5-25 % cheaper than call 1 in every case: what the first call builds is mostly built again. The
rebuilt work per call is a tree for each face the solve visits (every spline, sphere, torus, cone face; 0 for planes),
the `BrepCache`, and the per-face copy and lend. Whether keeping these across calls pays: for spline and curved bodies
the tree builds are the bulk of a ~0.4-1.3 ms call (the copy is 1-3 %), so a cached prepared table would remove most
of the call time; for planar bodies there is nothing but the copy and the `BrepCache` (41 % of a 32 us call on a box).
Time of the `BrepCache` build alone was not measured (count only).

## 6. U1-U9

| U | Answer |
|---|---|
| U1 | Copies per call today: exactly 1 standing copy per `classify_point(s)`/`face_at` call (150 of 150, 30 of 30, 30 of 30, 30 of 30), 27 of 30 for `bounds`; the `with_body` entries make 1 snapshot instead (30 of 30 each). Pass rates: section 3 (P1+P2 93 %, P1+P2+P3 13 % cold, 15 % warm). |
| U2 | **No write on a tree build when every trim exists.** 846 rows have P1 true at entry: 0 made a UV trim and none changed an edge or vertex tolerance (hash of all tolerances, before and after); 76 of the 126 P1+P2+P3 rows built a tree. Implants were not inspected beyond that. |
| U3 | **No difference** in 629 comparisons (section 4). |
| U4 | Not prototyped (not cheap). Proxy: of 126 calls with P1+P2+P3 at entry, 0 would have refused for a missing trim, 0 for a classification cache miss, but 76 built a solve tree. The expected 0 refusals holds for trims and for the classification lookups, not for the solve's tree. |
| U5 | **0** `with_face_entity_mut` writes, 0 `classifies_on_face` calls, 0 100x retries over 846 calls (these bodies and points). Not shown for adverse bodies. |
| U6 | Not measured (no read form). |
| U7 | Copy 0.0-0.5 % for 1,000 points, 0.5 % for 10, 1.6 % for 1 point overall but 35 % on planar bodies; `BrepCache` build and lends: counts only (1 build per call, 1 lend per face), no separate time. The tree builds are what repeat. |
| U8 | Not measured (`inside_points` not called). |
| U9 | `fire_ray` and `face_hit_by_ray` do the same counted work per call (36 cache prepares, 36 lends, 36 trees over the 30 body-states each); `fire_ray` on the kept table was not run, so its effect on answers is not measured. |

## 7. Not measured, and caveats

- Read-form prototype (item 4), U6, U8, U9's answers, the time of the `BrepCache` build alone.
- The probe's "P3" is the slot test of the report. It does not predict that the solve builds nothing (76 of 126 P111
  calls built trees). A read form would need a different test; a count of what the solve builds from the slot cache
  was not taken (sphere: slot holds a tree-bearing cache, yet 1 tree is built per call).
- `SLOT_STORE` counts every `store_trim_cache`, including the value-preserving return of a cache taken from the slot;
  "from a copy" means a standing copy exists in this call (set when the classifier or tight box makes it), not that the
  stored cache was built against the copy's trims. The comparison in section 4 is the real test.
- One run per cell, `goldens` profile, shared server, times indicative. The tolerance check hashes edge and vertex
  tolerances only. Bodies are small (4-60 faces); a large body would raise the copy cost (the copy is proportional to the
  arenas, about 0.3-0.4 us per edge use here) but also the call.
- "Fresh" = a kernel B-rep before the ops proof, built on the probe's kernel with all fixes; the ops constructors
  (`make_box`...) wrap the same B-rep with a template instead (not separately run).
- Face points came from ray hits, edge points from `edge_middle`; "on an edge" and "near a vertex" points are
  within tolerance only if the kernel says so (results not tabulated).

## 8. Scratch state

Local: worktree `sc-wt/cache-cls-count`, branch `cache-cls-count` (3 scratch commits, never pushed to
GitHub; also pushed as side ref `push/cache-cls-count` to the server's `sc-main` clone). Server: worktree removed (below), its side ref and branch remain in
`sc-main`, logs `~/cls-*.log` in the server's home. Local copies of the logs: `reports/cache-arch/logs/25-cls-*.log`.
Task `cache-cls-count` registered with `sc-task`.
