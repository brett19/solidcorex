# Review of research 61 - G2 and conic fillets

An adversarial review of `docs/research/61-g2-conic-fillets.md` ("61" below). Nothing was built
or run. The only computation was one plain-float script that checks a figure 61 quotes (§5). The
only file written is this one.

- **Labels:**
  - READ: read from code, docs or git history.
  - MEASURED: counted or computed here.
  - JUDGED: a judgement.
- **Paths:**
  - Pin paths are relative to the pin's `crates/`.
  - Fork paths are relative to the SMLib fork, read at `origin/main`.
  - Kernel cases are under `solidcore/tests/` in the pin.
- **Verdicts:** agree, agree with changes, or disagree.

---

## 0. Summary

61's main claim holds: G2 (Gⁿ) is the new system's own algorithm, so it should be ported rather
than deferred. Its map of old and new code is mostly accurate. Four things in it are wrong or
incomplete, and they change several decisions.

1. **F1 is wrong about the app.** SolidCraft runs with every `UpstreamFixes` flag on, so the
   exact classes are already the production path. The try-chain runs only under a bare `Kernel`
   default. Dropping it loses nothing the app uses (§1.3).
2. **The pin does not refuse the corners 61 says it refuses.** `classify_setback` returns `None`
   for a long list of cases, and each one stays the old, uncertified NxN patch. These include a
   G2 corner beside a curved face, variable rounds, chord-sized rounds, B-spline faces, edges
   tangent at the vertex, and mixed section kinds. V5 ("a G2 corner is always a Setback") and the
   §4.2 corner table therefore overstate what the new system covers. "Refused until certified"
   would be a real regression on cases the corpus contains (§2).
3. **"No user shape factor" is 61's reading, not the fork's design.** The fork's public new-system
   C API, `smlib_blend_order`, takes a user thumbweight. It was added in step 3. The design doc's
   §5 sentence is about the default curvature kind (§3).
4. **The refusal stance breaks owner rules.**
   - D5 (refuse when the certificate budget runs out) contradicts 04 row 11-26: a valid request
     that cannot meet accuracy succeeds and records `Unproven`.
   - D6 and D8 refuse valid input, which 01 §4.2 forbids.
   - Where refusal stays, 01 §5.1 calls the cause `Unsupported`, not `Refused`.

The conic design is sound geometrically. Two of its truth checks are not checks as written
(§4). The 12-17k line estimate is plausible at its low end and high at its top (§5).

---

## 1. The old/new map, §0.3-§0.5 and F1-F10

### 1.1 §0.3 module classification

Verdict: **agree with changes.** The origins are right (READ, spot-checked). Three of the "keep"
judgements are too coarse:

- **`fillet_corner/` is not wholly old.** `p1.rs` holds machinery that every corner path still
  calls from the session. That includes `calc_corner_vert_geom` (called at
  `blend/fillet_executive/session.rs:1281` and in `blend/fillet_geom/p3.rs`), `drop_end_rib`
  (`session.rs:3732`) and `check_and_fix_degeneracy` (`session.rs:9157`). The End event's end
  patch is also `create_end_patch`, called from `fillet_corner/p3.rs:2098`. These move to the new
  events; they are not deleted. READ.
- **`make_variable_radius_fillet` (`fillet_corner/p2.rs:1219`) is reached from
  `make_analytic_corner_surface`.** Nothing outside the corner module calls it, so it goes with
  the analytic patch. That is consistent with 61, but 61 does not say so. READ.
- **`fillet_exact/` is labelled "new, but opt-in".** It is on in production (§1.3). READ.

### 1.2 §0.4 entry points

Verdict: **agree.** `solidcore-ops/src/blend.rs:883-917` maps the variants as follows. READ.

| Variant | Maps to |
|---|---|
| Round | `prepare_per_edge_constant_radius(…, &[Circular], 1, 1.0)` |
| Curvature | BlendCurve, order 2, `GN_THUMBWEIGHT_EQUAL_AREA` |
| Chord | constant distance with Circular |
| Chamfer | `prepare_chamfers_along` |

One addition: `solidcore-ops/src/session.rs:130` (`set_fixes`) and `:168` (`current_fixes`) take
the fixes from the application. This is the switch that makes F1 moot in production.

### 1.3 The fallbacks F1-F10

