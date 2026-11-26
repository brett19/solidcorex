# 29 — Lazy fill-once UV trim behind `&Brep`: prototype and measurement

Local prototype only. Nothing was pushed to GitHub or the fork, and nothing was filed or commented.
- Branch `zz-lazytrim` (3 commits on origin/main 026e4aabc6), a local worktree.
- Server side ref `push/cache-lazytrim-a`, with the server worktree of the same name.
- Labels: MEASURED (counted on the build server), READ (from the code), INFERRED (reasoned, not counted).
- Counts come first. The only timing is the probe's total wall time (1047 s, serial), and it is indicative only.

## Verdict

With the switch on, the four `&Brep` mass reads answer without a write on 96% of reads over a fully stripped body set (MEASURED, 2,943 of 3,086 reads that main answers after settling).
- Main's read forms answer 10 of 3,086 on the same bodies; every other read gets `PreparationRequired`.
- The reads are deterministic over four fill orders and leave the Debug dump, write stamps and tolerances unchanged.
- No copy and no snapshot is taken.
- Settle after lazy reads equals settle without them.
- Each trim is built once per body. A second pass builds nothing.

The answers are not always bit-identical to settle-then-read: 96 of 2,943 differ (MEASURED). Every one of these is on a body where settle changed tolerances.
- Where the lazy trim equals the settled trim, the difference is at most 2.9e-16 relative.
- 10 trims on 7 corpus files differ from what settle builds, because settle builds them after earlier trims raised edge tolerances. On those faces the answers differ by up to 2.1e-5 relative.
- The lazy value is a pure function of the stored body. The settled value depends on settle's order.

The remaining refusals are these (MEASURED, stripped set):
- Tree: 93. The exact box and the containment walk would build a classification tree; that is not lazy yet.
- FitLoosened: 22.
- BoundExceeds: 20.
- NoCertifiedBound: 8. These are the ProceduralBlend-type trims that the item 1 carry cannot cover.
- Interval recovery, brute force and reentrant builds never occurred.

## Item 1: certified bound for a Line trim on a Plane

READ: `query/curve_surface_gap.rs` `TrimPair::of` (~:3341). The old refusal `w_slack != 0 → None` is replaced by a carry, which holds point by point:
- the analytic UV line is within `w_slack` of its NURB at the same parameter (`line_nurb_slack`);
- the plane moves a UV step d by at most L·d (`plane_uv_lipschitz`);
- so the pair's slack becomes `c_slack + L·w_slack`, rounded up in interval arithmetic, and the pair is enclosed with the NURB.

