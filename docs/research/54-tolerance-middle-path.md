# 54 - The middle-path tolerance model: is it feasible?

Read-only research. Nothing was built or run for this report. Every MEASURED figure is taken from an earlier report that measured it (30, 33, 34, 35, 41, 46), and the report number is given beside it. The grep counts were made with a script that leaves out `#[cfg(test)]` blocks and test files. That script and its outputs are kept outside this repository.

- Labels:
  - READ means read from the code at the solidcore pin (03e16c9ee7), or from solidcorex docs.
  - MEASURED means counted on real bodies, in the report cited.
  - JUDGED means reasoned from the above.
- Paths: `k/` is `solidcore/crates/solidcore/src/`. Other crates are named by crate, e.g. `solidcore-step/src/`.

The model under test (decisions 11-26):

1. One session-wide resolution in `Settings`.
2. A measured accuracy per edge and per vertex, recorded only where it is worse than the resolution. Every reader uses the recorded value.
3. No per-body tolerance and no per-body angle tolerance. The angle tolerance lives in `Settings`.
4. A trim that cannot be fitted within the default accuracy is not refused. It is fitted as well as possible, and the accuracy reached is recorded.
5. An imported coarse body is coarse only on its coarse entities. New geometry gets its accuracy from its own construction.
6. Combining two bodies needs no combined body accuracy.

## Verdict

**Feasible with these changes.** Nothing in solidcore needs a per-body tolerance that an entity accuracy plus the session resolution cannot supply.

**Changes needed.** In order of weight:

1. **Accuracy on approximated surfaces too, not only on edges and vertices.**
   - Exact surfaces need nothing: a plane is a plane.
   - A surface that is a fit, such as a procedural blend's net, an offset written as a NURBS, or a solved cache read from a file, is off its definition. That error is not an edge's to carry.
   - solidcore already holds such a bound on the surface (`ProceduralBlend` bound, the CADIR `surface_fit`).
   - So "edges and vertices only" becomes "edges, vertices, and the surfaces that are approximations". Store it as a certificate on the surface, never as a "face tolerance" read by every face test.
2. **A face accuracy is derived, not stored.** A face's accuracy = the maximum of:
   - its surface's certificate,
   - its edges' accuracies,
   - its vertices' accuracies.

   That replaces the 245 face-tolerance reads with one function. solidcore's own record already works this way: it stores no face tolerance, and convert derives the face tolerance from the edges.
3. **STEP and CADIR import must measure per entity.** Today they give the file's stated uncertainty to the body, to every edge and to every vertex. That is the one place where a whole body becomes coarse, and it must change to meet point 5. The reader already measures the real gaps (`cover_edge`, `raise_vertex`, `hold_deviation`). The change is to start entities at the resolution and keep only what is measured.
4. **The fallback fit must record a *certified* bound, not the sampled gap.**
   - The kernel's 20-sample gap understates the true gap on 293 trims, by up to 28× (MEASURED, 34).
   - A recorded accuracy that is too small is the one way this model can give a wrong answer.
   - Where no certified bound exists yet (offset and procedural-blend surfaces, 24 of the 40 refusals), record a stated, conservative bound and mark it as unproven.
5. **The angle tolerance cannot be a single `Settings` value for every use.** Coincidence and coplanarity tests must take their angle from accuracy ÷ size (solidcore already does this for planes). The `Settings` angle remains for feature thresholds (smoothness, rollover, G1). The 20°, 25° and 3° literals become named algorithm constants.
6. **The resolution is fixed for the session, or for the document, and recorded with it.** It is not recomputed per operation from operand sizes. One value is enough for parts from 0.5 to 500 mm. Larger models need a larger value, chosen once.
7. **Size-ratio validity checks are made relative to the resolution,** not to a body tolerance (`LARGE_SMALL_SIZE_RATIO × body tol`, 15 sites).

**Most important evidence.**

1. **solidcore's coincidence rule is already "resolution plus entity accuracy".**
   - READ: `coincidence_tol_3d = max(contact_tol_3d, zone1, zone2)` (`k/geom/tol.rs:519`).
   - READ: `contact_tol_3d` is the kernel's zone, one value whatever the entities have grown to (`k/geom/tol.rs:450`, WR-787).
   - The model needs that rule with the body term removed, not a new rule.