| # | Verdict | Note | Label |
|---|---|---|---|
| F1 | **disagree** (with the "probably not") | The code fact is right: `fillet_solver/p4.rs:504-507` picks `c3_exact_by_class` only under `fixes.fillet_exact_round_classes`. But `solidcore/tests/support/solidcraft_fixes.rs:1-13` says SolidCraft's preset is "every `UpstreamFixes` flag on" (`UpstreamFixes::all()`). The app therefore runs the exact classes, and `fillet_patch_keeps_fillet_edges` too. `c3_exact_by_class` (`p4.rs:2275`) has no fall-through to the try-chain: every failure is an error. So the try-chain is dead in production, and D4 costs nothing the app uses | READ |
| F2 | agree | The fork deleted `SmLinear`/`SmCircular` in commit 3aae1c4 ("the chord and the circle are the section generator's orders 0 and 1"). Fork `SmFilletExecutive.cpp:835-846, 1031-1042, 1149-1160` maps LINEAR → `SmGnSectionFSG(0)` and CIRCULAR → `SmGnSectionFSG(SmArcSection…)`. The pin keeps the classes, so this is a real divergence | READ |
| F3 | agree | as F2 | READ |
| F4 | agree | `session.rs:2500-2545`: `Declined` leaves the old corner | READ |
| F5 | agree with changes | Understated. See F11, the biggest fallback in the pin | READ |
| F6 | agree | `fillet_corner/p3.rs` ~1334 (ThreeByTwoMixed); FourByThree when 3 solvers meet 4 edges | READ |
| F7 | agree | Moved constructions, not fallbacks | READ |
| F8 | agree with changes | The `None` is not silent. The executive records `CornerUnsupported` / "Corner case not implemented" (`fillet_executive/p1.rs:1323-1393`), and the fork has `SMLIB_ERR_CORNER_UNSUPPORTED (-1111)` in `capi/include/smlib.h`. So the "no kind" is already an `Unsupported` error. What the new rule removes is the unnamed case, not the error | READ |
| F9 | agree | | READ |
| F10 | agree | | READ |

### 1.4 Fallbacks 61 missed

| # | Fallback | Where | Label |
|---|---|---|---|
| F11 | `classify_setback` returns `None`, and the corner stays the **old uncertified NxN patch**, in each of the cases listed below | `blend/blend_corner/setback.rs:1192-1340`; `session.rs:2610-2640` | READ |
| F12 | The Ball takes only Circular rounds. A declined Ball with nothing asked goes to the old NxN | `blend/blend_corner/ball.rs:574`; `setback.rs` | READ |
| F13 | `MIXED_CORNER_RAILS_CROSS` returns `Unsupported` ahead of ThreeByTwoMixed | `fillet_corner/p3.rs` ~1330 | READ |
| F14 | The pin has two more events, **Chamfer** (Mitre or Facet; 4+ non-coplanar chamfers get bilinear patches; three chamfers of both convexities are refused) and **Roll-over** (refused with `CornerUnsupported` until built). 61's event table omits both | pin `docs/generic-blend.md` §3.2 (771 lines; solidcore's own, newer design page, which 61 does not cite) | READ |
| F15 | The setback-scale bisection assumes monotonicity: "checked on a grid of cases, not proved". A certified fill sits on an unproved search | pin `docs/generic-blend.md` §3.2 | READ |
| F16 | G2 corners build with `fit: exact` only. `reduce` and `taper` fail (two known-failing kernel cases) | `kernel_cases_known_failing/fillets/9-fillet-corner-curvature-with-{reduce,taper}-fit.json` | READ |

The `None` cases of F11:

- A section that is neither Circular nor BlendCurve, such as a chamfer among rounds.
- A solver that is not ConstRadius, which covers Chord and constant-distance rounds, or offset
  radii that can change, which covers variable rounds.
- r0 ≠ r1.
- An edge with no curve.
- Two edges tangent at the vertex.
- A face that is neither a plane nor a natural quadric, such as a B-spline face.
- Between planes, an edge that is not straight.
- **A curved face where the corner is not one-kind Circular.** This includes a G2 corner beside a
  curved face.
- Mixed section kinds with nothing asked.
- Three equal Circular rounds with nothing asked. This is the Ball's case; if the Ball declines,
  the corner stays NxN.
- A curved face with an order other than 1 and nothing asked.
- Mixed convexity with an order other than 2, or beside a curved face.

