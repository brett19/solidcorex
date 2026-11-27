# Review: research 66, other kernels' performance

This is an adversarial review of `docs/research/66-other-kernels-performance.md` (commit 8a96535).
It was checked against the following:
- the pin, solidcore 03e16c9 (docs/06). Pin paths are relative to `crates/solidcore/src`.
- 05 and research 60 and 65;
- the 2026-10-27 benchmark profile, which is solidcore main dbaedda4a and is not in this repository;
- these upstream trees, at the SHAs 66 cites, read for their code because their licences permit it:
  - Manifold 57cf361 (Apache-2.0)
  - truck 117470c (Apache-2.0)
  - Fornjot 7e85668 (0BSD)
  - springrep f2ad60e (MIT/Apache-2.0)
  - arris 4408202 (MIT/Apache-2.0)
  - verb f3ba23c (MIT)
- OpenNURBS 2bd2e66. Its code was read, but see L1.
- the public OCCT Boolean specification at occt3d.com. No OCCT, CGAL or SISL code was read.

Each finding has a severity: **wrong**, **overstated**, **missing** or **minor**. Each one also has a
fix. Findings marked VERIFIED cite a file and line. Findings marked JUDGED are reasoning.

**Verdict: revise before tasking.**
- Rank 1 points at the wrong function. The 40% is not region classification, and the pin already
  floods by cell.
- Two of the "bit-identical" claims are false: the per-face memo (rank 3) and transformed boxes
  (rank 6).
- One cited source does not do what 66 says: Fornjot (rank 7).
- Most of the rest survives with narrower wording and a measurement first.

## 1. Findings

### F1. wrong: rank 1's 40% is point-in-face for curve intervals, not region in/out. VERIFIED.

66 §1 and rank 1 say that gear_60 spends 40% in "classification" and that a flood removes most of
it. The profile does not say that.
- The 40% node is `classify_face_interval` → `point_3d_classify`.
  - `intersect/curve_class/p7.rs:905` documents `classify_face_interval` as follows: it "Classifies
    interval `index`, known to lie on `face`, against it".
  - It uses the vertex sector or edge side at classified ends, and otherwise a point near the middle
    of the interval.
  - So this is the trimming of the SSI curve: which pieces of an intersection curve lie inside a
    trimmed face. It has nothing to do with whether a whole face of A lies inside B.
- Under that 40%, the cost is the point solve:
  - `global_point_solve` is 35.5%. It is a generic NURBS grid search, run here on a cylinder.
  - `check_bad_guess_stride` is 27.4%.
  - `fails_eval` → `eval_second_derivatives` is 15.7%.
  - The cost is per point and per solve, not per face.
- The pin already decides region membership per cell:
  - `boolean/merge/p1.rs:243` (`find_brep_region_of_other_face`) takes a single candidate region at
    once.
  - Otherwise it reads radial sectors at mated edges (`edge::find_radial_sector_between`).
  - Only for unmated edges does it fall back to a point classification, at a fixed edge parameter.
  - `propagate_other_src_region_attribs` then carries the result per region.
  - All of `make_faceuse`, which contains this, is 6.7% of gear_60.
- The same file's later notes undercut the easy reading:
  - `fails_eval` is faithful to upstream.
  - An analytic inversion (`DropPointFast`) already exists.
  - The notes call the point-solve cost "not an easy win".

**Fix.**
- Retarget rank 1 to "point-in-face cost on the curve-trimming path". Its levers are:
  - a certified uv-box reject before any solve;
  - analytic inversion for analytic surfaces;
  - reuse of the vertex-sector and edge-side answers along one curve.
- First measure why `DropPointFast` does not already take the gear's cylinder case.
- Drop the claim that flooding saves 40%. Describe flooding as already present in the pin's cellular
  merge (see F2).

### F2. wrong: the flood as briefed is not correct for non-manifold, lamina or cellular input. JUDGED, with VERIFIED sources.

