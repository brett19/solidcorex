# Vertex/face gap repeats inside one `check_brep`: are they redundant?

Date 2026-11-23. Measurement only, temporary counters on main b039c29b90 in the scratch worktree
`sc-wt/cache-vfgap-scratch` (branch `cache-vfgap-scratch`, local plus one side ref on the server, never on
the fork, no PR or issue touched). Raw logs: `/tmp/vfgap-run4.log` (main box) and `~/vfgap-run4.log` on server2; the probe
test is `solidcore/crates/solidcore/tests/zz_vfgap_probe.rs`.

## Plain statement

Of the computations of `calc_vertex_face_gap` in ONE check (check 1, fresh body, Level 2; Level 0 is identical to the
count in every scenario):

| body | computations | first for that vertex use (needed: cache empty) | A redundant | B inputs changed (not provably redundant) | C unexplained |
|---|---|---|---|---|---|
| box 40x16x10 + R8x20 cylinder (bar_into_post) | 114 | 60 | 44 | 10 | 0 |
| box 10x10x10 + r4x10 cylinder (box_into_cylinder, the 159 case) | 159 | 84 | 47 | 28 | 0 |
| Fillet/blade_hub_1.smb | 919 | 468 | 76 | 375 | 0 |
| ball less box corner | 55 | 28 | 9 | 18 | 0 |
| box with one filleted edge | 102 | 60 | 42 | 0 | 0 |

On a SECOND check of the same body (check 2) every computation is redundant: the 60/84/468/28/60 "first" computations
each equal that vertex use's last computation of check 1 (identical inputs, identical result bits), and every later
computation in check 2 is class A again. So check 2 has 101 / 134 / 936 / 44 / 84 computations, 0 necessary, 0 unclassified
(table 4). Class C (same inputs, different result) never occurred in 20 body/level/check runs, and the Debug-hash
cross-check of the trim curves never disagreed with the evaluated fingerprint (`dbgonly_diffs=0`).

Mechanism, in one line: no call is forced. Every computation comes through the "not initialised" branch of
`vertex_face_gap(.., force_calc=false)`, because `calc_vertex_face_gap` clears the cache of every vertex use of the same
vertex on the same face (including the one on the mate face use) and refills only its own. Two uses of one vertex on one
face therefore keep wiping each other. The earlier report's "forced recomputation at corner.rs:137/146" is not what
happens in a check: no call had `force_calc=true`, `init_vertex_face_gap` (corner.rs:137) was never reached, and no
notify-reset happened; the only resets are the sibling resets inside the routine itself.

Note on bodies: the brief's text describes the 159-call body as 40x16x10 + R8; per `20-check-gap-storage-counts.md` and
the numbers, 159 is the 10x10x10 box with an r4x10 cylinder, and 114 is the 40x16x10 + R8x20 body. Both were run. "uses" in the
VFGAP lines is the number of vertex uses that have a vertex/face gap (60 and 84), not the 42 that the earlier report
quoted (that was the vertex/edge computation count).

## Instrumentation (all temporary; line numbers in the scratch worktree, `solidcore/crates/solidcore/`)

- `src/zz_vfgap.rs`: the log (`static Mutex<Vec<Ev>>`), the counters (`LOCAL_SOLVES`, `GLOBAL_SOLVES`, `DROPS`) and the
  backtrace helper. `src/zz_probe_counts.rs` + the count-kit patch (applied cleanly): tree builds, surface-cache makes and gets,
  trim creations, with a `peek()` added. `src/lib.rs` declares both modules.
- `src/topology/vertexuse.rs`:
  - `calc_vertex_face_gap` (`#[track_caller]`, line ~624 on main; instrumentation at 632, 650-668, 679, 690-720). This is the routine that computes. The
    projection itself is `compute_uv_point` -> `uv_point_in`; the record is written right after it, so cache hits never reach it.
  - `vertex_face_gap` wrapper (`#[track_caller]`, 784-800): passes its caller's location and the `force_calc` flag to the
    routine (a thread-static `PENDING`), and logs a HIT when the cache is initialised and not forced.
  - `init_vertex_face_gap` (815) and `notify` (2372): log a reset with the caller's location.
  - `local_point_solve` call counter (1466), global solve counter (917), `zz_fingerprint` (2257).
