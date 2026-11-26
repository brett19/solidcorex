# 27 — Can a UV trim be a pure fill-once cache behind `&Brep`?

Read-only design study. Base: `sc-wt/cache-chk-trims-once` at 82f12a5bfe. Paths are relative to
`solidcore/crates/solidcore/src` unless they name another crate. Nothing was built or run. Labels:
**VERIFIED** means read in the code at the cited lines. **INFERRED** means reasoned from the code or from
earlier reports' measurements and not checked by a run. **UNVERIFIED** items list the count that would
settle them.

The coordinator's two additions are both covered: design (D), the owner's intent, is in section 3.4 and
ranked first in section 5. The owner's criterion (signatures tell the truth; `&Brep` means nothing
observable changes; one signature per operation) is the first ranking key in section 5.

## 0. Verdict

1. **A trim can be a pure fill-once cache behind `&Brep`, but only if attaching one stops writing
   anything of meaning.** Today a creation writes, at least: a tolerance raise from the trim's bound (the
   size depends on the edge's current tolerance and on which other trims exist); a second raise inside the
   fit when it had to loosen; an interval change on the edge plus a re-trim of the edge's other trims; a
   vertex-face-gap reset; and, through the face routine, moved ends of neighbouring trims and vertex
   raises. Each of these has to be moved out of the lazy path or made a function of the body alone. The
   curve itself nearly is one already. The raise is the part that cannot be lazy.
2. **Design (D), the owner's intent, works in one form only, called D2 below:** the trim is fitted to the
   claim the body already makes, and no trim ever raises a tolerance. The read verifies the claim
   (`B ≤ τ_e + τ_f`, the bound §5.2 / Edge test 13 already accepts). If verification fails, the read
   returns a final typed refusal, and the owner runs one explicit `&mut` settle step.
   - **D1 does not work.** D1 is "raise up front from an a-priori bound, then build lazily". The fit has a
     target but no a-priori bound: it is tested at 3 samples per span, escalates to twice the tolerance,
     and falls back to a brute-force interpolation with no target. On blade_hub_1 the certified bound
     (≈3.9e-5) is about 4 times the sampled distance (≈1e-5), so an up-front raise either fails
     verification or must be about 4 times today's sampled raise, on every edge.
3. **D2 moves results.** Tolerances that today rise through trim raises stay lower. On imported bodies
   like blade_hub_1, most edges will fail verification and need the settle step anyway. For the costly
   bodies, D2 therefore degenerates to (C): a visible prepare once, about 1.2 s. Its gain is signature
   purity and the cheap bodies, not speed.
4. **Recommendation:** first (C) with a typed precondition, plus the order-independence parts of (B), now.
   That gives honest signatures with no new interior mutability. Then D2 as a later, result-moving step,
   once the owner agrees the tolerance-model reading in section 4. Plain (A), with the raise derived on
   every tolerance read, is not worth it: it changes the meaning of `Edge::tolerance`, puts a 10 ms fill
   behind about 450 tolerance-read sites, and still needs every change D2 needs.

## 1. What creation computes and writes

### 1.1 Edge-use routine `create_uv_trim_curve` (`topology/edgeuse/p3.rs:482-1112`)

**What it reads (VERIFIED):**
- Edge record: `curve`, `interval` (`:515-525`).
- The face's `surface` (copied for evaluation) and its `uv_domain` (`:508-514`).
- Orientation of the edge use and of the edge's primary edge use (the curve-on-surface branch,
  `:543-549`).
- `fit_tolerance(k,b,e)` (`:413-468`). It reads:
  - the edge's current `tolerance`, replaced by the "before" value in `TrimCoverRaises` when the edge
    still holds the "after" value (`:417-422`);
  - every face tolerance of the edge (`:424-430`);
  - the edge uses' cached max edge-face gap, or that gap computed without storing it (`:439-452`);
  - kernel tolerances (`zone_tol3d`, `gap_gain`).
