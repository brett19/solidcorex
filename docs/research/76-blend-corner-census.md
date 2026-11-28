# 76 - Blend corner census: which corner path each fillet case reaches

Written for solidcorex, 2026-11-28. This is the gate of 01 §10.2 step 9 (research 61 §7 ruling 3).
Before `scx-blend` is designed, side-channel counts of which corner path each of the pin's fillet
cases reaches set the order of the corner work. Proposed for review.

- **Trust:**
  - MEASURED: counted on a throwaway copy of the pin (03e16c9ee7) with side-channel counters.
    The counters appended one line per event to a file named by an environment variable. They
    were never committed, and the copy was removed afterwards.
  - READ: read from the pin's code or the corpus files.
  - JUDGED: a design judgement.
- **Paths:** relative to the pin's `solidcore/crates/`. Short `blend/...` paths are under
  `solidcore/src/`. Line numbers are the pin's, not the instrumented copy's.
- **The run:**
  - The kernel rig's corpus was replayed through `solidcore-ops` by its own runner
    (`solidcore-ops/tests/kernel_cases.rs`, `KERNEL_CASES_AREA=fillets`), one process per case,
    with SolidCraft's fix preset. This is what the app runs.
  - The grades were the pin's own: 302 pass, 0 fail, 12 known-failing (listed), and 12 not run
    because they are unsupported in the runner's own grading (§1).
  - The goldens' `blend.*` units were also counted, as a second view of which code is reached at
    all (§6).

## 1. The 326 cases (MEASURED, READ)

**326 is right.** `solidcore/tests/kernel_cases/fillets/` holds 326 case files (`*.json`, not
counting the `.bodies.json.gz` records and `.jpg` renders: 932 files in all). The
known-failing list holds 12 of them (`kernel_cases_known_failing/fillets/`).

| What the case replays | Cases | Notes |
|---|---|---|
| At least one kernel blend (`blend_dropping` 293 calls, `chamfer_sides` 11, `fillet_variable` 1) | 300 | the census below |
| … of which no corner was made | 3 | refused before any corner: `2893-chamfer-distance-and-angle-past-the-wall-refused`, `cov-fillet-turn-swept-pocket-sharp-corner-refused-r1` (the Swept refusal, §3), `cov-junction-y-junction-120-round` (listed) |
| No kernel operation at all (`calls` empty, `operation_expressible: false`) | 16 | SolidCraft refuses before the kernel (setbacks that do not fit, picks off any edge, a smooth seam), or has no command (`fit: reduce`/`taper` with `corner: g2`) |
| A recorded result only (`tagged` + `stitch`, or a `boolean`) | 8 | the G2 corner cases (`9-fillet-corner-r5-curvature-default-setback`, `cov-fillet-g2-pocket-corner`, `every-box-corner-three-edges-r3-curvature-setback-3-5`, the `report-fillet-curvature-three-edges-at-bar-corner*` and `report-fillet-g2-corner-of-bar-running-into-cylinder*` cases): the trace starts from the filleted body, so the corner is never replayed |
| `blend_setback_open` (unported in the runner) | 2 | `cov-fillet-g2-corner-beside-a-cylinder`, `cov-fillet-g2-oblique-three-edge-corner` (both listed) |

So **26 cases reach no corner code** in this replay. Among them are almost all of the G2/setback
cases that review-61 §2.1 counted from the case files (§8, Q1).

## 2. Inventory of the corner code (READ)

### 2.1 Entry points

| Stage | Where | What it does |
|---|---|---|
| Ops front 1 | `solidcore-ops/src/blend.rs:1894-1910` (`separated_end_passes`, `:1250`) | two concave blends separated by an opening in their floor: one session per end. **Reached by no case** |
| Ops front 2 | `solidcore-ops/src/blend.rs:1912-1934`; `solidcore-ops/src/blend/exact_round.rs` (`round_on` `:1078`, `round` `:139`); kernel side `blend/exact_round.rs` | constant rounds between planes built in closed form (cylinder rounds, square plane ends, the sphere where three equal rounds meet). Anything else, or a result that does not finish, goes to the sessions |
| Session pass | `solidcore-ops/src/blend.rs:1007` (`pass`) → `session.rs` `create_corners` (`fillet_executive/session.rs:2372`) → `do_filleting` (`fillet_executive/session/finish.rs:994`) | one fillet session; ops may run several passes (overflow, roll-over second pass, fit retries) |
| Corner loop | `blend/fillet_executive/p1.rs:1336-1410` (`create_fillet_corners_recording_vertex_errors`) | per filleted vertex: `Created`, `Refused` (whole pass fails `CornerRefused`), `Unsupported` (noted `CornerUnsupported`) |
| Dispatch | `blend/fillet_corner/p3.rs:932-1450` (`FilletCorner::create_noting_roll_overs`) | the corner kind and blend event from the solver count and edge count (§2.2) |
| Reclassify | `fillet_executive/session.rs:2504-2547` (chamfer), `:2558-2609` (Ball), `:2615-2650` (Setback), `:2657` (`SETBACK_NOT_BUILT`), `:2677-2711` (Swept); `:3199-3274` (`set_back_marked_corners`) | the new events that override the dispatch's kind |
| Patch | `fillet_executive/session.rs:5517-5813` (`faces_corner_geometry`) | Ball, Setback, Mixed, Swept, Chamfer, Join (no patch), AtSmooth cap, Nx2, Nx1, 3x2, default |
| Topology | `fillet_executive/session.rs:8938` (`vertices_make_topology`) → `blend/fillet_corner/topology.rs` and `topology/*.rs` | by the kind's corner type: Nx1, Nx2, convex NxN, concave NxN, 3x2, end corners |

### 2.2 The dispatch (`fillet_corner/p3.rs`)

| Solver edges at the vertex | Branch | Result | Line |
|---|---|---|---|
| 1, closed edge | cliff contact; G1 to itself; one edge, not G1 | CliffJoin / Join / closed Turn; else `None` | 1087-1128 (None 1116, 1128) |
| 1, open edge | rails meet (Nx1 degenerate) | End RunsOut (kind Degenerate) | 1134-1138 |
| | lamina edge | End Open (kind Open) | 1140-1144 |
| | runs on into an edge not blended | End AtSmooth (kind Nx2) | 1146-1162 |
| | otherwise | End OnEndFace (kind Nx1) | 1164-1168 |
| 2 | knife edge | `None` | 1203 |
| 2, two edges | G1 / Nx1-degenerate / else | Join / Turn / `None` | 1210-1230 |
| 2, 3-4 edges, mixed convexity | sheet edge | `None` | 1257 |
| | `classify_mixed` (`blend_corner/mixed.rs:524`) | Mixed | 1262-1313 |
| | four edges, not Mixed | `Unsupported MIXED_CORNER_OF_FOUR_EDGES` (may note a Roll-over) | 1301-1303 |
| | rails cross | `Unsupported MIXED_CORNER_RAILS_CROSS` | 1329-1331 |
| | otherwise | old ThreeByTwoMixed | 1334 |
| 2, 3-4 edges, one convexity | `blend_corner::classify` (`blend_corner.rs:428`) | Turn or Join (CliffJoin where it can) | 1404 |
| | otherwise | `None` | 1425 |
| 3 over 4 edges | | old FourByThree | 1431 |
| other mismatch | | `None` | 1434 |
| N over N | convex / concave | old ConvexNByN (chamfer flag) / ConcaveNByN, then reclassified by the session (§2.1) | 1441, 1445 |

