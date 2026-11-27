# 61 - G2 and conic fillets: does solidcore have them?

Written for solidcorex. Research 60 §5 decision 3 says "G2 and conic fillets: the contract asks for
them and solidcore has none. Building them means new algorithms, outside R5". 60 §6 and 04 (row
11-27) defer them, and 01 §1.1 (the `scx-blend` row) repeats this. This report tests that claim.

It also maps the pin's two fillet systems (§0). The owner's ruling is that solidcorex uses the new
one only: not the old one, and not the split between them. The design in §4 is built on the new
system alone.

Nothing was built or run. Only this file was written.

It was reviewed adversarially (`reviews/review-61.md`). The factual errors the review found are
corrected in place below, and each says "(corrected after review-61)". The owner's rulings of
11-27 are in §7. Where §4-§6 and §7 differ, §7 holds.

- **Labels:**
  - READ: read from code, docs or git history.
  - JUDGED: a design judgement or an estimate.
- **Paths:**
  - Pin paths are relative to the pin's `crates/`.
  - Fork paths are relative to the SMLib repository (tangency-labs/smlib, `main`).
  - Upstream is NVIDIA-Omniverse/usd-brep.
  - Other kernels are named by repository, with a path inside it.
  - "The fork's design doc" is fork `docs/generic-blend.md`.
  - "The pin's design doc" is the pin's own `docs/generic-blend.md`, solidcore's newer page on the
    same design (771 lines). It adds the Chamfer and Roll-over events (§3.2) and the rule of §9:
    "a step lands as the default, with no UpstreamFixes flag … nothing is kept as a fallback".
    The first draft of this report cited only the fork's page (corrected after review-61).
  - Issue numbers without a repository are solidcore's; "smlib #n" and "fork #n" are the fork's.
- **Licences:** springrep, cadrs, anvilcad, arris and geomsolver are MIT/Apache, so their code may be
  reused. next.BREP (custom licence), kentron (AGPL) and OCCT (LGPL) were not used here, not even
  as ideas.

---

## 0. The old and the new fillet systems

### 0.1 What each one is (READ)

- **The old system** is upstream SMLib's `SmFillet*` code. It has these parts:
  - an executive (`SmFilletExecutive`) and a pseudo-B-rep of the blend (`SmFilletGeom`);
  - solvers (`SmFilletSolver`, `SmFilletStandardSolver`, `SmFilletCustomSolver`) and an intersector;
  - an analytic try-chain. It tries `CanMakeAnalytic`, then a cylinder, then a torus, then a
    sphere;
  - three cross-section generators. `SmLinearCrossSectionFSG` gives a ruled chamfer.
    `SmCircularCrossSectionFSG` gives approximated arcs. `SmBlendCurveCrossSectionFSG` gives
    Hermite G1-G3 sections, lofted and uncertified;
  - the Eiffel tower section and a secant solver;
  - corner classes chosen by edge counts: Nx1, Degenerate, Open, 3x2Mixed, Nx2, 4x3,
    ConvexNxN and ConcaveNxN. They use Coons and N-sided patches and `MakeAnalyticCornerSurface`;
  - a march into tangent points;
  - `SmFilletRemoval` and fillet recognition on `SmFace`.
