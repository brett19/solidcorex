# Review of research 60 (other kernels)

Adversarial review of `docs/research/60-other-kernels.md`. For each item: verdict (agree / agree with
changes / disagree), then evidence marked READ (read in the source), MEASURED (counted) or JUDGED
(argument). Cloned repos are cited by repo name and relative path. "pin" is solidcore at the pin
(`solidcore/src/...` is the kernel crate, `solidcore-ops/src/...` the ops crate). No builds were run,
so nothing here is MEASURED except where a source states its own measurement.

The rules checked against each item are 04's owner rules: idiomatic Rust; determinism across
executors and platforms; truth over pin; one document resolution with recorded accuracy; keep
solidcore's algorithms; honest `&Brep` / `Draft` signatures; no ambient state; std only. The guarded
features are freeform SSI, rolling-ball blends with overflow, healing imports, the cellular merge
with regions and laminas, and tangent and near-coincident handling.

## 0. The one change that matters most: coincidence is `max`, not the sum

**Disagree** with the note under T1 and the "smaller 01 edit" in §5 ("pick 09's sum, with the floor
counted once").

- READ, pin `solidcore/src/geom/tol.rs` (`coincidence_tol_3d`, lines near 505-515): the pin's
  coincidence is `contact_tol.max(zone1).max(zone2)`. The comment records why it left the sum: at
  the sum, "an edge and a curve 1.5e-5 beside it are one place where the tolerance is 1e-5, and a
  groove that deep cut into a block comes out as no groove at all" (pin `docs/booleans.md` §10).
  The sum survives only in `xsect_tol_3d_from_zones`, the gap allowed *at an intersection*.
- READ, `crates/scx-num/src/units.rs` (`coincident`): already `res.max(a).max(b)`, citing
  `coincidence_tol_3d`. `crates/scx-num/src/tol.rs` already has `coincidence_reach_3d` and
  `within_coincidence_tol`, difftested against the pin.
- READ, 01 §4.2: coincidence `max(res, acc_a, acc_b)`; the gap at an intersection `acc_a + acc_b`.
  01 already states both, in their right roles.
- READ, 09 §3: writes coincidence as `resolution + acc₁ + acc₂` and says this is "how today's
  `coincidence_tol_3d` and `xsect_tol_3d_from_zones` combine". That sentence is wrong about the pin.
- READ, `springcraft-ai_springrep/src/tol.rs` (`between`): `(a + b − LINEAR).max(LINEAR)`. This is
  springrep's choice. It was not measured against the pin's groove case.

Recommendation: 01 §4.2 is the statement. Fix 09 §3 to say coincidence is `max(res, acc₁, acc₂)`
plus `rounding_of_copies(size)`, and the sum `acc₁ + acc₂` is the intersection gap allowance only.
Choosing the sum would reverse a recorded correctness lesson (truth over pin cuts the other way
here: the pin's `max` is the more correct one, and the reason is written down). T1 must then be
restated under `max` (below).

## 1. Tolerance and coincidence

**T1. Point merging as connected components: agree with changes.**

- READ, `arris-labs_arris/docs/adr/0022-the-same-within-a-tolerance-is-an-equivalence.md` §1: as the
  doc says (components over `‖p−q‖ ≤ t_p + t_q` or the same operand vertex; numbered by first member
  in creation order; base plus spread; two operand vertices in one component is
  `OpError::Tolerance`; 72,710 booleans logged identical, "no diameter bound is needed"). §2, which
  the doc leaves out: the merged point prefers a member lying on an operand edge, and the pcurve ends
  that move have their edge tolerance raised by the cost of the move.
- JUDGED, changes needed:
  1. The relation `t_p + t_q` is arris's sum rule. Under §0 the relation is "within
     `coincident(res, t_p, t_q)` plus rounding", i.e. `max`. Say so, or justify the sum here as the
     intersection-gap case (two constructed points, each off the exact meeting by its own bound,
     which is the case where 01 already uses the sum). Pick one and write why.
  2. "A component wider than `k × res` is a `Defect`" conflicts with 04 11-26 (ops never refuse valid
     input; succeed and record accuracy) and with what `Defect` means (our bug). A wide component on
     valid input is not our bug. Record the spread as the vertex accuracy (`Basis::Measured`) with
     `Warning::AccuracyPastResolution`; only when the merge would collapse distinct topology (an
     operand edge, a loop) is it `Unsupported` with the chain in the trace.
  3. The component point: an operand vertex in the component wins (its record and `Arc` are kept, no
     geometry moves). Otherwise a member by a total key, as the doc says.
  4. "First member in creation order" must mean item-index order of the candidate list, not discovery
     time, or a parallel candidate search breaks R2.
  5. Edge and pcurve ends at a merged vertex: arris raises the edge's tolerance by the move. In
     solidcorex the vertex ball must cover every edge and pcurve end that now meets it (the
     `EndsAtVertex` bound). State it, since this is exactly what V1 will check.
- The evidence is a logged suite run with no chain observed (READ). That supports "chains are rare",
  not "chains are handled"; hence item 2.

**T2. `within` with ulp slack: disagree that it is new; keep only the lint.**

- READ, `crates/scx-num/src/tol.rs`: `rounding_of_copies(size) = γ₃₂·|size|`,
  `coincidence_reach_3d = coincident + rounding_of_copies`, `within_coincidence_tol(res, d, a, b,
  size)`, `within_contact_tol`, `zone_reach_3d`. These come from the pin, whose `within_contact_tol`
  is grounded on "sixteen failing booleans of the stress corpus read their copies at most 3.3 unit
  roundoffs of size apart" (pin `solidcore/src/geom/tol.rs`).
- So the doc's "no comparison helper" (Now row) is wrong, and the change would add a second slack
  constant beside γ₃₂. springrep's `16ε(1 + ‖at‖∞)` (READ, `springcraft-ai_springrep/src/tol.rs`,
  `within`) is the same magnitude.
- Keep: the clippy or xtask scan that flags raw `d <= res` comparisons outside `scx-num`. That is a
  real addition. Cite the existing helpers as the one path.

**T3 reject: agree.** READ, `springcraft-ai_springrep/src/tol.rs` (`TANGENT = LINEAR / RESOLVED`,
`RESOLVED = 0.1`). It is a global angle tolerance; 04 row 42 rules feature thresholds as operation
parameters.

**T4. A new vertex inside an operand vertex's ball takes that ball: disagree as written.**

- READ, `arris-labs_arris/docs/adr/0076-...` is about arris's `max_tolerance` *operation ceiling*:
  the new vertex may exceed the ceiling. solidcorex has no operation ceiling (01 §4.2; 09 §6), so the
  "past the operation ceiling" half has nothing to attach to.
- JUDGED: in solidcorex a new entity's accuracy is set when it is made (construction bound plus
  input bounds), so it is never a "raise" in 09 §6's sense to begin with. Recording it as
  `Basis::Stated` (from the input) would claim the new vertex was stated by an importer, which it
  was not. That is the laundering the doc's own risk row worries about.