The pin's own design page says this directly: "a marked corner beside a curved face keeps that
patch too until the curved Setback is the default", and "variable rounds or G1 rounds of mixed
convexity keep the N x N corner's uncertified patch" (pin `docs/generic-blend.md` §3.2).

The pin is refused only when an ease is asked beside a curved face (`session.rs:3175-3260`). 61
generalises that one refusal to every G2 corner beside a curved face. That is wrong: today those
corners are built, with the old patch.

---

## 2. Coverage lost under "new system only"

### 2.1 How often the corpus hits these corners

These counts come from `kernel_cases/fillets/` (326 cases; 293 `blend_dropping` calls) and the
known-failing list. The corner class each case reaches was not traced, since nothing was run.
MEASURED (counts); JUDGED (what they imply).

| Measure | Count |
|---|---|
| Fillet cases | 326 |
| Calls by kind | 192 round, 93 chamfer, 14 `kind: curvature` |
| Files with `corner: curvature` | 17 |
| Files that mention "corner" | 177 |
| Files that mention "setback" | 27 |
| Files that mention three edges | 43 |
| Files that mention four edges | 38 |
| Files with chamfers and rounds together | 3 |
| G2 cases in all (files matching `curvature` or `G2`) | about 31 (about 10%) |
| G2 case files that name a curved face (cylinder, cone, torus, sphere, tube, drum, boss, hole, pipe) | 17 |
| Known-failing fillet cases | 12 |
| Known-failing cases that are G2 corners | 9 |

The 9 known-failing G2 corner cases, by reason:

| Reason | Cases | Case files |
|---|---|---|
| G1 at a seam where G2 was wanted, i.e. a G1 fill | 3 | `cov-fillet-g2-pocket-corner`, `report-fillet-curvature-three-edges-at-bar-corner*` |
| "invalid input: the setback must be more than 3.9 / 3.5 mm" | 2 | four- and five-edge apex corners |
| The open setback is not ported, or not closed, beside a cylinder | 2 | `cov-fillet-g2-oblique-three-edge-corner`, `cov-fillet-g2-corner-beside-a-cylinder` |
| `fit: exact` only | 2 | F16 |

JUDGED, from these counts:

- G2 corners are already the weakest area: 9 of 12 known failures with about 10% of the cases.
  Adding refusals there makes the weakest area worse.
- 17 of the G2 files involve a curved face. Under 61's D8, every one that reaches a corner would
  be refused. Today the pin builds them through the old NxN patch (F11).
- The setback-size cases are reported as "invalid input". Under 01 §5.1 they are valid input the
  method cannot complete, which is `Unsupported`, unless the setback was user-asked and really is
  too small. The cause name should be checked when ported.
- **Old 4x3 and 3x2 corners.**
  - 4x3 needs three blends over four edges.
  - ThreeByTwoMixed needs mixed convexity that neither `classify_mixed` nor `classify_roll_over`
    takes.
  - Both are uncommon by construction, but the corpus has 38 four-edge files.
  - Under the exact classes there is no measurement of which of these reach the old classes.
    That needs one instrumented run (§2.3).
- **Declined chamfer corners (F4).** 93 chamfer calls. A chamfer is declined when an edge is not
  straight, a face is not a plane, or the width varies. Chamfers on hole rims, bosses and tubes
  are common in the corpus (`2903-*-rim-chamfer`, `chamfer-slot-back-wall-across-a-rounded-groove`).
  This is likely the largest single bucket of old-corner use. JUDGED.
- **The try-chain.** Zero app use (F1). It is used only by tests that build a bare `Kernel`.

### 2.2 Is "refused until certified" acceptable?

**No, not as a blanket rule.** JUDGED, against owner rules.

- **01 §4.2 and 04 11-26.** Valid input is never refused. An operation that cannot meet accuracy
  succeeds and records the accuracy it reached. A procedural blend whose trim falls back records
  `Unproven`. Building the corner with an uncertified fill and `Basis::Unproven` is exactly that
  rule. Refusing is not.
- **R3 (truth over pin)** allows results to change when they become equally or more correct. A
  refusal where the pin builds a usable, closed solid is not more correct.
- **Truth-over-pin does allow** refusing where the pin's old patch is actually wrong. For example,
  `report-peg-front-and-concave-chamfers-together-d2` has a Coons patch standing 7.2e-4 out of a
  face. Those cases should be listed individually, not covered by a class-wide refusal.