2. **solidcore's stored body has no face tolerance.**
   - READ: The record's `Face` has no tolerance field (`solidcore-record/src/brep.rs:94-115`). `Edge` and `Vertex` do (:158, :185).
   - READ: On load, a face's tolerance is rebuilt as the least of its loop's edge and vertex tolerances (`solidcore-convert/src/brep.rs:728-762`).
   - So the face value that 245 places read is already derivable from edges and vertices.
3. **Building the 40 refused trims "as well as possible" is safe downstream, and it does not creep.**
   - MEASURED (35): Main builds every one of them, raising edges by a median of 1.99× and at most 3.46× over L.
   - MEASURED (35): Building them adds no `check_brep` failures.
   - MEASURED (35): Results stay within 2.2e-6 relative area and 1.8e-9 volume of design T, where T accepts.
   - MEASURED (30): Over 20-step chains, tolerances settle at a plateau and do not grow without bound.
   - MEASURED (30): The growth that does happen comes from two writers (`set_trim` up to 2.4×, `face_trims` up to ×120). Both are rules this model replaces.

## 1. Booleans and intersections

### 1.1 What solidcore uses today (READ)

There are three kinds of value. All three are in `k/geom/tol.rs`:

- **The kernel zone.**
  - `ZONE_TOL_3D = 1e-5` (:209).
  - `contact_tol_3d(k)` (:450) returns the kernel's zone, a single value. The comment there gives the reason: a test against grown entity tolerances fused features that were really apart (WR-787, #15429, #14599).
  - `within_contact_tol` (:483) adds γ₃₂·size, for the rounding of copies.
- **Entity zones.** Edge, vertex and face `tolerance()`.
  - `xsect_tol_3d_from_zones` (:434) is their sum.
  - `coincidence_tol_3d` (:519) = max(contact, zone1, zone2). A NaN zone stays NaN.
- **The body tolerance.** `Brep.tolerance`, which defaults to `ZONE_TOL_3D`. It is read in:
  - `brep_zone_tol_3d(` (27 lines in 11 files), `brep_xsect*` (4), `brep_approx*` (2), `brep_calc_*` (8), `brep_size_zone_tol_3d` (8 lines in 6 files);
  - 156 non-test `.tolerance()` calls on a body, in 62 files (some overlap with the above);
  - the Boolean result: the larger operand tolerance is taken (p7.rs:2348, 2661, 2809, research 31);
  - `matched_tolerances` (`k/boolean/operand_tolerances.rs:57-85`): this raises the finer operand to the coarser one before the Boolean;
  - `LARGE_SMALL_SIZE_RATIO * b.tolerance()`: the wide-vertex and validity checks (15 lines in 8 files);
  - about 47 fallbacks of the form `zone_tol_3d(k, NormalSize)` with no owner (research 31);
  - 4 temporary kernels made only to carry an edge tolerance into a routine (edgeuse/p3.rs:1215, fillet_executive/p1.rs:1510, fillet_solver/p1.rs:3701, p2.rs:3252; research 31).

### 1.2 What replaces each use (JUDGED)

| solidcore use | Count | Replacement in the middle path |
|---|---|---|
| `contact_tol_3d` (kernel zone) | 13 lines, 8 files | `settings.resolution`. Same meaning, same single value. |
| `coincidence_tol_3d(a, b)` | 14 lines, 7 files | `max(res, acc(a), acc(b))`, unchanged except that `acc` is the entity's recorded accuracy or `res`. |
| `xsect_tol_3d_from_zones` | sum | `acc(a) + acc(b)`, with each at least `res`. Unchanged. |
| Point-on-face, curve-on-face (`topology_solver/p1.rs:1601-1629`, `p2.rs:485`, `curve_class/p1.rs:3091-3120`, `p6.rs:1035-1067`, `topology_intersector/p1.rs:3040-3193`, `merge/p4.rs:857`, `point_class.rs:2081, 2112`, `query/brep.rs:243`) | 245 face reads, 75 files | `face_accuracy(f)` = max(surface certificate, edge and vertex accuracies of f). See §3. |
| Boolean result = larger body tolerance | 3 | Nothing. Each entity keeps its own accuracy. New intersection edges get theirs from construction (§4). |
| `matched_tolerances` | 1 | Removed. Raising one operand to the other's tolerance is the "whole body coarse" behaviour the model rejects. |
| `brep_zone_tol_3d`, `brep_xsect*`, `brep_approx*` | 33 | `res` where the code means "the session floor". The accuracy of the entities involved where it means "how far apart these things may be". Each site must be read once to tell which. |
| `brep_size_zone_tol_3d` | 8 lines, 6 files | `res`, fixed per session (§7). |
| `LARGE_SMALL_SIZE_RATIO * body tol` | 15 lines, 8 files | `LARGE_SMALL_SIZE_RATIO * res`. This is a model-size sanity check, not an accuracy. |
| `ZONE_TOL_3D / 10` guards | a few | Named fractions of `res`. |
| Ownerless `zone_tol_3d(k, NormalSize)` | about 47 | `res`. |
| Temporary kernels carrying an edge tolerance | 4 | An explicit accuracy argument. |
| Cache keys that include `k.tolerances` | | The `Settings` that matter, fixed per session, so they key nothing per operation. |