- Replace with: a new entity whose accuracy does not exceed the input accuracies it lies within
  raises no `AccuracyPastResolution` warning (no accuracy was lost); `Basis::Measured` with the
  containment test as the measurement. This keeps the CTC-05 healing case working without a new
  basis meaning.

**T5. Search radius named apart: agree.** READ, 09 §3.1 classifies uses but has no search-radius
type. A `SearchRadius` newtype that cannot be passed as a coincidence length is idiomatic Rust and
cheap. Cite next.BREP as idea only (as the doc does).

**E1. Raise on import; ceiling bounds operations: disagree with "already covered".**

- READ, 09 §6: a raise is legitimate only if imported, enclosed, tube clear, *and* "the ratio stays
  within `MAX_ACCURACY_RATIO`". READ, arris ADR-0076 context: CTC-05 needs 2.05e-2 at a 1e-5
  resolution, ratio 2050, above `MAX_ACCURACY_RATIO` = 1000 (`crates/scx-num/src/tol.rs`,
  `LARGE_SMALL_SIZE_RATIO`).
- So 09 §6 today refuses the raise that heals a real import, against 04 11-26 and against "healing
  imports" as a guarded feature. The doc's caution is aimed at `scx-check`; the conflict is in 09
  §6 itself. Change 09 §6 item 3 to: past the ratio the raise is still recorded, with a warning.
