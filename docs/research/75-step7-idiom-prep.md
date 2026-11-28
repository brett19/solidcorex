# 75 - Step 7: the idiom hunt ahead of the full Boolean, make, deform and repair

Written for solidcorex, 2026-11-28. Step 7 of 01 §10.2 is layers 7 and 8:

- `scx-boolean` in full (beyond the analytic slice of step 4);
- `scx-deform`;
- `scx-make` in full;
- `scx-repair`.

It is done when the Boolean suite passes, the Boolean algebra properties hold, and the difftest
judges every step-7 operation by outcome.

This note does for step 7 what 72 did for step 4 and 74 did for step 6. Steps 5, 6 and 7 overlap,
so every task below names the step-5 and step-6 rows it waits on, and a task that waits on nothing
can start now. For each crate the note gives:

1. what the pin does, its size, and who calls it;
2. the algorithms and the lessons the pin records;
3. the idiom hazards, with file and line;
4. which tests assert truth and which only assert the pin's output;
5. the performance traps;
6. the geometry it needs, and what can be built now on analytic geometry;
7. the proposed Rust shape.

§1.3 is the capability ledger. §7 is the task table and §7.1 the graph. §8 gives the truth tests per
task. §9 holds the open questions, §10 the case against this plan, §11 the proposed 04 rows.

- **Labels** (as in 57, 63, 72 and 74):
  - READ: read from the pin's code or docs.
  - MEASURED: counted here; the method is given.
  - JUDGED: a design judgement.
- **Pin paths.** `K/` is the pin kernel's `src/`, `O/` is `solidcore-ops/src/`. `B/` is
  `K/boolean/`, `TI/` is `B/topology_intersector/`, `PT/` is `K/topology/proto_topology/`,
  `PC/` is `K/primitive/primitive_creation/`. Line numbers are at the pin commit of 63.
- **Nothing here was built or run on the pin.** It was read with grep, sed and wc only.

## 0. How the counts were made (MEASURED)

- Lines are `wc -l` per file, tests included. "Non-test" means above the file's `#[cfg(test)]`.
- A **swallow** is a non-test `.ok()`, `unwrap_or*`, `Err(_) =>` or `let _ =`, plus the pin's named
  status-ignoring helpers (`ignored`, `ignore_soft`, `ignore_failure`, `ignore_status`,
  `status_free`). Each was counted by grep and read where it is listed by line.
- A **literal** is a float literal below 1e-2 in non-test code.
- "Truth test" means the assertion is a closed form, an algebraic property, an independent oracle or
  a stated invariant. "Pin-output test" means counts, kinds, goldens or layout compared with the pin
  itself.

| Area | Pin files | Lines | Non-test swallows | Unit tests | Integration tests |
|---|---|---|---|---|---|
| Boolean (beyond the slice) | `B/` (33,434), `PT/` (23,105), `K/topology/brep/{cutting, planar_section}.rs` (2,028), `O/{boolean, split, trim}.rs` (2,639) | ~61,200 | ~240 (arena_pair 55, TI p1 30, imprint_ends 23) | ~250 | `O/../tests/boolean.rs` 78, `boolean_api.rs` 29, trim 17, split 19, kernel_cases 269 entries |
| Make: revolve, sweeps, prisms | `K/sweep/` (26,303 with loft), `PC/` (20,700), `K/prism/` (662), `O/{sweep, prism}.rs` and helpers | ~45,000 | ~140 plus 28 `ignored` | ~280 | ops sweep 37 plus a 26-case closed-form table, prism 35, kernel sweep suite ~140 |
| Make: loft, skins, patches | `loft_sections`, `loft_rails`, `warp`, `skin_*`, `section_blend`, `K/nurbs/{certificate, gordon, n_sided}.rs`, `sheet.rs`, `O/{surface, blend_surface, network}.rs` | ~17,400 | ~60 | ~120 | loft_rails 13, section_blend 9, two rails 4, skin 11, section orders 23 |
| Deform | `K/surface/{b_spline_volume*, bend, twist, unbend, srf_in_volume}`, `K/surface/volume/`, `K/curve/crv_in_volume.rs` | 15,353 | ~100 | 85 | none |
| Repair | `K/heal/` (23,663), `O/heal.rs` (329) | 23,992 | ~70 | 66 | ops heal 18 |

## 1. Shapes common to step 7, and the rulings

### 1.1 What the pin does on every path (READ)

- **Retry on failure.** The ops Boolean (`O/boolean.rs:183-226`, `union_copies`) retries a union
  that fails in `make_faceuse` with the operands swapped, then with `rebuild_native` operands. The
  kernel's `glue_all_or_none` (`B/api_brep.rs:433`) restores a snapshot and goes on unglued.
  `classify_remaining` (`B/merge/p4.rs:1397`) casts a ray at fractions 0.45678 and 0.456789 along an
  edge and moves to the next edge when that fails (:1481, :1483). The curve sweep drops the exact
  procedural definition with `.ok()` and fits one instead (`PC/p3/construct.rs:991, :1000`).
  `planar_sides` falls back to the skin on any error (`O/sweep.rs:1225-1314`).
- **Build in NURBS, then recognise.** Every revolve edge builds the B-spline surface of revolution
  first (`K/sweep/rotational_sweep_geometry.rs:867, :925`), then accepts the analytic candidate only
  if 16 sampled side lengths agree within 1e-3 relative (`analytic_revolution.rs:172`). The linear
  sweep builds a NURBS extrusion and recognises a plane or cylinder
  (`trans_sweep_geometry.rs:302-307`). Flat skin walls are found after the B-spline is built
  (`skin_flat_walls.rs:36`, WR-782).