This mirrors `raised_trim_bound`. Report 28 found 4,018 no-bound trims; this covers 3,880 + 124 of them (READ/INFERRED from report 28's split). The 14 ProceduralBlend trims remain.

The carry is gated on the switch, so main still refuses. Ungated, it would change `trim_raise_distance` raises on main (INFERRED).

MEASURED: with the switch on, the lazy acceptance found a certified bound for every Line-on-Plane trim it met. NoCertifiedBound fell to 8 reads (2 slot entries) over the stripped set.

## Item 2: the lazy fill-once trim (READ)

- **`TrimBuildBrep { Mut(&mut Brep), Lazy(&Brep) }`** (`topology/edgeuse/p3.rs:349`). `create_uv_trim_curve_in` (:619) and `evaluate_binormal_uv_in` (:3229) are today's code. Every write goes through `b.write(why)`, and in Lazy mode that write is a typed refusal instead. The writes are:
  - `set_interval` / `trim_other_uv_curves` → IntervalRecovery;
  - the loosened fit (`max_edge_face_gap` caching, `edge_set_tolerance` in `finish`) → FitLoosened;
  - `brute_force` → BruteForce;
  - `delete_uv_trim_curve` / reentrant `get_or_create` → Reentrant.

  Owner clearing, the surface-tolerance sync and the face-surface-cache note run only in Mut. In Lazy, the binormal's surface copy has its owner cleared, which matches Mut.
- **`LazyTrimRefusal`** (:371) has 8 causes. For the prototype it is encoded as `PreparationRequired { operation: "lazy trim: …" }`, decoded by `of`. A real version needs a typed failure.
- **`lazy_uv_trim_curve(k, &Brep, eu)`** (:3006) reads in this order:
  1. the stored trim (own or mate's; a failing record's status is the error);
  2. the slot, keyed by write stamps and `k.tolerances` (the mate is looked up too);
  3. otherwise a Lazy build, then acceptance via `trim_deviation_bound(..., [lim, lim])` with lim = edge tolerance + face tolerance (None → NoCertifiedBound; above lim → BoundExceeds).

  Only then does it insert, if absent (fill once). The owner is set to the SAME member, as `SetUVTrimCurve` does. The kernel's error statuses Unported, Cancelled, InvalidInput, PortInvariant and NotFound propagate. Any other build failure becomes BuildFailed.
- **Slot `LazyTrims`** (`topology.rs:858`) is a `Mutex<BTreeMap<EdgeuseId, Result<Arc<Curve>, Refusal>>>`.
  - It has no write stamp, and its Debug prints the constant "LazyTrims".
  - Copies and snapshots start empty, and records export only stored trims.
  - The key does not cover the kernel's `fixes`. A real version must key on them or show that they cannot change a trim.
- **`fit_tolerance` is now `&Brep`.** It goes through a split `calc_max_edge_face_gap_read` and `Gap::end_points_read`, which are the same computation (heal/gap.rs, edgeuse/p1.rs).
- **`QueryTopology::Lazy(&Brep)`** (`topology/loopuse.rs:144`) is handled in four places:
  - face/p1 `normalized_evaluate_uv` and `uv_trim_curve_in` read the lazy trim;
  - `oriented_trim_boundary_in` skips the up-front create;
  - properties.rs `gap_closure` reads the lazy trim;
  - the tight box and the trim-cache tree refuse as Tree.
- **The `&Brep` forms** route through `read_topology` (properties.rs:2747): Lazy when the switch is on, else today's Prepared after `require_uv_trims`. The forms are `face_area_prepared`, `face_centroid_prepared`, `brep_area_about_prepared` and `brep_mass_properties_estimated_prepared`, plus the new `brep_volume_read` and `brep_properties_read` (:2762, :2777), which use the loose box read only. There is no fallback.

## Item 3: settle (READ, MEASURED)

`topology/brep/p2.rs:521` `settle(k, &mut Brep)` empties the slot and runs `create_missing_uv_trim_curves` in the body's edge-use list order. That order is stored and reads cannot change it, so it is canonical.

MEASURED: on 298 body variants, settling after any lazy reads gave the same stored trims (Debug) and the same tolerance bits as settling the untouched body (0 differences).

## Item 4: measurements (MEASURED)

Probe: `solidcore-convert/tests/zz_lazy_probe.rs`, run serially (`--test-threads 1`, because the switch and counters are process-wide; a parallel first run was discarded). Logs are on the build server at a local worktree.

Bodies:
- 5 primitives;
- 47 Boolean scenarios plus 11 fillet, offset, shell, sweep and revolve bodies (58 "O");
- 28 corpus files (26 distinct: blade_hub_1 = cliff_1 and blade_hub_2 = cliff_2 byte for byte);
- record round trips of K and O (53 of 58 O round-trip).

Each body is measured twice: as built, and with every stored trim stripped. That gives 298 variants.

Reads per variant:
- face area and face centroid for each face;
- body area (FaceMiddle), volume (loose box) and estimated mass properties.

The reference is main's read forms on a settled copy. "Main" is main's read forms on the unsettled copy (switch off). "Lazy" is the same reads with the switch on.

### (a) Reads and refusals (3,091 reads per variant set)

| | main, as built | lazy, as built | main, stripped | lazy, stripped |
|---|---|---|---|---|
| answered | 2,446 | 3,054 | 10 | 2,943 |
| PreparationRequired | 640 | 0 | 3,076 | 0 |
| Tree | – | 16 | – | 93 |
| FitLoosened | – | 4 | – | 22 |
| BoundExceeds | – | 8 | – | 20 |
| NoCertifiedBound | – | 4 | – | 8 |
| IntervalRecovery / BruteForce / Reentrant / BuildFailed | – | 0 | – | 0 |
| mass error (same in reference) | 5 | 5 | 5 | 5 |

Slot entries (stripped): 5,845, of which 5,829 were accepted, 8 BoundExceeds, 6 FitLoosened and 2 NoCertifiedBound. Lazy never answered a read that the reference refused (lazy_only = 0).

### (b) Bit for bit against settle-then-read

| | same | differ | trims compared equal / differ | settle tolerance changes |
|---|---|---|---|---|
| as built | 3,039 | 15 | 894 / 0 | 406 (corpus only) |
| stripped | 2,847 | 96 | 5,819 / 10 | 813 |

- All 32 variants with a difference have tolerance changes from settle. Of the 41 variants whose tolerances settle changed, 9 show no difference.
- Where all trims are equal, the largest difference is 2.9e-16 relative (volume, revolution2). The mechanism is INFERRED: the integrands read edge and vertex tolerances (gap closure, error scale), so a different tolerance moves the last bits.
- Where trims differ (10 trims, 7 corpus files), the largest difference is 2.1e-5 relative (bottle f15 area).
- The O and RO differences (6 each: cylinder_slot, ball_less_corner) are centroid coordinates that should be zero, at about 1e-16 absolute.

### (c) Determinism over fill orders

There are four fill orders: faces forward, reversed, rotated and interleaved, and volume first then faces reversed. Across all 298 variants, 0 answers and 0 slot contents differed from the forward order. A second pass on the same body gave the same answers and built nothing.

### (d) Work counts (stripped set, totals)

| | trim builds | deviation bounds | body copies | snapshots |
|---|---|---|---|---|
| settle | 5,881 | 541 | 0 | 0 |
| lazy, first pass | 5,845 | 5,839 | 0 | 0 |
| lazy, second pass | 0 | 0 | 0 | 0 |

Lazy slot traffic, first pass: 62,336 reads, 56,491 hits and 5,845 builds, with no key resets.

The added work is one certified bound per lazily built trim. That is 5,839 bounds where settle does 541 (settle bounds only the trims it raises). As built: 898 lazy builds, 897 bounds, against settle's 907 builds and 334 bounds.

### (e) Body unchanged by reads

For all 298 variants, the Debug dump, the write stamps and the edge and vertex tolerance bits were equal before and after the lazy reads (0 failures).

## Item 5: lists (READ)

`&Brep`-able now, with the switch on, needing nothing more than this prototype:
- `face_area_prepared` (properties.rs:2497)
- `face_centroid_prepared` (:2532)
- `brep_area_about_prepared` (:2615)
- `brep_volume_read` / `brep_properties_read` (:2762 / :2777, new)
- `brep_mass_properties_estimated_prepared` (:3053); it still refuses where `require_classification_trees` says a tree would be built
- ops `measure.rs` `read_else_make` (:63), used at :616, :658, :1459 and :1513. Its read arm needs no change; the make arm is still reached for Tree and the write refusals.
- `face_box` (already `&Brep`).

Not `&Brep`-able yet: anything through the tight box (`calculate_tight_bounding_box`) or a trim-cache tree. These are the Tree refusals: 93 stripped, 16 as built.

Deletable public items, once lazy reads replace prepare-and-fallback. The `&mut` forms are not deletable: they have many callers, and settle-then-read still needs them for the write refusals.

| item | where | condition |
|---|---|---|
| `missing_uv_trim` | properties.rs:2637 | only `require_uv_trims` (:2660, private) and one ops test (`solidcore-ops/tests/proof_reuse.rs:297`) use it |
| `require_uv_trims` (private) | properties.rs:2660 | goes with it |
| the Prepared arms for trims | face/p1.rs:337, :358, :822; properties.rs:1868 | when Lazy replaces Prepared for mass |
| `read_else_make`'s PreparationRequired arm | ops measure.rs:63-78 | only after trees are lazy, else Tree still needs make |
| `existing_uv_trim_curve`, `normalized_evaluate_existing_uv` | p3.rs:2938, :3783 (pub(crate)) | remain used by `lazy_uv_trim_curve` and query paths; not deletable |

## Item 6: golden units with the switch on (MEASURED, no bless)

- `solidcore --lib` filtered to `mass:: edgeuse trim curve_surface_gap`:
  - switch off: 398/398 pass;
  - switch on (`ZZ_LAZY=1`): 397/398 pass.

  The one failure is `mass::integrator::properties::tests::prepared_measures_ask_for_a_missing_trim` (properties.rs:4143). It asserts that `brep_mass_properties_estimated_prepared` refuses when a trim is missing, which is exactly the contract the switch changes. It is expected and would be rewritten, not blessed.
- `solidcore-ops --lib` + `--test ops` filtered to `measure`, switch on: 34/34 and 23/23 pass.

Draft tolerance-doc sentence:

> A read of a body that lacks a UV trim curve builds the trim without changing the body and keeps it only when its certified distance from the edge is within the edge's and face's tolerances; it is the same curve, fill once per body state, whatever order the reads come in, and the answer can differ from the one after a settle in the last bits where that settle raised a tolerance, or by more where the settle's earlier raises changed a later trim's build.

## Caveats

- Main-mode equivalence with origin/main is INFERRED (every behavioural change is gated on the switch or the Lazy variant) plus the 398 unit tests. No full golden run was made.
- Two refactors run on main too: the `&Brep` gap computation and the binormal split. They are the same code read only (READ).
- The slot key leaves out the kernel's `fixes`.
- Refusals travel as `PreparationRequired` strings.
- Counters are process-global statics (`zz_lazy.rs`), so measurement must be serial.

Disk: local worktree 759 MB; the build server `cache-lazytrim-a/solidcore/bin` 4.4 GB. Both are kept.

---

# Stage 2: lazy fill-once cache tree; variant P (stored trims) and L (lazy trims)

Status at hand-back (2026-11-24 00:15 CET). Branch zz-lazytrim, local commits 6eca8faa0d, 008744f304, 9c39b4a73b. Nothing was pushed except the side ref to the build server.
Measured so far: the corpus part only (46 variants: import files, as-is and stripped). The kernel-body and round-trip variants of the same run were still going on the build server at hand-back. Log: server a local worktree; local copy a local log

## Verdict so far (MEASURED on 46 corpus variants)

- **P agrees with main bit for bit.** P is stored trims made by settle, plus the lazy tree behind `&Brep`. On the same settled body, P and main gave the same answer bit for bit on all 11491 answers that both gave: tight box, 200-point classification, 20 ray fires, and the mass reads. 0 answers differ.
  - 17 answers come only from main. These are classifications whose ray hits an edge or vertex. The `&Brep` radial-sector step is not wired yet (`Unported "zz lazy: radial sector at an edge or vertex hit"`). It is a known gap, not a refusal of the design.
- **Fill order does not matter.** Across 4 family orders, answers differ in 0 cases and tree dumps differ in 0 cases.
- **Reads do not change the body.** On 46 of 46 variants the Debug of the body, the write stamps, and the edge and vertex tolerances are all unchanged after the reads.
- **A repeat pass builds nothing.** It does 0 trim builds, 0 bounds, 0 tree builds, 0 body copies and 0 snapshots. A second pass gives the same answers (46 of 46).
  - **Remaining per-call cost:** the repeat pass still made 834 whole-cache copies. The lazy route still copies the tree per solve through `build_surface_cache` → `map_handles` on path L. Serving `Shared(Arc)` there as well is the fix. It is not done.
- **Tree builds, first pass (P):** box 282, classification 353, ray 0, mass 0. Whole-cache copies: classification 695, ray 139.
- **P on the unsettled body** refuses cleanly: 2821 answers give `PMissing`, the typed precondition failure. Another 931 give a plain `PreparationRequired`, which comes from main's request path that the lazy classifier still reaches on some route (not traced). 7745 answer. 0 differ in a way that is not explained.
- **L (lazy trims) as the comparison column**, against main on the unsettled body:
  - 10262 same and 38 differ;
  - 888 answered only by L, 269 answered only by main;
  - refusals: BoundExceeds 215, FitLoosened 67.
- **P against main on the unsettled body**, where main makes its trims in its own order: 10508 same and 44 differ. These 44 are the creation-order cases. The per-key list is in the ZZPDIFFB lines of the log. The 44 have not been classified yet.
- **Times (indicative, serial, the four families):** main 502 s, P 509 s, L 467 s. Classification dominates. P is not slower beyond noise.
- **Trims read:** the four families together read 3084 of 6168 edge-use trims, so 50% were never read by any family on these bodies. Per family (summed over variants): box 1296, classification 2814, ray 744, mass 3318. Mass reads all faces. A single ray reads few.

## Item status (coordinator's lists)

| Item | Status |
|---|---|
| 1. Lazy fill-once tree per face behind `&Brep` (key = write stamps + tolerances + fixes), served to classification and solves | Built and measured (above). A PMissing failure inside a tree build is not cached; it is retried on each read. |
| 1. Write-site trace | Partly. The key covers every Brep field by name (`write_stamps` destructures all 32 fields, so a new field fails to compile), plus kernel tolerances and fixes. Settle clears the trees. A per-write-site table is not written. |
| 2. `&Brep` forms, no fallback: tight box, `classify_point(s)` (`BrepClassifier::point_3d_classify` routes to `BrepQuery::classify_lazy` under the switch), one ray query (`BrepQuery::ray_fire_lazy`) | Done, except the edge and vertex radial sector (Unported). `face_at` and closest point are not started. |
| 2. Tolerance the tree uses where a raise would supply one | From the code: the tree reads the body's stored face, edge, edge-use and vertex tolerances (`zone_tol_3d`, `vertexuse_vertex_tolerance`). Under P these are the settled (raised) values, so answers equal main's (measured: 0 diff). Under L they are the unraised values. |
| 3. Measurements | Corpus 46/298 measured. Kernel bodies and round trip were running. Closest point is not measured. |
| 4. FitLoosened, BoundExceeds and NoCertifiedBound bodies under settle | Not started for this stage. Under P these refusals do not exist: settle makes every trim, and settle succeeded on 46 of 46. |
| 5. Lists (`&Brep` entry points, deletable items, `read_else_make` make arm) | Not started. |
| 6. Exactness of the 96 differing answers | Not started. |
| P(1). Does a tree build write anything? | From the code: no. The trims are read, never made; the editing flag is local to the adapter. Body diff: unchanged on 46 of 46. Nothing found that it writes. |
| P(2). Differences from main only where creation order differs | Partly. 0 diffs against settled main; 44 against main on the unsettled body. Not classified. |
| P(3). Trims read per family | Measured (above). |
| P(4). Producers leaving trims missing | Not measured separately for this stage. The `missing` column is in the log per variant. |
| P(5). Lists for P | Not started. |
| Cost section (owner's items 1-4: never-read trims, incrementality, persistence, alternatives) | Partly. The probe `zz_c_*` is written (sequence pick → ray → box → volume → Boolean → proof → export; transforms; clone, standing copy, `to_record`, `to_kernel` and kernel file round trips) but has not been compiled or run. From the code, the app keeps `solidcore_record` records and rebuilds the live form with `Body::from_record` (crates/kernel/src/body.rs ~814, ~1087), so trims persist only as far as the record carries them. Not counted. |

## Stage 2b: the cost of preparing every trim ahead (variant P) (MEASURED)

Probe: `zz_c_kernel_bodies` + `zz_c_corpus` (182 variants: kernel bodies, operation results and import files, each as-is and stripped). Server log: a local worktree; local copy a local log Commits up to the "PREP operation names" commit on zz-lazytrim, local only.

**Method.**
- Settle makes every missing trim. A side-channel log records each edge-use pair's settle cost: its build time, the certified bounds inside the build, and any bound computed outside a build. The log counts each pair once: a trim serves an edge use and its mate.
- The sequence runs under P on the settled body and records each step's trim reads:
  1. one pick (point classification);
  2. three more picks and one ray fire;
  3. the tight box;
  4. the volume.

  It then runs a proof (`check_brep` level 1), a Boolean (the body minus a box at its max corner), settle of the result, a second proof, and the export (`to_record`).
- "Never read" counts a pair that settle made but that no step of the sequence up to that point read.
- Times are indicative. Counts come first.

### 1. Trims made but never read, per sequence (182 variants summed)

Settle made 5055 pairs in 35.0 s. 812 of them needed a certified bound, and those took 34.5 s, which is **98.7% of the build time**. The other 4243 trims (lines and planar or analytic cases) cost about 0.45 s together.

| sequence | pairs never read | of which needed a bound | never-read build time | time of the trims it did read | sequence time | never-read share of the sequence |
|---|---|---|---|---|---|---|
| one pick | 4898 | 655 | 34.95 s | 0 | 48.4 s | **72%** |
| 4 picks + 1 ray | 3301 | 448 | 20.3 s | 14.6 s | 48.9 s | **42%** |
| + tight box | 2455 | 275 | 10.3 s | 24.7 s | 467 s* | 2% |
| + volume (mass) | 38 | 8 | 0.14 s | 34.8 s | 472 s* | 0% |

\* The tight box dominates these sequence times: it is slow on the corpus fillet bodies.

- **Per query family:**
  - one pick reads 0 to 24 pairs (a point that resolves without a hit reads none);
  - one ray reads the trims of the faces it meets (25 on blade_hub_1);
  - the tight box reads about a third of the pairs;
  - mass reads every face's trims.
- **Worst bodies, by never-read time in the sequence without mass:** all are import files with fillets.

| body | pairs made | never read | of which needed a bound | never-read time | sequence time | share | share with mass |
|---|---|---|---|---|---|---|---|
| blade_hub_3 | 150 | 82 | 23 | 1.56 s | 2.9 s | 53% | 0% |
| blade_hub_1 | 120 | 56 | 17 | 1.29 s | 2.9 s | 45% | 0% |
| cliff_1 | 120 | 56 | 17 | 1.05 s | 2.1 s | 49% | 0% |
| cliff_2 | 88 | 48 | 17 | 0.86 s | 1.7 s | 50% | 0% |
| blade_hub_2 | 88 | 48 | 17 | 0.75 s | 1.6 s | 47% | 0% |

  Their stripped variants follow, at 0.56 to 1.05 s each. Every other body is under 0.1 s.

### 2. Which producers leave trims missing today (as-is variants: missing pairs / face edge uses)

- **Kernel primitives** (box, cylinder, sphere, cone, torus): 0 missing.
- **Import files:** 613 missing pairs / 4218 face edge uses on 10 of 28 files. These are the fillet corpus files, read with rebuild off.
- **Booleans:**
  - difference: 104 missing / 1624 over 23 results;
  - union: 113 / 1192 over 14 results;
  - intersection: 42 / 224 over 10 results.

  The missing trims are where the faces were cut, for example:
  - slot_through_holes difference: 32 / 336;
  - edge_in_face union: 18 / 96;
  - shared_face unions: 15 / 96.
- **Offset:** box out 21 / 192, cylinder out 2 / 28, round slab in 4 / 60.
- **Sweeps:** revolve_box_profile 3 / 24, seam_pin_sweep 1 / 12, revolve_arc 0.
- **Fillets and shells:** round_slab, variable_round, chain_round, shell_box and shell_cyl leave 0.

### 3. What survives edits, copies and round trips (all 182 variants)

- **Edits.** Boolean on a settled body (172 ran, 10 failed): 568 missing of 19972 face edge uses (2.8%). The surviving faces keep their trims; only the cut faces' new edges lack trims. Settling the result costs 2 ms in total. Proof (`check_brep` level 1) makes no trims: 0 missing after it.
- **Transforms** on a settled body: 45° rotation, scale ×2 with tolerances, and mirror leave 0 missing everywhere. Trims are kept.
- **Copies.** `clone_sharing_surface_caches` and `clone_standing_for_self` lose 0 trims. Snapshot and rollback and the ops layer's `Held` body were not measured separately: `Held` keeps the live body, so it holds whatever trims it has.
- **`to_record` creates the missing trims on the live body it exports.** Confirmed: it made 4626 of the 4898 missing pairs on the unsettled bodies. Export is therefore a hidden settle today.
- **Record round trip** (`to_record` → `to_kernel`, on settled bodies; 150 succeeded, 32 failed on import with "surface::from_param"): 8 pairs lost, all on variable_round (4 per variant). Rebuilding them costs 86 ms per variant. Every other body keeps every trim through the record. Revolutions and analytic surfaces in this set kept theirs. The earlier reading that "analytic and revolution records get no pcurves" did not hold on these bodies; the loss is only on the variable-radius fillet surface.
- **Kernel file round trip:** not measured. My `FromBrepEnv` set-up failed on all 182 bodies; it is probe code to fix.
- **App persistence**, read from the code: crates/kernel `Body` keeps a `solidcore_record` record and makes its live form with `solidcore_ops::Body::from_record(record, true)` on first use. It keeps that form until it is released (`crates/kernel/src/body.rs` ~814 and ~1087). Reopening a design therefore rebuilds from the record, and trims persist as far as the record carries them: all of them, except the variable-round case above.

### 4. Under P, work done that is never done today

- **Today**, main makes a trim only when something needs it: a read of that face, an edit, or the export (`to_record` makes all of them). In a session that exports, main already makes every trim once. P moves that work earlier; it adds none.
- **Where P adds work**, in sessions that never export or save between edits: the never-read trims of the sequence.
  - 35 s summed over 182 variants for a one-pick session, and 20 s with picks and a ray. 99% of it is the ~800 trims that need a certified bound, almost all on 10 corpus fillet bodies (blade_hub, cliff), at 0.5 to 1.6 s per body.
  - Once a mass query runs, it is about 0.
- **The app saves records**, and the record keeps the trims. "Done once and saved" therefore holds for designs once the export has run: reopening finds the trims.
- **Alternatives, from the recording:**
  - *All trims at defined points* (end of an operation, import): 35.0 s total, 99% of it bound-needing trims, mostly unread unless mass runs.
  - *Per face on first need*, by an explicit `&mut` prepare for the face set a read will touch: the trims it did read are 0 s for a pick, 14.6 s for picks and a ray, 24.7 s with the box, and 34.8 s with mass. This saves the 20 to 35 s on the light sequences. The cost is an extra `&mut` step per read site, which the ops layer would make.
  - *Cheap trims eagerly, bound-needing trims on first need*: eager cost about 0.45 s total. The ~800 expensive ones would follow the face-set rule. This captures almost all of the saving with one rule: "a trim whose build needs a certified bound is made by the first `&mut` prepare that needs its face". It is not measured as a rule; the numbers are derived.
- **Edits are incremental already:** after a Boolean, 2.8% of trims are missing, and settling them costs ms.

## Stage 2: full P run totals (MEASURED, 295 variants, 6190 s)

The corpus, kernel bodies and round-trip variants together: 295 variants; 5 more failed the round trip.

- **P against main on the same settled body:** 70337 answers the same bit for bit, **0 differ**.
  - 411 answered by main only:
    - 349 classifications whose ray hit an edge or vertex (the `&Brep` radial sector is not wired);
    - 62 Tree refusals, all on the sphere_tangent_box_union variants (classification 7 and ray 8 or 9 per variant). These are the lazy vertex-use UV computation giving up where main's computes it. This is a real P gap to trace.
- **Fill order:** answers differ 0 times and trees differ 0 times over 4 orders. Body unchanged after reads: 295 of 295. Repeat pass the same: 295 of 295.
- **P against main on the unsettled body**, where main makes its trims in its own order: 66627 same, 51 differ (the creation-order cases), 3659 answered only by P (main's prepared forms refuse with `PreparationRequired`), 411 answered only by main.
- **L against main on the unsettled body:** 66301 same, 49 differ. Refusals: BoundExceeds 254, FitLoosened 74, NoCertifiedBound 42, Tree 62.
- **Times (indicative):** main 819 s, P 800 s, L 518 s.
- **P on unsettled bodies:** 21795 answers give PMissing, 3650 give a plain `PreparationRequired` (being traced; it matches main's own 3659), 45107 answer.
- **Settle's work by producer:** imports made 2602 trims on 37 of 55 variants; operation results 2134 on 84 of 116; primitives 42 on 5 of 9 (the stripped ones).

**Addendum to item 4 (READ).**
- In the app path, operation results are turned into records:
  - `solidcore_ops::Body::to_record` → `with_record` → `solidcore_convert::brep::to_record` on the live body (solidcore-ops body.rs ~519 and ~537);
  - for example, blend.rs ~132 records a fillet's result, and the kernel crate keeps `Body.record`.
- Since `to_record` makes every missing trim on the live body, an app session already makes all trims of each operation result that it records. Under P, the app would do no new trim work on those bodies.
- The added cost from item 1 applies only to kernel-level sequences that read without recording. That is the probe's setting, and possibly ops calls that read a `Held` body before any record of it is made. This is not counted.

## Stage 2: the plain `PreparationRequired` on unsettled bodies (TRACED)

The 3650 plain `PreparationRequired` answers under P on unsettled bodies are **all P's own missing-trim refusal** (`zz P: a needed UV trim is missing`). It arises in the mass reads. The probe's mass helper classified errors with `class_err`, which did not name P_MISSING, so they were counted as plain PREP.

The re-run on blade_hub_1 with operation names shows this:
- P gives `PREP:zz_P` = 89, the same count as main's own `PREP:mass` = 89 on the same keys;
- `PMissing` from the other families = 101.

So under P every refusal on an unsettled body is the typed precondition failure: 25445 in total. There is no other route.

## Stage 2: lists for P (READ; candidates, not compiled as deletions)

**`&Brep` entry points in the prototype.** Each writes nothing to the body: Debug, write stamps and tolerances are unchanged on 295 of 295 variants. Each may fill only the side slot, which under P holds the trees only.
- `query::brep::calculate_tight_bounding_box(k, LazyBrep::Lazy(&b))`.
- `BrepClassifier::point_3d_classify`, which routes under the switch to `BrepQuery::classify_lazy(k, &b, ..)`: no copy, the boundary solve and the ray fires on the lazy trees.
- `BrepQuery::ray_fire_lazy(k, &b, ..)`: the ray solve on the lazy trees and the sector at a face hit. Edge and vertex hits are still Unported.
- `query::brep::lazy_face_tree(k, &b, face)`: the fill-once tree, keyed by (write stamps, kernel tolerances, kernel fixes).
- Mass read forms through `read_topology` → `QueryTopology::Lazy`: `face_area_prepared`, `face_centroid_prepared`, `brep_area_about_prepared`, `brep_volume_read`, `brep_mass_properties_estimated_prepared`.
- Crate-internal: `query::face::p1::point_3d_classify_at_lazy` (no lease), `query::face::p2::point_classify_lazy`, and `TopologyObjects::classifies_on_face` for lazy `BrepObjects` (no body copy).

**Deletable under P** (every finished body has every trim; trees are lazy; a missing trim is the typed failure):
- **Request and retry machinery for trims:**
  - `RayPreparation`'s trim requests and the trim side of the `PreparationRequired` retry loop (`prepared_trim`'s `request_trim`);
  - `point_classify_requested`'s request arm;
  - the trim half of `require_uv_trims`, which becomes the typed check.
- **Tree-before-read guards:**
  - `require_classification_trees`;
  - `classification_builds_tree`;
  - the "tree not built here" `PreparationRequired` arms of the read forms.
- **Copies and leases:**
  - `BrepClassifier`'s private copy (`Target::Shared` with its `clone_standing_for_self`);
  - the `FaceSurfaceCacheGuard` lease in `point_3d_classify_at`;
  - the per-call trimmed-cache rebuild on `RaySurfaceContext` (once the whole-cache copy, still owed, is served shared).
- **The lazy-trim machinery itself (variant L only)**, which P never uses:
  - `TrimBuildBrep::Lazy`, `build_lazy_trim` and the lazy acceptance with its certified-bound check;
  - the `LazyTrimRefusal` causes other than Tree;
  - the `trims` half of the lazy slot.
- **`read_else_make`'s make arm** (solidcore-ops measure.rs ~63; 4 callers): deletable under P, provided `Body::from_record` ends with the `&mut` step that makes the missing trims.
  - The record round trip loses only the 4 variable_round trims, so this step is about 86 ms on that body and nil elsewhere.
  - Until then, the arm is the only answer for those 4 trims.
- **Not deletable under P:**
  - the radial-sector `&mut` chain (until wired behind `&Brep`);
  - settle and `create_missing_uv_trim_curves` (they become the one producer step);
  - main's on-demand creation inside edits (Boolean, fillet), which still makes the 2.8% of trims an edit leaves missing.