- READ, `arris-labs_arris/docs/adr/0058-...`, `0060-...`: verified as described.

**E2. Export states the body's uncertainty: agree.** READ, `bmander_geomsolver/rust/cad-kernel/README.md`.
Write the resolution as the context uncertainty and per-entity accuracies where AP242 allows; the
"largest recorded accuracy" as the context value would loosen every entity on re-read, so prefer
resolution plus per-entity values, falling back to the largest only where per-entity values cannot
be written (JUDGED).

## 2. Booleans

**B1. Caller-stated facts: agree with changes.** READ, `springcraft-ai_springrep/src/topo/facts.rs`
(`Facts { on_face: Vec<OnFace { side, edge, face }> }`) and `doc/design.md` (checked at ends and
quarter points against `LINEAR`; false is `Error::InvalidInput`); `src/boolean/blend.rs` builds them.
Changes: verification within `coincidence_reach_3d` (the existing helper, §0), not a new comparison.
A false fact from a caller inside solidcorex (blend, pocket) is our bug, so it is a `Defect` there,
and `Refused(FalseFact)` only at the public API where a user states facts (JUDGED). Facts must not
key memos as if they were geometry.

**B2. Dimension-order imprint: agree.** READ, `springcraft-ai_springrep/doc/design.md` imprint table
(VV, EF pierce, EE, VF, FF, place hits, VE split, imprint, dedup). It differs from Jackson '95 in
four places, which the spec should record if this order is copied. The doc's summary order "VV,
VE, EE, EF, FF" in §1.1 does not match springrep's (EF pierce before EE; VE split late); fix the
list. Also READ there, missed by the doc: springrep's vertex tolerances *grow* at grazing meetings.
That is the growth T1 forbids; say which one solidcorex follows (T1's).

**B3. Tangent contact precedence: agree.** READ, springrep design doc (Facts, then exact, then
tracing) and geomsolver README ("refused, never read as none"). Freeform SSI is kept as the last
step.

**B4. N-ary Booleans: agree.** READ, `arris-labs_arris/docs/adr/0050-...`; `brep-conformance/doc/api.md`
(`union_all`, `difference_all`). Citation fix: the B-numbers (B28 and the others) are in
`brep-conformance/doc/contract.md`, not `doc/api.md`. Port two-operand first, as the doc says.

**B5. Split, either side empty: agree.** READ, arris ADR-0051; `brep-conformance/doc/contract.md`
B34. Note it is a pin defect fix (truth over pin), so it needs a truth test, not a pin diff.

**B6. Same-body shortcut: agree with changes.** READ, `springcraft-ai_springrep/src/boolean/same.rs`:
it tests whether two solids are the same *within tolerance* by dimension-order matching, which is
broader than identity A op A. Say which is adopted. The shared-`Arc` fast fact is sound (JUDGED).
READ, arris ADR-0087 as described.

## 3. Validity and errors

**V1. Cheap check on every result: agree with changes.**

- The doc's check "each loop's consecutive edge ends meet" is a pin-ism. In 01 §2 edges share vertex
  ids, so consecutive ends meet by construction. The geometric check is: each curve end lies within
  its vertex's ball, and each pcurve end, evaluated on its surface, lies within the same (READ, 01
  §2.1, §2.3; JUDGED).
- `Unfinished { draft, cause: StructureCause }` has no geometric cause (READ, 01 §2.3); the change
  must add one.