### 1.3 Can each test be answered? (JUDGED)

- **Intersection.** Two faces meet within `acc(fa) + acc(fb)`. Both are face accuracies, derived from the faces' own surfaces and boundaries, so no body term is needed.
  - One caveat: solidcore's box-disjoint test (`topology_intersector/p1.rs:3040-3193`) grows the face's box by the face tolerance.
  - With a derived face accuracy, a face with one coarse imported edge gets a bigger box everywhere.
  - That is conservative, never wrong, but it costs work. The box can instead be grown by the surface certificate, plus the edge accuracies near the boundary.
- **Classification.** A point is on a face when its distance is ≤ `res + acc`. Ray and point classification read `face_tol` only through the point-on-face test, so the same replacement holds.
- **Coincidence.** `max(res, acc1, acc2)`, as now. The WR-787 lesson stands: the floor is the resolution, never a grown body value.
- **Gaps.**
  - The edge-to-face gap G is tested against L = `acc(edge) + acc(face)`. The vertex-to-edge gap is tested against `acc(vertex)`.
  - These are the budgets research 34/35 already use, and none of them reads a body tolerance.
- **The risk.**
  - Removing `matched_tolerances` means a fine body and a coarse body are no longer made equal before a Boolean.
  - Where a coarse imported face meets a fine new one, the intersection works at `acc(coarse) + acc(fine)`, which is correct.
  - Where two fine faces of the coarse body meet, they now work at `res`, not at the coarse body's value. This is the intended gain, and it is also what research 30 shows: the "whole body coarse" lift on vase1 came from that very raise.

## 2. Imports and healing

### 2.1 What the importers do today (READ)

- **STEP** (`solidcore-step/src/topology.rs`):
  - The context uncertainty starts the body and every edge and vertex (:24-31). It is `DEFAULT_TOLERANCE = 1e-3` when the file states none, clamped to `MIN_TOLERANCE` 1e-7 and `MAX_FIT_TOLERANCE` 1.0.
  - Measured gaps can only raise that value: `raise_vertex` (`assembly.rs:514`) and `cover_edge` (`assembly.rs:625-645`).
  - A gap is covered even below the file's uncertainty (`cover_edge`).
  - Pcurves have their own construction tolerance, `pcurve_tolerance() = min(tol, DEFAULT_TOLERANCE)` (`assembly.rs:2215`).
- **The feature-tolerance re-read** (`topology.rs:377-460`):
  - When the shortest edge is below `FEATURE_MARGIN = 0.25` of the stated value, the body is read again at a finer value. The example in the code is a file stating 0.01 mm beside edges of 0.2-10 µm.
  - If the finer read does not hold, the reader falls back to the stated value and adds a note.
- **Producer-stated fits** (`assembly.rs:650-680`, `hold_deviation`): a fit deviation the producer states goes to the edges, pcurves and vertices.
  - The CADIR reader also uses it for a procedural surface's solved cache (`solidcore-cadir/src/lib.rs:950-989`): "a fit within the tolerance its construction states: the face's edges hold it".
- **Surface repairs** in the STEP reader: closing control lines (:233), collapsed sides made exact (:239), slivers narrower than the tolerance (:710, :1249).
- **Older paths.**
  - The convert path keeps the data zone verbatim with `force_same_tol`, and each entity gets max(own, body) (research 31).
  - The app array import uses zone = TOLERANCE/2.
  - The export writes 2·max(own, body, faces) (`k/topology/brep/record.rs:463-493`).
  - The exporter writes a global uncertainty of 1e-6 (`step-export/src/lib.rs:1335`).

### 2.2 Does any case force a whole body coarse? (JUDGED)