- **The new system** is the fork's "generic rolling-ball blend" (the fork's design doc). Its rule
  (§1 of that doc) is that every case is built by one construction: a spine, two rails, a section,
  and the events met along the trace. Failures are fixed in that construction, "never ... by a
  point solution for that case, a fallback to the old code, or a chain of methods tried in turn".
  Each step "deletes one of" the old try-chain, the old vertex and corner classes, and the march.
  It has these parts:
  - **The section**, chosen by order n (§2.2 of the doc):
    - 0 is the chord (the chamfer);
    - 1 is the exact rational circle;
    - 2 to 4 is Hartmann's Gⁿ section, at the equal-area thumbweight per station.
  - **One corner class, `SmBlendCorner`.** It names End, Join, Turn (Mitre or Swept), Ball, or
    Setback. It "never returns 'no kind': a configuration the table does not name is a Setback",
    and every corner where G2 is selected is a Setback (§3.2). The pin's design doc adds two
    events: Chamfer (Mitre or Facet) and Roll-over (refused with `CornerUnsupported` until built).
    - (Corrected after review-61.) That is the design. The pin's code does not yet match it:
      `blend_corner::classify` ignores the section kind, so a two-blend G2 corner is built as a
      Turn or a Join, not a Setback; and `classify_setback` declines many corners, which then stay
      the old NxN patch (§0.5 F11). The pin's design doc says so itself (§3.2): "a marked corner
      beside a curved face keeps that patch too until the curved Setback is the default".
  - **Exact classes** (Dahl–Krasauskas) in place of the try-chain (§4).
  - **Fold and room checks**, and refusals that name the size that fits
    (`smlib_blend_fit_size`; §3.3, §3.5).
  - **Overflow trimmed at the crease** (§3.4).
  - **The round stored as its definition**, with a certified net (§2.3, §2.4).
  - Its status (§6): steps 1-3 and 5-7 are merged (smlib #76, #77, #78, #79, #102). Step 4
    (corners) is part merged, with the old corner code that could no longer be reached deleted in
    #113. "SMLib construction ended on 2026-11-02; remaining corner work continues in solidcore"
    (#2343, #2345, #2347, #2348, #2349, #2361). The pin's design doc lists the open ones as
    #2343, #2346-#2349, #2356, #2361 and #2400 (delete the old corner hierarchy).
  - **The fork is itself split** (corrected after review-61). `SmFillet3x2MixedConvexityCorner`,
    `SmFillet4x3Corner` and the Convex and Concave NxN corners are still reachable on fork `main`
    (`SmFilletCorner.cpp:1244, 1304, 1367, 1380`). "New system only" has no reference
    implementation for those vertices yet.
- **The new system is not a separate executive.** It was built inside the old executive, solver,
  trace, pseudo-B-rep and face insertion and merge. Those parts are shared infrastructure. The new
  system keeps them: the station solve, Newton on both offsets, is the doc's own §2.1. Old and new
  therefore differ module by module, and sometimes inside one file.
- **The Gⁿ section belongs to the new system.** It is step 3 (smlib #78), `SmGnSectionFSG` in
  `SmFilletSection.cpp`. It replaced upstream's `SmBlendCurveCrossSectionFSG`.

### 0.2 Line counts in the fork (READ)

The counts are for the fork's `main` against upstream. "Upstream" is 0 for a file the fork added.

| File (`source/SMLib/src/`) | Fork | Upstream | System |
|---|---|---|---|
| `SmFilletSection.cpp` | 5,435 | 0 | new: the section family (`SmGnSectionFSG`) |
| `SmBlendCorner.cpp`, `…Ball`, `…End`, `…Join`, `…Mixed`, `…Setback` | 2,544 + 920 + 1,801 + 338 + 1,996 + 922 = 8,521 | 0 | new: classification and the corner events |
| `SmBlendSetback.cpp` | 3,144 | 0 | new: the setback fill |
| `SmFilletExact.cpp` | 2,631 | 0 | new: the exact classes |
| `SmFilletOverflow.cpp` | 1,815 | 0 | new: the crease trim |
| `SmFilletKeepFeatures.cpp` | 802 | 0 | new |
| `SmProceduralBlend.cpp` | 757 | 0 | new: the definition |
| new headers (`SmBlendCorner.h`, `SmBlendSetback.h`, `SmFilletExact.h`, `SmProceduralBlend.h`, `SmBlendNetScope.h`) | 941 | 0 | new |
| `SmFilletCorner.cpp` | 5,642 | 8,872 | old. It shrank by the deletions in #113 and #86. What is left is the corner record and the patches that can still be reached |
| `SmFilletExecutive.cpp` | 8,318 | 6,212 | shared infrastructure, grown by the new events |
| `SmFilletGeom.cpp` | 14,384 | 13,875 | shared (the pseudo-B-rep) |
| `SmFilletSolver.cpp` | 10,727 | 9,252 | shared (the trace and the §3.3 checks) |
| `SmFilletStandardSolver.cpp` | 3,646 | 4,900 | shared solvers. The old Hermite blend-curve FSG was retired |
| `SmFilletCustomSolver.cpp` | 1,399 | 1,399 | old (Eiffel tower, secant) |
| `SmFilletIntersector.cpp` | 2,062 | 1,864 | shared |
| `SmFilletRemoval.cpp` | 2,210 | 1,552 | old operation (removal) |
| `SmBlendSurface.cpp` | 1,781 | 1,781 | old, unchanged (a surface-blend operation, not a fillet) |
| `SmCurveFilletExecutive.cpp` | 252 | 252 | old (2-D curve fillets) |

### 0.3 The pin's `blend/` mapped (READ for origin; JUDGED for the "keep" column)

The counts are lines per directory, plus its `.rs` head file. They include tests. `blend/` totals
232,752 lines.

| Module (`solidcore/src/blend/`) | Lines | System | Evidence |
|---|---|---|---|
| `fillet_section/` | 22,214 + 128 | **new** (Gⁿ, the certificate, the definition, recertification) | fork `SmFilletSection.cpp` header; `fillet_solver/p1.rs:252-267` |
| `blend_corner/` | 21,465 + 1,267 | **new** (`SmBlendCorner`: ball, chamfer, end, join, mixed, rollover, setback, swept, piece) | module header |
| `blend_setback/` | 8,200 + 4,134 | **new** | `blend_setback.rs:1-60` |
| `fillet_exact/` | 3,145 + 4,123 | **new**. Behind a fix flag, but the app turns every flag on, so it is the production path (§0.5 F1; corrected after review-61) | `fillet_solver/p4.rs:82, 504` |
| `fillet_overflow.rs`, `fillet_cliff_overflow.rs`, `law_corners.rs` | 1,468 + 896 + 438 | **new** | headers |
| `fillet_geom/` | 31,777 + 19 | shared (the upstream pseudo-B-rep). `tangent_end_rib.rs` (2,080) and its certificate (1,262) are new (step 1). The `calc_cone` and `calc_torus` recalculation in `p3.rs:1319-1370` is old (the try-chain) | `fillet_geom/p3.rs:74, 1319` |
| `fillet_executive/` | 53,154 + 17 | shared (upstream `SmFilletExecutive`), with the new events in `session/*` (trim at creases, keep features, ease, law corners, cliff overflow, fit size, ball, band cuts). `p3_tests.rs` (1,819) are upstream-shaped tests | module headers |
| `fillet_solver/` | 25,803 + 19 | shared (the station and the trace). The analytic geometry in `p2.rs:971-2200` (`calc_cylinder_fillet_geom`, `calc_cone…`, `calc_torus…`, `calc_sphere…`) and `c3_try_analytic` (`p4.rs:2220-2265`) are **old** | `fillet_solver/p4.rs:504-507` |
| `fillet_standard_solver/` | 9,303 + 12 | shared solvers (constant radius, constant distance, variable radius). The FSGs are **old**: `LinearCrossSectionFsg` (`p1.rs:2388`, `p2.rs:3503`) and `CircularCrossSectionFsg` (`p1.rs:2415-3533`, `p2.rs:2618-3500`), upstream classes with fork fixes (an exact circle, WR-708; the definition surface). `touch_circle.rs` (105) is new | `fillet_solver/p1.rs:252-470` |
| `fillet_corner/` | 17,911 + 11 | **old** (`SmFilletCorner`): `p1.rs` 4,584, `p2.rs` 2,735, `p3.rs` 3,257, with the 4x3, 3x2, Nx2, NxN and analytic corner patches. Its dispatch (`p3.rs:932`, `create_noting_roll_overs`) now hands most vertices to the new events. Some `topology/` files serve the new events (`ball`, `closed_turn`, `join`, `swept_turn`, `end_smooth`, `mixed`). Others are old (`concave_4x2` 286, `concave_nxn` 291, `mixed_3x2` 475, the 4x3 part of `end_corners` 533). (Corrected after review-61.) `p1.rs` is not old-only: it holds corner machinery every path still calls, `calc_corner_vert_geom` (`session.rs:1281`, `fillet_geom/p3.rs`), `drop_end_rib` (`session.rs:3732`), `check_and_fix_degeneracy` (`session.rs:9157`), and the End event's `create_end_patch` (called from `p3.rs:2098`). `make_variable_radius_fillet` (`p2.rs:1219`) is reached only from `make_analytic_corner_surface` and goes with it | headers of each file |
| `fillet_custom_solver.rs` | 2,631 | **old**. `EiffelTowerFs`, `EiffelTowerCrossSectionFsg` and `SecantConstantRadiusFs` (:844-2120) have no production constructor. `ConstantRadiusAssistedFs` and `OffsetSurface` are shared; the trace uses them | file |
| `fillet_intersector/` | 1,102 + 4,712 | shared. `skin_bound` is new | header |
| `fillet_removal/` | 2,352 + 2,379 | **old** operation (`SmFilletRemoval`). `ring.rs` is not upstream | header |
| `identify_fillet.rs` | 1,127 | **old** recognition. It knows the Hermite kinds, not Gⁿ (§1.1) | `:219-345` |
| `feature_executive/`, `hcr.rs` | 542 + 3,297, 2,303 | not fillets (defeature, hidden curves) | headers |
| `exact_round/`, `fillet_tangent_rollover.rs`, `band_room`, `round_room`, `blend_refusal`, `carried_face`, `chamfer_side`, `definition_check`, `fillet_classification`, `weld_bead` | 75 + 1,183, 1,394, and 262 to 846 each | solidcore's own, beside the new system | headers |

### 0.4 Every entry point and the system it reaches (READ)

| Entry point | Path | Reaches |
|---|---|---|
| Kernel `fillet_edges`, `fillet_edges_per_edge` | `fillet_executive/api.rs:187-245, 247` → `FilletSession::prepare_constant_radius_edges` / `prepare_per_edge_constant_radius` (`session.rs:2042, 2099`) → `do_fillet` | shared executive. The section follows `continuity`: 0 → `Linear` (old), 1 → `Circular` (old class, fixed), 2 → `BlendCurve` → `GnSection` (new) (`session.rs:2059-2060`; `fillet_executive/p2.rs:836-866`). Corners: §0.4.1 |
| Kernel `variable_radius_fillet` | `api.rs:374-410` → `prepare_variable_radius_edges` (`session.rs:2313`, continuity at `:2330`) | the same split. A law round at order ≥ 2 is `GnLawRound` (new; `section.rs:207-213`) |
| Ops `Blend::Round` | `solidcore-ops/src/blend.rs:883-917` `prepare` → `prepare_per_edge_constant_radius(…, &[Circular], 1, 1.0)` | Circular (the old class, fixed) |
| Ops `Blend::Variable` | `prepare` → `prepare_variable_radius_edges(…, law, 1, 1)` | Circular (old, fixed) |
| Ops `Blend::Curvature` | `prepare` → BlendCurve, order 2, `GN_THUMBWEIGHT_EQUAL_AREA` | **new** (Gⁿ) |
| Ops `Blend::Chord` | `prepare` → `prepare_per_edge_constant_distance` (`session.rs:2157`) with the circular section | constant-distance solver (shared) with Circular |
| Ops `Blend::Chamfer`, `chamfer_sides*` | `prepare` → `prepare_chamfers_along`; `blend.rs:2849-2986`; the session builds `Linear` (`session/two_offsets.rs:673, 711, 727`; `session/surface.rs:403`) | **old** ruled FSG, not Gⁿ order 0 |
| Ops `blend`, `blend_ordered`, `blend_with` | `blend.rs:1702, 1740, 1770` → `prepare`, then `corners()` (`:922`) | as the variants above |
| Ops `fillet_variable*` | `blend.rs:3151-3277` | the variable path |
| Ops `exact_round::round` | `solidcore-ops/src/blend/exact_round.rs:139` | solidcore's own closed-form rounds between planes, run before the fillet session. A parallel path, in neither system |
| Ops `blend_edges`, `fillet_sheets`, `fillet_crossing` | `solidcore-ops/src/blend/blend_surface.rs:99, 337, 507` | the session path, on sheets |
| Ease | `fillet_executive/session/ease.rs:500-524` | **new** (a G2 target, `GnSectionFsg::new(2, 0.0)`) |
| Setback asked by the user | `session.rs:3180-3260` (`set_set_back_fill`) → `classify_setback` | **new** |
| `identify_fillet` | `blend/identify_fillet.rs` | **old** recognition |
| Fillet removal, `delete_fillet_chain` | `blend/fillet_removal/`; `solidcore-ops/src/blend.rs:108` | **old** operation |
| Offset reblend | `offset/offset_faces.rs:1079` `reblend` → `FilletSession::prepare_per_edge_solvers(…, sections, …, 1, 1.0)` (`:1127`), with **every section set to `Circular`** (`:1108`) | Circular. A G2 round that an offset carries is reblended as a circle |
| Offset geometry | `surface/procedural_blend/offset.rs:40, 201-208` | **new** (the offset of the Gⁿ definition) |
| Procedural blend record | `surface/procedural_blend.rs:112-117` (`order_is_valid`: chord 0, circle 1, Gⁿ 2-4) | **new** |

#### 0.4.1 Corners

- **The dispatch.** `fillet_corner/p3.rs:932` `create_noting_roll_overs` is the old
  `SmFilletCorner::Create`. Under it, vertices go to the new code like this:
  - One blend ends: a `blend_corner` End. Its construction (Nx1, Open or Degenerate) and its end
    patch are the old Nx1 corner's, "moved unchanged" (`p3.rs` ~1130-1170, `:1497-1530`).
  - A closed edge, or two edges: Join or Turn (#2349, #16654).
  - Two blends of one convexity among three or four edges: `blend_corner::classify`, a Turn or
    a Join, "built by the two-blend construction moved from SmFilletNx2Corner" (`p3.rs:1335-1420`).
  - Mixed convexity: `classify_mixed` makes a Mixed event, or `classify_roll_over` takes it. Else
    it is the **old ThreeByTwoMixed** (`p3.rs:1334`).
  - Every edge blended, 3 blends over 4 edges: the **old FourByThree** (`p3.rs:1431`).
  - Every edge blended otherwise: the old ConvexNByN or ConcaveNByN (`p3.rs:1441-1445`). The
    session then overrides it (below).
  - Other counts: `None // MSG: Corner Case Not Implemented` (`p3.rs:1405, 1433`). This is the
    old "no kind" that the new rule forbids. It is not silent: the executive records
    `CornerUnsupported` (`fillet_executive/p1.rs:1323-1393`), and the fork has
    `SMLIB_ERR_CORNER_UNSUPPORTED` (-1111). What the new rule removes is the unnamed case, not the
    error (corrected after review-61).
- **The session overrides** (`fillet_executive/session.rs`):
  - `:2500-2545`: a chamfer NxN corner goes to `blend_corner::chamfer::classify_chamfer`. "A corner
    the event declines (an edge not straight, a face not a plane, a chamfer not of constant width)
    stays the pinned 3x3 one". That 3x3 corner is **old**.
  - `:2550-2605`: an NxN corner goes to `classify_ball`, which gives a Ball (new).
  - `:2610-2640` and `:3180-3260`: it goes to `classify_setback`, which gives a Setback (new). This
    covers unequal radii and curvature-continuous sections without being asked (`:3187-3189`).
    "Elsewhere ... it stays the N x N corner" (`:3185`), which is **old**.
  - `:5731`, `:5794`, `:5809` and `:5811` build the old Nx2, Nx1, 3x2 and default NxN patches. The
    default patch is Coons, N-sided, or `make_analytic_corner_surface` (`fillet_corner/p2.rs:915`).

### 0.5 Where a new-system path still reaches old code (READ)

| # | Fallback | Where |
|---|---|---|
| F1 | **The analytic try-chain is the default of a bare `Kernel`.** The exact classes run only with `fixes.fillet_exact_round_classes`, and `UpstreamFixes` is "opt-in ... all off by default" with a derived `Default`. The fork deleted the try-chain (step 2, smlib #77). (Corrected after review-61.) SolidCraft's preset is "every `UpstreamFixes` flag on" (`UpstreamFixes::all()`, `solidcore/tests/support/solidcraft_fixes.rs:1-13`), taken through `solidcore-ops/src/session.rs:130` (`set_fixes`) and `:168` (`current_fixes`). So the app runs the exact classes, and `c3_exact_by_class` (`p4.rs:2275`) has no fall-through: every failure is an error. The try-chain is reached only by a bare `Kernel`, in tests; dropping it loses nothing the app uses | `fillet_solver/p4.rs:504-507` (else `c3_try_analytic`, `:2220-2265`); `fillet_geom/p3.rs:1319`; `kernel.rs:1075-1092`; `solidcore-ops/src/session.rs:60, 130, 168, 200-204` |
| F2 | Chamfers use the old ruled `LinearCrossSectionFsg`, not the section family's order 0. The fork's design doc says the chord is order 0 of the one generator | §0.4; `procedural_blend.rs:112-117` notes the chord record is "not the fork's" (#2486) |
| F3 | The round uses the upstream `CircularCrossSectionFsg` class, with the exact-circle fixes, not the section family's order 1. The geometry is the new system's exact circle; the code path is the old class | `fillet_solver/p1.rs:252-470`; `section.rs:1127` (order 1 also exists in Gⁿ) |
| F4 | A chamfer corner that `classify_chamfer` declines falls to the old 3x3 corner | `session.rs:2504-2505` |
| F5 | An NxN corner that is neither a Ball nor a Setback falls to the old default patch. The fix flag `fillet_patch_keeps_fillet_edges` (WR-546) exists only for that patch. This understates it; F11 is the largest fallback in the pin (corrected after review-61) | `session.rs:3185, 5811`; `fillet_corner/p2.rs:915` |
| F6 | FourByThree and ThreeByTwoMixed are still the old classes. Solidcore #2348 (four blends over three faces) is the open port | `fillet_corner/p3.rs:1334, 1431`; `session.rs:5809` |
| F7 | End, Turn and Join are new events built from moved old constructions (the Nx1 end patch, the Nx2 two-blend construction). This is the new system's own choice ("moved unchanged"), not a fallback, but the code is old in shape | `fillet_corner/p3.rs:1497-1530`; `topology/nx1_end.rs` |
| F8 | The dispatch can still return no kind ("Corner Case Not Implemented"). It is recorded as `CornerUnsupported`, so it is already an `Unsupported` error, not a silent case (§0.4.1) | `fillet_corner/p3.rs:1405, 1433`; `fillet_executive/p1.rs:1323-1393` |
| F9 | Offset reblend forces `Circular` | `offset/offset_faces.rs:1108` |
| F10 | `identify_fillet` and fillet removal work on the old section kinds | §0.4 |
| F11 | (Added after review-61.) `classify_setback` returns `None`, and the corner stays the **old uncertified NxN patch**, in each case listed below this table | `blend/blend_corner/setback.rs:1192-1340`; `session.rs:2610-2640` |
| F12 | The Ball takes only Circular rounds. A declined Ball with nothing asked goes to the old NxN | `blend/blend_corner/ball.rs:574` |
| F13 | `MIXED_CORNER_RAILS_CROSS` returns `Unsupported` ahead of ThreeByTwoMixed | `fillet_corner/p3.rs` ~1330 |
| F14 | Two events this report's first draft omitted: Chamfer (Mitre or Facet; four or more non-coplanar chamfers get bilinear patches; three chamfers of both convexities are refused) and Roll-over (refused with `CornerUnsupported` until built) | the pin's design doc §3.2 |
| F15 | The setback-scale bisection assumes monotonicity, "checked on a grid of cases, not proved". A certified fill sits on an unproved search | the pin's design doc §3.2 |
| F16 | G2 corners build with `fit: exact` only; `reduce` and `taper` fail (two known-failing kernel cases) | `kernel_cases_known_failing/fillets/9-fillet-corner-curvature-with-{reduce,taper}-fit.json` |

The `None` cases of F11:

- a section that is neither Circular nor BlendCurve, such as a chamfer among rounds;
- a solver that is not ConstRadius (Chord and constant-distance rounds), or offset radii that can
  change (variable rounds);
- r0 ≠ r1;
- an edge with no curve;
- two edges tangent at the vertex;
- a face that is neither a plane nor a natural quadric, such as a B-spline face;
- between planes, an edge that is not straight;
- a curved face where the corner is not one-kind Circular. This includes **a G2 corner beside a
  curved face**;
- mixed section kinds with nothing asked;
- three equal Circular rounds with nothing asked (the Ball's case; if the Ball declines, the
  corner stays NxN);
- a curved face with an order other than 1 and nothing asked;
- mixed convexity with an order other than 2, or beside a curved face.

The pin refuses only when an ease is asked beside a curved face (`session.rs:3175-3260`). Every
other case above is built today, with the old patch.

### 0.6 Old-only capabilities and how the new system covers them (JUDGED)

| Old capability | New-system cover |
|---|---|
| The 4x3 and 3x2 mixed corners | Setback. "Everything else is a Setback" (the fork's design doc §3.2), and mixed convexity is already a Setback event (`blend_setback/saddle.rs`). Four blends over three faces is solidcore #2348 |
| The NxN Coons, N-sided and analytic corner patches | Ball for three equal rounds, else Setback with its certified fill. Where the fill does not certify, the Setback's last step: one N-sided hole fill, returned with `Unproven` (§7 ruling 3) |
| The F11 cases (unequal radii, chord and variable rounds, B-spline faces, edges not straight between planes, mixed kinds) | `classify_setback` extended: the Setback geometry needs only the rails and contact lines, not ConstRadius; a variable round takes its law's radius at the vertex. Until each certifies, the `Unproven` hole fill. Tangent edges at the vertex go to the Join or the tangent end, else `Unsupported` (added after review-61) |
| Chamfer corners where an edge is curved, a face is not a plane, or the width varies | Generalise the chamfer event (a planar close is only one case). Or take the Setback with order-0 fills: the chord rows are G0, so the fill is a G0 fill of the setback split. Solidcore #2345 is the port |
| The analytic cylinder, cone, torus and sphere | The exact classes (`fillet_exact`), with no flag |
| The Linear ruled chamfer | The section family at order 0 (the chord on the same rails) |
| The upstream Circular class | The section family at order 1 (the exact rational circle) |
| The Eiffel tower section and the secant solver | Nothing: neither has a production caller. Asymmetric shapes are the two-distance chamfer and the variable round. Drop them |
| Corners the old code could not name (`None`) | Setback, with the `Unproven` hole fill as its last step; `Unsupported`, naming the corner kind, only where even that is not built. A refusal that names a size only where the size really does not fit (corrected after review-61: valid input is not refused, 01 §4.2) |
| Fillet removal and recognition | Rewritten against the new definitions. A body made by solidcorex carries its procedural record, so removal reads the definition; recognition is only needed for imported bodies and must learn the Gⁿ and conic sections |

### 0.7 The `scx-blend` scope, new system only (JUDGED)

Not ported, in non-test lines:

| Part | Estimate |
|---|---|
| Old corner patches and their topology: the analytic and variable patches of `p2.rs`, parts of `p3.rs`, about 1,050 lines of `topology/` (`concave_4x2`, `concave_nxn`, `mixed_3x2`, the 4x3 part of `end_corners`), and the session's old patch calls. The dispatch becomes the new classification. `p1.rs` (the shared corner machinery, §0.3) and the topology files the new events use are kept | 4,000 to 7,000 |
| `EiffelTowerFs`, `EiffelTowerCrossSectionFsg`, `SecantConstantRadiusFs` | about 1,300 |
| The try-chain (`fillet_solver/p2.rs` `calc_*`, `c3_try_analytic`, `can_make_analytic`, the `fillet_geom/p3.rs` recalculation) | 1,500 to 2,500 |
| `LinearCrossSectionFsg` and the approximated-arc path of `CircularCrossSectionFsg` (folded into the one section generator) | 1,000 to 2,000 |
| **Total** | **about 10,000 to 15,000** |

- (Corrected after review-61.) The first draft counted `p1.rs` as deletable, which it is not, and
  put a 2,000-3,000 line test row in this non-test table. The tests of the removed paths
  (`fillet_corner` tests, `fillet_custom_solver` tests, part of `p3_tests.rs`) go with them but are
  not counted here.
- `scx-blend` would shrink from 01 §1.1's 150,425 lines to about **135,000 to 140,000**, a cut of 7
  to 10%. `fillet_removal`, `identify_fillet`, `feature_executive` and `hcr` already go elsewhere
  in 01's plan (`scx-local`, `scx-hlr`), so they are not counted here. Removal and recognition are
  rewritten there against stored blend definitions.
- There is new work to set against the cut, not netted off: the Setback extensions of §0.6 (the
  corners the 4x3, 3x2 and NxN classes took, #2348), the `Unproven` hole fill, the generalised
  chamfer event (#2345), the Roll-over, and the conic with its corner cases (§7 ruling 4).
- **`scx-surface`:** little change in size. The definition, section and certificate are already
  there (01 §1.1). The section family takes orders 0 and 1 in place of the separate FSG classes.
  The conic adds a few hundred lines, with its exact classes (a rational quadratic extrusion, the
  conic revolved) beside the circle's (§4).
- **`scx-offset`:** no change in size. One behaviour change: reblend carries the blend's recorded
  section (order and fullness), not `Circular` (F9).
- The 01 §1.1 `scx-blend` row is updated to this scope (§7 ruling 2).

---

## 1. Verdict

**The claim is false for G2 and true for conic sections.** The two halves need different
decisions. The G2 that exists is the new system's.

| # | Finding | Evidence | Label |
|---|---|---|---|
| V1 | The pin has a complete Gⁿ cross-section, orders 2 to 4 (G2, G3, G4), with a thumbweight (fullness) and a per-span certificate. It is the only generator for blend-curve requests | `solidcore/src/blend/fillet_solver/p1.rs:252-267` (the `GnSection(GnSectionFsg)` variant; "every blend-curve request is built by it"); `solidcore/src/blend/fillet_section/section.rs:137-159, 1145-1149` | READ |
| V2 | It can be reached today. The kernel API takes `continuity` 1-3 and a thumbweight, and the ops layer has `Blend::Curvature` (order 2 at the equal-area thumbweight) | `solidcore/src/blend/fillet_executive/api.rs:187-245, 374-410`; `solidcore/src/blend/fillet_executive/p2.rs:836-866`; `solidcore-ops/src/blend.rs:184-196, 902-910` | READ |
| V3 | No feature switch gates it. None of the 11 `UpstreamFixes` flags is for Gⁿ. The module header still calls it "opt-in (#2250)", but that note is stale | `solidcore/src/kernel/fixes/`; `solidcore/src/blend/fillet_section.rs` header | READ |
| V4 | It is tested against truth. A curvature round removes the ball's area to 1e-6 relative. Two planes meet the section to order n+2. A budget refusal is tested, and so is scale invariance | `solidcore-ops/tests/blend.rs:282-293, 2975`; `section.rs:2689, 2715, 2776, 2813` | READ |
| V5 | G2 also reaches corners. (Corrected after review-61.) In the pin, a G2 corner of three or more blends, between planes or natural quadrics, of one kind, is a Setback, classified without being asked. Beside a curved face, and in the other F11 cases, the pin builds the old NxN patch. Two-blend G2 corners are a Turn or a Join (`blend_corner::classify` ignores the section kind), and the order reached at their seams is not checked. Setback fills can be G1 or G2, and rounds can be eased into the G2 section. The fork's design doc §3.2 says every G2 corner is a Setback; that is the design, not the pin's code | the fork's design doc §3.2; the pin's design doc §3.2; `blend_corner::classify`; `blend/blend_corner/setback.rs:1192-1340`; `fillet_executive/session.rs:3187-3189`; `blend/blend_setback.rs:1-60`; `blend/blend_corner/setback.rs:1175-1320` (`g2 = one_kind && kind == BlendCurve`); `session/ease.rs:500-524` | READ |
| V6 | Variable-radius Gⁿ exists (`GnLawRound`, a ball whose radius follows a law) | `section.rs:207-213, 1281` | READ |
| V7 | Upstream SMLib already had G1/G2/G3 blend-curve sections (`SmBlendCurveCrossSectionFSG`), built as uncertified lofts through sampled sections. That is the old system. The fork replaced it with the certified Gⁿ section, step 3 of the new system. So G2 is solidcore's own algorithm, and its kept form is the new one | upstream `source/SMLib/inc/SmFilletSolver.h:468-511`, `source/SMLib/src/SmFilletStandardSolver.cpp:3997-4490`; fork `source/SMLib/src/SmFilletSection.cpp:1-48`; the fork's design doc §6 step 3 | READ |
| V8 | The exact procedural blend record stores the section's order and thumbweight, so stored blends keep Gⁿ exactly | `solidcore/src/surface/procedural_blend.rs:2, 112-117, 264-265, 300` | READ |
| V9 | solidcorex has already ported Gⁿ's layer-0 mathematics (series, interval, ring, Taylor models, Krawczyk), and 01 plans the station, section and certificate into `scx-surface`. The "deferred" row in 01 §1.1 contradicts 01's own inventory and the offset dependency | `crates/scx-num/src/{series,interval,ring,taylor_model}.rs`; `crates/scx-algo/src/krawczyk.rs`; 01 inventory row (`fillet_section` to `scx-num` and `scx-surface`), 01 §1.1 `scx-surface` row, and the offset row (41 references to the certified section) | READ |
| V10 | No conic cross-section exists anywhere: not in the pin, the fork or upstream, old or new. Every "conic" hit is something else: `fillet_exact`'s `Conic` classes (the round's spine is a plane conic and the section is still circular), a ball radius called rho in corner code, the corner curve in `SmBlendCornerMixed`, and the setback's "spring conic" | `solidcore/src/blend/fillet_exact.rs`, `fillet_exact/rational.rs`; `blend/fillet_corner/topology/join.rs:522-590`; fork `SmBlendCornerMixed.cpp:452, 1643`, `SmBlendSetback.cpp:1639` | READ |
| V11 | A conic section is a small addition to the new section family, not a new algorithm family. It is a rational quadratic between the two rails, with the apex where the faces' tangent lines meet in the section plane and middle weight rho/(1-rho). It uses the same station, rails and events as the circle | §4 | JUDGED |
| V12 | (Corrected after review-61.) The new system's G2 has a shape factor with a default. The design doc §2.2 says "Two parameters shape it: Thumbweight w… Its default is equal area". The fork's public C API exposes it: `smlib_blend_order(brep, order, thumbweight, …)` ("raising it pulls the section towards the edge"), with `SMLIB_BLEND_THUMBWEIGHT 2.0` as the equal-area default, added in step 3 itself (fork 0bd3f4d, 1947a45). The §5 line "equal-area thumbweight per station, with no user shape factor" describes the default curvature kind, not the generator. The first draft read it as a ban on a shape factor and called the kernel's free thumbweight residue; both were wrong. The contract's `fillet_g2` (a middle radius) is a second consumer of a shape factor | the fork's design doc §2.2, §5; fork `capi/include/smlib.h:686-711`; `api.rs:374-410` | READ |

**So:**

- "solidcore has no G2" is wrong. G2 must not be deferred. Deferring it would drop an algorithm
  solidcore has (against R5) and leave the offset port without its dependency. The G2 to port is
  the new system's Gⁿ section, with its corners through the new events (§7 rulings 3 and 6).
- "solidcore has no conic" is right. A conic section is outside R5 in the narrow sense. But it is
  one member added to the new system's section family, on its station and events. Every
  permissive kernel that has it builds it the same way.

### 1.1 Real gaps around G2 (not "no algorithm")

| Gap | Evidence | Label |
|---|---|---|
| The ops layer exposes only order 2 at the equal-area thumbweight. Orders 3 and 4 reach the kernel API but not `solidcore-ops`. A free thumbweight also reaches the kernel, as the fork's C API has it (V12) | `solidcore-ops/src/blend.rs:184-196, 344, 902-910` | READ |
| The brep-conformance `fillet_g2` asks for a size and a middle radius. SolidCraft's adapter leaves it unscored because the curvature round takes one size at the equal-area weight. Its `fillet_setback_g2` corner meets the rounds only tangentially | springcraft-ai_brep-conformance-solidcraft `src/main.rs:1264-1285` | READ |
| The pin's setback fill can be G2 to the rounds, and G2 to the faces after an ease, on planes. Beside a curved face the Setback declines a G2 corner, which is then built as the old NxN patch (F11); only a corner there asked for an ease is refused (`session.rs:3192-3195`). The G2 fill held convex is open (#2347). The corpus shows this is the weakest area: 9 of the 12 known-failing fillet cases are G2 corners (review-61 §2.1) | `blend/blend_setback.rs:39-53` (`FillOrders`); `blend/blend_corner/setback.rs:1316-1320` | READ |
| `identify_fillet` (old) recognises upstream's Hermite blend sections (kinds 3, 4 and 5 are G1, G2 and G3 interpolants of degree 3, 5 and 7). A Gⁿ rational section would most likely classify as "other" (6) | `solidcore/src/blend/identify_fillet.rs:219-345, 740` | READ (code); JUDGED (outcome) |
| Offset reblend turns a G2 round into a circle (F9) | `offset/offset_faces.rs:1108` | READ |
| The fork's agent doc says "thumbweight > 1 fuller", but the code takes w < 1 or the equal-area marker 2.0. The doc is stale | fork `.agents/operations/fillets.md:58-81`; `section.rs:1145-1149` | READ |
| The fork's doc says G2 and G3 are not guaranteed across corner patches. That note predates the Setback rule for G2 corners, but it still holds in the pin for two-blend Turn and Join corners and for the F11 patches | fork `.agents/operations/fillets.md:121-135` | READ |

---

## 2. What the pin and the fork have

### 2.1 The Gⁿ section (new system)

- **The formula** comes from Hartmann 2001 ("Parametric Gⁿ blending of curves and surfaces", The
  Visual Computer 17):
  - S(t) = f·T1(1-p1) + (1-f)·T2(p2);
  - f = (1-t)^(n+1) / ((1-t)^(n+1) + t^(n+1));
  - p1 = t/(1-w+wt) and p2 = t(1-w)/(1-wt).
  - T1 and T2 are truncated power series of the feet on each face, which the section solves for.
  - The section is a rational curve of degree 3n+1. Its control points are linear in the face jets
    (READ, `solidcore/src/blend/fillet_section.rs` header).
- **Thumbweight w** controls fullness. w near 1 approaches the corner, and w towards -∞ approaches
  the chord. Valid values are w < 1, or 2.0, the equal-area marker. The equal-area weight is solved
  so that the round removes the ball's area, r²(cot(α/2) - (π-α)/2). It is the default
  (`section.rs:128-131, 1151-1170`). The fork keeps w as a public parameter with that default
  (V12); the first draft's "the only shape" was wrong (corrected after review-61). (READ)
- **Order 1** is the ball's exact circle (`section.rs:1127`). (READ)
- **Contact** is order n+1 with a face, or n+2 between two planes (`section.rs:2348-2359`). (READ)
- **Certificate.** Each span of the fitted surface is bounded against the definition. The tools
  are a Hermite remainder of order h⁴, interval arithmetic, Krawczyk and Taylor models
  (`fillet_section/{certificate,interval,tay,taylor_model,series,definition_box}.rs`). (READ)
  - Budget: `GN_HALVING_BUDGET` 128, `GN_HALVINGS_PER_SPAN` 3, `GN_MAX_STATIONS` 4096 and a stall
    limit of 4 halvings (`section.rs:109-122`).
  - The budget's measured basis is 579 certificates, of which 576 were certified.
  - The section records `certified_bound` and `certified_spans`.
- **Size:** about 22,300 lines in `blend/fillet_section/`. The largest files are certificate.rs
  (7,433), station.rs (3,027) and section.rs (2,984). (READ)
- **The executive** maps `params.continuity` to the order. A `BlendCurve` request becomes
  `GnSection(GnSectionFsg::new(order, thumbweight))`. An invalid pair returns
  `BlendFailure::GnSectionInvalid` (`fillet_executive/p2.rs:836-866`). The `prepare_*` session
  calls carry continuity and thumbweight (`fillet_executive/session.rs:2042-2360`). (READ)
- **Corners and transitions:**
  - Eased corners use `GnSectionFsg::new(2, 0.0)` as their target (fork 22d258a).
  - G2/G3 rail interpolation appears in `fillet_geom/p2.rs:766, 4641`.
  - Corner paths also appear in the old `fillet_corner/p1.rs:3377` and `fillet_corner/p2.rs:1616`.
    These are not needed on the new path, where a G2 corner goes through the new events (§7 ruling 3).
  - The setback fill orders are in `blend_setback/graph.rs`, which reports "no G2 fill up to degree
    9 is certified convex" at :1004. The mixed G2 corner is in `blend_setback/saddle.rs:2`.
  - (READ)
- **Ambient state:** the equal-area root is memoised on the kernel (`kernel.rs:1390`,
  `blend_w_memo: RefCell<ThumbweightMemo>`, reset per operation in `section.rs:417-441`). (READ)
- **Completeness:** order, thumbweight, variable radius, corners, certificate and exact record
  are all present. The Gⁿ section is complete in the kernel. The ops surface is narrow (§1.1).
  (READ + JUDGED)

### 2.2 Other sections in the pin

- Linear (the chord, or chamfer; old class), Circular (the exact circle, WR-708; old class, fixed),
  and the Eiffel tower (two offset radii and a swept angle, in `fillet_custom_solver.rs`; old and
  unreachable). There is no conic. (READ)

### 2.3 The fork and upstream

- **Fork (new system):** `SmGnSectionFSG` (`source/SMLib/src/SmFilletSection.cpp`, `source/SMLib/inc/SmFilletSolver.h:42, 674`), Apache-2.0. The Python binding is `fillet_edges(..., continuity=1|2|3, thumbweight=...)` (`.agents/operations/fillets.md:15, 58-81`). (READ)
- **Upstream (old system):** `SmBlendCurveCrossSectionFSG(dBlendScale=1.5, lContinuity=1)`, "1, 2, or 3 for G1, G2 or G3". It samples sections at the rail knots (at least 4) and lofts them, uncertified. (READ)

---

## 3. What the permissive kernels have

| Kernel | Sections | Where | Scope | Label |
|---|---|---|---|---|
| springcraft-ai_springrep | conic by rho (`fillet_conic`: a Bézier [a, apex, b] with weights [1, rho/(1-rho), 1]); G2 by Hartmann's weights (`gn_weights`, `g2_section`, an exact rational quintic; `gn_center_w` sets w from the middle radius of curvature); a G2 setback corner (`fillet_setback_g2`, implicit φ plus a B³ fill) | `src/blend/shaped.rs:382-426, 505, 527-531, 569-604`; `src/blend/shaped/corner.rs` | straight convex edges between planes only (`section_blend`, :289): a prism cut by a Boolean | READ |
| rvdende_cadrs | `FilletProfile { Circular, Conic{rho}, Curvature{magnitude}` (a quintic Bézier with zero curvature at the faces), `Asymmetric`, `Thumbweight{center}` (Hartmann; `None` is equal area) `}`; size by radius, width or setback | `crates/cadrs_kernel/src/types.rs:1331-1371`; `crates/cadrs_kernel/src/native/blend.rs:1-11, 157-171, 1282-1302` | straight edges between planes, or revolved for a circular edge; Boolean | READ |
| swperb_anvilcad | `FilletProfile { Circular, ConicRho{rho} (0.5..0.95), ConicRadius{radius}, CurvatureContinuous }` | `crates/anvil-model/src/document.rs:740-750`; `crates/anvil-brep/src/rolling.rs:72-150, 454-459` | **any edge**: rolling-ball stations solved by Newton, a profile per station, a tool by Boolean. Its G2 is a quartic matched to a normal curvature found by finite differences. It is approximate and has no certificate | READ |
| arris-labs_arris | none (the "conic" hits are conical faces) | | | READ |
| bmander_geomsolver | sketch rounds only | | | READ |
| brep-conformance (the contract) | conic arc rho = w/(1+w) (`doc/api.md:195-200`); the blend section with an equal-area thumbweight (:202-210); `fillet_conic` (:738-754, rho = √2-1 gives a quarter circle at a square edge); `fillet_g2` (:758-776, the equal-area middle radius ≈ 1.389 × offset at a square edge); `fillet_setback_g2` (:808-820). B39: a curvature-continuous blend bends at its seams as what it meets does. Q11 and Q12 check for creases and curvature jumps | `doc/api.md`, `doc/contract.md:144, 207` | contract | READ |

**Lessons:**

- Only anvilcad puts a section inside a rolling-ball station, which is the pattern the new system
  already has. Its G2 is weaker than the pin's: approximate, uncertified, and G2 only along the leg
  tangent. (JUDGED)
- springrep and cadrs supply closed-form references between planes, for the conic and for G2 with
  a given middle radius. These are useful truth fixtures, not designs to port. (JUDGED)
- `gn_center_w`, which picks w from a middle radius, is the piece the conformance `fillet_g2` needs.
  The fork's API already takes a thumbweight (V12), so a middle-radius fullness is within the new
  system; it is ruled in §7 ruling 4. (READ + JUDGED)
- Note: the conformance `fillet_g2` at the equal-area weight has a middle radius of about 1.389 ×
  offset at a square edge (`doc/api.md:758-776`). Review-61 computed 1.38860 (bisection on the area
  law for w, then the radius by finite differences). It is a reference value, not a closed form:
  the equal-area w is the root of a transcendental equation. (READ; the value MEASURED by
  review-61)

---

## 4. Proposed design in `scx-blend`, on the new system only (JUDGED throughout)

### 4.1 The section as data

The section is the new system's family, keyed by order, with the conic as one more member.

```rust
/// The shape across a round, given at every station of the rolling ball,
/// between the two rails the station solves.
#[non_exhaustive]
pub enum Section {
    /// Order 0: the chord between the rails (the chamfer).
    Chord,
    /// Order 1: the ball's exact rational circle (the default round).
    Circle,
    /// Orders 2 to 4: Hartmann's Gⁿ section, its fullness solved per station.
    Smooth { order: SmoothOrder, fullness: Fullness },
    /// A rational quadratic between the rails, with the apex where the faces' tangent
    /// lines meet, and the shoulder rho of the way from the chord's middle to the apex.
    /// G1 to the faces, like the circle.
    Conic(Rho),
}

pub enum SmoothOrder { G2, G3, G4 }

/// How full a smooth section is. The thumbweight is solved from this per station;
/// it is never given raw.
#[derive(Default)]
pub enum Fullness {
    /// The round removes the ball's area, r²(cot(α/2) − (π−α)/2).
    #[default]
    EqualArea,
    /// The section's radius of curvature at its middle.
    MiddleRadius(Length),
}
```

(Revised after review-61 and ruled in §7 ruling 4. The first draft had no fullness field, on the
mistaken premise of V12.)

- The section is a field of the fillet spec, beside the size (radius, radius law, setback or
  width) and the chamfer kinds.
- **One generator.** `Chord`, `Circle` and `Smooth` are the one section generator at orders 0, 1
  and 2-4, as in the fork's design doc §2.2. The old `LinearCrossSectionFsg` and
  `CircularCrossSectionFsg` are not ported (F2, F3). A chamfer is `Chord` on the rails the
  constant-distance (or distance and angle) station gives.
- **Fullness for `Smooth`.** `EqualArea` is the default, as in the fork. `MiddleRadius` answers
  the conformance `fillet_g2` exactly; it is one scalar solve for w per station, the same root
  finder as equal area. The pin's raw thumbweight and the 2.0 equal-area marker are not ported: a
  raw w is not offered (R1).
- `Rho` is a checked newtype in the open interval (0, 1), built by a constructor that returns
  `Result`. There is no integer `continuity` (R1, the 04 idiom rules).

### 4.2 How a section plugs into the new construction

- **The station solve is shared.** The spine is O1 ∩ O2 and the rails are the feet (design doc
  §2.1). The station does not depend on the section. Spine tracing, overflow at the crease, the
  cliff edges and the fold and room checks stay as the new system has them (R5).
- **Chord, Circle and Smooth:** the pin's `GnSectionFsg` family, ported as it is, with the
  certificate. `GnLawRound` covers variable radius.
- **Conic:** at each station the apex is where the face tangent lines meet in the section plane,
  the plane through the rails that the circle uses. The middle weight is rho/(1-rho), and the
  section is [rail 1, apex, rail 2] with weights [1, w, 1].
  - When the two tangents are nearly parallel (a nearly tangent edge), the apex runs to infinity.
    This is the same limit at which the circle's angle goes to zero and the tangent end (design doc
    §3.1) takes over. The conic uses that tangent end and does not fall back to the circle: a
    fall-back would be the kind of try-chain the new system forbids.
  - Exact classes: on a straight spine between planes, the surface is an exact rational quadratic
    extrusion. On a circular spine of a revolved pair it is the conic revolved. These two are added
    to the one exact classification (design doc §4). Everything else is the definition with a
    certified net (§4.3).
  - A conic section is linear in (rail 1, apex, rail 2), as Gⁿ is linear in the jets, so the same
    Taylor-model bound applies.
- **Corners, by the new event table** (design doc §3.2):

  (Revised after review-61. "New work" marks a cell with no construction in the pin or the fork.)

  | Event | Circle | Smooth (G2-G4) | Conic | Chord |
  |---|---|---|---|---|
  | End | yes | yes | yes, to be checked for a non-circular section (the end rib S ∩ E) | yes |
  | Join | yes | yes, as the pin builds it; seam order to be checked | yes, to be checked (one shared section) | yes |
  | Turn: Mitre | yes | a Turn in the pin (`blend_corner::classify`), not a Setback; seam order to be checked | new work: the two-blend construction checked for a non-circular section | yes |
  | Turn: Swept | yes | as Mitre | new work (the section revolved) | yes |
  | Ball | three equal rounds | Setback | Setback (no sphere meets a conic) | the chamfer event |
  | Chamfer (Mitre or Facet) | n/a | n/a | n/a | the pin's event; generalised (#2345) |
  | Roll-over | `Unsupported` until built | as Circle | as Circle | as Circle |
  | Setback | everything else | three or more blends | everything else; G1 fill, new work (`classify_setback` takes only Circular and BlendCurve) | G0 fill |
  | Last step of the Setback | the `Unproven` N-sided hole fill (§7 ruling 3) | the same | not built: `Unsupported` | the same |

  - Mixed sections at one vertex are a Setback, filled to the lower order.
  - Every conic cell marked new work is `Unsupported` until it is built (§7 ruling 4).
  - (Corrected after review-61.) A G2 Setback beside a curved face is G1 only today (#2343), and
    the pin builds that corner as the old NxN patch (F11); it refuses only when an ease is asked
    (`session.rs:3192-3195`). The first draft proposed refusing every such corner. That refuses
    valid input (01 §4.2) where the pin builds a closed solid. The ruling (§7 ruling 6) is a G1
    Setback fill, G1 recorded as the continuity on that face, and a warning; refusal only when the
    caller states G2 as a hard requirement. A downgrade that is recorded and reported is not
    silent.
- **Exact record:** `scx-record` stores the section kind with its parameters. The pin already
  stores order and thumbweight (V8), and the conic adds one more variant.
- **Offset reblend** reads the section from the blend's record, so a G2 or conic round stays one
  after an offset (F9).
- **The equal-area memo:** in the pin it is a `RefCell` on the kernel. Here it is either dropped
  (one bisection per definition, cached in the definition's `OnceLock` cell) or kept in the
  operation's local state. The cache key is total and the result is independent of order (R2, R7).
- **Not ported:** the old FSGs, the try-chain (the exact classes run with no flag), the 3x2, 4x3,
  Nx2 and NxN patches and the default Coons patch, the chamfer 3x3 corner, the Eiffel tower and
  secant solvers, old-section-only recognition and removal (rewritten against stored blend
  definitions), and a dispatch that can return an unnamed case (§0.6, §0.7). The shared machinery
  is ported: the `fillet_corner/p1.rs` helpers, the station, the trace, the intersector and face
  insertion.

### 4.3 Certificates, accuracy and refusals (01 §4, 09 §5)

- Each fitted span carries `Cert { claim: Claim::NetToDefinition, method: Method::Taylor |
  Method::Krawczyk, bound }` in the procedural payload, with its uv region (01 §4.1).
- **Budget failures** (corrected after review-61; ruled in §7 ruling 5). The fork's design says
  "no uncertified fill is ever returned" (design doc §1, §3.2). solidcorex's owner rule is the
  binding one: an operation that cannot meet accuracy succeeds and records the accuracy it reached
  (04 §1, 11-26), and valid input is never refused (01 §4.2). The first draft of this section
  proposed refusing; that is withdrawn.
  - A blend that fails its size or room checks is refused with the size that fits (design doc
    §3.5), as a typed refusal. That is input that does not fit, not a failed proof.
  - A span that exhausts the certificate budget succeeds. It records `Basis::Unproven` with the
    measured bound, and the report carries a warning. Smaller spans or a larger budget make it rare
    (fork #96); they are never a reason to refuse.
- Exact cases (Chord and Circle on their exact classes, the conic's extrusion and revolution, Gⁿ
  between two planes on a straight spine where its exact form exists) carry no certificate.

### 4.4 How truth is checked (01 §9.2)

| Check | Reference | Section |
|---|---|---|
| Contact order n+1 (n+2 between planes) at the rails | `scx-testkit` exact NURBS derivatives (`nurbs.rs`) in double-double (`dd.rs`) | Smooth |
| Curvature at a rail equals the face's normal curvature in the section direction (Meusnier) | analytic face curvature (`analytic.rs`) against the section's second derivative in dd | Smooth |
| A curvature jump of less than 1/(4r) within r/100 across each seam, and no crease beyond 0.1 rad | the conformance Q11/Q12 scan, rebuilt in `scx-testkit` | all |
| The equal-area round removes r²(cot(α/2) - (π-α)/2) | closed form; `scx-mass` area of the section | Smooth |
| The middle radius at a square edge is 1.38860 × offset at equal area (corrected after review-61: a reference value, not a closed form, since the equal-area w is the root of a transcendental equation) | reference value computed independently to 1e-9 in dd; the area law above stays the primary exact check | Smooth |
| The conic is the conic it claims to be: dense samples satisfy the implicit equation in barycentric form, λ_P² = 4w²·λ_A·λ_B with (A, P, B) the reference triangle, and its end tangents lie in the faces (G1). (Corrected after review-61: the first draft checked that the shoulder lies rho of the way to the apex, which is the definition of rho) | dd sampling | Conic |
| The conic at the circle's weight is the circle. Exactly: faces at an angle with cos(θ/2) = 3/5, so w = 3/5 and rho = 3/8, and every sample lies on the ball's circle, in exact rationals (`rat.rs`). At a square edge, rho = √2 − 1 gives the quarter circle, checked in dd. (Corrected after review-61: √2 − 1 is irrational, so the square edge cannot be checked in exact rationals) | `rat.rs`; dd | Conic |
| The chord equals the pin's ruled chamfer between planes | closed form | Chord |
| The conic and the equal-area G2 between planes match springrep's and cadrs's closed forms | permissive reference values, recomputed in our tests, not called | Conic, Smooth |
| Certified bound ≥ measured distance from net to definition | dense sampling in dd | fitted spans |
| A G2 Setback's fill is certified convex, or records `Unproven` with its bound | the fill's own certificate, checked against dense curvature sampling | Smooth |
| G2 Turn and Join seams are G2, or the order they reach is reported (added after review-61: in the pin these corners are not Setbacks) | the Q11/Q12 scan across the seam | Smooth |
| A G2 corner beside a curved face records G1 on that face and warns, and refuses only under a stated hard G2 requirement | the recorded continuity against a curvature scan | Smooth |
| Metamorphic: a rigid move or uniform scale gives the moved or scaled round, with the same span count (as `section.rs:2813` does) | 01 §9.2 | all |
| The pin as a smoke detector: Gⁿ results agree with `fillet_edges` within tolerance, and differences go to the truth. The pin's old-path results (try-chain, NxN patches) are not a reference | 01 §9.1 | Smooth, Circle |

### 4.5 Where it lands in 01 §10.2

| Step | Work |
|---|---|
| 5 (`scx-surface`, procedural kinds) | The section family (orders 0-4, with `Fullness`), station, series solve and certificate, as 01 already plans. The definition holds the section kind. Add the conic variant and its exact classes here: it is one generator and costs little |
| 9 (`scx-blend`) | The new system only: `Section` in the fillet spec; the corner classification with End, Join, Turn, Ball, Chamfer, Roll-over and Setback and no unnamed case; the Setback's last step, the `Unproven` hole fill; the exact classes with no flag; overflow at the crease; refusals that name the size; variable-radius Gⁿ. Port none of §0.7's list. **Gate** (§7 ruling 3): before the app switches, side-channel counts (not committed) of which corner path each of the pin's 326 fillet cases reaches set the order of the corner work |
| 10 (`scx-offset`) | depends on the certified section (already in 01). Reblend carries the recorded section |

---

## 5. Risks and the adversarial check

### 5.1 Risks (JUDGED)

| Risk | Note |
|---|---|
| Size of the certificate code | About 22k lines. It is already planned in `scx-surface`, so it is not new work. Its build time weighs on the tier-1 loop (01 notes `scx-surface` as the worst) |
| Corners the old classes took | (Corrected after review-61.) The set is much larger than the first draft said: 4x3, 3x2, the NxN default and every F11 case. They go to the Setback (or the chamfer event), whose last step is the `Unproven` hole fill, so they are built, not refused. `Unsupported` remains only where even that fill is not built (tangent edges at the vertex, 4x3 and 3x2 until #2348, Roll-over). #2348 and #2345 are open. Corpus (review-61 §2.1, from the case files, not traced): 326 fillet cases, 192 round, 93 chamfer and 14 curvature calls, 43 files naming three edges and 38 four, about 31 G2 cases, 17 G2 files naming a curved face, and 9 of 12 known failures are G2 corners. Declined chamfer corners (hole rims, bosses, tubes) are likely the largest bucket. Which path each case reaches is the gate of §4.5 |
| Results change when the try-chain goes | The exact classes become the default. Results move where the try-chain built a near-analytic face differently. This is the new system's own step 2, so the change is intended |
| Chamfers move from the ruled FSG to the chord | Between planes the result is the same plane. Elsewhere the chord on the same rails is the same ruled surface, so little should change. This needs a check |
| Budget failures | 3 of 579 in the pin's sample. They succeed with `Unproven`, the measured bound and a warning (§7 ruling 5; corrected after review-61) |
| G2 corners beside curved faces | A G1 Setback fill, G1 recorded on that face, and a warning, until a curved G2 fill exists (#2343, #2346, #2347). Refused only when the caller states G2 as a hard requirement, as the pin refuses the eased case (§7 ruling 6; corrected after review-61) |
| G2 corners with `fit` other than `exact` | `reduce` and `taper` fail in the pin (F16). They are open work, not a reference to match |
| The setback-scale search | Monotonicity is checked on a grid, not proved (F15). A fill certified on an unproved search should say so in its trace |
| `identify_fillet` blind to Gⁿ | Recognition of imported G2 rounds fails. Bodies made by solidcorex carry the record, so removal of our own rounds does not depend on recognition |
| The conic apex at nearly tangent faces | It runs to infinity. The tangent end takes over, with no fall-back to the circle |
| The conic is G1 only | Users choose it for its look, not for curvature. Its corners follow the circle's, except that a Ball becomes a Setback. No conic corner construction exists yet: the Setback, Ball and Mitre cells are new work and `Unsupported` until built |
| rho limits | The valid range is open (0, 1). anvilcad uses 0.5 to 0.95 as a UI range. We accept the open range and record nothing extra |

### 5.2 Against the owner rules (04 §1)

| Rule | G2 (port the new system's Gⁿ) | Conic (new member of the family) |
|---|---|---|
| New system only | Gⁿ is the new system's step 3. Its corners go through the new events. Nothing old is reached. Pass | It is a member of the new family, on the new station and events, with no fall-back to the circle. Pass |
| R1 idiomatic Rust | `Section` enum, `SmoothOrder`, no integer `continuity`, no 2.0 marker, no raw w. Pass | `Rho` newtype. Pass |
| R2 determinism | The series and certificate use `fmath` and fixed loops. The memo is replaced by a per-definition cell. Pass if the memo is removed | Closed form per station. Pass |
| R3 truth over pin | Checked against dd derivatives, closed forms and the area law, not only the pin. The pin's old-path results are not a reference. Pass | No pin to match. Truth only. Pass |
| R4 one resolution, record accuracy | Per-span `Cert` in the payload. A budget failure succeeds with `Unproven` (04 §1, 11-26). Pass | Same certificate path. Pass |
| 04 §1 11-26 and 01 §4.2: valid input is never refused | (Added after review-61.) Budget failures record `Unproven`; corners the Setback cannot certify get the `Unproven` hole fill; G2 beside a curved face is built G1 with a warning. Pass | Unbuilt conic corners are `Unsupported`, which is honest about the method, not a refusal of the input. Pass |
| 01 §5.1: `Refused` only for invalid input | Where a case stays unbuilt the cause is `BlendError::Unsupported` naming the corner kind, never `Refused`. Setback sizes reported by the pin as "invalid input" are checked when ported. Pass | as G2 |
| R5 keep solidcore's algorithms | **Deferring G2 would break R5**: it is solidcore's algorithm (V1-V8). Dropping the old FSGs, try-chain and corner classes does not break R5: the new system replaced them, and the fork deleted most of them on purpose | A new generator, but it reuses the kept station, overflow, corner and certificate code. R5 is about not replacing solidcore's algorithms, not about never adding one. This needs the owner's ruling (D2) |
| R7 no ambient state | `blend_w_memo` on the kernel goes away | none |
| The new system's shape factor | (Corrected after review-61.) The fork's C API takes a thumbweight (V12), so a user shape factor is within the new system. `Fullness` keeps it typed: equal area by default, or a middle radius; no raw w | Rho is a shape factor like the thumbweight, so it does not break the new system |
| Adversarial: "the contract asks for it" as the only evidence | G2 is used by the app today (`Blend::Curvature`). It is not there only to pass a suite | The conic is a common CAD section (anvilcad's UI, cadrs and springrep all have it), and the contract only reflects that. It is still weaker evidence than G2's, so it is offered as a recommendation, not a requirement |
| Adversarial: does dropping the old system lose a capability? | §0.6: every old capability has a new cover, except the Eiffel tower and secant solvers, which have no caller. Corners the Setback cannot yet certify are built with the `Unproven` hole fill, not refused | No |
| Adversarial: is the "new system" a separate executive to port alone? | No. It lives inside the shared executive, trace and pseudo-B-rep (§0.1). Porting "only the new modules" would leave it with no station or trace. The cut is the old classes, FSGs and try-chain, not the shared core | |

---

## 6. Decisions needed

These are the recommendations as first written, kept for the record. Review-61 disagreed with D3,
D5 and D8 and changed D2 and D6. The owner's rulings are in §7, and they hold where they differ.

1. **Correct the record (recommended).** Research 60 §5 decision 3 and §6, 04 row 11-27, and 01
   §1.1's `scx-blend` row should say two things. G2 (Gⁿ, orders 2-4) is the new system's
   algorithm and is ported with the blend (steps 5 and 9). `scx-blend` ports the new system only,
   as §0.7 restates it (about 135,000 to 140,000 lines against 150,425, as corrected). This report
   does not edit those files.
2. **Add the conic section.** Recommendation: yes, as `Section::Conic(Rho)` at step 5 (in the
   definition) and step 9 (in the spec). It is one member of the new family, on the new station
   and events. Its rho is a user shape factor, which the new system's §5 forbids for G2; the owner
   should say whether that rule extends to the conic. The alternative is to keep it out, which
   leaves the conformance `fillet_conic` unanswered.
3. **A shape factor for G2.** The new system says "equal-area thumbweight per station, with no
   user shape factor". Recommendation: keep that rule. Expose orders G2-G4, equal area only, and
   drop the pin's free thumbweight and 2.0 marker. The conformance `fillet_g2` is then answered
   only at the equal-area middle radius (about 1.389 × offset at a square edge). Overturning §5
   would add a `MiddleRadius` fullness: one scalar solve.
4. **Exact classes by default.** The pin keeps the try-chain as its default and the exact classes
   behind `fillet_exact_round_classes` (F1). Recommendation: port the exact classes with no flag
   and no try-chain, as the fork's step 2 did. Results that move are listed as excuses.
5. **Budget failures.** The new system refuses rather than return an uncertified span.
   Recommendation: refuse with a typed cause (not a fit size), and fix the budget in the
   construction (fork #96). The alternative is an `Unproven` span recorded under R4. This report's
   earlier draft recommended that, and it conflicts with the new system.
6. **Old corners to Setback.** The 4x3, 3x2 and NxN default patches and the chamfer 3x3 corner are
   not ported. Their vertices become Setbacks or the chamfer event, and are refused with a size or
   cause where those do not yet certify (#2345, #2348). Recommendation: yes. The alternative is to
   port the old patches as a stopgap, which the new-system-only ruling excludes.
7. **Chamfer through the chord.** Chamfers move from the old ruled FSG to order 0 of the section
   family. Recommendation: yes, with a check that results between planes are unchanged.
8. **G2 corners beside curved faces.** Recommendation: refuse with the cause until a curved G2 fill
   exists, matching the pin's refusal of the eased case and the new rule against silent changes.
   The alternative is a G1 fill with the face's recorded continuity saying so, which is a silent
   downgrade.

---

## 7. Ruling (11-27)

The owner directed that solidcorex uses the new fillet system, "not the old one and not the 'split'
between them", and that G2 and conic fillets be introduced, using other kernels and the SMLib fork.
The coordinating session ruled on each item below on that direction, this report and review-61.
The rulings are written into 01 (§1.1, §5.1, §10.2), 04 §3 and research 60 (§5 decision 3, §6).

1. **The record is corrected.** G2/Gⁿ (orders 2-4, certified; the fork's step 3) is solidcore's
   new-system algorithm and is ported. The line "G2 and conic deferred" in research 60 (§5
   decision 3, §6), 04 §3 and 01 §1.1 was wrong. It is marked corrected in place in 60 and 01, and
   a 04 row supersedes it.
2. **`scx-blend` ports the new fillet system only.** Not ported:
   - the analytic try-chain. Only a bare `Kernel` reaches it; the app runs the exact classes
     through its fix preset (F1);
   - the old Linear and Circular FSG classes. They are replaced by orders 0 and 1 of the one
     section generator; a chamfer is the chord, as the fork did in 3aae1c4 (F2, F3);
   - the old 4x3, 3x2, Nx2 and NxN corner classes;
   - the Eiffel tower and secant solvers;
   - old-section-only recognition and removal. They are rewritten against stored blend
     definitions (in `scx-local`, per 01).

   The shared machinery is ported: the `fillet_corner/p1.rs` helpers, the station, the trace, the
   intersector and face insertion. About 10,000 to 15,000 non-test lines are not ported (§0.7).
3. **Corners.** Every vertex the old classes handled goes through the new system. The Setback
   gains a last step: one N-sided hole fill, built in the new system. When it cannot certify, it
   returns success with `Unproven` (bound recorded, warning). `Unsupported`, naming the corner
   kind, is only where even that fill is not built. Valid input is not refused (01 §4.2, §5.1; 04
   §1, 11-26). Before the app switches, side-channel counts (not committed) of which corner path
   each of the pin's 326 fillet cases reaches decide the order of the corner work. This is a gate
   in 01 §10.2's blend step.
4. **The section is one enum.** `Chord`, `Circle`, `Smooth { order: G2 | G3 | G4, fullness:
   Fullness }` with `Fullness { EqualArea (the default), MiddleRadius(Length) }`, and
   `Conic(Rho)`. There is no raw w and no 2.0 marker (§4.1). The conic is adopted as new work. Its
   corner cases (Setback, Ball and Mitre on a non-circular section) are listed as new work and are
   `Unsupported` until built (§4.2).
5. **A certificate budget failure** succeeds with `Unproven`, its measured bound and a warning
   (04 §1, 11-26). It is not a refusal (§4.3).
6. **A G2 corner beside a curved face** is built as a G1 Setback fill. G1 is recorded as the
   continuity on that face, and a warning is reported. It is refused only when the caller states G2
   as a hard requirement, through a typed option of the fillet spec, not a set of flags (§4.2).
7. **The exact round classes are the only path**, with no flag. Offset reblend carries the
   recorded section, so a G2 or conic round stays one after an offset (F9).
8. **The two broken truth checks are fixed** as review-61 gave them (§4.4). The conic is checked
   by rho = √2 − 1 at a square edge in dd, and exactly on the circle at a rational-cosine angle
   (cos(θ/2) = 3/5). The shoulder is checked by the implicit equation λ_P² = 4w²·λ_A·λ_B, not by
   the definition of rho. The 1.389 middle radius is a reference value (1.38860), not a closed
   form.