- Every writer must record measured gaps before `finish` (stitch, STEP import, T1's merge), or
  `finish` fails good imports. State it as a writer obligation.
- Missed: pin ops already runs the proved outward test on every solid result. READ, pin
  `solidcore-ops/src/body.rs` (`finish_proved`: manifold and closed tests, then `prove_naming`;
  `Contract::Proved` vs `Closed`). So R-9's "too slow for every call" ignores that the pin does a
  full-strength proof per result. And 01 §1.3 maps the ops "result contract" to `scx-brep`, while the
  outward proof lives in `scx-check` (04 11-27: `is_outward_solid` stays in `scx-check`), which
  `scx-boolean` and `scx-brep` sit below. That is an inconsistency in 01's graph to resolve: the
  result contract belongs in the layer above `scx-check` (the features or ops layer), not in
  `scx-brep`.

**V2. No crossing faces: agree.** READ, `rvdende_cadrs/crates/cadrs_kernel/src/native/check_faces.rs`
exists as cited; springrep `src/check.rs`. In `scx-check`, run in tests on every Boolean result.

**Typed refusal table: agree.** READ, `brep-conformance/doc/api.md` per-operation refusals.

## 4. Determinism and budget

**D1. Total budget metered in index order: agree with changes.**

- READ, `arris-labs_arris/docs/adr/0030-cancellation.md` §3-4 as described, plus two lines the doc
  omits: steps are stable "within a release only", and the budget is "a cap a consumer tunes, not a
  contract".
- Missed, JUDGED, affects both D1 and today's split: memo hits (01 §3.3 `Scratch`) and fill-once
  cells skip work. If skipped work is not counted, the work count depends on cache warmth, and with
  per-worker scratch on scheduling, so the stop point moves. That breaks R2 under either budget
  rule. Rule to add: work counts are cache-independent (a hit charges the recorded cost of the
  fill), or memoizable work is not metered.
- A total makes item k's outcome depend on items before it. State how that meets 01's skip mode
  and memo keys (an item cut by budget must not be memoized as a result).

**fmath / libm, cadrs fingerprint: agree** (already covered). READ, arris ADR-0064;
`rvdende_cadrs/crates/cadrs_kernel/tests/fingerprint.rs` exists as cited.

**D2. Fuzzing: agree.** READ, `rvdende_cadrs/crates/cadrs_kernel/tests/fuzz_readers.rs` exists. Note
proptest is a dev-dependency only; std-only holds for the shipped crates.

## 5. Queries

**Q1. Distance output contract: agree.** READ, arris ADR-0054. Algorithm stays the pin's.

**Q2. Ray hits: agree.** READ, arris ADR-0053.

**Q3. Interference: agree with changes.** READ, arris ADR-0055. In the 01 graph `scx-boolean` (L7)
depends on `scx-bodyq`, `scx-ssi` and `scx-stitch` only, not on `scx-mass` (READ, 01 §1.1). The
volume half needs either a new edge `scx-boolean → scx-mass` (no cycle; `scx-mass` sits on
`scx-brep`) or the boolean returns the common and the caller takes its volume with `scx-mass`. Name
the edge or the split.

**Q4. Certified tight box: agree, mostly redundant.** READ, 01 §1.1 already lists "the tight face and
body boxes" in `scx-bodyq`; 04 11-24 holds the *kept* tight box out (wrong on offset faces, research
32). A certified, unkept read is consistent with that ruling; say so and cite it. READ, arris
ADR-0093.

**Q5. Betti numbers: agree.** READ, `springcraft-ai_springrep/src/measure.rs` (`betti`, from χ
over edges and vertices); `brep-conformance/doc/contract.md` B19.

**Q6. Sliver finder: agree.** READ, `bmander_geomsolver/rust/cad-kernel/src/sliver.rs`,
`springcraft-ai_springrep/src/probe.rs` exist as cited.

**Swept contact: agree.** READ, `springcraft-ai_springrep/src/overlap.rs` (`swept_contact`). Cite
`src/overlap.rs`, not the design doc's module map.

## 6. Transforms

**X1. Mirror: disagree with the premise; agree a mirror is needed.**

- READ, pin `solidcore-ops/src/body.rs` (`Body::transform`): any regular affine matrix; "Reflections
  mirror it, a non-uniform scale or a shear turns its geometry into NURBS, tolerances scale with
  it". READ, pin `solidcore/src/topology/brep/mirror.rs`: `mirror` and `transform_matrix` (SVD into
  scale and rotation, a mirror where a factor is left-handed, `reverse_reflection_edgeuses`,
  `swap_reflection_shells`). So "pin has it: no (rigid only)" in §4 is wrong. 01 §2.4's
  `transform(&Brep, Rigid)` loses a pin capability; under keep-the-algorithms the mirror is the
  pin's, not arris's.