66 §10.1 and the rank 1 brief say: classify one interior point per connected component, with the
lowest face id as representative, then copy the status across the component. These are the problems:
- **The seed is the wrong way round.**
  - truck (`faces_classification/mod.rs`, `integrate_by_component`) seeds from faces next to the
    intersection that already have And/Or status. It floods only the unknown components.
  - The pin seeds from radial sectors at the mated intersection edges.
  - Both are local, exact-topology decisions. An interior-point global solve is the slow, fragile
    fallback that the 40% is made of.
- **The status is per face use and per cell, not per face.**
  - In solidcore's cellular merge, a face of A can bound two cells, and the answer is "which cell
    of B". In/out is the wrong question.
  - A component bounded by a non-manifold edge has more than two radial neighbours. Flooding across
    such an edge without radial pairing (arris ADR-0090) joins cells that are distinct.
- **Laminas and sheets.**
  - A lamina face does not separate cells, so a lamina inside B's volume and the volume on either
    side have the same status. A face-based flood happens to get that right.
  - But a sheet operand has no inside. Classification against a sheet B is meaningless, and a flood
    of "inside B" would invent one.
- **ON faces.**
  - Faces coincident with B's boundary are ON. They need their own decision (same or opposite
    normal), and the flood must not cross from them.
- **truck is a weak model.**
  - It checks only `boundary[0][0]` of a component.
  - Its fallback is per face, a ray cast against a tessellated shell (`integrate/mod.rs`), with a
    hashed pseudo-random direction (`hash::take_one_unit`).

**Fix.** Recast rank 1's brief:
- Flood membership over connected components of face uses, split at every intersection edge and at
  every non-manifold edge.
- Seed from local radial or side-of-curve decisions at the split edges.
- Fall back to a point classification only for components that touch none of them.
- State ON and sheet handling explicitly.
- Note that this is already the shape of the pin's merge. Our job is to keep it and not regress it,
  not to add it.

### F3. wrong: the per-face mass memo (rank 3) is not bit-identical. VERIFIED.

66 §13.2 says memo reuse is exact and rank 3 says "adopt as cells". The pin does not compute a
face's integral independently of the body:
- `mass/integrator/properties.rs`: `brep_volume` is computed about the bounding-box midpoint
  (`bbox.mid()`).
- `boundary_properties` sets its error scale from `face_box.min_distance(origin)` and the estimated
  area.
- So a face's volume and moment contributions, and its subdivision depth, change when the body's box
  changes. A boolean that removes material changes the box, even for faces it does not touch.

Faces that look uncut can still change:
- They can gain inner loops.
- Their edges can be split by imprinting.
- They can reverse sense as face uses of B in a difference.

springrep's `FaceMemo` (`memo.rs`) keys its `integrals` and `moments` on an `IntegralKey`: centre,
size, tolerance and sense. That key is exactly what 66's brief leaves out. springrep also holds a
global `AtomicU64` id and `OnceLock`/`Mutex` fields. That is ambient state, and our determinism rules
forbid it.

**Fix.** Rank 3 needs one of these changes:
- (a) Fix the reference point and the error scale so that they do not depend on the body. For
  example, integrate about the face's own box centre and shift the results exactly with the parallel
  axis theorem. Then key the memo on the face's geometry, its loops, its sense and the accuracy. Only
  with this is the memo bit-identical.
- (b) Accept a "truth-checked" label and validate against exact volumes.

In either case the memo lives in an explicit cache owned by the caller, not in global ids.

### F4. wrong: transforming boxes instead of rebuilding them (rank 6, §18 Q4) is not a certified enclosure as cited. VERIFIED.

- **Manifold.** `collider.h:364` `Transform` asserts `IsAxisAligned`. `impl.cpp:629` transforms the
  collider only for axis-aligned transforms and otherwise calls `UpdateBoxes`, which refits. So
  Manifold does not do what 66 credits it with for rotations.
- **OpenNURBS.** `opennurbs_rtree.h:459-487` uses the |L|·halfwidth trick. For a rotation this
  inflates every box. Its comment calls the result a "conservative enclosure".
- **Repeated rotation compounds the inflation.** The gear's 60 rotated tools are exactly that case.
  The boxes lose the tightness that research 60's Q4 wants.
- **Rounding.** Neither implementation rounds outward. A box transformed in floating point does not
  certainly enclose the rounded transformed control points.