- `src/topology/edgeuse/p3.rs`: `create_uv_trim_curve` logs a trim creation (491); `surface_drop_point` drop counter (268).
- `src/topology/vertex.rs:209`, `edge.rs:2326`, `face/p3.rs:433`: log a tolerance change made through `set_tolerance`.
- `tests/zz_vfgap_probe.rs`: scenarios, the analysis, and the output lines. One `check_brep` per log; check 2 runs on the same body.
- Call path = the `#[track_caller]` location of the call to `vertex_face_gap` (immediate caller) plus the first two solidcore
  frames of a backtrace taken inside the routine, wrapper and routine frames removed.
- Cost deltas are taken around the `compute_uv_point` call only; the fingerprint is computed before it, so it perturbs
  neither the counts nor the per-computation costs.

## The inputs recorded, and why each is one

The routine (`calc_vertex_face_gap` -> `uv_point_in`, vertexuse.rs ~1175-1600) was read in full. It does not read the use's own
cached gap (it is reset first and never read); the seed is rebuilt from the edge use trims. The previous cached value is
recorded as `seed` (init flag, gap bits, uv bits) and compared in the analysis, but it is not an input.

| key | content | why it is an input |
|---|---|---|
| `vpt`, `vtol` | vertex point bits, vertex tolerance bits | the point projected; `vtol + face tol` is the accept threshold of the local solve and the global-solve tolerance |
| `ftol`, `fdomain` | face tolerance bits; face uv domain bits | threshold; the domain bounds and clamps the solve |
| `face`, `fsurf` | face id; surface kind plus natural domain bits plus 9 evaluated points over the face domain | the surface projected on; `is_closed`, `natural_uv_domain` and the local solve are functions of the surface only |
| `kind` | edge vertex use or loop vertex use | selects the branch |
| `ktol` | `Kernel::tolerances` (Debug) | zone tolerances used inside the solves |
| `eu` | this edge use: trim present, which edge use holds it (own or mate), trim interval bits and 5 evaluated points, Debug hash; without a trim: edge id, edge tolerance, edge curve interval and 5 points; orientation | the seed: `normalized_evaluate(eu, 0)`; without a trim it is the 3D point dropped to the surface (a different path with its own numerical result) |
| `prev` | the same for the clockwise neighbour, evaluated at 1 | the seam/singularity test compares its end with this use's start |
| `ftrims` | count and hash of the trims (Debug) of every edge use on the face | only read by a global solve (point classification against the face's trims). Included for completeness: no global solve ran in any scenario |

Not fingerprinted, and assumed: the state of the surface cache tree and the face's `point_test_enabled` / `face_was_modified`
flags (read only by the global solve, which never ran: `GLOBAL_SOLVES` is 0 in every computation), the brep tolerance (the
routine does not read it), and the internal numeric state of a surface that builds a NURB lazily (the computation reads it
through `&self`; output is a function of the surface data).

## Output (Level 2; Level 0 gives identical VFGAP and VFCALLER lines)

All numbers are from `VFGAP`, `VFCALLER`, `VFBSIG`, `VFCOST`, `VFCROSS` lines.

### Table 1: per check

```
VFGAP bar_into_post/L2/check1: uses=60 computations=114 hits=12 A=44 B=10 C=0 first=60 trims_created=15 tol_changes=0
VFGAP bar_into_post/L2/check2: uses=60 computations=101 hits=25 A=41 B=0 C=0 first=60 trims_created=0 tol_changes=0
VFGAP box_into_cylinder/L2/check1: uses=84 computations=159 hits=17 A=47 B=28 C=0 first=84 trims_created=42 tol_changes=0
VFGAP box_into_cylinder/L2/check2: uses=84 computations=134 hits=42 A=50 B=0 C=0 first=84 trims_created=0 tol_changes=0
VFGAP Fillet/blade_hub_1.smb/L2/check1: uses=468 computations=919 hits=33 A=76 B=375 C=0 first=468 trims_created=360 tol_changes=270
VFGAP Fillet/blade_hub_1.smb/L2/check2: uses=468 computations=936 hits=16 A=468 B=0 C=0 first=468 trims_created=0 tol_changes=0
VFGAP ball_less_corner/L2/check1: uses=28 computations=55 hits=6 A=9 B=18 C=0 first=28 trims_created=18 tol_changes=0
VFGAP ball_less_corner/L2/check2: uses=28 computations=44 hits=17 A=16 B=0 C=0 first=28 trims_created=0 tol_changes=0
VFGAP fillet_box_edge/L2/check1: uses=60 computations=102 hits=18 A=42 B=0 C=0 first=60 trims_created=0 tol_changes=0
VFGAP fillet_box_edge/L2/check2: uses=60 computations=84 hits=36 A=24 B=0 C=0 first=60 trims_created=0 tol_changes=0
```
(A+B+C+first = computations in every row.) The blade_hub_1 numbers reproduce the earlier counts (919 on a first check, 936 on a
second); the 33 `hits` of check 1 are cached uses read before a sibling reset reached them.

Body checks (scenario 3 premise): `ball_less_corner` before the check has 16 edge uses on faces with a UV trim and 12 without;
the check creates 18 trims. `bar_into_post` has 50 with and 10 without, and the check creates 15. `fillet_box_edge` has all 60 with a
trim, and the check creates none (its 0 B). box_into_cylinder creates 42, blade_hub_1 creates 360 (and raises 100 edge and 170
vertex tolerances through `set_tolerance`, written by the trim-creation code at edgeuse/p3.rs:1184, 2109 and face/p3.rs:1470).

### Table 2: first computation of each use in check 2 against its last computation in check 1

```
VFCROSS bar_into_post/L2/check2:           A=60  B=0 C=0
VFCROSS box_into_cylinder/L2/check2:       A=84  B=0 C=0
VFCROSS Fillet/blade_hub_1.smb/L2/check2:  A=468 B=0 C=0
VFCROSS ball_less_corner/L2/check2:        A=28  B=0 C=0
VFCROSS fillet_box_edge/L2/check2:         A=60  B=0 C=0
```

### Table 3: by call path (Level 2). A / B / first (C is 0 everywhere); "hit" rows are cache reads

Path L1 = location of the call; the two frames above it are in the last column.

| call site (immediate caller) | up two levels | scenario | check | first | A | B | hits |
|---|---|---|---|---|---|---|---|
| assert_valid/brep.rs:371 and :373 (edge use start/end vertex gap) | `validate_and_update` < `assert_valid::brep` | bar_into_post | 1 / 2 | 60 / 50 | 0 / 0 | 0 / 0 | 0 / 10 |
| | | box_into_cylinder | 1 / 2 | 84 / 66 | 0 / 0 | 0 / 0 | 0 / 18 |
| | | blade_hub_1 | 1 / 2 | 451 / 468 | 0 / 0 | 0 / 0 | 17 / 0 |
| | | ball_less_corner | 1 / 2 | 28 / 21 | 0 / 0 | 0 / 0 | 0 / 7 |
| | | fillet_box_edge | 1 / 2 | 60 / 42 | 0 / 0 | 0 / 0 | 0 / 18 |
| corner.rs:146 (`is_within_xsect_tol_3d_of_faces`) | `assert_valid::vertex::vertex` | bar_into_post | 1 / 2 | 0 / 10 | 41 / 38 | 10 / 0 | 9 / 12 |
| | | box_into_cylinder | 1 / 2 | 0 / 17 | 45 / 49 | 28 / 0 | 11 / 18 |
| | | blade_hub_1 | 1 / 2 | 13 / 0 | 76 / 464 | 375 / 0 | 4 / 4 |
| | | ball_less_corner | 1 / 2 | 0 / 6 | 7 / 15 | 18 / 0 | 3 / 7 |
| | | fillet_box_edge | 1 / 2 | 0 / 18 | 42 / 24 | 0 / 0 | 18 / 18 |
| query/face/p2.rs:1615 (`vertex_face_gap_length`) | `curve_on_classify_mut` < `classify_seam` | bar_into_post | 1 / 2 | 0 / 0 | 3 / 3 | 0 / 0 | 3 / 3 |
| | | box_into_cylinder | 1 / 2 | 0 / 1 | 2 / 1 | 0 / 0 | 6 / 6 |
| | | blade_hub_1 | 1 / 2 | 4 / 0 | 0 / 4 | 0 / 0 | 12 / 12 |
| | | ball_less_corner | 1 / 2 | 0 / 1 | 2 / 1 | 0 / 0 | 3 / 3 |

(In check 2 the brep.rs rows hold the per-use "first of the check", which Table 2 shows to be class A against check 1; the same
holds for the 10/17/6/18 "first" at corner.rs:146 and the 1 at p2.rs:1615.) Rows are exact; the brep.rs hit counts are merged
across :371 and :373. The tolerance pass of `update_tolerances_after_boolean`, `wide_vertices.rs:136` and the merge callers
(`merge/p1.rs:981,930,1129`) are not reached by `check_brep` in any of these bodies.

### Table 4: which resets caused the A repeats, and what each B was

The A repeats are all caused by a sibling reset between the two computations of the same use (the log shows the resetter for each A):
for blade_hub_1 check 1 every one of the 76 A, and for check 2 every one of the 468 (+4), is a reset by the use on the MATE face use
(`matefu`, nearly all A in all runs); in bar_into_post 8 A per check also had a reset by the other use on the SAME face use
(`samefu`, a seam of the cylinder: start and end use of one edge, brep.rs:371/373). Typical pairs: brep.rs:373 computes use X and
resets mate Y; brep.rs:371 then computes Y and resets X; corner.rs:146 later finds X uninitialised and recomputes it (A).

B in blade_hub_1 check 1 (375): trim appeared on this edge use or its neighbour since the last computation 243 (173 with identical
result bits, 70 with a result that differs, uv or gap differences of 1e-13 to 1e-12, i.e. noise from a different seed);
trim replaced / holder changed with a raised vertex tolerance 101 (all identical result); vertex tolerance alone raised 27 (all
identical result); another edge use of the face got a trim and nothing else changed 4 (identical result). B in the three smaller
bodies is all "trim appeared" (10, 28, 18) with identical result in every one. A B whose result is bit-identical is NOT proven
redundant by this fingerprint (the routine was fed a different seed); it only means the answer did not move.

### Table 5: cost of one computation, and what the check does around it

From the code, one computation does: one `local_point_solve` onto the face surface from the seed (no tree, no surface cache),
two reads of an edge use at 0 and 1 (a trim curve evaluation, or without a trim an edge-curve evaluation plus one `surface_drop_point`: 2
drops), a handful of surface evaluations for the seam test (`is_closed`, `natural_uv_domain`), and one surface evaluation for the gap
(`VertexFaceGap::set` -> `end_point_distance`). The global point solve (which builds or leases the face surface cache and classifies
against the face trims) is only the fallback when the local solve fails or lands beyond the tolerance: it ran 0 times in every
computation measured. No computation made or got a surface cache or built a tree (the deltas around the projection are 0 for
`BASE_TREE_BUILDS`, `MAKE_SURFACE_CACHE`, `GET_SURFACE_CACHE`, `SURF_CACHE_MAP_HANDLES`): the 2 to 4 tree builds per check are made
elsewhere, so removing redundant vertex/face gap computations does not remove them and a repeat check's repeated builds are not
caused by this routine.

Per check (counts of `local_point_solve` calls in `uv_point_in` and of drops; "all vfgap" = inside this routine, "class A" = of
those, the redundant ones; "check total" = same counter over the whole check, which also includes uv-point calls from sector and trim
creation code):

| body | check | local solves in vfgap (of check total) | class A solves | drops in vfgap (of check total) | trees built in the check |
|---|---|---|---|---|---|
| bar_into_post | 1 | 114 of 398 | 44 | 20 of 20 | 1 |
| bar_into_post | 2 | 101 of 325 | 41 (101 vs state of check 1) | 0 of 0 | 1 |
| box_into_cylinder | 1 | 159 of 549 | 47 | 56 of 56 | 1 |
| box_into_cylinder | 2 | 134 of 440 | 50 (134 vs state of check 1) | 0 | 1 |
| blade_hub_1 | 1 | 919 of 3045 | 76 | 479 of 487 | 3 |
| blade_hub_1 | 2 | 936 of 2594 | 468 (936 vs state of check 1) | 0 | 3 |
| ball_less_corner | 1 | 55 of 195 | 9 | 24 of 36 | 2 |
| ball_less_corner | 2 | 44 of 151 | 16 (44 vs state of check 1) | 0 | 1 |
| fillet_box_edge | 1 | 102 of 312 | 42 | 0 | 0 |
| fillet_box_edge | 2 | 84 of 294 | 24 (84 vs state of check 1) | 0 | 0 |

So the redundant work is cheap per computation (a local solve, no tree). All 936 vfgap computations of a second check on blade_hub_1 are
redundant, 36% of that check's uv-point solves (936 of 2594); on a first check of bar_into_post the routine is 29% of them (114 of 398), 11% redundant (44).

## Why each caller of the A repeats recomputes, and whether reuse is equivalent

Why: `vertex_face_gap(force=false)` recomputes when the cache is not initialised, and `calc_vertex_face_gap` leaves it uninitialised for
the mate use. This is the upstream behaviour, reproduced faithfully: `SmVertexuse.cpp:1806-1860` (`CalcVertexFaceGap`) clears
`m_sVertexFaceGap3d` of every use returned by `GetVertexusesOfFace(pFace)` before it computes, and then sets only its own; the
copy to the equivalent uses is commented out in the same function ("Vertexuses aren't equivalent. Across a seam we have jumps in
sUV [B678]"). The Rust port keeps both (vertexuse.rs ~645-655 on main, the reset loop, comment "keep equivalent vertexuses
synchronized"). The three call sites:

1. corner.rs:146 inside `is_within_xsect_tol_3d_of_faces` (called from `assert_valid::vertex`, for every vertex of the body): reads
   the use after brep.rs:371/373 and the calls of the other vertices' uses have wiped it. Produces nearly all A.
2. brep.rs:371/373 (`validate_and_update`, start and end vertex gap of each edge use): produce the "first" computations; their
   reset of the mate wipes values that corner.rs:146 then recomputes (the A rows above).
3. query/face/p2.rs:1615 (`vertex_face_gap_length`, in `classify_seam`): hit or recompute for a handful of uses (3 per check in
   the 40x16x10 body).

Would reusing the earlier value in the same check be exactly equivalent? Verified: for every A the fingerprint (all inputs the routine
reads) is bit-identical and the result (uv bits and gap bits) is bit-identical, on 5 bodies x 2 levels x 2 checks (per level 817 A within a check plus 700 first-of-check-2 against check 1 = 1517 comparisons, 0 C). Verified from the code that the routine writes nothing the result depends on (its only writes are the gap cache of the use
and the resets), reads no cached gap and, in these runs, never took the global solve (the only branch that reads the surface cache or the other
trims). Assumed: the surface state outside my evaluated points (a replaced surface with the same 9 points would pass), and that no hidden
input exists for the global-solve branch (not exercised). Not proven for a body where the global solve runs.

An important limit, ARGUED from the code and not measured: the reset is read by the check itself. `assert_valid/brep.rs:360-378`
keeps upstream's pointer semantics on purpose: after computing the start use and then the end use, it reads
`start.vertex_face_gap.gap_3d` from the stored field; where the two uses are siblings on one face use (a closed edge, a cylinder seam: the
`samefu` resets seen in bar_into_post at brep.rs:371/373), the end computation resets the start use and the field reads as undefined.
Keeping the value instead would change what `start_face` holds, so a change must keep the sibling reset in the same-face-use case or show
the check output unchanged on those bodies. The mate-face-use resets (`matefu`) are not the case that comment describes; I did not find a reader
that depends on them, but I did not audit every reader of the field across the crate (grep shows `heal_data/p2.rs:284`, `vertex.rs:1195`
and `assert_valid/brep.rs:376-378` read it without a refresh call).

## Open points, stated plainly

- The 8 `samefu` A cases per check in bar_into_post (the cylinder seam) are where equivalence of reuse is least safe.
- 70 of the 375 B in blade_hub_1 recompute to a different bit pattern (1e-13 noise): the earlier value was computed from the
  3D-drop seed before the trims existed; the later one is the one the check keeps. Those recomputations change stored values in the
  last bits; whether any check outcome depends on that was not tested.
- Not covered: the three non-check callers (`update_tolerances_after_boolean`, merge, `wide_vertices`), which do force.
- Timings were not taken.

## Scratch state left

- Local: worktree `sc-wt/cache-vfgap-scratch`, branch `cache-vfgap-scratch` (5 local commits, the count kit applied, never pushed
  to fork or origin), and the side ref on the server's sc-main `push/cache-vfgap-scratch`. Nothing in the main clone changed (`git worktree add` only).
- Server: the worktree is removed (see the hand-back); logs `~/vfgap-run1.log` to `~/vfgap-run5.log` remain in the home directory.