**The code does, but the data does not.** Today the STEP reader makes every entity as coarse as the stated value, and that is a policy choice. Nothing measured requires it:

- The measured gaps are per entity already. The reader computes them in order to raise.
- The stated uncertainty describes the file's producer, not each entity. Research 34 (MEASURED) shows imported trims spread over a wide range: G/L has p99 0.92 and max 1.01. A few entities are near the stated value, and most are well below it.
- #18848 (cited at `topology.rs:393`) is the strongest real case. It is a single Inventor face whose tolerances mix 1.6e-6 and 2.6e-4 mm. Treating it all at 2.6e-4 gave coincident edges in the next Boolean.
  - That is evidence *for* per-entity accuracy, not against it. The fault came from a coarse value applied to fine entities.

**One case needs care: entities the reader cannot measure.**

- An edge with no pcurve to compare against, or a vertex shared by curves the reader did not intersect, has no measured gap.
- For those, the stated uncertainty is the only evidence, and it should be recorded on those entities alone.
- This keeps point 5 while staying honest. Coarse is recorded where nothing better is known, and only there.

**Healing.** Research 30 (MEASURED) shows that heal lowers vase1's tolerances by up to ×651 at import. That is the per-entity measured value replacing the stated one. Healing is the middle path's import step already, only applied late.

### 2.3 What changes in the importers (JUDGED)

1. Start each edge and vertex at `res`, not at the stated value.
2. Record the measured gap where it is worse than `res`. `cover_edge`, `raise_vertex` and `hold_deviation` do this already.
3. Record the stated value only on entities with no measurement (see above), and mark it as stated rather than measured.
4. The feature re-read becomes unnecessary. No entity is ever coarser than its evidence, so short edges never meet a coarse value. Its fallback note stays as the report for (3).
5. A procedural cache's fit (`hold_deviation(bound, fit)` in CADIR) goes on the surface as its certificate, not onto the face's edges.

## 3. Face and surface accuracy

### 3.1 Evidence (READ)

- **The stored body has none.**
  - The record's `Face` (`solidcore-record/src/brep.rs:94-115`) has no tolerance field, while `Edge.tolerance` (:158) and `Vertex.tolerance` (:185) exist.
  - On load, `own_tolerances` (`solidcore-convert/src/brep.rs:728-762`) sets each face to the least of its loop's edges' and vertices' tolerances, capped by the body zone.
  - Every operation starts from the record (research 41), so a stored face tolerance never reaches an operation.
- **The older native file format does store one.** The `.smb` face line includes `Tol` (the fixtures read show `1e-5` on every face). It is read into the live face (`face/p3.rs:427` `set_tolerance` cascades it to the edges).
- **It is read widely and in decisions.** There are 245 non-test face-tolerance reads in 75 files. The deciding ones:
  - the point-on-face tolerance plus `face_tol` (`topology_solver/p1.rs:1601-1629`, `p2.rs:485`);
  - `max_dist_to_surf > face_zone/2` (`curve_class/p6.rs:1035-1067`);
  - `PointClassification::new(face_zone_tol_3d)` and `max2(face_tol, approx)` (`topology_intersector/p1.rs:3040-3193`, :5538);
  - the max of two faces in the merge (`boolean/merge/p4.rs:857`);
  - face boxes grown by the face tolerance (`query/brep.rs:243`).
- **Writers.** 22 lines in 12 files. They are:
  - the operand match (`operand_tolerances.rs:91`);
  - `update_object_tolerance` (`k/geom/tol.rs` about :1860-2000; its 2.001×gap raise also raises faces);
  - heal (`heal_data/p2.rs:2011`);
  - scale (`brep/p5.rs:4120`);
  - the constructors (`brep_constructor.rs:621`, `vertex.rs:1340`, sweep `access.rs:371`);
  - p2/p7/p9 body edits.

  None of them records a surface's own error. Each copies or raises a value taken from edges, the body, or a gap.
- **What surfaces carry.**
  - A procedural blend carries its own certified bound (`k/surface/procedural_blend/convert.rs:861-905`, `d.bound`, and the net scope "is its net where its certified bound is at most that tolerance").
  - A CADIR procedural cache carries `surface_fit`.
  - These are surface errors, held on the surface or pushed onto edges.