- **Tree shape.** A tree kept through a transform is not the tree a rebuild would produce, so query
  visit order differs. Results that depend on visit order are then not bit-identical to a rebuild.

**Fix.**
- Adopt exact box transport only for signed-permutation, uniform power-of-two scale and translation
  transforms, with outward rounding.
- For general rigid motions, refit leaves from the transformed geometry and keep the topology only
  if the query result is order-independent.
- Answer Q4 with "no, except those cases".

### F5. wrong: Fornjot's curve cache is not a pure function of the edge (rank 7 source). VERIFIED.

66 cites Fornjot `CurveApproxCache` beside arris ADR-0052 as "edge samples as a pure function of the
edge".
- Fornjot keys the cache on `(Handle<Curve>, CurveBoundary)`.
- But `approx_curve_with_cache` takes a `surface` and computes the samples on `local_on(surface)`.
- The first face to ask fills the cache, and the second face reuses samples sized for a different
  surface. The result depends on face order.
- The same path has a `todo!()` for a circle on a curved surface.

arris ADR-0052 is the real source:
- Its count is the largest that any coedge asks for, each read along its own pcurve box.
- It takes NURBS curvature over the whole surface domain, which is conservative and costs extra
  samples.

**Fix.**
- Cite arris only.
- Name the order-dependence as the thing to avoid.
- Note the cost of the whole-domain curvature bound.

### F6. overstated: rank 5's Gauss-Kronrod, as sourced, is not adaptive Kronrod and is not bit-identical across targets. VERIFIED.

- **It is not adaptive Kronrod.** springrep `measure.rs:887-910` (`integrate_pieces`) does a one-shot
  G3K7 or G7K15 acceptance test per piece (`kronrod_k`), then falls back to adaptive Gauss halving
  (`adapt_k`). 66 §13.1's "adaptive 15-point Kronrod" misreads it.
- **Its nodes are not fixed.** springrep `numeric.rs` (`legendre_rule`) computes its Gauss nodes at
  run time, using `cos` and Newton steps. `cos` comes from libm and differs across x86-64, aarch64
  and wasm32. Only the Kronrod constants are hard-coded.
- **"2-3 orders fewer evaluations" contradicts 65.** 65 row 8 says 5-20x. Neither number was
  measured on our integrand.
- **The pin already uses G3K7 in places.** 65 cites `properties.rs:1002`. The baseline is therefore
  not plain Gauss everywhere.

**Fix.**
- Hard-code every node and weight as exact literals.
- Measure evaluation counts on plate_12's faces before giving a ratio.
- Label the result "truth-checked": a different rule changes the last bits.

### F7. overstated: rank 4's NS2 brief misdescribes OpenNURBS and repeats work already done. VERIFIED.

The OpenNURBS buffers are not what 66 says:
- `ON_EvaluateNurbsBasis` puts only 2d doubles of knot differences in its `stack_buffer[80]`. The
  order² basis triangle is a buffer the caller provides. 66 §4.1 says the triangle goes in the stack
  buffer.
- `NonRationalSpan` uses `[64]` for the order² basis.
- `SurfaceSpan` uses `[128]`, sized as
  `((order0²+order1²)·8) + Pcount·cvdim·8` bytes, where `Pcount = (der+1)(der+2)/2`.
- So the brief's `order0²+order1²+count` buffer is not OpenNURBS's formula.

The span hint is a small gain:
- `ON_NurbsSpanIndex` uses the hint to narrow one side only, then does a binary search
  (`ON_SearchMonotoneArray`).
- For the few-knot surfaces in the corpus, that saves one or two compares.

The brief repeats work already done:
- 05 records that NS2 already has degree-sized basis scratch.
- 05 lists the point's fixed cost (span search, `Result` moves, `Basis` size) as unmeasured.

The hint path needs a check:
- It is bit-identical only if it applies the same `Approach` rule at knot ties as the full search.

**Fix.**
- Correct the description.
- Make rank 4 "measure the fixed cost first, then try a span hint", with a difftest that pins
  knot-tie inputs.

### F8. wrong: Manifold's tie rule is tolerance coincidence, not simulation of simplicity (§11.1). VERIFIED.