The session then turns an NxN chamfer corner into the Chamfer event (`classify_chamfer`,
`blend_corner/chamfer.rs:771`; `Declined` keeps the old 3x3, F4). An NxN round corner becomes the
Ball (`blend_corner/ball.rs:543`), else the Setback (`blend_corner/setback.rs:1192`), else it stays
the old NxN default patch (F5, F11). A Turn may become Swept (`blend_corner/swept.rs:281`).

### 2.3 Lines behind each path (non-test lines, to the first `#[cfg(test)]`)

| Path | Files | Lines | Share |
|---|---|---|---|
| Setback (G1/G2 fill, spring curves, certification, curved faces) | `blend_corner/setback.rs`, `blend_setback.rs`, `blend_setback/*`, `blend_corner/on_face.rs`, `blend_corner/piece.rs` | 13,940 | 26.5% |
| Shared corner machinery (every path) | `fillet_corner/p1.rs`, `p2.rs`, `p3.rs`, `topology.rs` | 10,501 | 20.0% |
| End (OnEndFace, RunsOut, AtSmooth, Open) | `blend_corner/end.rs`, `topology/nx1_end.rs`, `topology/end_corners.rs`, `topology/end_smooth.rs` | 5,589 | 10.6% |
| Roll-over | `blend_corner/rollover.rs`, `rollover/*`, `fillet_tangent_rollover.rs`, `session/tangent_rollover.rs` | 4,819 | 9.2% |
| Mixed | `blend_corner/mixed.rs`, `mixed/algebra.rs`, `topology/mixed.rs` | 3,769 | 7.2% |
| Ball | `blend_corner/ball.rs`, `topology/ball.rs`, `session/ball_regular.rs` | 3,119 | 5.9% |
| Exact-round front (kernel + ops) | `blend/exact_round.rs`, `solidcore-ops/src/blend/exact_round.rs` | 2,456 | 4.7% |
| Join (incl. closed Turn) | `blend_corner/join_gap.rs`, `topology/join.rs`, `topology/closed_turn.rs` | 1,994 | 3.8% |
| Chamfer | `blend_corner/chamfer.rs`, `chamfer_side.rs` | 1,975 | 3.8% |
| Separated ends | `blend_corner/separate_ends.rs`, `separate_ends/*` | 1,226 | 2.3% |
| Turn/Mitre (classify, exact mitre) | `blend_corner.rs` | 1,172 | 2.2% |
| Swept Turn | `blend_corner/swept.rs`, `topology/swept_turn.rs` | 996 | 1.9% |
| Old concave NxN, 4x2 | `topology/concave_nxn.rs`, `topology/concave_4x2.rs`, `session/concave_nxn.rs` | 631 | 1.2% |
| Old 3x2 mixed | `topology/mixed_3x2.rs` | 384 | 0.7% |
| **Total** | | **52,571** | |

Not counted here: the session's dispatch arms in `session.rs` and the per-path parts of
`fillet_section`, `fillet_geom` and the intersector, which every path uses.

## 3. The census (MEASURED)

Corners were counted per filleted vertex, per session pass. For a case, a path counts if any
pass made a corner of it. The 297 cases that made at least one corner are counted. "Only" means
the case needs no other path. Exact-round square ends are estimated as 2 × rounds − 3 × corners
per build.

| Path | Cases reaching | Cases needing only it | Corners | Lines (§2.3) |
|---|---|---|---|---|
| End OnEndFace (Nx1) | **184** | 101 | 494 | 5,589 (shared with RunsOut, AtSmooth) |
| Join | **124** | 60 | 272 | 1,994 |
| Turn Mitre | **37** | 20 | 126 | 1,172 |
| Exact-round front, square end | 20 | 17 | 120 | 2,456 (with the ball below) |
| End RunsOut (Degenerate) | 8 | 0 | 11 | in End |
| Mixed | 8 | 0 | 8 | 3,769 |
| CliffJoin | 4 | 2 | 6 | in `blend_corner.rs`, `p3.rs` |
| Exact-round front, ball corner | 4 | 1 | 14 | in exact round |
| Ball (session) | 3 | 0 | 7 | 3,119 |
| End AtSmooth | 2 | 0 | 2 | in End |
| Chamfer Mitre | 2 | 1 | 9 | 1,975 |
| Old ThreeByTwoMixed | 1 | 0 | 1 | 384 + `p3.rs:1906-2110` |
| Setback G2 | **1** | 0 | 1 | **13,940** |
| Turn Swept | 1 | 1 | 4 | 996 |

In all, 1,075 corners were made: 941 by sessions and 134 by the exact-round front.

Refusal and fallback paths reached:

| Path | Cases | Where |
|---|---|---|
| Dispatch `None`, knife edge (F8) | 1 (`1390-chamfer-hole-rim-touching-bottom`, 2 vertices) | `p3.rs:1203`; the pass is refused, as the rig's was |
| `Unsupported MIXED_CORNER_OF_FOUR_EDGES` noting a Roll-over | 1 (`10876-fillet-ledge-convex-and-concave-edges-at-bore-r0-5`) | `p3.rs:1301`; ops runs the Roll-over's second pass, which builds |
| Swept refused (sharp concave corner edge) | 1 (4 vertices) | `session.rs:2700-2711` |
| Ball declined, not Circular | 1 (`cov-fillet-g2-mixed-corner`) | `ball.rs:574` (F12), then Setback G2 |
| **Old Coons corner blend** inside the new End (three-edge cap) | 5 cases (7 vertices): the `22186-*` caps and pockets | `p3.rs:2155` → `create_corner_patch_default_with` → `BSplineSurface::create_corner_blend_on_faces` |
| Old Coons patch of the 3x2 corner | 1 | `p3.rs:2102` |
| `make_analytic_corner_surface` | tried 8 times, succeeded 0 times | `p2.rs:926` |
| Ops retries after a refused pass | 6 cases | 5 are fit retries on `1080-*` and `5158-*`; 1 is the Roll-over above |

Outcomes over the 326 cases:

| Outcome | Cases | Grade |
|---|---|---|
| success: every pass made | 279 | 278 pass, 1 listed |
| fallback: a pass refused, then ops built another way | 6 | pass |
| refused, as the rig was | 15 | 13 pass, 2 listed |
| no corner code reached (§1) | 26 | 17 pass, 9 listed |

**The cumulative cover** of the 297 cases that make a corner, adding paths in the order of §5:

| Paths built | Cases fully covered |
|---|---|
| End OnEndFace (with the exact-round square end) | 118 (40%) |
| + Join | 228 (77%) |
| + Turn Mitre | 263 (89%) |
| + End RunsOut and AtSmooth | 273 (92%) |
| + Mixed (taking the one 3x2 case) | 282 (95%) |
| + CliffJoin | 286 (96%) |
| + Ball (session and exact) | 293 (99%) |
| + Chamfer Mitre | 295 |
| + Turn Swept | 296 |
| + Setback | 297 (100%) |

The per-case table is in the appendix.

## 4. Paths no case reaches (MEASURED)

None of these was reached by any of the 326 cases:

| Path | Line | Proposal (JUDGED) |
|---|---|---|
| End Open (lamina edge, kind Open) | `p3.rs:1140-1144` | build: sheets are in scope and it is a small variant of End. Name `Unsupported(OpenEnd)` until then |
| Old ConvexNByN / ConcaveNByN default patch (F5, F11): no NxN corner stayed old | `p3.rs:1441-1445`, `session.rs:5811` | drop. Per ruling 3 these go to the Setback, with the `Unproven` hole fill |
| `make_analytic_corner_surface` success | `p2.rs:915-1219` | drop (0 of 8 tries succeeded) |
| Old FourByThree | `p3.rs:1431` | drop. Setback (#2348); `Unsupported(FourOverThree)` until then |
| Old concave 4x2 and NxN topology | `topology/concave_4x2.rs`, `topology/concave_nxn.rs` | drop |
| Setback G1 (order 1), marked setbacks (`set_back_marked_corners`), Setback beside curved faces | `setback.rs:1192-1338`, `session.rs:3199` | build. It is the ruling's last step for every corner. The corpus does not replay it (§1, Q1), so the count says nothing about demand |
| `classify_setback` returning `None` (all 13 F11 reasons) | `setback.rs:1209-1332` | not reached: no NxN round corner got that far except the Ball's and the one G2 case |
| Chamfer Facet; chamfer `Declined` (F4) / `Refused` / mixed convexity | `chamfer.rs:771-1008`; `session.rs:2504-2547` | build Facet with the chamfer event. No case reached the old 3x3 fallback |
| `MIXED_CORNER_RAILS_CROSS`, the sheet-edge `None` | `p3.rs:1257`, `:1329` | keep as named `Unsupported` |
| Dispatch `None` at `p3.rs:1116`, `1128`, `1230`, `1425`, `1434`; `SETBACK_NOT_BUILT` | | keep each as `Unsupported` by name, not a silent `None` |
| Separated ends (`separated_end_passes` always `NotApplicable`) | `solidcore-ops/src/blend.rs:1250` | open: no case builds through it. Check its issue's own case before dropping |

## 5. Geometry each path needs from step 5 (READ, JUDGED)

| Path | Needs |
|---|---|
| End OnEndFace | the blend surface (section at a station, rails), the trace to the end face, and the SSI of blend and end face (marching for a non-planar end). A three-edge cap needs a three-sided patch; today it is the old Coons `create_corner_blend_on_faces`, which must be replaced by a new-system fill (the Setback's N-sided fill, or a Coons patch in the section family) |
| End RunsOut | the blend surface; a degenerate (collapsed) end section |
| End AtSmooth | the blend surface; a plane section square to the edge (the cap) |
| End Open | the blend surface; the end section at a lamina edge |
| Join | the blend surface at a shared section; no SSI |
| Turn Mitre | SSI between two blend surfaces (the marcher of research 73); the exact mitre of two equal cylinders (`exact_mitre_curve`) as a closed form |
| Turn Swept | exact torus (the section revolved about the side edge), analytic |
| CliffJoin | the chord/circle section continued across a tangent rim; plane and quadric trims |
| Mixed | two Coons patches in R⁴ (`mixed/algebra.rs`); swept rail surfaces along a face normal; rail-rail crossings |
| Ball | offset surfaces (plane, quadric offsets) meeting at the ball centre; the exact sphere patch |
| Exact-round front | closed forms only: cylinder, sphere, plane trims |
| Chamfer Mitre / Facet | plane intersections; bilinear patches for Facet |
| Setback G1/G2 | offset contact lines, spring curves (rational Bézier), the N-sided G1/G2 fill with certification (convexity, containment, saddle), quadric faces; and the `Unproven` hole fill of ruling 3 |
| Roll-over | rail continued across a tangent seam; the blend surface rolled over a tangent edge; SSI |

## 6. The goldens' `blend.*` units (MEASURED)

The goldens' 25 `blend.*` units were run on the same counted copy. **This view is weak.** The
copy lacks the upstream test models, so 15 units broke on missing files, among them all three
`fillet_corner` units and `fillet_executive_p3`. The units that ran made no session corner. They
emitted:

- 27 dispatch notes from unit calls of the dispatch (mostly chamfer-flagged, `Linear` sections);
- 16 `Created` vertices;
- 90 direct calls of the old Coons default patch (`corner_blend`, n = 2 to 6 sides), from units that
  test the patch itself.

So the goldens reach the old default patch as a unit. They show nothing about which paths user
cases need, and they reach none of the paths in §4 through a session. A full goldens count
needs the upstream models in place (Q6).

## 7. Proposed order of the corner work (JUDGED)

1. **End (OnEndFace), Join, Turn Mitre**, with the shared machinery and the exact-round front.
   These cover 263 of 297 cases (89%) and 892 of 1,075 corners, for about 21,700 lines (41%,
   counting the shared 10,501). The End's three-edge cap must not keep the old Coons patch
   (5 cases): it takes the first new-system fill (step 6 below, or a Coons patch of the section
   family as an interim `Unproven`).
2. **End RunsOut and AtSmooth** (10 cases): small variants of the End.
3. **Mixed** (8 cases), taking the one ThreeByTwoMixed case. Mixed, or a Setback of mixed
   convexity, replaces the old 3x2 class (ruling 2).
4. **CliffJoin** (4 cases).
5. **Ball**, session and exact (7 cases). The exact-round front's sphere is the same geometry;
   build them as one.
6. **Setback**, G1 then G2, with the `Unproven` N-sided hole fill as its last step. Every NxN
   corner the old classes took goes here (ruling 3), and so does the End's three-edge cap.
7. **Chamfer Mitre and Facet**, then **Turn Swept**, then **Roll-over**, then **End Open**.
8. Every other dispatch arm is `Unsupported` by name from the start (§4).

**Why this order.** Each step is weighted by the cases and corners it unlocks, against its
lines. The first three paths are 89% of the cases and need only the blend surface and the
blend-blend SSI that step 5 already plans (research 73). The Setback is the largest single path
(26.5% of the lines) and the corpus reaches it once.

**The strongest case against.** The census measures what the corpus replays, not what users
need. The corpus does not replay the Setback. Most G2/setback cases enter as recorded results,
refusals made before the kernel, or `blend_setback_open`, which the runner does not port (§1).
The order that the ruling implies would also put the Setback first:

- the Setback with its hole fill is the catch-all for every corner the old classes took;
- the End's three-edge cap needs a fill anyway;
- with the Setback in place, every later path is an improvement on a built corner, not a
  missing one, and valid input is never refused (01 §4.2). Under the order above, the corners
  between step 1 and step 6 that would have been Setbacks are `Unsupported` by name. That is
  allowed by ruling 3 only as long as the app has not switched.

The answer (JUDGED) is to keep the order and gate the app switch on step 6. The switch waits
for the Setback in any order, and building it last lets it be designed against the End, Join
and Turn it must meet. It also lets the corpus gain kernel-replayed setback cases first (Q1).

## 8. Open questions

- **Q1.** About 24 of review-61's "31 G2 cases" reach no corner code. They enter as recorded
  results (8), SolidCraft refusals (16, some G2), or the unported `blend_setback_open` (2). The
  Setback's real demand is unmeasured. Should the rig re-capture them as kernel blends, and should
  the runner port `blend_setback_open`, before step 6 is designed?
- **Q2.** The End's three-edge cap reaches the old Coons `create_corner_blend_on_faces` in 5
  cases. Research 61 §0.5 does not list this as a fallback. Should it be added as F17, and what
  is the interim fill: the Setback's, or a section-family Coons patch returned `Unproven`?
- **Q3.** The closed-edge Join and the closed Turn were not told apart from the open ones (both
  are counted as Join/Turn). The Turn Mitre was not split between the exact cylinder mitre and the
  marched SSI. A finer count would show how much of step 1 needs the marcher.
- **Q4.** No case builds through `separated_end_passes`. Is its case missing from the corpus, or
  is the front dead?
- **Q5.** The whole-length tangent roll-over (`roll_over_tangent_edges`) was not counted. Only the
  corner Roll-over via `MIXED_CORNER_OF_FOUR_EDGES` was seen (1 case).
- **Q6.** The goldens view (§6) ran without the upstream test models. A rerun with them would
  show whether any unit reaches the §4 paths through a session, before any of them is dropped.

## Appendix: per case

Paths: End = End OnEndFace, Join, Turn = Turn Mitre, Swept = Turn Swept, xEnd/xBall = the
exact-round front's square end/ball corner, RunsOut, AtSmooth, Mixed, Cliff = CliffJoin, Ball,
ChamMitre = Chamfer Mitre, OLD3x2 = the old ThreeByTwoMixed, SetbackG2. "×n" counts vertices over
all passes. After `;` come vertices that were refused or unsupported, and old Coons patches made
inside a new path. Grade: the pin's (pass, or on the known-failing list).

| case | corners reached (path×vertices) | passes | grade |
|---|---|---|---|
| 1008-fillet-tray-floor-and-three-of-four-corners | Ball×3 End×3 Turn×1 | success (1 pass) | pass |
| 1080-chamfer-beside-openings-fits | End×2 | success (1 pass) | pass |
| 1080-chamfer-equal-slope-basin-1-mm-rim-loop-0-5 | Turn×4 | success (1 pass) | pass |
| 1080-chamfer-equal-slope-basin-1-mm-rim-loop-2 | Turn×4 | success (1 pass) | pass |
| 1080-chamfer-runs-into-openings-cut-by-them | End×2 | success (1 pass) | pass |
| 1080-chamfer-sloped-basin-1-mm-rim-2-past-the-rim | End×2 | success (1 pass) | pass |
| 1080-chamfer-sloped-basin-1-mm-rim-as-wide-as-the-rim | End×2 | success (1 pass) | pass |
| 1080-chamfer-sloped-basin-1-mm-rim-half-its-width | End×2 | success (1 pass) | pass |
| 1080-fillet-equal-slope-basin-1-mm-rim-loop-0-5 | Turn×4 | success (1 pass) | pass |
| 1080-fillet-equal-slope-basin-1-mm-rim-loop-4 | Turn×8 End×8 | fallback (5 refused, then 1 made) | pass |
| 1080-fillet-notched-block-top-outline-r7 | Turn×4 | success (1 pass) | pass |
| 1080-fillet-notched-block-top-outline-r8 | End×12 Turn×6 | fallback (5 refused, then 2 made) | pass |
| 1080-fillet-sloped-basin-1-mm-rim-r0-5-4-edges | Turn×4 | success (1 pass) | pass |
| 1080-fillet-sloped-basin-1-mm-rim-r0-5 | End×2 | success (1 pass) | pass |
| 1080-fillet-sloped-basin-1-mm-rim-r4-2-edges | End×8 Turn×2 | fallback (3 refused, then 1 made) | pass |
| 1080-fillet-sloped-basin-1-mm-rim-r4-4-edges | Turn×8 End×8 | fallback (5 refused, then 1 made) | pass |
| 1080-fillet-sloped-basin-1-mm-rim-r4 | End×2 | success (1 pass) | pass |
| 10876-fillet-ledge-convex-and-concave-edges-at-bore-r0-5 | End×4 Join×1 ; ERR×1 | fallback (1 refused, then 2 made); unsupported corner | pass |
| 1113-chamfer-arc-top-edge-full-wall-height | End×2 | success (1 pass) | pass |
| 1191-fillet-both-rims-of-a-tube-top | Join×2 | success (1 pass) | pass |
| 11991-chamfer-cylinder-edge-ending-on-a-round-tangent-line | End×2 | success (1 pass) | pass |
| 11991-round-cylinder-edge-ending-on-a-round-tangent-line | End×2 | success (1 pass) | pass |
| 12034-round-loft-square-to-circle-circular-rim | Turn×4 | success (1 pass) | pass |
| 12060-chamfer-circular-shoulder-between-tangent-walls | Cliff×2 Join×1 | success (1 pass) | pass |
| 12060-chamfer-closed-shoulder-at-tangent-seam | Cliff×1 | success (1 pass) | pass |
| 12060-round-circular-shoulder-between-tangent-walls | Cliff×2 Join×1 | success (1 pass) | pass |
| 12060-round-closed-shoulder-at-tangent-seam | Cliff×1 | success (1 pass) | pass |
| 1207-fillet-bar-top-edge-after-boss-arc-round-partial-chain | AtSmooth×1 End×1 | refused (1 pass) | pass |
| 1207-fillet-bar-top-edge-after-boss-arc-round-r0-5 | End×2 Join×1 | success (1 pass) | pass |
| 1207-fillet-bar-top-edge-after-boss-arc-round | End×2 Join×1 | success (1 pass) | pass |
| 1207-fillet-bar-top-edge-and-boss-arc-together | End×2 OLD3x2×1 Join×1 ; oldCoons×1 | success (1 pass) | pass |
| 1207-fillet-box-end-edge-into-unblended-round-partial-chain | End×2 | success (1 pass) | pass |
| 1207-fillet-box-end-edge-into-unblended-round | Join×2 End×2 | success (1 pass) | pass |
| 1207-fillet-concave-arc-at-boss | End×2 Join×1 | success (1 pass) | pass |
| 1207-fillet-straight-edge-ending-on-boss | End×2 | success (1 pass) | pass |
| 1213-round-end-circle-of-bent-tube | Join×1 | success (1 pass) | pass |
| 12342-round-wall-and-ledge-edges-r2-5 | xEnd×4 | success (ops front only) | pass |
| 12342-round-wall-and-ledge-edges-r5-refused | End×12 | refused (4 passes) | listed |
| 12342-round-wall-edge-r5-past-the-wall-refused | End×2 | success (1 pass) | pass |
| 1262-fillet-boss-bottom-arc | End×2 Join×1 | success (1 pass) | pass |
| 1390-chamfer-hole-rim-touching-bottom | End×2 Join×1 ; Unsupported×2 | refused (1 pass); unsupported corner | pass |
| 14283-chamfer-tangent-bore-edges | RunsOut×2 Join×1 | success (1 pass) | pass |
| 14755-round-between-cone-and-plane-chamfers | End×2 | success (1 pass) | pass |
| 1488-fillet-slab-end-after-nudged-loft-join | xEnd×2 | success (ops front only) | pass |
| 15437-fillet-round-ends-at-crease-beside-drafted-side | End×1 AtSmooth×1 | success (1 pass) | pass |
| 15505-chamfer-end-cap-between-two-faces-of-one-cylinder | End×2 | success (1 pass) | pass |
| 15505-chamfer-end-cap-on-one-cylinder-face | End×2 | success (1 pass) | pass |
| 1557-fillet-vertical-corners-after-bottom-rounds | xBall×4 xEnd×4 | success (ops front only) | pass |
| 15768-fillet-ledge-concave-edge-beside-bore-mirror-r0-5 | End×2 | success (1 pass) | pass |
| 15768-fillet-ledge-concave-edge-beside-bore-r0-5 | End×2 | success (1 pass) | pass |
| 15858-chamfer-spline-loft-larger-end-rim | Join×1 | success (1 pass) | pass |
| 15858-round-circle-loft-larger-end-rim | Join×1 | success (1 pass) | pass |
| 15858-round-loft-circle-to-ellipse-rim-normal-jump-7e-11 | Join×1 | success (1 pass) | pass |
| 15858-round-loft-circle-to-ellipse-rim-normal-jump-7e-9 | Join×1 | success (1 pass) | pass |
| 15858-round-spline-loft-equal-sections-rim | Join×1 | success (1 pass) | pass |
| 15858-round-spline-loft-larger-end-rim | Join×1 | success (1 pass) | pass |
| 16019-chamfer-1-on-the-top-rim-of-a-cylinder-r125 | Join×1 | success (1 pass) | pass |
| 16019-fillet-1-on-the-top-rim-of-a-cylinder-r50 | Join×1 | success (1 pass) | pass |
| 16654-fillet-drum-rim-beside-a-boss-flush-with-its-top-r0-5 | End×2 Join×1 | success (1 pass) | pass |
| 16654-fillet-drum-rim-under-a-box-standing-across-it-r4 | End×2 Join×1 | success (1 pass) | pass |
| 16654-fillet-drum-rim-under-a-tube-standing-across-it-r4 | End×2 Join×1 | success (1 pass) | pass |
| 16654-fillet-equal-tube-tee-pinched-at-the-sides-r2 | Turn×2 Join×2 | success (1 pass) | pass |
| 16654-fillet-tube-joint-across-a-chamfer-crease-r1 | End×2 Join×1 | success (2 passes) | pass |
| 16654-fillet-tube-joint-through-its-cylinders-own-splits-r4 | RunsOut×2 Join×1 | success (1 pass) | pass |
| 1667-fillet-rim-line-beside-quarter-turns | Join×8 | success (1 pass) | pass |
| 1668-fillet-both-sides-of-thin-wall-r7 | End×4 | success (1 pass) | pass |
| 1668-fillet-both-sides-of-thin-wall | End×4 | success (1 pass) | pass |
| 1668-fillet-outer-top-edge-of-thin-wall-r7-past-inside-corner | - | no blend call replayed | pass |
| 18823-chain-and-wall-window-s1p65-top5p1-d0p4-bands-apart | End×4 Join×2 | success (1 pass) | pass |
| 18823-chain-and-wall-window-s1p65-top5p15-d0p4-bands-apart | End×4 Join×2 | success (1 pass) | pass |
| 18823-chain-and-wall-window-s1p65-top5p2-d0p3881-bands-apart | End×4 Join×2 | success (1 pass) | pass |
| 18823-chain-and-wall-window-s1p75-top5p05-d0p4-bands-apart | End×4 Join×2 | success (1 pass) | pass |
| 18823-chain-and-wall-window-s1p75-top5p1-d0p4-bands-overlap-refused | End×4 Join×2 | refused (1 pass) | pass |
| 18823-chain-and-wall-window-s1p75-top5p15-d0p376-bands-apart | End×4 Join×2 | success (1 pass) | pass |
| 18823-chain-and-wall-window-s1p75-top5p2-d0p3557-bands-apart | End×4 Join×2 | success (1 pass) | pass |
| 18823-chain-and-wall-window-s1p8-top5-d0p4-bands-apart | End×4 Join×2 | success (1 pass) | pass |
| 18823-chain-and-wall-window-s1p8-top5p05-d0p4-bands-overlap-refused | End×4 Join×2 | refused (1 pass) | pass |
| 18823-chain-and-wall-window-s1p85-top4p95-d0p4-bands-apart | End×4 Join×2 | success (1 pass) | pass |
| 18823-chain-and-wall-window-s1p85-top5p1-d0p3616-bands-apart | End×4 Join×2 | success (1 pass) | pass |
| 18823-chain-and-wall-window-s1p9-top4p95-d0p4-bands-apart | End×4 Join×2 | success (1 pass) | pass |
| 18823-chain-and-wall-window-s1p9-top4p98-d0p4-bands-overlap-refused | End×4 Join×2 | refused (1 pass) | pass |
| 18823-chain-and-wall-window-s1p95-top4p95-d0p4-bands-overlap-refused | End×4 Join×2 | refused (1 pass) | pass |
| 18823-chamfer-slot-wall-edge-running-on-into-a-round | Join×2 End×2 | success (1 pass) | pass |
| 190-fillet-disc-rim-past-its-2mm-wall | Join×1 | success (1 pass) | pass |
| 190-fillet-rim-as-tall-as-wall | Join×1 | success (1 pass) | pass |
| 193-paste-copies-the-fillets | xEnd×8 | success (ops front only) | pass |
| 1986-chamfer-arc-edge-of-joined-sectors | End×2 | success (1 pass) | pass |
| 199-fillet-uses-up-end-face | End×8 | success (1 pass) | pass |
| 20421-round-ending-on-a-side-a-hair-inside-its-cylinders-tangent-plane | End×2 Join×1 | success (1 pass) | pass |
| 20450-two-chamfers-that-exactly-use-up-the-face-between-them | End×8 | success (1 pass) | pass |
| 219-pattern-of-filleted-pockets | xEnd×8 | success (ops front only) | pass |
| 22186-free-caps-on-a-through-opening | End×2 ; oldCoons×2 | success (1 pass) | pass |
| 22186-inside-edge-at-rounded-pocket-opening | End×2 ; oldCoons×1 | success (1 pass) | pass |
| 22186-inside-round-in-a-hexagonal-socket | End×2 ; oldCoons×1 | success (1 pass) | pass |
| 22186-seven-inside-edges-at-a-pocket-opening | Ball×3 End×3 Turn×1 ; oldCoons×3 | success (1 pass) | pass |
| 2341-pattern-of-chamfered-bosses | Join×8 | success (1 pass) | pass |
| 237-fillet-concave-edge-between-two-cylinders | End×2 | success (1 pass) | pass |
| 2498-fillet-tiny-end-edges-rounded-rectangle | Join×8 | success (1 pass) | pass |
| 2672-chamfer-one-pick-large-plate | Join×4 | success (1 pass) | pass |
| 2708-fillet-curvature-continuous-angled-ends | End×2 | success (1 pass) | pass |
| 2719-chamfer-as-wide-as-corner-round | Join×8 | success (1 pass) | pass |
| 2752-fillet-rim-of-flat-cut-on-sphere | Join×2 | success (1 pass) | pass |
| 2797-chamfer-two-distances-circular-rim | Join×1 | success (1 pass) | pass |
| 2811-fillet-rib-roots | xEnd×64 | success (ops front only) | pass |
| 2852-fillet-outer-round-concentric-with-inner | End×4 | success (1 pass) | pass |
| 2852-fillet-outer-round-r30-past-inner-round-refused | - | no blend call replayed | pass |
| 2893-chamfer-distance-and-angle-past-the-wall-refused |  | refused (1 pass) | pass |
| 2893-chamfer-two-distances-across-the-whole-top-is-a-cone | Join×1 | success (1 pass) | pass |
| 2893-chamfer-two-distances-elliptical-rim | Join×2 | success (1 pass) | pass |
| 2893-chamfer-two-distances-past-the-bottom | Join×1 | success (1 pass) | pass |
| 2893-chamfer-two-distances-past-the-top-refused | Join×1 | refused (1 pass) | pass |
| 2903-edited-rim-chamfer | Join×8 | success (1 pass) | pass |
| 2903-edited-rim-round | Join×8 | success (1 pass) | pass |
| 2903-original-rim-chamfer | Join×8 | success (1 pass) | pass |
| 2903-original-rim-round | Join×8 | success (1 pass) | pass |
| 2907-chamfer-two-end-outlines-rounded-top | Join×8 End×4 | success (1 pass) | pass |
| 318-fillet-plate-top-outline-as-tall-as-wall | Turn×4 | success (1 pass) | pass |
| 324-fillet-over-hole-cut-by-overlong-cylinder | End×2 | success (1 pass) | pass |
| 324-fillet-over-hole-overlap | End×2 | success (1 pass) | pass |
| 3276-chamfer-edge-of-1mm-protrusion-on-joined-l | End×2 | success (1 pass) | pass |
| 334-fillet-top-outline-after-concave-rounds | Join×4 | success (1 pass) | pass |
| 3369-fillet-edge-ends-on-tangent-seam | End×2 | success (1 pass) | pass |
| 3435-fillet-larger-than-top-ring | - | no blend call replayed | pass |
| 3655-chamfer-edge-order-box | ChamMitre×8 | success (1 pass) | pass |
| 3753-circular-pattern-of-hole-and-its-rim-chamfer | Join×1 | success (1 pass) | pass |
| 3779-chamfer-all-spline-spans | Turn×4 Join×4 | success (1 pass) | pass |
| 3779-fillet-all-spline-spans | Turn×4 Join×4 | success (1 pass) | pass |
| 3868-chamfer-concave-edge-adds-material | End×2 | success (1 pass) | pass |
| 4055-chamfer-bar-edge-over-joined-pin | End×2 | success (1 pass) | pass |
| 4101-chamfer-tapered-plug-corners | Turn×4 | success (1 pass) | pass |
| 4141-fillet-corner-after-wedge-cut-beside-bspline | End×2 | success (1 pass) | pass |
| 4143-fillet-notch-rim-on-bspline-ramp | End×2 | success (1 pass) | pass |
| 4174-fillet-cylinder-base-arc-ending-at-block-edge | End×2 Join×1 | success (1 pass) | pass |
| 4265-chamfers-meeting-on-a-face-as-tall-as-both | End×4 | success (1 pass) | pass |
| 4332-two-distance-chamfer-of-spline-rim | Join×7 | success (1 pass) | pass |
| 4365-fillet-top-outline-chain-ending-at-boss | Join×7 End×2 | success (1 pass) | pass |
| 4369-fillet-boss-base-arc-running-out-at-outer-wall | End×2 Join×1 | success (1 pass) | pass |
| 4702-chamfer-slot-wall-edge-on-outer-cylinder | End×2 | success (1 pass) | pass |
| 477-fillet-cube-corner-rounds-as-wide-as-faces-reduce-fit | xEnd×3 xBall×1 | success (ops front only) | pass |
| 477-fillet-cube-corner-rounds-as-wide-as-faces-refused | - | no blend call replayed | pass |
| 4782-fillet-bar-edge-wider-than-both-faces | End×2 | success (1 pass) | pass |
| 4782-fillet-end-top-edge-half-the-wall | xEnd×2 | success (ops front only) | pass |
| 4782-fillet-full-round-two-end-verticals | End×4 | success (1 pass) | pass |
| 4782-fillet-two-end-verticals-past-a-full-round-reduce-fit | End×4 | success (1 pass) | pass |
| 4782-fillet-two-end-verticals-past-a-full-round | End×4 | refused (1 pass) | pass |
| 485-fillet-d-prism-top-outline-arc-and-flat | Turn×2 | success (1 pass) | pass |
| 4859-chamfer-inside-corner-of-thin-wall-ring | End×2 | success (1 pass) | pass |
| 4989-chamfer-top-edge-of-drafted-face | End×2 | success (1 pass) | pass |
| 5129-fillet-concave-edge-where-cylinder-meets-block | End×2 | success (1 pass) | pass |
| 5157-chamfer-bottom-loop-of-box-with-rounded-corners | Join×8 | success (1 pass) | pass |
| 5158-fillet-adjacent-plate-edges-as-thick-as-plate | End×10 Turn×1 | fallback (3 refused, then 2 made) | pass |
| 5158-fillet-round-just-past-a-10mm-face | End×2 | success (1 pass) | pass |
| 5181-two-distance-chamfer-runs-past-tapered-sides | End×2 | success (1 pass) | pass |
| 5236-fillet-cylinder-end-with-its-own-radius | Join×1 | success (1 pass) | pass |
| 5300-fillet-tube-rim-as-wide-as-its-wall | Join×1 | success (1 pass) | pass |
| 556-fillet-edge-ending-on-curved-boss | End×2 | success (1 pass) | pass |
| 5835-chamfer-sloped-basin-1-mm-rim-0-5-4-edges | Turn×4 | success (1 pass) | pass |
| 5835-chamfer-sloped-basin-1-mm-rim-2-two-edges | End×2 Turn×1 | success (1 pass) | pass |
| 5835-chamfer-sloped-basin-4-mm-rim-2-2-edges | End×2 Turn×1 | success (1 pass) | pass |
| 783-fillet-pick-far-from-any-edge-refused | - | no blend call replayed | pass |
| 814-chamfer-hex-hole-corners | End×12 | success (1 pass) | pass |
| 851-chamfer-arc-and-top-front-2 | End×2 Turn×1 | success (1 pass) | pass |
| 851-chamfer-arc-top-edge-9-5 | End×2 | success (1 pass) | pass |
| 851-chamfer-knife-edge-tip-2-box-less-cylinder | End×2 | success (1 pass) | pass |
| 851-chamfer-knife-edge-tip-2-sketched | End×2 | success (1 pass) | pass |
| 851-chamfer-knife-edge-tip-5 | End×2 | success (1 pass) | pass |
| 851-fillet-arc-and-upright | End×2 Turn×1 | success (1 pass) | pass |
| 851-fillet-arc-then-top-front | End×2 Join×2 | success (1 pass) | pass |
| 851-fillet-arc-top-edge-9-5 | End×2 | success (1 pass) | pass |
| 851-fillet-arc-top-edge-full-wall-height | End×2 | success (1 pass) | pass |
| 851-fillet-knife-edge-tip-r2 | End×2 | success (1 pass) | pass |
| 851-fillet-three-edges-at-knife-edge-arc-corner | End×3 Ball×1 | success (1 pass) | pass |
| 851-fillet-top-front-and-arc | End×2 Turn×1 | success (1 pass) | pass |
| 851-fillet-top-front-and-front-left-upright | End×2 Turn×1 | success (1 pass) | pass |
| 851-fillet-top-front-and-upright | End×2 Turn×1 | success (1 pass) | pass |
| 851-fillet-top-front-then-front-left-upright | End×2 Join×2 | success (1 pass) | pass |
| 851-fillet-top-front-then-upright | End×2 Join×2 | success (1 pass) | pass |
| 854-fillet-revolved-disc-both-rims-full-round | Join×2 | success (1 pass) | pass |
| 862-fillet-curvature-continuous-box-edge | End×2 | success (1 pass) | pass |
| 896-fillet-smooth-seam-refused | - | no blend call replayed | pass |
| 9-fillet-corner-curvature-with-reduce-fit | - | no blend call replayed | listed |
| 9-fillet-corner-curvature-with-taper-fit | - | no blend call replayed | listed |
| 9-fillet-corner-r5-curvature-default-setback | - | no blend call replayed | pass |
| 9-fillet-corner-setback-1-5-not-past-r2-curvature | - | no blend call replayed | pass |
| 9-fillet-corner-setback-1-5-not-past-r2-rolling | - | no blend call replayed | pass |
| 9-fillet-corner-setback-2-not-past-r2-curvature | - | no blend call replayed | pass |
| 9-fillet-corner-setback-2-not-past-r2-rolling | - | no blend call replayed | pass |
| 926-chamfer-top-edge-ending-on-boss-wall-above-it | End×2 | success (1 pass) | pass |
| 926-fillet-top-edge-ending-on-boss-wall-above-it | End×2 | success (1 pass) | pass |
| chamfer-slot-back-wall-across-a-rounded-groove-0-2-fits | End×6 Join×2 | success (2 passes) | pass |
| chamfer-slot-back-wall-across-a-rounded-groove-0-4 | End×6 Join×2 | success (2 passes) | pass |
| cov-chamfer-concave-edge-through-up-down-joined-squares | End×4 | success (1 pass) | pass |
| cov-chamfer-outer-corner-edge-of-a-shelled-box-with-a-lip | End×2 | success (1 pass) | pass |
| cov-fillet-arc-edge-on-an-oblique-cut-of-a-bar | End×2 | success (1 pass) | pass |
| cov-fillet-exact-coaxial-cones-r2 | Join×1 | success (1 pass) | pass |
| cov-fillet-exact-cone-cylinder-sharing-a-ball-r1 | Join×2 | success (1 pass) | pass |
| cov-fillet-exact-cylinders-touching-at-a-point-r1 | Join×3 End×2 | success (1 pass) | pass |
| cov-fillet-exact-oblique-plane-cone-rim-r1-5 | Join×2 | success (1 pass) | pass |
| cov-fillet-exact-oblique-plane-cylinder-rim-r2 | Join×2 | success (1 pass) | pass |
| cov-fillet-exact-sphere-coaxial-cone-r3 | Join×1 | success (1 pass) | pass |
| cov-fillet-exact-sphere-plane-rim-r2 | Join×2 | success (1 pass) | pass |
| cov-fillet-g2-corner-beside-a-cylinder | - | no blend call replayed | listed |
| cov-fillet-g2-five-edge-apex-corner | - | no blend call replayed | listed |
| cov-fillet-g2-four-edge-apex-corner | - | no blend call replayed | listed |
| cov-fillet-g2-mixed-corner | End×3 SetbackG2×1 | success (1 pass) | pass |
| cov-fillet-g2-oblique-three-edge-corner | - | no blend call replayed | listed |
| cov-fillet-g2-pocket-corner | - | no blend call replayed | listed |
| cov-fillet-pocket-floor-round-corners-r3-chain-r1 | Join×8 | success (1 pass) | pass |
| cov-fillet-section-chamfer-box-edge-3 | End×2 | success (1 pass) | pass |
| cov-fillet-section-chamfer-pipe-on-plate-2 | End×2 | success (1 pass) | pass |
| cov-fillet-section-curvature-boss-root-r2 | Join×1 | success (1 pass) | pass |
| cov-fillet-section-curvature-box-edge-r3 | End×2 | success (1 pass) | pass |
| cov-fillet-section-curvature-pipe-on-plate-r2 | End×2 | success (1 pass) | pass |
| cov-fillet-section-round-box-edge-r3 | xEnd×2 | success (ops front only) | pass |
| cov-fillet-section-round-pipe-on-plate-r2 | End×2 | success (1 pass) | pass |
| cov-fillet-tee-pipe-r8-into-pipe-r20-r3 | Join×1 | success (1 pass) | pass |
| cov-fillet-turn-mitre-boss-base-r1 | Turn×4 | success (1 pass) | pass |
| cov-fillet-turn-mitre-pocket-floor-corners-r1 | Turn×4 | success (1 pass) | pass |
| cov-fillet-turn-swept-boss-base-r1 | Swept×4 | success (1 pass) | pass |
| cov-fillet-turn-swept-pocket-sharp-corner-refused-r1 |  ; Refused×4 | refused (1 pass) | pass |
| cov-fillet-variable-box-edge-1-to-5-on-its-cone | End×2 | success (1 pass) | pass |
| cov-junction-boss-tilt-00-root-round | Join×1 | success (1 pass) | pass |
| cov-junction-boss-tilt-20-root-round | End×2 Join×1 | refused (1 pass) | pass |
| cov-junction-boss-tilt-40-root-round | End×2 | success (1 pass) | pass |
| cov-junction-tee-equal-round | RunsOut×2 Join×2 | success (1 pass) | pass |
| cov-junction-tee-oblique-round | Join×1 | success (1 pass) | pass |
| cov-junction-tee-oblique-unequal-round | Join×1 | success (1 pass) | pass |
| cov-junction-tee-offset-round | Join×3 | success (1 pass) | pass |
| cov-junction-tee-unequal-round | Join×1 | success (1 pass) | pass |
| cov-junction-wedge-030-chamfer | End×2 | success (1 pass) | pass |
| cov-junction-wedge-030-round | xEnd×2 | success (ops front only) | pass |
| cov-junction-wedge-045-chamfer | End×2 | success (1 pass) | pass |
| cov-junction-wedge-045-round | xEnd×2 | success (ops front only) | pass |
| cov-junction-wedge-060-chamfer | End×2 | success (1 pass) | pass |
| cov-junction-wedge-060-round | xEnd×2 | success (ops front only) | pass |
| cov-junction-wedge-075-chamfer | End×2 | success (1 pass) | pass |
| cov-junction-wedge-075-round | xEnd×2 | success (ops front only) | pass |
| cov-junction-wedge-090-chamfer | End×2 | success (1 pass) | pass |
| cov-junction-wedge-090-round | xEnd×2 | success (ops front only) | pass |
| cov-junction-wedge-105-chamfer | End×2 | success (1 pass) | pass |
| cov-junction-wedge-105-round | xEnd×2 | success (ops front only) | pass |
| cov-junction-wedge-120-chamfer | End×2 | success (1 pass) | pass |
| cov-junction-wedge-120-round | xEnd×2 | success (ops front only) | pass |
| cov-junction-wedge-135-chamfer | End×2 | success (1 pass) | pass |
| cov-junction-wedge-135-round | xEnd×2 | success (ops front only) | pass |
| cov-junction-wedge-150-chamfer | End×2 | success (1 pass) | pass |
| cov-junction-wedge-150-round | xEnd×2 | success (ops front only) | pass |
| cov-junction-y-junction-120-round |  | refused (1 pass) | listed |
| every-block-cylinder-crease-block-top-edge-r2-curvature | End×2 | success (1 pass) | pass |
| every-block-cylinder-crease-block-top-edge-r2-no-tangent-chain | End×2 | success (1 pass) | pass |
| every-block-cylinder-crease-concave-arc-r2 | End×2 | success (1 pass) | pass |
| every-block-cylinder-crease-cylinder-top-rim-r2 | Join×1 | success (1 pass) | pass |
| every-block-cylinder-crease-top-edge-after-arc-round-r1 | End×2 Join×1 | success (1 pass) | pass |
| every-block-cylinder-tangent-block-top-edge-r2-no-tangent-chain | End×2 | success (1 pass) | pass |
| every-block-cylinder-tangent-concave-arc-r2-no-tangent-chain | End×2 Join×1 | success (1 pass) | pass |
| every-block-cylinder-tangent-concave-arc-r2-partial-chain | End×2 Join×1 | success (1 pass) | pass |
| every-boss-root-round-r10-as-tall-as-boss | Join×1 | success (1 pass) | pass |
| every-boss-root-round-r2 | Join×1 | success (1 pass) | pass |
| every-boss-top-rim-r2 | Join×1 | success (1 pass) | pass |
| every-box-all-twelve-edges-r2 | xBall×8 | success (ops front only) | pass |
| every-box-corner-three-edges-chamfer-2 | End×3 ChamMitre×1 | success (1 pass) | pass |
| every-box-corner-three-edges-r3-curvature-setback-3-5 | - | no blend call replayed | pass |
| every-box-corner-three-edges-r3-curvature-setback-3-refused | - | no blend call replayed | pass |
| every-box-corner-three-edges-r3-tangent | xEnd×3 xBall×1 | success (ops front only) | pass |
| every-chamfer-box-edge-2 | End×2 | success (1 pass) | pass |
| every-chamfer-box-edge-two-distances-3-1-flipped | End×2 | success (1 pass) | pass |
| every-chamfer-box-edge-two-distances-3-1 | End×2 | success (1 pass) | pass |
| every-chamfer-pocket-rim-4-9-fits | Turn×4 | success (1 pass) | pass |
| every-chamfer-pocket-rim-5-5-past-the-floor | Turn×4 | success (1 pass) | pass |
| every-chamfer-pocket-rim-5-full-depth | Turn×4 | success (1 pass) | pass |
| every-chamfer-pocket-rim-two-distances-2-1 | Turn×4 | success (1 pass) | pass |
| every-fillet-after-fillet-curvature-front-then-side-r3-partial-chain | End×2 | refused (1 pass) | pass |
| every-fillet-after-fillet-curvature-front-then-side-r3 | Join×2 End×2 | refused (1 pass) | pass |
| every-fillet-after-fillet-front-r3-then-vertical-r2 | End×2 Join×2 | success (1 pass) | pass |
| every-fillet-after-fillet-front-then-vertical-r2 | End×2 Join×2 | success (1 pass) | pass |
| every-fillet-after-fillet-vertical-then-front-r2 | End×2 Join×2 | success (1 pass) | pass |
| every-hole-both-rims-chamfer-1 | Join×2 | success (1 pass) | pass |
| every-hole-both-rims-r1 | Join×2 | success (1 pass) | pass |
| every-hole-top-rim-chamfer-10-full-depth | Join×1 | success (1 pass) | pass |
| every-hole-top-rim-chamfer-9-9-fits | Join×1 | success (1 pass) | pass |
| every-hole-top-rim-r1-curvature | Join×1 | success (1 pass) | pass |
| every-hole-top-rim-r1 | Join×1 | success (1 pass) | pass |
| every-hole-top-rim-r10-as-deep-as-plate | Join×1 | success (1 pass) | pass |
| every-hole-top-rim-r12-past-plate-bottom | Join×1 | success (1 pass) | pass |
| every-l-bracket-both-bends-r3-r8-concentric | End×2 | success (1 pass) | pass |
| every-l-bracket-front-inner-outline-after-bend-round-r1 | Join×2 End×2 | success (1 pass) | pass |
| every-l-bracket-inside-bend-r3-curvature | End×2 | success (1 pass) | pass |
| every-l-bracket-inside-bend-r3 | xEnd×2 | success (ops front only) | pass |
| every-l-bracket-inside-bend-r35-uses-both-faces | End×2 | success (1 pass) | pass |
| every-l-bracket-inside-bend-r36-refused | - | no blend call replayed | pass |
| every-l-bracket-outside-bend-r5 | End×2 | success (1 pass) | pass |
| every-pocket-floor-and-rim-edges-r2 | Turn×8 | success (1 pass) | pass |
| every-pocket-floor-edges-r2 | Turn×4 | success (1 pass) | pass |
| every-pocket-rim-edges-r2 | Turn×4 | success (1 pass) | pass |
| every-pocket-rim-edges-r5-as-deep-as-pocket | Turn×4 | success (1 pass) | pass |
| report-drafted-peg-front-and-concave-rounds-together-r2 | End×2 Mixed×1 | success (1 pass) | pass |
| report-drafted-peg-front-round-r2 | End×2 | success (1 pass) | pass |
| report-fillet-bar-top-edge-running-tangent-into-cylinder | End×2 | success (1 pass) | pass |
| report-fillet-concave-arc-and-front-top-edge-together-r5-solver-residues | End×2 Mixed×1 | success (1 pass) | pass |
| report-fillet-concave-arc-and-front-top-edge-together-r7-solver-residues | End×2 Mixed×1 | success (1 pass) | pass |
| report-fillet-concave-arc-and-front-top-edge-together-solver-residues | End×2 Mixed×1 | success (1 pass) | pass |
| report-fillet-concave-arc-bar-top-into-cylinder-solver-residues | End×2 | success (1 pass) | pass |
| report-fillet-concave-arc-bar-top-into-cylinder | End×2 | success (1 pass) | pass |
| report-fillet-corner-after-round-running-into-cylinder-solver-residues | End×2 Join×2 | success (1 pass) | pass |
| report-fillet-corner-after-round-running-into-cylinder | End×2 Join×2 | success (1 pass) | pass |
| report-fillet-curvature-three-edges-at-bar-corner-solver-residues-outside-setback | - | no blend call replayed | pass |
| report-fillet-curvature-three-edges-at-bar-corner-solver-residues | - | no blend call replayed | listed |
| report-fillet-curvature-three-edges-at-bar-corner | - | no blend call replayed | listed |
| report-fillet-edge-tangent-at-one-end-into-cylinder | RunsOut×1 Join×1 End×1 | success (1 pass) | pass |
| report-fillet-g2-corner-of-bar-running-into-cylinder-outside-setback | - | no blend call replayed | pass |
| report-fillet-g2-corner-of-bar-running-into-cylinder | - | no blend call replayed | pass |
| report-peg-concave-chamfer-d2 | End×2 | success (1 pass) | pass |
| report-peg-concave-round-r3-then-front-round-r1-5 | End×1 Join×1 RunsOut×1 | success (1 pass) | pass |
| report-peg-concave-round-then-front-round-r2 | End×1 Join×1 RunsOut×1 | success (1 pass) | pass |
| report-peg-front-and-concave-chamfers-together-d2 | End×2 Mixed×1 | success (1 pass) | listed |
| report-peg-front-and-concave-rounds-together-r2 | End×2 Mixed×1 | success (1 pass) | pass |
| report-peg-front-and-concave-rounds-together-r5 | End×2 Mixed×1 | success (1 pass) | pass |
| report-peg-front-chamfer-d1-then-concave-chamfer-d2 | End×2 | success (1 pass) | pass |
| report-peg-front-chamfer-d2 | End×2 | success (1 pass) | pass |
| report-peg-front-curvature-round-r2 | End×2 | success (1 pass) | pass |
| report-peg-front-round-r1-5-then-concave-round-r3 | End×1 Join×1 RunsOut×1 | success (1 pass) | pass |
| report-peg-front-round-then-concave-round-r2 | End×1 Join×1 RunsOut×1 | success (1 pass) | pass |
| report-peg-step-front-and-concave-rounds-together-r2 | End×2 Mixed×1 | success (1 pass) | pass |
| report-peg-step-front-round-r2 | End×2 | success (1 pass) | pass |
| report-round-branch-tube-under-flange-ledge | Join×3 End×2 | success (1 pass) | pass |
| round-slot-back-wall-across-a-rounded-groove-r0-9 | End×6 Join×2 | success (2 passes) | pass |
