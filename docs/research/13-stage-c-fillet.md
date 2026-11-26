# 13. Stage C study: the fillet (constant and variable radius rounds, with their corners)

2026-11-22, read-only, on `782ef9afc` (worktree `sc-wt/cache-arch`). This study uses the method and vocabulary of
report 07 (Boolean gather/apply): P preparable, R requestable, C coupled, S scratch; a prepared view; a phase table
with no key; typed requests; the fulfil loop; a kernel per worker.

Marks: **[V]** verified in code at the cited lines. **[A]** stated by the code's own comments or docs, and not traced
further. **[I]** my inference. Paths are relative to `solidcore/crates/solidcore/src` unless they name another crate
(`ops:` means `solidcore/crates/solidcore-ops/src`).

## 0. Summary

- **A clean compute/insert split already exists.** Everything up to the end of `create_fillet_brep` (p3.rs:4461-4526)
  builds a separate intermediate body (the "pseudo" `FilletBrep` and the fillet faces). The target is edited only
  after that, by marking, shrinking and `merge_fillet` / `trim_at_creases` / `keep_features_in_bands`. There are three
  exceptions, listed under F.
- **The heavy read-like phase is `calculate_solver_geometry`** (p3.rs:3216), together with the definition builds and
  certificates it triggers. Its items are solvers, one per filleted edge. Inside a G2-and-above round, the real unit
  of work is the certificate span (hundreds per edge, about 23 ms to 450 ms each).
- **The phase is not write-free today, and its signature says so.** Each solver gets `&mut Brep` (target),
  `&mut FilletBrep` (pseudo), `&mut [FilletCorner]` and read access to every other solver (session.rs:543-608).
- **The kernel memos are already S.** The station memo, the thumbweight memo and the recertify memo each key on
  everything their result depends on (§4, C). A per-worker kernel (`cache_kernel`) is enough.
- **The coupled items are new in kind compared with the Boolean.** They are:
  - the certificate's span-to-span level hint (§2, C1);
  - the shared pseudo-body arena that solvers write into (C2);
  - solver-to-solver and solver-to-corner reads (C3);
  - the point-step latch (C4, removed by plan item A4).
- **The merge phase is the Boolean's own machinery** (`intersect_insert_relate`, p3.rs:4623, finish.rs:1247). So
  stage C of the Boolean carries over to it unchanged.

## 1. The operation as it is (phases)

Entry points:
- api: `fillet_edges` (blend/fillet_executive/api.rs:202-231), `fillet_edges_per_edge` (:242), `variable_radius_fillet`
  (:290-314). All are wrapped by `failing_as_a_whole` (:97-115, `brep.with_rollback` then `refresh_cached_topology`).
- ops layer: `blend_impl` (ops:blend.rs:1617) → `blend_on_body` (:1681) → `pass` (:909-1069), which re-runs the same
  session chain.