- **Tolerance ladders.** The Boolean widens by 10, 4, 1.25 and 5 times a tolerance at
  `B/merge/p2.rs:1717`, `PT/p2.rs:3707`, `PT/p1/proto.rs:2911` and `proto.rs:40` (ED_TOL_FACTOR),
  with CBI_FUDGE 10 and LOCAL_TOLERANCE_RATIO 1000 in `PT/p3.rs:947, :2443`. `matched_tolerances`
  (`B/operand_tolerances.rs:57`, #3292) copies the coarse operand and checks it at Level2. The SMLib
  healer resets every tolerance from a model-size bucket (`K/heal/heal_data/p2.rs:3035-3058`) and
  grows edge tolerances to `gap_gain * gap` (:3138).
- **Global mode flags.** `set_doing_boolean` is saved and restored by hand at 13 sites
  (`K/sweep/topology_sweep/engine.rs:937-956`) and in the skin (`PC/p2/skin.rs:277-302`), where
  `set_editing(brep, true)` is never unset (:323). The app's heal holds a `static AtomicBool` and a
  `thread_local` (`crates/kernel/src/tolerance_heal.rs:31-37`, outside the kernel).
- **Positional bools and integer modes.** `MergeOptions::new(false, true, false, false, true)`
  (`O/boolean.rs:425`); `TopologyIntersector`'s ten bools and a `u64` lamina mode
  (`TI/p1.rs:1221-1230`); `cap_ends: i32` with four meanings (`K/sweep/api_brep.rs:280`); 22
  `which: i32` parameters in `proto.rs`; `HealerOpType` compared by `code()`.
- **Out-parameters for the faces made.** `create_curve_sweep` fills three `&mut Vec<FaceId>`
  (`construct.rs:826`), the skin four (`skin.rs:123-135`), and `ruled_surface_from_offset` four,
  including `missing_faces: &mut bool` (`PC/p3/ruled.rs:69`).
- **Reads through `&mut`.** `SweepAccess`'s reads take `&mut self` (`access.rs:207, 442, 446, 518,
  522, 526`); `gap.rs:1047, 1189` take `&mut Brep` to read gaps; `sheet.rs:149, 172` likewise;
  `classification.rs:130` copies the whole B-rep per vertex-face gap query.
- **Caps instead of budgets.** MAX_CELLS 4e6 (loft pairing), MAX_HALVINGS 12 and MAX_SEGMENTS
  65536 (loft certificate), DEFAULT_MAX_WIRES 257, MAX_SHRINK 20000 (screw), repetitions 4096
  (whole-body sweep), ten mark and repair passes in `manifold_boolean` (`p4.rs:1030, :2046`),
  1<<16 face and edge lists.

### 1.2 Rulings for step 7 (JUDGED)

These apply the house style of 04 row 107 and 01 §5 to what §1.1 found. Each is argued where the
crate section applies it, and the contested ones are in §9.

- **R1. One Boolean core.** Every Boolean-family entry (two-operand, n-ary, imprint, split, section,
  trim, interference, intersection) is a selection or a truncation of one pipeline: pairs,
  interferences, merged points, face-face, imprint, classify, build (BO1-BO4). The cellular merge
  is that pipeline over n operands without the final selection (04 row 16). No entry has its own
  intersector.
- **R2. No retry, no operand swap, no rebuild.** A failure is a typed cause the caller sees. A
  stage that the pin retries is either right on the first run or `Unsupported` with the cause
  named. The result must not depend on operand order beyond the stated rule (`a`'s ids are kept);
  BO10 tests it.
- **R3. No tolerance ladders.** Coincidence is `max(res, acc₁, acc₂)` (09 §4) everywhere in step 7.
  None of the pin's factors (10, 4, 1.25, 5, 100, 1000) is ported. A stage that needs room proves a
  bound and records the accuracy it reached (a `Raise`), never widens a test.
- **R4. Build exactly, never build-then-recognise.** Make constructs the analytic kind directly
  from the profile when one exists (as MK3 does for extrude): a line revolved is a plane, cylinder
  or cone; an arc revolved is a sphere or torus. Where no analytic kind exists the surface is the
  step-5 procedural kind (revolution, extrusion, sweep), whose evaluation is exact. A NURBS fit is
  built only where the operation is a fit (loft, skin, Coons, Gordon), and then with a certificate.
- **R5. Choose before building, not after failing.** Where the pin catches an error and builds
  another way, scx decides by a predicate on the input first (a path of lines and arcs takes the
  exact construction; anything else takes the general one). Both branches must be correct;
  neither is a fallback for the other's bugs.
- **R6. Caps become budget; certificates get a fixed budget.** Every loop the pin caps charges the
  operation's `Budget` and polls `Stop`. A certificate's halving limit is a fixed per-operation
  amount (01 §5.2) that yields `Unproven` with its bound, never a refusal and never a size taken
  from the remaining budget.
- **R7. `Made<R>` and `Done<C>` carry what the out-params carried.** Constructors return
  `Made<R> { body, faces: R }` with a typed face record per construction (`ExtrudeFaces` is the
  model). Operations return `Done<C> { body, changes, report }`; lineage is `Changes`.
- **R8. No global modes.** `set_doing_boolean`, `set_editing`, `set_make_analytics` and the app's
  static have no counterpart. A behaviour that changes the answer is a typed option on the call
  (01 §7.2), part of every memo key it feeds.
- **R9. Reads are `&Brep`.** No read takes `&mut`. Cached data (box trees, boundary trees) is filled
  by `scx-brep` on first read and charged the same whether warm or cold (01 §6.3).
- **R10. Sampled acceptance becomes a proof or a typed answer.** The pin's 16-sample revolve test,
  6×6 deform exactness grid, 5×5 fold grid on Coons sheets and 20-sample gap peaks are replaced by
  construction (R4) or by a certified bound (the loft and Gordon certificates, SU6's gaps). Where
  no proof exists the answer is `Unproven` with a bound, as for the fold check (Q9).

### 1.3 Capability ledger (READ the pin's pub items and ops entries; JUDGED homes)

"Row" is the task in §7 or an existing row. "Not ported" means the capability is a workaround that
the rulings remove, with the reason.

| Pin capability | Called from ops | scx home | Row or replacement |
|---|---|---|---|
| `boolean`, `boolean_union/difference/intersection` (`B/api_brep.rs:687-722`) | yes | `scx_boolean::boolean` | BO1-BO9 (built); NURBS faces BO23 |
| `boolean_with_options(Imprint)` (:785) | `O/boolean.rs:400`, `trim.rs:311` | `scx_boolean::imprint` | BO13 |
| `boolean_with_options(Slice)` | `O/split.rs:368` | `scx_boolean::split` | BO15 |
| `boolean_with_curves` (:804) | `trim.rs:311` | `Done` of `imprint` carries the section edges in `changes` | BO13 |
| `union_all` (`O/boolean.rs:310`) | yes | `scx_boolean::boolean_all` | BO11, then BO12 |
| `merge_breps`, `boolean_lists`, `boolean_trees`, `non_manifold_boolean` | no | `scx_boolean::merge` (cellular) | BO12 |
| `piecewise_merge` (kernel sweep and offset only) | no | `merge` plus `select` | BO12; callers in MK8 and step 8 |
| `boolean_merge` (stitch, measure in ops) | yes | `merge` plus `select(Union)`; measure's uses move to bodyq | BO12 |
| `boolean_2d` (:902; `O/prism.rs:82`) | yes | `scx_boolean::planar` | BO17 |
| `split_body` (`O/split.rs:423`, two Booleans on copies) | yes | `split` returns both sides from one pass | BO15 |
| `cut_face` (`O/split.rs`) | yes | `split` with a face scope | BO15 |
| `trim_sheet` (`O/trim.rs:276`) | yes | `scx_boolean::trim` | BO16 |
| `PlaneCutter`, `cut`, `do_cut` (`K/topology/brep/cutting.rs`) | no | `section` by plane | BO14 |
| `create_planar_section_curves`, `face_plane_section_curves` (`planar_section.rs`) | sweep only | `section` returning curves | BO14 |
| face/face and body/body intersection | no ops API | `scx_boolean::intersect` | BO18 |
| interference | none in the pin | `scx_boolean::interference` | BO19 |
| same-body refusal (`ptr::eq`, `split.rs:510`) | yes | the same-body shortcut | BO20 |
| caller-stated incidences | none in the pin | `boolean_with` and `Incidences` | BO21 |
| tangent patches (`resolve_tangent_patches`, `p4.rs:509`, #17118) | inside | stated facts, closed form, then tracing | BO22 |
| `matched_tolerances` (`operand_tolerances.rs`) | yes | not ported: coincidence rule (R3; 01 §1.3) | — |
| `glue_self_touching_edges` (WR-754) | inside | not ported as a pass: a body that needs it fails `finish` | — |
| `union_copies` retries | yes | not ported (R2) | — |
| `temporary_brep_transform` origin shift (`p1.rs:1369`) | inside | not ported: scx's evaluation is exact or certified at any origin; BO10 tests a far-origin recipe | — |
| `extrude`, `create_linear_sweep` | `O/sweep.rs:815` | `scx_make::extrude` | MK3 (built); draft walls step 8 taper |
| `revolve` (`directed.rs:61`, `PC/p3/sweeps.rs:474, :618`) | `O/sweep.rs:876` | `scx_make::revolve` | MK4 (analytic), MK4b (procedural) |
| `sweep_along_segments`, `cornered_sweep` | `O/sweep.rs:1011` | `scx_make::sweep` on line and arc paths | MK5 |
| `create_curve_sweep_with_placements` | `O/sweep.rs:1069` | `scx_make::sweep` general path | MK7b |
| `stations`, `fractions` (rotation-minimising frames) | `controlled.rs` | `scx_make::frames` | MK7a |
| `screw_sweep*` | `O/sweep.rs:1831, :1903` | `scx_make::screw` | MK6 |
| `sweep_whole` (whole-body sweep by silhouette, `engine.rs:1171`) | through `screw_sweep_with_body`, member | `scx_make::sweep_solid` | MK8 |
| `create_sweep_along_planar_path` (`PC/p2/planar_path.rs`) | no | `sweep` on a planar path (MK5) | MK5 (no Boolean needed) |
| `radial_sweep` (`local_operation.rs:404`) | through local ops | `scx_make::radial` | MK13 |
| `extrude_region`, `sheet_regions` (`K/prism/`) | `O/prism.rs:184, :231` | `scx_make::prism`, `scx_make::sheet` | MK14 |
| `face_on`, `sheet_on`, `create_planar_faces` (`sheet.rs`, `api_trim_ops.rs:242`) | `O/prism.rs:123` | `scx_make::face` | MK15 |
| `match_loft_sections` | `O/sweep.rs:1133` | `scx_make::loft::pair` | MK9 |
| `loft`, `loft_sheet`, `loft_to_point*`, `skin_with_ends`, `sweep_stations` | `O/sweep.rs` | `scx_make::loft` | MK10 |
| `loft_rails`, `sweep_two_rails`, warp | `railed.rs`, `birail.rs` | `scx_make::loft` with rails | MK10b |
| loft certificate (`K/nurbs/certificate.rs`) | `O/curve_knots.rs:53` | `scx_make::certificate` (01 §1.3) | MK10 |
| Coons (`sheet.rs:469`), `n_sided`, `gordon` | `O/surface.rs:251, :285`; `network.rs` | `scx_surface::{coons, n_sided, gordon}` (01 §1.3), faces by `scx_make::face` | MK11 |
| `section_blend`, `section_blend_surface` | `railed.rs`, `blend_surface.rs:168` | `scx_make::blend` | MK12 |
| `blend_edges`, `fillet_sheets`, `fillet_crossing` (`O/blend_surface.rs`) | yes | `scx_make::blend` on SSI | MK12b |
| `ruled_surface_from_offset` (taper only) | no | step 8 taper | — |
| `create_taper_extrude*`, `taper_walls` | through extrude | step 8 taper | — |
| `BendVolume`, `TwistVolume`, `UnbendVolume`, `SrfInVolume`, `CrvInVolume` | none | volume maps in step 5; `scx_deform::deform` | DF1-DF3 |
| `BSplineVolume` (a stub upstream: always `SM_ERR`) | none | not ported until a caller exists | Q6 |
| `heal_short_edges_and_slivers` (`tolerance_defects.rs:1221`) | `O/heal.rs:183` | `scx_repair::repair` | RP1-RP4 |
| SMLib healer (`heal_data/`, `gap.rs`, `face_props.rs`) | `heal_made` on untrusted records | not ported: it widens tolerances (R3); its gap queries are SU6's bounds and its seam fixes are stitch and `finish` work | Q7 |

## 2. `scx-boolean`: beyond the analytic slice

### 2.1 What the pin does (READ)

**Size and entries.** `B/` is 33,434 lines: `topology_intersector/` (p1 6,650, p2 3,589, p3 2,142,
curved_strip 2,078, coincident_strip 1,112, duplicate_edge_run 362), `merge/` (p4 3,000 with
kernel_adapter 2,548 and arena_pair 2,448, p2 2,483, p1 2,124, p3 1,950, p5 799) and `api_brep.rs`
1,636. The proto topology it builds on is another 23,105 (`PT/p2.rs` 6,958, `p1/proto.rs` 5,187,
`p3.rs` 3,873, `p2/imprint_ends.rs` 1,522). The ops entries are `O/boolean.rs` 557, `O/split.rs`
553 and `O/trim.rs` 892.

The ops layer calls `boolean` and `boolean_with` (`O/boolean.rs:151, :160`), `union_all` (:310,
pairwise level by level; the only n-ary entry), `boolean2d` (:361), `imprint` (:400), `slice` and
`split_body` (`O/split.rs:368, :423`), `cut_face`, and `trim_sheet` (`O/trim.rs:276`).
`boolean_merge` is also called by ops stitch (:476) and measure (:3076, :3314, :3377, :3611).
`piecewise_merge`, `boolean_lists`, `boolean_trees`, `non_manifold_boolean`, `merge_breps` and
the plane cutter have no ops caller.

**The ops pipeline** (`O/boolean.rs`):

1. `matched_tolerances` (`operand_tolerances.rs:57`, #3292): a trial copy of the coarser operand
   and a Level2 check, FEATURE_MARGIN 0.25 (:22).
2. `Witnesses::of` copies the other operand (:276).
3. `sm::boolean`, then `update_tolerances_after_boolean`.
4. An empty result is checked against vertex locations (`refute`, :281).
5. On a radial-sector failure (:231), retry with the operands swapped under `Contract::Proved`
   (:204-215), then with `rebuild_native` operands (:216-226).

**The kernel** (`merge_boolean_by`, `api_brep.rs` near :606) glues self-touching edges first
(WR-754), then runs `manifold_boolean` (`p4.rs:2257`) when both operands are manifold solids and
`non_manifold_boolean` otherwise. `manifold_boolean`: intersect, insert and relate (`TI/p2.rs:1386`),
then `mark_common` (up to ten passes, :1030), coincidence repair (ten attempts, :2046),
`classify_edge_faces`, `classify_remaining` by ray (:1397), delete faces, post-process.
`intersect_topology` (`TI/p2.rs:1530`) pairs faces by a spatial index (WR-527, #1526), checks
surface coincidence, crossings inside coincident pairs (#15718, #1794, #16146), intersects face
pairs (`TI/p1.rs:924`), probes curved strips (#18166) and drops duplicate edge runs (WR-647).

**Recorded lessons** are dense: 48 issue references in `PT/p2.rs`, 32 in `proto.rs` and `PT/p3.rs`,
28 in `TI/p2.rs`. The ones step 7 must answer:

- #372 (`p3.rs:1402`): a failed imprint leaves the operands half edited. scx's operands are `&Brep`,
  so this cannot happen.
- #17118, #16937 (`set_relationships`, `resolve_tangent_patches`): tangent patches. BO22.
- #16935 (`close_intersection_loop`), #432 (`fix_gaps`): curves that do not close. In scx a curve
  from the certified march closes by construction (SM7, SM8) or the pair is `Unsupported`.
- WR-452, WR-453 (`imprint_ends.rs`): extend a curve to its vertex, else move the end. In scx the
  ends are merged points (stage 3) and the imprint cuts at them; nothing moves.
- #16637 (`proto.rs:2283`): widen to absorbed points. Not ported (R3): a merged point is within
  coincidence or it is not.
- #11652, #21801 (`coincidence_bound`, `api_brep.rs:329`), #16640 (`certified_line_gap`): certified
  coincidence. scx has SU6's bounds.

### 2.2 Idiom hazards (READ)

Non-test counts: `&mut Kernel` / `&mut Brep` / swallows / bool fields.

| File | &mut Kernel | &mut Brep | Swallows | Bool fields |
|---|---|---|---|---|
| `merge/p4/arena_pair.rs` | 10 | 8 | 55 | 10 |
| `TI/p1.rs` | 15 | 4 | 30 | 40 |
| `PT/p2/imprint_ends.rs` | 0 | 0 | 23 | 4 |
| `PT/p3.rs` | 4 | 0 | 17 | 3 |
| `TI/curved_strip.rs` | 0 | 0 | 12 | 0 |
| `PT/p2.rs` | 3 | 0 | 11 | 7 |
| `merge/p4.rs` | 0 | 38 | 10 | 19 |
| `TI/p2.rs` | 14 | 0 | 10 | 10 |
| `merge/p4/kernel_adapter.rs` | 3 | 27 | 8 | 7 |
| `merge/p3.rs` | 27 | 13 | 3 | 10 |

The worst sites:

1. `merge/p1.rs:195`, `p4.rs:400-404`: `Err(_) => Ok(None)` through `ignore_failure` and `ignored`.
2. `p4.rs:1481-1483`: ray classification at magic fractions, and on failure the next edge.
3. `p4.rs:809`: `Err(_) => return Ok(false)`.
4. `TI/p1.rs:5025, :5472`: `Err(_) => continue`.
5. `TI/p1.rs:1221-1230`: ten bools and a `u64` lamina mode, set by `set_intersect_lamina_edges`
   (`kernel_adapter.rs:718`).
6. `TI/p1.rs:924`: `face_face_intersect_curves` takes `Option<&mut Vec<_>>` out-params.
7. `merge/p1.rs:105`: `MergeOptions` as five positional bools.
8. `api_brep.rs:433`: `glue_all_or_none` returns 0 on failure and the merge continues.
9. `operand_tolerances.rs:71-73`: a failed check reads as "not matched".
10. `kernel_adapter.rs:318, :542`: `let _ =` on tolerance sync and SSI clearing.
11. `proto.rs`: 22 `which: i32` parameters; `topo_dimension` returns `i32` (:342).
12. `O/split.rs:149`: `Error::Status(SM_ERR)`.

No statics, thread-locals or HashMaps in the kernel's Boolean files. The only shared mutable state
is an `Arc<AtomicU32>` (`kernel_adapter.rs:87`) and two `OnceCell`s (`arena_pair.rs:199, :287`).

scx's slice already avoids every one of these (04 rows 135-155). The hazards matter for step 7
because the entries the slice lacks (imprint, slice, trim, the 2-D Boolean) are the ones that set
the mode flags: each mode becomes its own entry (R1).

### 2.3 Truths and tests (READ)

Most traced tests assert validity, solid count or topology counts (`[6,12,8]` inline at
`O/boolean.rs:116`, `[12,27,18]` at `tests/boolean.rs:3680`). Those record SMLib's seam and merge
choices and are not ported as assertions (72 §8.1 rule: counts printed, not compared). The truth
tests to port:

1. `tests/boolean.rs:37` two overlapping boxes: volumes 15, 7, 1.
2. `:3585` `joins_as_both_less_the_shared`: inclusion-exclusion; used at :3633, :3672.
3. `:2948`, `:2990`: ten wedges cut, then joined, in turn; equal to the target less or plus them.
4. `:3528`: a symmetric cut through a half pipe is the same for every length past it.
5. `:500`: a union that fills a void is the whole box.
6. `:2791`: truly empty differences and intersections; lens and circle areas at `:746`, `:4455`.
7. `boolean_api.rs:381`: closed solids with analytic volume.
8. `boolean_api.rs:483`, `:509`: A − A is empty, also with STEP tolerances and for a bore minus its
   copy.
9. `boolean_api.rs:873` (closed form at :843), `:563`: a tiny sphere against a box edge; a shallow
   cap.
10. `tests/trim.rs:371` (slot area on a half cylinder), `:627` (chains cut in either order agree);
    inline `O/boolean.rs:477, :489, :503` (union_all against closed forms).

The pin tests commutativity once (the either-order flag in `joins_as_both_less_the_shared`). It has
no associativity, idempotence, absorption or De Morgan test. BO10 adds them.

### 2.4 Perf traps (READ)

- Up to three Boolean attempts, each with whole-body copies and `rebuild_native`; a witness copy per
  subtraction or intersection; a trial copy and a Level2 check per Boolean (`matched_tolerances`).
  None is ported (R2, R3).
- `split_body` runs two Booleans on copies (`split.rs:483-484`); `both_sides` copies per side
  (:383). BO15 runs one pass and selects twice.
- Proto-vertex creation scans linearly (`proto.rs:4224`, `PT/p3.rs:2360, :2478`): O(n²) in merged
  points. scx's stage 3 uses a spatial index (BO1).
- `split_faces` loops tools × faces (`split.rs:282`); `trim_sheet` sorts a row on every insert.
- Curves are classified against faces twice (`TI/p3.rs:1635-1638`).
- Fixed sampling: curved strip PIECES 32, EDGE_PIECES 16, COMMON_POINT_STEPS 32, MINIMAX_STEPS 64.
  scx replaces each with a certified bound or the march's own step control.

### 2.5 Geometry it needs

| Piece | Needs | Analytic now? |
|---|---|---|
| Face pairs on NURBS faces | SM9 (SM1, SM6b, SM8 for the general intersector) | no |
| Imprint of a curve with no exact pcurve | the step-5 pcurve fitter | no |
| Classify on NURBS faces | step 5's ray against NURBS (or `sides_along` alone where every piece has a section) | no |
| Plane against any surface (sections) | `scx_ssi::analytic` for quadrics; SM9 for NURBS | quadrics yes |
| Tangent contact by tracing | SM6b | stated facts and closed forms yes (SM2) |
| 2-D Boolean of coplanar regions | `scx-planar` crossings (PL1) and PL6 | yes |
| Interference | stages 1-2 and BW3's contact bounds | yes |
| N-ary and cellular merge | the slice's stages, generalised | yes |

### 2.6 Proposed shape (JUDGED)

All entries take finished `&Brep` operands of one document and `&mut Cx`, and return `Done<C>`.
`BooleanError` stays the crate's one error type; new causes are added to the existing
`BooleanRefusal`, `BooleanUnsupported` and `BooleanDefect` (04 row 135).

```rust
// The existing entry (BO1), unchanged.
pub fn boolean(a: &Brep, b: &Brep, op: BooleanOp, cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;

// N-ary: one operation over many operands, in the order given (BO11, then BO12).
pub fn boolean_all(first: &Brep, rest: &[&Brep], op: BooleanOp, cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;

// The cellular merge (BO12): every operand imprinted on every other, each cell labelled by the
// operands it lies in. A Boolean is a selection over cells.
pub fn merge(operands: &[&Brep], cx: &mut Cx<'_>) -> Result<Merged, BooleanError>;
pub struct Merged { pub body: Brep, pub changes: Changes, pub cells: Cells, pub report: Report<NoItems> }
impl Merged {
    pub fn select(&self, keep: &dyn Fn(Membership) -> bool, cx: &mut Cx<'_>)
        -> Result<Done<NoItems>, BooleanError>;
}

// Imprint (BO13): the tool's intersection with the target written into the target's faces and
// edges; nothing is removed. `Dimension` says what is imprinted: edges only, or edges and the
// tool's faces that lie on the target.
pub fn imprint(target: &Brep, tool: &Brep, what: Imprint, cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;

// Split (BO15): both sides from one pass. `Tool` is a surface (bounded by the body) or a sheet.
pub fn split(body: &Brep, tool: &Tool, cx: &mut Cx<'_>) -> Result<Done<Sides>, BooleanError>;
pub struct Sides { pub front: Vec<RegionId>, pub back: Vec<RegionId> }

// Sections (BO14): the sheet (or wire, for a sheet operand) where a surface meets the body.
pub fn section(body: &Brep, by: &Surface, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// Trim a sheet by a sheet (BO16): the pieces of `sheet` on the kept side of `by`.
pub fn trim(sheet: &Brep, by: &Brep, keep: Side, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// The 2-D Boolean of two coplanar sheets (BO17).
pub fn planar(a: &Brep, b: &Brep, op: BooleanOp, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// Intersection without a build (BO18): the curves and points where two bodies meet, as a wire body.
pub fn intersect(a: &Brep, b: &Brep, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// Interference (BO19): a decided answer, never a body.
pub fn interference(a: &Brep, b: &Brep, cx: &mut Cx<'_>) -> Result<Interference, BooleanError>;
pub enum Interference { Apart { gap: Bracket }, Touch { at: Vec<Contact> }, Overlap { depth: Bracket } }

// Stated incidences (BO21): facts the caller asserts, each checked; a false one is Refused(FalseFact).
pub fn boolean_with(a: &Brep, b: &Brep, op: BooleanOp, facts: &Incidences, cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;
```

- **The same-body shortcut** (BO20) is inside `boolean`: when `a` and `b` are the same `Brep`
  (`ptr::eq`, or equal body ids in one document after 04's id rules), union and intersection
  return `a` and subtraction the empty body, with no stages run. Two bodies that are equal but not
  the same are run through the pipeline; BO10's idempotence tests prove the pipeline gets them
  right.
- **Imprint order.** 01 says "imprint in a stated dimension order". The rule: vertices first, then
  edges, then faces, each in id order, so an edge's cut points exist before the edge is cut. That
  is already the slice's stage order; `imprint` stops after stage 5.
- **Membership** is a small bit set of operand indices, so a cell's label is exact. A cell that is
  `On` a coincident face carries the side of each operand's material, as BO4 already records.
- **Non-manifold results.** Sections, trims and the cellular merge produce sheets, laminas and
  regions that are not one manifold solid. `Unsupported(NonManifold)` stays only for what scx-brep
  cannot represent; everything 04 row 16 allows is built (BO25).

## 3. `scx-make`: revolve, sweeps, prisms, faces

### 3.1 What the pin does (READ)

**Revolve.** `analytic_revolution.rs` (1,041), `rotational_sweep_geometry.rs` (1,736) and
`PC/p2/rotational.rs` (366). A partial turn (`PC/p3/sweeps.rs:618`) splits curves on the axis
(WR-590), then runs the generic topology sweep with the rotational geometry; every edge builds the
B-spline surface of revolution, and the analytic candidate is accepted only if corners, start
tangent, turn sense and 16 sampled side lengths agree within 1e-3 (:172). A line perpendicular to
the axis gets a bilinear plane swollen by 1.1 (:907). A full turn (`:474`) tries the analytic
surface per curve and falls back to `SurfOfRevolution`, swallowing that failure (:579); spline ends
near the axis are snapped onto it (#17204); shells are stitched and nested (#478). Lessons: #545,
#1463 and #1490 (never inside out), #17085 (a cone below MIN_HEIGHT is refused), #1491.

**The topology sweep** (`topology_sweep/`, about 8,000 lines with `store.rs` 2,993, `engine.rs` 2,034
and `access.rs` 890) is the generic engine: a `SweepAccess` trait of about 60 methods, an
edge-to-face store, merge, and the whole-body sweep (`engine.rs:1171`) that runs HCR (0.001, 5°) to
find the silhouette, sweeps it, moves back faces and merges at 20°.

**Path sweeps.** `sweep_placed` (`O/sweep.rs:952`) tries a line path (linear sweep), an arc path
(rotational), a path of lines and arcs (`sweep_along_segments`, each piece built separately, then
`copy_faces`, planar caps and stitch) and otherwise the curve sweep (`construct.rs:869-1061`: a
traced NURBS net, the exact procedural definition kept if its bound is within ZONE_TOL_3D, else a
fitted one, both errors swallowed). The cornered sweep (`cornered.rs`, no ops caller) mitres by
rotation θ/2 stretched 1/cos(θ/2). The planar path (`PC/p2/planar_path.rs`) sweeps each piece,
trims mitres by `brep_intersect` and unions the pieces (WR-780, WR-793, WR-400). Stations
(`stations.rs`) give closed-form rotation-minimising frames on lines, arcs and helices; the
controlled sweep skins them cubically, uncertified (#13637), refusing corners and closed paths
(#13638, #13639). The screw sweep (`screw.rs`) shrinks spans by ×0.99 until the lift error is within
tolerance, carrying a `ScrewMotion` definition (#2572).

**Radial sweep** (`local_operation.rs:404`, cones only): outer and inner faces and annular caps
built separately, one stitch at the input bound, an outward check, then a Boolean subtract.

**Prisms and faces.** `extrude_region` (`K/prism/extrude.rs:37`), `sheet_regions`
(`K/prism/sheet.rs:115`, the only HashMap in these files), `create_planar_faces`
(`api_trim_ops.rs:242`), `face_on` (`sheet.rs:83`).

### 3.2 Idiom hazards (READ)

| File | .ok() | unwrap_or | Err(_)=> | let _ = | &mut Brep/Kernel | literals |
|---|---|---|---|---|---|---|
| `PC/p1/primitives.rs` | 7 | 3 | 0 | 0 | 67 | 4 |
| `api_primitives.rs` | 0 | 2 | 0 | 2 | 27 | 5 |
| `api_brep.rs` | 0 | 5 | 0 | 0 | 22 | 0 |
| `screw.rs` | 0 | 4 | 0 | 0 | 13 | 3 + ~10 inline |
| `store/classification.rs` | 13 | 0 | 0 | 1 | 3 | 0 |
| `PC/p3/construct.rs` | 6 | 3 | 1 | 3 | 0 | 0 |
| `O/sweep.rs` | 1 | 6 | 1 | 0 | 4 | ~25 |

Plus `ignored()` swallows: `analytic_revolution` 9, `primitives` 8, `PC/p3/sweeps` 5.

The worst sites:

1. `classification.rs:130`: copies the whole B-rep per vertex-face gap query; error swallowed.
2. `construct.rs:991, :1000`: exact and fitted definitions dropped by `.ok()`; `:731` a reskin
   failure ignored.
3. `rotational_sweep_geometry.rs:191`: `Err(_) => Ok(true)`, a failed line test reads as a line.
4. `engine.rs:155`: a failed cancellation check reads as "not cancelled".
5. `K/sweep/extrude.rs:98`: any error, including cancellation, becomes `InvalidInput`.
6. `PC/p3/sweeps.rs:579`: a face silently dropped.
7. `engine.rs:1219-1222`, `:1009`: HCR at 0.001 and 5°, merge at 20°; `planar_path.rs:641` stitch at
   0.01; `O/sweep.rs:134-149` PATH and DRAFT constants 1e-6 to 1e-12.
8. `rotational_sweep_geometry.rs:907, :967`: swell 1.1, normal agreement 0.8.
9. `access.rs`: six reads through `&mut self`; `bounding_box` (:620) with an out-param.
10. `api_brep.rs:280`: `cap_ends: i32` (none, at curves, at offset, both).
11. `TopologySweepOptions::self_intersection` stored and never read (`topology_sweep.rs:161`).
12. `set_doing_boolean` at 13 sites; `set_make_analytics`, `set_same`, `set_higher` setters.

### 3.3 Truths and tests (READ)

The truth tests to port (closed forms; the rest check face kinds, layout or the pin's own frames):

1. `tests/screw_sweep.rs:271` square tooth by Pappus, with the wall deviation within tolerance.
2. The ops 26-case closed-form table (`O/../tests/sweep.rs:188-505`, relative 1e-9 at :543):
   rectangles revolved full and quarter (:298, :302), a circle swept straight and along an arc
   (:307, :320), a bent path (:343), screw threads (:474, :492), extrudes and drafted frustums
   (:199, :224-281).
3. `tests/wr_590_revolve_splits_curve_on_axis.rs:85`: three faces and the Pappus volume.
4. `api_brep.rs:1735`: a whole sphere's volume.
5. `tests/sweep_cornered.rs:112` (area × length on an L), `:178` (the end-scale integral), `:363`.
6. `tests/sweep_stations.rs:74`, `:505`: the helix frame turns by its torsion (1e-12).
7. `tests/issue_2905_revolved_vase.rs:158`: the meridian volume.

### 3.4 Perf traps (READ)

- Every revolve edge builds the B-spline surface even when the analytic one wins, then samples 32
  lengths per side and 8 plane points. R4 removes both.
- The curve sweep fits the NURBS net and also builds the procedural definition.
- The whole-body sweep runs HCR and a merge per repetition, up to 4,096.
- The screw shrink loop runs up to 20,000 times, with an unbounded inner loop down to 1e-4.
- Segment and cornered sweeps build each piece as its own body, then copy and stitch globally. MK5
  builds one Draft with the shared section edges known.
- The planar path runs a Boolean union per piece and an SSI per mitre. MK5 cuts mitres by the
  closed-form intersection of each wall with the mitre plane.

### 3.5 Geometry it needs

| Construction | Surfaces | Analytic now? |
|---|---|---|
| Revolve of lines and arcs | plane, cylinder, cone, sphere, torus; exact pcurves (a meridian is a line in uv) | yes (MK4) |
| Revolve of other curves | the procedural revolution kind (step 5) | no (MK4b) |
| Sweep of a line-and-arc profile along a line-and-arc path | planes, cylinders, cones, tori; mitre edges are conics (plane against quadric, `scx_ssi::analytic`) | yes, when every wall is a quadric or torus (MK5) |
| Sweep along a general path, controlled sweep | the procedural sweep kind (step 5) | no (MK7b) |
| Screw sweep | the procedural sweep kind with a screw motion | no (MK6) |
| Whole-body sweep | HL5's silhouettes, the cellular merge, procedural kinds | no (MK8) |
| Radial sweep of cones | cones and planes | yes (MK13) |
| Prisms and sheets from planar regions | planes, cylinders (arcs) | yes (MK14) |
| Faces on surfaces | any `Surface`; iso-line edges are exact on every kind | yes (MK15) |

### 3.6 Proposed shape (JUDGED)

Constructors as in MK1-MK3: inputs are validated values (`scx_planar::Region`, `Frame`, `Span`),
the document is `&Document`, and the result is `Made<R>` with a typed face record.

```rust
pub fn revolve(profile: &Region, about: &Axis, turn: Turn, doc: &Document)
    -> Result<Made<RevolveFaces>, MakeError>;
pub enum Turn { Full, Partial { from: Angle, to: Angle } }
pub struct RevolveFaces { pub start: Option<Vec<FaceId>>, pub end: Option<Vec<FaceId>>, pub walls: Vec<Vec<FaceId>> }

pub fn sweep(profile: &Region, path: &Path, frame: PathFrame, doc: &Document)
    -> Result<Made<SweepFaces>, MakeError>;
pub fn frames(path: &Path, law: &FrameLaw, cx: &mut Cx<'_>) -> Result<Frames, MakeError>;
pub fn screw(profile: &Region, motion: &Screw, doc: &Document) -> Result<Made<SweepFaces>, MakeError>;
pub fn sweep_solid(tool: &Brep, path: &Path, cx: &mut Cx<'_>) -> Result<Done<NoItems>, MakeError>;
pub fn radial(cone: &RadialCone, doc: &Document) -> Result<Made<RadialFaces>, MakeError>;
pub fn prism(regions: &[Region], frame: &Frame, span: Span, doc: &Document)
    -> Result<Made<Vec<ExtrudeFaces>>, MakeError>;
pub fn sheet(regions: &[Region], frame: &Frame, doc: &Document) -> Result<Made<Vec<FaceId>>, MakeError>;
pub fn face(surface: &Surface, bound: &FaceBound, doc: &Document) -> Result<Made<FaceId>, MakeError>;
```

- **Profiles touching the axis.** A profile segment on the axis is not a face; its ends become
  pole vertices (a sphere's or cone's apex) with `OnChart` pcurves (SM1, SM1b). A profile that
  crosses the axis gives a body that passes through itself, so it is `Refused(CrossesAxis)` with
  the crossing point. The pin splits such a profile (WR-590) and revolves each side; that builds a
  different solid from the one asked for, so scx refuses and lets the caller split (Q3).
- **Caps.** `RevolveFaces::start` and `end` are `None` for a full turn. An open (uncapped) result is
  a separate constructor (`revolve_sheet`), never a `cap: bool`.
- **Mitres.** A corner in a line-and-arc path is a mitre plane through the corner bisecting the
  turn. Each wall meets it in a conic (or a line), computed in closed form; the wall is trimmed
  there with an exact pcurve. A path that turns back within its profile's reach is
  `Refused(PathTooTight)` with the radius that fits (a `Resize` remedy).
- **No stitching.** Like MK3, every construction knows its topology: the shared edges are made
  once and used by both faces, so `scx-stitch` is not called.
- **Errors.** `MakeError` gains the new causes in its existing `Refusal`, `Unsupported` and
  `Defect` (MK1). A construction that needs a step-5 kind not yet built is
  `Unsupported(NeedsKind { kind })`, so callers can see the boundary.

## 4. `scx-make`: loft, skins, patches

### 4.1 What the pin does (READ)

**Section pairing** (`loft_sections.rs`, 3,762): each section wound like the one before; vertices
paired by a cyclic least-cost alignment (distance after moving to the centroid and scaling to unit
area, plus tangent difference, `UNPAIRED` 0.1 per lone vertex), searched as a shortest path for
every start by divide and conquer (`best_walk`), capped at MAX_CELLS 4e6 and then a banded walk
of 8 passes. A lone vertex cuts the other section at the same share of length (#16943). Full
circles are cut at the neighbouring section's directions.

**The loft certificate** (`K/nurbs/certificate.rs`, 2,021): an upper bound on the distance from
the skin to a rail between stations, holding whatever nodes the untrusted projection gives. τ is
cut at both knot sets; the patch is restricted by de Casteljau and read along its diagonal by the
Farouki-Rajan product; the homogeneous difference `S_h W_r − R_h W_s` is bounded by its hull box
plus Higham γₙ terms; failing pieces halve (MAX_HALVINGS 12, MAX_SEGMENTS 65536). The rail loft
adds a station at each failing span's middle and re-skins, up to 257 wires, then refuses
(`RailPastStationCap`); no tolerance is loosened. Also `on_circle`, `turn_about_axis`,
`meridian_collapse`. Lessons #12920, #17790, #17907, #18018: why these are certificates and not
sampled tests.

**Rails and warp** (`loft_rails.rs`, `warp.rs`): one or two rails use an affine station map; three
or more fit an affine move then an inverse-multiquadric warp (#12188). The two-rail sweep is The
NURBS Book §10.4.

**Coons, Gordon, n-sided.** Coons (`sheet.rs:469`) makes opposite sides compatible and calls the
upstream patch; no certificate; folds are looked for on a 5×5 normal grid. Gordon
(`K/nurbs/gordon.rs`) is `L1 + L2 − T` with crossings from curve intersections, refitting an
incompatible network by a monotone map doubling from 8 to 64 points per span, then certified and
refused past the tolerance. n-sided (`n_sided.rs`) fills N quads round a centre; G0 is certified,
G1 is not; rational curves are refused.

**Section blend** (`section_blend.rs`): Hermite end rows G0-G3, interior derivatives by finite
differences.

### 4.2 Idiom hazards (READ)

- `PC/p2/skin.rs:277-302, :323`: the global mode flags (§1.1).
- `skin.rs:123-135`: four `&mut Vec` out-params, `curves: &mut [Option<Curve>]`, a positional
  `cap_ends: bool`.
- `skin_flat_walls.rs:41, :51-57, :84-93`: `.ok()` chains; a failed replacement is skipped.
- `loft_rails.rs:1311, :1328`: projection errors dropped; `certificate.rs:514` falls back to the
  line midpoint (by design: the bound holds for any node).
- `O/sweep.rs:1269, :1310`: errors swallowed to fall back to the skin; `:1001` `Err(_) =>
  Error::Argument`.
- `loft_sections.rs:245, :326, :826, :1240-1259`: default directions and lengths by `unwrap_or`.
- `api_surface_ops.rs:177, :226`: approximation at a literal 0.001; `skin.rs:98` planarity at
  `tol*1000`.
- `loft_rails.rs:566, :1141`, `warp.rs:68, :222, :278`: collapse and pivot floors 1e-6 to 1e-12.
- `OffsetRuleOptions::surface_intersection: bool` (a mode); `create_coons_patch(tol, true, …)`.

### 4.3 Truths and tests (READ)

1. `gordon.rs:866`: a network of lines is the bilinear patch.
2. `O/surface.rs:543`: three arcs bound a sphere octant (side points at |r| = 10).
3. `O/../tests/sweep.rs:1359`: a loft through stacked circles is the cylinder (25π·12 to 1e-9);
   `:1334`, `:1398` closed-form centres and end weights.
4. `tests/loft_rails.rs:147`: similar sections on a straight rail give the frustum.
5. `tests/sweep_two_rails.rs:130`: diverging line rails scale the profile in closed form.
6. `tests/section_blend.rs:132`, `:47`: quadratic sections blend to the quadratic; two G0 sections
   give the ruled surface.
7. `certificate.rs:1658`, `:1678`: an exact arc is proved on its circle; a bulge is refuted.
8. `n_sided.rs:276`, `O/surface.rs:668`: a pentagon of lines fills flat.
9. `tests/skin_parallel_sections.rs:87`: the skin stays between its end planes.

The certificate is compared with an independent sample only through `sampled_off`
(`tests/loft_rails.rs:119`), which uses the kernel's own closest point. MK10's test samples with
the testkit's exact evaluation instead. Most `loft_sections` tests (37) assert the pin's pairing.

### 4.4 Perf traps (READ)

- Each rail-loft round re-skins every wire and projects every station onto every rail: about
  W·S·R closest-point calls per round, up to 257 wires. MK10 re-skins only the spans the new
  station changes where the interpolation is local, and certifies only those spans.
- Pairing visits up to 4e6 cells; a length bisection of 100 iterations per cut.
- Gordon rebuilds and recertifies on each refit doubling.
- `span_bound` allows 65,536 pieces per span, each with a projection.

### 4.5 Geometry it needs

All of §4 except pairing needs NURBS surface construction: global interpolation with given
parameters, skinning, knot insertion, degree elevation and compatibility, iso-curve extraction,
and Bernstein restriction and products for the certificate. `scx-nurbs` and `scx-nurbs-surface`
have evaluation and refinement; the interpolation and skinning are 01's "construction by
certified approximation" in step 5, which has no row yet (Q1). Pairing (MK9) needs only curves.
The Coons patch of four exact boundary curves needs only compatibility and the bilinear blend, so
it can start once knot insertion and degree elevation are in `scx-nurbs-surface`.

### 4.6 Proposed shape (JUDGED)

```rust
pub fn loft(sections: &[Section], how: &Loft, cx: &mut Cx<'_>) -> Result<Made<LoftFaces>, MakeError>;
pub struct Loft { /* rails, end conditions, closed, fit; #[non_exhaustive], with_* */ }
pub fn pair(sections: &[Section], budget: &mut Budget, stop: &Stop) -> Result<Pairing, MakeError>;
pub mod certificate { pub fn span_bound(/* skin, rail, stations */) -> Bound; }
pub struct LoftFaces { pub start: Option<FaceId>, pub end: Option<FaceId>, pub walls: Vec<Vec<FaceId>> }
```

- `Made::body`'s faces carry the certificate's bound as their recorded accuracy. A span the
  certificate cannot prove within the document's resolution under its fixed budget is recorded as
  a `Raise` with the bound reached (R6), never a refusal; `RailPastStationCap` becomes that.
- Flat walls are planes because the section pair is planar and coplanar with the rail, decided
  before the skin is built (R5), not by replacing a built B-spline.
- `scx_surface::{coons, gordon, n_sided}` return a `NurbsSurface` with its certified deviation, and
  `scx_make::face` bounds it.

## 5. `scx-deform`

### 5.1 What the pin does (READ)

**The app cannot deform a body today.** No kernel function maps a body through a volume and ops has
no deform entry. The pin has volume geometry kinds that are read from records and evaluated:
`BendVolume` (`bend_volume.rs`, 1,973), `TwistVolume` (1,208), `UnbendVolume` (1,671), `SrfInVolume`
(1,370), `CrvInVolume` (`K/curve/crv_in_volume.rs`, 1,978), the generic `Volume` chain
(`surface/volume/p1.rs` 3,278, `p2.rs` 2,057). `BSplineVolume` is a stub that always returns
`SM_ERR` (upstream builds without `USE_VOLUMES`). Sheet-metal develop does not use the unbend
volume; it unrolls with its own rigid maps (`fold.rs:75, :412`). `api_transform.rs:152-157` refuses
to transform a standalone volume curve or surface.

**The maps.** Bend sends (u, v, w) to (u cos(v/d), u sin(v/d), w), singular on the axis; unbend is
its inverse; twist rotates about the axis by an angle proportional to height. A `Volume` holds an
orienting transform and a chain of next maps, with derivatives by the chain rule. Inversion is a
guess then a global or local point solve. Exactness is decided by sampling a 6×6 grid and
recognising three bend cases (plane normal to the axis → plane; plane parallel to the neutral
plane → cylinder; plane normal to the binormal → plane through the axis), their unbend inverses,
and one twist case (plane normal to the axis → rotated plane). Lessons: WR-45 (third-derivative
terms wrong upstream), WR-46 (sign of Duw), WR-59 and WR-60 (box bounds and trimming across the
period), #17549, WR-49, WR-33.

### 5.2 Idiom hazards (READ)

`volume/p2.rs` has 12 `unwrap_or`, mostly `unwrap_or(Vec3::UNDEF)` (:228, :254, :1021, :1106,
:810); `bend_volume.rs:178` and `unbend_volume.rs:177` map an error to `(Vec3::Z, false)`; `let _ =`
drops statuses at `bend_volume.rs:648, :741, :847`; `volume/p1.rs:404` drops a next map it cannot
copy. `EvalOptions` (`p1.rs:202`) has four behaviour bools; owner flags are integers. No statics or
`&mut Brep`.

### 5.3 Truths and tests (READ)

85 unit tests, none of which deforms a body. Truths to port: the closed-form maps
(`bend:1567`, `twist:1078`, `unbend:1488`), finite-difference derivatives (`bend:1580`, `twist:1089`,
`unbend:1500`, `p1:2918`, `p1:3053`, `p1:2660`) and box containment (`bend:1775`).

### 5.4 Perf traps (READ)

Every domain test copies the volume and builds a curve in it (`p1.rs:833-895`); compound
evaluation walks the chain per point; the exactness test costs 36 samples per call.

### 5.5 Proposed shape (JUDGED)

```rust
pub enum VolumeMap { Bend(Bend), Twist(Twist), Unbend(Bend) /* chain: Vec<VolumeMap> */ }
pub fn deform(body: &Brep, map: &VolumeMap, cx: &mut Cx<'_>) -> Result<Done<NoItems>, DeformError>;
```

- The maps are geometry: step 5's volume maps, in `scx-surface` (01 §1.1). `scx-deform` holds only
  the body operation.
- Each face goes to its exact image where one exists, decided by the face's kind and its frame
  relative to the map (R4, R10), never by sampling: the pin's three bend cases, their inverses, and
  the twist cases (a plane normal to the axis; a cylinder or cone coaxial with the twist axis maps
  to itself, which the pin misses). Lines map to lines or arcs by the same rule.
- Every other face becomes the procedural in-volume kind (step 5), evaluated exactly through the
  map. Its pcurves are unchanged: the map acts on 3-D, and a face's uv stays the source's.
- Topology is unchanged, so the body is the source's Draft with new geometry: no stitch.
- A body that meets the bend axis is `Refused(MeetsAxis)` (the map is singular there).

## 6. `scx-repair`

### 6.1 What the pin does (READ)

There are two healers, and neither uses a Boolean.

1. **The certified healer** (`K/heal/tolerance_defects.rs`, 1,286), solidcore's own, the one the app
   uses (`O/heal.rs:183` from the app's tolerance heal, used by STEP import and Repair):
   1. cut zero-length Bézier pieces from B-spline edges, accepted within the edge tolerance and
      within half of it to the faces;
   2. cluster vertices joined by edges shorter than their tolerance (union-find), target the
      cluster box's centre;
   3. squeeze an edge only if interval de Boor enclosures put its whole curve within its tolerance
      of that point, re-end neighbouring edges there (lines rebuilt, B-spline end control points
      moved), each new curve certified within half its tolerance of its faces (GAP_SHARE 0.5);
   4. collapse two-edge faces whose curves are certified coincident, keeping the larger tolerance;
   5. repeat 3-4 at most faces+1 rounds.

   Refusals are reported and never retried, and no tolerance grows. The ops wrapper heals a copy,
   runs the full check on both, and keeps the copy only if no tolerance grew, it fails fewer tests,
   and no failure class got worse (`O/heal.rs:236-311`).
2. **The SMLib healer** (`heal_data/`, `gap.rs`, `face_props.rs`; about 22,000 lines), ported for
   parity, called only by `heal_made` on untrusted records. It widens tolerances by a model-size
   ladder and by `gap_gain * gap`.

### 6.2 Idiom hazards (READ)

- `tolerance_defects.rs`: 7 `&mut Brep` or `&mut Kernel`, `:380` a failed line becomes "no curve",
  `:447`, `:493`, `:905` errors become an infinite gap; `:548` a literal `1e-12 * size`.
- `O/heal.rs:239-240`: `.ok()` on lookups, so a removed entity skips the growth check.
- The SMLib healer: `ignore_status` 9 times; 18 `let _ =` in `p2.rs`; 42 bool or tri-state fields;
  `HealerOpType` by integer code; `gap.rs` reads through `&mut Brep`.
- The app's `static AtomicBool` and `thread_local`.

### 6.3 Truths and tests (READ)

The 18 tests in `O/../tests/heal.rs` are truths: `heals()` (:481-527) asserts that the full check
passes, exact counts, no tolerance grew, and `|ΔV| ≤ area × largest bound`. Port all of them, in
particular :534, :575, :611, :632, :958, :827, :1062; :1402 tests the SMLib path and is ported
only if Q7 keeps it. The 66 kernel unit tests and the goldens assert the pin's state.

### 6.4 Perf traps (READ)

Two full checks per accepted heal; `collapse_short_edges` restarts the edge scan after each pass
with a linear `find` per edge (:728-750); faces+1 outer rounds.

### 6.5 Proposed shape (JUDGED)

```rust
pub fn repair(body: &Brep, what: &Repairs, cx: &mut Cx<'_>) -> Result<Done<RepairCause>, RepairError>;
pub struct Repairs { /* short_edges, slivers, empty_pieces; #[non_exhaustive], with_* */ }
pub enum RepairCause { NotWithinTolerance { edge: EdgeId, bound: Bracket }, NotCoincident { face: FaceId, bound: Bracket }, /* … */ }
```

- Items are the defects found (each short-edge cluster, each sliver, each empty piece) in id order,
  so the default reports the lowest failing one and `Skip` mode repairs the rest (01 §5.2). This
  replaces "reported and never retried".
- Every edit is certified locally (step 3's enclosure, the GAP_SHARE rule on SU6's bounds), so the
  result is valid by construction. The run-time check-compare of the ops wrapper is not ported; the
  tests assert it instead (Q8).
- No tolerance grows. A repair that would need a larger accuracy is a `RepairCause`, never a raise.
- Lines and planes exactly; B-spline end control points moved; analytic edges other than lines
  are converted to their exact NURBS form only for the moved end (an arc re-ended is still an arc
  if both ends stay on its circle, which the enclosure decides).

## 7. Task table

Model: Opus where a correctness argument is the work (a proof of completeness, a certificate, an
order-independence argument); Sonnet where the work is porting tests, mapping or plumbing. "Edges"
names each new Cargo dependency; each must be legal under layers.txt after LY2.

| Id | Crate | Work | Needs | Edges | Model |
|---|---|---|---|---|---|
| LY2 | layers | Add `scx-deform` after `scx-hlr` and `scx-repair` after `scx-make` in layers.txt; correct 01's boolean → stitch edge (Q2); new crates take test bodies from testkit | — | — | Sonnet |
| BO10 | boolean (tests) | The algebra suite on the testkit recipes (§8.1): commutativity, idempotence, A − A = ∅, absorption, De Morgan within a box, inclusion-exclusion, associativity, A = (A − B) ∪ (A ∩ B); judged by volume, area, the T10 oracle and the contract; run twice and compared by bits | — | — | Sonnet |
| BO11 | boolean | `boolean_all`: n-ary union, subtraction and intersection as a balanced fold in operand order, work summed in order; port `union_all`'s closed-form tests | BO10 | — | Sonnet |
| BO12 | boolean | `merge` and `select`: the cellular merge over n operands (stages 1-6 generalised, membership per cell, two-sided faces and laminas by 04 row 16); `boolean` and `boolean_all` become selections; proof that a cell's membership does not depend on operand order | BO10, BO25 | — | Opus |
| BO13 | boolean | `imprint` (stop after stage 5) with the `Imprint` choice; section edges in `Changes` | — | — | Sonnet |
| BO14 | boolean | `section` by a surface: sheet result for a solid, wire for a sheet; plane against quadrics now, NURBS after SM9 | BO25 | — | Opus |
| BO15 | boolean | `split` by a surface or sheet: both sides from one pass; `cut_face` as a face-scoped split; port `O/split.rs` tests that assert areas and volumes | BO12 | — | Opus |
| BO16 | boolean | `trim` a sheet by a sheet; port `tests/trim.rs:371`, `:627` | BO12 | — | Opus |
| BO17 | boolean | `planar`: the 2-D Boolean of coplanar sheets on `scx-planar` regions, faces built with exact pcurves | — | — | Sonnet |
| BO18 | boolean | `intersect`: the curves and points two bodies meet in, as a wire body, from stages 1-4 | — | — | Sonnet |
| BO19 | boolean | `interference`: apart (with a certified gap), touching (contacts), or overlapping (a certified depth); stages 1-2 with BW3's bounds | BW3 | boolean → bodyq (exists) | Opus |
| BO20 | boolean | The same-body shortcut; tests that equal but distinct bodies give the same results through the pipeline | BO10 | — | Sonnet |
| BO21 | boolean | `boolean_with` and `Incidences`: stated face-on-face and edge-on-face facts, each checked (`Refused(FalseFact)`), used to skip their SSI | — | — | Opus |
| BO22 | boolean | Tangent contact: stated facts, then closed form (SM2), then tracing (SM6b); lifts the BO7 limits on tangent pairs | BO21; tracing waits on SM6b | — | Opus |
| BO23 | boolean | NURBS faces: stage 4 on `General` (SM9), stage 5 pcurves by the fitter, stage 6 classify by step 5's ray against NURBS | SM9, the fitter, the NURBS ray | — | Opus |
| BO24 | boolean | Lift `Unsupported(Pcurve)` on analytic faces whose cut curve has no closed-form pcurve (a quartic on a torus) by the fitter | the fitter | — | Opus |
| BO25 | boolean | Non-manifold results: sheets, laminas, mixed regions in the build stage; `Unsupported(NonManifold)` kept only for what scx-brep cannot hold | — | — | Opus |
| DT4 | difftest | The step-7 outcome difftest: t_slice widened to every step-7 entry and constructor, each side judged against truth, pin defects recorded in `pin_defects.rs`, work rows in `slice_work.txt` | BO11, MK4 (first rows); grows with each task | — | Sonnet |
| BB1 | bench | Perf rows before and after each step-7 task: work counts against the pin with side-channel timers, no committed counters | DT4 | — | Sonnet |
| MK4 | make | `revolve` of line and arc profiles, full and partial: plane, cylinder, cone, sphere, torus built directly with exact and `OnChart` pcurves, poles from segments on the axis, `Refused(CrossesAxis)`; `revolve_sheet` | SM1b for poles (built) | — | Opus |
| MK4b | make | `revolve` of other curves onto the procedural revolution kind | step 5 procedural kinds | — | Sonnet |
| MK5 | make | `sweep` of a line-and-arc profile along a line-and-arc path: walls by kind, closed-form mitres, one Draft, no stitch; the planar path is this case | MK4 | make → ssi (mitre conics) | Opus |
| MK6 | make | `screw` on the procedural sweep kind with a screw motion; exact cylinder for a profile line along the axis | step 5 procedural kinds | — | Opus |
| MK7a | make | `frames`: rotation-minimising frames in closed form on lines, arcs and helices, least rotation at corners; port the stations tests | — | — | Sonnet |
| MK7b | make | `sweep` along a general path and the controlled sweep, on the procedural sweep kind | MK7a, step 5 procedural kinds | — | Opus |
| MK8 | make | `sweep_solid`: the whole-body sweep from HL5's silhouettes and the cellular merge | HL5, BO12, MK7b | make → hlr, make → boolean | Opus |
| MK9 | make | `pair`: section pairing by the cyclic least-cost alignment, exact optimum under the budget (Q5) | — | — | Opus |
| MK10 | make | `loft` with the certificate: skinning through the step-5 constructor, `span_bound` ported, local re-skin per added station, `Raise` on an unproved span | MK9, step 5 surface construction | — | Opus |
| MK10b | make | Rails: one and two rails, the warp for three or more, the two-rail sweep; certified by MK10 | MK10 | — | Opus |
| MK11 | surface, make | Coons (four exact boundaries, compatibility, certified deviation), Gordon, n-sided; `face` bounds them | knot insertion and degree elevation in `scx-nurbs-surface`; Gordon also MK10's constructor | — | Opus |
| MK12 | make | `section_blend` with Hermite ends G0-G3 | step 5 surface construction | — | Sonnet |
| MK12b | make | Sheets between sheets (`blend_edges`, `fillet_sheets`, `fillet_crossing`) | MK12, BO14, SM9 for NURBS sheets | — | Opus |
| MK13 | make | `radial`: the hollow cone built directly (outer, inner, annular caps) with no stitch or subtract | — | — | Sonnet |
| MK14 | make | `prism` and `sheet` over several planar regions; port `O/prism.rs` and `prism/tests.rs` truths | — | — | Sonnet |
| MK15 | make | `face` on any surface by a uv bound or a planar region in uv; iso-line edges exact | — | — | Sonnet |
| MK16 | make (tests) | The closed-form make table: the ops 26-case table, Pappus, cornered and stations truths, against T9 | MK4, MK5 | — | Sonnet |
| DF1 | deform | Crate; `deform` with the exact image rules of §5.5 on analytic faces; `Refused(MeetsAxis)` | step 5 volume maps (the bend, twist and unbend maps only) | deform → surface, brep | Opus |
| DF2 | deform | Faces with no exact image onto the in-volume procedural kind | DF1, step 5 procedural kinds | — | Sonnet |
| DF3 | deform (tests) | Jacobian volume closed forms, bend ∘ unbend = identity, derivatives against finite differences in exact arithmetic, the pin's map tests | DF1 | — | Sonnet |
| RP1 | repair | Crate; `repair` with items per defect; short-edge clusters and squeezes on lines and planes exactly | SU6 (built) | repair → surface, brep, check (findings) | Opus |
| RP2 | repair | B-spline ends: enclosures, control-point moves, GAP_SHARE certified on SU6; zero-length piece cutting | RP1 | — | Opus |
| RP3 | repair | Sliver collapse by certified coincidence | RP1 | — | Opus |
| RP4 | repair (tests) | Port the 18 ops heal tests as truths; one-defect recipes at δ = 10·acc, acc, acc/10 | RP1-RP3 | — | Sonnet |

### 7.1 Graph and critical path

```
LY2 (first: before DF1 and RP1 create crates)
BO10 ──┬─> BO11 ──> DT4 ──> BB1
       ├─> BO20
       └─> BO12 (with BO25) ──┬─> BO15
                              ├─> BO16
                              └─> MK8 (HL5, MK7b)
BO25 ──> BO14 ──> MK12b
BO13, BO17, BO18, BO21 alone;  BO21 ──> BO22 (tracing: SM6b);  BW3 ──> BO19
BO23 (SM9, fitter, NURBS ray);  BO24 (fitter)
MK4 ──┬─> MK5 ──> MK16
      └─> MK4b (procedural kinds)
MK7a ──> MK7b (procedural kinds) ──> MK8;  MK6 (procedural kinds)
MK9 ──> MK10 (surface construction) ──> MK10b;  MK11, MK12 (surface construction) ──> MK12b
MK13, MK14, MK15 alone
DF1 (volume maps) ──┬─> DF2 (procedural kinds)
                    └─> DF3
RP1 ──┬─> RP2 ──┐
      └─> RP3 ──┴─> RP4
```

- **Can start now:** LY2, BO10, BO13, BO17, BO18, BO21, BO25, MK4, MK7a, MK9, MK13, MK14, MK15, RP1.
- **Startable once a step-7 or step-6 row lands:** BO11, BO12, BO14 to BO16, BO19 (BW3), BO20,
  BO22's stated and closed-form halves, DT4, BB1, MK5, MK16, RP2 to RP4.
- **Waits on step 5:**
  - BO23 on SM9, the pcurve fitter and step 5's ray against NURBS;
  - BO24 on the fitter; BO22's tracing on SM6b;
  - MK4b, MK6, MK7b and DF2 on the procedural kinds;
  - MK10, MK10b, MK12 on step 5's surface construction (interpolation and skinning); MK11 on knot
    insertion and degree elevation;
  - DF1 on the volume maps.
- **Waits on step 6:** BO19 on BW3; MK8 on HL5.
- **The critical path to "done when"** is BO10 → BO12 (with BO25) → BO15 and BO16, with DT4 growing
  beside it. BO10 can fail on main today, and every failure it finds is a bug to fix before BO12
  builds on the pipeline. The NURBS Boolean (BO23) is outside the analytic gate, as in step 4.
- **Step-8 families that can start early:**
  - `scx-mesh` (mesh to body; make, tess, mass): now, on MK3 and MK14.
  - `scx-sheetmetal` (develop, fold): once MK4 lands. The pin's develop uses its own rigid maps
    (`fold.rs:75, :412`), so it waits on no deform row; bends are cylinders.
  - `scx-taper` (draft walls, the pin's `taper.rs` and `taper_walls.rs` for line and arc profiles,
    which are exact): once MK5 and BO12 land; offsets of non-planar walls wait on step 5's offset
    kind.
  - `scx-local` (move and contract faces): once BO12 and BO25 land.

## 8. Truth tests per task

### 8.1 The Boolean algebra (BO10)

On every pair (A, B) and triple (A, B, C) of the testkit's Boolean recipes (T12) and convex recipes,
with `≡` meaning: equal volume and area to 1e-9 relative (by `scx_mass`, within the Estimate's
bound), the same T10 classification at 1,000 seeded points outside the band, the contract passing,
and the same count of shells and regions.

| Property | Statement | Notes |
|---|---|---|
| Commutativity | A ∪ B ≡ B ∪ A, A ∩ B ≡ B ∩ A | ids differ (`a`'s are kept); not compared |
| Idempotence | A ∪ A ≡ A, A ∩ A ≡ A, for A and a translated copy A′ = A (two bodies) | the pipeline path, not BO20's shortcut |
| Self difference | A − A′ = ∅ | the pin's `boolean_api.rs:483` |
| Absorption | A ∪ (A ∩ B) ≡ A, A ∩ (A ∪ B) ≡ A | |
| Partition | vol(A − B) + vol(A ∩ B) = vol(A); (A − B) ∪ (A ∩ B) ≡ A | |
| Inclusion-exclusion | vol(A ∪ B) = vol(A) + vol(B) − vol(A ∩ B) | the pin's `tests/boolean.rs:3585` |
| De Morgan within a box X ⊃ A ∪ B | X − (A ∪ B) ≡ (X − A) ∩ (X − B) | |
| Associativity | (A ∪ B) ∪ C ≡ A ∪ (B ∪ C), same for ∩ | |
| Distributivity | A ∩ (B ∪ C) ≡ (A ∩ B) ∪ (A ∩ C) | |
| N-ary order | `boolean_all` over every permutation of 4 operands ≡ one another | BO11, BO12 |
| Rigid motion | T(A ∘ B) ≡ T(A) ∘ T(B) for a rotation and a far translation (1e6) | replaces the pin's origin shift |
| Determinism | every result encoded by `scx-record` is bit-identical over two runs | |

A property is asserted only where every Boolean it uses is built. A pair where one side is built and
another is `Unsupported` is counted and printed, so the suite measures coverage as well.

### 8.2 Per task

| Task | Truths | Pin tests to port |
|---|---|---|
| BO11, BO12 | §8.1 n-ary order; closed-form volumes of unions of k boxes and k cylinders; cells counted against the arrangement of the operands (a closed form for boxes) | `O/boolean.rs:477, :489, :503`; `tests/boolean.rs:2948, :2990, :500` |
| BO13 | the target's volume unchanged; section edges lie on both operands (certified gap); faces split into the closed-form count for a box imprinted by a cylinder | `wr_619_imprint_skips_collapsed_definition` (assertions on outcome only) |
| BO14 | section areas in closed form: plane through a sphere, cylinder, torus, box; the section's boundary on both surfaces | `planar_section.rs` tests that assert curves |
| BO15 | vol(front) + vol(back) = vol(body); each side's classification by the half-space oracle | `tests/boolean.rs:3528`; split tests asserting volumes |
| BO16 | the kept sheet's area in closed form | `tests/trim.rs:371`, `:627` |
| BO17 | 2-D areas by the shoelace and arc segment formula, in exact rationals where the inputs are | `boolean2d` tests asserting areas |
| BO18 | intersection curves of box and cylinder, sphere and sphere, plane and torus in closed form | `intersect_faces_read_only` (9) |
| BO19 | gap and depth brackets contain the closed-form distance and penetration of spheres, boxes | — |
| BO21 | a true fact gives the same result as no fact, in less work; a false fact is `Refused(FalseFact)` | — |
| BO22 | tangent sphere-in-cylinder, cylinder-on-plane: closed-form volumes; the contact curve on both | `tests/boolean.rs` tangent cases, `curved_sliver_coincidence` |
| BO23 | the 73 §9 NURBS truths (SM0) as Boolean volumes | BO5's tests turn into results (SM9) |
| BO25 | a section sheet's area; laminas counted against the closed form | — |
| MK4 | Pappus: volume 2π·x̄·A and area 2π·x̄·L per profile; a full sphere, torus and frustum; poles valid by the contract | ops table :298, :302; `wr_590` (as a refusal plus the split recipe); `api_brep.rs:1735`; `issue_2905:158` |
| MK5 | area × length on straight runs, Pappus on arcs, the mitre's exact conic | `sweep_cornered.rs:112, :178, :363`; ops table :307, :320, :343 |
| MK6 | Pappus for a square thread, the wall exactly on the screw surface | `screw_sweep.rs:271`; ops table :474, :492 |
| MK7a | torsion of a helix frame, identity on lines, rotation on arcs, to 1e-12 | `sweep_stations.rs:74, :505` |
| MK9 | the optimum against a brute-force search on small sections; pairing invariant under rotation of the start vertex | `fork_ea59e9c_section_orders` (23) as outcomes |
| MK10 | the cylinder through stacked circles; the frustum on a straight rail; the certificate's bound above an exact sample on every span | ops `sweep.rs:1359, :1334, :1398`; `loft_rails.rs:147`; `certificate.rs:1658, :1678` |
| MK11 | the bilinear patch of four lines; a sphere octant from three arcs; a flat pentagon | `gordon.rs:866`; `O/surface.rs:543, :668`; `n_sided.rs:276` |
| MK12 | two G0 sections give the ruled surface; quadratic sections give the quadratic | `section_blend.rs:47, :132` |
| MK13 | hollow cone volume in closed form | `local_operation.rs` radial tests asserting volume |
| MK14, MK15 | area and volume per region; iso edges on the surface exactly | `prism/tests.rs` (73) where they assert quantities; `O/../tests/prism.rs` (35) likewise |
| DF1, DF3 | the bent block's volume ∫ (u/d) dV in closed form; bend ∘ unbend = identity on points to a stated bound; plane cases exact by construction | `bend:1567, :1580`; `twist:1078, :1089`; `unbend:1488, :1500`; `p1:2660, :2918, :3053`; `bend:1775` |
| RP1-RP4 | the check passes; no accuracy grew; `|ΔV| ≤ area × largest bound`; counts in closed form | all 18 of `O/../tests/heal.rs` except `:1402` (Q7) |

## 9. Open questions

**Q1. Step 5 has no rows for the fitter, the procedural kinds, surface construction, the volume
maps or the ray against NURBS. Who writes them?**

- Recommend: the coordinator asks for one step-5 widening note (like 73 for the SSI) that gives
  these five their rows before step 7's waiting tasks are briefed. Step 7's tasks cite them by
  name until then. Fourteen of this note's 45 rows can start now and 15 more wait only on other step-6 or step-7 rows.
- Against: a planning note for five large pieces delays them by a review cycle. Mitigation: the
  analytic step-7 rows (§7.1, "can start now") keep builders busy for that cycle.

**Q2. Does `scx-boolean` depend on `scx-stitch`?**

- Recommend: no. layers.txt already orders boolean below stitch, and the slice builds without it
  (BO3: known topology, no stitch). Every step-7 entry builds its topology from the imprint, and the
  pin's uses of stitch inside the Boolean (`boolean_2d`'s `StitchFaces`, the planar path) are
  replaced by known topology (BO17, MK5). LY2 removes the edge from 01's table.
- Against: some later entry (trimming sheets that only nearly meet) may want to sew. Mitigation:
  such a case is two operations at the caller's level (stitch, then Boolean), and the Boolean
  stays lower.

**Q3. A revolve profile that crosses the axis: refuse, or split it as the pin does (WR-590)?**

- Recommend: refuse with `Refused(CrossesAxis { at })`. Split, the input asks for the union of two
  revolved halves, which is a different solid from the one the profile describes (the pin's test
  shows three faces and the Pappus volume of the split).
- Against: users draw profiles across the axis to mean "the half on one side", and the pin's
  behaviour is what SolidCraft users know. Mitigation: the remedy is data (`ChangeInput` naming the
  crossing), so the app can offer the split; DT4 records the outcome difference as by design.

**Q4. N-ary Booleans: a fold now and the cellular merge later, or only the cellular merge?**

- Recommend: BO11's balanced fold now (small, starts at once, gives DT4 rows), replaced by
  selection over BO12's merge when it lands; the fold's tests stay as the oracle for the merge.
- Against: two implementations of one entry is waste, and a fold's intermediate bodies cost
  memory and work. Mitigation: BO11 is a few dozen lines over `boolean`; BB1 measures fold against
  merge before the fold is removed.

**Q5. Loft pairing: the pin falls back to a banded walk past 4e6 cells, which may miss the optimum.
Keep it?**

- Recommend: no banded fallback. The exact optimum is computed under the operation's budget, and
  running out is `Exhausted`. A change in the answer when the input grows past a cell count breaks
  "truth over pin" and determinism under different caps.
- Against: large sections (hundreds of vertices each) become `Exhausted` where the pin gives a
  good-enough answer. Mitigation: the divide-and-conquer search is O(n² log n) per section pair;
  4e6 cells is 2,000 by 2,000 vertices, far past CAD profiles; BB1 measures the worst corpus case.

**Q6. Is `scx-deform` in step 7 at all?**

- Recommend: yes, but narrow: DF1 to DF3 on the bend, twist and unbend maps only, and no B-spline
  volume (a stub in the pin, with no caller). The body operation is new capability, the pin's app
  has none, so it is last in priority among step-7 rows.
- Against: a crate with no caller is speculative work, and 01's "done when" does not name deform.
  Mitigation: DF1 is small once the maps exist, and sheet metal may want bend-of-body later; if
  step 5's volume maps slip, DF moves to step 8 without blocking anything.

**Q7. Port the SMLib healer?**

- Recommend: no. It widens tolerances by model-size buckets and gap gains (R3), samples gaps
  (R10), and is reached only from untrusted record conversion. Its useful parts are queries that
  SU6 and `scx-check` already do, and its seam fixes are `finish`'s and stitch's jobs.
- Against: some imported records may need its fixes to become valid, and the converter
  (`scx-import-sc`) may then refuse designs the pin opens. Mitigation: the converter's corpus run
  counts such refusals; each becomes a typed repair item (RP rows) if real.

**Q8. Repair keeps the pin's run-time "check before and after, keep if better" gate?**

- Recommend: no. Every edit is certified, so validity follows by construction; the tests assert
  the full check on every result. A run-time check costs two full checks per repair and hides bugs
  by rolling back.
- Against: a certified local edit can still break a global property (two squeezed clusters making
  faces cross). Mitigation: RP4 includes the adjacent-cluster recipes, and an interaction found
  there becomes a typed cause checked before the edit, not a rollback.

**Q9. A Coons or n-sided patch that may fold: the pin samples a 5×5 grid. What does scx do?**

- Recommend: certify the normal's sign over the patch by the Bernstein form of the cross product's
  component along a reference direction (exact on polynomial patches). A patch that fails is
  `Refused(Folds)` with the uv where it fails; one the budget cannot decide is `Unproven`.
- Against: the certificate costs a degree-(2p−1)·(2q−1) Bernstein form per patch. Mitigation: it is
  computed once per construction, and it replaces a grid that can miss folds.

**Q10. The revolve's analytic candidates: is a cone whose height is below the pin's MIN_HEIGHT
(#17085) refused?**

- Recommend: no fixed floor. A cone wall whose ends are within the coincidence of each other is a
  flat annulus by construction (the profile segment is perpendicular to the axis within
  coincidence, which `scx-planar` snaps), so it is a plane. Any longer segment is a valid cone.
- Against: a near-flat cone is ill-conditioned for later SSI. Mitigation: SM2 and SM4 handle it by
  certified bounds; a conditioning warning is recorded, never a refusal (11-26: never refuse valid
  input).

## 10. Against this plan

- **The plan's analytic gate is narrow.** Most SolidCraft designs that need step 7 have NURBS faces
  (lofts, fillets from step 8's blends, imported splines), and BO23, MK10 and MK11 wait on step 5.
  Against that: the analytic rows build the pipeline that BO23 widens, and DT4 rows measure each
  widening. The step's "done when" is met on analytic recipes first, as step 4's was.
- **BO10 may find a long tail.** The algebra suite will pass on few pairs at first (coincident faces
  everywhere in A ∪ A′). That is its purpose. If it delays BO12 badly, BO12 starts on the subset
  that passes and BO10 failures become issues.
- **The cellular merge is a big rewrite of a working Boolean.** BO12 changes the core of the slice.
  Against that: the slice already classifies by connected piece sets with membership, so BO12 is a
  generalisation of stage 6's labels from two operands to n, and the slice's tests are its first
  oracle.
- **Refusing what the pin builds** (Q3, Q5, Q10) moves difftest outcomes. Each is recorded in DT4 as
  a by-design difference with its reason, never as a pin defect.
- **No sampling anywhere is ambitious.** R10 replaces every sampled acceptance with a proof. Where a
  proof is not yet available (G1 on n-sided patches, which the pin also does not certify), the
  answer is `Unproven` with its bound, which callers must handle.
- **Make in one crate is large.** Revolve, sweeps, lofts and patches together are about 60,000 pin
  lines. Mitigation: modules per construction, and the geometry (Coons, Gordon, n-sided, the volume
  maps) lives in `scx-surface`, so `scx-make` holds topology only.

## 11. Proposed 04 rows (for the coordinator's ruling and Brett's review)

| date | status | decision | why/against | ref |
|---|---|---|---|---|
| 11-28 | proposed | **Step 7 plan.** `scx-boolean`'s entries (`boolean`, `boolean_all`, `merge`/`select`, `imprint`, `split`, `section`, `trim`, `planar`, `intersect`, `interference`, `boolean_with`) are one pipeline (R1), take `&Brep` and return `Done<C>` (`interference` returns a decided `Interference`); no retry, operand swap or rebuild (R2); no tolerance ladder (R3). Tasks: 75 §7 | The pin's modes and retries are the hazards; one pipeline means one set of proofs. Against: entries with different needs (2-D Boolean) may be faster alone; BB1 measures | 75 §1.2, §2.6 |
| 11-28 | proposed | **Make builds exactly.** Constructors build the analytic kind directly where one exists (R4); otherwise the step-5 procedural kind; NURBS only where the operation is a fit, with a certificate. Choice by a predicate on the input before building (R5). `Made<R>` with typed face records replaces out-params (R7); no `cap: bool` or integer cap modes; no stitch in constructors | Removes build-then-recognise sampling and its fallbacks. Against: more code paths than one general sweep; each is small and closed-form tested | 75 §3.6, §4.6 |
| 11-28 | proposed | **Revolve refuses a profile crossing the axis** (`Refused(CrossesAxis)`, remedy `ChangeInput`); segments on the axis become pole vertices; no MIN_HEIGHT floor | The split the pin does builds another solid. Against: users expect the split; the app can offer it from the remedy | 75 §3.6, Q3, Q10 |
| 11-28 | proposed | **Loft pairing is exact under the budget**, with no banded fallback; certificates have a fixed per-operation budget and record `Raise` on an unproved span, never `RailPastStationCap` | Determinism and truth over pin. Against: very large sections may exhaust; BB1 measures | 75 §4.6, Q5 |
| 11-28 | proposed | **Repair ports only the certified healer** (`tolerance_defects`), with items per defect and `Skip` mode; no run-time check-compare gate; the SMLib healer is not ported; no tolerance grows | The SMLib healer widens tolerances; certified edits make the gate redundant. Against: imported records may need SMLib fixes; the converter's corpus run counts them | 75 §6.5, Q7, Q8 |
| 11-28 | proposed | **Deform is narrow in step 7**: bend, twist and unbend maps (geometry in `scx-surface`), `scx_deform::deform` with exact images decided by kind and frame and the procedural in-volume kind otherwise; no B-spline volume | The pin has no body deform and a stub B-spline volume. Against: no caller; it is last in priority and may move to step 8 | 75 §5.5, Q6 |
| 11-28 | proposed | **Layers (LY2).** `scx-deform` after `scx-hlr`; `scx-repair` after `scx-make`; no `scx-boolean` → `scx-stitch` edge (01 corrected); new edges: `scx-make` → `scx-ssi`, `scx-hlr`, `scx-boolean`; `scx-repair` → `scx-check`; `scx-deform` → `scx-surface` | Matches layers.txt as it stands and the built slice. Against: a later Boolean entry may want to sew; it composes at the caller | 75 §7, Q2 |
| 11-28 | proposed | **The Boolean algebra suite (BO10) is the step-7 gate's property half**: the §8.1 properties on every built pair and triple, judged by volume, area, the T10 oracle, the contract and run-twice bits | The pin has one commutativity check; properties catch what recipes miss. Against: it will fail widely at first; failures are issues, not blockers to BO12 | 75 §8.1 |

## 12. What this note did not do

- It did not build or run the pin, measure any work count, or run any scx code.
- It did not read copyleft kernels; every algorithm named is from the pin (Apache-2.0 here) or a
  published source (The NURBS Book, Farouki-Rajan, Higham).
- It did not plan step 5's fitter, procedural kinds, surface construction, volume maps or ray
  against NURBS (Q1), nor step 8's families beyond naming their early starts.
- It did not check the testkit recipes' coverage of step-7 cases; DT4 and BO10 will.