- READ, `arris-labs_arris/docs/adr/0031-mirror.md` §2 table: planes and NURBS keep parameters and
  their normal is opposite `R n` (use orientation toggled); quadrics take `u ↦ 2π − u` and keep the
  same normal, with the loops reversed. The doc's "A mirror reverses every face's sense" holds for
  planes and NURBS only. Ids can be kept either way.
- Change: `transform(&Brep, Affine)` in `scx-brep`, ported from the pin's `transform_matrix`, with
  `Similarity` as the fast path. Uniform scale scales accuracies (as the pin does).

**X2 / decision 2. Affine images: disagree with both options.** JUDGED, from the pin above: the pin
already does affine bodies by sending non-similarity cases to NURBS. Affine images of rational
NURBS are exact through the control points, and every analytic kind has an exact rational NURBS
form, so the image is exact without a `Mapped` kind (option b) and without volume-map images in
`scx-deform` (option a). Only procedural kinds (offset, swept, procedural blends) need their
definition mapped or an approximation with recorded accuracy. Recommendation: port the pin's path
into `scx-brep::transform`; keep closed kinds where the image is that kind (planes, similarity
images of quadrics); NURBS otherwise; no new surface kind. READ, `bmander_geomsolver/rust/cad-kernel/src/affine.rs`
exists as cited.

## 7. Offsets and blends

**O1. Rounded offset: agree.** READ, cadrs and geomsolver READMEs. A second join mode, never a
fallback, built on B1 and B4. Its cost should be measured before it goes into the plan's order
(it is O(faces + edges + vertices) Booleans).

**cadrs fillet fallback reject: agree.** READ, `rvdende_cadrs/crates/cadrs_kernel/README.md`. The
rolling ball with overflow is guarded.

**Decision 3, G2 and conic fillets: agree, leave out.** READ, pin `solidcore/src/blend`: no conic or
G2 section. JUDGED: a conic fillet is a section shape over the same rolling-rail frame, so record it
as a later variant of the pin's blend rather than a new algorithm. Completeness rows marked "to
confirm": pin has a chamfer by distance and angle (`solidcore/src/blend/blend_refusal.rs`,
`ChamferAngleTooLarge`, READ), setback-style vertex blends (`blend_corner.rs`, READ), and full rounds
as two rounds whose rails meet (`solidcore/src/blend/fillet_geom/p1.rs` test
`a_full_round_whose_rails_meet_along_part_of_one_of_them`, READ). Update those rows.

## 8. Provenance

**P1. Guaranteed split order: agree.** READ, arris ADR-0009. The key reads input ids only, which also
makes `carry` order-independent.

## 9. Already covered (§3.9): agree, two citation fixes

- cadrs naming is `rvdende_cadrs/crates/cadrs_kernel/src/naming.rs`, not `src/native/naming.rs` (READ).
- The edge-samples watertight test is a good addition; keep it.

## 10. Rejects R-1 to R-9: agree, with two notes

- R-2 snap: READ, `springcraft-ai_springrep/src/boolean/snap.rs` has `SNAP` 1e-6 and `SNAP_GRAZING`
  1e-4. The doc's "up to 1e-6" understates it by 100. The reject stands, more strongly.
- R-9: the reason should be the graph (operation crates sit below `scx-check`), not speed: the pin
  proves outwardness on every ops result (§3 V1 above). The reject stands as "operations do not call
  the full check"; where the result contract is proved is the V1 graph question.
- R-6 polytope: READ, `rvdende_cadrs/crates/cadrs_kernel/src/native/polytope.rs` exists. Agree.
- R-7: READ, `springcraft-ai_springrep/src/boolean/near.rs`. Agree.

## 11. Completeness list (§4): agree with changes

- Mirror and similarity: pin has it (X1). Affine: pin has it, via NURBS (X2). Crate `scx-brep`.
- Section area: `scx-bodyq` (L5) cannot use `scx-boolean`'s (L7) section. Area is `scx-mass` on the
  section sheet the Boolean returns.