- Iteration 2 (`:675`): `max_edge_face_gap(k,b,eu,false)`, which takes `&mut Brep` (`edgeuse/p1.rs:907`)
  and can store the gap.
- The two-curve short-piece rule reads the edge tolerance and both vertex tolerances (`:820-828`).
- The shorter-trim recovery reads the edge tolerance, the vertex tolerances and the vertex points
  (`:963-1001`).
- Seams: `pick_seam_curve` (`:1357`) takes `&mut Brep` and calls
  `normalized_evaluate(.., ok_to_make=true)` on the same edge use (`:1373`). That call can create the
  trim reentrantly, which is why the routine later deletes a "duplicate" (`:913-916`, `:1059-1062`).
- Kernel: `k.tolerances`, plus `k.fixes.bspline_trim_preserves_uv_spans` in `trim_other_uv_curves`
  (`:1210`).

**Does it read a neighbour's trim?** Not for the fit itself (VERIFIED). But it reads the edge's current
tolerance, which an earlier trim on another face of the edge may already have raised. `TrimCoverRaises`
masks only raises made by the read-time cover (`brep/p2.rs:578-597`), not raises made by
`set_uv_trim_curve`. So the curve can depend on build order through `fit_tolerance` (VERIFIED as a
dependence).

The measured result is that the curve and interval were identical across 1,543 builds by different
routes. INFERRED: either the drop's span test passes the same way at both fits, or the sibling is
seldom raised first.