| # | Phase | Boundary (file:line) | Kind |
|---|---|---|---|
| 0 | ops copy / clean copy: `without_splits_at_picks` may replace `*b`; a fresh `brep_copy` for the overflow-first and roll-over retries | ops:blend.rs:1764, 1794-1798, 1826-1831 | edit (copy) [V] |
| 1 | Session and parameters: `FilletSession::new`/`with_second_base`, `prepare_per_edge_with_second` → `refuse_rounds_too_small`, `set_fillet_parameters`, `typed_unblendable` | session.rs:1700, 1711, 2175; p2.rs:740-800 | mostly read; **edits target**: vertex-fan splits p2.rs:782, two_offsets.rs:225 [V] |
| 2 | Corners: `create_corners`, smooth edge sequences (target edge curves trimmed and replaced by one joined curve) | session.rs:2293; p1.rs:1458-1545 | edit of target edge curves [V] |
| 3 | Scope opens: `do_fillet` → `NetScope::within_mut(approx_tol3d(0.5·tol))` → `do_fillet_within` → `do_filleting_steps` (setback marks, `OpenInput::capture`) | finish.rs:880-936, 1063-1211 | edit (marks) [V] |
| 4 | `create_fillet_brep` begins: `creation_smooth`, optional `creation_extend_trace_to_its_rails` (creates UV trims on target, trace_reach.rs:214), the seven pre-trace refusals | p3.rs:4461-4490 | read-like, apart from the trims [V] |
| 5 | `create_corner_vertices`, `adjust_corner_topology` | p3.rs:3109, 3188 | edit of pseudo [V] |
| **6** | **`calculate_solver_geometry`**: per solver in list order, `calc_fillet_geom_with_context` (trace `c4_trace` → `do_point_intersection` → `flush_curve` → `create_fillet_surface` → definition → `certify`) | p3.rs:3216-3236; session.rs:543-608; fillet_solver/p4.rs:2595, 4722 | **read-like in intent; writes target, pseudo and corners** (§2) |
| 7 | Post-trace refusals: `refuse_blends_larger_than_faces`, `refuse_face_consuming_blends`, `refuse_overlapping_bands` | p3.rs:3248, 3446, 3539 | read-like [V] |
| 8 | `ease_set_back_sections`, `calculate_corner_edges`, `trim_solver_rails`, `classify_solver_rails` | p3.rs:3583, 3613, 3664, 3758 | corner edges are read-like per corner [I]; rail trims edit pseudo [V] fillet_solver/p1.rs:3617-3640 |
| 9 | `build_fillet_faces`, `stitch_fillet_faces` into the intermediate body; then `band_cut_cache` cleared | p3.rs:3824, 3890; session.rs:2832 | edit of intermediate [V] |
| 10 | Target edits: `shrink_fillet_faces`, `note_radius_corner_surfaces`, marks, `set_editing_enabled(true)` | finish.rs:1063-1211 | edit [V] |
| 11 | Merge: `merge_fillet` → `insert_intersection_topology` (per solver, then per corner, in list order) → `intersect_insert_relate` → `trim_and_merge`; or `merge_fillet_in_place` / `trim_at_creases` / `keep_features_in_bands` | p3.rs:4597, 4623; finish.rs:1247; trim_at_creases.rs | Boolean-like edit [V] |
| 12 | Scope closes; `join_blended_vertex_fans`, `split_fillet_faces_at_radius_corners`, `check_kept`, `take_blend_faces`, `FilletRefused` with fit size | finish.rs:968-1020 | edit [V] |
| 13 | api / ops: `stitch_and_orient`, `refresh_cached_topology`, the WR-212 closed check, `rounds_cross` (Proved contract) | api.rs:127-148; ops:blend.rs:1007 | edit + check [V] |

There is one read-like phase of real weight, phase 6, with phases 4 and 7 around it. Phase 8's corner edges are a
second, smaller one. Phase 11 has the same read-like pair and classification phases as a Boolean, run between the
intermediate and the target.

## 2. Writes inside the read-like phases (4, 6, 7, 8)