- Project a curve onto a body: 01 §1.1 already puts it in `scx-bodyq` (with the 04 11-27
  `scx-bodyq → scx-ssi` edge). The row's `scx-boolean` / `scx-surface` placement is wrong.
- Interference: needs the `scx-mass` edge or split (Q3).
- Chamfer by distance and angle, full round, setback: see decision 3.
- Unify same-surface faces: the pin merges faces inside the Boolean and stitch (READ, pin
  `solidcore/src/boolean/merge/p3.rs`, `solidcore/src/heal/stitch/p1.rs`); a public `scx-repair`
  entry should port that code, not springrep's or cadrs's `unify.rs`.

## 12. Decisions

1. **Budget (D1):** adopt the total, metered per item, summed in index order, *plus* the rule that
   work counts do not depend on caches, and a stated rule for skip mode and memo keys. Without the
   cache rule neither budget form is deterministic.
2. **Affine (X2):** neither (a) nor (b). Port the pin's `transform_matrix` into
   `scx-brep::transform(&Brep, Affine)`: closed kinds where the image is that kind, exact rational
   NURBS otherwise, recorded accuracy for procedural kinds. No `Mapped` kind.
3. **G2 and conic fillets:** leave out (agree). Fix the "to confirm" rows from the pin.
4. **Cheap checks (V1):** in `finish`, checking curve and pcurve ends against vertex balls (not loop
   ends), with a geometric `Unfinished` cause and every writer recording measured gaps first.
   Separately, move the ops result contract (outward proof) above `scx-check` in 01 §1.3.
5. **Two operand vertices in one component (T1):** disagree with the reasoning. A component can hold
   two operand vertices through a chain of candidates, with the two vertices neither within
   coincidence of each other nor joined by an edge. And `LARGE_SMALL_SIZE_RATIO` is 1000 × res (the
   small-topology zone, `crates/scx-num/src/tol.rs`), not the coincidence distance, so an edge
   shorter than coincidence is not what it tests; the claim "already fails it" is false. Answer:
   - direct case (two operand vertices within coincidence of each other): the input violates vertex
     clearance; `Refused` naming both vertices, and `scx-check` reports the same condition so it is
     checkable before the call;
   - chain case: anchor each candidate to its nearest operand vertex (ties by id), splitting the
     component; if that is impossible, `Unsupported` with the chain, never `Refused`.

## 13. Summary of changes I would make

1. §0: coincidence stays `max` (01 §4.2); fix 09 §3's text; the sum is the intersection gap only.
2. T1: relation under `max` (or a stated reason for the sum); no `Defect` for wide components;
   operand vertex as the point; index-order numbering; vertex balls cover moved edge and pcurve ends.
3. T2: drop as new; cite `scx-num` `within_coincidence_tol` / `coincidence_reach_3d`; keep the lint.
4. T4: replace with "no warning when no accuracy is lost", `Basis::Measured`.
5. E1: fix 09 §6 item 3 (ratio past `MAX_ACCURACY_RATIO` warns, does not refuse).
6. E2: resolution as context uncertainty, per-entity accuracies.
7. B1: verification through existing helpers; internal false fact is `Defect`.
8. B2: correct the order list to springrep's; note its grazing growth is not followed.
9. B6: say whether A op A means identity or within-tolerance sameness.
10. V1: vertex-ball checks, geometric cause, writer obligation; move the result contract above
    `scx-check`.
11. D1: cache-independent work counts; skip mode and memo keys under a total.
12. Q3: name the `scx-boolean → scx-mass` edge or split the query.
13. Q4: cite 01 §1.1 and 04 11-24.
14. X1 / X2: pin has mirror and affine; port `transform_matrix`; per-kind sense rule from ADR-0031.
15. §4 rows: mirror, affine, section area, project, interference, blend "to confirm" rows, unify.
16. Citations: B-numbers in `brep-conformance/doc/contract.md`; cadrs `src/naming.rs`; springrep
    `src/overlap.rs`; `SNAP_GRAZING` 1e-4 in R-2.
17. Decision 5: the reasoning above, replacing the `LARGE_SMALL_SIZE_RATIO` argument.