- **Typed-errors goal.** Where refusal remains, the cause is `BlendError::Unsupported` with the
  corner kind named (01 §5.1). `Refused` is reserved for invalid input.

### 2.3 How the new system should cover each case

| Old case | Proposed cover | Until then |
|---|---|---|
| Try-chain | Nothing; the exact classes are the default | n/a |
| Ball declined (three equal rounds) | Setback with its certified fill | Setback fill with `Unproven` if it does not certify |
| Unequal radii, chord rounds, variable rounds at an NxN corner | Extend `classify_setback`: the Setback geometry depends only on the rails and contact lines, and does not need ConstRadius. Variable rounds take the law's radius at the vertex | the old patch's shape, ported as one `Unproven` fill of the Setback hole. See note 1 |
| B-spline faces, or edges not straight between planes | Setback over general faces (pin issue #2343 covers curved faces) | `Unproven` fill |
| Tangent edges at the vertex | The Join, or the tangent end (fork design doc §3.1) | `Unsupported` |
| G2 beside a curved face | Curved G2 fill (#2343, #2346, #2347) | G1 fill, recorded G1 and reported as a warning (D8) |
| Mixed section kinds, or a chamfer among rounds | Setback filled to the lower order, which 61 §4.2 already says | `Unproven` |
| Declined chamfer corner (F4) | Generalise the chamfer event (#2345); a G0 Setback fill of order-0 sections | `Unproven` bilinear or Coons fill |
| 4x3, 3x2 mixed | Setback (#2348) and the Mixed/Roll-over events | `Unsupported` with the corner kind |
| Roll-over (F14) | build it | `Unsupported` (as the pin does) |

Notes:

1. **The `Unproven` fill.** This is a single "uncertified fill of a Setback hole" used as the
   last step. It is not a ported 4x3/3x2/NxN class hierarchy, so it does not reintroduce the
   split. It is the same shape the owner already allows for a fallback trim (04 11-26).
2. **The gate.** Before the switch, run the 326 fillet cases with a side-channel counter on each
   arm that reaches the old patch, the `None` arms of `classify_setback`, and `Declined`. That
   follows the "measure locally" rule and gives the real per-case frequency that §2.1 could only
   estimate.

---

## 3. Is Gⁿ new-system, and is "no user shape factor" the fork's design?

**Gⁿ is new-system: agree.** READ.

- Fork design doc §6 step 3: "one Gⁿ section generator (chord, circle, curvature)".
- The executive maps every kind to `SmGnSectionFSG` (§1.3 F2).
- BLEND_CURVE becomes `SmGnSectionFSG(lContinuity, dThumbweight)`, with `IsValid` requiring
  "order 2 to 4 and its thumbweight below 1".

**"No user shape factor" is 61's reading: disagree with V12 and D3.** READ.

- The fork design doc §2.2 says "Two parameters shape it: Thumbweight w… Its default is equal
  area". The thumbweight is a parameter with a default.
- The fork's public C API exposes it. `capi/include/smlib.h:686-711` declares
  `smlib_blend_order(brep, order, thumbweight, …)` with "raising it pulls the section towards the
  edge", and `SMLIB_BLEND_THUMBWEIGHT 2.0` as the equal-area default. This came in commits 0bd3f4d
  and 1947a45 (2026-10-31), part of step 3 itself.
- So the §5 line "equal-area thumbweight per station, with no user shape factor" describes the
  default curvature kind (what `Blend::Curvature` gives). It does not describe the generator. The
  free thumbweight in the kernel API is not residue; the fork kept it on purpose.
- The conformance `fillet_g2` asks for a middle radius (61 §1.1, adapter at
  springcraft-ai_brep-conformance-solidcraft `src/main.rs:1264-1285`). That is a second, real
  consumer of a shape factor.

---

## 4. Is the conic design sound?

**Geometry: agree.** JUDGED, with the algebra checked by hand.

- A rational quadratic [A, P, B] with weights [1, w, 1] has its shoulder (t = ½) at
  M + rho·(P − M), where M is the chord's middle and rho = w/(1+w). Equivalently,
  w = rho/(1−rho), as 61 says.
- The apex P lies in the station plane, and the end tangents run along A→P and P→B. So the
  section is G1 to both faces at every station, and it uses the circle's station and rails
  unchanged.
- The circle is w = cos(θ/2). At a square edge that is w = √2/2, rho = √2 − 1 ≈ 0.41421. MEASURED
  (float).

The exact classes extend:

- a straight spine between planes gives a rational quadratic extrusion;
- a revolved pair gives the conic revolved;
- plane/cone pairs also stay rational if the rails are rational.

**Corners: agree with changes.** No conic corner construction exists. `classify_setback` accepts
only Circular and BlendCurve (F11), and the Ball rejects non-circular rounds (F12). So, for a
conic:

- "Setback; G1 fill" in the §4.2 table is new work, not a reuse;
- "Turn: Mitre yes" needs the two-blend construction checked for non-circular sections.

The §4.2 claim that G2 Turn corners are Setbacks has no construction behind it either. The pin's
`blend_corner::classify` ignores the section kind, so two-blend G2 corners are built as Turn or
Join. 61's table follows the fork's design doc §3.2 ("any corner where G2 is selected"), not the
pin's code. READ.

**Truth checks:**

| 61 check | Verdict | Fix |
|---|---|---|
| "rho = √2−1 … quarter circle, closed form (`rat.rs` exact rationals)" | **disagree**: √2 is irrational, so this cannot be done in exact rationals | Use a rational cos(θ/2), e.g. faces at an angle with cos(θ/2) = 3/5, w = 3/5, and check on the circle in exact rationals. Check the square edge in dd |
| "The shoulder lies rho of the way from the chord's middle to the apex" | **disagree**: tautological, since it is the definition of rho | Check that dense samples satisfy the conic's implicit equation in barycentric form, `λ_P² = 4w²·λ_A·λ_B` with (A, P, B) as the reference triangle, and check G1 to the faces |
| "Middle radius at a square edge ≈ 1.389 × offset at equal area, closed form at t = ½" | **agree with changes**: the value is right (**1.38860** computed here; equal-area w found by bisection on the area, radius from finite differences, offset r = 1; MEASURED). It is not a closed form: the equal-area w is the root of a transcendental equation | Call it a reference value, computed independently to 1e-9 in dd, and keep the area law (r²(cot(α/2) − (π−α)/2)) as the primary exact check |
| "A G2 corner is a Setback, and its fill is certified convex" | **disagree** | The Turn and Join cases are not Setbacks (above). Add a check that G2 Turn and Join seams are G2, or report the order they reach |
| The rest | agree | |

---

## 5. Is 12-17k lines not ported plausible?

**Agree with changes.** MEASURED (pin `wc`); JUDGED (the split).

Pin `blend/` line counts:

| Module | Lines |
|---|---|
| `fillet_corner/` | 12,330 (`p1` 4,584, `p2` 2,735, `p3` 3,257, `topology.rs` 1,743) |
| the four old topology files (`concave_4x2` 286, `concave_nxn` 291, `mixed_3x2` 475, `end_corners` 533) | 1,585 |
| `fillet_corner/` + `topology/` (61's 17,911 + 11) | matches |

- **The 6,000-9,000 for "p1, p2, part of p3"** counts `p1.rs` as deletable. But `p1.rs` holds the
  session-wide corner geometry, `drop_end_rib`, the degeneracy fix and the end patch (§1.1). A
  realistic figure is p2's analytic and variable patches plus parts of p3 plus about 1,050
  topology lines: **about 4,000 to 7,000**.
- 61's header says "non-test lines", but the table includes a 2,000-3,000 line test row. Pick
  one.
- The total is plausible at **about 10,000 to 15,000**. That is still a 7 to 10% cut. It has to
  be netted against new work: the Setback extensions of §2.3, the `Unproven` hole fill, the
  generalised chamfer event and the Roll-over.

---

## 6. The 8 decisions

| D | 61's recommendation | My answer |
|---|---|---|
| D1 Correct the record | yes | **Agree.** Also correct 04 11-27's premise ("solidcore has none") as 61 says. Add that the app runs every fix (F1) |
| D2 Add the conic | yes, owner rules on rho | **Agree with changes.** Add `Section::Conic(Rho)` in the definition at step 5. List its corner gaps (Setback, Ball, Mitre on non-circular sections) as work, with `Unsupported` until built. Since the fork already exposes a G2 shape factor, rho does not violate the new system |
| D3 No G2 shape factor | keep the rule, drop the thumbweight | **Disagree with the premise.** The fork's API exposes w (§3). Offer `Smooth { order, fullness: Fullness }` with `enum Fullness { EqualArea, MiddleRadius(Length) }` and `EqualArea` as the default. This drops the raw w and the 2.0 marker (R1) and answers `fillet_g2` exactly. It costs one scalar solve per station |
| D4 Exact classes by default | yes | **Agree, strengthened.** It is already the app's path; only bare-kernel tests change |
| D5 Budget failures refuse | refuse with typed cause | **Disagree.** 04 11-26 already ruled on this: succeed, record `Basis::Unproven` with the measured bound, and warn. Pursue fork #96 to make it rare. Refusing a valid round because a proof did not close is the "refuse valid input" 01 §4.2 forbids |
| D6 Old corners → Setback, refuse otherwise | yes | **Agree with changes.** The set is much larger than 61's (F11-F14). Do not port the old class hierarchy. Do give the Setback one `Unproven` hole fill as its last step, and use `Unsupported` (never `Refused`) where even that is not built. Gate the switch on the instrumented corpus count of §2.3 |
| D7 Chamfer through the chord | yes | **Agree.** The fork did it in 3aae1c4. Check plane/plane identity and one curved-face chamfer |
| D8 G2 beside curved faces refused | refuse | **Disagree.** The pin builds these today, as the old NxN patch. Build a G1 Setback fill, record its continuity (G1) on the face, and return a warning naming the seam order. A downgrade that is recorded and reported is not silent. Refuse only when the caller asked for G2 as a hard requirement (the eased case), as the pin does |

---

## 7. What 61 missed

1. **The app's fix preset (F1).** This reverses the frequency argument for the try-chain.
2. **`classify_setback`'s `None` set (F11)** and the pin's own design page that documents it.
3. **Solidcore's own `docs/generic-blend.md`.** It has the Chamfer and Roll-over events, the
   monotonicity caveat, §9 ("a step lands as the default, with no UpstreamFixes flag… nothing is
   kept as a fallback"), and the open issues #2343, #2346-#2349, #2356, #2361 and #2400 (delete
   the old corner hierarchy). 61 cites only the fork's page.
4. **The fork is itself split.** `SmFillet3x2MixedConvexityCorner`, `SmFillet4x3Corner` and the
   Convex/Concave NxN corners are still reachable on fork `main` (`SmFilletCorner.cpp:1244, 1304,
   1367, 1380`). "New system only" has no reference implementation for those vertices yet.
5. **The public thumbweight in `smlib.h` (§3).**
6. **Owner rules 04 11-26 and 01 §4.2 and §5.1** (D5, D6, D8). 61 §5.2 checks R1-R7 but not
   these.
7. **Corpus evidence.** 61 gives no frequency for any lost case. The known-failing list shows G2
   corners are 9 of 12 fillet failures (§2.1).
8. **G2 corners with `fit` other than `exact` (F16).**
9. **Two-blend G2 corners are Turn and Join in the pin, not Setbacks** (§4). The seams there need
   a continuity check.
10. **Conic corners have no construction** (§4).

---

## 8. Changes I would make to 61

- §0.3 and §0.6: `fillet_corner/p1.rs` is shared corner machinery, not old-only. `fillet_exact`
  is on in production.
- F1: replace "probably not" with the SolidCraft preset fact.
- §0.5: add F11-F16 and correct F5 and F8.
- V5: "a G2 corner of three or more blends, between planes or natural quadrics, of one kind, is a
  Setback. Beside a curved face, or for the F11 cases, the pin builds the old NxN patch. Two-blend
  G2 corners are Turn or Join".
- V12, D3: the thumbweight is part of the fork's public new-system API. Propose `Fullness`.
- §4.2 table: mark the conic's Setback and Mitre cells, and the Smooth Turn cells, as new work.
- §4.3 and D5: follow 04 11-26 (`Unproven` plus a warning).
- §4.4: fix the three checks in §4 above.
- §5.1: replace "Refused until a curved G2 fill exists" with the recorded G1 fill. Add the corpus
  counts.
- §0.7: correct the estimate to about 10,000 to 15,000 lines, and make the table and its header
  agree on whether tests are counted.
- D6 and D8: rewrite as in §6, and gate on the instrumented corpus run.