| # | Write | Where | Frequency | Class |
|---|---|---|---|---|
| W1 | UV trim curves created on target edge uses (the filleted edge's uses, the rails' faces) | fillet_solver/p4.rs:815, 4675; fillet_solver/p2.rs:2314; fillet_geom/p2.rs (2 sites); trace_reach.rs:214 | always: the box-edge run made 14 UV trims [V count from plan §1] | **P** (which edge uses is known from the solver list: the filleted edges and the boundaries of their two faces) [I] |
| W2 | Trimmed surface caches built on target faces through `BrepFaceServices::new(target)` → `prepare_surface_cache(&mut Brep)` / `face_surface_cache` | query/brep/ray/live.rs:688-731; call sites fillet_geom/p2.rs:4152, p5.rs:1738, p6.rs:692, fillet_corner/p1.rs:1845, blend_corner/end.rs:2556, fillet_corner/topology/nx1_end.rs:85, fillet_overflow.rs:238 | common on curved faces (each site makes **new, empty** services, so a cache is kept only for the life of one call) [V] | **P** for the build; the per-fetch copy is S/B3 (§4) |
| W3 | Face-level trim passes (20 in the box run) reached through W1/W2 | via `prepare_surface_cache` | always [V count] | P, with W1 |
| W4 | Pseudo-body records: rails, edge uses, vertex uses, curves, tolerances (`set_tolerance` on pseudo rails) | fillet_solver/p2.rs:2849-3025; p4.rs:1026-1089, 1256, 3368-3411, 3541-3560, 3669-3675, 3876 | always (every solver adds its rails) [V] | **C2** (shared arena: id allocation order and, at shared vertices, a later solver finds an earlier solver's records [I]). Reformulation: each solver returns its records; they are applied in solver order. Today's order is solver order, so no result change [I] |
| W5 | `&mut [FilletCorner]` through `SessionGeometryContext::split_corners` | session.rs:1384-1470 | rare to common, at corners and splits [I] | **C3** (with W6) |
| W6 | Reads of other solvers' live state (`before`/`after`, `set_extension_flags`) | session.rs:554-566, 1428-1462 | at every solver adjacent to another [V access; what is read is UNVERIFIED] | **C3** if it reads phase-6 output; P if it reads only phase-1/2 data |
| W7 | Span level hint: `hint = reuse.first − 1` carried to the next span; levels coarser than the hint are not tried, so a span's proved level and bound depend on the span before | fillet_section/section.rs:2153-2190; certificate.rs:4524-4557, 4665-4698 | always on G2+ certificates [V] | **C1**. Reformulation: a hint from the span's own previous round or its parent (`inherit` is already per span), or start at 0. That changes bounds, so it is a **result change** (bounds feed `net_answers`) |
| W8 | `vary` (equal-area varying law) and `defs` passed `&mut` across spans | section.rs:2173-2186 | when equal area is on, or with definition supports [V] | UNVERIFIED: S if it is a lazily extended table of pure values, C otherwise |
| W9 | Point-step latch `k.latch_fillet_point_step` | fillet_solver/p4.rs:4031-4041 | flag-off runs only (the app runs flag-on) [V] | **C4**, removed by plan A4 |
| W10 | Station memo `k.blend_station_memo` (4-slot ring) | fillet_section/definition.rs:365, 497-513, 608-664 | every station evaluation [V] | **S** (§4) |
| W11 | Thumbweight memo `k.blend_w_memo` | fillet_section/section.rs:362-430 | equal-area sections [V] | **S** |
| W12 | Recertify memo `k.recertify_memo` | surface/procedural_blend/convert.rs:783-853 | record reads [V] | **S** |
| W13 | `blend_counts`, `work_counters`, `blend_face_log`, `net_evaluations` counter | procedural_blend.rs:117-140, 387-425; kernel.rs:867-940 | always [V] | **S** |
| W14 | `NetScope` cell `k.blend_net_tolerance`, set and restored (nested `exact` at station.rs:560; `new` in cache builds) | procedural_blend.rs:155-236 | always [V] | **S** (a mode flag), better as an argument (D) |
| W15 | `k.context.set_doing_boolean(true)`, `intersector.doing_fillets` | p3.rs:4553 | always [V] | S (mode flag) |
| W16 | Scratch kernels `tk` for curve trims (rng cloned, cancel shared) | p1.rs:1510-1517; fillet_solver/p1.rs:3607-3619; p2.rs:3151-3163 | per trimmed edge [V] | S already. Note `tk.rng = k.rng.clone()`: the draws of the scratch kernel do not advance the caller's, which A1 makes moot |
| W17 | `brep: Option<&mut Brep>` threaded into `step_solve`, which reads the solver's edge uses | fillet_solver/p4.rs:4579-4592 | every trace step [V] | read only as far as I traced; the `&mut` is inherited from `LoadInitialValues` [A]. Should become `&Brep` (no class needed if no write; UNVERIFIED) |
| W18 | Lazily made surface kinds and NURB cells on supports (`take_made_kind_for_edit`, cache slots) | surface/surface/p2.rs:985-1010 | common [I] | S/P (deterministic lazy init on `Sync` values) |

Totals (phases 4, 6, 7, 8):
- **P 3**: W1, W2 (build), W3.
- **R 1**: the inferred case where a trace leaves its expected faces (overflow onto a neighbour) and needs a UV trim
  or cache on a face not predicted (W1/W2 on an unforeseen face) [I].
- **C 4**:
  - **C1**: the span level hint (W7).
  - **C2**: the pseudo-body arena written per solver (W4).
  - **C3**: corner records and neighbour-solver reads (W5 + W6).
  - **C4**: the point-step latch (W9). Plan A4 removes it.
- **S 9**: W10-W16 and W18, plus the `cache_kernel`-style per-worker state. W8 and W17 are unresolved (S or nothing).

## 3. What "prepared" means for the fillet

These must exist before phase 6 so that the common path writes nothing to the target:

1. UV trim curves on every edge use of the filleted edges and on the boundary edge uses of the two faces at each
   filleted edge (W1). On the box run that is 14 trims. Cost: one trim creation each, which today happens anyway [I].
2. The trimmed surface cache of each of those faces, built once (W2). Planes and cylinders need none (plan §1). The
   box run built 2 trees.
3. The vertex-fan splits (p2.rs:782) and the smooth-sequence curve joins (p1.rs:1458) already happen in phases 1 and
   2. They are prepare edits, already ahead of the read phase.
4. The pseudo topology that `create_corner_vertices` and `adjust_corner_topology` build (phase 5), frozen for
   phase 6. Each solver then returns its rails and records (W4) as values instead of writing them.
5. A per-worker kernel copy (`cache_kernel`, boolean/merge/p4/kernel_adapter.rs:76-97 already copies the net
   tolerance and counts and starts the memos empty).

None of this exists on input bodies in general. An imported body usually has no UV trims (plan §1).

## 4. Caches (and question C, the memos)

**Face caches.** Each fillet site builds `BrepFaceServices::new(target)` with no caches, uses it for one solve or
drop, and drops it (sites in W2). Inside a site, every fetch maps the kept cache through `map_handles`, which is a
whole copy (live.rs:671-731), even on a hit. The two fetch kinds are `cache_mapped` and `cache_state_mapped`. So copies
scale with solve calls, not with faces. A phase-scoped keyless table (face → built cache or build failure, valid
while `&Brep` is held) **would do here, as for the Boolean**. Every build input (the face, its loops, its UV trims) is
constant once item 1 of §3 is done. No solver writes into a tree (brief facts). Nothing in phase 6 edits a face of the
target [V for the sites read; I for the whole phase]. The one difference: the table must outlive the per-call services
objects, so the sites must take it as an argument instead of creating services.

**Question E (the 2×128 copies).** One copy is made for each solve call routed through a `SurfaceSolveContext` /
`FaceQueries` on `BrepFaceServices` or `SidedFaceServices` (live.rs:781-914, 1000-1030). The equal counts (128 and 128)
suggest each solve calls `surface_cache_without_get_tree` and then `build_surface_cache` once, i.e. about 128 solves on
2 built faces [I]. There are two candidate sources, and I could not tell them apart by reading:
- (a) the fillet geometry sites in phase 6/8 (fillet_geom/p2.rs:4152 drop, p5.rs:1738 side-face intersection,
  p6.rs:692 blending-rail point solve, corners);
- (b) the Boolean-like `intersect_insert_relate` of phase 11, whose `SidedFaceServices` (live.rs:895, 914) also call
  `cache_mapped` / `cache_state_mapped`.

UNVERIFIED; settled by a per-call-site counter (§8). Either way the fix is B3 (borrow through a handle view) plus the
phase table.

**Question C: the memos.**

| Memo | Key | Complete? | Capacity / eviction | Cross-station effect | Class |
|---|---|---|---|---|---|
| Station memo (definition.rs:365-664) | definition identity (`Arc::ptr_eq`, renewed on copy/transform, procedural_blend.rs:845-860, 1525), `s`, `ts`, `te`, side when sided | Yes [V/A]. A miss is seeded only from `def.seeds` (the definition's own curves), never from another station's result, so a hit equals a fresh solve. The solve runs under `NetScope::exact` (station.rs:557-561), so the caller's scope is not an input. Tolerance `1e-9·max(1,|l|)` comes from inside the key | 4 slots, round robin (`next`) | None on results; only on cost. Parallel stations each get their own ring, with no result change | **S** |
| Thumbweight memo (section.rs:362-430) | `(order, u.to_bits())` | Yes: the solve is a pure function of the key [V code + A comment] | 65,536 entries, cleared when full; emptied when the outermost `OperationScope` opens (session.rs:1716, p2.rs:1718) | None | **S** |
| Recertify memo (convert.rs:783-853) | debug text of the record plus the kernel tolerances and fixes | Yes [A] | byte cap (`RECERTIFY_MEMO_BYTES`) | None; it serves record reads, not the fillet's own trace | **S** |
| `band_cut_cache` | session field | n/a | cleared after `create_fillet_brep` (session.rs:2832) | phase-scoped already | S/phase |
| Curve caches retained by `load_solver` (session.rs:2888) | curve | UNVERIFIED | n/a | n/a | S [I] |

## 5. Items

- **Solvers**: one per filleted edge (or per smooth sequence member). Typical count 1-12, at most a few dozen [I].
  Phase 6 runs them in list order (p3.rs:3216-3236), and the first failure ends the stage, recorded with error code 10.
  That is an early exit that depends on order: the reported failing edge is the first in list order. A parallel run
  must report the lowest index, not the first to finish.
- **Stations and spans inside a solver**: 75 → 127 → 137 stations over 3 refinement rounds on the tilted loft, about
  600 spans per edge on an extruded spline, 0 on a cylinder (circular sections take no certificate), from
  solidcraft#13778. `gn_certify_span(_reusing)` (certificate.rs:4498) takes `&Kernel` and shared inputs only, apart
  from `vary`/`defs`/`reuse` [V signature]. Results are combined as one bound per span in a `Vec`, then compared with
  the target. There is no floating-point sum. The span loop and the halving rounds are coupled by the level hint
  (C1).
- **Cost skew**: very large. A plane/plane round takes milliseconds [I]. A G2 round on a spline takes 23 ms to 450 ms
  per span (29 s and 90 s fillets in #13778).
- **Plain values**: per solver, the trace (rails, centre curve, UV curves), the fillet surface (net plus definition
  plus bound), and pseudo records (once W4 is reformulated). Per span, a bound and a kept level.

**Question B (which stage dominates).** Phase 6's certificate dominates where there is one: 86% of samples in
`GnSectionFsg::certify` (spline extrusion), and 90.2 s of 91.6 s in 198 spans (tilted loft) [A, #13778]. After the
build, S evaluations inside UV-trim fitting (`create_uv_trim_curve` → `fit_tolerance` → `calc_max_edge_face_gap`, 67%;
6,100 ns per S evaluation against 81 ns on the net) dominate a short history. Those evaluations fall in phases 11-13
and in later reads. Rounds without a certificate still take 2.5-8 s each, unprofiled (UNVERIFIED where). The 260
cache copies are not known to matter in time (plan §7).

## 6. What stays sequential

- Phases 0-3 and 5: parameters, fan splits, smooth joins, corner topology.
- Phase 8's rail trims and phase 9 (building and stitching the intermediate).
- All of phases 10-13: shrink, mark, merge, trim and merge, join fans, stitch, orient, the closed check. Phase 11
  inherits the Boolean's own sequential steps: trim creation, raises, proto topology, imprint, marks, merge.
- Inside phase 6: the certificate's halving rounds (each round is a barrier), and the corner phase after all solvers
  (`calculate_corner_edges` reads every solver's geometry).

## 7. Cost

PRs to make phase 6 (with 4, 7, 8) write-free on the common path, specific to the fillet:

| # | Step | PRs | Result change |
|---|---|---|---|
| F1 | `NetScope` becomes an evaluation argument (or a value carried in the evaluation context). Also fix `Surface::evaluate_ignoring_status` (surface/surface/p2.rs:2233), which takes the net for any active scope (`is_active`), while `ProceduralBlend::evaluate` uses `net_answers` (`tolerance ≥ bound`, procedural_blend.rs:1164) | 2 | **yes** for the inconsistency fix, wherever a scope below a bound reached that path |
| F2 | Prepare step (§3 items 1, 2) over the filleted edges' faces | 1-2 | no if trims are created in the same order today's first touch creates them [I]; else small |
| F3 | The fillet sites take the phase table instead of `BrepFaceServices::new(target)` (W2), on top of B3 | 2-3 | no |
| F4 | Solver geometry takes `&Brep` (target): remove `&mut` from `calc_fillet_geom_with_context`, `step_solve`, `get_or_create_uv_trim_curve` callers (W1, W17) | 2-3 | no |
| F5 | Pseudo records returned per solver and applied in solver order (W4; about 60 write sites in fillet_solver p2/p4, fillet_geom p1-p6) | 3-5 | no if applied in today's order [I] |
| F6 | Neighbour and corner reads: snapshot the phase 1/2/5 data each solver reads, or prove that it reads only that (W5, W6) | 1-3 | possible, if a solver today reads a neighbour's phase-6 output |
| F7 | Span hint per span (W7), plus `vary`/`defs` settled (W8) | 1-2 | **yes**: bounds change by level choice; goldens with certified bounds move |
| F8 | Per-solver (and per-span) kernel from `cache_kernel`; first failure by index | 1 | no |
| F9 | Refusals in phases 4 and 7 return typed values (they already run before any target edit) | 1 | no |
| — | Point-step latch (W9) | 0 (plan A4) | flag-off goldens only |

Fillet-specific total: **14-22 PRs, two or three with result changes** (F1, F7, possibly F6).

Shared with the Boolean's five parts (report 07 §6):
- the prepared view (part 1);
- the phase table (part 2, the same table);
- typed requests for the R case (part 3, `NeedFace`/`NeedTrim`);
- the fulfil loop (part 4);
- `cache_kernel` (part 5).

Phase 11 *is* the Boolean pair and classification machinery. Once Boolean C1-C5 land, the merge gets them with no
fillet work beyond passing its two bodies [I].

**Question A (generic-blend terms).**

| Generic-blend term | Fillet equivalent |
|---|---|
| Spine | the filleted edge chain (phases 1-2) |
| Rails | the pseudo rails (W4) |
| Section | `GnSectionFsg`, n = 1 circle, 2-4 Gⁿ |
| Definition | `ProceduralBlend`, with net B and certified bound |
| Certificate | per span (W7) |
| Events and corners | phases 5 and 8 |

Units and counts are in §5.

The claim in generic-blend.md §5 (line 540) is that chains, spines, stations, spans and corners are "pure functions
over immutable inputs with results in a fixed order". It fails at:
- `FilletSession::calc_solver_geometry_noting` (`&mut Brep`, `&mut FilletBrep`, `&mut [FilletCorner]`,
  session.rs:543-608);
- `FilletSolver::calc_fillet_geom_with_context` and `step_solve` (`Option<&mut Brep>`, fillet_solver/p4.rs:4579);
- `get_or_create_uv_trim_curve` callers (p4.rs:815, p2.rs:2314);
- `calc_rail_rail_interpolation_with_solvers` (fillet_geom/p2.rs:3922/4152);
- `calc_fillet_int_side_face_with_context_and_error_info` (fillet_geom/p5.rs:1519/1738);
- `calc_blending_rail` (fillet_geom/p6.rs:692), all three of which build caches on `&mut target`;
- `GnSectionFsg::certify`'s span loop (section.rs:2153-2190, the level hint makes span j depend on span j−1);
- `evaluate_definition` writes the station memo, which is scratch, not a purity break.

The §2.3 claim (line 172), that the station memo agrees bit for bit across kernels, holds [V].

**Question D (NetScope).**
- **Setters** (non-test):
  - finish.rs:883 (whole `do_fillet`);
  - trim_at_creases.rs:1583;
  - session.rs:7269;
  - fillet_geom/p2.rs:1440;
  - fillet_geom/surface_plane_curve.rs:266;
  - surface_point_solve/p2.rs:1038, 1067;
  - station.rs:560 (`exact`);
  - surface cache builds under `NetScope::new`: surface_cache p1.rs:3301, p2.rs:846, p3.rs:419/868/990/1180, and
    surface/surface/p7.rs:2615.
- **Span**: the scope at finish.rs:883 covers phases 3-12, including the merge, but not phase 13 (`stitch_and_orient`,
  the closed check, `rounds_cross`).
- **Readers**:
  - `ProceduralBlend::net_answers` / `evaluate` (procedural_blend.rs:1164-1187);
  - `Surface::evaluate_ignoring_status` (surface/surface/p2.rs:2233, uses `is_active`, the inconsistency above);
  - procedural_sweep.rs:562/1124/1143;
  - mass/integrator/properties.rs:480;
  - fillet_geom/tangent_end_rib/certificate.rs:158;
  - `cache_kernel` copies it (kernel_adapter.rs:76).
- **Making it an argument**: one `EvalMode { net_tolerance }` value in the evaluation calls of the readers above. The
  scope's span is lexical, so this is mechanical, but it touches every `Surface::evaluate` caller on the path (count
  UNVERIFIED).
- **Cross-scope cache reads**: surface caches are always built on the net (`NetScope::new`), so a cache built inside
  the scope and read outside it, or the reverse, is the same [V]. Evaluations and UV-trim fits made inside the merge
  (net where bound ≤ tolerance) and made later outside it (S) differ by at most the bound. Gap and trim caches filled
  during phase 11 therefore carry net values into later reads [I]. This is a value-level history dependence of at most
  the bound, not a cache-key defect.

**Question F (body edited before the heavy computation ends).** Yes, in four places:
1. **Fan splits**, a topology edit of the target in phase 1 (p2.rs:782; two_offsets.rs:225), undone in phase 12
   (finish.rs:1020).
2. **Smooth-sequence curve joins** in phase 2 (p1.rs:1499-1545).
3. **UV trims and face caches on the target during phases 4 and 6** (W1-W3).
4. **The ops-layer copies**, which replace the body (phase 0).

Otherwise a clean compute/insert split exists at the end of `create_fillet_brep` (p3.rs:4526). Items 1 and 2 already
sit in prepare. Item 3 is what §3 prepares. Item 4 is outside the kernel.

**Question G (refusals).**
- **Decided before any target edit except 1-3 of F, already in the read-like phases 4 and 7:**
  - up-front `supported_size` / `no_edges` (api.rs:157);
  - `refuse_rounds_too_small` and `typed_unblendable` (phase 1);
  - the seven pre-trace refusals;
  - the three post-trace refusals (phase 7);
  - `overlapping_setbacks` and the unbuilt-setback refusal (finish.rs:1063-1100).

  All of these can return typed values from a read phase.
- **Decided after edits:**
  - `check_kept`;
  - `FilletRefused` with a fit size computed on the body (finish.rs:887-936; ops `sized_on` :720);
  - the WR-212 closed check;
  - `rounds_cross`;
  - the ops retries (roll-over, overflow-first).

  These need the merged body and stay in the apply phase; `with_rollback` and the ops copy already undo them.

## 8. UNVERIFIED (with the run that settles each)

1. **Which code makes the 2×128 cache copies** (phase 6/8 fillet sites, or the phase-11 merge). Settle with a work
   counter per call site of `cache_mapped`/`cache_state_mapped` (caller name) on the box-edge fillet and on one curved
   round.
2. **What a solver reads from `before`/`after` and writes to `corners`** in phase 6 (W5, W6). Settle by logging, per
   solver, which neighbour fields are read and whether their last write was in phase 6, on the 3-edge corner battery.
3. **Whether `vary`/`defs` (W8) carry state between spans that changes a bound.** Settle by certifying the spans in
   reverse order and in isolation on the tilted loft and comparing the bounds.
4. **How many bounds and goldens move if the level hint is made per span (F7).** Settle with a golden run with the
   hint forced from the parent or from 0.
5. **Whether `step_solve`'s `&mut Brep` is ever written (W17).** Settle with a counter on target writes inside
   phase 6, or a type change.
6. **Whether a trace ever needs a trim or cache on a face outside the predicted set (the R case).** Settle by
   counting W1/W2 hits on faces not adjacent to a filleted edge, over the fillet goldens.
7. **Whether preparing trims in a fixed order gives today's body** (F2). Settle by comparing record bytes with and
   without the prepare step on the fillet goldens.
8. **Where 2.5-8 s goes in a round without a certificate.** Settle with one profile.
9. **The time share of the cache copies.** Settle with a profile of a slow curved fillet (plan §7).
10. **How many goldens move from the `is_active` versus `net_answers` fix (F1).** Settle with a golden run.

**Verdict:** The read phase is harder to clean than the Boolean's. Its writes go through `&mut` signatures across
about 60 sites, it shares a pseudo-body arena, and the certificate has an order-coupled hint. On the other hand there
are few items, they are heavy, the memos are already scratch, the compute/insert split already exists, and the merge
reuses the Boolean's stage C wholesale.