- **Approximated faces drift.**
  - The export fits approximated faces to half the body's largest zone (`convert.rs:735, :832`, research 41).
  - After a shell, one OffsetSurface face moved 2.10e-4, which is 21× its face tolerance (MEASURED, 41).
  - That error lives in the surface, away from any edge.

### 3.2 Conclusion (JUDGED)

- **Faces need no stored accuracy.** Their value is a function of their surface and their boundary, and solidcore's own record already treats it that way.
- **Approximated surfaces need a recorded accuracy.** An exact surface's error is zero. A fitted surface's error is its fit bound, and nothing on its edges can bound the interior of the face. Without it, the 21× drift in research 41 is invisible to every reader. Store it as the surface's certificate:
  - a `Cert` with claim "within ε of definition", the bound, the method and the reads (doc 09);
  - zero, and so absent, for exact surfaces.
- **The derived face accuracy** is the maximum of the surface certificate and the accuracies of the face's edges and vertices.
  - A test away from the boundary (box growth, interior classification) should read only the surface certificate.
  - A test at the boundary reads the edge or vertex involved.
  - Splitting the two avoids the cost of a whole face growing to one coarse edge's accuracy, and it is what solidcore's own `max2(face_tol, approx)` approximates.

## 4. Propagation

### 4.1 The rule (JUDGED)

1. **A new curve or point** made by an operation takes `acc = max(res, construction bound + the accuracies of the surfaces and curves it was computed from)`.
   - An intersection edge of faces A and B: its certified fit bound plus `surface_cert(A) + surface_cert(B)`.
   - It does not take the accuracy of A's coarse boundary edges. It meets them only at vertices.