`boolean3.cpp`:
- `Shadows` compares `|p-q| > tol ? p<q : movedP-p < movedQ-q`, where `TieTol` is the minimum
  epsilon.
- `Moved::P` displaces by `(expandP ? 1.125 : -1.125)·tol/4` along the vertex displacement, plus a
  fixed skew step of (1/61, 1/89, 1/113).
- `expandP` is true only for `Add`. So intersection contracts P as well as difference does, and
  66's "expand for union, shrink otherwise" is only half the picture.

This is a tolerance rule:
- Values within `tol` count as equal, and that relation is not transitive.
- Calling it "simulation of simplicity by another name" is wrong.
- Adopting it inside tessellation would conflict with rank 9's exact predicates.

**Fix.**
- Describe it as a tolerance tie-break with a deterministic skew.
- Say it is not a model for an exact-predicate path.

### F9. minor: OpenNURBS Bernstein evaluation (§4.3). VERIFIED.

- OpenNURBS uses closed forms up to degree 4, a recursion for degrees 5-8, and `pow` with a binomial
  from degree 9.
- 66's "de Casteljau elsewhere" is wrong. Only the doc comment recommends de Casteljau.
- `pow` is a cross-target libm hazard, so this path must not be copied.

**Fix.** Correct the text and add the `pow` warning.

### F10. wrong: "the docs give no figures" for OCCT gluing (§11.3). VERIFIED.

The OCCT Boolean specification's Gluing section says the following:
- gains of 70% to 90%;
- "user should guarantee that the arguments are really coinciding";
- "The algorithm does not check this itself";
- an inappropriate option "is likely to lead to incorrect result";
- it "cannot be used on the shapes having real intersections";
- it has Shift and Full modes.

The Fuzzy section cites one case as 45x faster.

**Fix.**
- Quote the figures and the warning.
- 66's plan to verify coincidence ourselves (research 60, B1) stands. Note that the verification has
  a cost, which the 70-90% does not include. That cost is unmeasured.

### F11. overstated: N-ary union by composition (rank 8). VERIFIED.

- Manifold's `BatchUnion` (`csg_tree.cpp`) sorts by vertex count, with insertion order breaking ties.
  It then partitions greedily by first fit.
  - The result is deterministic for a given input order, but it depends on that order.
  - Boxes are compared with ±min(eps) slack, so touching boxes count as overlapping.
  - `kMaxUnionSize` is 1000.
- In a B-rep, composition makes a multi-lump body. If one box lies inside another, the boxes are not
  disjoint. A lump can sit inside another's cavity, so composition still needs a containment and
  nesting check that a mesh concatenation does not.
- Manifold's Arvo box transform is plain floating point (F4).

**Fix.**
- Keep rank 8 as a prototype.
- Specify the order (stable, by a body key) and the touching rule.
- Add the lump-nesting check to the brief.

### F12. overstated: "the pin's profile" is not the pin's. VERIFIED.

The 2026-10-27 profile is solidcore main dbaedda4a, a month before the pin (03e16c9, 2026-11-26).
It predates:
- the dense-Marks change, which alone took gear_60 from 758 to 612 ms (−19%);
- the seed-record and Marks fixes merged after it;
- the v1.0.1 mass changes.

Its file is not public, so readers of 66 cannot check its shares.

**Fix.**
- Re-profile gear_60 and plate_12 at the pin and at scx HEAD before ranking.
- Until then, label every percentage as "pre-pin, dbaedda4a".

### F13. overstated: rank 2's box prefilter is described as proven. VERIFIED/JUDGED.

The same benchmark notes say the `redistribute_inner_loops` prefilter is new compared with upstream
and "needs proving".
- It is exact only if the uv box certainly encloses the trimmed loop.
- That enclosure must include pcurve control-point rounding and any loop that crosses the period
  seam.

**Fix.**
- State the enclosure requirement.
- Add a seam-crossing and a near-touching case to the difftest list.

## 2. Licences