**The fit loop and its target (VERIFIED; answers D's question 1):**
- The target is `zone_tol = fit_tol` (`:661`). Iterations 0 and 1 run `DropCurve` and
  `DropAndTrimCurve` at it. Iterations 2 and 3 run at `max(2·gap, 2·fit)` (`:672-685`).
- A span passes when 3 Hermite samples (`surface_drop_lib/p1.rs:514-575`) each satisfy
  `|deviation from normal line| < approx_tol · dist` (`surface/surface/p4.rs:1115-1121`).
  - `max_dist_to_surf` is the sampled distance from the edge curve to the surface. It is the data's gap,
    not a fit error.
  - `max_approx_dev` is the sampled fit deviation.
- **No a-priori bound exists.** The samples are a lower estimate. When all 4 iterations fail, the
  routine uses `brute_force` (`:1082-1097`): 101 dropped points interpolated, with no target.
- It does not refuse when it misses the target. It loosens. With `ii>1`, `finish` raises the edge to
  `1.01 × error` (`:1171-1181`), inside the creation itself.

**What it writes (VERIFIED):**

| Write | Where | Class |
|---|---|---|
| Face surface owner cleared and restored, `begin/end_surface_owner_cleared` | `:646-655`, `:1103-1108` | S: scratch, makes it `&mut` today |
| Edge-face gap stored on iteration 2 | `:675` via `p1.rs:907` | derived cache |
| `edge::set_interval` plus `trim_other_uv_curves`, which re-trims every other trim of the edge | `:1017-1025`, `:1041-1049`, `:1193-1225` | **write of meaning** |
| Raise `1.01 × error` when `ii>1` | `:1171-1181` | **write of meaning** |
| Duplicate trim deleted (reentrant creation) | `:913-916`, `:1059-1062`, `:1611` | cleanup |
| `sync_face_surface_tolerance` | `:1110` | follows the face tolerance |

### 1.2 `get_or_create_uv_trim_curve` (`edgeuse/p3.rs:1701-1784`)

1. If a trim exists, it returns it (`existing_uv_trim_curve`, `:2785`, read-only).
2. Otherwise it creates the trim.
3. `trim_raise_distance` (`:1833-1893`) chooses the raise:
   - It reads the current edge tolerance, `fit_tolerance`, and `k.caches.prove_lazy_trim_raise`, which
     the import path toggles (`kernel.rs:1027`).
   - It reads the certified `B = trim_deviation_bound` (`:2689-2720`). B's inputs are the trim, the edge
     curve, the interval, a surface copy and the fit; it does not read the tolerance itself.
   - When B lowers the raise below upstream's: `lowered_covers_rounding` and `other_trims_bound`
     (`:1940-1996`). The latter reads every other trim of the edge and its certified bound, plus the
     edge's gap bound to each untrimmed face.
   - The resulting tolerance:
     - `2B` when `B < u` and the other trims stay covered;
     - `2u` when `u ≤ B ≤ 2u`;
     - `B` when `B > 2u` and B is within the check's limit.
     - Here `u` is the drop's reported distance, which depends on `want_deviation` (#21161).
4. `set_uv_trim_curve` (`p1.rs:716-828`):
   - it attaches the trim to the Same member of the pair;
   - it resets the vertex-face gaps of both uses (`:773-779`);
   - unless the body is validate-only, it raises the edge to `2 × raise` with the cascade to the vertices
     (`:808-825`, `edge.rs:2312-2340`).

### 1.3 Face routine `create_uv_trim_curves_body` (`topology/face/p3.rs:1295-1480`)

- First pass: for each edge use without a trim, `get_or_create` (`:1310-1395`).
- Second pass, at **every** corner of the face, including corners whose trims already existed
  (`:1397-1470`):
  - `fix_uv_trim_curves_notified` (`face/p2.rs:3366`) moves both trim ends to the vertex use's UV
    point. That point comes from the cached vertex-face gap, or from `compute_uv_point`, whose guess
    comes from the two neighbouring trims' ends (`vertexuse.rs:1159-1166`).
  - The move goes through trim, `edit_parameterization` and an end-point edit, with
    `keep_trim_covered` raises (`face/p2.rs:3620-3650`).
  - `tol_3d` is passed but not used (`let _ = tol_3d;`).
- With `adjust_tolerances`, each vertex is raised to `2 × gap_3d` (`face/p3.rs:1465-1467`).

Read paths reach this routine with `adjust_tolerances = true` through the face cache tree build:
`trim_srf_cache.rs:851` calls `FaceQueries::create_uv_trim_curves` (`query/face/p2.rs:256-259`). So a
classification read can move trim ends and raise vertices even when every trim exists. INFERRED: once
the gaps are closed it is idempotent. Report 25 measured no tolerance moved on 30 body-states.

### 1.4 Which parts are pure functions of state the creation does not change

| Part | Pure? |
|---|---|
| The dropped curve on the common path (iterations 0 and 1, no interval recovery) | **Yes**, given the edge curve, interval, surface, domain, orientations, fit tolerance (edge tolerance, face tolerances, gaps) and kernel settings. |
| The certified bound B | **Yes**, given trim, curve, interval, surface and fit. |
| `ii>1` paths | **No**: `finish` raises the edge before `trim_raise_distance` reads tolerance and fit again. The creation changes its own bound's input. |
| Interval recovery | **No**: changes the interval, B's input, and other trims. |
| The raise | **Not a function of the trim alone**: it depends on the current tolerance and on the set of other trims. |
| The UV-gap fix | **Not a function of one trim**: it depends on both neighbours and on the vertex-use UV, which itself comes from the neighbours' ends. |
| Seam selection | **Pure in substance** (it picks among dropped curves), but it runs through a reentrant `&mut` evaluate today. |

## 2. Readers, and whether a reader needs the raise

**Readers of a trim (VERIFIED by grep, non-exhaustive):**
- the trimmed-surface-cache implant (`trim_srf_cache.rs:851-885`), used for classification and mass;
- face point classification (`query/face/p2.rs:836-1062`, with the variants `Mutable`, `Prepared` and
  `Requested`);
- `loopuse::prepared_trim` (`loopuse.rs:170-190`);
- `normalized_evaluate`, behind the validity walk;
- `compute_uv_point`;
- the mass integrator (`mass/integrator/properties.rs`);
- rays (`query/brep/ray.rs`);
- fillet, offset, boolean and heal.

`get_or_create_uv_trim_curve` has 47 call sites in 27 files.

**Does the reading query's own answer need the raise?**
- **Yes for the tree build.**
  - After creation it refreshes the face zone tolerance (`trim_srf_cache.rs:852-858`, "Creation may
    raise the owning face's tolerance").
  - It implants each trim with `owner_zone_tol_3d` from the edge-use tolerance (`:875-878`,
    `query/face/p2.rs:324-333`).
  - It implants each vertex use with `vertex_tolerance` (`:889-895`).
  - So a tree built from a trim without its raise has narrower zones (VERIFIED). Whether a
    classification answer flips is INFERRED: only for points within the pcurve's deviation of a
    boundary.
- **Yes for the validity tests:** gap tests compare against τ.
- **Mass, area and volume:** INFERRED that they need only the curve.

**Effective tolerance derived on read**, `τ_eff(e) = max(stored, cover(trim))` over the edge's trims:
- Every edge-tolerance reader would have to call it. Grep finds ≥295 edge-tolerance read sites and ≥153
  vertex-tolerance sites; this is a lower bound, since direct `rec.tolerance` field reads are not
  counted.
- A vertex's value is the maximum over its incident edges' effective values plus the face routine's
  `2 × gap_3d`.
- Per read: 2 to 4 cell lookups per edge (about 3 to 6 edges per vertex) once filled. The **first** read
  must fill every trim of the edge, about 10 ms each on blade_hub_1 (1.2 s / 120). A tolerance read
  becomes the most expensive read in the kernel.
- `fit_tolerance` must keep reading the *stored* value; otherwise a cycle forms.
- The hot readers are the boolean's imprint tolerance updates, the intersection tolerance
  (`xsect_tol_3d_from_zones`), tree implants and the gap tests (INFERRED from call sites; the hit
  counts are UNVERIFIED, countable by a side-channel counter on `edge::tolerance` per operation).

## 3. Designs

The trim slot today holds **two kinds** of trim (VERIFIED):
- **authored trims**: set by boolean, fillet, offset, sweep and file reads through
  `set_uv_trim_curve`. It has 51 call sites in 24 files, plus about 111 direct field writes in about 40
  files;
- **dropped trims**: made by creation.

A cache can only hold dropped trims. Every design keeps the authored slot as data.

### 3.1 (A) Fill-once trim and bound per edge use, raise derived on read

**Cell:** a new `Brep` field, `trim_cells: Stamped`-exempt `Arc<…>` keyed by `EdgeuseId`. Each entry is
`OnceLock<DerivedTrim { curve, bound, fail_record }>` plus a key.

**Key:** the write stamps of `edges`, `edgeuses`, `faces`, `vertices` and `tolerance`
(`topology.rs:961-1035`; every `&mut` access bumps them, `topology/stamped.rs:46`) and the trim-relevant
kernel settings:
- `k.tolerances`;
- `fixes.bspline_trim_preserves_uv_spans`;
- `fixes.uv_trim_corner_end_points_only`;
- `prove_lazy_trim_raise`.

Without the settings in the key, two kernels with different settings fill one body's cell first-wins.
That breaks "same value whoever computes it".

**Precedents:**
- `NurbSlot` (`nurbs/b_spline_surface/p1.rs:461-511`): `Arc<OnceLock>`, shared by evaluation copies,
  detached on `&mut`.
- `CurveMemo` (`curve/curve/p5.rs:479-486`): left out of `Debug` as "not part of the curve's observable
  state".
- The surface `CacheSlot` (`surface/surface/p2.rs:509`, `Arc<Mutex>`):
  - shared by `clone_for_evaluation` (`:987`);
  - fresh on `Clone`, `detach_for_edit` (`:1026`) and `snapshot_copy` (`:1042`).

**Copies, snapshot, equality, dumps:**
- `Brep::copy` gives new origins (`topology.rs:907-940`), so stamps differ. The cells must be carried
  explicitly with a re-keying, or dropped. Dropping is simpler; it costs a refill per copy.
- Snapshot and rollback (`topology.rs:1432-1534`): carry the cells; they are valid because the snapshot
  holds the same inputs.
- `write_stamps` destructures every field (`topology.rs:965-1005`). The cell field must be listed there
  but must not count as a write.
- `Debug` must skip it, as `CurveMemo` does, so dumps don't depend on read history.
- `to_record` must export what it reads: derived trims become authored trims in the record, with equal
  values.

**Race:** `OnceLock::get_or_init` runs one initialiser, and the others block. The inputs are frozen under
`&Brep`, so the values are identical. `Brep` stays `Send + Sync` if `DerivedTrim` is `Send + Sync`.
`Curve` is listed `Send + Sync` in the brief, but `curve/curve/p5.rs:482` says "not Sync". UNVERIFIED:
compile-check before relying on it.

**Invalidation sites:**
- Every write that sets or removes a trim: the 51 `set_uv_trim_curve` calls, about 111 direct field
  writes, `remove_uv_trim_curves` (`face/p1.rs:1495`, `brep/p4.rs:264`), `clear_uv_trim_curves`
  (`trimming_tools.rs:779`), `reverse_orientation` (`edge.rs:2369`), `check_and_fix_uv_trim_curves`
  (`brep/p2.rs:520`).
- Every write of an input: curve, interval, surface, orientation, tolerance, transform.
- With stamp keys, all of these are covered at once. The price: any edit stales every cell (INFERRED
  acceptable, since edits happen in the write phase).

**What moves:**
- The raise rule must become per-trim (no `other_trims_bound`). That changes results wherever today's
  lowering or keeping of upstream's raise depended on siblings.
- The interval recovery and the `ii>1` raise must leave the lazy path (see D).

**Cost:** the highest. It changes every tolerance reader (section 2) and the meaning of the stored field.

### 3.2 (B) Trims stay data; creation made order-independent

Changes:
- Raise by a per-trim rule, or enclose every bound first and raise after. `cover_kept_uv_trim_curves`
  (`brep/p2.rs:578-598`) already does "enclose all, then raise" and records the before values.
- Fix #21161 so `want_deviation` does not change the raise.
- Run the UV-gap fix as a separate pass over the finished set of trims, at fixed corners.
- Record raises in a `TrimCoverRaises`-like before map, so `fit_tolerance` of a sibling is not loosened.

Then a prepare step is neutral: any route gives the same body. The results move where the order mattered
(the 7 bodies of #21135, blade_hub 28 edges and 43 vertices).

### 3.3 (C) Status quo plus the ops layer creating trims once when a body enters

This is brett19's 2026-11-23 comment on #21105. `Body::from_record` leaves no trims; `to_record` calls
`create_missing_uv_trim_curves` (`solidcore-convert/src/brep/record.rs:131`). Report 25 measured trims
present at entry on 28 of 30 body-states. No result moves except the timing of the raises.

### 3.4 (D) Write phase raises up front, then reads fill trims lazily (owner's intent)

**D1, the raise computed up front without the trim: not possible as a bound** (D question 1).
- The fit has a target, `fit_tolerance`, but only sampled, with escalation and an untargeted brute force
  (section 1.1).
- An up-front value would have to be `2·(G + T)`, where:
  - G is the edge's certified gap to the face. `edge_face_gap_bound` (`edgeuse/p3.rs:2621`) needs no
    trim, but it is certified and costs a bisection too.
  - T is the fit target, which is the edge tolerance itself.
- So every edge with a dropped trim would at least double, `≥ 2·(τ + G)`. Even that is not proved
  enough. On blade_hub_1 (coordinator's #21161 figures, INFERRED reading):
  - dist ≈ 2.6e-6;
  - the sampled whole distance ≈ 1e-5;
  - the edge-19 results are 3.86e-5 and 4.18e-5, i.e. B ≈ 3.9e-5, about **4 times the sampled distance
    and about 15 times dist**.
- So the fitted trim, or its enclosure, lies about 4 times farther than the target says. An a-priori
  raise would need a factor of about 4 on top of the target and would then raise edges that today keep
  their tolerance. The 99-of-120 raised trims and the edge before-values are not in the reports:
  UNVERIFIED, countable by logging τ before and after and `2(G+T)` per edge on blade_hub_1/3 and
  cliff_1.

**D2: the trim fitted to the existing claim, no raise from any trim.**
- Fill-once cell as in (A), but nothing is derived on tolerance reads.
- What the read verifies (D question 2): `B ≤ τ_e + τ_f`.
  - This is the cover the model already grants a point on S near C. §5.2 and Edge test 13 allow
    `g ≤ τ_e + τ_f`.
  - It is a within-test, `trim_image_gap_bound_within` with `enough = τ_e + τ_f`. That is cheaper than
    today's full bisection, which is "not stopped at a cutoff" when raising (`:1853-1857`), and
    `trim_deviation_bound`'s note cites 274 s against 3.5 s for such tightening.
  - It still needs the certified bound. The cheap sampled distance proves nothing (§8, "a bound must be
    a bound").
- On failure, the read returns a final typed error (not `PreparationRequired`, which today means "the
  caller fixes it and resumes"). The owner then runs a `&mut` settle step.
- What cannot be lazy (D question 3), each with its D2 placement:

| Item | In D2 |
|---|---|
| The interval recovery (`:1017-1049`) | Replaced by re-parameterising the trim onto the edge interval, which is pure; invariant 4 asks that the trim share the edge interval. It moves results on those edges. Otherwise it goes to settle. |
| The `ii>1` raise (`:1171-1181`) | Becomes "verification fails, so settle". |
| The UV-gap fix | Goes to settle, as a function of the finished set of trims at fixed corners. It cannot be lazy: it edits two trims at a shared vertex. |
| The vertex raise `2·gap_3d` | Goes to settle. |
| Seam selection | Stays lazy once `pick_seam_curve` stops evaluating through a reentrant `&mut` make. |
| The face-routine call from `trim_srf_cache.rs:851` | Must stop creating or fixing in every design. |

- What moves: every edge and vertex tolerance today raised by a trim. On blade_hub-like imports most
  trims will fail `τ_e + τ_f` (B ≈ 3.9e-5 against a probable τ ≈ 1e-5 to 2e-5). These bodies then need
  settle, which is (C)'s 1.2 s prepare. The cheap bodies (boolean results: 15 to 18 trims, never reach
  the bound, about 10 ms, #21131) fill lazily and write nothing.

**D question 4, no stored raise:** see section 2. That is (A), not D2.

## 4. Tolerance document (`solidcore/docs/validity-and-tolerances.md`)

**§5.2** (pcurve not in the edge-face gap; tests 5 to 7 not evaluated):
- (A) and D2 make the pcurve's distance an explicit cover question.
- D2's `τ_e + τ_f` needs a sentence there: "a pcurve's image is covered by the edge's tube plus the
  face's band, as the edge-face gap is". That is a decision for the owner.

**§8** (a tolerance is a claim; a bound must be a bound; growth recorded):
- Today's trim raise grows τ for an approximation the kernel made itself. By §8 that is the "construction"
  branch: fit within the claim or refuse, never widen. D2 is the design consistent with §8.
- (A) keeps widening, but on read, and records nothing on the body. It is consistent only if
  "effective τ" is defined as the claim.
- (B) and (C) keep the growth. (B) records it (before map), which §8 asks for.

**§9:**
- Invariant 4 (pcurve shares the edge interval) is touched by the interval recovery in all designs.
- Invariant 9 (constructed tolerances from certificates) is satisfied only by D2.
- Invariant 11 (a check is a function of the body) holds under (A) and D2 by construction, and under
  (B)/(C) only once the check stops creating (#21105 step 1).

**§10:** the open items "No growth is recorded" and "The pcurve is not measured against its edge" are
closed by (B) for the first and by D2 for the second.

**Goldens:**
- (C) moves none.
- (B) moves the order-dependent bodies (#21135's 7, plus #21161's tolerance changes).
- (A) and D2 move every body whose trims raise today. Plus the kernel_cases and `uv_trim_edge_tolerance`
  tests (`solidcore/tests/uv_trim_edge_tolerance.rs`), and every unit whose expectations print edge or
  vertex tolerances after a trim-making read. That set is UNVERIFIED; count it by running all goldens
  with the raise disabled.

## 5. Ranking

The owner's criterion comes first, results moved second, cost third.

| | Signatures | Interior-mutable fields added | Pairs deletable? | Results moved | Cost |
|---|---|---|---|---|---|
| **D2** | Reads (volume, area, tight box, classification, rays, check) take `&Brep` and fill trims; a trim failing its claim is a final typed error. `settle_tolerances(&mut)` replaces `create_missing_uv_trim_curves`. | Trim cells: pure, a function of the body plus key, with the raise removed. Existing ones unchanged: surface `CacheSlot`, `NurbSlot`, `MadeKindSlot`, `CurveMemo`, `ContinuityMemo`, `SingularitiesMemo`. | Yes for trims: `point_classify_prepared`, `QueryTopology::{Prepared,Requested}` (`query/face/p2.rs:857-909`), `prepared_trim`'s `PreparationRequired` (`loopuse.rs:170-190`), `LazyBrep`/`Target` copy-on-first-write (`query/brep.rs:1085-1160`). The surface-cache `PreparationRequired` (`ray.rs:89`) stays until the tree is a cell too. | Most: every trim raise | High: about 4 PRs |
| **C + B + typed precondition** | Reads take `&Brep` and require a prepared body: a type such as `Prepared<'a>` made only by the `&mut` prepare. On a missing trim they refuse; there is no fallback. Exception: the check, which follows the #21105 answer (a): a private copy, the caller's body unchanged. `create_missing_uv_trim_curves(&mut)` is the prepare, canonical by B. | None new | Yes, the same set, once every caller prepares (ops at body entry; kernel operations after their edit phase) | B's order cases only | Low: 2 to 3 PRs |
| **C alone** | As above, but the prepare is order-dependent | None | Yes | None | Lowest |
| **A** | Reads take `&Brep`; tolerance reads become `&Brep` fills | Trim plus bound cells; `τ_eff` | Yes | Raise rule plus recovery | Highest: about 450 reader sites |

C+B and D2 are both honest. C+B is honest with no new cell and few moved results, so it ranks first in
practice. D2 ranks first as the end state if the owner accepts the §5.2/§8 reading.

## 6. First steps

1. **Stop the read path's hidden creation.** `trim_srf_cache.rs:851` (and `FaceQueries`) create trims or
   fix corners only on a `&mut` prepare; a tree build on a body without trims refuses with a typed
   error. Results should not move on prepared bodies (report 25: no tree build creates there). Counts
   that should fall: trims built on copies (`ball_minus_corner` 6, `c_endcap` 1 per call), and the
   per-call re-making on `c_endcap` (100 of 100 calls, 41 ms).
2. **Make the prepare canonical (B), one change at a time:**
   - land #21161 (moves the check-route tolerances);
   - enclose all bounds, then raise, with a before map (moves the #21135 order cases);
   - run the gap fix as a separate pass in a fixed corner order.
   Count that should fall: bound computations per prepare (#21131: 51 of 100 repeat identical inputs;
   enclose-all-first removes them).
3. **Measure D2's reach before building it.** Side-channel instrumentation, local only: per trim on the
   corpus, log `B`, τ_e, τ_f, and whether `B ≤ τ_e + τ_f`. That fraction is the share of bodies D2 can
   serve lazily, and the share that would move. No results move.

UNVERIFIED, with what settles each:
- the frequency of the interval-recovery and `ii>1` paths: count `DropCurveFail::create_path_type` per
  corpus body;
- whether `Curve` is `Sync`: a compile check;
- whether a missing raise flips any classification: rerun report 25's 629 comparisons with the raise
  suppressed.