2. **A vertex** takes the maximum of `res`, its own construction bound, and the gaps to each edge meeting it, as measured with certified bounds.
3. **A surviving entity** (a part of A's face not touched by the Boolean) keeps its accuracy unchanged. A split edge's two halves each keep the parent's accuracy, which may later be lowered by a certified re-measure, never raised by a rule.
4. **Raises come only from measurement**, never from copying a neighbour's or a body's value.

The rule depends only on the entity and its inputs, never on the order the entities are processed in. It is a max of certified bounds, so it is deterministic and simple. Design T is the evidence that per-trim fitting gives the same result in any order: it is order-independent (MEASURED, 35).

### 4.2 Creep (MEASURED, 30; JUDGED from it)

Research 30 ran six chains. Their end states:

| Chain | End state | Cause |
|---|---|---|
| (a) box + 20 Booleans | stays at 1e-5 | none |
| (b) with fillets | plateaus at 4e-5 | one stitch at ×4 |
| (c) blade_hub_1 | max flat at 4.619e-4 | imported value kept |
| (c) vase1 | doubles once at the first union (1.958e-3 → 3.901e-3), then flat | `face_trims` (up to ×120) and `set_trim` (up to ×2.4) |
| (d) scale ×2 / ×0.5 | one-time lift to the size zone (×10 on vase1's faces) | `brep_size_zone_tol_3d` |
| (e) round trips, (f) checks | exact fixed points | none |

There were no writes from `fit1.01`, `after_boolean`, `validate_update`, `operand_match`, or from heal after import.

- No chain creeps without bound.
- The growth that does happen comes from three rules:
  - copying a neighbour or face value into a new trim (`face_trims`, `set_trim`);
  - raising a whole body to a size zone on scale;
  - `matched_tolerances`.
- The middle path removes all three. Under rule 4.1 a value can grow only by a measured bound, and each operation's new entities are bounded by their own construction.
- So the plateau should be lower than solidcore's, not higher. That is JUDGED. Measuring it means running research 30's chains on a solidcorex prototype, which does not exist yet.

**One real growth path remains.** An entity that is re-fitted again and again (fillet after fillet across the same edge) adds its fit bound each time, if the inputs' accuracies are added. Guard against it two ways:

- fit from the exact inputs where they exist;
- add a surface certificate only when the input is itself an approximation.

## 5. Angle tolerance

### 5.1 Every use (READ)

- **The kernel value.** `ANG_TOL_DEG = 0.5` (`k/geom/tol.rs:213`). It is held in `Tolerances` and never set per body; no body or entity angle exists.
- **Direct `k.tolerances.ang_tol_deg` reads (11):**
  - `blend_corner/rollover.rs:202, 355, 402, 427, 458, 537, 598, 664`;
  - `blend/feature_executive.rs:1656`;
  - `fillet_executive/session/two_offsets.rs:467`;
  - `topology/brep/seam_turn.rs:111` (`test_g1_continuity`).

  These are all feature thresholds: is this corner a rollover, is this seam smooth.
- **Parameters with their own defaults:**
  - `fillet_executive/p2.rs:752-754` (20° when ≤ 0);
  - `topology_intersector/p2.rs:1779` (20°);
  - `tess.rs:197/224` (25°/20°, tessellation);
  - `vertex/corner.rs:228` (`unwrap_or(ANG_TOL_DEG)`);
  - `surface_cache/p2.rs` (`opts.ang_tol_rad`).
- **Wider counts.** `ang_tol_deg` appears in 96 non-test lines in 26 files. `ANG_TOL_*` / `ang_tol_rad()` appears in 73 lines in 37 files. `curve_class` uses 6×ang = 3° (doc 09 §1.1).
- **A derived angle.** `surface/plane/p1.rs:1995-2001` computes the coplanarity angle as `rad_to_deg(tol_3d / size)`, an angle made from a length accuracy over the face's size.

### 5.2 Is the `Settings` value enough? (JUDGED)

- **For feature thresholds, yes.** No use reads a body angle, and none would gain from one. The 11 direct reads and the parameter defaults are design decisions about shape, not accuracies. Per-body angle tolerance is not needed.
- **For coincidence and parallelism, no.** A fixed angle is wrong at both ends of the size range:
  - 0.5° over a 500 mm face is 4.4 mm of deviation;
  - 0.5° over a 0.5 mm face is 4 µm.

  The plane code already derives the angle as accuracy/size, and every coincidence-type angle test should do the same with the entities' recorded accuracies. That keeps the angle tied to the model: a coarse edge permits a wider angle exactly where it is coarse.
- **The 20°, 25° and 3° literals are algorithm constants** (search cones, tessellation). Name them as constants of the algorithm, not as tolerances.

## 6. Fallback fitting

### 6.1 The 40 refusals (MEASURED, 35, 34)

Design T refuses 40 distinct trims that main builds:

| Class | Count | Example | What a best fit reaches |
|---|---|---|---|
| FLOOR | 1 | cliff_1 eu316 | G = 4.593e-4 against L = 4.543e-4; the input gap is itself past the budget, so the edge's accuracy is G, 1.01 L |
| G-SLACK plane | 4 | sampled G ≈ 1e-15, certified 1.1e-2 to 1.9e-2, L 3e-5 to 7.7e-5 | the true gap is tiny; only the proof is slack |
| G-SLACK B-spline | 4 | test8, G 3.51e-4 against L 3.86e-4 | within L once the gap is certified tightly |
| OFFSET | 20 | vase1/2/3, sampled G exactly L/1.1 | no certified gap on offset surfaces yet |
| BUILD | 6 | bottle, open_shell, vase3, G/L 0.70-0.91 | the fitter cannot reach the target in time; a looser fit is available |
| BOUND-SLACK | 1 | shell_fillet2, D 7.4e-5, L 2e-4 | within L; the bound is loose |
| NO-PROOF | 4 | variable_round, D ≤ 1.1e-6 (at most 2.3% of L) | well within L; no proof method for this surface kind |

### 6.2 Can all 40 be fitted with a stated, measured accuracy? (JUDGED from the MEASURED data)

**Yes. Main already builds all 40.**

- Main's `create_missing` raised 507 edges and 712 vertices on 41 bodies. The largest ratio per body was a median of 1.99× and a max of 3.46× (MEASURED, 35).
- With T plus the face pass, the raises fall to 40 edges and 54 vertices, all on refused bodies (MEASURED, 35).
- So the stated accuracies would be about 1-3.5× the budget L. In absolute terms that is a few times 1e-5 to a few times 1e-4 on these bodies.
- Of the 40, only 1 (FLOOR) is truly worse than L. Another 13 (G-SLACK, BOUND-SLACK, NO-PROOF) are within L, with the proof the only obstacle. The remaining 26 (OFFSET, BUILD) are best fits at a stated, measured bound.

**The bound must be certified, or explicitly marked as unproven.**

- The sampled 20-point gap understates the true gap on 293 trims, by up to 28× on torii_unioned (MEASURED, 34).
- If the fallback records a sampled value, a later coincidence test trusts an accuracy that is too small, and that can merge or split wrongly.
- For the 20 OFFSET trims there is no certified bound yet (35). Until there is, record a conservative stated bound (for example the sampled G times a safety factor) and mark it as unproven, so that a reader can refuse to rely on it for a coincidence decision.

### 6.3 Does anything downstream fail? (MEASURED, 35, 33)

- **check_brep.** Building the 40 the way main does gives the 4 check failures that T removes. T plus the face pass gives none new.
  - Those 4 come from main's settle raising neighbours, which the middle path's rule 4.1 does not do.
- **Answers.** Compared with T on the trims T accepts, main's are within 2.2e-6 relative in area and 1.8e-9 in volume.
- **Answers T refuses.** T refuses 407 classifications, 109 mass answers and 100 rays, and breaks 7 goldens, because it refuses the trim. Under the middle path those answers exist, at the recorded accuracy.
- **Settling.** Settling changes 50 ray answers (all last-bit ties), 19 mass answers and 4 check reports (MEASURED, 33). In 4 of the mass answers the face pass moves a trim by 1e-6 (1e-7 relative). These are the size of change to expect when a recorded accuracy replaces a raise.

**JUDGED.** Nothing measured fails because a fallback accuracy is 1-3.5× L. The risks are elsewhere:

1. A recorded accuracy that is too small (sampled, not certified). Addressed in §6.2.
2. A large one making a later Boolean slow, because boxes grow. Addressed by §3.2's split between surface and boundary.

One question is left for the owner: should there be a ratio past which the fallback is reported as a warning? See the questions below.

## 7. Session resolution

### 7.1 What solidcore does (READ)

- `size_estimate_to_zone_tol_3d` (`k/geom/tol.rs:331`) is a decade table:
  - 0.5-500 → 1e-5;
  - 500-5e4 → 1e-4;
  - and so on.
- It is applied per body through `brep_size_zone_tol_3d` (8 lines in 6 files).
- Doc 09 §4 proposes `settings.resolution_for(max(size_a, size_b))` per operation.
- After a scale, solidcore lifts the whole body to its new size zone (MEASURED, 30: a one-time ×10 on vase1's faces).

### 7.2 Is one value enough? (JUDGED)

- **For one part class, yes.** The table gives 1e-5 for every model from 0.5 mm to 500 mm, which is the whole range of the test corpus. Nothing in research 30-46 needed a different resolution.
- **Across part sizes, no.**
  - A 5 m frame at 1e-5 is a relative resolution of 2e-9, which is near double precision for coordinates of that size once copies and transforms add rounding. `within_contact_tol` already adds γ₃₂·size for this.
  - A 0.1 mm MEMS part at 1e-5 has features near the resolution.
  - So the resolution must scale with the model's size.
- **It must be fixed per session or per document, not per operation.**
  - If it is chosen per operation from the operand sizes, the same two faces compare at 1e-5 in one operation and 1e-4 in another, depending on what else is in the operands. Repeating one operation still gives the same answer, but the answer for a pair of faces then depends on the history that put them in that operation.
  - A scale that crosses a decade would then change every entity's floor (as in research 30 (d)).
  - So the size class is chosen once from the document's declared extent (or the first model), recorded with the document, and changed only by an explicit user action that is itself an operation.
- **Determinism.** A fixed, recorded resolution makes every test a pure function of its inputs plus that value. Recomputing it per operation also gives deterministic answers, but answers that depend on which bodies were in the operation. Fixing it is the simpler guarantee.

## Questions only the owner can answer

1. **Resolution scope.** Is the resolution fixed per session or per document? How is the size class chosen for a large assembly: declared by the user, or taken from the first body? And may a user change it later, as an operation that re-checks everything?
2. **Surface accuracy.** Do you accept a recorded accuracy on approximated surfaces (procedural blend nets, offsets fitted as NURBS, solved caches read from files)? This is a narrow addition to "edges and vertices only", but it is a change to the model as stated.
3. **Fallback cap.** Should a fallback accuracy past some ratio of the resolution be reported as a warning, or refused (for example 100× `res`, where research 30's ×120 `face_trims` case sits)? Or is any measured value accepted silently? The 40 measured cases are all at most 3.5× L.
4. **Unproven bounds.** Until offset and procedural surfaces have a certified gap, may their fallback accuracy be a stated, conservative bound marked as unproven? Or must those 24 cases wait for the certified bound?
5. **Entities with no measurement on import.** Is the file's stated uncertainty, applied to those entities alone, the right value? Or should they get `res` plus a note?
