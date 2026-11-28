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
  `PC/` is `K/primitive/primitive_creation/`. `A/` is the app's
  `crates/kernel/src/` in the pin's repository (the SolidCraft kernel crate that calls ops). Line
  numbers are at the pin commit of 63.
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
| Boolean (beyond the slice) | `B/` (33,434), `PT/` (20,024), `K/topology/brep/{cutting, planar_section}.rs` (2,028), `O/{boolean, split, trim}.rs` (2,639) | ~58,100 | ~240 (arena_pair 55, TI p1 30, imprint_ends 23) | ~250 | `O/../tests/boolean.rs` 78, `boolean_api.rs` 29, trim 17, split 19, kernel_cases 269 entries |
| Make: revolve, sweeps, prisms | `K/sweep/` (26,303 with loft), `PC/` (17,390; all of `K/primitive/` is 20,700), `K/prism/{extrude, sheet}.rs` (662 of the module's 19,613, of which 12,142 are in non-test files), `O/{sweep, prism}.rs` and helpers | ~44,400 | ~140 plus 28 `ignored` | ~280 | ops sweep 37 plus a 26-case closed-form table, prism 35, kernel sweep suite ~140 |
| Make: loft, skins, patches | `loft_sections`, `loft_rails`, `warp`, `skin_*`, `section_blend`, `K/nurbs/{certificate, gordon, n_sided}.rs`, `sheet.rs`, `O/{surface, blend_surface, network}.rs` | ~17,400 | ~60 | ~120 | loft_rails 13, section_blend 9, two rails 4, skin 11, section orders 23 |
| Deform | `K/surface/{b_spline_volume*, bend, twist, unbend, srf_in_volume}`, `K/surface/volume/`, `K/curve/crv_in_volume.rs` | 15,351 | ~100 | 85 | none |
| Repair | `K/heal/` (23,663), `O/heal.rs` (329); in the app `A/{repair, heal, fill, delete_face, tolerance_heal}.rs` (708, 207, 1,675, 1,283, 372) | 28,237 | ~70 | 66 | ops heal 18 |

## 1. Shapes common to step 7, and the rulings

### 1.1 What the pin does on every path (READ)

- **Retry on failure.** One ops entry retries: `union_copies` (`O/boolean.rs:183-226`) retries a
  union that fails in the radial sector search with the operands swapped under
  `Contract::Proved`, then with `rebuild_native` operands. `boolean` and `boolean_with`
  (:151, :160) do not retry, but every app union goes through `union_copies` (the app's
  `crates/smlib/src/core/boolean.rs:51`), so every union the app makes can take the retry. The
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
  `thread_local` (`A/tolerance_heal.rs:31-37`, outside the kernel), switched by the global
  `set_heals_tolerance_defects` (re-exported at `A/lib.rs:623`; the review cited :621, which is
  the `heal_tolerance_defects` re-export two lines above).
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
- **R2. No retry, no operand swap, no rebuild.** A failure is a typed cause the caller sees. The
  pin's one retry (`union_copies`, §1.1) exists because its radial sector search at a shared edge
  depends on operand order; scx's ordering of faces about a shared edge is a function of the
  geometry alone (BO12's proof covers it), so the union is right on its first run or
  `Unsupported` with the cause named. The result must not depend on operand order beyond the
  stated rule (`a`'s ids are kept); BO10 tests it, including the #20408 collars in both orders.
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
| `boolean_with(.., Contract)` (`O/boolean.rs:160`; `Contract` at `O/body.rs:645`) | yes | no mode: `boolean` always gives what `Contract::Proved` gives (§2.6) | BO10b |
| empty-result refutation (`Witnesses::refute`, `O/boolean.rs:281`; `Error::EmptyRefuted`) | yes | a postcondition of every `boolean` with an empty result (§2.6) | BO10b |
| `boolean_with_options(Imprint)` (:785) | `O/boolean.rs:400` (returns `(Body, bool)`), `trim.rs:311` | `scx_boolean::imprint`; the bool is whether `Done.changes` is empty | BO13 |
| `split_face_by_curves` (`O/trim.rs:237`): curves on one face, the regions returned for picking | yes | `scx_boolean::imprint_curves` | BO13b |
| `trim_by_dropped_curves` (`O/trim.rs:438`): curves dropped onto faces, regions kept by points | yes | `scx_surface` drop (SF1), then `imprint_curves` and `keep` by points | BO16b |
| `boolean_with_options(Slice)` | `O/split.rs:368` | `scx_boolean::split` | BO15 |
| `split_faces` (`O/split.rs:282`): faces cut by several tools in turn, `Error::ToolStopsShort` | yes | `scx_boolean::split_faces` (one pass over all tools) | BO15b |
| `split_body_by_sheet` (`O/split.rs:506`): sides named by one sheet face's normal | yes | `split` with `Tool::Sheet { sheet, facing }` | BO15 |
| `boolean_with_curves` (:804) | `trim.rs:311` | `Done` of `imprint` carries the section edges in `changes` | BO13 |
| `union_all` (`O/boolean.rs:310`) | yes | `scx_boolean::boolean_all` | BO11, then BO12 |
| `merge_breps`, `boolean_lists`, `boolean_trees`, `non_manifold_boolean` | no | `scx_boolean::merge` (cellular) | BO12 |
| `piecewise_merge` (kernel sweep and offset only) | no | `merge` plus `select` | BO12; callers in MK8 and step 8 |
| `boolean_merge` (stitch, measure in ops) | yes | `merge` plus `select(Union)`; measure's uses move to bodyq | BO12 |
| `boolean_2d` (:902; `O/prism.rs:82`) | yes | `scx_boolean::planar` | BO17 |
| `split_body` (`O/split.rs:423`, two Booleans on copies) | yes | `split` returns both sides from one pass | BO15 |
| `trim_sheet` (`O/trim.rs:276`): regions kept by the points nearest them | yes | `scx_boolean::trim` with `keep: &[Point3]` | BO16 |
| `remove_faces`, `remove_feature_with_crossing_patches`, `remove_round_junction`, `remove_ring_groove` (`O/trim.rs:767, :656, :670, :821`) | yes | `scx-local` (step 8, defeature) | step 8 |
| `project_curves`, `face_section`, `drop_curves` (`O/sweep.rs:2039, :2097, :2167`) | yes | projection and drop in `scx-surface` (SF1); `face_section` is `section` scoped to a face | SF1, BO14 |
| `PlaneCutter`, `cut`, `do_cut` (`K/topology/brep/cutting.rs`) | no | `section` by plane | BO14 |
| `create_planar_section_curves`, `face_plane_section_curves` (`planar_section.rs`) | sweep only | `section` returning curves | BO14 |
| face/face and body/body intersection | no ops API | `scx_boolean::intersect` | BO18 |
| interference | none in the pin | `scx_boolean::interference` | BO19 |
| same-body refusal (`ptr::eq`, `split.rs:510`) | yes | the same-body shortcut | BO20 |
| caller-stated incidences | none in the pin | `boolean_given` and `Incidences` | BO21 |
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
| `screw_sweep*`, with the radial law (`O/sweep.rs:1824-1871`) and joined walls (`Walls::Joined`, `K/sweep/screw.rs:53`, the app's `join_spline_walls`) | `O/sweep.rs:1831, :1903` | `scx_make::screw` with `Screw { law, walls }` | MK6 |
| `SweepControl` twist, end scale, one or two rails (`A/sweep_path.rs:46-68`) | app | `scx_make::sweep`'s `PathFrame` | MK7b |
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
| `ruled` (`O/surface.rs:340`), `reverse_sheet` (:366) | yes | `scx_make::ruled`; `reverse` as a Draft method | MK17 |
| `section_blend`, `section_blend_surface` | `railed.rs`, `blend_surface.rs:168` | `scx_make::blend` | MK12 |
| `blend_edges`, `fillet_sheets`, `fillet_crossing` (`O/blend_surface.rs`) | yes | `scx_make::blend` on SSI | MK12b |
| `ruled_surface_from_offset` (taper only) | no | step 8 taper | — |
| `create_taper_extrude*`, `taper_walls` | through extrude | step 8 taper | — |
| `BendVolume`, `TwistVolume`, `UnbendVolume`, `SrfInVolume`, `CrvInVolume` | none | volume maps in step 5; `scx_deform::deform` | DF1-DF3 |
| `BSplineVolume` (a stub upstream: always `SM_ERR`) | none | not ported until a caller exists | Q6 |
| `heal_short_edges_and_slivers` (`tolerance_defects.rs:1221`) | `O/heal.rs:183` | `scx_repair::heal` | RP1-RP4 |
| Repair Part step 1: rebuild a face SMLib refuses as its analytic surface (`A/repair.rs:11-14`) | app | `scx_repair::rebuild_analytic` | RP7 |
| Repair Part step 2: sew free edges within a gap (`A/repair.rs:15-18`, `stitch_no_worse` at :246) | app | `scx_repair::sew` over `scx-stitch` | RP5 |
| Repair Part step 3: patch free-edge holes, planar or G1 (`A/repair.rs:19-21`, `patch_holes` :582, `A/fill.rs:100` `patch_edges_with`) | app | `scx_repair::patch_holes` over MK11 | RP6 |
| `RepairOptions { gap, patch_holes, heal_first }` (`A/repair.rs:51`) | app | separate entries and an ordered `&[Step]` list (§6.5) | RP8 |
| faces on one surface and edges on one curve merged (`A/heal.rs`) | app, after operations | `scx-brep` Draft methods, called by the Boolean's build stage and by repair | BR11 |
| delete face with heal (`A/delete_face.rs`) | app | `scx-local` (step 8) | step 8 |
| SMLib healer (`heal_data/`, `gap.rs`, `face_props.rs`) | `heal_made` (`K/topology/brep/p10.rs:546`) on untrusted records, from `solidcore-convert/src/brep.rs:317` | not ported: it widens tolerances (R3); counted first (RP0) | RP0, Q7 |

## 2. `scx-boolean`: beyond the analytic slice

### 2.1 What the pin does (READ)

**Size and entries.** `B/` is 33,434 lines: `topology_intersector/` (p1 6,650, p2 3,589, p3 2,142,
curved_strip 2,078, coincident_strip 1,112, duplicate_edge_run 362), `merge/` (p4 3,000 with
kernel_adapter 2,548 and arena_pair 2,448, p2 2,483, p1 2,124, p3 1,950, p5 799) and `api_brep.rs`
1,636. The proto topology it builds on is another 20,024 (`PT/p2.rs` 6,958, `p1/proto.rs` 5,187,
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
5. In `union_copies` only (:183-226): on a radial-sector failure (:231), retry with the operands
   swapped under `Contract::Proved` (:204-215), then with `rebuild_native` operands (:216-226).
   `boolean` and `boolean_with` run once; every app union calls `union_copies`. Its regression
   is `tests/boolean.rs:50-75`, the #20408 collars: two swept parts whose coaxial 1.3 mm collars
   overlap over 2 mm, joined to the sum of the volumes less π·1.3²·2.

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

// Split (BO15): both sides from one pass. `Tool` is a surface (bounded by the body) or a sheet,
// whose sides are named by one of its faces' normals (`split_body_by_sheet`).
pub fn split(body: &Brep, tool: &Tool, cx: &mut Cx<'_>) -> Result<Done<Sides>, BooleanError>;
pub enum Tool { Surface(Surface), Sheet { sheet: Brep, facing: FaceId } }
pub struct Sides { pub front: Vec<RegionId>, pub back: Vec<RegionId> }

// Sections (BO14): the sheet (or wire, for a sheet operand) where a surface meets the body.
pub fn section(body: &Brep, by: &Surface, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// Trim a sheet by a sheet (BO16): the connected regions of `sheet`, cut where `by` meets it, that
// lie nearest the points in `keep` (as `trim_sheet` picks). A point as near two regions is
// Refused(AmbiguousPick { point }); a cut that separates nothing is Refused(DoesNotCut).
pub fn trim(sheet: &Brep, by: &Brep, keep: &[Point3], cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// The 2-D Boolean of two coplanar sheets (BO17).
pub fn planar(a: &Brep, b: &Brep, op: BooleanOp, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// Intersection without a build (BO18): the curves and points where two bodies meet, as a wire body.
pub fn intersect(a: &Brep, b: &Brep, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// Interference (BO19): a decided answer, never a body.
pub fn interference(a: &Brep, b: &Brep, cx: &mut Cx<'_>) -> Result<Interference, BooleanError>;
pub enum Interference { Apart { gap: Bracket }, Touch { at: Vec<Contact> }, Overlap { depth: Bracket } }

// Stated incidences (BO21): facts the caller asserts, each checked; a false one is Refused(FalseFact).
pub fn boolean_given(a: &Brep, b: &Brep, op: BooleanOp, facts: &Incidences, cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;

// Curves lying on one face (BO13b): imprinted, the regions they bound returned for picking.
pub fn imprint_curves(body: &Brep, face: FaceId, curves: &[Curve], cx: &mut Cx<'_>)
    -> Result<Done<Regions>, BooleanError>;

// Several faces cut by several tools in one pass (BO15b).
pub fn split_faces(body: &Brep, faces: &[FaceId], tools: &[Tool], cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;
```

- **Proof is the default, not a mode.** The pin's `boolean_with` takes `Contract::{Closed,
  Proved}` (`O/body.rs:645`): Closed checks topology only; Proved requires each solid operand
  proved valid and facing out, and the result proved likewise. scx has no `Contract` parameter.
  Operands are finished bodies (`scx-brep`'s `finish` holds the topology and the outward sense
  as data), and `boolean` always gives Proved's result: a solid result is a closed manifold that
  `finish` accepts and its outward sense is established from the operands' senses and the
  selection, not assumed. A result that cannot be established is a typed `Defect`, never a
  weaker result. The full geometric check stays outside the operation (04 row 107's result
  contract), and every test runs it.
- **The empty result is refuted (ported postcondition).** When the selection leaves no material,
  `boolean` checks the pin's witness before returning it: every vertex of each operand is
  classified against the other (charged to the budget, a cost linear in vertices), and one that
  shows material must remain (for a subtraction, a vertex of `a` strictly outside `b`; for an
  intersection, one strictly inside both) makes the result `Defect(EmptyRefuted { at })`. The
  pin returns this as an error to the caller (`Error::EmptyRefuted`); in scx it is a defect,
  because it means the pipeline is wrong, not the input.
- **The same-body shortcut** (BO20) is inside `boolean`: when `a` and `b` are the same `Brep`
  (`ptr::eq`, or equal body ids in one document after 04's id rules), union and intersection
  return `a` and subtraction the empty body, with no stages run. Two bodies that are equal but not
  the same are run through the pipeline; BO10's idempotence tests prove the pipeline gets them
  right.
- **Imprint: what exists and what is left.** `scx-boolean/src/imprint.rs` (1,227 lines) is the
  slice's stage 5, crate-private: it writes the interference and face-face results into
  `a.edit()`. BO13 adds the public entry that stops there (finishing the imprinted Draft with no
  selection), the `Imprint` choice (edges only, or edges and the tool's faces lying on the
  target), `Done.changes` as the pin's `(Body, bool)` (the bool is "changes not empty"), and
  tests. That is small, so BO13 runs before BO12 and accepts the rework when BO12 generalises the
  stage beneath it; the entry and its tests are unchanged by that rework.
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
pub struct Screw { /* axis, pitch, turns, law: Option<RadialLaw>, walls: Walls { Pieces, Joined } */ }
pub struct PathFrame { /* orientation, twist, end scale, rails: Vec<Rail>, rail_fit: RailFit */ }
pub fn ruled(a: &Curve, b: &Curve, doc: &Document) -> Result<Made<FaceId>, MakeError>;
pub fn sweep_solid(tool: &Brep, path: &Path, cx: &mut Cx<'_>) -> Result<Done<NoItems>, MakeError>;
pub fn radial(cone: &RadialCone, doc: &Document) -> Result<Made<RadialFaces>, MakeError>;
pub fn prism(regions: &[Region], frame: &Frame, span: Span, doc: &Document)
    -> Result<Made<Vec<ExtrudeFaces>>, MakeError>;
pub fn sheet(regions: &[Region], frame: &Frame, doc: &Document) -> Result<Made<Vec<FaceId>>, MakeError>;
pub fn face(surface: &Surface, bound: &FaceBound, doc: &Document) -> Result<Made<FaceId>, MakeError>;
```

- **Profiles touching the axis.** A profile segment on the axis is not a face; its ends become
  pole vertices (a sphere's or cone's apex) with `OnChart` pcurves (SM1, SM1b). A profile curve
  that touches the axis at an interior point (a circle tangent to the axis, revolved to a horn
  torus) is split there and the touch point becomes a pole vertex shared by the two pieces' faces,
  as the pin does (WR-590, `PC/p3/sweeps.rs:181-194`, `split_curves_on_axis`). Only a curve that
  truly crosses the axis, so the body would pass through itself, is `Refused(CrossesAxis { at })`;
  the pin refuses the same case (`SweepFailure::EdgeCrossesRotationAxis`,
  `K/sweep/rotational_sweep_geometry.rs:364`). WR-590's test is ported as a build test (Q3).
- **The plane snap rule** (Q10). A profile line whose two ends lie at axial heights within the
  document's coincidence of each other revolves to a plane normal to the axis, at the start
  vertex's height; the end vertex lies within coincidence of that plane and its measured distance
  is recorded as the vertex's accuracy. Any other line revolves to a cylinder (both ends at one
  radius within coincidence) or a cone, however low. No height floor applies.
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
upstream patch; no certificate; folds are looked for on a 5×5 normal grid. The ops `boundary`
(`O/surface.rs:251`) takes three or four curves to a Coons patch with no stitch; five or more go
to `n_sided` (:256), whose patches are stitched into one sheet (`sheet_of`, `stitch_faces` at
:326). (The review said the Coons boundary stitches; it is the n-sided path that does.) MK11
builds the n-sided sheet with its shared sides known, so it needs no `scx-stitch`; RP6, which
sews a patch into a body, does. Gordon
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
- `Pairing` carries its status: exact within the fixed per-operation cell limit (the pin's
  MAX_CELLS), else the banded answer, `Proven` when its walk avoids the band's edge and `Unproven`
  with its cost as the bound otherwise (Q5). The limit is a constant, not the remaining budget, so
  the answer is a function of the input.
- Flat walls are planes because the section pair is planar and coplanar with the rail, decided
  before the skin is built (R5), not by replacing a built B-spline.
- `scx_surface::{coons, gordon, n_sided}` return a `NurbsSurface` with its certified deviation, and
  `scx_make::face` bounds it.

## 5. `scx-deform`

### 5.1 What the pin does (READ)

**The app cannot deform a body today.** No kernel function maps a body through a volume and ops has
no deform entry. Evidence (grep for `BendVolume`, `TwistVolume`, `UnbendVolume`, `SrfInVolume`,
`CrvInVolume` followed by a constructor, outside their own files): seven sites, all under
`#[cfg(test)]` (`K/heal/stitch/bounds.rs:1762`, test module from :1548;
`K/curve/crv_on_surf.rs:2863, :2890, :2956`, from :2311; `K/curve/curve/p2.rs:4876`, from :4607;
`K/surface/surface/p5.rs:2725`, from :2661; `K/surface/surface/p2.rs:8414, :8614`, from :8023).
The other files that name the kinds match on them to read or evaluate. The pin has volume geometry kinds that are read from records and evaluated:
`BendVolume` (`bend_volume.rs`, 1,973), `TwistVolume` (1,208), `UnbendVolume` (1,671), `SrfInVolume`
(1,370), `CrvInVolume` (`K/curve/crv_in_volume.rs`, 1,978), the generic `Volume` chain
(`surface/volume/p1.rs` 3,278, `p2.rs` 2,057). `BSplineVolume` is a stub that always returns
`SM_ERR`: upstream builds without `USE_VOLUMES`, and the port follows the build
(`K/surface/b_spline_volume.rs:32-58`; `load_nlib_transform` at :191 always errs). Sheet-metal develop does not use the unbend
volume; it unrolls with its own maps: `Wrap` (`K/develop/fold.rs:60-90`, the neutral-arc wrap)
and `bent_across` (:412, a `Rigid3`), and no file in `K/develop/` names a volume. `api_transform.rs:152-157` refuses
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
   parity, called only by `heal_made` (`K/topology/brep/p10.rs:546`, from
   `solidcore-convert/src/brep.rs:317`) on untrusted records. It widens tolerances by a model-size
   ladder and by `gap_gain * gap`. How many corpus records it actually fixes is not known (RP0).

The app's **Repair Part** (`A/repair.rs`) runs, in order:

0. the certified heal above, when `heal_first` is set;
1. an analytic rebuild: cylinder, cone, sphere and torus faces that SMLib refuses on their NURBS
   form are rebuilt as the analytic surface (`A/repair.rs:11-14`);
2. sewing within the gap: free edges paired within `gap` are glued, through `stitch_no_worse`, with
   each glued edge's tolerance at least twice the gap (`A/repair.rs:17`, the stitch tolerance
   `widest * 1.05` at :244);
3. hole patches, when `patch_holes` is set: free loops filled by `A/fill.rs:100` (a plane face, or
   a patch meeting its neighbours G0 or G1), then sewn again.

The steps are chosen by `RepairOptions { gap, patch_holes, heal_first }` (`A/repair.rs:51`). Beside
it, `A/heal.rs` merges faces on one surface and edges on one curve, and `A/delete_face.rs` deletes
faces and heals the opening (step 8's `scx-local`).

### 6.2 Idiom hazards (READ)

- `tolerance_defects.rs`: 7 `&mut Brep` or `&mut Kernel`, `:380` a failed line becomes "no curve",
  `:447`, `:493`, `:905` errors become an infinite gap; `:548` a literal `1e-12 * size`.
- `O/heal.rs:239-240`: `.ok()` on lookups, so a removed entity skips the growth check.
- The SMLib healer: `ignore_status` 9 times; 18 `let _ =` in `p2.rs`; 42 bool or tri-state fields;
  `HealerOpType` by integer code; `gap.rs` reads through `&mut Brep`.
- The app's `static AtomicBool` and `thread_local`, switched by the global
  `set_heals_tolerance_defects` (`A/lib.rs:623`): whether import and Repair heal depends on a
  process-wide setting, not on the call.
- `RepairOptions`' two bools (`patch_holes`, `heal_first`) choose which steps run; with `gap` they
  make one entry with four behaviours.
- `A/repair.rs:17`, :244: the sewn edge's tolerance is set by a factor (2× the gap, 1.05× the
  widest), not by the gap measured.

### 6.3 Truths and tests (READ)

The 18 tests in `O/../tests/heal.rs` are truths: `heals()` (:481-527) asserts that the full check
passes, exact counts, no tolerance grew, and `|ΔV| ≤ area × largest bound`. Port all of them, in
particular :534, :575, :611, :632, :958, :827, :1062; :1402 tests the SMLib path and is ported
only if Q7 keeps it. The 66 kernel unit tests and the goldens assert the pin's state.

### 6.4 Perf traps (READ)

Two full checks per accepted heal; `collapse_short_edges` restarts the edge scan after each pass
with a linear `find` per edge (:728-750); faces+1 outer rounds.

### 6.5 Proposed shape (JUDGED)

Each repair the app runs is its own entry, so no option bool chooses a step (the pin's
`RepairOptions { gap, patch_holes, heal_first }`, `A/repair.rs:51`). The app's Repair Part is an
ordered list of steps, run in order, each step's `Done` the next step's input and its `Changes`
composed.

```rust
// The certified healer (RP1-RP3): short-edge clusters, slivers, empty pieces.
pub fn heal(body: &Brep, cx: &mut Cx<'_>) -> Result<Done<HealCause>, RepairError>;
// A face on a NURBS form that is exactly an analytic surface goes as that surface (RP7).
pub fn rebuild_analytic(body: &Brep, cx: &mut Cx<'_>) -> Result<Done<RebuildCause>, RepairError>;
// Free edges that pair within `gap` sewn (RP5), through scx-stitch.
pub fn sew(body: &Brep, gap: Length, cx: &mut Cx<'_>) -> Result<Done<SewCause>, RepairError>;
// Loops of free edges no pair explains filled by a patch and sewn in (RP6).
pub fn patch_holes(body: &Brep, cx: &mut Cx<'_>) -> Result<Done<PatchCause>, RepairError>;
// The app's Repair Part (RP8): the steps in the order given.
pub fn repair(body: &Brep, steps: &[Step], cx: &mut Cx<'_>) -> Result<Done<StepCause>, RepairError>;
pub enum Step { Heal, RebuildAnalytic, Sew { gap: Length }, PatchHoles }
```

- Items are the defects found (each short-edge cluster, each sliver, each empty piece, each
  free-edge pair, each hole, each face to rebuild) in id order, so the default reports the lowest
  failing one and `Skip` mode does the rest (01 §5.2). This replaces "reported and never retried"
  and the pin's "a heal that cannot run is skipped and the steps below go on" (`A/repair.rs:9-10`),
  which becomes `Skip` mode stated by the caller.
- **heal.** Every edit is certified locally (step 3's enclosure, the GAP_SHARE rule on SU6's
  bounds), so the result is valid by construction. The run-time check-compare of the ops wrapper is
  not ported; the tests assert it instead (Q8). No accuracy grows: a cluster that would need a
  larger accuracy is a `HealCause`. Lines and planes exactly; B-spline end control points moved; an
  arc re-ended stays an arc if both ends stay on its circle, which the enclosure decides.
- **sew.** Each pair of free edges whose certified gap is within `gap` becomes one edge, keeping one
  of the pair's curves, as the pin does. The kept edge's accuracy is the certified gap bound
  itself, recorded in `report.raised` with the pair: an accuracy the edge measurably has, which 01
  requires every writer to record before `finish`. The pin's factor (`SmBrep::GlueEdgesGeneral`
  gives at least twice the gap, `A/repair.rs:17`, and the stitch tolerance `widest * 1.05`, :244)
  is not ported (R3). The body's resolution and every other edge stay as they were. A pair beyond
  `gap` is a `SewCause::TooFar { gap: Bracket }` with a `Resize` remedy.
- **patch_holes.** A loop of free edges is filled: by a plane face when the loop lies on one plane
  within coincidence and its neighbours are that plane (exact, G0); otherwise by MK11's n-sided or
  Coons patch meeting the neighbours G1. G0 is certified; a G1 join that the certificate cannot
  prove is recorded `Unproven` with its bound (R10), the patch kept, as 01's Setback hole fill does.
  Extending the neighbours is not done (as in the pin). The patch is sewn in by `sew`'s rule.
- **rebuild_analytic.** The pin rebuilds a cylinder, cone, sphere or torus face that SMLib refuses
  on its NURBS form (`A/repair.rs:11-14`). scx accepts NURBS faces, so the reason is different:
  exact analytic faces make every later SSI and classify closed-form. A face is rebuilt only when a
  certificate proves its NURBS form is the analytic surface (MK10's `on_circle` and
  `turn_about_axis`, the same Bernstein bounds); a face the certificate refutes or cannot decide is
  left as it is and listed, never rebuilt by sampling.
- **Face and edge merge** (`A/heal.rs`: faces on one surface meeting along an edge become one; two
  edges on one curve meeting at a vertex nothing else uses become one) goes in `scx-brep` as Draft
  methods (BR11), not in the Boolean's build stage. Reason: the Boolean's build calls it at the end
  (SMLib's Booleans do the same), repair calls it, and step 8's local operations will; the lowest
  crate all three use is `scx-brep`, and the edit is pure topology over a test of equal surfaces
  and curves that `scx-brep` already defines. Faces split on purpose (the pin's split lines) are
  kept apart by a typed set of protected edges passed in, not a flag on the body.

## 7. Task table

Model: Opus where a correctness argument is the work (a proof of completeness, a certificate, an
order-independence argument); Sonnet where the work is porting tests, mapping or plumbing. "Edges"
names each new Cargo dependency; each must be legal under layers.txt after LY2.

| Id | Crate | Work | Needs | Edges | Model |
|---|---|---|---|---|---|
| LY2 | layers | layers.txt: `scx-deform` after `scx-hlr`, `scx-repair` after `scx-make`. 01's table is amended in this branch (rows for boolean, deform, make, repair; §7.2). New crates take test bodies from testkit | — | — | Sonnet |
| BO10 | boolean (tests) | The algebra suite on the testkit recipes (§8.1), including the coaxial shared-edge collar recipes in both operand orders and the #20408 collars (`tests/boolean.rs:50-75`) as a mandatory truth; judged by volume, area, the T10 oracle and the contract; run twice and compared by bits | — | — | Sonnet |
| BO10b | boolean | Fix what BO10 finds on the slice, before BO12 builds on it; with it, proof as the default (no `Contract`) and the empty-result refutation as a postcondition (§2.6) | BO10 | — | Opus |
| BO11 | boolean | `boolean_all`: n-ary union, subtraction and intersection as a balanced fold in operand order, work summed in order; port `union_all`'s closed-form tests | BO10b | — | Sonnet |
| BO12 | boolean | `merge` and `select`: the cellular merge over n operands (stages 1-6 generalised, membership per cell, two-sided faces and laminas by 04 row 16); `boolean` and `boolean_all` become selections; proof that a cell's membership, and the radial order of faces about a shared edge, depend on the geometry alone and not on operand order | BO10b, BO25 | — | Opus |
| BO13 | boolean | `imprint`: the public entry over the existing stage 5 (`imprint.rs`), finishing with no selection; the `Imprint` choice; `Done.changes` for the pin's `(Body, bool)`. Runs before BO12, rework accepted (§2.6) | — | — | Sonnet |
| BO13b | boolean | `imprint_curves`: curves lying on one face imprinted, the regions they bound returned (`split_face_by_curves`); curves with exact pcurves now, others after the fitter | BO13 | — | Sonnet |
| BO14 | boolean | `section` by a surface: sheet result for a solid, wire for a sheet; scoped to one face for `face_section`; plane against quadrics now, NURBS after SM9 | BO25 | — | Opus |
| BO15 | boolean | `split` by a surface or a sheet (sides named by a sheet face's normal): both sides from one pass; port `O/split.rs` tests that assert areas and volumes | BO12 | — | Opus |
| BO15b | boolean | `split_faces`: several faces by several tools in one pass; `Refused(ToolStopsShort { tool, face })` when nothing splits and a tool ends inside a face | BO15 | — | Sonnet |
| BO16 | boolean | `trim` a sheet by a sheet, keeping the regions nearest given points; `AmbiguousPick`, `DoesNotCut`; port `tests/trim.rs:371`, `:627` | BO12 | — | Opus |
| BO16b | boolean | `trim_by_dropped_curves`: curves dropped onto faces (SF1), imprinted (BO13b), regions kept by points | BO13b, BO16, SF1 | boolean → surface (exists) | Sonnet |
| BO17 | boolean | `planar`: the 2-D Boolean of coplanar sheets on `scx-planar` regions, faces built with exact pcurves | — | — | Sonnet |
| BO18 | boolean | `intersect`: the curves and points two bodies meet in, as a wire body (shells listing wire edges exist in `scx-brep`, `finish.rs:100, :182`), from stages 1-4 | — | — | Sonnet |
| BO19 | boolean | `interference`: apart (with a certified gap), touching (contacts), or overlapping (a certified depth); stages 1-2 with BW3's bounds | BW3 | boolean → bodyq (exists) | Opus |
| BO20 | boolean | The same-body shortcut; tests that equal but distinct bodies give the same results through the pipeline | BO10b | — | Sonnet |
| BO21 | boolean | `boolean_given` and `Incidences`: stated face-on-face and edge-on-face facts, each checked (`Refused(FalseFact)`), used to skip their SSI | — | — | Opus |
| BO22 | boolean | Tangent contact: stated facts, then closed form (SM2), then tracing (SM6b); lifts the BO7 limits on tangent pairs | BO21; tracing waits on SM6b | — | Opus |
| BO23 | boolean | NURBS faces: stage 4 on `General` (SM9), stage 5 pcurves by the fitter, stage 6 classify by step 5's ray against NURBS | SM9, the fitter, the NURBS ray | — | Opus |
| BO24 | boolean | Lift `Unsupported(Pcurve)` on analytic faces whose cut curve has no closed-form pcurve (a quartic on a torus) by the fitter | the fitter | — | Opus |
| BO25 | boolean | Non-manifold results: sheets, laminas, mixed regions in the build stage; `Unsupported(NonManifold)` kept only for what scx-brep cannot hold | — | — | Opus |
| BR11 | brep | Draft methods merging faces on one surface across an edge and edges on one curve at a free vertex, with a protected-edge set; called by the Boolean's build stage | — | — | Sonnet |
| SF1 | surface | Curves projected along a direction and dropped along the normal onto a surface (`project_curves`, `drop_curves`): closed forms now (a line or circle onto a plane, a line onto a cylinder or cone along its axis), the fitter for the rest. Home: `scx-surface`, because the map is curve against surface with no view and no topology; the restriction to a face's trim is `scx-bodyq`'s existing curve-onto-body projection (01 row for bodyq). Not `scx-hlr`, whose projections are view-dependent visibility | general curves wait on the fitter | — | Opus |
| DT4 | difftest | The step-7 outcome difftest: t_slice widened to every step-7 entry and constructor, each side judged against truth, pin defects in `pin_defects.rs`, work rows in `slice_work.txt` | BO11, MK4 (first rows); grows with each task | — | Sonnet |
| BB1 | bench | The perf rows of §8.3 before and after each step-7 task: work counts against the pin with side-channel instrumentation, no committed counters | DT4 | — | Sonnet |
| MK4 | make | `revolve` of line and arc profiles, full and partial: plane, cylinder, cone, sphere, torus built directly with exact and `OnChart` pcurves; segments on the axis and interior axis touches (WR-590, horn torus) become pole vertices; the plane snap rule; `Refused(CrossesAxis)` only for a true crossing; `revolve_sheet`; WR-590 ported as a build test | SM1b for poles (built) | — | Opus |
| MK4b | make | `revolve` of other curves onto the procedural revolution kind | step 5 procedural kinds | — | Sonnet |
| MK5 | make | `sweep` of a line-and-arc profile along a line-and-arc path: walls by kind, closed-form mitres, one Draft, no stitch; the planar path is this case | MK4 | make → ssi (mitre conics) | Opus |
| MK6 | make | `screw` on the procedural sweep kind: the radial law (`O/sweep.rs:1824-1871`), `Walls::{Pieces, Joined}` (the app's `join_spline_walls`), an exact cylinder for a profile line along the axis | step 5 procedural kinds | — | Opus |
| MK7a | make | `frames`: rotation-minimising frames in closed form on lines, arcs and helices, least rotation at corners; port the stations tests | — | — | Sonnet |
| MK7b | make | `sweep` along a general path and the controlled sweep on the procedural sweep kind, with `PathFrame`'s twist, end scale and one or two rails with `RailFit` (`A/sweep_path.rs:34-68`) | MK7a, step 5 procedural kinds | — | Opus |
| MK8 | make | `sweep_solid`: the whole-body sweep from HL5's silhouettes and the cellular merge | HL5, BO12, MK7b | make → hlr, make → boolean | Opus |
| MK9 | make | `pair`: section pairing by the cyclic least-cost alignment, exact within a fixed cell limit, else the banded answer marked `Unproven` (Q5) | — | — | Opus |
| MK10 | make | `loft` with the certificate: skinning through the step-5 constructor, `span_bound` ported, local re-skin per added station, `Raise` on an unproved span | MK9, step 5 surface construction | — | Opus |
| MK10b | make | Rails: one and two rails, the warp for three or more, the two-rail sweep; certified by MK10 | MK10 | — | Opus |
| MK11 | surface, make | Coons (four exact boundaries, compatibility, certified deviation), Gordon, n-sided (its sheet built with shared sides known: no stitch); `face` bounds them; G1 joins certified or `Unproven` | knot insertion and degree elevation in `scx-nurbs-surface`; Gordon also MK10's constructor | — | Opus |
| MK12 | make | `section_blend` with Hermite ends G0-G3 | step 5 surface construction | — | Sonnet |
| MK12b | make | Sheets between sheets (`blend_edges`, `fillet_sheets`, `fillet_crossing`) | MK12, BO14, SM9 for NURBS sheets | — | Opus |
| MK13 | make | `radial`: the hollow cone built directly (outer, inner, annular caps) with no stitch or subtract | — | — | Sonnet |
| MK14 | make | `prism` and `sheet` over several planar regions; port `O/prism.rs` and `prism/tests.rs` truths | — | — | Sonnet |
| MK15 | make | `face` on an analytic surface by a uv box or a planar region in uv; iso-line edges exact (lines and circles) | — | — | Sonnet |
| MK15b | make, nurbs-surface | `face` on a NURBS surface: iso-curve extraction in `scx-nurbs-surface` (exact, by knot insertion to full multiplicity; not built yet); bounds other than iso lines wait on step 5's curve-on-surface kind | MK15; knot insertion | — | Sonnet |
| MK16 | make (tests) | The closed-form make table: the ops 26-case table, Pappus, cornered and stations truths, against T9 | MK4, MK5 | — | Sonnet |
| MK17 | make | `ruled` between two curves (lines and coplanar arcs exact by kind; others a degree-1 NURBS after compatibility) and `reverse` of a sheet as a Draft method (`O/surface.rs:340, :366`) | knot insertion and degree elevation (for NURBS curves) | — | Sonnet |
| DF1 | deform | Crate; `deform` with the exact image rules of §5.5 on analytic faces; `Refused(MeetsAxis)` | step 5 volume maps (bend, twist and unbend only) | deform → surface, brep | Opus |
| DF2 | deform | Faces with no exact image onto the in-volume procedural kind | DF1, step 5 procedural kinds | — | Sonnet |
| DF3 | deform (tests) | Jacobian volume closed forms, bend ∘ unbend = identity, derivatives against finite differences in exact arithmetic, the pin's map tests | DF1 | — | Sonnet |
| RP0 | difftest | Count, over the corpus's untrusted records, what the SMLib healer (`heal_made`, `K/topology/brep/p10.rs:546`, from `solidcore-convert/src/brep.rs:317`) changes: records it alters, by defect class, and how many then pass the check that failed before. Its result decides Q7 finally | — | — | Sonnet |
| RP1 | repair | Crate; `heal` with items per defect; short-edge clusters and squeezes on lines and planes exactly | SU6 (built) | repair → surface, brep, check (findings) | Opus |
| RP2 | repair | B-spline ends: interval enclosures (`scx-nurbs`'s `eval_over`, built), control-point moves, GAP_SHARE certified on SU6; zero-length piece cutting | RP1 | — | Opus |
| RP3 | repair | Sliver collapse by certified coincidence | RP1 | — | Opus |
| RP4 | repair (tests) | Port the 18 ops heal tests as truths; one-defect recipes at δ = 10·acc, acc, acc/10 | RP1-RP3 | — | Sonnet |
| RP5 | repair | `sew` within a gap over `scx-stitch`: the kept edge's accuracy is the certified gap, recorded; `TooFar` past it | RP1 | repair → stitch | Opus |
| RP6 | repair | `patch_holes`: plane faces exactly; otherwise MK11's patch meeting the neighbours G1, `Unproven` where uncertified; sewn in by RP5 | RP5, MK11 | repair → make | Opus |
| RP7 | repair | `rebuild_analytic`: a NURBS face proved to be a cylinder, cone, sphere or torus goes as that surface; refuted or undecided faces listed and kept | RP1, MK10's certificate module (portable before the loft) | — | Opus |
| RP8 | repair | `repair(&[Step])`: the steps in order, `Changes` composed, `Skip` mode across steps; the app's Repair Part as a step list | RP1, RP5 (RP6, RP7 as they land) | — | Sonnet |

### 7.1 Graph and critical path

```
LY2 (first: before DF1 and RP1 create crates)
BO10 ──> BO10b ──┬─> BO11 ──> DT4 ──> BB1
                 ├─> BO20
                 └─> BO12 (with BO25) ──┬─> BO15 ──> BO15b
                                        ├─> BO16 ──┐
                                        └─> MK8 (HL5, MK7b)
BO13 ──> BO13b ──────────────────────────────────┴─> BO16b <── SF1
BO25 ──> BO14 ──> MK12b
BO17, BO18, BO21, BR11 alone;  BO21 ──> BO22 (tracing: SM6b);  BW3 ──> BO19
BO23 (SM9, fitter, NURBS ray);  BO24 (fitter)
MK4 ──┬─> MK5 ──> MK16
      └─> MK4b (procedural kinds)
MK7a ──> MK7b (procedural kinds) ──> MK8;  MK6 (procedural kinds)
MK9 ──> MK10 (surface construction) ──> MK10b;  MK11, MK12 (surface construction) ──> MK12b
MK13, MK14, MK15 alone;  MK15 ──> MK15b;  MK17 (knot insertion)
DF1 (volume maps) ──┬─> DF2 (procedural kinds)
                    └─> DF3
RP0 alone (decides Q7)
RP1 ──┬─> RP2 ──┐
      ├─> RP3 ──┴─> RP4
      ├─> RP5 ──> RP6 (MK11)
      ├─> RP7 (MK10's certificate module)
      └─> RP8 (with RP5)
```

- **Can start now (17 rows, plus a module):** LY2, BO10, BO13, BO17, BO18, BO21, BO25, BR11, SF1 (closed forms), MK4,
  MK7a, MK9, MK13, MK14, MK15, RP0, RP1, and MK10's certificate module on its own (for RP7).
  Checked against main: BO18's wire body exists (`scx-brep` shells list wire edges); MK15 is
  narrowed to analytic surfaces, because iso extraction on NURBS surfaces is not in
  `scx-nurbs-surface` yet (MK15b); RP2's B-spline enclosures exist (`scx-nurbs/src/eval_over.rs`)
  and SU6's gaps exist (`scx-surface/src/gap.rs`), so RP2 is ready once RP1 lands.
- **Startable once a step-7 or step-6 row lands:** BO10b, BO11, BO12, BO13b, BO14 to BO16b, BO19
  (BW3), BO20, BO22's stated and closed-form halves, DT4, BB1, MK5, MK16, RP2 to RP5, RP8.
- **Waits on step 5:**
  - BO23 on SM9, the pcurve fitter and step 5's ray against NURBS;
  - BO24, SF1's general curves and BO13b's general curves on the fitter; BO22's tracing on SM6b;
  - MK4b, MK6, MK7b and DF2 on the procedural kinds;
  - MK10, MK10b, MK12 on step 5's surface construction; MK11, MK15b and MK17 (NURBS curves) on
    knot insertion and degree elevation; RP6 on MK11;
  - DF1 on the volume maps.
- **Waits on step 6:** BO19 on BW3; MK8 on HL5.
- **The critical path to "done when"** is BO10 → BO10b → BO12 (with BO25) → BO15 and BO16, with
  DT4 growing beside it. BO10 can fail on main today; BO10b fixes what it finds before BO12 builds
  on the pipeline. The NURBS Boolean (BO23) is outside the analytic gate, as in step 4.
- **Step-8 families that can start early:**
  - `scx-mesh` (mesh to body; make, tess, mass): now, on MK3 and MK14.
  - `scx-sheetmetal` (develop, fold): once MK4 lands. The pin's develop uses its own maps
    (`K/develop/fold.rs:60-90, :412`), so it waits on no deform row; bends are cylinders.
  - `scx-taper` (draft walls, the pin's `taper.rs` and `taper_walls.rs` for line and arc profiles,
    which are exact): once MK5 and BO12 land; offsets of non-planar walls wait on step 5's offset
    kind.
  - `scx-local` (move, replace and delete faces with heal; the defeature removals `remove_faces`,
    `remove_feature_with_crossing_patches`, `remove_round_junction`, `remove_ring_groove`;
    `A/delete_face.rs`): once BO12, BO25 and BR11 land.

### 7.2 Amendments to 01 in this branch

LY2's design changes four rows of 01's crate table (`docs/01-architecture.md`), amended in this
branch:

- `scx-boolean` (row at :100): depends on bodyq, ssi, mass; the stitch edge is dropped (Q2).
- `scx-deform` (:102): the bend, twist and unbend maps (no B-spline volume, Q6); depends on brep and
  surface; no stitch (topology is unchanged) and no check (the result contract runs outside).
- `scx-make` (:103): adds ssi (MK5's mitre conics).
- `scx-repair` (:104): the certified heal, sew within a gap, hole patches and analytic rebuild, none
  needing Booleans; depends on stitch, check, make and surface (n-sided patches); the boolean edge
  is dropped. 01 placed it at layer 8 for Booleans it does not need; it stays at layer 8 because
  RP6 depends on make.

## 8. Truth tests per task

### 8.1 The Boolean algebra (BO10)

On every pair (A, B) and triple (A, B, C) of the testkit's Boolean recipes (T12) and convex recipes,
with `≡` meaning: equal volume and area to 1e-9 relative (by `scx_mass`, within the Estimate's
bound), the same T10 classification at 1,000 seeded points outside the band, the contract passing,
and the same count of shells and regions.

| Property | Statement | Notes |
|---|---|---|
| Commutativity | A ∪ B ≡ B ∪ A, A ∩ B ≡ B ∩ A | ids differ (`a`'s are kept); not compared |
| Shared-edge collars | for coaxial collars that meet along a shared circular edge or overlap over a length ℓ: A ∪ B ≡ B ∪ A, and vol(A ∪ B) = vol(A) + vol(B) − πr²ℓ, in both operand orders | the radial order at the shared edge, which the pin retries for (R2); the #20408 test (`tests/boolean.rs:50-75`) is mandatory: as a testkit recipe of its geometry now, and its own two records through DT4 once they can be read |
| Idempotence | A ∪ A ≡ A, A ∩ A ≡ A, for A and a translated copy A′ = A (two bodies) | the pipeline path, not BO20's shortcut |
| Self difference | A − A′ = ∅ | the pin's `boolean_api.rs:483`; also the empty-result refutation |
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
another is `Unsupported` is counted and printed, so the suite measures coverage as well. The
collars and #20408 are the exception: they must pass before BO10b closes.

### 8.2 Per task

| Task | Truths | Pin tests to port |
|---|---|---|
| BO10b | every BO10 failure fixed or filed with a typed cause; an empty result with a witness vertex is `Defect(EmptyRefuted)` on a recipe built to provoke it | `O/boolean.rs` refutation tests; `boolean_api.rs:483, :509` |
| BO11, BO12 | §8.1 n-ary order; closed-form volumes of unions of k boxes and k cylinders; cells counted against the arrangement of the operands (a closed form for boxes); the collars in every operand order | `O/boolean.rs:477, :489, :503`; `tests/boolean.rs:2948, :2990, :500, :50` |
| BO13, BO13b | the target's volume unchanged; section edges lie on both operands (certified gap); faces split into the closed-form count for a box imprinted by a cylinder; regions bounded by curves have their closed-form areas | `wr_619_imprint_skips_collapsed_definition` (outcome only); `split_face_by_curves` tests asserting areas |
| BO14 | section areas in closed form: plane through a sphere, cylinder, torus, box; the section's boundary on both surfaces | `planar_section.rs` tests that assert curves; `face_section` tests |
| BO15, BO15b | vol(front) + vol(back) = vol(body); each side's classification by the half-space oracle; a short tool refused | `tests/boolean.rs:3528`; split tests asserting volumes; `ToolStopsShort` tests |
| BO16, BO16b | the kept regions' areas in closed form; a pick on the cut refused | `tests/trim.rs:371`, `:627`; `trim_by_dropped_curves` tests asserting areas |
| BO17 | 2-D areas by the shoelace and arc segment formula, in exact rationals where the inputs are | `boolean2d` tests asserting areas |
| BO18 | intersection curves of box and cylinder, sphere and sphere, plane and torus in closed form | `intersect_faces_read_only` (9) |
| BO19 | gap and depth brackets contain the closed-form distance and penetration of spheres, boxes | — |
| BO21 | a true fact gives the same result as no fact, in less work; a false fact is `Refused(FalseFact)` | — |
| BO22 | tangent sphere-in-cylinder, cylinder-on-plane: closed-form volumes; the contact curve on both | `tests/boolean.rs` tangent cases, `curved_sliver_coincidence` |
| BO23 | the 73 §9 NURBS truths (SM0) as Boolean volumes | BO5's tests turn into results (SM9) |
| BO25 | a section sheet's area; laminas counted against the closed form | — |
| BR11 | merged faces' area equals the sum; protected edges survive; the contract passes | `A/heal.rs` tests asserting outcome |
| SF1 | a circle projected onto a plane along a direction is the closed-form ellipse; a line dropped onto a cylinder along its axis is a line | `project_curves`, `drop_curves` tests asserting geometry |
| MK4 | Pappus: volume 2π·x̄·A and area 2π·x̄·L per profile; a full sphere, torus, horn torus and frustum; poles valid by the contract; a true crossing refused | ops table :298, :302; `wr_590` (a build test: three faces and the Pappus volume); `api_brep.rs:1735`; `issue_2905:158` |
| MK5 | area × length on straight runs, Pappus on arcs, the mitre's exact conic | `sweep_cornered.rs:112, :178, :363`; ops table :307, :320, :343 |
| MK6 | Pappus for a square thread, the wall exactly on the screw surface; a law's offsets at its turns | `screw_sweep.rs:271`; ops table :474, :492 |
| MK7a | torsion of a helix frame, identity on lines, rotation on arcs, to 1e-12 | `sweep_stations.rs:74, :505` |
| MK7b | a twisted square prism's volume (twist preserves area); an end scale s gives the frustum volume; rails as MK10b | the app's `sweep_path.rs` tests asserting volumes |
| MK9 | the optimum against a brute-force search on small sections; pairing invariant under rotation of the start vertex; a banded answer that avoids its band edge reported as proven | `fork_ea59e9c_section_orders` (23) as outcomes |
| MK10 | the cylinder through stacked circles; the frustum on a straight rail; the certificate's bound above an exact sample on every span | ops `sweep.rs:1359, :1334, :1398`; `loft_rails.rs:147`; `certificate.rs:1658, :1678` |
| MK11 | the bilinear patch of four lines; a sphere octant from three arcs; a flat pentagon | `gordon.rs:866`; `O/surface.rs:543, :668`; `n_sided.rs:276` |
| MK12 | two G0 sections give the ruled surface; quadratic sections give the quadratic | `section_blend.rs:47, :132` |
| MK13 | hollow cone volume in closed form | `local_operation.rs` radial tests asserting volume |
| MK14, MK15, MK15b | area and volume per region; iso edges on the surface exactly | `prism/tests.rs` (73) where they assert quantities; `O/../tests/prism.rs` (35) likewise |
| MK17 | the ruled surface between two parallel lines is the plane strip of closed-form area; reversing twice is the identity | `ruled`, `reverse_sheet` tests asserting outcome |
| DF1, DF3 | the bent block's volume ∫ (u/d) dV in closed form; bend ∘ unbend = identity on points to a stated bound; plane cases exact by construction | `bend:1567, :1580`; `twist:1078, :1089`; `unbend:1488, :1500`; `p1:2660, :2918, :3053`; `bend:1775` |
| RP0 | none: a count, printed and recorded | — |
| RP1-RP4 | the check passes; no accuracy grew; `|ΔV| ≤ area × largest bound`; counts in closed form | all 18 of `O/../tests/heal.rs` except `:1402` (Q7) |
| RP5 | a box with one face moved off by δ < gap sews to a solid whose volume is within area × δ; each sewn edge's recorded accuracy contains the closed-form gap; δ > gap is `TooFar` | the app's `repair.rs` tests asserting outcome |
| RP6 | a box less one face patched back: the exact volume; a cylinder's missing cap patched planar; a curved hole's patch with its G1 bound recorded | `A/fill.rs` and `fill_general_tests.rs` tests asserting areas or continuity |
| RP7 | a NURBS cylinder, cone, sphere and torus rebuilt as the analytic surface; a perturbed one refuted and kept | the app's repair rebuild tests |
| RP8 | the step list in order equals the entries composed by hand, bit for bit | — |

### 8.3 Work counts for Q5 and R2 on large and hard inputs

The perf rule asks for before and after counts of added work. Two rulings add work in some cases;
BB1 measures them, with side-channel instrumentation in local runs (no committed counters), and
reports each as a table against the pin.

- **R2 (no union retry).** What is measured: per union, the work units charged (`Report.work`) and
  wall time in scx, against the pin's total for the same union including every attempt
  `union_copies` makes, the operand copies, the witness copy, and `matched_tolerances`' trial copy
  and Level2 check (counted by wrapping those calls in the bench harness, not in the pin's code).
  Inputs: DT4's Boolean recipes; the #20408 collars and BO10's collar recipes in both orders (the
  cases that take the pin's retry); and the largest corpus unions. The expected result is less
  work in scx on every case the pin retries, because scx runs once. The risk measured is the cost
  of BO12's geometry-only radial ordering at shared edges (sorting faces about an edge by exact
  tangent comparison, then curvature where tangents agree): counted as comparisons per shared
  edge, with the worst case on the collars. A rise above 10% against the slice's baseline on
  recipes with no shared edges fails, as the t_slice work rule does.
- **Q5 (exact pairing within a fixed limit, then banded).** What is measured: cells visited per
  section pair, by the exact search and by the band, against the pin's count on the same sections;
  and how many corpus lofts reach the band at all. Inputs: every loft in the corpus, plus generated
  section pairs of 10, 100, 1,000 and 2,000 vertices. The exact search costs O(mn log n) cells; the
  pin's MAX_CELLS of 4e6 (`loft_sections.rs:80`) is kept as the fixed limit so the branch taken is
  the pin's, and scx's added work is the band's proof check (whether the walk touches the band's
  edge), linear in the walk. BB1 records the cell counts before MK9 (the pin) and after.

## 9. Open questions

**Q1. Step 5 has no rows for the fitter, the procedural kinds, surface construction, the volume
maps or the ray against NURBS. Who writes them?**

- Recommend: the coordinator asks for one step-5 widening note (like 73 for the SSI) that gives
  these five their rows before step 7's waiting tasks are briefed. Step 7's tasks cite them by
  name until then. Seventeen of this note's 56 rows (and MK10's certificate module) can start now,
  and about 20 more wait only on other step-6 or step-7 rows.
- Against: a planning note for five large pieces delays them by a review cycle. Mitigation: the
  analytic step-7 rows (§7.1, "can start now") keep builders busy for that cycle.
- Answered by 77 (11-28): `research/77-step5-widening.md` gives the five pieces their rows (§7)
  and the step-7 rows each one unblocks.

**Q2. Does `scx-boolean` depend on `scx-stitch`?**

- Recommend: no. layers.txt already orders boolean below stitch, and the slice builds without it
  (BO3: known topology, no stitch). Every step-7 entry builds its topology from the imprint, and the
  pin's uses of stitch inside the Boolean (`boolean_2d`'s `StitchFaces`, the planar path) are
  replaced by known topology (BO17, MK5). LY2 removes the edge from 01's table.
- Against: some later entry (trimming sheets that only nearly meet) may want to sew. Mitigation:
  such a case is two operations at the caller's level (stitch, then Boolean), and the Boolean
  stays lower. `scx-repair` does keep its edge to `scx-stitch` (RP5's sew), and adds one to
  `scx-make` for RP6's patches (§7.2).

**Q3. A revolve profile that touches or crosses the axis.**

- What the pin does (READ): WR-590 is an interior touch: the profile meets the axis at one point
  inside it and leaves on the same side, so the revolve is a horn torus pinched at a pole. The pin
  splits the profile at the touch (`PC/p3/sweeps.rs:181-194`) and builds both halves, giving three
  faces and the Pappus volume. A true crossing, where the profile passes to the other side, is
  refused (`K/sweep/rotational_sweep_geometry.rs:364`).
- Recommend: the same split by meaning. MK4 makes an interior touch a pole vertex shared by the
  faces on either side of it (no split into two bodies: one solid with a pole, which `scx-brep`
  holds), and refuses only a true crossing with `Refused(CrossesAxis { at })`. WR-590 is ported as a
  build test: three faces, the Pappus volume, the contract passing.
- Against: a touch the input states only to within coincidence may be a crossing by a hair.
  Mitigation: the decision is the sign of the profile's distance from the axis on each side of the
  touch, decided by certified bounds; a touch that cannot be decided is `Unproven`, never guessed.

**Q4. N-ary Booleans: a fold now and the cellular merge later, or only the cellular merge?**

- Recommend: BO11's balanced fold now (small, starts at once, gives DT4 rows), replaced by
  selection over BO12's merge when it lands; the fold's tests stay as the oracle for the merge.
- Against: two implementations of one entry is waste, and a fold's intermediate bodies cost
  memory and work. Mitigation: BO11 is a few dozen lines over `boolean`; BB1 measures fold against
  merge before the fold is removed.

**Q5. Loft pairing: the pin falls back to a banded walk past 4e6 cells, which may miss the optimum.
Keep it?**

- Recommend: keep both, with the answer's status stated. The exact search runs up to a fixed
  per-operation cell limit (the pin's 4e6, a constant of the operation, not the remaining budget,
  so the branch taken is a function of the input alone, R6). Past it, the banded answer
  (`loft_sections.rs:1161-1336`) is returned with its bound: `Proven` when its walk does not run
  along the band's edge (the pin's own test that the band held the optimum), otherwise `Unproven`,
  with its cost as an upper bound on the optimum. Not `Exhausted`: an answer exists and is good.
  It is deterministic because the band is computed from the input.
- Against: an `Unproven` pairing may be worse than the optimum, and callers must handle the status.
  Mitigation: 4e6 cells is 2,000 by 2,000 vertices, far past CAD profiles; §8.3 counts how many
  corpus lofts reach the band at all.

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
  (`scx-import-sc`) may then refuse designs the pin opens. Mitigation: RP0 counts, before this is
  final, how many corpus records `heal_made` changes and how many of those then pass the check;
  each class it really fixes becomes a typed repair item (RP rows), certified.

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

**Q10. The revolve's analytic candidates: what of a cone whose height is below the pin's
MIN_HEIGHT (#17085)?**

- What the pin does (READ): it does not refuse. Below the floor, or when any analytic construction
  fails, the candidate is dropped and the face keeps its revolution surface or B-spline
  (`K/sweep/analytic_revolution.rs:26-30, :333`).
- Recommend: no fixed floor, and every line or arc profile gets its analytic surface. The plane snap
  rule (§3.6): a profile line whose two ends lie at axial heights within the coincidence
  (max(res, acc₁, acc₂)) of each other revolves to a plane normal to the axis at the start
  vertex's height; the end vertex's measured distance from that plane is recorded as its accuracy.
  Any other line is a cylinder (both ends at one radius within coincidence) or a cone, however
  low.
- Against: a near-flat cone is ill-conditioned for later SSI. Mitigation: SM2 and SM4 handle it by
  certified bounds; a conditioning warning is recorded, never a refusal or a fallback surface.

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
- **Differing from the pin where it is not truth** (Q3's single solid with a pole, Q5's status on
  the banded answer, Q10's cone where the pin keeps a B-spline) moves difftest outcomes. Each is
  recorded in DT4 as a by-design difference with its reason, never as a pin defect.
- **No sampling anywhere is ambitious.** R10 replaces every sampled acceptance with a proof. Where a
  proof is not yet available (G1 on n-sided patches, which the pin also does not certify), the
  answer is `Unproven` with its bound, which callers must handle.
- **Make in one crate is large.** Revolve, sweeps, lofts and patches together are about 60,000 pin
  lines. Mitigation: modules per construction, and the geometry (Coons, Gordon, n-sided, the volume
  maps) lives in `scx-surface`, so `scx-make` holds topology only.

## 11. Proposed 04 rows (ruled by the coordinator, for Brett's review)

| date | status | decision | why/against | ref |
|---|---|---|---|---|
| 11-28 | Ruled 11-28 (coordinator), for review | **Step 7 plan.** `scx-boolean`'s entries (`boolean`, `boolean_all`, `merge`/`select`, `imprint`, `imprint_curves`, `split`, `split_faces`, `section`, `trim`, `planar`, `intersect`, `interference`, `boolean_given`) are one pipeline (R1), take `&Brep` and return `Done<C>` (`interference` returns a decided `Interference`); proof is the default of `boolean`, not a mode, and an empty result with a witness inside is `Defect(EmptyRefuted)` (a ported postcondition); no retry, operand swap or rebuild (R2: the pin's one retry is `union_copies`, which every app union goes through; BO12's geometry-only radial order at shared edges replaces it, and the #20408 collars are a mandatory truth); no tolerance ladder (R3). BO10b fixes what BO10 finds before BO12; BO13 runs before BO12, rework accepted. Face and edge merge are `scx-brep` Draft methods (BR11). Tasks: 75 §7 | The pin's modes and retries are the hazards; one pipeline means one set of proofs. Against: entries with different needs (2-D Boolean) may be faster alone; and the retry hides an order dependence BO12 must remove; BB1 measures both (75 §8.3) | 75 §1.2, §2.6, §8.3 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Make builds exactly.** Constructors build the analytic kind directly where one exists (R4); otherwise the step-5 procedural kind; NURBS only where the operation is a fit, with a certificate. Choice by a predicate on the input before building (R5). `Made<R>` with typed face records replaces out-params (R7); no `cap: bool` or integer cap modes; no stitch in constructors, including n-sided patches (shared sides known). Added: `ruled` and `reverse` (MK17), the screw's radial law and joined walls (MK6), the sweep's twist, end scale and rails (MK7b), `face` on analytic surfaces now and NURBS after iso extraction (MK15, MK15b) | Removes build-then-recognise sampling and its fallbacks. Against: more code paths than one general sweep; each is small and closed-form tested | 75 §3.6, §4.6 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Revolve splits at an interior axis touch and refuses only a true crossing.** A touch (WR-590, a horn torus) becomes a pole vertex in one solid; a crossing is `Refused(CrossesAxis)`, remedy `ChangeInput`; segments on the axis become pole vertices; WR-590 is a build test. No MIN_HEIGHT floor: the plane snap rule makes a segment perpendicular to the axis within coincidence a plane, every other segment a cone; the pin's kept revolution surface or B-spline is not ported | Matches the pin's meaning (it splits a touch and refuses a crossing) with exact surfaces. Against: a touch within coincidence of a crossing; decided by certified bounds or `Unproven` | 75 §3.6, Q3, Q10 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Loft pairing is exact within a fixed per-operation cell limit, then banded with its status**: past the limit the banded answer is returned, `Proven` if its walk avoids the band's edge, else `Unproven` with its cost as the bound (never `Exhausted`); deterministic since the band is a function of the input. Certificates have a fixed per-operation budget and record `Raise` on an unproved span, never `RailPastStationCap` | Keeps the pin's good answer on large sections and states what is proved. Against: callers must handle `Unproven`; 75 §8.3 counts how many corpus lofts reach the band | 75 §4.6, Q5, §8.3 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Repair is a list of certified steps.** Separate entries `heal` (the certified healer, items per defect, `Skip` mode, no accuracy grows), `rebuild_analytic` (only where a certificate proves the analytic surface), `sew` within a gap over `scx-stitch` (each sewn edge's accuracy is its certified gap, recorded; the pin's 2× factor not ported) and `patch_holes` (plane exact; else MK11's patch G1, `Unproven` where uncertified), and `repair(&[Step])` for the app's Repair Part, replacing `RepairOptions`' bools. No run-time check-compare gate; no global `set_heals_tolerance_defects`. The SMLib healer is not ported unless RP0's count of what `heal_made` fixes on the corpus shows a class worth a certified item | Certified edits make the gate redundant; steps make the options data. Against: imported records may need SMLib fixes; RP0 counts them first | 75 §6.5, Q7, Q8 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Deform is narrow in step 7**: bend, twist and unbend maps (geometry in `scx-surface`), `scx_deform::deform` with exact images decided by kind and frame and the procedural in-volume kind otherwise; no B-spline volume; depends on brep and surface only. Supersedes 01's `scx-deform` row (01:102), amended in this branch | The pin has no body deform and a stub B-spline volume. Against: no caller; it is last in priority and may move to step 8 | 75 §5.5, Q6, §7.2 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Layers (LY2).** `scx-deform` after `scx-hlr`; `scx-repair` after `scx-make`; no `scx-boolean` → `scx-stitch` edge; new edges: `scx-make` → `scx-ssi`, `scx-hlr`, `scx-boolean`; `scx-repair` → `scx-stitch`, `scx-check`, `scx-make`, `scx-surface` (no `scx-boolean`); `scx-deform` → `scx-brep`, `scx-surface`. 01's rows for boolean, deform, make and repair (01:100-104) are amended in the same branch | Matches layers.txt as it stands and the built slice. Against: a later Boolean entry may want to sew; it composes at the caller | 75 §7, §7.2, Q2 |
| 11-28 | Ruled 11-28 (coordinator), for review | **The Boolean algebra suite (BO10) is the step-7 gate's property half**: the §8.1 properties on every built pair and triple, including coaxial shared-edge collars in both operand orders and the #20408 collars (mandatory), judged by volume, area, the T10 oracle, the contract and run-twice bits | The pin has one commutativity check; properties catch what recipes miss. Against: it will fail widely at first; BO10b fixes the slice's failures before BO12, the rest become issues | 75 §8.1 |

## 12. What this note did not do

- It did not build or run the pin, measure any work count, or run any scx code.
- It did not read copyleft kernels; every algorithm named is from the pin (Apache-2.0 here) or a
  published source (The NURBS Book, Farouki-Rajan, Higham).
- It did not plan step 5's fitter, procedural kinds, surface construction, volume maps or ray
  against NURBS (Q1), nor step 8's families beyond naming their early starts.
- It did not check the testkit recipes' coverage of step-7 cases; DT4 and BO10 will.

## 13. As built

### 13.1 RP0 as built (11-28)

**What was run.** `crates/scx-difftest/examples/heal_count.rs` (`cargo run --release -p
scx-difftest --example heal_count`, deterministic, no counters in any library). Each record is
built twice through the pin's `solidcore-convert` (`to_kernel_body`): trusted (no healer) and
untrusted (`heal_made` runs). Both bodies go through the pin's `check_brep` at level 2
(pointers, then `AssertValid`), failures classed by the failing report's name. A record is
*altered* when the healed body differs in entity counts, vertex or edge tolerances, or the
record it converts back to. Records: every distinct body of the rig corpus
(`kernel_cases`, 1,484 bodies, 1,248 distinct; all of them are marked trusted there, so
none reaches the healer in the pin: each is healed here as if it were untrusted) and the pin's
STEP fixtures read by `solidcore-step` (4 solids). The NIST STEP files are not on the build
machines (the fetch script needs `unzip`); `SCX_HEAL_STEP_DIR` runs any directory of them.

| | rig corpus | STEP fixtures |
|---|---|---|
| distinct records | 1,248 | 4 |
| refused to build, with or without the healer | 0 | 0 |
| failing the check before the healer | 1 | 0 |
| of those, passing after the healer | **0** | 0 |
| altered by the healer | 177 | 3 |
| of which valid before | 176 | 3 |
| altered, tolerances only | 131 | 3 |
| altered, entity counts (topology) | 31 | 0 |
| altered, geometry or trims | 15 | 0 |
| passing before, failing after | **15** | 0 |

The one record failing before (three reports: Face has Coincident Edges, Coincident
Vertices, partially Coincident Edges) is not fixed: two of the three reports remain. The 15
that go from passing to failing all fail `Tolerance` after healing (the healed
bodies carry a tolerance the check refuses; e.g. `booleans/5322-cut-with-a-5000-mm-tool.json:b3`,
`fillets/16019-chamfer-1-on-the-top-rim-of-a-cylinder-r125.json:b1`).

**Recommendation for Q7: do not port the healer.** On this corpus it repairs no failing
record (0 of 1) and makes 15 valid records fail the check, while widening tolerances on 131
and changing the topology of 31 valid ones. The class worth a certified item: none shown.
For (keeping the question open): the corpus holds no record that reaches the healer in the pin
(all trusted) and only 4 STEP solids, so real untrusted input is not sampled; the NIST files
should be run once through `SCX_HEAL_STEP_DIR` before the row is closed, and a Q7 reversal
needs a class of failing records with a count. Against porting, in numbers: 15 regressions,
0 fixes.

NIST run (coordinator, 11-28): the 33 NIST MBE PMI STEP files (public domain; fetched by the pin's
pinned-hash script) through the same harness (`SCX_HEAL_STEP_DIR`). 33 records: none refused; 2 fail
the check before the healer (Bad Knot Count, Bad Repeated CPts) and both still fail after it; the
healer alters 32 (31 tolerances only, 1 geometry or trims) and turns no valid record invalid. Over
all three sets the healer fixes 0 of 3 failing records, so Q7 is ruled: not ported.

### 13.2 BO18 as built (11-28)

`scx_boolean::intersect(a, b, cx) -> Result<Done<NoItems>, BooleanError>` runs stages 1-4 of `boolean` (`interfere`, `face_face`) and stops; `crates/scx-boolean/src/intersect.rs`. The result is a `Draft::new()` body of no faces:

- each stage-4 `Section` is a wire edge on its curve over its span, between the vertices of its merged points; a closed section no merged point lies on gets one vertex at its start;
- each `Lying` piece (an operand edge on the other body) is a wire edge on the operand edge's curve over the piece, written once when the other operand's edge also runs along it (same end vertices and a midpoint within the zone);
- each `Touch`, and each merged point no edge ends at, is a lone vertex (touches deduped against written vertices within the zone);
- a coincident pair is a region, so only its boundary (the lying pieces) appears;
- one shell of the outside region per connected piece (union-find over vertices, shells in order of lowest vertex), since a shell must be connected: two circles apart are two shells; a lone vertex is a shell of its own; no contact is a body with no shells.
- lineage is `Origin::Operand(op, entity)` for both operands (the draft is not `a.edit()`, so `Origin::Face` of `a` has no meaning).

Decisions. Fresh draft against `a.edit()` with everything removed: for, the result is a new body and carries no `a` ids; against, ids of `a` are not preserved for callers that want to map back (they use the lineage). Dedupe by geometry against by `along`: `along` is only set for edges lying along an edge, but the same piece can also come from two faces of one operand, so geometry (ends plus midpoint) covers both; against, a quadratic scan over written edges (fine at the sizes here). Lone vertices from unused merged points are included: the other choice (touches only) would lose a point where only edges meet.

Truths (tests/intersect.rs, 12 tests): disjoint cubes, no shells; overlapping cubes, six lines of 0.5 and six vertices; cylinder through a plate, two circles of 2 pi in two shells; cylinder on a block side, arcs pi, pi and rulings 1, 1 in one shell; cylinder with a square plane, a unit circle; a tilted plate, two ellipses of semi-axes 1 and sqrt 2 (perimeter by quadrature); two unit spheres, circle of length sqrt 3 pi (two arcs); plane through a sphere's centre, 2 pi; tangent plane, one lone vertex at the closed-form point; plane through a torus's mid plane, 20 pi (the inner and outer circles of R = 5, r = 1.5); operand swap and repeat agree; shared cube face, four unit edges once each. The solid `contract` does not apply to a wire body; `finish` checks it.

Open: no pin test of `intersect_faces_read_only` (9) was ported (it needs the pin's face-level API); the NURBS and general-SSI pairs stay `Unsupported(SsiPair)` as in `boolean` (BO22/BO23).

### 13.3 BO13 imprint as built (11-28)

`scx_boolean::imprint(target, tool, what, cx)` (`imprint_op.rs`) runs stages 1 to 5 and
finishes the draft with no selection. Choices 75 left open:

- **The result is the target's part of the stage-5 draft.** Stage 5 writes both operands whole;
  the entry removes the tool's faces, then the edges, vertices, shells and regions only they
  held, and finishes. `Done.changes` is then the target's own edit (split faces with their
  pieces, cut edges, new vertices, section edges whose origins include the tool's faces as
  operand 1); the pin's `(Body, bool)` is `!changes.is_empty()`. For: the target keeps its
  material and sense, so volume and area are the target's; nothing needs classifying. Against: a
  caller wanting the tool's faces cut as well must swap the operands and call again.
- **`Imprint::Edges` or `Imprint::EdgesAndFaces`.** Both write every transversal section and cut
  the target's edges at the merged points. `EdgesAndFaces` also writes the tool's edges lying in
  the target's faces (stage 4's `lying` with no `along`), which is what splits a target face a
  tool face rests on into the part under it and the rest; `Edges` drops those. For the split: a
  block standing on a face is then imprinted or not at the caller's choice (tests: 7 faces
  against 6 with no changes). Against: the pin has one Imprint mode; BO12 may find the choice
  unnecessary once cells replace pieces, and the variant can then go.
- **No new counters, no new stage code.** The entry filters stage 4's list and reuses stage 5;
  the cleanup repeats the draft cleanup the stage-5 tests used.

Truth tests (`tests/imprint.rs`, closed forms): offset cubes give 9 faces, 21 edges, 14 vertices,
areas 3 x 1/4 + 3 x 3/4 + 3 x 1; a thin cylinder through a cube gives 8 faces, 14 edges, 10
vertices, two disks of pi/16 and two faces of 1 - pi/16; a block on a face, a block apart,
determinism and operand swap, and a refusal of another document. Volume is 1 throughout.

### 13.4 MK14 as built (11-28)

`scx_make::prism(&[Region], &Frame, Span, &Document) -> Made<PrismFaces>` and
`scx_make::sheet(&[Region], &Frame, &Document) -> Made<SheetFaces>`. `PrismFaces { solids:
Vec<ExtrudeFaces> }` is one record per region, in the order given; `SheetFaces { faces: Vec<FaceId> }`
one face per region. The solids are written by the same code as `extrude` (which now calls
`extrude_into`), so each is exactly what `extrude` makes of its region.

Decisions, with cases for and against:

- **One solid per region, in one body; not a union.** Each region has its own solid region and
  shell, and its own shell in the outside region. For: the pin joins separate extrusions by
  `union_all`, and the regions are apart (below), so the union is the same solids with no
  Boolean. Against: the pin's body is one `union_all` result whose solids could be re-ordered;
  ours are in input order, which is what a caller wants to index by. The outside needs a shell
  per solid because `finish` refuses a disconnected shell (`ShellNotConnected`).
- **Regions that overlap, touch or cross are refused, `Unsupported::RegionsMeet { first, second }`,
  not merged.** The pin first unites the regions in the plane (overlap normalised, volume 300
  for two overlapping 10 x 10 squares) and refuses pieces of the union that touch. No planar
  Boolean exists until BO17, so the merge cannot be done; a refusal is the honest answer and is
  `Unsupported`, not `Refused`, since the input is valid. For: nothing is guessed, and BO17 can
  add the merge as a wider entry later without changing this one. Against: a caller with
  overlapping profiles must unite them first. A region in another's hole, or beside it, is kept
  (the pin's islands).
- **Meeting is decided by `Region::new` on a probe, not by new geometry.** For `a` and `b`, the
  probe is `a`'s outer loop with every other loop of both as holes; its contact check (every
  pair of segments within the resolution, crossing before touching, between loops too) names a
  crossing or touch before nesting, so those two errors mean the boundaries meet. If the
  boundaries are apart, one start point of each region is classified against the other
  (`Region::classify`): inside or on the boundary means one lies in the other's material. For:
  no new predicate in `scx-planar`, exact and certified as `Region::new` is, a hole-in-hole
  island handled by classification. Against: it builds a throwaway region per pair, so the check
  is O(k^2) probes for k regions and each probe is linear in the segments of both.
  Measured (release, throwaway timers, whole circles on a grid, one probe per pair): `prism` of 4, 16, 64
  and 256 regions takes 0.13, 0.44, 3.6 and 28 ms against 0.09, 0.30, 1.2 and 2.6 ms for the same
  regions extruded one by one; the sheet's 0.03, 0.20, 1.9 and 27 ms is nearly all the check. So the check
  added is quadratic, small to about 64 regions and dominant at 256; a bounding-box prune of pairs is the
  fix if a caller passes hundreds of regions (not done: no such caller yet).
  A pair's one start point decides nesting only because the boundaries are apart (a region is
  wholly inside or wholly outside another's material).
- **A sheet's face has both sides in one shell**, a shell per face (the `scx-brep` sheet form);
  its front is the frame's +z side and the outer loop runs counter-clockwise about it, so
  `scx_mass` gives the regions' area and no volume. The sheet is `z = 0` of the frame; there is
  no height argument (a sheet's place is its frame's).
- **`Assembly` gained `sheets`, `begin_solid` and `sheet_face`**, and the outside shell and solid
  shell are `Option`s (a sheet has neither): `face` on a sheet is `Defect::NoSolid`. `error.rs`
  gained `Refusal::NoRegions`, `Unsupported::RegionsMeet` and `Defect::NoSolid`.

Tests (`crates/scx-make/tests/prism.rs`, 11): see the file's header for the pin tests removed on
purpose and why.

### 13.5 MK13 as built (11-28)

`scx_make::radial(&RadialCone, &Document) -> Result<Made<RadialFaces>, MakeError>` builds the hollow cone in one `Draft`: outer wall, inner wall and an annular cap at each end that is not a hinge, with no stitch, no outward check and no subtract. `RadialCone { frame, height, outer: [Length; 2], inner: [Length; 2] }` gives radii at `z = 0` and `z = height`; `RadialFaces { outer, inner, bottom: Option, top: Option }`. Counts: both ends capped 4 faces, 6 edges, 4 vertices; one hinge 3, 5, 3 (the walls share the rim and its vertex). Walls are cylinders when their radii are equal, cones otherwise; the inner wall is the same surface with `Sense::Reversed` and a loop run the other way round, so its front is the hollow.

Decisions.
- Hinge by exact equality of the radii. For: the pin's hinge case is the draft whose neutral plane is the end plane, equal by construction, and exact equality needs no tolerance (R3). Against: a caller whose radii agree to rounding gets a cap of width zero refused; the refusal is `Refusal::Annulus` and the caller can set them equal.
- A wall thinner than the resolution without being a hinge, an inner radius above the outer, and two hinges (no volume) are `Refusal::Annulus { outer, inner }`; inner radii must be above the resolution (an inner apex is not made; the pin refuses it too, `RadialRimsNotAnnulus`).
- A reversed `Cone` runs `v` along `-z`, so the inner wall's half-angle is the negated one; found by the exact-pcurve write refusing the first build.
- New `Dimension::InnerBaseRadius` and `InnerTopRadius`. Edits to `lib.rs` are the module and its re-export; `error.rs` gains the two dimensions and the `Annulus` refusal.

Truths (`tests/radial.rs`, 5 tests): the pin's `radial_sweeps_close_at_the_hinge_or_on_two_annular_caps` (hinge and both-capped, volume `pi h (300 - b^2 - b t - t^2) / 3`) and `a_cylinder_whose_top_rounds_...` (volume cylinder less frustum) against the difference of the frusta, at an axis-aligned and a tilted far frame; narrowing and widening cones, hinges at either end, a tube, crossing slopes. Each: counts, closed-solid structure with Euler-Poincare of genus 1, the BR8 oracle, volume, area and centroid to 1e-12 with the mass estimate's error within, face kinds, and membership of each face against the CSG difference of two frusta; determinism; size and annulus refusals. The pin's stitch, outward-check and attribute-copy parts have no counterpart (nothing to stitch; attributes belong to the caller of the local operation).

### 13.6 BO10 as built (11-28)

`crates/scx-boolean/tests/algebra.rs` (about 1,100 lines, 15 live tests, 12 ignored; the live ones run in about 10 s wall under nextest).

**Shape.** An expression over a pool of 11 placed parts (8 in general position: three boxes, a cylinder, a sphere, a rotated block, a cone, a torus; then a block sharing A's z planes, a block abutting A's x = 10 face, and a sphere centred on A's x = 10 plane) is built twice: as bodies through `boolean`, and as a testkit `Csg` oracle. Two expressions are equivalent when each result passes `scx_check::contract` (`Contacts::Radial`), volume and area agree to 1e-9 relative plus the Estimates' errors, solid-region and shell counts agree, and 1,000 seeded points classify as the oracle says wherever it is certain at 2C. Laws: commutativity, idempotence (two separately built bodies), self difference, absorption, partition, inclusion-exclusion and partition in volume, De Morgan in a box (both forms), associativity, distributivity, rigid motion (rotation plus a 1e6 translation), determinism (`scx_record::encode` bit-identical over two runs). N-ary order waits on BO11 and BO12. A case where any Boolean answers `Unsupported` is counted and printed per law (`--no-capture`), never asserted.

**Collars.** Coaxial r = 1.3 collars, bare and each on a block, overlapping over 2 or meeting in a ring, in both operand orders: union volume = vA + vB - pi r^2 l, intersection = pi r^2 l, both differences, one solid of one shell. They pass today. The #20408 case is the blocks-overlap-2 variant (its geometry: r = 1.3, l = 2); the pin's two records wait for DT4.

**Known failures.** The failing cases are listed in `KNOWN` as (cause, law, label); the live sweeps skip them and one ignored test per cause runs its cases and asserts they pass, so the suite keeps them. Causes: wrong-points (9, silent wrong geometry), seeds (28), loops (16), build (17), classify (10), loosespan (9), uvundecided (6), piece (1), contract-undecided (16 incl. far), contract-nobox (23 incl. far).

**Choices, for and against.**
- Case-granular known list with one ignored test per cause, against one `#[ignore]` per law: a whole-law ignore hides the cases that pass, and one per case is 135 tests. Cost: the table is data that BO10b must delete as it fixes.
- Judging each result against its own oracle as well as against its twin: a law could hold between two equally wrong results (both Booleans missing the same region); the oracle catches that.
- Sphere centre: the first pool put the sphere's centre on A's x = 10 plane, which fails with `Seeds` in every combination with a box. A sphere off every plane passes those, so the pool holds both: the general one for the laws, `sphereOnFace` as a real contact case.

### 13.7 MK7a frames as built (11-28)

`scx_make::{Path, Piece, Stop, Station, Rotation, Corner, FrameError, frames}` (`frames.rs`,
`tests/frames.rs`). `Path::new(&[Piece], &Document)` validates (1 to 1000 pieces; lines longer
than the resolution; arcs and helices off their axis, angle in (0, 1000 turns]; pieces meeting
within the resolution; a corner that doubles back refused) and measures once, so `frames(&Path,
&[Stop]) -> Result<Vec<Station>, FrameError>` cannot fail on the path. A `Station` is the point,
unit tangent and a `Rotation` (the transport from the path's start, columns, `apply`, `after`,
`inverse`, `defect`). Closed forms as the pin's: identity on a line, the angle run about the axis
on an arc, the screw then `psi = -b phi / sqrt(a^2 + b^2)` about the tangent on a helix, the least
rotation `t0 x t1` by the angle at a corner (none within `G1_TANGENTS` = 1e-8 of each other).
`Stop::Corner(j)` is the mitre plane (half turn, bisector tangent); `Corner` carries the angle,
the joint, the fraction and the stretch `1 / cos(angle / 2)`. Rodrigues runs on `fmath`
(deterministic across platforms; the pin used the platform's libm).

Decisions and cases:
- **Own `FrameError`, not `MakeError`.** 75 §3.6 writes `Result<Frames, MakeError>`. For: a frame
  law has no body, three of `MakeError`'s four kinds cannot occur, and a new `Refusal` variant
  would touch `error.rs` that MK4, MK9, MK13 and MK14 also edit. Against: one error type for the
  crate. The sweep (MK7b) adds `From<FrameError> for MakeError` when it needs it; no change here.
- **No `Cx`, no laws.** The signature is pure (`&Path`, `&[Stop]`): the work is closed form and
  O(pieces) per stop, no budget, scratch or cancellation to meter. Twist, end scale, orientation
  and rails are MK7b's (the pin's `Laws`, `Orientation`, `RailFit`, `railed`); `fractions` (the
  skin's one station per degree rule) is a SolidCraft convention for the skin and goes with the
  sweep. The pin's `Stop::Corner` mitre stretch is `Corner::stretch`, applied by the mitre's user.
- **Literals into the document.** The pin's `1e-9 (1 + |P|)` for "pieces meet" and for a line
  with no length is the document's resolution here; `G1_TANGENTS` stays a named constant (the
  procedural sweep's rule, which step 5 will own; the sweep must use the same one).
- **Axis by type.** `Piece` takes a `UnitVec3`; the pin's "unit within 1e-12" check and its
  re-normalising are gone.
- `Piece` and `Stop` are `#[non_exhaustive]`: MK7b may add pieces when step 5 certifies a
  spline path's frame (#2572).

Truths (`tests/frames.rs`, 10 tests, closed forms, 1e-12): the line is the identity; a planar
arc turns the frame by the angle run (point, tangent, frame); a whole helix turn leaves the
tangent and turns the frame by `-2 pi b / sqrt(a^2 + b^2)` (pin :74); at a quarter and a half
turn the start normal becomes `cos psi N + sin psi B`, right- and left-handed (pin :505);
parallel transport turns about no axis along the chord on tilted helices of both hands and a
tilted arc (skew part along the chord < 1e-9, pin :569); a right-angle corner is a quarter turn
with the mitre the eighth and stretch sqrt 2, a fraction at the joint is the second piece's
start (pin :166); the corner is the least rotation at 1, 30, 90, 135 and 179 degrees (new); a
corner then a helix compose (pin :536); a kink of 1e-10 is no corner and 1e-7 is one (pin :768);
refusals of a doubling-back path, a gap, no pieces, a degenerate or axis-start piece, a negative
angle, fractions outside [0, 1] or NaN, and a closed arc's whole-turn identity. Orthonormal
defect below 1e-13 throughout. Removed on purpose, with the file header listing them: the pin's
twist/scale, orientation, rail, `fractions` and limit tests (MK7b's laws), and the stretch
half of the corner test (checked as `Corner::stretch`).

Cost: new code only, no existing path changed. A station walks the pieces before it, one
`sin_cos` and about 40 flops per rotation composed; 1,001 stations of a helix (the transport
test) run in a few milliseconds in a debug build.

### 13.8 MK9 as built (11-28)

`scx_make::loft::{align, align_banded, pair}`. `align(first, second, &mut Budget, &Stop) ->
Result<Alignment, PairError>` pairs two closed loops of `scx_planar::Segment` (lines, arcs, whole
circles) given in one plane's coordinates. `Alignment { start, events: Vec<Event>, cost,
optimality }`. Its events run round both loops in order, from vertex 0 of the first and vertex
`start` of the second: `Pair { first, second }`, `FirstAlone { vertex, on }` or
`SecondAlone { vertex, on }`, where `on` is the other loop's segment the lone vertex lands on.
`pair(&[&[Segment]], ..) -> Pairing { alignments }` pairs each section with the next. It checks
every section before pairing any, and refuses a section wound the other way from the one before
it. `align_banded(first, second, cells, ..)` always runs the band. It is public so that the band
can be measured against the exact search. `MAX_CELLS = 4_000_000`. The cost, the
divide-and-conquer search over starts and the band are the pin's (`loft_sections.rs`). The area
and centroid are in closed form for arcs.

Decisions, with cases for and against:

- **The banded answer is always `Unproven`, against Q5 and the 04 row's "`Proven` if its walk
  avoids the band's edge".** That rule proves nothing. The band has one start, so a walk that
  keeps off the band's edge is least only among walks inside the band from that start. A cheaper
  walk can start elsewhere or run wholly outside the band.
  - The pin's own case shows this (in-module test
    `a_banded_walk_off_its_edge_is_not_the_optimum`). It pairs a 10 x 1 rectangle in 498
    segments with a 1.5 x 1 rectangle in 500, through a band of 40,000 cells. The band's walk
    keeps off its edge and matches the whole grid's walk from the same start, yet the search
    over every start finds a walk more than 1% cheaper.
  - On random star polygons (`the_band_is_an_upper_bound_on_the_exact_search`: 200 pairs of 20
    to 120 vertices, half-width n/10), the band finds the optimum in 97 of 200. It is up to
    108% above it in the worst case.
  - For: the status is never false, and the type already carries both values.
  - Against: every loft past the limit is `Unproven`, even when the band happens to be optimal
    (on regular shapes it is: `on_regular_shapes_the_band_finds_the_optimum`).
  - Considered and not built:
    - A cheap lower bound, `Σ_i` (the least cost of a step into row `i`): O(mn) time and linear
      memory. It would turn some `Unproven` answers into `Optimal` when bound and cost meet, but
      on these shapes it is far from tight.
    - An exact search confined to a band by a proof of the band's width (Ukkonen-style
      cut-offs). Its bound needs a least step cost, and UNPAIRED alone (0.1) gives a width near
      the whole grid.
  - Either can come later without changing the API. The 04 row should drop "`Proven` if ...".
- **A moved band is pinned to the walk's ends (truth over pin).** The pin re-centres the band on
  the middle of each row's run of the walk. When the walk runs along the last row for more than
  the band's width, the last row's centre falls short of the end. The end then lies outside the
  band, and the pin's walk fails with "unpaired".
  - Seen here on a random 27 x 102 pair with half-width 1, now in
    `the_band_is_an_upper_bound_on_the_exact_search`.
  - `settle` makes the centres run from the start on row 0 to the end on the last row, never
    back.
  - With that, each row's band meets the next and the end is inside. The band's walk is always
    found when h >= 1.
- **Exact means exact.** `the_exact_search_is_the_brute_force_optimum` (integration test, 600
  random star polygons of 3 to 7 vertices) checks against code that shares nothing with the
  search. That code recomputes each step's cost from the polygon's vertices (shoelace centroid,
  unit area) and enumerates every monotone walk from every start.
  - The search's cost equals the brute-force least to 1e-12 relative.
  - Its events form a walk round both loops, and their recomputed cost is the reported one.
  - The in-module `the_search_finds_the_brute_force_optimum` does the same over the module's own
    step costs for 1 to 6 vertices, with arcs and whole circles among them.
  - Turning either loop's numbering round leaves the cost unchanged to 1e-9.
- **Its own error type, `PairError`, not `MakeError`.**
  - Variants:
    - `Refused(PairRefusal)`: `Empty`, `NotFinite`, `Degenerate`, `NoArea` or `Winding`.
    - `UnsupportedSegment`.
    - `TooLarge`: even a band one column wide does not fit.
    - `Exhausted` and `Stopped`.
    - `Unreached` (`Defect`).
  - For: MK4, MK13 and MK14 edit `error.rs` at the same time, and the pairing has no solid to
    refuse. MK10 maps it into `MakeError` with the section index kept.
  - Against: one more error type for a caller of `pair` alone.
- **Work.** Each cell visited charges one unit to the caller's `Budget`, and the `Stop` flag is
  polled once per grid row. Which search runs depends on the fixed `MAX_CELLS`, not on the
  remaining budget, so the answer is a function of the input (R6).
- **Scope left to MK10:**
  - winding the sections alike;
  - carrying a shared frame from plane to plane (the caller gives every section in one plane's
    coordinates);
  - cutting segments where lone vertices land (the pin's placement by share of length);
  - splitting whole circles at the neighbouring section's directions;
  - merging runs of one curve.
- **75 §8.2's `fork_ea59e9c_section_orders` is a blend cross-section test, not loft pairing.**
  In its place are the pin's loft-pairing outcomes, as tests on the alignment:
  - square to diamond;
  - 5000-gons and a 3000-gon with a 5000-gon, in the band;
  - split 5000-segment rectangles pairing all four corners;
  - both comb cases of 1100 teeth, past the limit even for their corner polygons.

Measured (release on a 24-core build machine, throwaway timer; random star polygons):

| m x n | search | cells charged | ms |
|---|---|---|---|
| 10 x 10 | exact | 558 | 0.02 |
| 100 x 100 | exact | 76,835 | 2.0 |
| 500 x 500 | exact | 2.59 M | 67 |
| 1000 x 1000 | exact | 11.5 M | 306 |
| 1999 x 1999 | exact | 49.7 M | 1,476 |
| 2000 x 3000 | band, 8 passes | 41.6 M | 1,051 |
| 3000 x 3000 | band | 6.65 M | 161 |
| 5000 x 5000 | band | 7.49 M | 180 |
| 10000 x 10000 | band | 7.86 M | 198 |

- The exact search charges about 11 to 12 times the grid at 1000 to 2000 vertices (`m·n·log2 n`
  / 2 plus overlaps), at about 30 ns a cell.
- A band charges at most `BAND_PASSES` times `MAX_CELLS` plus the corner polygons' search, so
  about 1 s at worst.
- A band of a tenth of the grid on the same random shapes is 1% to 5% above the optimum.

Tests:

- `crates/scx-make/tests/pair.rs`: 14 tests.
- In-module: 3 tests (brute force over the module's costs, the half disc's closed-form area and
  centroid, and the band counterexample).
- None removed.
### 13.9 BO17 `planar` as built (11-28)

`scx_boolean::planar(a, b, op, cx)` (module `planar.rs`, tests in `planar/tests.rs`). It reads
each face of each operand as a `scx_planar::Region` in the plane of `a`'s first face, builds the
arrangement of the two boundaries and builds the result as a sheet of planar faces in a new
draft, loops given exact pcurves by `Draft::exact_loops`. No stitch (Q2), no 3-D stage.

Decisions where 75 left a choice:

- **Selection by sides, not by cells.** Every piece between cuts is grouped with the pieces on
  the same curve; each group knows the faces of each operand on its left and right (from the
  operands' own uses, whose loops keep material on the left, else by `Region::classify` of its
  midpoint). The result's boundary is the groups whose left and right differ in the operation's
  answer. For: no angular ordering is needed to decide anything but loop tracing; coincident
  and shared edges fall out; an operand of several faces sharing edges is its union. Against:
  a midpoint on an operand's boundary the other's pieces do not name is `PlanarUndecided`.
- **Arcs in pieces of a quarter turn or less** (a circle is four), each with its exact rational
  quadratic for `crossings` (which takes no arc past a half turn). The cut points of a meet are
  moved onto both curves by the closed form (line-line, line-circle, circle-circle, nearest
  root to the certified point): the certified `at` was 1e-6 off, which showed as 4e-8 relative
  area error. Joined back by chain at degree-2 vertices on one line or circle, so a union of two
  rectangles has 4 edges, a lens 2, a whole circle 1.
- **Tracing**: at a vertex with several kept pieces leaving, the first met turning clockwise
  from the way back, ties (within 1e-9 rad) by larger signed curvature. This is the one angle
  tolerance; it only orders pieces already known to leave one vertex.
- **One shell per result face** (a sheet shell must be connected; `finish` refuses a shell of
  faces apart or empty). An empty answer is a body with no faces.
- **Fresh draft, not `a.edit()`**: the result is mostly new faces and edges; lineage is
  `Origin::Operand(0|1, face)` for every face on the left of a result face's boundary.
  Edge and vertex lineage are Free.
- **Result plane** is `a`'s first face's frame (`b`'s when `a` has none); faces of `b` in the
  opposite frame are the same set.
- New errors: `Refused(NotCoplanarSheet)`, `Refused(BadProfile)`, `Unsupported(PlanarEdge)`,
  `Unsupported(PlanarUndecided)`.

Known limit: a result face whose hole touches its outer loop at a point (a disc tangent inside
a plate, subtracted) is not a `Region` and is `PlanarUndecided`; the answer needs a face with a
pinched loop, which `scx-planar` does not take. Two rectangles touching at a corner are fine; two discs tangent from outside are `PlanarUndecided` (open: not yet separated into the certified crossings of two tangent quarter arcs or the midpoint test).
Cost: all pairs of atoms with meeting boxes are crossed (quadratic in the worst case; the
perforated plate of 16 discs, about 80 atoms a side, runs in 0.3 s in debug).
### 13.10 BO21 as built (11-28)

`scx_boolean::boolean_given(a, b, op, &Incidences, cx) -> Result<Done<NoItems>, BooleanError>`
(`crates/scx-boolean/src/given.rs`); `boolean` is `boolean_given` with `Incidences::NONE`, one
pipeline (R1). `Fact` is `FaceOnFace { a, b }` (operand 0's face, operand 1's face) or
`EdgeOnFace { operand, edge, face }` (the face on the other operand); `Incidences` collects them
sorted and each once. A false fact is `Refused(BooleanRefusal::FalseFact { fact })`.
`scx_surface` gained `curve_on_surface(c, ce, s, se, res, budget, stop) -> Result<OnSurface, _>`,
`OnSurface::{Proved, Isolated, Leaves}`: `intersect_curve`'s answer, labelled by how it was had.

How a fact is used and checked:

- An edge fact (stated, or implied by a face fact: every edge of either face on the other's
  surface) sends stage 2's edge-face test through `curve_on_surface` in place of
  `intersect_curve`. On a plane the curve's height is bounded in closed form (a line's two ends;
  `|gamma| + hypot(alpha, beta)` on a conic; a few products on the open conics); within
  `tol - rounding` the answer is the overlap's clip with no root isolated, one budget unit against
  two a piece. Otherwise it is `intersect_curve`'s own isolation (`isolate`, the old body moved
  out unchanged).
- A stated edge fact is false when the roots show the edge leaving (`Leaves`), or when stage 2
  never pairs that edge and face and the same test, run on its own, says so; a fact on a missing
  edge or face, an edge with no curve, or a third operand is false too. An implied fact never
  refuses: one the bound does not prove is answered by the roots as without it.
- A face fact is false when stage 4 finds no coincidence of exactly those two faces. Stage 4
  still runs for that pair.

**Same result, the argument.** The only change a fact makes is `intersect_curve` replaced by
`curve_on_surface` for one edge and face. `intersect_curve` answers `Overlap` pieces
(`path.clip`) exactly when every event (roots, turns, the ends or the closed curve's probes) is
within `tol`, `Event::new` computing each distance in floats. A proof bounds the true height by
the computed bound plus its rounding, and each event's computed distance by the true height
plus its rounding; both roundings are a few steps on numbers no larger than the coordinates'
size `m` (curve, axes origin, surface size), and `rounding = 64 eps m` covers both with room. So
proved implies every event near, and the answer is the same `path.clip(c, target, se)` call,
the same bits. Not proved, it is the same function. Tested to the bit: `scx-surface`'s
`a_stated_curve_is_answered_as_intersect_curve_answers_it` (300 on-surface cases of every kind
at two tolerances, plane cases all proved, curved ones all isolated at equal work, and the
random pairs of 10 seeds) and `scx-boolean/tests/given.rs` (bodies compared by
`scx_record::encode`, changes compared, volumes against closed forms).

**A false fact cannot change a result silently.** A stated fact is judged by the very test that
answers without it, so a false one meets a disagreement and is refused, and no answer is built
from it. A proved fact is true by the bound. An implied fact is never trusted: unproved, it is
the plain test. So the result with facts is `boolean`'s result or a refusal.

Decisions (analysis and adversarial pass):

1. *A closed-form bound on planes only, not SU6's certified gap.* The first build used
   `curve_surface_gap` (any analytic kind). Measured, it took 5 points for a line on a plane
   against isolation's 2 units: the facts cost more work (387 against 366 on the stacked
   cubes). For: analytic isolation is itself a closed form (two Bernstein solves a piece), so no
   bound saves much and only a closed form undercuts it; the plane is where stated facts arise
   (mates, bosses, pockets). Against: curved coincidences (coaxial walls, BO22's tangent
   contacts) get no saving and no proof independent of the roots; BO22 can add the gap where
   isolation is ill-conditioned (a double root), where it is worth more points.
2. *Face facts are checked by running stage 4, not skipped.* For: the analytic pair is O(1) and
   decides the coincidence the build needs; trusting the fact would need the overlap's regions
   anyway. Against: a NURBS face pair would gain from skipping; that waits on BO23.
3. *Implied edge facts never refuse.* For: the caller stated the faces, not each edge; an edge
   of a coincident face that the bound misses (rounding near `tol`) is still on it as the roots
   say. Against: a face fact whose faces' edges leave each other's surface is only caught by
   stage 4, which does catch it.
4. *Unpaired stated edge facts are checked alone* (`judge_edges`, with stage 2's reach and
   resolution). For: else a fact on an edge far from the face is silently ignored. Against: one
   extra test per such fact (cheap; the NURBS case is `Unsupported` as stage 2 would be).
5. *Work may be less, never more*, so a budget that exhausts `boolean` may suffice for
   `boolean_given`; the result when both finish is the same.
6. *The rounding constant is an error-model argument* (`64 eps m`), not machine-checked; the
   test at 0.99 `tol` (proved) and 1.5 `tol` (leaves, as `intersect_curve`) brackets it.

Measured (throwaway prints and timers, removed; debug counts, release times):

| case | facts | edge-face isolations | work | time (release, us) |
|---|---|---|---|---|
| stacked cubes, union | one face pair | 41 -> 34 (7 proved) | 366 -> 359 | 229 -> 227 |
| stacked cubes, union | 8 edges | 41 -> 34 (7 proved in stage 2, 1 unpaired fact proved alone) | 366 -> 360 | 232 -> 236 |
| stacked cubes, subtract | 8 edges | 41 -> 34 (7 proved in stage 2, 1 unpaired fact proved alone) | 363 -> 357 | 175 -> 170 |
| boss on a plate | rim circle | 2 -> 1 (1 proved) | 38 -> 35 | 57 -> 56 |
| boss on a plate | disc face pair | 2 -> 1 (1 proved) | 38 -> 35 | 63 -> 63 |
| hole through the plate | 2 rims | 5 -> 3 (2 proved) | 75 -> 69 | 108 -> 108 |
| coaxial cylinders | wall pair, 2 rims | 12 -> 12 (curved: none) | 102 -> 102 | 156 -> 168 |

  Per call (release, 200,000 calls, line and circle on a tilted plane): `intersect_curve` 205-223 ns
  and 842-851 ns, `curve_on_surface` 65 ns and 100 ns. The Boolean's times move within the main
  box's noise (two runs differed by up to 25 % on one case): stage 2's isolations are a few
  microseconds of 60-240. So on analytic planes the facts save the roots, not the Boolean's time;
  they matter where the test is dear (BO22's tangent pairs, NURBS once BO23 lands).

Tests: `crates/scx-boolean/tests/given.rs` (10): a stated face pair, stated edges (union,
subtract, intersect), a boss on a plate (rim, disc, through hole), coaxial walls, false edge facts
(unpaired, crossing, beside true ones), false face facts (parallel, perpendicular), facts naming
what is not there, near the coincidence (0.25 and 0.9 zone judged as the test judges, 50 zones
refused), no facts is `boolean`, `Incidences` order. `crates/scx-surface/tests/intersect.rs` (+2):
the bit-for-bit comparison above and the line just off a plane. None removed.

Open: curved bounds (a cylinder's ruling and parallel have closed forms too) if a caller's
curved facts show up in profiles; BO22 reuses `OnSurface` and `Fact` for tangent pairs.

### 13.11 MK15 as built (11-28)

`scx_make::face(&Surface, &Bound, &Document) -> Result<Made<FaceId>, MakeError>` makes a
sheet of one face on a plane, cylinder, cone, sphere or torus; `Bound` is `Box(Extent2d)` or
`Region(Region)` (in uv). Code in `scx-make/src/face.rs`; tests in `tests/face.rs` (8).

- **The uv is the pin's, not the angle.** Every turned kind's `u` (and a sphere's or torus's
  `v`) is a rational quadratic parameter, one arc per quarter turn, so an iso circle's
  sweep is `Arcs::angle(u1) - Arcs::angle(u0)`, not `2 pi du`. The first build assumed
  proportional and the writer refused the loop (`LoopOpen`); the closed forms in the tests
  take their angles from the surface's own points (`atan2`, heights), not from `Arcs`.
- **Box.** Handles a whole period (the seam is one edge used twice), a pole (sphere
  `v = 0` or `1`, cone apex: a collapsed edge; snapped within 1e-12) and a torus's both
  ways, so the constructors' whole faces are boxes (`[0,1]x[0,1]` torus: 1 vertex, 2 edges).
- **Region.** On a plane it is `sheet` of the region (arcs exact). Elsewhere only lines
  along `u` or `v` are iso-lines, so any other segment, and any on a pole, is
  `Unsupported::Segment`; extent past a period or the domain is `Refusal::UvBox`.
  Case for also taking arcs in uv on a cylinder (helices, ellipses): not exact, so not
  built; they would need the fitted writer.
- **NURBS** is `Unsupported::NurbsFace` until MK15b.
- Errors added: `Refusal::UvBox`, `Unsupported::NurbsFace`.
- Not ported: the pin's face-reading tests (step 6) and NURBS faces (MK15b).

### 13.12 MK4 as built (11-28)

`scx_make::revolve(&Region, &Frame, Turn, &Document) -> Result<Made<RevolveFaces>, MakeError>` and
`scx_make::revolve_sheet(..) -> Result<Made<RevolveSheetFaces>, MakeError>` turn a line-and-arc
region in the frame's `x, y` plane about the frame's y axis. `Turn::{Full, Partial { from, to }}`
(radians, right-handed about y, zero at the region's plane). `RevolveFaces { start: Option<FaceId>,
end: Option<FaceId>, walls: Vec<Vec<Wall>> }`, walls `[loop][segment]` in the order given to
`Region::new`; `Wall::{OnAxis, Face(FaceId), Split { before, after }}`. Every wall is built
directly with exact pcurves: a line is a plane, a cylinder or a cone; an arc is a sphere (centre on
the axis), a ring torus, or a horn torus (its circle touches the axis); every surface's frame is
`(axis point at the piece's height, z = axis, x = the start radial)`, so the parallels are the
surface's u lines and each meridian its v line at `u = 0` or `u = sweep`.

Decisions, with cases for and against:

- **Touches by the zone, crossings refused.** A profile point within the resolution's zone of the
  axis is put on it; a segment whose both ends are on it is `Wall::OnAxis` and sweeps nothing; a
  vertex on it is a pole (a collapsed edge per cone, sphere or horn-torus face; a plane passes
  through it). A segment reaching more than the zone across the axis (its side-mapped radius
  minimum, an arc's interior included) is `Refused(CrossesAxis { at })`. For: the pin's meaning
  (it splits a touch and refuses a crossing) with no new predicate; the zone is what makes two
  points one in the finished body. Against (the adversarial row in 04 11-28): a touch within the
  zone of a crossing is decided by the zone, not by certified bounds or `Unproven`. The profile is
  exact data in a `Region` (lines and circles from `f64`s), so the only uncertainty is the rounding
  of a radius computation, many orders below the zone; `Unproven` would never fire. Recorded as a
  deviation.
- **Interior axis touch: split, or re-seam a whole circle.** An arc whose circle touches the axis
  at `T = (0, c.h)` with `T` inside the arc is split there into two pieces (`Wall::Split`, both
  horn-torus faces); a whole circle touching the axis is re-seamed to start at `T`, so it is one
  horn-torus face whose loop runs from the pole (WR-590's three-face partial turns and one-face
  full turn). An arc whose circle crosses the axis outside the arc (`|c.r - r| > zone` with
  `c.r < r`) is `Unsupported::SpindleTorus`: its surface is a spindle torus, which `scx-surface`
  does not have; the pin keeps a revolution surface there (not ported).
- **Horn torus support across crates.** `Torus::horn(frame, radius, sense)` (major = minor) and
  `Torus::pole() -> Option<Pole>` (`v = 1/2`, the centre). The pcurve writer, `check/geometry.rs`
  and the testkit oracle read the torus pole as they read the sphere's and cone's. The writer's
  pole-side rule changed from `side_v < pole.v` to `side_v < start.v`: the horn pole sits inside
  the torus's periodic v range (at `v = 1/2`, or `3/2` a period on), so only the pole use's own
  start v says which side of it the loop is on; for the sphere and cone, whose poles are at the
  ends of the v interval, the two rules agree. `scx_make::torus` still refuses a horn torus
  (`Unsupported::HornTorus`); the refusal is lifted by MK4b (13.33).
- **The plane snap rule at the mean height, not the start's.** A line whose ends are within the zone
  in height is a plane at their mean height; within the zone in radius, a cylinder at their mean
  radius. For the mean: each end is off by at most half the difference, not all of it, and the rule
  is symmetric in the segment's direction (a clockwise loop gives the same body). Against: 75 §3.6
  places it at the start vertex with the end's distance recorded as its accuracy; that distance is
  within the resolution, so `VertexRecord::accuracy` would be `None` either way. Deviation.
- **Orientation from the profile's side.** The side is `+1` when the region's far reach is on the
  frame's `+x`, else `-1`; each loop's walls are written for the solid on the left of the stored
  traversal and reversed (list and senses) for side `-1`. Each surface's sense follows from the
  piece's direction (`Forward` iff `o * dh > 0` for a cylinder or cone, iff the arc's turn agrees
  with `o` for a sphere or torus, the plane's normal against `o * sign(dr)`), so no orientation
  pass is needed.
- **A hole of a full turn is a void.** Its walls close on themselves apart from the outer loop's,
  so they get a void region's shell in front and a new shell of the solid behind
  (`Assembly::hollow`); a partial turn's caps join every loop into one shell. `revolve_sheet` puts
  each loop's walls in an outside shell of their own (`Assembly::sheet_shell`), since without caps
  the loops are pieces apart. `Assembly` also gained `sides` and `face_between`.
- **Caps** (partial turns) are planes through the axis, `z = D_from x Y` at the start and
  `Y x D_to` at the end, holding the region's loops at that angle.
- **Refused turns.** `Refusal::Turn { from, to }` for angles not finite, `to <= from`, a whole turn
  or more apart, or with the farthest point less than the zone from the other cap either way round.

Deviations from 75 §3.6 and its 04 row: the axis is the `Frame`'s y axis (no separate `Axis` type);
angles are `f64` radians (no `Angle` type in `scx-num`); caps are `Option<FaceId>` and walls a
`Wall` enum rather than `Vec<Vec<FaceId>>`, since an on-axis segment has no face and a split one
two; `revolve_sheet` takes a `Region` (closed loops), not an open path; touches by the zone, not
certified bounds; the plane snap at the mean height.

Measured (release, throwaway timers, best of 20 runs of 50, tilted frame): a 3 x 4 rectangle
revolved whole 21 us and a quarter turn 27 us (extruded: 13 us); a circle to a ring torus 9.7 us
against `scx_make::torus`'s 9.3 us; a half disc to a sphere 8.5 us against `sphere`'s 8.1 us;
WR-590's quarter turn 12 us. So a revolve costs what the matching primitive does, within 5%. The
cost added to existing paths is one comparison per pole lookup (`Torus::pole` in the writer and the
check) and the changed pole-side comparison; not measurable.

Tests (`crates/scx-make/tests/revolve.rs`, 16): a tube from a rectangle (plain and tilted frame,
full, quarter, 3.5 rad and almost-whole turns), the mirrored side, a solid cylinder (axis segment),
a cone to an apex and a frustum, a sphere from a half disc, a ring torus, a region with a round and
a square hole (voids for the full turn, genus 2 for partial), the plane and cylinder snaps, WR-590
(circle centre (5, 0) radius 5, quarter, half and three-quarter turns from 0, pi/2 and pi, and the
full turn), an arc split at its interior touch, the sheet, and the refusals (crossing by a line and
by an arc's bulge, spindle arc, eight bad turns); determinism; the given-order walls of a clockwise
loop. Each solid: Pappus's volume and area to 1e-9 with `scx_mass`'s estimate within it, face kinds
per segment, the closed-solid structure (fins, sides, exact pcurves, Euler-Poincare) and the BR8
oracle (the pinched WR-590 and split bodies: the oracle and exact pcurves, without the Euler count).
No pin test was removed. The pin's `issue_2905_revolved_vase` is a chamfer test on a captured
body, not a revolve test; its meridian (a circle of centre `(-16, 33)`, radius `sqrt 1690`, between
`z = 30` and `60`) is the far arc of a circle crossing the axis, an apple (spindle) torus, which this
revolve refuses as `Unsupported::SpindleTorus`. A real report needs it: the spindle surface is the
first open question.

### 13.13 SF1 as built (11-28)

`scx_surface::project_curve(&Curve, Extent1d, &Surface, along: UnitVec3, Resolution)` and `scx_surface::drop_curve(&Curve, Extent1d, &Surface, Resolution)` return `Result<Vec<Projected>, ProjectError>`. A `Projected { curve, extent, from, kept: Option<ParamShift> }` is one piece: the image curve over `extent`, the image of the source over `from`, run the same way, with `kept` the source-to-image parameter map where the image is an affine copy (`None` for the cylinder's arc, where only the ends and the order hold). `ProjectError` is `NeedsFitter { curve: CurveKind, surface: SurfaceKind }` (Unsupported, `projection_needs_fitter`: the one variant the step-5 fitter replaces), and the Refused `Along`, `Flattened`, `OnAxis`, plus `Image(CurveError)`. The module is `crates/scx-surface/src/project.rs`, with the closed-form table and the pin-vs-here table in its header.

Closed forms: onto a plane, every curve kind, through the projection affine and `Curve::transformed` (a circle to its exact ellipse on principal axes, a NURBS curve by its control points); onto a cylinder, a line parallel to the axis projected (0, 1 or 2 rulings) and a line dropped in a plane through the axis (a ruling per side) or square to the axis (an arc at its height); onto a cone, a line in a plane through the axis, projected along the axis or dropped (a generator per side). Every other pair is `NeedsFitter`.

Decisions.
- **The whole projection line, both signs.** For: one answer for a plane (the pin's SMLib projection is also unsigned), no split where the curve passes through the surface, and a caller wanting one side filters by the image's side. Against: a caller casting "down" onto a cylinder gets both rulings; the pieces are ordered by `s`, so the first is the near side.
- **A miss is `Ok(vec![])`, not an error.** For: a projection line that misses the surface is a final answer, not a failure (no failing default either way: the vector says what happened). Against: the pin's `Error::Empty` lets a caller `?` it; the face caller in `scx-bodyq` decides that, after its trim, where empty can also come from the trim.
- **No surface extent or uv box.** For: the analytic kinds are unbounded (but the cone's nappe, handled), and the face's trim belongs to `scx-bodyq` (75 SF1 row). Against: the pin's drop clipped to the face's uv box first; that is now the body's step.
- **A cone's apex part is left out of the drop**, not returned as a point: the feet there are the apex alone, not a curve. A piece whose image has no length is left out; when none is left the answer is `Flattened` (a line dropped square through a cylinder's axis, a circle seen edge-on).
- **Predicates within the zone over the extent** (parallel, meridian, square): R5, chosen before building, no fallback. Against: a line a hair outside the zone gets `NeedsFitter` though its image is nearly a line; the fitter answers it, which is what it is for.
- **`kept`.** An affine image keeps the parameter through a `ParamShift` (the plane's, rulings and generators: `NONE`), so a caller can map vertices; an arc's angle is not an affine function of the line's `t`, so it says `None` rather than a wrong map.

Truths (`crates/scx-surface/tests/project.rs`, 14 tests, plus 2 unit tests): a circle projected along z onto a plane tilted by beta is the ellipse with semi-axes r and r / cos(beta) about the centre's image; a tilted circle projected onto a level plane has semi-axes r and r cos(gamma); for 64 random circles, planes and directions the image keeps the conjugate-diameter invariants a^2 + b^2 = |u|^2 + |v|^2 and ab = |u x v|; a line dropped onto a cylinder in a plane through its axis is the ruling at the radius (and splits at the axis); a chord dropped onto a cylinder is the arc at its height with the closed-form ends. Every piece is sampled: each point on the surface and on the projection line through its source point, or equal to the kind's `closest` foot. Ported from the pin: `a_line_above_a_box_drops_to_its_foot_on_the_top`, `a_line_beside_a_cylinder_drops_onto_its_wall_at_the_radius`, the #20836 seam chord (its geometry; the seam split and the area are the body's), and `a_segment_projects_onto_a_box_s_record_face_at_its_index` (both levels). Not ported: `a_curve_over_nothing_of_the_face_is_refused` (a face concern; the analogue here is a miss being empty) and `rational_quarter_circle` (a record form, not geometry).
### 13.14 BR11 as built (11-28)

**What was built.** `scx-brep` `draft/heal.rs`: `Draft::merge_faces_across(edge, &protected, &doc)`
and `merge_faces`, `join_edges_at(vertex, &protected)` and `join_edges`, and `heal`
(faces, then edges, to a fixpoint). A protected set is a `&BTreeSet<EdgeId>`; an id that is not a
live edge is `Absent` before any write (the pin's out-of-range `Argument`). Reports are typed:
`FaceMerge { absorbed, survivor, removed }`, `EdgeJoin { absorbed, survivor, vertex }`, and
`Left { at, why }` for a candidate (equal surfaces or curves) that was left, `Why` one of
`Protected, Sides, Orientation, Loops, Pcurves(e), Fins, Interval, Uses`. Each merge is planned
read-only and then written, so a refusal writes nothing.

**Decisions, each with a case against.**

1. *Equal means `==`* on `Surface` and `Curve` (the test cells and `Changes` already use), not a
   geometric coincidence test. For: exact, no tolerance, and the pcurves carry over unchanged;
   pieces of one cut face or edge are equal. Against: two coplanar faces of different operands
   (different frames) are not merged; the Boolean's own merge, which asks the surface
   intersection, keeps doing that. A kernel-wide "same surface up to frame" test is open.
2. *A pair is merged by its twins* (every edge only the two faces bound), walking loops across
   them (the Boolean's splice). A connected group of mergeable faces (union-find over the
   removable twins) is merged at once, one loop walk a group, so a long strip is linear. A group
   goes pair by pair instead when a twin's two uses lie in different uv windows (a seam) or a
   protected, other-shell or one-sense edge lies inside it; the pair walk keeps seams and
   refuses what a group cannot. For: handles a disc in a plate (both loops vanish), collinear
   shared pieces and seams. Against: two code paths, and a dirty group is still quadratic.
   A first version merged pair by pair only: 17 s on the 4,102-face bar.
3. *Seams.* Twins are grouped by the uv offset between their two uses; the group in step with
   the first one in one window goes, the others stay as seams. A pair meeting across a seam only
   is written with `exact_loop`/`placed`. Against: windows are compared with 1e-7 in uv.
4. *Protected.* A protected edge is never removed and two faces sharing one never merge; two
   edges join only when both or neither are protected (the survivor is the lower id, the
   report names the absorbed one). Against: refusing every protected join would keep the
   `Id`s stable, but leaves a split line in many pieces and gives 16 edges where the pin gives
   15 (the pin's #21864 joins them too).
5. *Edge joins write no pcurve at a guess.* The two uses' pcurves must be equal Curve2 values
   running on (a turn on for circles and ellipses in uv); otherwise `Why::Uses`. Against: the
   wall of a cylinder whose seam lune has its window a turn off (`start` -2 pi) leaves its two
   circle arcs at the seam unjoined; closing that needs a uv shift of the rest of the loop
   (open, below). Rewriting with `write_pcurves` was rejected: it can fail after the edge is
   written.
6. *Joined accuracy* is the larger of the two, downgraded `Certified` to `Unproven` (a proof of
   each piece is not one of the whole); equal ones are kept.
7. *Survivor is the lower id*, so the result does not depend on which edge the walk met first.

**Tests (all in `tests/heal.rs`, 14).** Ported from the pin's `merge_tests.rs`: a split cube and
two unioned boxes (a lattice, 6 faces, 12 edges, 8 vertices, area `2(ab+bc+ca)`, volume `abc`,
the check passing), a box changes nothing and every planar generated kind changes nothing,
a picked edge keeps the faces apart (7, 15, 10), an unknown kept edge is refused before any
write, more than 4,096 kept edges (1,728 and 6,912 edges all protected), the 4,100-edge bar with
one kept cut. Not ported: `picks_near_nothing` and the point-list `MAX_POINTS` cap (ids are
typed, so neither failure exists); the stitch tests (`scx-stitch`). New: a disc in a plate, a
cylinder's wall, sides refused, one merge and one join by name, parts equal the whole.

**Measured** (a 24-core build box, unoptimised, whole test including the build and the check):
the 4,102-face bar, pair by pair 17.1 s, by group 4.2 s; the 20 x 20 x 20 lattice (2,400
faces, heal and parts) 0.25 s before, 0.21 s after. What is left of the bar's 4.2 s is the
edge joins: the bar has four 1,025-piece cube edges, and each of its ~4,100 joins rebuilds the
loops of its faces (clone and fin diff of a loop of about 1,000 uses). Joining a whole chain
at once would remove it.

**Open.** (a) join a chain of edges at once, linear in the chain; (b) the seam-window join
above (a generated cylinder's circles stay split at the seam lune, `Why::Uses`; for BO12 to
close with a uv shift of the rest of the loop); (c) merging coplanar faces of different frames (needs the intersection test of
`scx-ssi`, above this crate); (d) the Boolean's `build.rs` still has its own merge and join,
to be replaced by these calls (BO12).

### 13.15 RP1 as built (11-28)

**Built.** `crates/scx-repair`: `heal(&Brep, OnItemFailure, &mut Cx) -> Result<Done<HealCause>,
RepairError>` and `items(&Brep) -> Vec<Item>`. The one item kind so far is `Item::Cluster(Cluster
{ vertices, edges, survivor, reach, target })`. `RepairError` is 01 §5.1's shape: `Cause`, `Items`
(collect mode only), `Exhausted { item, work }`, `Cancelled`, `Defect`. Its `RepairCause` is
`OtherDocument` or `Heal(HealCause)`, and RP2-RP8 add their own causes beside `Heal`. There are 12
`HealCause` variants, each naming its entity: 6 refused (`Spread`, `TooLong`, `OffFace`,
`EndOffVertex`, `NotClear`, `NeighbourShort`) and 6 unsupported (`NeighbourKind`, `FaceKind`,
`Sliver`, `LoopVanishes`, `Pinch`, `NotFinite`). `scx-check` gains `Finding::VerticesWithinCoincidence
{ a, b, distance, allowed }` and its `Undecided` twin (research 74 §3.3's pair test, which `check`
did not have). These are found by binning the vertices in cubes of twice the widest reach and
testing neighbouring cubes, after the gaps and on the budget they leave; a pair the budget does not
reach is `Undecided { why: Exhausted }`, as a gap is (74 R6).

**How a cluster heals.**
- Vertices joined by a curve edge are clustered by union-find when their distance is certified
  within the larger reach.
- The survivor is the vertex of the largest reach (the lowest id on a tie). The target is the
  centre of the cluster's box, as in the pin.
- Edges with both ends in the cluster are squeezed: removed with their uses. Every point of the
  edge's curve must be within the survivor's reach of the target, enclosed over 16 interval pieces.
- Every other edge at the cluster must be a line on plane faces. It is rebuilt between its new ends
  over `[0, 1]`, and its pcurves are the line's projection.
- Each rebuilt edge is certified against the check's own allowances:
  - it is longer than its reach;
  - its end is at its vertex within `re + rv`;
  - the lift of each pcurve is within `min(re, rb)` of the line.
- The target is clear of every other vertex by more than the larger reach. The test uses the
  same cube binning, plus the targets of the items already done.
- No loop is left empty, with two or fewer lines, or passing the survivor twice.

**Choices, with the case for and against.**

1. **The pair finding covers every vertex pair, not only pairs an edge joins.**
   - For: it is the defect itself, and 74 §3.3 asks it of the whole body. A heal fixes what the
     check reports, so the finding must exist before the heal can be held to it.
   - Against: heal merges only pairs an edge joins. A pair with no edge between them stays a
     finding until a later item kind handles it (open question 1). The check now costs a sweep;
     see the measurements below.
8. **Cubes, not a sort along x.**
   - For: CAD bodies are axis-aligned. A block's faces put whole grids of vertices in one x
     slice, and the first build (an x-sorted window) tested those pair by pair: 3.33M units and
     139 ms at 9,602 vertices, 56% of the gaps' time. Cubes test only points within two sides of
     each other, and a pair within the reach lies in adjacent cubes, whatever the rounding.
   - Against: a body with one wide vertex makes the side large, and that brings back pair tests
     near it (still correct). Past 2^52 sides a body falls back to one cube, which tests every
     pair.
9. **Clearance after the gaps, out of budget = undecided.**
   - For: the check's contract is that a budget past the structure part never errors and never
     gives a false finding (`gaps.rs` and `structure.rs` tests). The first build charged
     clearance as a hard `Exhausted` before the gaps, and those tests failed.
   - Against: a pair left undecided for budget carries the bracket `[0, inf)`, which says
     nothing. A caller who wants the answer gives more budget.
2. **`heal` takes `OnItemFailure { Stop, Collect, Skip }` and has no rounds.** 75 §6.5 wrote
   `heal(body, cx)` with no mode.
   - For: 01 §5.2 makes the mode explicit and the default all-or-nothing. Each item is certified
     against the items done before it, so dropping a failing item cannot change a later item's
     answer. One pass is the fixpoint, and every `Skipped` is round 1.
   - Against: a heal that could only succeed after a later item lands (for example, a sliver that
     a later collapse removes) is not retried. RP3 may need rounds back, as a cap the mode states.
3. **No GAP_SHARE. The bound is `min(re, rb)`.**
   - For: the check's two use questions are "edge on face within `re`" and "use follows edge within
     `rb`". The pcurve is the exact projection, so one lift gap answers both. A heal certified this
     way passes the check by construction, with no factor to tune. The pin's 0.5 was a share of one
     tolerance.
   - Against: the bound is tighter than the pin's when `rb < re / 2`, so some clusters the pin heals
     are refused here. That is correct by the check, and no case in the tests needs it.
4. **Certifying the two ends certifies the whole line.**
   - For: the lift of a plane's projected line differs from the line by an affine function of the
     parameter, so its norm is largest at an end. The two ends are enclosed in intervals.
   - Against: this holds only for lines on planes. Curved neighbours need SU6's gaps (RP2), and are
     `Unsupported(NeighbourKind)` / `(FaceKind)` until then.
5. **The survivor keeps its accuracy, and face planes are kept rather than refit.**
   - For: "no accuracy grew" is the truth, and moving a face would need its other edges
     re-certified.
   - Against: a cluster whose re-ended edges leave a face by more than their reach is refused
     (`OffFace`). It is not healed by widening, which the pin's SMLib healer would do.
6. **A re-ended edge between two clusters is re-certified by the later one with both new ends.**
   - For: the plan stays per item, and the later certificate is about the final line.
   - Against: if the later item fails, the edge keeps the earlier item's line. That is still
     certified, since it was certified with the earlier ends.
7. **Edges.** repair → brep, curve, num, surface (normal dependencies); check, mass and testkit are
   dev-dependencies only.
   - For: RP1 code reads no finding at run time. The items are found from the body by the same
     test the check makes, and the tests hold each heal to `scx_check::check`.
   - Against: 75 §7 drew repair → check. RP8 or a caller that heals from findings would add it
     back.

**Tests.**
- `crates/scx-repair/tests/heal.rs`: 8 tests on constructed polyhedra.
  - The hip roof (ridge δ = 1e-4) heals to the square pyramid: 5/8/5. The check passes after; no
    accuracy grew; `|V − a²h/3| ≤ area × largest bound`. With the accuracy at δ/10 it has no item
    and is left unchanged. With the slanted edges at zone accuracy it is refused `OffFace`.
  - The chamfered prism heals its bottom pair and leaves its top pair, which would be a sliver.
    Stop, Collect and Skip each return their own answer.
  - Two chamfers share an edge between clusters.
  - Budget exhaustion is typed at its item, and so is the stop flag.
  - A body from another document is refused.
  - The heal is deterministic.
- `crates/scx-check/tests/clearance.rs`: 3 tests, at 10d, d and d/10 against the block's own
  distances, plus exhaustion. No test was removed.

**Measured.** These are throwaway timers (release build, not committed).
- `check` on testkit blocks, n = 5 / 10 / 20 / 40 (V = 152 / 602 / 2,402 / 9,602):
  - clearance adds V work units: 152 / 602 / 2,402 / 9,602, against 6,900 / 27,600 / 110,400 /
    441,600 before;
  - it takes 63-90 us / 0.30 ms / 1.3 ms / 6.0 ms, against the gaps' 3.9 / 15.4 / 61.3 / 244 ms
    (+1.5-2.5%).
  - The x-sorted first build took 2,172 / 22,142 / 256,482 / 3,329,762 units and 0.09 / 0.95 /
    10.8 / 139 ms.
- `heal` (Skip mode):
  - hip roof: 59 units, 11.5 us;
  - one chamfer: 115 units, 17.2 us;
  - two chamfers: 215 units, 29.8 us.
  - The check of the same bodies takes 116 / 207 / 244 us, so a heal costs about a tenth of one
    check. The pin runs two full checks per heal (75 §6.4).

**Open.**
1. Vertex pairs within coincidence that no edge joins: merging them changes topology, not edge
   ends. This is a later item kind, or ST1's identification.
2. Moving an edge can make it cross another face. No tube-clearance test is made, since 74 §3.3
   defines none, and the sweep covers only vertex pairs.
3. The cube grid is built per call, once in `check` and once in `heal`. A memo in `Scratch`
   could share it when a heal is followed by its check.

### 13.16 CN1 as built (11-28)

`CompatibleCurves::<D>::from_curves(&[NurbsCurve<D>]) -> Result<Self, NurbsError>` in `crates/scx-nurbs/src/compatible.rs` (77 §4.3, R77 row 4). Steps: each curve's knots are mapped affinely onto the first curve's domain (end knots exactly the first's ends, interior knots clamped inside; a curve already on that domain is borrowed, not copied); every curve is elevated to the highest degree with `NurbsCurve::elevate`; the union of interior knots (largest multiplicity over the curves after elevation) is inserted into each curve; if any input is rational the polynomial ones get unit weights. Inputs are `&`, the result is new, nothing is half done on an error. Inputs that already share a knot vector come back bit for bit. One new cause, `NurbsError::NoCurves` (Refused), for an empty slice; everything else refused is an existing cause: `Unclamped` (work needed and an end knot not of full multiplicity), `Unstable` or `Degree` (from elevation), `Multiplicity` (the mapping rounded two knots together).

Decisions.
- **No snapping.** Knots `0.5` and `0.5 + 1e-12` stay two knots (tested). For: the pin's averaging moves geometry unmeasured. Against: more knots on lofts of nearly aligned sections (77 §9 expects it).
- **Weights by homogeneous insertion and elevation**, not by a weight solve: exact up to rounding for any positive weights, so no rational case is refused for its weights. Polynomial curves among rational ones get weights of exactly 1.
- **Unclamped curves are refused when work is needed.** Insertion cannot reach the ends of an unclamped vector, so the knots could never be equal. Against: a surface net with unclamped rows that differ is refused; none exists in the corpus.
- **Per-knot insertion** (`insert_knot` copies the polygon per knot, O(k n)). For: reuses the checked single-curve edit. Against: a batch insertion would be O(n + k); not measured as a cost until MK10 lofts need it.

Work counts (the test family: 3 curves, degrees 1-5, mixed knot kinds and domains, summed over curves and families): knots in 1630, out 3819; control points in 1049, out 3096. The growth is elevation (each interior knot gains `q - p` copies) plus the union.

Tests (`compatible::tests`, 4): 1,000 parameters per curve of every sampled family evaluate as the source at the relabelled parameter to `1e-9 (1 + |x|)`, with knot vectors and degrees equal; unchanged inputs come back bit for bit; near knots stay two and polynomial curves get unit weights; the empty and unclamped refusals.

### 13.17 FT2 as built (11-28)

`scx_nurbs::fit` (77 §2, row FT2): `Param` and `parameters`; `averaging` and `approximation` knot
vectors; `interpolate` / `interpolate_on` at given parameters with `End` conditions; `least_squares`
and `weighted_least_squares`; one banded solver. Typed `FitError` (cause per variant, `kind()`,
`name()`); no flags, setters, out-params or failing defaults.

- **`End<N>`** is `Point` (G0), `Tangent{d1}` (G1), `Curvature{d1,d2}` (G2), `Hermite{d1,d2,d3}`
  (G3): parametric derivatives at the end, which the caller derives from a unit direction, a speed
  and a curvature. Every condition is stated; the pin's estimate of a missing end derivative from
  the data is not ported. Points are `[f64; N]` for any `N`, so a homogeneous fit is `N = D + 1`
  and `Spline::into_curve` / `NurbsCurve::from_homogeneous` finish it (replaces `FitPoint`).
- **Knots by averaging** use one rule for every pair of end orders `(a, b)`: the interior knots are
  the means of `p` consecutive entries of the parameters with the first repeated `a` more times and
  the last `b` more. It reproduces eq. 9.8 for `(0,0)` and the book's end-derivative knots for
  `(1,1)` (tested against hand values); the pin has five routines for these cases. Needs `a, b < p`.
- **Banded solve**: Givens rotations row by row (`band.rs`, private), `p + 1` entries per row of
  `R`, any number of right-hand sides at once. A square system (interpolation) and a rectangular one
  (least squares) use the same code; no pivoting, and no normal equations, so a fit's condition
  number is not squared (the pin forms `N^T N` and a dense LU). A pivot below
  `columns * EPSILON * max entry` is `Singular { column }` (a span without data, fewer data than
  unknowns): the named tolerance replaces the pin's `LU_TOL`.
- **Least squares** holds the end point and `order` derivatives by pinning the first and last
  `order + 1` control points (the `k`-th derivative at an end depends on `P[0..=k]` with a nonzero
  last coefficient), then solves for the rest.
- **Not ported** (decided): first derivatives at every point and the closed-curve conditions (CN3
  and MK12's), LU reuse between solves (`cpts_with_matrix`; a surface fit pushes its rows again),
  `square_to_plane`, and `approx_end_directions` (direction held, reach free). The last is FT3's:
  it needs the unknown reaches as two extra shared columns of the solve and is only meaningful with
  the certificate; the band solver takes them as a Schur step.

Tests (truth is the closed form, not the pin): samples of a polynomial of degree `d <= p` (p 1-6)
interpolate back to 2e-11 (relative to 1 + |value|) at 200 parameters, with and without its own end
derivatives; random data with every end combination up to G3 meets `C'`, `C''`, `C'''` to 1e-8
and passes through every point to 1e-10; least squares of exact data from a random spline on its
own knots gives the control points to 1e-10 (weighted, any positive weights: 1e-9), also with
Hermite ends held, and with perturbed data the held ends are still exact; the fit is a minimum
(200 random perturbations never lower the residual); a polynomial lies in the approximation-knot
space and is recovered; a homogeneous quarter circle; typed refusals; and a 20 000-point
interpolation (a dense matrix would be 3.2 GB).

**Cost** (throwaway counter and timer, release build, interpolation of `n + 1` points in 3D,
rotation-entry operations counted as `width + N` per rotation; nothing committed):

| p | n | rotations x (p+1+3) | per point | time |
|---|---|---|---|---|
| 3 | 100 / 1 600 / 25 600 | 791 / 13 454 / 212 065 | 7.9 / 8.4 / 8.3 | 32 us / 0.41 ms / 4.0 ms |
| 5 | 100 / 1 600 / 25 600 | 1 971 / 30 726 / 495 558 | 19.7 / 19.2 / 19.4 | 18 us / 0.27 ms / 5.4 ms |

Linear in the number of points (per-point work constant to 5% over a 256x range, and about
`p + 1` times its row width); a dense LU of the same system is `n^3/3 = 5.6e12` operations at
`n = 25 600` and `n^2` storage. Least squares adds the same per data row, plus the `O(p)` pinned
points. The pin's interpolation is dense (`RealMatrix`); its least squares is banded but through
normal equations.

For: one solver, no squared conditioning, linear time, every end condition linear and exact.
Against: the knot rule for `a >= p` is refused (a G3 end needs `p >= 4` by averaging; explicit knots
via `interpolate_on` accept `order <= p`); rotations cost about twice the flops of a Cholesky on
the normal equations at equal `n` (not measured against the pin; the factor is in `p`, not `n`).


### 13.18 MK10c as built (11-28)

MK10's certificate module on its own, without the loft: `scx_surface::certificate`. The public API:
- `Proof<P>`, one of:
  - `Proved { bound }`;
  - `Refuted { at, by }`, where `by` is a proved lower bound of the witness's distance and lies above the tolerance;
  - `Undecided { bound: Option<Length> }`, the bound reached.
- `on_analytic(&NurbsSurface<3>, &Analytic, Accuracy, &mut Budget, &Stop) -> Result<Proof<Point2>, SurfaceError>`, with `Analytic::{Plane, Cylinder, Cone, Sphere, Torus}` built by `Analytic::of(&Surface)`.
- `on_circle(&NurbsCurve<3>, &Circle, Accuracy, &mut Budget, &Stop) -> Result<Proof<f64>, _>`.
- `turn_about_axis(&NurbsCurve<3>, Point3, UnitVec3, Extent1d) -> Result<Turn, _>`, where `Turn` is `Positive`, `Negative` or `Undecided`.
- `span_bound(&NurbsSurface<3>, &NurbsCurve<3>, Node, Node, Accuracy, &mut Budget, &Stop) -> Result<SpanBound, _>`. `Node { t, uv }`; `SpanBound` is `Bounded(Length)` or `Unbounded`.

SU7's recognition certificate moved here from `recognize.rs`, which now calls the shared `prove`. RP7 gets a typed verdict for every kind, with no failing default.

Decisions.
- **Home crate: `scx-surface`, not `scx-make` as 01 had it.**
  - For:
    - The surface certificate is SU7's, already in `scx-surface`. It needs the analytic kinds, their axes and NURBS inversion, all of which live there. Putting it in `scx-make` would either duplicate it or make `scx-surface` call upward.
    - RP7 (`scx-repair`) and MK11's patch certification both use it. `scx-repair` sits above `scx-make` in the layers anyway, but nothing that only needs the certificate should have to pull in the constructors.
  - Against:
    - 01 named `scx-make`.
    - `scx-surface` gains about 1,900 lines (about 1,500 moved or new beside its tests) and so builds slower.
    - `span_bound`'s one caller, the loft (MK10), stays in `scx-make`.
  - The layers decide it: `span_bound` reads only a NURBS surface and curve, inversion and Bernstein forms. 01's `nurbs` row is amended.
- **R6 limits:**
  - `on_analytic` stops at 2^16 pieces in all;
  - `on_circle` at 2^12 pieces and 40 halvings of a span;
  - `span_bound` at 2^16 segments and 12 halvings.

  Past a limit the result is `Undecided` (or, for `span_bound`, the bound reached), never a refusal. Each piece charges one unit of the budget and polls the stop.
- **Refutation is rigorous.**
  - On a surface, a corner whose bound is past the tolerance becomes a witness only when its proved floor distance, less the corner's coordinate slack, is above the tolerance. The cone's floor uses both nappes: `|rho - |s|| cos`.
  - On a curve, a witness is a point read at a span's ends or middle, its distance less the evaluation's rounding.
  - The truth tests check that `by` is at most the closed-form distance at `at`.
- **No early exit for recognition.** `recognize`'s certificate no longer exits early on an unprovable piece. It now halves until each piece is proved, refuted or at a limit. A piece whose sampled distance is already within 1/64 of the tolerance is not halved (the pin's `NEAR`).

Deviations from the pin.
- `span_bound` returns the finite bound reached past its segment limit; the pin returned infinity. `Unbounded` is kept only for a piece with no bound at all: a weight not proved positive, or a diagonal of degree above 52.
- The pin's "sliver" term is replaced by a plain norm bound: `sqrt` of the sum of each coordinate's `max |N/D|`.
- `on_circle` bounds the distance from the circle itself, `hypot(|F/W^2| / (r + foot), |Z/W|)`. The pin ran two separate tests, one for the radius and one for the plane.
- `turn_about_axis` takes a `UnitVec3` and an `Extent1d`, where the pin refused a reversed interval at run time. It has no budget, since it makes one pass with no halving.
- New error `SurfaceError::OffRail { at }` (Refused) for a node outside the rail's domain.
- Not ported: `meridian_collapse` and `axis_foot`, which belong to step 8's blends.

Truths. There are 10 tests in `certificate/tests.rs`, judged against closed forms.

`on_circle`:
- The pin's `certificate.rs:1658` (an exact arc is proved, and is undecided at tolerance zero).
- `:1678`:
  - A 1e-4 bulge is refuted at 1e-7 with `1e-7 < by <=` its true distance.
  - It is proved at 1e-3 with a bound at least the largest of 1,001 samples.
  - Another radius, centre or plane is refuted.
- `:1709` (an arc 1e4 from the origin is proved at 1e-9).
- The budget is charged and the stop polled.

`turn_about_axis`:
- `:1575`: a quarter circle turns one way; it turns the other way about the opposite axis or reversed.
- `:1601`: a curve that turns back, or a radial line, is undecided; the part before the turn is proved.

`on_analytic`:
- The exact image of every kind is proved within 1e-9. This covers the plane, both senses of the cylinder and cone, the sphere and the torus, all in a tilted frame.
- A cylinder 0.01 wider is refuted with `by <= 0.01`.
- A cylinder with an interior control point moved out 1e-3:
  - At 1e-6 it is refuted, with `by` at most the closed-form distance at the witness.
  - At 1e-2 it is proved, with a bound at least the largest of 41 x 41 samples.

`span_bound`, on a cylinder of radius 10:
- A rail circle of radius 10.5 gives a bound in [0.5, 0.5005].
- A rail on the cylinder gives 2.2e-13.
- A free cubic rail's bound is at least its largest sampled distance.
- Nodes off either domain are refused with a typed error.

Costs, release build on the build server, timed with throwaway timers:
- **`span_bound`:**
  - The on-surface rail takes 1 segment in 6.7 us.
  - The free cubic rail takes 7 segments in 0.48 ms, about 70 us a segment.
  - The off-surface rail at a tolerance 5e-4 above the exact distance 0.5 takes 1,403 segments in 48 ms, about 34 us a segment. Each segment is one inversion of the rail's middle plus a diagonal of degree `du + dv` and its `max_ratio`. The segment count grows as the tolerance nears the true distance. MK10's caller should pass its loft tolerance, not the distance it expects.
- **`recognize`:** the suite takes 10 to 12 ms on base 7c1b668 and 10 ms on the branch. No change is measurable on its tests: the halving that replaced the early exit is bounded by `NEAR` and the piece limit.

### 13.20 BR11b as built (11-28)

**What was built.** `join_edges` and `heal` (`scx-brep` `draft/heal.rs`) join a whole chain of
edges on one curve at once: a maximal run through free vertices (each the end of exactly two
joinable edges, both or neither protected) is planned as one join, so each affected loop is
rebuilt once per chain, not once per pair. `join_edges_at` is unchanged and is the primitive:
a vertex that is in no chain, a chain of one pair, a closed ring, and a chain with any
refusal (`Why::Uses` for pcurves that are not equal and running on, or a use of a chain edge
outside a whole run of the chain) go pair by pair, in the order of the vertices, exactly as
before, so the refusals are reported as before.

**Decisions, each with a case against.**

1. *The pair check is one function (`link`) used by the single join and the chain walk*, so a
   chain is exactly the pairs a single join would accept. For: no second set of rules. Against:
   the chain walk evaluates each link twice (walk, then plan), which is linear and small next to
   the loops.
2. *A loop's run must cover the whole chain* (forward in order, reversed in the other), else
   the chain falls back to the pair path. For: it cannot accept what the pair path would refuse
   (a use pair of two edges but not the third). Against: a loop that legitimately holds part of
   a chain (not possible at a free vertex, where the only two edges are the loop's way through)
   would be refused the fast path.
3. *Survivor is the lowest id of the chain*, as the single joins leave it (each takes the
   lower); accuracy is folded in chain order, the interval by the same `run_on` from the left.
   Against: the fold order differs from the vertex order the pair path takes, so a tie in
   accuracy between unequal records could pick another record; no test finds one.
4. *Reports*: one `EdgeJoin` per vertex of the chain, naming the edge on the far side from the
   survivor, in chain order; the single joins name the pair's higher id in vertex order, so
   `absorbed` of a middle pair may differ while the set of absorbed edges is the same.

**Tests** (`tests/heal.rs`, one new, 15 in all): `joining_a_chain_at_once_equals_the_single_joins`
joins by `join_edges` and by `join_edges_at` at every vertex in order, on four lattices (with
and without the protected line) and five cylinders, and requires the same number of joins and
the same dump of every face, edge (geometry and fins) and vertex record. All 14 earlier tests
pass unchanged.

**Measured** (a 24-core box, unoptimised, whole test including build and check; a throwaway
counter of loops rebuilt and their uses, not committed): the 4,102-face bar 6.2 s to 0.36 s (4.2 s
on a quieter run before), loops rebuilt 8,188 to 12, uses copied 7,375,455 to 13,383; the 20 x
20 x 20 lattice's `heal` 0.30 s to 0.25 s, loops 456 to 24, uses 19,380 to 1,236.

**Open.** The cylinder wall's face merge stays pairwise across the seam (not a natural part of
the chain; a seam group needs the uv-window shift of 13.14 (b)). A ring of edges (a circle in
pieces) still joins pair by pair, quadratic in its pieces.

### 13.19 FT1 as built (11-28)

`scx_nurbs::deviation` (77 §2.3, row FT1): `distance` and `distance_by_span` (`SpanDistance`) for the
same-parameter distance of two curves, `distance_floor`, and `interpolation_bound`; typed
`DeviationError` (cause per variant, `kind()`, `name()`), generic over the dimension `D`. Not
ported: `certified_hausdorff` (no row names it). The pin's 20 tests: 12 port as they stand (the
numbers-not-finite, overflow and underflow ones change their cause types), the 4 `certified_hausdorff`
tests and the clamped/degree-cap refusals go (a built `NurbsCurve` is finite, degree-checked, and
needs no clamped knots here; the rational-refusal test is replaced by rational tests below).

- **Bezier pieces by the blossom, not by inserting knots.** Each common span's piece of each curve
  is read from that span's `p + 1` local control points by de Boor's recurrence with the arguments
  `s`, `e` (every control point `p` convex combinations deep). The pin refines a copy of the curve
  at the other's breakpoints and extracts the pieces, charging every span for all the insertions
  (`gamma_(12 (I+1))`, `I` the whole curve's inserted knots). Here a point depends on its own span
  only, `gamma_(12 p)` of `2 M`: a span's bound does not grow with the knot count. A refinement of a
  curve against itself has bound at most `2 x floor` (test), and no clamped end is needed.
- **Rational curves** (new; the pin refuses them), through homogeneous points `(w P, w)`, the other
  curve with unit weights if it is polynomial. With numerators `A`, `B` and weights `Wa`, `Wb` of a
  span's two pieces, `a - b = (A Wb - B Wa) / (Wa Wb)`; the numerator is a Bezier piece of degree
  `pa + pb` with control points `n_k = sum_{i+j=k} C(pa,i) C(pb,j) / C(pa+pb,k) (A_i wb_j - B_j wa_i)`,
  so `|a - b| <= max |n_k| / (min wa * min wb)` (a Bezier weight function is a convex combination of its
  weights). No elevation. Rounding: each homogeneous coordinate carries `e` (blossom plus frame), the
  exact numerators differ from the computed by `4 s e + 2 s^2 gamma_(terms+10)`, `s` the net size plus
  `e`; the least weights are lowered by `e`, and a weight within `e` of zero is
  `WeightsLost`. Against: not the polynomial bound's tightness (below); a rational pair with
  identical weights could use the cheaper convex-combination bound `max |P_i - Q_i|`, not done since
  after a refinement or elevation weights agree only to rounding.
- **Floor** is polynomial only (`RationalFloor` otherwise): a rational bound divides by the second
  curve's weights, so no number is below every bound. The floor forms `a`'s errors at its own degree
  and the other's at degree 1 (the least), by the operations the bound uses, so no span's bound is
  below it to the bit; self-comparison is then above the floor by the degree term, not equal to it
  (the pin's equality held because its error did not depend on `p`).
- **Pin bound not certified, fixed or noted.** (1) The pin's `interpolation_bound` added a count
  `gamma_(12 p + 4)` that belonged to its own evaluation routine; here evaluation is the blossom the
  bound's own count describes, so count and code agree. (2) The pin's `size` for the evaluation is the
  largest coordinate in absolute position; kept (no frame move, the data points are in it). (3)
  the pin's norm factor `2` stands for `sqrt(3)`; here `ceil(sqrt(D))`, so any `D` holds. No case
  where the pin's polynomial bound was below the truth was found (a sampled search over 6 pairs and
  hundreds of spans); the changes are in how the rounding is counted.
- **Tests** (21): self, refinement, elevation (bound at least the floor, at most twice it, below
  1e-12); a moved control point bounded by the move and above the 2 000-point sampled distance; other
  degrees and knots on common pieces; a periodic (unclamped) curve's ends by the general basis;
  interpolation misses (0.5 exactly to 1e-12); non-finite parameters and points; overflow by name;
  the 1e-200 underflow case, both calls; size not place (1e9); other domains refused; the largest span
  is the distance bit for bit; spans are the breakpoint union in order; each span holds its sampled
  gaps (6 pairs); no bound below the floor, and the floor follows size; spans outside a moved point's
  support at rounding level; the circle (below); a rational curve against its refinement and
  elevations (< 1e-12); a moved rational point; weights near zero refused; spans count linear; error
  names and kinds.
- **Circle** (closed form): quarter circle as quadratic rational against the cubic through it at four
  parameters. Radial error `max ||C(t)| - 1| = 8.91e-3`; the sampled same-parameter distance is
  `9.31e-3`; the bound is `4.58e-2`, 5.1 times the radial error and 4.9 times the true distance, and
  at least both (asserted at most 8 times). The looseness is the degree-5 control polygon and the
  weights (least 0.707); subdividing the span would tighten it (not done: one call, one bound).

**Work per call** (release build, throwaway timer, cubic against a cubic with one moved point,
`n + 3` points): 100 / 1 000 / 10 000 / 100 000 spans take 0.13 / 1.24 / 12.7 / 121 ms, 1.2-1.3 us
per span at every size: linear in the spans. The polynomial per-span work is `(p + 1)` blossoms of
`p (p + 1) / 2` combinations (`O(p^3)`), plus one elevation per piece when the degrees differ. The pin
extracts after a refinement of a copy of the curve (`O(spans x points)` for the insertions).

**Tightness** (bound against the 20 000-point sampled distance, polynomial): moved control point
1.12, quadratic against cubic 1.11, a quartic pair 1.0000, a cubic against its elevation 1.08
(a moved point). Against itself or a refinement the bound is the rounding floor: 1.6e-13 on a net of
size 5 (sampled distance 1e-15).

Open: whether the rational bound should subdivide a span while the bound exceeds a caller's aim
(adaptive, so FT3 gets the tightness it needs on rational sources, at the cost of a variable work per
call); whether `certified_hausdorff` is wanted (CN4's `through`, PK1's net rounding use the same-parameter
distance only).

### 13.21 BO13b as built (11-28)

`scx_boolean::imprint_curves(body, face, &[Cut], cx) -> Result<Imprinted, BooleanError>`
(`imprint_curves.rs`; tests in `tests/imprint_curves.rs`). `Cut::whole(curve)` or
`Cut::over(curve, interval)`; `Imprinted { done: Done<NoItems>, regions: Vec<CutRegion> }`, a
`CutRegion` being a face of the result and a point inside it. The curves are read in the
plane of the face as lines and circles (arcs), each proved on the surface by
`scx_surface::curve_on_surface` (`Leaves` is `Refused(Cut(OffFace))`), crossed with the face
boundary and with each other by `scx_planar::crossings` (moved onto both curves by BO17's
`polish`), and the meets within coincidence are one point. Each curve is kept between its
consecutive points where its middle is inside the face (`Region::classify`). The kept pieces
are `Section`s over synthetic merged points (the face's vertex, the edges through the point
with their parameter, or the face), run through the same stage 5 as `imprint` with an empty
second operand, then `finish_target` (the cleanup `imprint` already had, now shared). So edge
cutting in the neighbouring faces, exact pcurves, face tracing, holes and lineage are not
rewritten; a section with no tool face derives from the face only.

Errors: `Refused(Cut(NoCurves | NoFace | OffFace | Misses | Overlap | Dangles))`,
`Unsupported(Cut(Surface | Curve))`.

Decisions, with cases for and against:

- **`&[Cut]` where 75 wrote `&[Curve]`.** A line has no ends and a projected curve (SF1) is
  a bounded piece, so the interval is part of the input. For: BO16b passes projections
  unchanged and a whole curve is still `Cut::whole`. Against: a second type. `Report<C>`
  has no item list, so the regions ride in `Imprinted`, not in `Done<Regions>`.
- **Planar faces only, lines and circles only.** For: every crossing is certified and the
  pcurves are exact, as BO17; the pin's own cases are planar sheets. Against: a circle on a
  cylinder wall has an exact pcurve and is common; it needs curve-curve crossing in the
  wall's uv, which waits for the step 5 intersector. Both are typed `Unsupported`, never a
  wrong answer; ellipses wait on BO17's arcs being widened.
- **Stage 5 reused with synthetic points, not a new tracer.** For: no second copy of edge
  splitting across faces, pcurve writing or hole placement; the face cut is the one
  `imprint` writes. Against: a fake `Interference` and `Section` (the second face is the same
  id, unused) tie this to stage 5's field layout; the empty operand is built per call.
- **A free end inside the face is refused (`Dangles`), as the pin.** Against: stage 5 would
  drop it silently. For: a dangling curve is almost surely the caller's error, and BO16b wants
  every projected curve to bound something. A curve that contributes nothing is `Misses`.
- **A curve that only slits (hole to boundary) is allowed**: one region, the face with the cut
  twice in its loop. The pin refuses it as `CurveDoesNotCut`. Against: nothing is bounded.
- **Shared stretch of two cuts is refused (`Overlap`)**; one with the boundary is dropped and
  its ends are points. Deduplicating two equal curves silently would hide a caller error.
- **Region point**: the middle of the widest stretch inside the face on a horizontal uv line
  through the box of its outer loop (first of 7 heights that has one), on the loop samples.

Truth tests: a disc cut from a cube face (pi/16 and 1 - pi/16; 9 vertices, 13 edges, 7
faces), a line (two halves; 10, 15, 7), two crossing lines (four quarters; 13, 20, 9), a
circle of radius 0.6 across the four sides (central area `pi r^2 - 4 (r^2 acos(d/r) - d
sqrt(r^2 - d^2))` and four equal corners; 16, 24, 10), nested circles (pi/100, 0.08 pi,
1 - 0.09 pi), a chord of a disc cap (segment area), a bounded curve equal to the whole line,
determinism, and the seven refusals; volume 1 and pi unchanged.
Added work: none to existing entries (the cleanup was moved, not changed); the new call costs
one `crossings` per atom pair with meeting boxes, quadratic in atoms (a few for these cases).

### 13.22 BO25 as built (11-28)

`scx_boolean::combine(a, b, &Rule, cx) -> Result<Done<NoItems>, BooleanError>`
(`crates/scx-boolean/src/boolean.rs`, `cells.rs`); `boolean` is `combine` with `Rule::of(op)`,
and `boolean_given` passes `Rule::of(op)` too, one pipeline. A `Membership` is the set of operands
a cell lies in (bits over operand indices). A `Rule { solid, faces }` says which cells are solid
(`solid: &dyn Fn(Membership) -> bool`) and which faces with the same state on both sides are kept
(`Faces::Boundary`: none, the regularised result; `Faces::Walls`: solid on both sides, each cell
its own region; `Faces::Closure`: walls and laminas or sheets, void on both sides;
`Faces::Where(fn)`: chosen by the two sides' memberships). `Rule::of(op)` is the Boolean;
`Rule::with(solid, faces)` any other.

What the build now holds (04 row 16):

- *Walls.* A face with solid on both sides, kept by the rule, is one face used by two shells
  of different regions. Shells come from union-find over the radial sectors about each edge
  (`cells::shells`), so two boxes sharing a face give two solid regions and one shared face; a
  cell-by-cell result of overlapping boxes gives three solid regions with the overlap's faces as walls.
- *Laminas and sheets.* A kept face with void on both sides is used twice by one shell of a void
  region; its free edges (one fin) are allowed only for these faces (`edges(m, free)`). Coincident
  faces of two boxes on a patch give a lamina when the rule keeps them; a plane's section of a
  box is a sheet.
- *Touches in a face.* Where the operands touch at a point inside a face (a sphere on a box, two
  tangent spheres, the seam cases), that point is a collapsed edge `[v, v]` with one fin, a
  `UseGeom::Point(uv)` loop of its own, in each kept piece that holds it inside (`cells::touches`).
  The six such recipes (tangent and seam spheres on boxes, two spheres, a box minus its inscribed
  sphere) were `Unsupported(NonManifold)`; they now build as solids with the contract passing.
  A touch that no kept piece of one of its faces reaches is not in the result (that face's
  material is gone there, the other face is smooth) and is skipped: a ball inside a cube
  touching one face and crossing another builds all three Booleans, and only the difference
  holds the point. This cleared every `non_manifold` answer in the algebra suite (`boxC` against
  `sphereOnFace`, which touches `boxC`'s y = -2 face from inside: 7 cases over five laws were
  skipped as Unsupported, now asserted).
- *Nesting.* Shells that share a face never contain each other; a void shell of zero volume lies
  in the void round it. A cavity wall whose box reaches past its holder's (a face's box encloses,
  a sphere's reaches past its poles) falls back to `holders_loose`, which tests containment of the
  cavity's points rather than boxes, only after the box test found no holder.

`Unsupported(NonManifold)` is left for exactly three things (the `BooleanUnsupported::NonManifold`
doc): two kept faces leaving an edge of three or more fins in one direction, or one with no
direction there (no normal at the edge's midpoint), so their radial order is undecided; a touch on
a piece's boundary (an edge or vertex of a kept face), where no point loop is the right form; and
scx-bodyq's own `NonManifold` for an operand where more than two regions meet at a place a query
reads. The last matters to BO12, which feeds wall bodies back in as operands.

Decisions (analysis and adversarial pass):

1. *The rule is closures over a `u64` membership, not an enum of the three Booleans.* For: BO12
   needs n-ary selections (at least one, exactly k, a chosen cell) and the predicate is the honest
   signature; `Rule::of` keeps the three. Against: a dyn call per piece (measured below, no cost
   seen) and a 64-operand cap; BO12 can widen `Membership` if it needs more.
2. *The regular path keeps the old shell builder; the radial one runs only when walls or laminas
   are kept.* For: no change to every regular result, no new cost there. Against: two shell
   builders, which BO12 may unify once its cells are general.
3. *`Faces::Closure` keeps faces only.* The closed intersection of boxes sharing an edge or a
   vertex is a wire or a point; `combine` builds no wires or lone points, so that result is empty.
   For: scx-brep holds lone vertices only in shells of their own and BO18's `intersect` already
   gives the wire; the regularised and wall results are what callers ask. Against: the closure is
   then not the topological closure in those cases; BO14 or a later row adds wires and points.
4. *A touch is a point loop, not a lone vertex of the shell.* For: it is on the face, so a query
   or a later Boolean finds it on the face's loops, and the difftest recipes (patch complexes) say
   the same. Against: a degenerate edge is one more form the checks must allow; the contract and
   recipes pass with it.
5. *`holders_loose` for cavity walls only.* A void shell (a lump in a cavity) still nests by
   boxes; with a loose box it could be placed outside. Open: no case seen; BO12 should decide on
   points for both.
6. *Recipe truths changed.* `scx-testkit`'s six touch recipes were `Unsupported(NonManifold)`; they
   are now `Built` with cells and volumes in closed form (truth over pin: the pin builds them too).

Measured (release, best of 7 x 5 runs, before / after, microseconds): plate-4 2970 / 2970, plate-12
120565 / 121667-122245, gear-60 87856 / 87342, box-box-union 124.2 / 122.2, sphere-and-box 45.2 /
44.3, coplanar-union 204.1 / 201.2, torus-minus-box 62.9 / 61.3, corner-contact-union 98.1 / 98.0,
edge-contact-union 207.5 / 207.3, disjoint-union 57.4 / 57.6 (box-cyl-hole noisy both sides, load
10-18). Added on the regular path: the rule's call per piece and, when nesting runs, each shell's
face ids and a sorted merge to test sharing; the touch and loose-holder work run only where the
Boolean used to fail.

Tests: `crates/scx-boolean/src/tests/nonmanifold.rs` (9): the spelt rules are the Booleans, boxes
sharing a face (closed intersection a lamina of area 1 with four free edges; walls two unit
regions and one two-sided face), a box on a box sharing a 1 by 1 patch, boxes sharing an edge (two
regions at a four-fin edge, closure empty) and a vertex (two outer shells at it, closure empty),
overlapping boxes cell by cell (three regions of 1.75, 0.25 and 1.75, six walls), a box in a box
cell by cell (the inner box's faces the outer cell's cavity wall), sections by a plane (a 2 by 2
square, a disc of pi r^2, free edges), the closure of a regular result is it.
`contacts.rs`: `point_contacts_are_built` (was the Unsupported check) asserts volume, cells and a
point loop per touch. Changed on purpose: `whole.rs`'s `non_manifold_results_are_unsupported`
(proved a ball on a cube was refused) is now `a_ball_on_a_cube_touches_it_at_a_point` (volume
1 + pi/6, two solid regions, contract); new `a_ball_touching_a_face_from_inside_and_crossing_another`
(the three volumes in closed form, a vertex at the touch only in the difference); `scx-boolean/tests/recipes.rs` and `scx-testkit/tests/recipe.rs`
judge the six recipes as built. `scx-difftest`'s `t_slice` counts a touch as truth's patch
complex does (a vertex of each sheet, a point loop of each patch) on both sides: scx's loop of one
collapsed edge, or a seam or pole vertex that is a touch; the pin's vertex loop, whose vertex its
Euler count had missed. The six recipes' work joins `slice_work.txt` (9 to 78).

Open: wires and points in `Closure` (BO14); void-shell nesting on points (BO12); scx-brep's sphere
face box past its poles is loose (a tighter box helps every nesting test).

### 13.23 CN2 as built (11-28)

`crates/scx-nurbs-surface` (77 §4.3, R77 row 4, CN1's `CompatibleCurves::from_curves` reused):

- `NurbsSurface::iso(axis, t) -> Result<NurbsCurve<D>>` (`iso.rs`): the curve where parameter `axis` equals `t`, along the other parameter (`iso(U, t)` is `v -> S(t, v)`; the pin's `Direction` names the free parameter, this names the fixed one, and `Direction::UV` is gone). The lines of the net along `axis` are taken as `CompatibleCurves`, `t` is inserted to multiplicity `p` (nothing when it already is one), and control point `k - p` of each refined line (`k` the last knot equal to `t`) is the curve's point; at an end of a clamped domain it is the boundary row, copied. Rational surfaces give rational curves from the homogeneous points.
- `NurbsSurface::restrict(&Extent2d) -> Result<Self>`: splits at each end of the box that is inside a domain; parameters are kept, so `S'(u, v) = S(u, v)` on the box and the domain of `S'` is the box. A box equal to the domain returns a copy bit for bit.
- `CompatibleSurfaces::from_surfaces(axis, &[NurbsSurface<D>])` (`compatible.rs`) with `axis()`, `knot_vector()`, `len()`, `surfaces()`, `get(i)`, `into_surfaces()`: every line of every source along `axis` goes through one `CompatibleCurves::from_curves`, then the lines are split back into surfaces; the other direction of each is untouched. Each result is its source with the `axis` parameter mapped affinely onto the first source's domain; rational if any source is.
- Typed causes (new): `NurbsSurfaceError::Restrict(RestrictCause::{Outside, Empty})` and `NoSurfaces`, all `Refused`. A `t` outside the domain or not finite, an unclamped end, and the lines' compatibility causes come as the existing `Edit { axis, cause }` over the curve cause. No input is changed (`&`), nothing is snapped.

Decisions.
- **Insertion, not the pin's local de Boor.** The pin runs the triangle on the one span (`O(lines p^2)`) and extrapolates outside the domain. Insertion on the lines costs `O(lines n)` per call (the polygon is copied) but is the checked edit of CN1/NS3, keeps the rational case in homogeneous form for free, and shares the multiplicity and domain checks. Against: a caller taking thousands of iso-curves of a large net (a Gordon network, MK11) pays `n/p^2` more; a local triangle can replace the body without changing the signature if MK11 measures it.
- **Outside the domain is refused**, not extrapolated: the pin's end-span continuation is a surface the net does not define.
- **`restrict` cuts, never snaps.** The pin's `extract_patch` moves cuts within `1e-6` onto knots; a cut a hair from a knot leaves both knots (more knots, exact geometry). A zero-width box is `Empty` (it has no area; the caller wants `iso`). Cost: up to four splits, each computing both pieces and dropping one (about twice the minimum); not measured as a need.
- **Compatibility over all lines at once**, not one representative line per surface: simple, and the union and the result are the same since the lines of a surface already share knots. Cost `O(sources lines)` curve edits where one line each plus a replay would do; a batch `from_curves` on `CompatibleCurves` inputs is the follow-up if MK15b shows it. A rational source's lines round-trip through `w P / w`, so already-compatible rational sources are not bit for bit (polynomial ones are, tested).
- **Domain mapping is CN1's:** the first source's `axis` domain; the caller who needs another maps the first.

Work counts (compatibility test family: groups of 3 surfaces over both axes, degrees 1-3, uniform, random and multiple knots, polynomial and rational, `axis` domains `[0,1]`, `[-2,1]`, `[5,5.5]`): knots along the axis in 326, out 588; control points in 1300, out 2848 (elevation to the highest degree plus the union, as in 13.16).

Tests (`construct_tests`, 11; truth is a closed form or the source): the iso-curve of the NURBS cylinder (9-point rational quadratic circle, radius 2.5, height 3) at 25 heights (ends, `1e-9` from each end, random) matches the analytic circle (each quarter's rational Bezier written out, turned by right angles) at 2001 parameters to `1e-14 (1 + |x|)` with radius `r` to `1e-14 r`, on the circle's own knots and degree; across the circle (`iso(U, t)`, at a knot, between knots and at the ends) it is a degree-1 line from the base point to the top; the bilinear patch's iso-curves are lines equal to the lerp of its edges to `1e-15`; random surfaces' iso-curves (every knot kind, polynomial and rational, parameters on and within 4 ulps of knots) evaluate as the source at 40 parameters to `1e-11` with the other direction's knots; `restrict` agrees with the source at 121 points on 100+ boxes (domain, half-domain, knot-adjacent, random) to `1e-10`, with the box as domain, source degrees and clamped ends; the whole domain restricts to the surface bit for bit; compatible surfaces evaluate as their sources (200 parameters each, relabelled), with equal knots, the highest degree, the other direction's knots kept, and inputs unchanged; already-shared polynomial knots come back bit for bit; refusals (empty, outside, not finite, unclamped, no surfaces) with their causes and unchanged inputs.

### 13.24 MK5 as built (11-28)

`scx_make::sweep(&Region, &Frame, &Path, &Document) -> Result<Made<SweepFaces>, MakeError>` sweeps a
line-and-arc region, placed in the frame's `x, y` plane, along an MK7a `Path` with no law: the
profile is carried by the path's rotation-minimising frame (`frames`, `Stop::At` at each piece's
start, `Stop::Corner` at each mitre, `Corner::stretch`; nothing re-derived). The frame's origin is
the path's start and its z axis the start's tangent. `SweepFaces { start: Option<FaceId>, end:
Option<FaceId>, walls: Vec<Vec<Vec<FaceId>>> }`, walls `[piece][loop][segment]` with segments in
the order given to `Region::new`; the caps are `None` for a closed path. The module is
`crates/scx-make/src/sweep.rs`; `frames.rs` gained the crate-private `Path::joints` (each piece
with its start fraction and corner).

Decisions, with cases for and against:

- **Walls by kind, chosen in a plan before anything is built** (R4, R5). A line piece is an
  extrusion: a profile line's wall is a plane, an arc's a cylinder. An arc piece is a partial
  revolve about the arc's axis (which lies in the profile's plane): a line's wall is a plane
  across the axis, a cylinder or a cone (MK4's snap rules: within the zone in height a plane at the
  mean height, in radius a cylinder at the mean radius, else the cone with the revolve's frame and
  half-angle), an arc's a sphere (centre on the axis), a ring torus or a horn torus; a spindle arc
  is `Unsupported::SpindleTorus` as in the revolve. For: no B-spline net, no sampling, no
  orientation pass; the sense of each surface follows from the profile's side. Against: the
  revolve's code is not shared (its plan is private and about the frame's y axis); the snap rules
  are restated, about 60 lines. Sharing waits for a second user of a "revolve about any axis" plan.
- **One draft, sections shared, no stitch.** Each joint's section (vertices and rim edges) is made
  once and used by the walls either side and by a cap; the edge along a piece is a line on a line
  piece and a circle about the axis on an arc piece. For: the pin's per-piece bodies plus copy and
  global stitch (75 §3.4) go, and nothing is matched by distance. Against: none found; the record is
  `[piece][loop][segment]` rather than the pin's flat face list.
- **Mitres in closed form.** At a corner between lines the section is the profile turned by the
  corner's half rotation into the bisecting plane and stretched by `Corner::stretch` along the turn
  (`to - from`). Its lines stay lines; its arcs are ellipses whose principal axes are the
  eigenvectors of the placement's Gram matrix (`psi = atan2(2 g12, g11 - g22) / 2`), semi-axes
  `r |ex'|`, `r |ey'|`, and both cylinders either side carry them (the writer's ellipse-on-cylinder
  pcurve). For: no Boolean, no SSI, no `scx-ssi` dependency (75 §8 listed one for "mitre conics";
  not needed). Against: a corner next to an arc piece would cut a cone, sphere or torus by its
  mitre plane in a curve that is not a conic (a torus's is quartic): `Unsupported::CornerOnArc`
  until the procedural kinds or a fitted curve can carry it.
- **Closed paths.** A path that ends where it starts (`Path::is_closed`) has as many sections as
  pieces, the last piece ending on the first section; no caps; each hole's walls line a hollow of
  their own (`Assembly::hollow`), as a full revolve's holes do. It may close smoothly or at a
  corner between two lines (the first section is then that mitre, built from the start's side: half
  the closing rotation backwards). A whole-turn arc is allowed only as the single piece of a closed
  path (a torus or, for a square, two cylinders and two annuli). The profile must come back to
  itself: the end's rotation (after the closing corner) moves the farthest profile point by at most
  the zone, else `Refused(ClosedTurned { off })`. For: the pin cannot close at a corner; an
  out-of-plane closed path needs the twist that makes it close, which is MK7b's law. Against: a
  closed path whose holonomy is a symmetry of the profile (a square turned by 90 degrees) is
  refused although its body exists; that needs the profile's symmetry, not the frame's, and is left
  open.
- **Refusals by closed form.** `TightArc { piece, radius, reach }` when an arc's radius does not
  exceed the profile's reach towards its axis (its support function in that direction: vertices, and
  an arc's centre plus radius where its normal lies within it) by more than the zone.
  `MitresMeet { piece }` when a line piece's length does not exceed the profile's reach along
  `n_out / (t . n_out) - n_in / (t . n_in)` (the two mitre planes' slants, `t` for an end with no
  corner) by the zone: a U's middle piece is refused, a zig's (parallel mitres) never is.
  `ArcTurn { piece, angle }` for an arc of a whole turn or more (within the zone at the widest
  radius) unless it is that single closed piece. `Placement { offset, tilt }` for a frame off the
  path's start (zone) or its z off the start tangent (`G1_TANGENTS`). `Path(FrameError)` for a
  closed path that doubles back where it closes. `Unsupported::HelixPiece { piece }` for a helix
  (the procedural kind; a zero rise is still a helix). `Defect::Station(FrameError)` if a station
  the plan asks for is refused (cannot happen for a valid path).
- **A profile touching an arc's axis is refused as tight**, not built with a pole as the revolve
  does. For: a pole needs collapsed rails and walls that end at a point on an arc piece, and the
  line pieces either side would then meet that piece at a section with a collapsed edge; the plan
  stays one shape per piece, and the case (a profile on the axis of its own bend) is one a revolve
  builds directly. Against: a deviation from MK4's behaviour
  for the same profile; recorded, open.
- **Not checked**: the swept body meeting itself across pieces that are not neighbours (a path
  that comes back near itself). The pin does not check it either; a later check would be a
  distance between non-adjacent pieces' tubes.

Pin shapes designed out: per-piece bodies plus copy and global stitch; the planar path's Boolean
union per piece and SSI per mitre; `cap: bool` (open paths always have caps and closed ones none;
an uncapped open sweep is a sheet body, left for a sheet entry if one is asked for); a G1-only path (corners are mitres here);
the try-chain that swallows a failed piece.

Deviations from 75 (§3.5, §7, §7.2): no `scx-ssi` dependency (mitre conics in closed form; 01's
`scx-make` row may drop it); a corner next to an arc piece is `Unsupported`, not built; closed
paths may close at a corner; the record is `[piece][loop][segment]`.

Measured (release, throwaway timers, best of 20 runs of 50, single lane): a 3 x 4 rectangle along
a line 15.7 us against `extrude` 14.3 us (+10%, the frame stations and the plan's maps); a circle
along a line 10.3 us against 9.8 us (+5%); a rectangle along a quarter arc 25.3 us against
`revolve`'s quarter turn 25.9 us; a circle 12.5 us against 12.2 us; an L of two lines with a
mitre 24.6 us (rectangle) and 16.6 us (circle, an ellipse mitre). No existing path changed:
`Path::joints` is new and the error variants are new arms.

Tests (`crates/scx-make/tests/sweep.rs`, 21), all against closed forms with the structure check
(two fins of opposite senses per edge, Euler-Poincare) and the BR8 oracle: a straight rod and a
tilted straight square (prism volume and area); a quarter arc (Pappus: a centred and an off-centre
circle, an off-centre rectangle with its annular-sector caps); an arc piece's kinds (plane, cone,
torus, cylinder) and its Pappus volume; line-arc-line rods; a five-segment profile's counts
(12 F, 25 E, 15 V) and kinds per piece; square Ls at 60, 90 and 120 degrees (`a^2 (L1 + L2)`,
mitre vertices at `hypot(a/2, a/(2 cos(theta/2)))`); a circle L whose mitre is the ellipse of
semi-axes `r` and `r sqrt 2`; a four-piece out-of-plane polyline tube (circle and square); a bend
then a corner; a square with a round hole along a bent path (genus 1); a closed square frame
(square and circle, four ellipse mitres); a closed frame with a hole (a hollow, four shells); a
whole-turn torus and square ring; determinism to the bit; and the refusals `TightArc` (and the
same circle moved off the axis built), `MitresMeet` (a U refused, a wider U and a short zig built),
`Placement` (offset and tilt), `HelixPiece`, `CornerOnArc`, `ArcTurn` and `ClosedTurned` (a
tetrahedral closed polyline, turned by 60 degrees). Truths from the pin's `sweep_cornered.rs` (75
§8.2) are the mitre-joined polyline tubes above; none are ported verbatim (they build through
the pin's stitch).

Open: the profile-symmetry closing above; the pole case; self-meeting across pieces; corners next
to arcs (procedural kinds or fitted mitre curves).

### 13.25 MK7b laws (11-28)

The first part of MK7b: the sweep's laws over MK7a's frames, everything that places the
profile at a station, and one public `sweep` shared with MK5 (§13.24). The walls of a sweep
with laws wait on step 5's procedural sweep kind (`Surface` has none). Code in
`scx-make/src/sweep/mod.rs` (laws, entry) and `sweep/rail.rs`, beside MK5's
`sweep/walls.rs`; tests in `tests/sweep_laws.rs` (19), MK5's 21 in `tests/sweep.rs`.

- `placements(&Path, normal: UnitVec3, &PathFrame, &[Stop], &Document) -> Result<Vec<Placement>,
  MakeError>`: a `Placement { from, at, m }` takes the profile's point `q`, drawn at the path's
  start, to `at + m (q - from)`. The shape laws are the pin's `shape(v) = k Rot(n, twist u) v`,
  `k = 1 + (end_scale - 1) u`, with `n` the profile's normal turned to point along `t0`. The
  orientation follows them: `Perpendicular` uses the frame's turn, `Parallel` uses none, and
  `Direction` uses `[t, r, t x r] [t0, r0, t0 x r0]^T`. At a `Stop::Corner` the columns are
  stretched by `Corner::stretch` along `to - from`, except for a parallel profile. With rails
  the map is the pin's `rail_map`. One rail gives `d` from the path to the rail, square to `t`.
  Two rails give `d` from the first rail to the second. The profile turns `d0` into `d`, sized
  by `|d| / |d0|`, evenly (`RailFit::Scale`) or only across (`Stretch`), and `at` puts the
  profile's point on the (first) rail onto it.
- `fractions(&Path, &PathFrame, &Document)`: SolidCraft's skin rule. It takes one station per
  degree of `|twist|` plus the tangent's turn (`Path::tangent_turn`, `a phi / sqrt(a^2 + b^2)`
  per helix) plus the corners' angles, at least a rail's control points less one, and clamps
  the count to `[MIN_SKIN_STATIONS, MAX_SKIN_STATIONS]` = `[16, 2048]`. The fractions are even
  in arc length with every joint included exactly. The rule is carried over, not derived; the
  kind's own bound replaces it.
- `sweep(&Region, &Frame, &Path, &PathFrame, &Document) -> Result<Made<SweepFaces>, MakeError>`
  is the one public sweep. `PathFrame::Shaped(Shape::PLAIN)` runs MK5's walls by kind
  (`walls::build`, its refusals and `Unsupported::HelixPiece` unchanged). Any other frame
  (twist, end scale, another orientation, rails) is checked with every station the skin would
  use, then answered `Unsupported::NeedsKind { kind: Needed::ProceduralSweep }`. The kind
  plugs in at the comment at the end of `sweep`, taking the profile's segments and the
  placements, and fills MK5's `SweepFaces` (`[piece][loop][segment]`, caps `Option`).
- A rail crosses a station's plane where the Bernstein coefficients of `(X - p W) . t` have a
  root. Each span is rational Bezier: one for a line, a quadratic per quarter for an arc, and
  the knots inserted to the degree for a clamped NURBS curve. The roots come from
  `scx_algo::bernstein_roots`, and the coefficients are zeroed within
  `gamma_5 (1 + gamma_6) size`. A rail's own end within the resolution reaches the plane. A
  root at a knot is one root, and the crossing must change sign over the rail. Anything else
  is `RailFault::{Short, Touches, Twice, InPlane, TooNear}`.
- Errors added: `Refusal::Law(LawFault)`, `Unsupported::NeedsKind { kind: Needed }` and
  `Unsupported::RailsAtCorner`. `Refusal::Path(FrameError)` and `From<FrameError>` are MK5's,
  shared. `frames.rs` gains one crate-private reader, `tangent_turn`; the skin's joints come
  from MK5's `Path::joints`.

Decisions and cases:
- **`PathFrame` is an enum, `Shaped(Shape) | Railed { rails: Rails, fit }`, and `Rails` is
  `One | Two`.** 75 wrote a struct with `rails: Vec<Rail>` beside twist and scale. For: the
  pin's refusals "rails set the twist and scale" and "more than two rails" cannot be written.
  Against: a caller with a twist and rails must choose, where the pin's error told them why.
  The pin refused that pair anyway, so nothing is lost.
- **The least angle lives in its orientation.** `Parallel { least_crossing }` and
  `Direction { along, least_sine }` replace the pin's always-present fields that only one mode
  read (a field ignored unless a mode is set). For: no dead fields. Against: the caller writes
  SolidCraft's 5 degrees and `1e-3` themselves; the docs name them.
- **The caller's twist and scale limits are not ours.** The pin refused past
  `max_twist_turns` and `end_scale_within`, which are SolidCraft's UI policy. Only the geometric
  limits are refused here: twist finite and at most `MAX_TURNS` turns, scale finite and
  positive. For: one rule, no policy setter. Against: a caller that forgets its own limit gets
  a 1000-turn sweep. That is legal geometry, and the skin clamps at 2048 stations.
- **`Rail` is closed-form `Line | Arc | Nurbs(Arc<NurbsCurve<3>>)`, not any curve.** The pin
  took any `&Curve` and converted it to NURBS, refusing one with no NURBS form at run time.
  For: each variant splits exactly into rational Bezier spans, so the crossing is the roots
  of a polynomial, not a march, and a rail with no exact form cannot be passed. Against: an
  ellipse or other kind must come as NURBS. A helix has no exact NURBS form, so a helical rail
  is not possible yet (the pin refused it too). That is an open question.
- **The resolution replaces the pin's `1e-9 (1 + |P|)`** where a rail's end reaches a plane,
  as in MK7a for pieces meeting.
- **A placement is a self-contained affine map** (`from`, `at`, `m`), not the pin's
  `Station::translation(first)` that needed the path's first point beside it. For: the kind
  and the tests read one value. Against: `from` repeats in every placement (24 bytes).
- **Rails along a path with a corner are `Unsupported::RailsAtCorner`.** The pin did the same.
  A mitre's plane across a rail has no single crossing rule yet.
- **`sweep` validates before it answers Unsupported.** For: a caller gets its real refusal
  (a rail stopping short) now, not after step 5, and the station work is exercised. Against:
  it does work whose result it drops. That costs about 1 ms for 2049 stations (below).
- **`fractions` stays SolidCraft's uncertified count.** For: the skin is the kind's to bound.
  Deriving a count now would be a bound for a surface that does not exist. Against: nothing
  proves 1 degree is close enough. The rule is recorded as residue.

Idiom hunt: there are no setters, out-params or failing defaults. `Shape::PLAIN` is a value,
not a fallible default. The pin's `Laws` (a `Default` of nine fields, `rail_fit` ignored
without rails, `min_crossing` and `min_across` ignored outside their modes, `rails:
Vec<&Curve>` checked for length at run time) is gone.

Truths (`tests/sweep_laws.rs`, 19 tests, closed forms, 1e-12 unless stated):
- On a straight path, twist and scale act linearly. `X` goes to `k_u (cos Tu, sin Tu, 0)`,
  `Z` is kept, a square's corner is turned and scaled, and a circle's radius is `10 k`
  (pin :138).
- A twist alone keeps the map orthonormal and the corner's angle at `pi/2 u`. This is the
  app's twisted prism's section; its volume waits for the walls.
- A diverging line rail scales by `(d0 + s x) / d0`, evenly or across (pin :238). An arc rail
  scales by `sqrt(R^2 - x^2) / R` (pin :268).
- **A rail is followed exactly** (new). Along a quarter arc, concentric arc rails of radii 13
  and 8 are crossed at `r (cos phi, sin phi)`, one or both. A line rail that rises out of the
  path's plane is crossed at its point on the station's plane, for both fits.
- Rails that stop short, touch, cross twice or lie in the plane are refused, and so are rails
  that meet the path. A polyline's vertex on a plane is one crossing, at a scale of 5/3. A
  cubic `x - 4 = -4 (1 - 2s)^2 (1 - s/2)` touches the plane `x = 4` and crosses `x = 3` twice
  (pin :291, :364, :387, :407).
- Two rails span the profile with `k = (4 + s x) / 4`, and the origin goes to `(x, 2 - 2k, 0)`
  (pin :420). Rails at a corner are `Unsupported` (pin :455).
- The parallel orientation is the identity map. The crossing is refused at 90 degrees, and at
  84 degrees it passes with 5 and fails with 7 (pin :627). The direction orientation along a
  helix takes `t0` to `t` and keeps the direction across, and a path along the direction is
  refused (pin :665).
- A profile normal against the path gives equal placements (pin :692).
- A right-angle corner's mitre is turned to the bisector and stretched by `sqrt 2` along
  `b`, with the scale at the corner's fraction. This is the stretch half MK7a left.
- The refusals (pin :706) are twist (infinite, NaN, 1001 turns), end scale (0, -1, NaN,
  infinite), least crossing and least sine out of range, a bad rail shape, and the frames'
  refusals coming through as `path`.
- `fractions` (pin :309): a quarter arc twisted a quarter turn gives 181 values; a line gives
  17; a helix with `a = 3, b = 4` gives 217 (216 degrees); a joint is included exactly; a rail
  of 40 points asks for 39 steps.
- `sweep` with laws refuses first and otherwise answers `NeedsKind`; with the plain frame it
  builds.

Removed on purpose, as listed in the test file's header:
- From `out_of_range_paths_laws_and_fractions_are_refused`, the "more than two rails" and
  "rails with a twist" cases. They proved the refusal, and the type now forbids both.
- `the_caller_s_twist_and_scale_limits_refuse_past_them`. It proved SolidCraft's policy
  limits, which are now the caller's.

Remaining for the later MK7b part, once step 5 adds the kind:
- the walls and caps of a sweep with laws on the procedural sweep kind, in MK5's
  `SweepFaces`;
- the volume truths: a twisted square prism keeps `area x length`, and an end scale gives the
  frustum's `h pi r^2 (1 + k + k^2) / 3`;
- the certified station count from the kind's bound, replacing `fractions`;
- a mitre's rail crossing (corners with rails);
- helical rails.

Cost: new code only, and no existing path changes. In a release build, 2049 placements along
a 20-turn helix with twist and scale take 0.39 ms (about 190 ns a station). Two line rails
take 0.82 ms (about 400 ns: two crossings and a frame). `fractions` takes 3 us. Measured
with a throwaway timer that was not committed.

### 13.26 CN3 as built (11-28)

`scx_nurbs_surface::fit` (77 §4.3, row CN3; FT2's `scx_nurbs::fit` and CN1's `CompatibleCurves` reused) and `NurbsSurface::bernstein_over` (`bernstein.rs`). Both passes of every fit are FT2's banded rotation solves; the tensor system is separable, so no matrix larger than one line's is formed.

API (all `Result<_, SurfaceFitError>`):
- `interpolate(&Net<[f64; D]>, u, v, (p, q))` and `interpolate_on(grid, &ku, &kv, u, v)`: the surface through a grid at given parameters (row `i` of the net at `u[i]`, column `j` at `v[j]`), knots by averaging or given. Rows (along u) are fitted by FT2 into `CompatibleCurves`, then their control columns along v.
- `interpolate_rows(&CompatibleCurves<D>, v, degree, &Edge, &Edge)` and `interpolate_rows_on(.., &kv, ..)`: skinning with given parameters, the curves being CN1's. Rational rows are fitted in homogeneous space (`w P` and `w`, two solves per column); the surface passes through the rows and is rational.
- `Edge<D>` is `Point` or `Held(Vec<End<D>>)`: one FT2 `End` per control point of a row, a derivative of that control column in v, all of one order (G1/G2/G3 boundary). Refused for rational rows (lifted by CN4, §13.28, which adds `Edge::Homogeneous` and drops `RationalEnds`).
- `least_squares(grid, u, v, (p, q), (nu, nv))`, `least_squares_on`, `least_squares_rows`, `least_squares_rows_on`: knots by `approximation`, corners held (the end points of every line are fixed).
- `grid_parameters(grid, Param) -> (u, v)`: FT2's `parameters` per line, averaged over the lines (lines with all points equal left out).
- `Edge`, `SurfaceFitError::{Fit { axis, cause }, Curves, Surface, EdgeCount, EdgeOrders, RationalEnds, Defect}` with `kind()` and `name()`; the frames inherit their cause's kind. Nothing is snapped, clamped or retried.
- `NurbsSurface::bernstein_over(&Extent2d) -> BernsteinForm<D>`: for a box inside one span (no knot strictly inside it) the Bernstein forms (`scx_algo::bernstein::Bernstein2`) of `w P` (or `P`) and `w` over the box mapped to the unit square, taken from `restrict` (CN2), whose Bezier control points are the coefficients. `BernsteinForm::{coordinates, weight, degrees, point, restrict}`; products, sums and derivatives are `Bernstein2`'s (and `Poly3`'s dot and cross). New cause `RestrictCause::Interior { axis, knot }`; the box checks of `restrict` are now `check_box`, shared. `scx-nurbs-surface` gains the `scx-algo` dependency (layers.txt already allows it).

Decisions.
- **Separable two-pass fit, not a tensor system.** For: linear in the grid, one code path with FT2, the same numbers as fitting each curve. Against: the least-squares result is the separable one with the four boundary curves fitted to the boundary data (the pin's), not the global minimum with corners only held; for exact data on the knot space they agree.
- **Ends only on the row fit (`Edge`), not on a grid.** The pin's first-derivatives-at-every-point surface and the twist they need are CN4's with the boundary conditions. For: no guessed twist. Against: a grid with prescribed boundary slopes needs the caller to fit the derivative curve itself.
- **Derivative ends on rational rows are refused** (`RationalEnds`) rather than interpreted in homogeneous space. For: no meaning invented. Against: rational skins with tangent ends wait for CN4. (Lifted by CN4, §13.28.)
- **A fitted weight that is not positive is `Surface(BadWeights)`**, not clamped (the pin clamps weights to `[1e-3, 100]`). Test: weights 1, .01, 1, .01, 1 at five rows.
- **Bernstein through `restrict`.** For: exact, one code path, rational for free. Against: `restrict` copies the whole surface per call, so all spans of a large surface cost `O(spans * net)`; a Bezier decomposition in one pass is the follow-up if MK10 loops over every span (scx-surface's own patch builder already does one).
- **Not ported:** `func_interp`'s own type (a `D = 1` grid does it), weighted surface fits, closed surfaces, per-point first derivatives, the pin's `i64` count arithmetic and its three overflow tests (they assert typed refusals of impossible counts; counts here are `usize` lengths of the data and cannot overflow a signed index).

Work (release, throwaway timer; degree (3, 3), square grids): interpolation of an `n x n` grid is `2n` banded solves of size `n` (`n` along u, `n` along v; `4n` rational), `n` = 20/40/80/160/320/640: 374/361/350/203/193/183 ns per grid point (150 us to 75 ms total), so linear in the grid. Least squares to a net of `n/4`: `n` solves of `n` rows by `n/4` unknowns then `n/4` of `n` by `n/4`; 332/365/408/347/445/706 ns per point; the rise at 640 is the column gather over a 10 MB grid, not the solves (per-row work is `O(p^2)`).

Tests (`fit_tests`, 12; truth is a closed form or the data): a bilinear patch from 4 corners, degrees (1,1), and from 5x4 and 7x9 uneven grids at degrees 1-4 x 1-3, at the data to `1e-12` and at 100 random parameters between to `1e-11`; random polynomial surfaces of degree `(p, q)`, p, q in 1-4, interpolated and least-squared (net sizes with and without interior knots, 3x oversampled) match at 200 parameters to `1e-9` / `1e-8`; circles (the 9-point rational quadratic) at 2, 4 and 7 uneven heights, v-degree 1-3, interpolated and least-squared, equal the closed-form cylinder at 300 parameters to `1e-12`, radius `r` to `1e-12 r`, rows equal the source circles; Held ends of order 1 and 2 on a bicubic reproduce it and its end derivative; the pin's `interpolation_hits_points` (5x4 chord grid, degrees (3,2)), `approximation_of_plane_is_exact` (9x7, net 5x4), `function_interpolation` (scalar grid) and `params_uniform_and_chord`, with its refusals (wrong knot count, net larger than the data); random spline samples interpolate back; typed refusals with unchanged inputs; the non-positive weight; Bernstein forms equal the surface on every span and an inner box of a polynomial and a rational surface (1e-11), the form of a cross product equals the cross of finite-difference partials, a form's `restrict` equals the form over the smaller box, and the interior, outside and empty refusals. Removed: none. The pin's three `huge_*` and `oversized_degrees` tests are not ported (above).

### 13.27 PK1 as built (11-28)

`scx_surface::Procedural` (`crates/scx-surface/src/procedural.rs`) and the new variant
`Surface::Procedural(Arc<Procedural>)` (77 row PK1, 77 §3.3). A `Procedural` is a
`Definition` and the exact rational net of that definition, built once in `Procedural::new` and
kept with the value; it has no `NetScope` and no setters.

- `Definition::Revolution(Revolution)`: `Revolution::new(&Curve, Extent1d, Point3, Vec3)`
  takes the generator over its range, an axis point and an axis direction. `u` is the turn in
  the quarter-arc parameter (63 D13), periodic over `[0, 1]`, with `u = 0` at the generator.
  `v` is the generator's parameter.
- `Definition::Extrusion(Extrusion)`: `Extrusion::new(&Curve, Extent1d, Vec3, Extent1d)`.
  `u` is the generator's parameter and `v` multiplies the vector over its range.
- Generators: a line; a circle or ellipse over whole quarter arcs, in the quarter-arc
  parameter of its angle so the net is exact in the same uv; or a NURBS curve over its domain.
  A parabola or hyperbola is `ProceduralError::Generator`.
- `ProceduralError` (`Copy`, `#[non_exhaustive]`, with `kind()` and `name()`): `Generator`,
  `Range`, `NotFinite`, `ZeroDirection`, `OnAxis`, `Flat` and `Net`. It is wrapped once as
  `SurfaceError::Procedural` (`procedural_failed`).
- `Procedural::{new, definition, net, bound, net_over, point, eval, jet, normal, domain,
  to_nurbs, reversed, transformed}`. `eval<T: Ranged>` is the definition in `f64` or `Ivl`.
  `jet<D>` gives the definition's partials in closed form.

**The net and its bound.** The control data are built twice, in `f64` and in outward
intervals, from the same closed formulas (quarter arcs of weight `1/sqrt 2`, which are exact
up to the one rounding of `sqrt 2 / 2`). Inside its knots' box, the net's point is a convex
combination of its rounded points with weights `N w~`. So
`|S~ - S| <= delta + eps / (1 - eps) * D`, where:

- `delta` is the largest `|P~ - P|`;
- `eps` is the largest `|w~ / W - 1|`;
- `D` bounds the spread of the exact points.

Each is taken from the interval build, upper end. `Procedural::bound` is that number. It
covers the rounding of the control data only; each side's own evaluation rounds on top.

**The per-call rule (77 §3.3).** Point, jet, normal, `eval_many` and `eval_grid` read the
definition, always. The calls that need control points read the net when the uv they cover
lies in the net's knots' box (`net_over`), and add the bound wherever the answer must hold the
definition:

- `box_over`: the NURBS box widened by the bound.
- `range_along`: the NURBS range ± `sum |d| * bound`.
- `hulls`: each NURBS hull widened by the bound.
- `invert`: the net's inversion, whose foot is on the net.
- `to_nurbs`: the net restricted.

Past the box, `box_over` and `range_along` enclose the definition (`analytic` cut at the
quarter arcs). `to_nurbs` builds a net over the quarter arcs that cover the extent, up to
two turns. Inversion past the box is `nurbs_invert` against the net, which is periodic in a
revolution's `u`.

**The other arms.**

- `chart` is `None`.
- `reversed` and `transformed` are exact and return the `UvMap`. An affine map keeps a
  revolution only where it is a similarity (`UvMap::similar`); otherwise it is
  `SurfaceError::NotSimilar` (the torus's rule).
- `gap` sends a procedural face to `Pending::NurbsSurface`, as a NURBS one.
- `intersect`'s closed forms, `pcurve_of` and `axial` refuse it (`SurfaceKind::Procedural`,
  `NotRevolved`).

**Wildcard audit (external `match` on `Surface` with a `_` arm).** Nothing builds a
`Procedural` yet; each site below is what such a face meets today.

Refuses with a typed error:

- scx-boolean `faceface.rs` 759: now refuses `Nurbs | Procedural` (`BooleanUnsupported::Nurbs`).
- scx-bodyq `cap.rs` 172: now refuses `Nurbs | Procedural` (`BodyqUnsupported::Nurbs`).
- scx-ssi `general::intersect`: refuses a procedural operand before any work, with the new
  `SsiError::Procedural { operand }` (`ssi_procedural`, `Unsupported`, no point). Without it,
  `general/patch.rs` 195 and 213 give no jet and the survey would split to the budget. A
  net-based patch jet with the bound is the follow-up.
- scx-surface `certificate/kind.rs` `Analytic::of`: `None`, as for NURBS.
- scx-make `extrude.rs` 374 (`Unsupported::Segment`), `revolve.rs` 435 (`Defect::Interval`)
  and `face.rs` 154 (`Unsupported::NurbsFace`).
- scx-bodyq `recognize/prism.rs` 512, 521 and 529 (`Unsupported`).
- scx-mass `patch.rs` 39 (`PatchFail::Kind`).
- scx-hlr `silhouette.rs` 372 (`Why::Freeform`).
- scx-step `geom.rs` 271 (`Waits::OtherSurface`).

Answers "no closed form", "none" or "unknown", so the caller takes its general or refusing path:

- scx-ssi `analytic/mod.rs` 229 (to `coaxial`, then `General`), `section.rs` 104,
  `coaxial.rs` 115 and 520, `quadric.rs` 57 and 192, `overlap.rs` 225, and `clip.rs` 448
  and 568 (`Region::new` `None`).
- scx-ssi `general/implicit.rs` 90 (no implicit form).
- scx-boolean `faceface.rs` 489, 598 and 638, and `planar.rs` 124.
- scx-bodyq `along.rs` 212 and `recognize/bores.rs` 114.
- scx-mass `face.rs` 428 (`Unknown`) and `breaks.rs` 36, 49 and 51. No breaks is moot,
  because the patch refuses.
- scx-hlr `visibility.rs` 1130 (`Ray::Unplaced`).
- scx-brep `cells.rs` 179 (not ruled, so the tighter hull is skipped) and
  `check/geometry.rs` 94 and 103 (no poles; a revolution's poles on its axis are not found,
  as a NURBS one's).
- scx-step `build.rs` 710 and scx-tess `curved.rs` 155 (`None`, the caller's NURBS path).
- scx-surface `recognize.rs` 227 (returned unchanged, deviation zero, which is correct).

Not on `Surface` (a wildcard on another enum): scx-make `face.rs` 168, scx-hlr
`visibility.rs` 1148 and scx-brep `check/geometry.rs` 115. No site answers wrongly; the only
changes outside scx-surface are the three refusals above.

Decisions (analysis and adversarial pass):

1. *Point plus axis, not a `Frame`* (77 §3 wrote `axis: Frame`). For: a revolution needs
   only the axis line. The generator fixes `u = 0`, so a frame's `x` would be a second,
   conflicting origin of the turn and one more thing to validate. Against: the analytic kinds
   all carry a `Frame` and `Sense`, so a procedural revolution does not share `Chart`
   machinery, and PK3a's screw may want the frame back. A frame is cheap to derive (axis
   plus generator foot).
2. *One bound for the whole net, not a bound per span* (77 §3.3's `spans: Vec<SpanBound>`). For: one number, one widening rule,
   and it is tiny (a few roundings of the size; the tests check under `64 eps * scale`).
   Against: a long net with one far span widens every box by the worst span's rounding.
   Per-span bounds are a later refinement if a measured case needs them.
3. *Quarter-arc turns.* `u` and a conic generator's parameter are the pin's quarter-arc
   parameter, not radians. For: the net is then exactly the definition in the same uv
   (rational quadratic quarters), so the net-or-definition rule needs no reparameterisation,
   and the torus already uses it (63 D13). Against: a caller with an angle must convert
   (`Arcs`), and the generator's own uv for a circle is not the curve's radians.
4. *A parabola or hyperbola generator is refused* (`ProceduralError::Generator`). For: each
   has an exact rational quadratic arc, but over a parameter that is not the curve's; mapping
   it is the same work as FT's conversions, and no step-6 row needs it yet. Against: a
   revolved parabola (a paraboloid) is a common surface. Converting the generator to NURBS
   first (`to_nurbs`) is the workaround until a row takes it.
5. *Net-or-definition by the net's box (`net_over`), not by bound against the query's zone.*
   77 §3.3 reads the net when its bound is within the query's zone. A PK1 net's bound is a
   few roundings, inside every zone, so the zone test would always pass and only the box is
   left to decide. For: one test, and the bound holds exactly there. PK2 and PK3b, whose
   bounds are real approximation errors, add the zone test. Against: a query just past the box (a periodic `u` past one turn) falls to
   the definition's slower enclosure. `to_nurbs` rebuilds over the covering quarters instead.
6. *`range_along` for a NURBS generator past the net: partials are `Ivl::whole()`.* For: the
   range itself is sound. Against: callers that read the partials get nothing; none does
   today.
7. *A non-similar map refuses a revolution (`NotSimilar`)* instead of 04 row 105's new
   `Procedural` or certified `Nurbs`. For: the torus does the same today, and the
   certified-record path is not built. Against: the caller loses the surface, not just
   the definition. The net transformed (exact for an affine map) is the follow-up.
8. *`Procedural::new` does not turn itself into an analytic kind.* A line coplanar with the
   axis is a cone, and a circle centred on it is a sphere. 77 §3.3's "analytic first" is the
   constructors' job (MK4), not this type's. For: a value is what its caller built, and the
   tests compare the two. Against: a caller can hold a procedural cone, which the closed-form
   SSI then refuses.
9. *`axial` answers `NotRevolved` for a revolution.* For: `axial`'s consumers want an
   analytic kind's frame. Against: a procedural revolution is revolved; when a consumer
   needs it, `axial` can read `Revolution::{origin, axis}`.

Tests (`procedural.rs`, 12). The three pin ports, from `surf_of_revolution/p1.rs`:

- :2349, `a_line_revolved_is_a_cone`;
- :2398, `the_sweep_iso_line_is_a_circle`;
- :2427, `a_generator_turns_and_a_copy_is_equal`.

Closed-form truths:

- an off-axis circle revolved is the torus, point for point against `Torus`;
- a skew line revolved is the hyperboloid `x^2 + y^2 - (z tan a)^2 = r^2`;
- an ellipse extruded is the elliptic cylinder.

The rest:

- the net against the definition within its bound, on a grid;
- the jet against the point by differences;
- boxes and hulls holding sampled points;
- `to_nurbs` inside and past the net;
- reversal and transform mapping every point;
- inversion finding points on the surface;
- typed refusals.

Measured: slice benchmarks, scx side only, as callgrind instruction counts (`slice_ic`), main
before against this branch. Nothing builds a `Procedural`, so the counts measure only what the
new arm adds to every existing kind's dispatch. The procedural paths sit in out-of-line
`#[inline(never)]` helpers. Every change is within code-layout noise (±0.3 percent), in both
directions:

| case | before | after |
|---|---:|---:|
| build/box-cyl-hole | 2,040,766 | 2,039,955 |
| parts/box-cyl-hole | 363,378 | 362,377 |
| mass/box-cyl-hole | 6,280,814 | 6,281,239 |
| mass_full/box-cyl-hole | 15,693,378 | 15,699,890 |
| classify/box-cyl-hole | 3,486,645 | 3,487,841 |
| tess/box-cyl-hole | 903,855 | 901,759 |
| wire/box-cyl-hole | 133,543 | 133,527 |
| make/torus-5-1.5 | 146,273 | 146,527 |
| tess/plate-4 | 5,029,301 | 5,014,011 |

### 13.30 MK16 as built (11-28)

Tests only: `crates/scx-make/tests/closed_form.rs` (7 tests), the closed-form make table over
`extrude`, `revolve`, `sweep` and `sheet`. Every case asserts volume and area (relative 1e-9, the
estimate's error within it), the first moment where the truth has one, face, curve-edge and vertex
counts, the Euler-Poincare count, regions and shells, the fin structure and the check contract
(`violations` empty). `sweep` is called through one helper (`sweep_along`), so MK7b's `&PathFrame`
changes one line. Nothing in `scx-make` changed.

Truths come from sources that share no code with the constructors: T9 (`scx_testkit::solid`:
extrusions, cylinder, cone, sphere, torus and torus sectors, carried to the case's frame, which is
also how a revolve's axis `y` and turn direction are matched), Pappus (T9's extrusion of the
profile gives `A` and `A x`; `int x ds` is chord length times mean `x` and the arc's
`r (cx t + r (sin a1 - sin a0))`, in the file), and the mitre argument (a profile whose area and
rim are centred on the path keeps area and perimeter times length through any corner of lines).

Tables:

- **Ops table** (the pin's 26 cases; 12 built here, one test): rectangle and circle extrudes, the
  circle in two halves and in a quarter and the rest, the half disc with a split arc, the ring
  section revolved whole and by a quarter, a circle along two lines end to end, along a quarter arc
  (T9's torus sector) and along a line then a tangent quarter arc (T9's cylinder plus a placed
  sector), a 4 x 3 rectangle round the closed rounded rectangle (9 pieces, no caps), and the planar
  face with a hole. Not ported, each waiting on a task: the drafted extrudes (5; a draft is
  `scx-taper`'s, step 8), the lofts (5; MK10 to MK12), the scaled stations (2; MK7b's laws) and the
  screw sweeps (2; MK6).
- **Revolve against T9** (plain and tilted frame): cylinder, cone, frustum, sphere, ring torus and
  its sectors at pi/2, pi and 3 pi/2, with first moments.
- **Pappus** (plain and tilted frame): nine profiles (rectangle, trapezoid, L, triangle, half disc
  off the axis, rounded rectangle, circle, rectangle with a round and with a square hole) by four
  turns (whole, a quarter, from -1 to 2.5, almost whole). Counts follow the rule stated on the test:
  a face per segment (and two caps), a whole turn a circle edge and vertex per profile vertex and a
  seam per segment but level lines, a partial turn three edges and two vertices per profile vertex.
- **Cornered**: five profiles (square, rectangle, circle, rounded rectangle, hexagon) along an L
  of 30, 60, 90, 120 and 150 degrees in three bending planes (two axis planes and an oblique one):
  T9's straight extrusion of the same length, counts `2n + 2, 5n, 3n` faces, edges, vertices (a
  circle 4, 5, 3), the vertices of each section on its plane (start, bisecting mitre, end) and the
  caps square to the end tangents through the end points.
- **Stations**: a line, a quarter bend and a line, for a circle and a square: the wall kinds per
  piece (cylinder, torus, cylinder; planes and cylinders), Pappus's volume, and each cap at the
  closed-form station. The pin's helix-frame truths (`sweep_stations.rs`) are MK7a's
  `tests/frames.rs`; a helix sweep is `Unsupported::HelixPiece` here.

Result: every case holds; no finding, nothing `#[ignore]`d. The two mistakes made on the way were
the test's own (a hole drawn counter-clockwise gives T9 a positive area; a full turn's seam count
excludes the level lines, whose plane annulus has none) and are in the rules above.

Runtime (debug, nextest, loaded machine): the four heaviest tests 1 to 5 s each, all 7 in about
5 s wall; the Pappus tables are 144 bodies, the cornered table 75.

### 13.31 VM1 as built (11-28)

77 §5's volume maps on their own, without DF1's deformation: `scx_surface::volume_map`, a new module (two lines in `lib.rs`). The public API:
- `Bend::new(Frame, neutral: Length) -> Result<Bend, MapError>` (positive and finite) and `Twist::new(Frame, pitch: Length) -> Result<Twist, MapError>` (signed, nonzero, finite; negative is left-handed). The frame's origin is on the axis, its x on the mid direction, its z along the axis.
- `VolumeMap::{Bend(Bend), Twist(Twist), Unbend(Bend), Chain(Arc<[VolumeMap]>)}`; a chain applies its maps in order.
- `map<T: Polar>(&self, Point3<T>) -> Point3<T>`: the closed form in `f64` or `Ivl`. `Polar` is a sealed `Trig` with `atan2`.
- `bound(&Extent3d) -> Option<Extent3d>`: `map` over `Ivl`.
- `inverse(&self, Point3) -> Result<Point3, MapError>`: closed form. `MapError::{BadNeutral, BadPitch, NotFinite, OnAxis, OutsideImage}`.
- `inverted(&self) -> VolumeMap`: bend and unbend swap, the twist's pitch is negated, a chain is reversed.
- `push(&self, p, t) -> Vec3` and `jacobian(&self, p) -> [Vec3; 3]`: closed-form first derivatives, chained through a chain.
- `image(&self, &Surface, near: Point3) -> Image`, with `Image::{Exact { surface, uv: Uv }, NotExact(NotExact)}`, `Uv::{Same, Moved}` and `NotExact::{NoRule, MeetsAxis, OnCut, OutsideDomain}` (non-exhaustive).

The image rules, in the map's frame. Each is a test on kind and frame within `ALIGNED` (16 eps, relative to the kind's size), never a sample:
- Bend:
  - a plane square to the axis is itself;
  - the plane `u = u0 > 0` is the cylinder of radius `u0`, Forward when its normal is `+x`;
  - the plane `v = v0` with `|v0| <= pi d` is the plane through the axis at `v0 / d`, with the same uv;
  - a plane with normal `(a, 0, c)` is the cone with its apex on the axis, opening along the half where `u` grows;
  - a cylinder along `y` centred at `u_c > rho` is the torus (major `u_c`, minor `rho`), and one centred on the axis is the sphere of radius `rho`. The sense is kept: the bend keeps orientation (its Jacobian determinant is `u / d > 0`).
- Unbend: the inverse of each bend rule. The half-plane of a plane holding the axis, and the nappe of a coaxial cone, are the ones `near` lies on. A half-plane at `pi` is `OnCut`.
- Twist: a plane square to the axis turns, with the same uv. A coaxial cylinder, cone or torus, and a sphere centred on the axis, are themselves (the pin misses the cylinder and cone).
- A chain carries the surface and `near` through each step. Its uv is `Same` only when every step's is.
- Everything else, NURBS included, is `NoRule`: PK4's in-volume kind or a certified fit.

Decisions.
- **`map` is generic over `Polar`, not `Ranged`.**
  - For: the unbend needs `atan2` and a square root, which `Ranged` lacks; `Trig` has the root but no angle. A sealed subtrait keeps the set to `f64` and `Ivl`.
  - Against: 77 §5.2 wrote `T: Ranged`; a jet ring cannot run the map until it gets `atan2`.
  - The interval `atan2` takes the four corners' angles, which bound a box off the origin and the cut. A box meeting the cut, holding the origin or holding a NaN gives `[-pi, pi]`. The twist over intervals clips `x` and `y` to the box's widest radius, which keeps a full turn within `sqrt 2` of the axis for the pin's unit box (dependency in `cos u - sin v` would otherwise double it).
- **`image` takes `near`, a point of the face.**
  - For: the unbend of a plane holding the axis is two planes, `v = d theta0` and `v = d (theta0 - pi)`, and a coaxial cone's two nappes unbend to two planes. Without a point there is no single answer. A chain maps `near` with the surface.
  - Against: 77 §5.2 wrote `image(&self, s)`; every caller must pass a point. DF1 has the face, so it has one.
- **`Image::Exact` carries `Uv`, and a not-exact image is `NotExact`, not `Procedural` or `Refused`.**
  - For: VM2 and DF1 must know whether the pcurves carry over. That is true only for the plane rules that are rigid motions of the plane (bend `v = v0`, twist and unbend planes). Building PK4's in-volume surface belongs to PK4, which is not built yet. A volume map refuses nothing about a surface; it only lacks a rule.
  - Against: the caller matches one more level; PK4 must add the `NotExact -> InVolume` step itself.
- **The bend's oblique plane is a cone at its apex.** The cone kind is both nappes, so any reference circle gives the same set. Placing it at the apex with the axis toward the image's nappe keeps the radius positive there and so the sense right without a point.
- **Derivatives are first order and closed form (`push`, `jacobian`), not jets.**
  - For: DF1's normals need `J^-T n`, which these give exactly. The finite-difference truths are the pin's.
  - Against: 77 §5.2 wants the map over jets so WR-45 (third order) cannot recur. That needs a jet ring with `Polar`; it is left open.
- **Tolerances.** `ALIGNED` (16 eps) for the image rules. `ON_AXIS` (4 eps of the point's size) for the bend's inverse. `CUT_MARGIN` (4 eps of `pi d`) past the bend's range for the unbend's inverse. A point on the cut (`y = +0`) inverts to `v = +pi d`, as the pin's first solution.

Tests (17, `volume_map/tests.rs`):
- The pin's truths: the simple maps, the maps in the frame at (1, 2, 3), the seam at `y = 0`, the finite-difference Jacobians, the bend box at `[1, 2] x [0, pi/2] x [0, 1]`, WR-59's boxes with their corner and pole, full periods, and the twist's full turn within `sqrt 2`.
- Map then inverse is the identity for every map in three frames, a chain included. The tolerance is `1e-13 (1 + d/u)`: the unbend's angle near the axis is conditioned by `d / u`.
- Every box holds a 7^3 grid of mapped points, for each map and a chain, across the cut too.
- Each image rule, both senses where a sense exists: the mapped points lie on the image within `1e-11` relative, by the image's own closed-form inversion. The image's normal there agrees with the source normal pushed forward, `J^-T n`. `Uv::Same` images give the mapped point at the same uv.
- `NotExact` cases: `MeetsAxis`, `OutsideDomain`, `OnCut` and `NoRule`. Construction refusals.

Costs, release build on the build server, throwaway timer, per point (map, inverse, box, Jacobian):
- bend: 43 ns, 417 ns, 399 ns, 145 ns;
- unbend: 388 ns, 42 ns, 420 ns, 50 ns;
- twist: 33 ns, 34 ns, 413 ns, 107 ns;
- a chain of a bend and two twists: 128 ns, 319 ns, 1.1 us, 771 ns.

The correctly rounded `fmath::atan2` is about 350 ns of the unbend's map and the bend's inverse. Against the pin's 36 samples per exactness test and a guess and point solve per inversion, every row is less work (77 §9): an image is one predicate and an inverse one closed form.

Deviations from 77 §5.2: `Polar` for `Ranged`; `image` takes `near`; `Image`'s shape (above); first derivatives, not jets. Not built: curve images (VM2) and DF1's `MeetsAxis` refusal of a body.

### 13.32 RY2 as built (11-28)

`scx_surface::intersect_curve` and `curve_on_surface` now answer a line, circle, ellipse, parabola or hyperbola against a `Surface::Nurbs` (77 §6.3, row RY2) in `crates/scx-surface/src/nurbs_hits.rs`; a NURBS curve against one is still `SurfaceError::NurbsIntersection`. The one public addition is `SurfaceError::Undecided { t }` (Unsupported, `intersection_undecided`): a box down to the floor that nothing dropped or proved. `scx-bodyq`, `scx-boolean` and `scx-hlr` read it as they read `NurbsIntersection` (unclean: a ray tries its next direction). `intersect.rs` gains two dispatch lines and nothing else.

How: each Bézier patch's certified hull grown by `tol` is a box in a balanced tree, culled against the curve's slab; on a surviving patch a box in `(u, v, t)` is enclosed by the patch polynomial about the curve's middle point. Where `g = (S_u x S_v) . C'` excludes 0, Krawczyk on `F = S - C` drops the box or proves one crossing. Where `g` holds 0 (grazing), Krawczyk on the foot system `G` proves one foot for every `t` in `T`, and the signed distance `delta(t)` along it is cut into far, monotone (one crossing at most) and turning pieces. Runs of pieces within `tol` read as the analytic arms do: Crossing, Tangent or Overlap.

Decisions.
- **`Undecided` its own variant, not `NurbsIntersection`.** For: the claims differ ("this pair is not built" against "this box could not be decided near `t`"), and `t` says where. Against: one more arm for callers; three callers map both the same way today.
- **A ray exactly through a pole is Undecided.** The foot system is singular at a degenerate edge (`S_v = 0`), so no Krawczyk box contains it; answering it would need the pole as a special point. For: unclean is honest and the ray caller already retries. Against: an exact pole hit is answered by retry, not here.
- **The foot proof before the floor check, about the Newton foot.** A grazing box at the floor is tried for its foot first, over a box about the Newton foot of the middle point (hulled with `U x V`), then over `U x V` grown by an eighth; a proved foot box is re-proved over its own growth while that halves it. For: the plain grown box is loose near a tangent (the cylinder tangent and the ruling were Undecided without it). Against: up to four more Krawczyk calls per grazing box.
- **Each piece of `T` searches its parent's foot box grown, not the whole `Y`.** The one foot over the parent lies in its proved box, so the child's foot does too. For: the ruling's two wide feet calls cost 13.5k and 32.8k pieces before; the whole ruling line is 1407 pieces now. Against: none found; it is a smaller box with the same claim.
- **A turning piece is one where `delta` is within `tol` throughout or the piece is at the floor**, not one where the slope is small: the slope enclosure is loose, and "small" spanned whole monotone pieces.
- **A box inside one already proved is skipped.** For: the patch's grown neighbours re-find the same foot. Against: a list scan per grazing box (short: one entry per proved foot).
- **No box tree over patches beyond the hull tree.** The counts below grow with the patches the ray meets, not the patch count; a tree would add build cost for no hull tests saved.

Added work per ray (release, tol 1e-6, throwaway counters, not committed; before, every such call was `NurbsIntersection` and did no work). The 24 sphere lines, the sphere refined by knot insertion in both directions:

| patches | hull tests | boxes | foot Krawczyk | `F` Krawczyk |
|---|---|---|---|---|
| 8 | 12 | 254 | 116 | 80 |
| 15 | 21 | 232 | 99 | 79 |
| 35 | 38 | 145 | 42 | 61 |
| 99 | 63 | 96 | 14 | 49 |
| 323 | 114 | 45 | 5 | 23 |

Hull tests grow with the patches the line passes near (about the square root of the count), and boxes fall as the patches flatten, so hull tests plus boxes per ray stay between 159 and 266; the hull tree is enough, and no further box tree is built. Grazing cases: the cylinder tangent takes 5 to 7 hull tests, 94 to 114 boxes, 68 to 101 pieces and 498 to 630 foot Krawczyk calls; the ruling 145 boxes, 1407 pieces and 4485 foot calls (47.6k pieces before the per-piece foot box, 897k before the tightening); the pole ends Undecided after 52 boxes.

Truths (`crates/scx-surface/tests/nurbs_hits.rs`, 7 tests): 24 lines through the NURBS sphere meet it at `|p| = r` (48 hits); a line through the seam and the knot lines meets each place once; a line tangent to the NURBS cylinder (on the seam, on a knot line, skew) touches it once as a Tangent; a line along a ruling of the bilinear saddle `z = uv`, clipped, is one Overlap and a crossing elsewhere is a Crossing at `u = 5/6`; a circle crosses the sphere twice; a NURBS curve is `NurbsIntersection`, and the budget and stop end the search typed; a line through a pole is Undecided or right. Changed: `nurbs_budget_and_stop_are_typed` in `tests/intersect.rs` (a circle on the NURBS sphere is now answered, empty; a NURBS line keeps the error). Nothing removed.

### 13.33 BO14 as built (11-28)

`scx_boolean::section(body, by: &Surface, cx) -> Result<Done<NoItems>, BooleanError>` and
`face_section(body, face, by, cx)` (`crates/scx-boolean/src/section.rs`), with
`SectionUnsupported::{Surface(SurfaceKind), Wire, Mixed, InPlane(FaceId), TouchBesideSheet}` and
`SectionRefusal::NoFace(FaceId)` under `BooleanUnsupported::Section` and
`BooleanRefusal::Section`.

- *The plane is a solid operand.* The half-space behind it, cut to a block: its face on the plane
  reaches twice the body box's half-diagonal past the box's centre, and its other five faces lie
  at least that far from the box, so stage 1 pairs only the plane's face with the body's faces. A
  plane farther than half-diagonal plus zone from the box's centre misses: a body of nothing.
- *A solid* goes through `combine` with no cell solid and `Faces::Where` keeping a piece whose two
  sides together lie in both operands and neither side is inside the block alone or with the
  body on its other side: the plane's pieces inside the body and the body's faces lying in the
  plane (either sense), never the body's faces behind the plane. The result is a sheet (void on
  both sides, free edges; scx-check's class `Sheet`). Where it keeps no face (the plane only touches
  along an edge, a tangent line, or at a vertex or tangent point), the result is the wire of
  stages 1 to 4 (BO18's writer, `intersect::wire`), so the closure's wires and points that 13.22's
  decision 3 left out are here. `run` is split into `staged`, which also returns stages 1 to 4,
  so this costs no second pass.
- *A sheet*, and *one face* of any body, give that wire directly: `interfere_only` keeps the
  stage-1 pairs of one face, then stage 4 and the writer.
- *Lineage.* The body is operand 0; the block is operand 1, and only its face on the plane can be
  an origin.

Tests (`crates/scx-boolean/tests/section.rs`, 15, every truth a closed form): a 10 mm box by x = 5,
area 100 and four edges of 10 on both the plane and the box (the pin's
`box_section_is_four_lines_ten_long`); the cube's diagonal hexagon, area (3 sqrt 3 / 2) s^2 with
s = 5 sqrt 2; a sphere at three heights, pi (r^2 - d^2) and boundary 2 pi sqrt(r^2 - d^2) on the
sphere; a cylinder across (pi r^2), through its axis (2rh) and at a slant (pi r^2 / cos theta); a
frustum across its axis; a torus at d = 0 and 0.5 (4 pi R sqrt(r^2 - d^2)) and through its axis
(two discs, 2 pi r^2); a miss; a plane on a face (that face, both senses); touches (a sphere's
pole as one vertex, a box edge as one line of 10, a box corner as one vertex, a cylinder's wall
line of h); a sheet's section as a wire (a line of 10, a chord of 2 sqrt 3); one face of a box at
z = 5 (four sides one line of 10, top and bottom empty: the pin's
`a_box_s_record_faces_section_at_their_index`, whose zero-normal case is the frame's own refusal
here); a sphere's face, its great circle; refusals (a sphere surface, a missing face, a sheet in
the plane); the touch beside a sheet; determinism. The pin's wound-NURBS test waits on SM9.

Work on the regular path: `run` returns what `staged` built and `interfere_given` delegates to
`interfere_pairs` over the same pairs, so no stage does more. A throwaway probe over 15 Booleans
and 5 intersections (box, cylinder, sphere, torus pairs) gave the same `Report::work` and the same
answers on main and the branch: box-box 121 per Boolean and 42 for `intersect`, box-cylinder 41
and 12, box-sphere 41 for `intersect`, total 486 over the Booleans that build. Its box against a
sphere centred on the box's corner is `Defect(Seeds)` on main and the branch alike (open).

Decisions (analysis and adversarial pass):

1. *The tool is a block, not a new half-space operand kind.* For: R1, the cut is the tested
   pipeline with nothing of its own; stage 1's boxes drop the five far faces; the BO25 test
   `sections_by_a_plane` already built a plane's section this way. Against: six faces and twelve
   edges built per call (microseconds), and a size read from the box: a body whose box
   understates it would be cut by a face too small. The box holds every face's box, so it does not.
2. *A touch beside a sheet is `Unsupported(TouchBesideSheet)`.* Where the plane crosses one solid
   and touches another part of the body (a box's top face and a ball's pole on one plane) the
   closure is a sheet with a point or wire apart from it. scx-brep holds lone vertices and wires
   only in shells of their own, and the sheet and the wire would need one builder. For: the answer
   is never a quiet drop. Against: such bodies (several lumps, a face flush with a tangent
   feature) are real; the check samples each tangent curve, lying edge and touch at one point
   and asks whether a sheet vertex or edge holds it, so a tangent curve that meets the sheet only
   at its middle would pass. Open: build sheet plus wire in one draft when a caller needs it.
3. *A sheet in the plane is `Unsupported(InPlane)`, not the sheet itself.* For: a sheet's section
   is a wire by contract; a face lying in the plane is two-dimensional. Against: the closure
   there is that face; a caller can test coplanarity first.
4. *Curved cutting surfaces wait.* `SectionUnsupported::Surface(kind)`: a tool body bounded by a
   cylinder or sphere needs the far faces' clearance argued per kind; NURBS waits on SM9 anyway.
5. *A wire body's section (points) is `Unsupported(Wire)`.* Stage 1 pairs faces; curve against
   surface is `scx-ssi`'s and has no Boolean-family entry yet.

### 13.34 FT3 as built (11-28)

`crates/scx-nurbs/src/fit` (77 §2, row FT3; the pin's `refit_within` and `approx_end_directions`), with the one-variable jet moved down to `scx-num`:

- `fit_within(&impl CurveFn<D>, &FitAim<D>, &mut Budget, &Stop) -> Result<Fitted<D>, FitError>` (`within.rs`): the clamped spline of `aim.degree` through the source's end points, leaving each given end along its direction, with the fewest spans the pin's halving search finds whose certified same-parameter distance `sup |F(t) - C(t)|` is within `aim.within`. `FitAim { within, degree, start, end }`; `Fitted { curve, spans: Vec<SpanBound>, bound, fits, start: Option<Held>, end: Option<Held>, ended: Ended }`; `SpanBound { span, bound, method: Method }`; `Held { reach, turn }`; `Ended::{Within, Cap, Floor, NoRoom}`.
- `CurveFn<const D>`: `domain`, `breaks` (where the source is less smooth; pieces are cut there), `eval<T: Trig>(t: T) -> [T; D]` (one code path for `f64`, `Ivl` and `Jet2`), `as_nurbs` (default `None`). `NurbsCurve<D>` implements it through `point_over` and its knots.
- `least_squares_along(knots, data, t, start, end) -> Result<Directed<N>, FitError>` (`along.rs`, the pin's `approx_end_directions`): least squares through the end data, each held end's leg along its unit direction, its length (the *reach*) free; `Directed { spline, start_reach, end_reach }`.
- `scx_num::jet::{Jet2, chord, norm, square, vector_bound}` (`jet/interval.rs`): SU6's interval jet `(f, f', f'')` with `Ring`, `Ranged` and `Trig`, and its chord bound, moved out of `scx-surface/src/gap.rs` (77 line 50). `gap.rs` now imports them; SU6's arithmetic is unchanged (the same fold order), so its bounds are the same bits.
- Named constants: `MAX_CONTROL_POINTS = 4096` (the cap; the pin's `most`, which its one caller passes as 4096), `MAX_PIECES = 256` (per span in the branch and bound), `CLOSE = 0.125`, `FLOOR_UNITS = 4.0`. No `1e-4` or other tolerance literal.
- Typed causes (new `FitError` variants, all `Refused` unless noted): `Aim`, `Degree`, `Directions`, `Direction`, `StartAgainst`, `EndAgainst`, `Domain`, `SourceNotFinite`, `Reaches`, and `Deviation`, `Exhausted`, `Stopped` carrying FT1's and the budget's causes with their kinds.

Decisions.
- **The pin's search, every bound certified.** One knot a round at the middle of the span with the largest bound (the first or last span while a held end's reach is not positive); samples only seed the least-squares fit, never accept it (04 fit rules, R10). The certificate per span: FT1's Bezier-hull `distance_by_span` for a polynomial B-spline source; for a rational one FT1's bound and, where that is past the aim, SU6's branch and bound too (the smaller kept); for any other source the branch and bound (chord bound on `g = F - C` and `|g|^2`, and the direct enclosure, the smallest, from `Jet2`s over each piece; the worst piece halved, a child never above its parent).
- **Knots nest across aims, by construction.** A span past the aim carries a bound that does not depend on the aim (FT1 and the branch and bound are deterministic, the heap's top is monotone, and the branch and bound stops early only on reaching the aim); a span within it carries one at most the aim. So the span halved is the same for every aim, and a looser aim's fit is one the tighter search passes through. Tested to the bit.
- **Rational sources: tighten, not subdivide; ranked by FT1.** FT1's rational bound is about 5 times loose on a circle's coarse spans and about 2 times on fine ones, where the weights hardly vary. For tightening: the branch and bound is already here for general sources and comes within `CLOSE` of the true distance. Against subdividing FT1 (splitting the source's spans so the weights vary less): no new code and cheaper per span, but its looseness falls only as the square of the split and never reaches the distance on a span of large weight ratio. The search halves by FT1's bound alone. The branch and bound only tries to end it sooner: it takes the spans FT1 leaves past the aim, closest first, and stops at the first it cannot bring within. It gives up on a span at once when `|g|` at a point is past the aim, or FT1's bound is within `CLOSE` of that `|g|`. So the fits are FT1's own sequence cut short, and the knots still nest. Running the branch and bound on every span past the aim every round (the first build) gave the same spans at 1e-6 and 1e-8 for 4 to 5 times the evaluations. Halving cuts a span's distance about 16 times, so a 2 times tighter bound rarely saves a span. Counts below.
- **The jets of a B-spline over a piece are in Taylor form**, about the piece's middle (`fit_over`: `C(m + s) = sum C^(k)(m) s^k / k!`, each derivative enclosed at the point, Horner in `Jet2`), not `point_over`'s basis form.
  - The basis form sums the absolute control points times the basis jets. Over a piece of width `w` in a span of width `h`, its second derivative is about `|P| w / h^3` too wide, so the chord bound's excess is about `|P| (w / h)^3`.
  - With at most `MAX_PIECES` pieces a span, that left a floor near 1.4e-7 on a circle of radius 10 that halving the spans did not lower. A tight aim ran to the cap, while the fit's true distance was 2e-15.
  - In Taylor form the excess is about `w^3 |C'''|`, the curve's own. One span of a cubic proves 6.8e-7 at width 3, 8.5e-8 at 1.5, 1.1e-8 at 0.75 and 1.3e-9 at 0.375 (true distance 2e-15).
  - A rational source is taken as `N / w`, each in Taylor form, divided in `Jet2` (`Homogeneous`). The fit's degree up to `TAYLOR_DEGREE = 18`; above it, the basis form.
- **A rounding floor for general sources.** No fit's branch and bound comes under half the width of the source's own enclosure at an end of the domain: the ends are breaks of every fit, and `g` is at least as wide as `F`. An aim under that stops after one fit (`Ended::Floor`), as the pin's floor does for a polynomial source; between it and the span-level `FLOOR_UNITS` rule, a tight aim runs on.
- **Variable projection for the reaches**, not the pin's dense normal equations of the whole system: the free points come from FT2's banded rotation solve, the two reaches from a 2 x 2 system on the residuals, so the condition stays the problem's.
- **`ends_held` and `EFF_ZERO_RAD` dropped.** Each held end reports its reach and the turn of its stored leg (`Held`); the caller who knows its angle judges them.
- **`within` is an `f64`**, not a `Length`: `D` is generic (2D pcurves are parameter-space).
- **`SpanBound` carries `method: Method`, not 77's `basis: Basis`.** The fitter does not know the caller's claim, and every bound it gives is proved; 77's "unproven" becomes a bound past the aim plus `Ended`.
- **`CurveFn::eval` bounds by `Trig`**, not 77's `Ranged`, so analytic sources use `sin`/`cos`.

Tests (`fit/within_tests.rs`, 12; the pin's within.rs tests ported and judged by closed-form truth, `|F - C|` at 65 points a span against each span's bound):
- 77's row:
  - `a_source_of_one_cubic_span_is_given_back_in_one_fit` (pin :554): one fit and one span, free, held at either end or both; also as a closed form proved by the branch and bound.
  - `closed_forms_are_refitted_within_a_ladder_of_aims`: circle, ellipse, helix, involute and parabola, free and held. Each bound holds and is within its aim, and the looser aim's knots are among the tighter's.
  - `two_halves_of_an_arc_refitted_with_the_shared_tangent_meet_g1`.
- The pin's: `pin_sources_are_refitted_within_a_ladder_of_aims` (:595), `the_cap_gives_the_last_fit_past_the_aim` (:674), `a_quadratic_with_both_directions_starts_on_two_spans` (:689), `a_direction_of_any_finite_length_is_read_as_its_unit` (:711) and `an_aim_below_the_floor_stops_after_one_fit` (:639; also a closed form under its rounding floor).
- New: `a_rational_circle_is_refitted_within_its_aims`, `jets_of_a_bspline_over_an_interval_enclose_its_derivatives`, `least_squares_along_gives_a_cubic_back` and `hostile_input_is_refused_never_a_panic` (every typed cause, the budget and the stop flag).
- No test removed; SU6's gap tests pass unchanged on the moved `Jet2`.

Counts (release, cubic, aims 1e-2 / 1e-4 / 1e-6 / 1e-8; spans; evaluations per fit, a point or a piece each; free ends; held ends within 1% of these):

| source | spans | evaluations per fit | ms at 1e-8 |
|---|---|---|---|
| circle r 10, `[0, 2]` | 4 / 8 / 32 / 128 | 73 / 175 / 766 / 4208 | 981 |
| ellipse 10 x 4, `[-1, 2.5]` | 5 / 16 / 64 / 128 | 99 / 328 / 1534 / 4585 | 1098 |
| helix r 5 pitch 2, `[0, 6]` | 8 / 32 / 64 / 256 | 130 / 543 / 1773 / 8627 | 3759 |
| the circle as a 30-point cubic B-spline (FT1) | 4 / 8 / 32 / 151 | 23 / 41 / 149 / 685 | 116 |
| rational half circle r 10 (FT1, ranked) | 10 / 22 / 72 / 263 | 55 / 134 / 352 / 1196 | 1139 |
| same, FT1 alone | 10 / 37 / 72 / 263 | 50 / 172 / 329 / 1189 | 1374 |
| same, branch and bound on every span past the aim | 10 / 22 / 70 / 263 | 120 / 375 / 1209 / 5787 | 4511 |

One fit per span added (the pin's search); every bound `Within` its aim.

Open questions.
- `scx-nurbs` `eval_over.rs` `derivs_over` builds the factor `p! / (p - k)!` in a `usize` and lifts it with `as f64`. It overflows from degree 21, and from 19 the factor is no longer an exact `f64` though it is lifted as a point. `MAX_DEGREE` is 31. FT3 keeps its Taylor form to degree 18; the fix belongs to that file (an `Ivl` factor or a checked product).
- The rational branch and bound's stop (`Enough`) is the only aim-dependent stop. It is sound because a rational search is ranked by FT1's bound, which does not depend on the aim; a pluggable certificate (FT5) has to keep that split.
- SU6 passes `h = b - a` unrounded to the chord bound (a possible half-ulp unsoundness); FT3's own calls round `h` up. Left in SU6 so its bits do not move.
- FT5 needs the certificate pluggable (a surface gap in place of `|F - C|`).
- The search is `O(n^2)` in the spans: each round refits and recertifies every span, one knot a round. The helix at 1e-8 takes 3.8 s for 256 spans. A local update, or several knots a round, would cut it, but the second changes the pin's search and the nesting argument; left for a measured need.
- Pieces that end on a fit knot include the neighbour's jets (`point_over` joins both sides at a knot): sound, slightly loose.

### 13.35 MK4b as built (11-28)

`scx_make::torus` builds a horn torus (radii within the resolution) instead of refusing it;
`Unsupported::HornTorus` is gone from `scx-make`, the testkit mirrors and the recipe tables;
`SpindleTorus` and `Refusal::SelfIntersecting` (minor above major by more than the resolution)
stay. The step-7 audit of what assumed minor < major follows.

- **The build.** As revolve's whole circle that touches the axis: one vertex (the pole at the
  frame's origin), the meridian circle through it (closed there, from tube angle pi once round,
  so `v` runs over `[1/2, 3/2]`), and a collapsed edge of the face's own at each end of that
  `v` range (the inner equator is both `v = 1/2` and `3/2`): 1 vertex, 3 edges, 1 face, exact
  pcurves. The radius is the mean of the two within the resolution, as revolve's. For: it is the
  shape revolve already builds and the check, the pcurve writer and the oracle already read.
  Against: one collapsed edge used in both senses would be a seam, but the check wants one fin
  per collapsed edge.
- **What the check says.** The contract passes (`Contacts::Refused` and `Radial`); the structure
  pass reports exactly one `VertexNotManifold { pieces: 2 }` at the pole and refuses it only
  under `Contacts::Manifold`. The body's genus is 1 (scx and the pin agree; the recipe states it and is `pinched`, so the manifold and mesh invariants are off). That is the truth: two lobes of the solid meet at the pole, a
  pinch. Not changed.
- **Mass.** Needed no change: the breaks put `v = 1/2` on a patch edge, and the volume and area
  match `2 pi^2 R^3` and `4 pi^2 R^2` to `1e-12` (tilted frames too). The testkit truth
  `solid::torus` now takes `minor == major` (it refused it).
- **SSI.** Read for `big - small`, `/ big`, `/ small`: the divisions are by `big` and `small`
  only; the Villarceau case's `cv = sqrt((big - small)(big + small)) / big` is 0 for a horn
  torus, which is the right limit (the two circles become the meridians). No change.
- **Tessellation (broke).** `face_need` refused a horn torus (`major <= minor`) and
  `Rules::of` took `rho = min(r, R - r) = 0`. Edge segments now bound by the tube's curvature
  alone; faces get `Kind::Round { pinch }` and `pinched_sag`: no one radius of curvature bounds
  a horn torus (the parallels' falls to 0 at the pole), so the meridian's bend (the sphere bound
  at the tube radius over the triangle's enclosing circle) and the parallels' radial deficit
  (`2 d sin^2(span / 4)`, `d` the largest circumference radius among the corners) are added.
  A first attempt, the least parallel radius over the corners, never passed the slivers along
  the meridian near the pole and refined to a duplicate point; the split bound fixes it. The
  ring torus path is unchanged (one `bool` test per triangle). The mesh oracle reports exactly
  one `NonManifoldVertex`, the pole, which the test expects for the horn case, and its fold limit
  is off for the horn: the fan round the pole is the tip of two needles (`rho = z^2 / 2R`) and
  turns by under a degree at a chord of 0.001, a property of the surface, not a fold.
- **Boolean (broke, refused).** A cut across a horn torus's face is `Defect(Build)` today: the
  imprint trace keeps one collapsed edge per pole vertex, and a horn pole has two (both lobes).
  It is now `Unsupported(HornPole { operand, face })` (name `horn_pole`) from the trace. A plane
  through the pole is the same refusal (it was `Defect(Seeds)`'s cousin). Follow-up: the trace
  needs the pole vertex's star in two uv sheets (key the poles by the arriving half-edge's `v`
  and order the star per sheet); not built. A horn torus wholly inside or clear of a block or
  cylinder works (union, intersection, difference volumes checked); a ring-torus control of
  the same shape passes.
- **Tests added.** `scx-make` round: `spindle_tori_are_refused_and_horn_tori_built` (replaces
  `..._horn_tori_unsupported`: the old test's three horn radii are now built) and
  `a_horn_torus_has_analytic_properties`; `scx-check` `a_horn_torus_is_a_checked_solid`;
  `scx-tess` curved gains a horn torus in `primitives` (closed, chord, crack and volume checks
  at three chords, two frames); `scx-boolean` horn.rs (four tests). The recipe `torus-horn`
  is `Built` with its closed-form truth; the difftest adapter drops its `HornTorus` arm.

### 13.28 CN4 as built (11-28)

`scx_nurbs_surface::skin` (77 §4.3, row CN4; CN3's `fit::interpolate_rows`, FT2's `interpolate_on` and `averaging`, FT1's `deviation::{distance, interpolation_bound}` reused).

API:
- `skin(&CompatibleCurves<D>, params, degree, &Skin<D>) -> Result<Skinned<D>, SkinError>`; `Skinned { surface, through }`, the sections being the surface's `u` rows at `params` along `v`.
- `Skin::Open { start: Edge<D>, end: Edge<D> }` (CN3's ends) and `Skin::Closed { coincidence: f64 }`; `MAX_CLOSED_DEGREE` = 7.
- `SkinError::{Fit(SurfaceFitError), NotClosed { gap, coincidence }, Coincidence { value }, TooFewSections { count, needed }, ClosedDegree { degree, most }, Seam, Gap(DeviationError), Through(DeviationError), WeightsLost, Defect { why }}` with `kind()` and `name()`; the frames inherit their cause's kind.
- `fit::Edge` gains `Homogeneous { points: Vec<End<D>>, weights: Vec<End<1>> }`; `SurfaceFitError::RationalEnds` is gone.

Decisions.
- **`through` is certified per control column.** `S(u, v_k) - C_k(u) = sum_i N_i(u) (X_i(v_k) - P_ki)` with `X_i` the curve of control column `i`; the basis is a partition of unity, so the largest FT1 `interpolation_bound` over the columns bounds the miss for every `u`. Rational skins bound the homogeneous columns (`dA`) and the weight columns (`dW`) and divide: `(dA + R dW) / (W_min - dW)`, with `R` the sections' reach and `W_min` their least weight, `WeightsLost` if the denominator is not positive. For: exact reasoning, FT1's rounding model, and no sampling. Against: it bounds the control columns, not the surface, so it can be larger than the true miss (rounding is all it measures for exact interpolation, 1e-14 to 1e-12 in the tests). Its cost is below.
- **Rational derivative ends lifted (CN3's `RationalEnds` refusal dropped).** On rational rows `Edge::Held(D_i)` means the boundary derivative is the rational curve of control points `D_i` on the row's own weights: the numerator's end is held at `w_i D_i` and the weights' ends at zero, so `S_v = sum N_i w_i D_i / sum N_i w_i` exactly (and the same for the second and third orders, since `W_v = W_vv = 0`). `Edge::Homogeneous` holds the homogeneous derivatives themselves (the pin's `create_skin_srf_boundary_derivs` meaning): it is what a rational neighbour's net gives across an edge. Polynomial rows under `Homogeneous` get unit weights, so the result is rational. For: both meanings are exact and documented. Held keeps CN3's polynomial meaning unchanged, and only the rational meaning is new. Against: Held cannot express a moving weight. Adversarial check: a caller who passes affine tangents on rational rows and expects the pin's homogeneous meaning gets a different surface, so the two are separate variants with different names, not one variant read two ways.
- **Per-point first derivatives and twist not lifted.** No 77 row needs them (MK12's ends are `Edge`s), and a guessed twist is the failing default 75 §1 forbids. Against: a grid with prescribed slopes still needs the caller to fit the derivative curves.
- **`Skin::Closed { coincidence }` carries the coincidence.** This deviates from 77's bare `Closed`. Layer 2 has no document tolerance to read, and R5 says the caller decides. The gap is FT1's certified `distance` of the first and last section. Bitwise-equal sections are 0 apart without it, since the certified bound carries about 3e-14 of rounding on unit data. A gap above the coincidence is `NotClosed { gap, coincidence }`, never a silent open skin. Within it, the last section is replaced by the first bit for bit, and `through` adds the gap. `f64`, not `Length`: the nurbs crates are unitless.
- **Closed is `C^(q-1)` across the seam, not the pin's C1.** For `r = q - 1`, `a = ceil(r/2)` derivatives are held at the start and `b = floor(r/2)` at the end on averaging knots. Both are the unknown seam derivatives `d_1..d_a`, and they are fixed by the `a` exact conditions `X^(k)(v_m) = X^(k)(v_0)` for `k = b+1..=r`. Superposition gives the solution: one base solve per column, plus `a` one-dimensional unit responses computed once, whose `a x a` residual matrix is inverted once with partial pivoting (`Seam` if it is singular). The last control value is then set to the first, so the seam's rows are equal bit for bit. For: the same smoothness as at every simple knot, no estimated derivatives (the pin's comes from a shifted interpolation), and the cost of an open skin. Against: the knots are the open averaging knots, not periodic ones, so the seam span is clamped (multiplicity `q + 1` at both ends), which is a seam in the knot vector though not in the shape. A periodic knot vector would need a cyclic banded solver that FT2 lacks.
- **Not ported:** the pin's `knot_tol` snapping (CN1's curves are compatible exactly), its weight clamp to `[1e-3, 100]` (a weight that is not positive is refused), its in-place mutation of the input curves, and its `dir` flag (`NurbsSurface::swapped` turns a skin).

Work (release, throwaway timer; `n` control points per section, `m` sections):

| sections | `interpolate_rows` | open skin | closed skin |
|---|---|---|---|
| poly n 8 m 16 q 3 | 13.2 us | 38.8 us (+193%) | 43.9 us |
| poly n 32 m 64 q 3 | 181 us | 592 us (+227%) | 611 us |
| poly n 128 m 64 q 5 | 1.17 ms | 3.93 ms (+235%) | 4.04 ms |
| rational circles m 128 q 3 | 199 us | 665 us (+235%) | - |

- The added work is the certificate: one `interpolation_bound` per control column (two for rational). It costs 1.8 to 2.4 times the fit, because each of those calls builds a curve and FT1's net and evaluates every parameter by de Boor.
- The closed seam adds 1 to 20% over the open skin. That is the gap's `distance`, `a` one-dimensional solves and an `a x a` back substitution per column.
- A shared-basis bound over many columns on one knot vector (one basis evaluation per parameter, the rounding model unchanged) would take most of the certificate's cost away. It is the follow-up if MK10's lofts make skins hot.
- Knots added against the pin: none. The `v` knots are averaging knots on the given parameters (the pin's), and `u` is CN1's exact union.

Tests (`skin_tests`, 13; truth is a closed form):
- Equal circles at 2, 4 and 7 heights on degree 1 give the cylinder.
- A radius linear in height gives the cone at degrees 1 to 3.
- Radii `1 + z/2 - z^2/5` at `z = 0, 1, 3` on degree 2 give the surface of revolution of that law. So do five sections at degrees 2 and 3.
- These tests hold to `1e-12` at 400 random points, with `through` at least the sampled miss and at most `1e-11`.
- A polynomial ring of 9 sections at degrees 1 to 5 and a torus of 13 rational sections at degrees 1 to 7 are closed: the seam rows are bitwise equal, and every column's derivatives up to `q - 1` agree across the seam to `1e-9` relative.
- A ring whose last section is lifted by `1e-10` is accepted within `1e-9`, and `through` covers the miss.
- Lifted by `1e-6` or `0.5` against a coincidence of `1e-7`, it is refused `NotClosed` with a gap in `[h, h + 1e-13]`.
- Rational `Held` tangents `(c x_i, c y_i, h)` on two circles at degree 3 give the cone, and `c = 0` gives the cylinder.
- A `Homogeneous` quarter-circle derivative on two polynomial segments gives the rational quarter cylinder (`|xy| = 1` to `1e-14`).
- `Homogeneous` with zero weight derivatives equals `Held` on polynomial rows.
- Ports of the pin's `srf_adv` cases, judged against the closed form: `skins` (the lines give the bilinear patch, and `S(1, 1) = (1, 0.3, 3)`), `interpolates_cross_sections`, `derivative_strips_add_rows` (6 columns, with the tangent held), and `rejects_bad_input` (degree 5 on 4 sections, and a wrong parameter count).
- One test checks the typed refusals and that the names are unique.

Removed, on purpose: the `RationalEnds` assertion in `fit_tests::refusals_are_typed_and_leave_the_inputs`. It proved that derivative ends on rational rows were refused. It is now a success assertion (the result is a rational surface), and `skin_tests` checks the meaning.

### 13.36 FT4 as built (11-28)

**Built.** `crates/scx-surface/src/gap/nurbs.rs` is the NURBS arm of `curve_surface_gap` and
`pcurve_edge_gap` (77 FT4). It runs through the same branch and bound (`search`) as the analytic
kinds, with two `Field`s: `PcurveOnNurbs` and `CurveToNurbs`.

- The surface over a uv box is cut at the knot lines inside the box. Each part is enclosed on its
  own polynomial piece by `NurbsSurface::partials_over_piece` (NS4) about the piece's middle, and
  the parts' enclosures are hulled.
- A pcurve's derivatives come from the chain rule over the box. The piece is then bounded as the
  analytic pcurves are (`vector_bound`).
- A curve against the surface takes each point's distance from `nurbs_invert::invert`'s certified
  nearest point. A piece is bounded through a uv correspondence: the two feet joined linearly, or
  either foot held. The smallest of the three bounds is kept.

API changes:
- `GapError::Pending` and `scx_surface::Pending` are removed workspace-wide. With them go
  `scx_check::Undecided::Pending` (and the `Pending` re-export) and `scx_step::Waits::NurbsGap`.
- `Short::Defect(&'static str)` is added: the nearest-point search inside the gap failed, which
  is the kernel's own fault (kind `Defect`, name `gap_defect`).
- `scx_bodyq::curve_lies_on_face` takes NURBS target faces. Where it had answered
  `Unsupported(Nurbs)`, it now gives the certified gap, and a gap defect maps to
  `Defect(Surface { face })`. Inside the trim it is `Undecided` until a NURBS surface has
  pcurves.
- `GapError::Procedural` is added (kind `Unsupported`, name `gap_procedural`), with
  `scx_check::Undecided::Procedural { edge, face }`. Main's PK1 had sent `Surface::Procedural`
  through `Pending` too. Its gap would be its exact net's, widened by `Procedural::bound`, but
  that needs the aim adjusted for the widening and the net's box checked against the face's
  domain, so for now it is refused by name. `curve_lies_on_face` keeps PK1's
  `Unsupported(Nurbs)` for a procedural face, now through the gap's error.

**Choices, with the case for and against.**

1. **Hull per knot piece, not one enclosure of the whole box.**
   - For: `partials_over_piece` is exact per polynomial piece, and a box crossing a knot line
     would otherwise need the rational form's global enclosure, which is far wider. Continuing an
     end piece past the domain covers pcurves enclosed a few ulps outside it, where
     `partials_over` refuses (the cylinder's boundary ruling test).
   - Against: a box crossing `k` lines in each direction costs `(k+1)^2` piece enclosures. After
     the first cuts, pieces cross at most one line, so the measured cost is small (below).
2. **Lipschitz bound across C0 knot lines.** `to_nurbs` circles have double knots in degree 2,
   which are C0 by knot structure. The chord bound needs `D''`, so a piece crossing such a line
   takes `min((|D(a)| + |D(b)| + h sup|D'|)/2, min(|D(a)|, |D(b)|) + h sup|D'|)` and the direct
   enclosure.
   - For: it is certified without any assumption of geometric smoothness.
   - Against: it converges linearly. Measured, it costs 16 to 124 pieces per call out of
     thousands, so it is accepted for now (coordinator, 11-28). An exact crossing cut for line
     pcurves is a possible follow-up.
3. **The point lower bound is padded by the search's gap.** `invert`'s foot is within
   `GAP_REL (diagonal + first incumbent)` of the true nearest point. The incumbent is bounded by
   `|q - centre| + diagonal/2` of the net box, so the pad is
   `2 (|C(t) - q| + eps) + |S(foot) - foot.point| + 8 EPS distance`.
   - For: it is the certificate `invert` gives, with no sampled acceptance.
   - Against: an aim tighter than about `1e-12` of the model's size ends in `Precision` (open
     question, kept by the coordinator).
4. **Three correspondences, the smallest bound kept.**
   - For: the linear one converges quadratically on smooth spans. The held ones keep a piece
     bounded where the feet jump: across a sphere's seam, or at a pole.
   - Against: there are three enclosures per piece. The linear correspondence still leaves a
     quadratic error against the true foot path, so the points needed grow as `w^-1/2` (below).
5. **`Pending` removed rather than left empty** (coordinator, 11-28): it has no dead variants.
   `scx-check`'s NURBS gaps are now proved or undecided like any others, and `scx-step` builds
   NURBS edges' gaps.

**Tests** (`gap/tests.rs`, against closed forms).
- A bilinear plane patch against a line at heights 2.5, 1e-6 and 0, and a slant line reaching 0.3.
- The rational sphere against a circle on it gives 0. Moved out radially by 1e-3 and 0.25, it
  gives `h`. This is tested on a short arc at `1e-9` and on the whole circle at `1e-6`.
- The NURBS cylinder's pcurve gap equals the analytic cylinder's within `1e-9`, for 0, 0.001 and
  1e-4. A boundary ruling gives 0 and 1e-5.
- Determinism: bracket and units are the same each call.
- `a_procedural_surface_is_refused_by_name`: both gaps give `GapError::Procedural` on a
  revolution, with nothing spent.
- Removed: `a_nurbs_surface_is_pending`, which proved a NURBS surface was `Pending`. This is on
  purpose, since FT4 lifts it.
- `scx-bodyq` `refusals_and_a_nurbs_target_face`: the NURBS target that was `Unsupported(Nurbs)`
  is now `OffSurface { by }`, with `1e-6 < by <= sqrt(57)`, the closed-form largest distance of
  the segment from the patch. A segment in the patch is `Undecided`, never refuted.

**Measured** (release, throwaway counters, aim width `1e-9`; "quarters" is how many of the
`to_nurbs` rational quarter arcs the range spans, so knot lines crossed are 0, 1 and 3).

| case | quarters | points | piece bounds | crossing C0 | units | time |
|---|---|---|---|---|---|---|
| plane patch, line at 2.5 / 0 | - | 5 / 5 | 4 / 4 | 0 | 246 / 80 | 0.59 / 0.03 ms |
| sphere, circle on it | 1 / 2 / 4 | 11,344 / 24,688 / 54,107 | 22,682 / 49,370 / 108,208 | 0 / 16 / 33 | 0.22M / 0.45M / 1.20M | 0.39 / 0.84 / 2.2 s |
| sphere, circle out 1e-3 | 1 / 2 / 4 | 7,196 / 14,905 / 34,870 | 14,386 / 29,804 / 69,734 | 0 / 20 / 43 | 0.54M / 1.12M / 2.76M | 1.6 / 3.3 / 7.9 s |
| cylinder pcurve, on | 1 / 2 / 4 | 1,655 / 4,093 / 8,231 | 3,304 / 8,180 / 16,456 | 0 / 16 / 100 | = points | 15 / 38 / 77 ms |
| cylinder pcurve, r 2.001 | 1 / 2 / 4 | 2,031 / 4,106 / 8,253 | 4,056 / 8,206 / 16,500 | 0 / 20 / 124 | = points | 19 / 38 / 78 ms |
| analytic cylinder, same pcurve | 1 / 2 / 4 | - | - | - | 5 (on), 127 / 248 / 513 (off) | 0.01 / 0.3-1.4 ms |

The linear correspondence gives the smallest bound on all but 25 to 36 of the sphere's pieces. The
cylinder's NURBS pcurve costs 15 to 330 times the analytic one, which has an exact chart shortcut.

- Cost is set by arc length and aim, not by knot count. Pieces that cross a C0 line (c0) are
  under 1% of the pieces.
- At width `w` the curve-to-surface points grow as `w^-1/2`; a whole circle at `1e-6` takes
  about 1/30 of the `1e-9` count.
- Tightness: at the end, the lower bound is within `3.3e-11` of the truth, from the search's
  pad. The upper bound is within the aim.
- Analytic arms: units are identical before and after on every path measured (sphere latitude
  5, off it 5, cylinder pcurve 513). Time, interleaved runs on a shared machine: sphere 11.3-19.3 us before, 17.1-17.7 us after; cylinder pcurve 1.73-2.29 ms before, 1.91-2.00 ms after. These are equal within the noise, and the arms are untouched apart from one `Option` match on the kind.

**Open.**
1. The `GAP_REL` pad limits how tight an aim can be (choice 3).
2. Linear convergence across C0 lines (choice 2).
3. The curve-to-surface arm costs one nearest-point search per point, 20 to 80 units, with
   `w^-1/2` points. A quadratic correspondence (a Newton step on the feet) would cut the count.
4. `curve_lies_on_face` on a NURBS face cannot prove the trim until a NURBS surface has pcurves.
5. The procedural arm: the net's gap widened by its bound, the aim tightened by the bound so
   the widened bracket still meets it (or undecided when it cannot).

### 13.37 RY1 as built (11-28)

`intersect_curve` (and `curve_on_surface`) now answer a NURBS curve against a plane, cylinder,
cone, sphere or torus (77 §6.3 RY1). A NURBS curve against a NURBS surface stays
`SurfaceError::NurbsIntersection`. The new module is `scx-surface/src/intersect/nurbs_curve.rs`,
and the parent's `Path::of` dispatch gains one arm.

- **Per span.** Each Bézier span, in homogeneous form and in the kind's own frame, is substituted
  into the implicit form as a `Bounded` Bernstein polynomial with a rounding bound. The degrees are
  `p` for a plane's height, `2p` for a cylinder, cone or sphere, and `4p` for a torus. The roots
  are isolated with `bernstein_roots`, and each one is polished by three Newton steps. The turns
  are the roots of the derivative (`h g' - d h' g` for a rational span). These two isolations make
  up the two budget units a span charges.
- **Contacts.** The span's events (Root, Turn, End, and a Probe at each interior knot) go through
  the parent's `classify`. A hit is therefore typed exactly as an analytic one is, by the same
  `Tangent` and `Crossing` rules.
- **Knots.** A knot is one event, never two: neighbouring spans share the knot's point bit for bit.
  A sign change in the slope across a knot (a kink) is a Turn, so a polyline's V touching a plane
  is one tangent. A crossing at a knot is one crossing.
- **Overlaps.** A span whose polynomial is zero within its bound has its overlap proved by the gap
  certificate (`curve_surface_gap`). A run of such spans is one `Overlap`, clipped by the
  surface's uv box. A closed curve merges its end hits.
- **Undecided.** An uncertified root far from the surface, a flat span the gap cannot prove, or a
  depth-4 sign check that fails returns `SurfaceError::Undecided { t }`, never an empty answer.
  The variant is RY2's (13.32); RY1 widens its doc to NURBS-curve roots.
- **`curve_on_surface` against a plane.** It first takes the control net's height bound,
  `max (|z| + ez)/(h - eh)`, which costs one budget unit. A curve that bound proves in the plane is
  `Proved` without isolation.

Choices:

1. **Undecided rather than empty.** I chose `Undecided` for an uncertified root. For: an empty
   answer is a wrong miss. Against: a caller now has one more error arm, which bodyq, boolean and
   hlr map with `NurbsIntersection`.
2. **Kinks are read from the slopes' end coefficients.** For: this costs nothing extra and matches
   the truth of a polyline V. Against: a span with a zero end slope relies on the turn isolation
   instead. That case is tested with the cubic spline at its knot.
3. **The surface's own uv box clips an overlap run.** This matches the analytic `Overlap` contract.

The hlr fix: `silhouette.rs` `trim`'s face test now reads a foot farther than the face's `near`
as off the face. RY1 made this code reachable. Before RY1, a NURBS rim made the trim `Undecided`.
Now the rim splits the generators, and the NURBS patch's inversion clamps a foot to the patch
edge, which read every point past the edge as `Near`. The test is renamed
`a_rational_cylinder_is_recognised_and_trimmed_by_its_nurbs_rims` and now checks the truth: of
the two generators, the one at about 106.7 degrees runs on the face from `z = 0` to `0.5`, and
the one at about 286.7 degrees lies in the arc's 90 degree gap and is dropped. The old test
asserted that both were `Undecided::NurbsEdge`, which was a stand-in for not knowing.
`rays.rs` is untouched; RY3 owns it.

Measured with throwaway counters, counting isolations per call, where an analytic piece counts
2:

| Case | Spans | Isolations | Analytic path |
|---|---|---|---|
| NURBS circle against each of the 5 kinds | 4 | 8 | 4 for the analytic circle, before and after |
| NURBS line | 1 | 2 | 2 for the analytic line, before and after |
| 7-span cubic spline | 7 | 14 | — |
| Quadratic line against a torus (degree 8) | 2 | 4 | — |
| Cubic grazing a cylinder | 1 | 2 | — |

The work is linear in the spans, and the budget charged equals the isolations. The analytic path
is unchanged: the same counts on main and on the branch, for the line and circle against all
five kinds.

Open questions:

- **A NURBS curve against a NURBS surface.** This is deferred, as 77 §6.3 says.
- **Degree-4p spans of high-degree curves.** These have not been measured for root-isolation
  depth. `RootLimits::DEFAULT` is used.