- **L1. wrong: OpenNURBS is labelled "permissive" without evidence.**
  - The repository's LICENSE at 2bd2e66 has an "All Rights Reserved" line, a warranty disclaimer
    and zlib's text. It has no explicit grant naming OpenNURBS.
  - GitHub reports the licence as NOASSERTION.
  - **Fix:** either confirm the grant from opennurbs.org and quote it in 66 and docs/06, or stop
    reading the code and treat OpenNURBS as documentation-only until that is settled.
- **L2. missing: arris ADRs can carry OCCT-derived ideas.**
  - arris's ADR README says that ADRs informed by OCCT, truck or Fornjot name the module read.
  - Several ADRs mention OCCT. ADR-0050 uses OCCT only as a test oracle.
  - Ideas are not code, but 66 should say which arris ADRs it used. Of those it cites, ADR-0052 and
    ADR-0090 do not name an OCCT module.
- **L3. minor: dependencies 66 implies.**
  - Rank 9 relies on the `robust` crate and Fornjot also uses `spade`. Both are permissive.
  - Neither has been checked for FMA use or for platform-dependent float paths. See M5.
- 66 shows no sign of having read OCCT, CGAL or SISL code. Its OCCT material matches the public
  specification.

## 3. Missing techniques

- **M1. Analytic point-in-face for analytic surfaces.** Planes, cylinders, cones, spheres and tori
  have closed-form inversion. On gear_60 this is the direct lever on the 40% (F1).
- **M2. A per-boolean face classifier cache.** OCCT's "Intersection Context is used to cache the
  tools" (from its specification). Per face, build the inversion setup, the uv loop boxes and the
  edge polylines once, then reuse them for every curve interval classified against that face. This
  is the per-solve fixed cost that `check_bad_guess_stride` shows.
- **M3. Incremental edit bookkeeping.**
  - `editing_stages` is 25.5% of gear_60: `delete_faces` is 16.8% and `make_faceuse` is 6.7%.
  - 12.4% is in `BTreeMap::find_key_index`.
  - Dense ids and slot maps belong in the ranking. 66 only mentions them in passing.
- **M4. Direct-topology primitives.** In plate_12, `create_cylinder` → `stitch_and_orient` is 13.2%.
  Primitives can be built with their topology known, without a stitch pass.
- **M5. A determinism audit of borrowed numerics.** Shewchuk-style predicates need exact two-product
  and two-sum. With FMA contraction off, the `robust` crate's plain operations are the right ones,
  but this must be checked on all three targets. `spade` is unaudited.
- **M6. Face-pair culling with a box tree over both operands.** OCCT's BOPDS_Iterator works this
  way. 66 has box transforms but no ranked item for pair culling.

## 4. Builder briefs

- **Rank 1:** do not task it as written. Replace it with M1 and M2 plus the F1 measurement.
- **Rank 3:** task it only with F3 (a) or the "truth-checked" label.
- **Rank 4 / NS2:** "measure, then hint", with the corrected buffer description (F7).
- **Rank 6:** restrict to the exact transform classes (F4).
- **Rank 7:** cite arris only and require independence from face order (F5).
- **All briefs:**
  - Use percentages from a pin or HEAD profile (F12).
  - Give before/after work counts, per our perf rule.

## 5. What survives

| Rank | Technique | Status after review |
|---|---|---|
| 1 | Region classification and flood | **Does not survive as stated.** The flood already exists in the pin. Retarget to point-in-face cost (F1, F2, M1, M2). |
| 2 | Exact box prefilter | Survives, with the enclosure requirement (F13). |
| 3 | Per-face memo | Survives only with a body-independent reference point, or as truth-checked (F3). |
| 4 | Stack scratch and span hint | Survives as a small change to measure first (F7). |
| 5 | G7K15 Gauss-Kronrod | Survives with hard-coded nodes, as truth-checked, with a measured ratio (F6). |
| 6 | Transform boxes | Survives only for exact transform classes. Refit otherwise (F4). |
| 7 | Edge samples as a function of the edge | Survives, sourced from arris only (F5). |
| 8 | N-ary union by composition | A prototype, with order and nesting caveats (F11). |
| 9 | Filtered exact predicates | Survives, after the M5 audit. |
| 10 | Interval pruning (libfive) | Survives as "later". |
