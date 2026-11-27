# 66 - Other kernels: how the fast ones get their speed, and what we can take

Status: **revised after the adversarial review (`reviews/review-66.md`, commit 030a26d); proposed
for review.** §0 lists every finding and what was done with it.

Scope: Brett's question of 11-27: "Don't forget about the other kernels we investigated, those are
useful to look at for how they achieve their performance. Being the fastest _correct_ kernel is our
goal, which may mean being slower than the fastest kernels, but we may be able to learn from them."

So this report asks two things of each kernel:

- how does it get its speed?
- which of its techniques can solidcorex adopt without giving up correctness or determinism under
  04 §1?

A technique that is fast only because it gives up correctness is rejected. It is still named, with
what it costs (§17).

60 covered these kernels for their design and contracts. 65 covered the published algorithms
against the pin. This report covers the kernels' performance devices and cross-references both by
row number (60's B and R rows, 65's ranked rows 1 to 14) and by 63's task ids (NS, CU, SU, SS, BR).

## Labels

- **READ**: from the kernel's source (permissive licences only, see the licence rule) or its
  published documentation. The repo path, or the page, is given.
- **PUBLISHED**: from a paper, thesis or vendor publication, cited in §3.
- **MEASURED**: a figure already in 05, or in the benchmark profile of 2026-10-27. **That profile is
  of solidcore main dbaedda4a, a month before the pin (03e16c9, 2026-11-26), and its file is not in
  this repository.** It predates the dense-Marks change (gear_60 758 → 612 ms, −19%), the seed-record
  and Marks fixes merged after it, and the v1.0.1 mass changes. Every percentage below is therefore
  marked **pre-pin** and is a lead, not a ranking input. Nothing was measured for this report.
- **INFERRED**: a judgement or estimate made here. Every gain below is INFERRED unless marked
  otherwise.

"Bit-identical" means the change leaves every result bit unchanged. "Truth-checked" means the result
may change, and the change must be shown equally or more correct against the truths (04 §1, truth
over pin).

**Licence rule.** Source was read only for MIT, Apache-2.0, 0BSD and zlib code, and for OpenNURBS
under the grant quoted below. OCCT (LGPL-2.1 with exception), CGAL (GPL/LGPL), SISL and GoTools
(AGPL) and libfive (MPL-2.0) were used through their documentation, papers and talks only. No line
of their code was read or paraphrased. Parasolid and ACIS are closed: published papers and vendor
documents only.

**OpenNURBS licence (review L1), checked 2026-11-27.**

- The repository's `LICENSE` at 2bd2e66 has a copyright line ("All Rights Reserved"), a warranty
  disclaimer and zlib's licence. It has **no grant** naming OpenNURBS. GitHub reports `NOASSERTION`.
- Each source file's header ends "For complete openNURBS copyright information see
  <http://www.opennurbs.org>". That address redirects to `https://www.rhino3d.com/opennurbs/`, whose
  "Legal Stuff" section reads: "Permission is hereby granted, free of charge, to any person
  obtaining a copy of the openNURBS SDKs and associated documentation files (the "Software"), to
  deal in the Software without restriction, including without limitation the rights to use, copy,
  modify, merge, publish, distribute, sublicense, and/or sell copies of the Software." The same page
  says "There are no restrictions. Neither copyright nor copyleft restrictions apply."
- **Verdict.** There is an explicit permissive grant, MIT-style without a notice condition, and the
  source files point to it. OpenNURBS therefore stays READ. Two caveats. The grant lives on a web page
  that can change, not in the repository, so it should be recorded with its date in
  `../06-licence-and-source.md` (not yet done). And nothing here copies OpenNURBS code: it is cited
  for techniques, several of which §4 and §6 now reject or narrow.

## 0. Response to the review

One line per finding. "Accepted" means the text below was changed to match. "Narrowed" means
accepted with a stated limit, where part of the finding or its fix is rejected and the reason is
given. No finding was rejected outright.

| Finding | Disposition | Where |
|---|---|---|
| F1 rank 1's 40% is point-in-face on the curve-trimming path, not region in/out | **Accepted.** Rank 1 retargeted to point-in-face cost: a uv-box reject plus analytic inversion. Step one is to measure why the pin's `drop_point_fast` misses the gear's cylinder | §1, §10.1 |
| F2 the flood as briefed is wrong for non-manifold, lamina, sheet and ON cases | **Accepted.** Restated as state per face use, seeded by local decisions at intersection edges, already present in the pin's merge; kept as a guard against regression, not a rank | §10.2 |
| F3 the per-face mass memo is not bit-identical | **Accepted, narrowed.** A body-independent reference makes memo reuse bit-identical to recomputing in solidcorex. It is still truth-checked once against exact volumes, because the per-face shift rounds and moves the last bits relative to the pin | §13.2 |
| F4 transformed boxes are not certified enclosures as cited | **Accepted.** Exact transport only for signed permutations, power-of-two uniform scale and translation, rounded outward; refit otherwise; Q4 answered | §11.4, §18 Q4 |
| F5 Fornjot's cache is not a pure function of the edge | **Accepted.** arris ADR-0052 is the only source; the order dependence is named as the thing to avoid | §12.1 |
| F6 Gauss-Kronrod is not adaptive Kronrod, and its nodes are computed with `cos` | **Accepted.** Description corrected; rule hard-coded as literals; ratio to be measured; truth-checked | §13.1 |
| F7 OpenNURBS buffers misdescribed; span hint small; NS2 already has degree-sized scratch | **Accepted.** Description corrected; rank 4 dropped out of the top ten as "measure, then hint" | §4.1, §7.1 |
| F8 Manifold's tie rule is a tolerance tie-break, not simulation of simplicity | **Accepted.** Recast; not a model for an exact-predicate path | §11.1 |
| F9 OpenNURBS Bernstein evaluation: recursion to degree 8, `pow` from 9 | **Accepted.** Corrected, with the `pow` warning | §4.3 |
| F10 OCCT gluing has figures and a warning | **Accepted.** Quoted; the cost of our verification is unmeasured | §11.3 |
| F11 n-ary composition: order, touching rule, lump nesting | **Accepted, narrowed.** Order and touching rule accepted. The nesting point is narrowed: with strictly disjoint boxes no lump can sit in another's cavity, so the check is needed only if composition is extended to operands whose boxes overlap but whose faces do not meet | §11.2, §18 Q7 |
| F12 the profile is solidcore main dbaedda4a, not the pin | **Accepted, narrowed.** Re-profiling on the pin is step 0. "Re-profile at scx HEAD" applies only to the layers solidcorex has ported (evaluation, box tree); it has no boolean yet, so boolean shares can only come from the pin until step 4 | Labels, §1 |
| F13 the box prefilter is not proven; it needs an enclosure | **Accepted.** Requirement stated; seam and near-touching difftest cases added | §10.3 |
| L1 OpenNURBS licence unproven | **Accepted.** Checked; an explicit grant exists on the page the source headers name; quoted above | Labels |
| L2 arris ADRs can carry OCCT-derived ideas | **Accepted.** ADRs used: 0052, 0030 and 0090. None names an OCCT, truck or Fornjot module. 0008 is mentioned only to say what it reads, not used | §3 |
| L3 `robust` and `spade` not audited for FMA or platform paths | **Accepted,** folded into M5 | §16 |
| M1 analytic point-in-face for analytic surfaces | **Accepted;** part of rank 1 | §10.1 |
| M2 per-boolean face classifier cache | **Accepted;** rank 2 | §10.1, §14 |
| M3 incremental edit bookkeeping (dense ids, slot maps) | **Accepted;** rank 3 | §14 |
| M4 direct-topology primitives | **Accepted;** rank 5 | §11.6 |
| M5 determinism audit of borrowed numerics | **Accepted;** a gate on ranks 8 and 10 | §16 |
| M6 face-pair culling with a box tree over both operands | **Accepted;** rank 7, after checking what the pin already does | §6.1 |

## 1. Summary

The fast kernels do not get their speed from clever arithmetic. They get it from **doing less
work**:

1. **Make each point query cheap, and ask fewer of them.** The pre-pin profile's largest boolean
   node, 40% of gear_60, is `classify_face_interval` → `point_3d_classify`: trimming SSI curves to
   faces, one point at a time. Under it, `global_point_solve` (a generic NURBS grid search, run on a
   cylinder) is 35.5%, `check_bad_guess_stride` 27.4% and `fails_eval` → `eval_second_derivatives`
   15.7% (MEASURED, pre-pin). The cost is per point solve, not per face. The levers are a certified
   uv-box reject before any solve, analytic inversion on analytic surfaces, and setup built once per
   face and reused for every interval on it (OCCT's intersection context). The pin already has an
   analytic inversion (`drop_point_fast`, `surface/surface/p2.rs`); why it does not take the gear's
   cylinder is the first thing to measure.
2. **Decide region membership locally, and carry it.** Manifold, truck and the pin all avoid a
   global point solve per face. They take each face use's state from local decisions at the
   intersection edges and carry it across uncut edges. In the pin this is the cellular merge
   (`boolean/merge/p1.rs`), 6.7% of gear_60 pre-pin. It is already in place, so the task is to keep
   it, not add it (§10.2).
3. **Make the decisions consistent by construction, so nothing needs repair or retry.** OCCT computes
   interferences in increasing dimension, V/V up to F/F, so that "redundant interferences" are
   skipped (READ, OCCT docs). 60 B2 already adopts the order. Manifold derives its edge-face
   decisions from one vertex-edge predicate, but its tie rule is a tolerance, which we do not copy
   (§11.1).
4. **Keep derived data, and move it only when moving is exact.** arris samples each edge once, as a
   pure function of the edge (ADR-0052). springrep carries a per-face memo through booleans that
   leave the face uncut; for us that is exact only once a face's integral no longer depends on the
   body's box (§13.2). Transforming a box tree instead of rebuilding it is exact only for a narrow set
   of transforms (§11.4).
5. **Cheap bookkeeping.** Pre-pin, `editing_stages` is 25.5% of gear_60 and 12.4% is in
   `BTreeMap::find_key_index`; deleted arena slots cost about 3× in walks on plate_12. Dense ids and
   live lists remove that (§14).

What the fast mesh kernels add on top (Manifold, truck's and verb's SSI by mesh) is **approximating
the geometry**: they trade exactness for speed. That trade is not open to us (§17). The fastest
exact kernels that are correct by our bar (CGAL's filtered exact predicates, Shewchuk's adaptive
predicates) show the other route: a cheap floating-point answer with a certified error bound, and
the slow exact path only when the bound fails.

### Step 0, before any rank is tasked

1. **Re-profile gear_60 and plate_12 on the pin (03e16c9).** Re-rank from those shares, and quote
   them in the briefs in place of the pre-pin figures. For the layers solidcorex has ported
   (evaluation, box tree), profile scx HEAD as well.
2. **Measure why `drop_point_fast` misses the gear's cylinder case.** Two candidate causes, both
   INFERRED: the cylinder reaches classification as a NURBS surface, not an analytic record, so
   dispatch falls to the generic solver; or `point_3d_classify` calls `global_point_solve` without
   trying the fast path. Count calls by surface kind and by path with side-channel instrumentation.

### Ranked table of adoptable techniques

Rank = expected gain × confidence, both INFERRED, from the pre-pin shares. Step 0 may reorder it.
"Gain" is on the cases where the technique applies. "Confidence" is that it applies cleanly under
04 §1.

| # | Technique | From | Maps to | Gain | Conf. | Verdict |
|---|---|---|---|---|---|---|
| 1 | Point-in-face on the curve-trimming path: a certified uv-box reject, then analytic inversion on analytic surfaces; reuse vertex-sector and edge-side answers along one curve | M1; Manifold `PointWinding` box reject; the pin's `drop_point_fast` | SS4; booleans step 4-5; pre-pin 40% of gear_60 | large where curved faces are cut | high once step 0.2 explains the miss | **adopt** (the reject is bit-identical if the box encloses; analytic inversion is truth-checked) |
| 2 | Per-boolean face classifier cache: inversion setup, uv loop boxes and edge polylines built once per face, reused for every interval on that face | OCCT intersection context (docs) | BR4 cells, or a cache owned by the boolean; `check_bad_guess_stride` 27.4% pre-pin | medium-large | high | **adopt** (bit-identical: same values, built once) |
| 3 | Dense ids and live-entity iteration in place of per-entity map lookups and slot walks | M3; Fornjot append-only store (ids); 51's per-face tables | the body tables (51); pre-pin 12.4% `BTreeMap::find_key_index`, 25.5% `editing_stages` | medium-large | high | **adopt** (entity order kept: bit-identical) |
| 4 | Exact uv-box prefilter in inner-loop redistribution and point classification | Manifold `PointWinding`; OCCT `BOPDS_Iterator` | BR4 uv boxes; pre-pin 11.4% of plate_12 in `redistribute_inner_loops` | medium-large on many-hole bodies | medium-high | **adopt**, only with a certified enclosure (§10.3) |
| 5 | Primitives built with their topology known, without a stitch pass | M4 | primitive constructors; pre-pin 13.2% of plate_12 in `create_cylinder` → `stitch_and_orient` | medium on primitive-heavy recipes | high | **adopt** (truth-checked: the same body, possibly different entity order) |
| 6 | Per-face mass memo carried through operations that keep a face, with a body-independent reference point and error scale | springrep `FaceMemo` (its `IntegralKey`) | BR3 `carry`, BR4 cells; pre-pin about 30% of plate_12 in mass properties | large on feature chains | medium | **adopt after the reference change** (truth-checked once; then reuse is bit-identical to recomputing) |
| 7 | Face-pair culling with one box tree over both operands' faces | M6; OCCT `BOPDS_Iterator` (docs) | booleans step 4; 65 row 3's tree | unknown until the pin's pair selection is read | medium | **check the pin first**, adopt if it lacks it |
| 8 | Edge samples as a pure function of the edge, computed once and shared by both faces | arris ADR-0052 | tessellation; BR4 edge cells | medium; watertight | high | **adopt**, after the M5 audit of any triangulation crate |
| 9 | Gauss-Kronrod for face integrals: one-shot G3K7/G7K15 acceptance per piece, adaptive Gauss halving otherwise, every node and weight a hard-coded literal | springrep `measure.rs`, `numeric.rs` | 65 row 8; mass properties | unmeasured (65 row 8 guesses 5-20×) | medium | **prototype** (truth-checked; measure evaluation counts on plate_12's faces first) |
| 10 | Filtered exact predicates (float with an error bound, exact fallback) for 2-D trim and tessellation decisions | CGAL Epick, Shewchuk, the `robust` crate | tessellation; pcurve loop tests | small speed; large robustness | high after M5 | **adopt** for orientation and in-circle tests, after the M5 audit |

Below the top ten:

- **Degree-sized scratch and a span hint (old rank 4).** NS2 already has degree-sized scratch, and
  the hint saves one or two compares on few-knot surfaces. Measure the point's fixed cost first,
  then try the hint, with a difftest on knot-tie inputs (§4.1, §7.1).
- **Transforming boxes instead of rebuilding them (old rank 6).** Exact only for signed
  permutations, power-of-two uniform scale and translation, rounded outward; refit otherwise
  (§11.4).
- **N-ary union by composition (old rank 8).** A prototype inside B4, with a fixed order and a
  strict touching rule; a lump-nesting check if composition goes past box-disjoint operands
  (§11.2).
- **Interval pruning (libfive).** Later, after 65 row 6's profile (§9.2).
- **OCCT-style gluing as a verified B1 fact.** Prototype; the cost of verifying is unmeasured
  (§11.3).
- **A Morton radix bulk build.** Reject for now; 65 row 3 is done and beats the pin (§6.2).
- **The local-seeded flood of §10.2.** Not a rank: the pin has it, and the port must keep it.

### What should change in builders' current briefs

- **All briefs.** Quote percentages only from a pin or scx HEAD profile, marked with the commit.
  Give before/after counts of the added and removed work.
- **Booleans and SS4 classification at a point.** Task ranks 1 and 2 after step 0. Do not task a
  "classify once per region" change: it already exists (§10.2).
- **NS2 surface point cost and grid eval.** Do not adopt "OpenNURBS's buffer" as a target: NS2
  already sizes its scratch by degree, and OpenNURBS's formula is not what the earlier draft gave
  (§4.1). Measure the fixed cost (span search, `Result` moves, the 256-byte `Basis`) first. A span
  hint follows only if span search shows, with a difftest that pins knot-tie inputs.
- **Curve inversion.** Warm starts and span hints are fine. Do **not** take a presearch grid plus
  Newton as the global step. That is truck's and verb's design, and it returns local minima (§7.2).
  The global step stays certified (65 row 7).
- **global_solve branch cost.** No change to the profile-first plan. Note libfive's interval
  pruning (§9.2) as the idea to test after Krawczyk at the leaves.
- **Closed-form cubic/quartic, elevation accuracy, curve restrict/reverse/transform.** No kernel
  read here does better than 65 rows 9 and 14. Do not copy OpenNURBS's Bernstein path: it uses
  `pow` from degree 9 (§4.3). For transform: move cached boxes only for the exact transform classes
  of §11.4; refit otherwise.
- **Surface enum.** Keep dispatch at batch granularity (one match per grid or batch, not per
  point). That is how OpenNURBS and Manifold keep their inner loops monomorphic. No other change.

## 2. What each kernel is, in one line

| Kernel | Kind | Speed comes from (INFERRED from what was read) |
|---|---|---|
| Manifold | triangle-mesh booleans, float | one shadow predicate behind every decision (with a tolerance tie rule), state flooded across unbroken mesh, a Morton-sorted radix BVH, TBB parallelism |
| truck | Rust NURBS B-rep | tessellate-then-refine SSI, status propagated from faces next to the intersection; little else tuned |
| Fornjot | Rust B-rep, shut down | "performance has not been a priority"; a curve approximation cache keyed by curve (but filled per surface, §12.1) |
| OpenNURBS | NURBS geometry library (Rhino's file format) | stack buffers for small orders, span hints, Bernstein closed forms, a packed R-tree |
| OCCT | full LGPL B-rep kernel | docs: dimension-ordered interferences, box pair selection, cached intersection context, parallel, fuzzy and gluing options |
| Parasolid, ACIS | commercial B-rep | published: local tolerances, multi-threaded operations and faceting |
| CGAL | exact computational geometry | filtered exact predicates, lazy exact constructions |
| SISL, GoTools | SINTEF spline libraries | published: recursive subdivision with simple-case tests; approximate implicitisation |
| verb, tinyspline | NURBS libraries | verb: lazy box trees, mesh-then-refine SSI. tinyspline: plain de Boor |
| libfive | implicit (f-rep) modeller | interval arithmetic with tape pruning, on CPU and GPU |
| springrep, arris, cadrs, geomsolver, anvil | small Rust kernels (60's table) | springrep: per-face memos keyed on an integral key, Gauss-Kronrod acceptance, banded LU. arris: per-edge sampling. cadrs: retry ladder and polytope fast path (rejected in 60) |

## 3. Sources

| Source | Licence | What was read |
|---|---|---|
| elalish/manifold @57cf361 | Apache-2.0 | `src/boolean3.cpp` (Shadow, Kernel11/02/12, Winding03), `src/collider.h`, `src/sort.cpp`, `src/impl.cpp`, `src/csg_tree.cpp`, README |
| ricosjp/truck @117470c | Apache-2.0 | `truck-geotrait/src/algo/curve.rs`, `truck-shapeops/src/transversal/` (intersection curves, `faces_classification`, `integrate`), `truck-meshalgo/src/analyzers/collision.rs` |
| hannobraun/fornjot @7e85668 | 0BSD | `crates/fj/src/core/storage/store.rs`, curve approximation cache, dependency list (`robust`, `spade`) |
| mcneel/opennurbs @2bd2e66 | the grant on rhino3d.com/opennurbs (quoted in Labels); zlib parts | `opennurbs_evaluate_nurbs.cpp`, `opennurbs_knot.cpp`, `opennurbs_rtree.h/.cpp` |
| pboyer/verb @f3ba23c | MIT | lazy bounding-box trees, surface-surface intersection, closest point |
| msteinbeck/tinyspline @bb3b02c | MIT | evaluation, `eval_all`, `sample`, `bisect` |
| springcraft-ai/springrep @f2ad60e | MIT/Apache-2.0 | `src/topo/memo.rs`, `src/numeric.rs`, `src/measure.rs` |
| arris-labs/arris @4408202 | MIT/Apache-2.0 | ADR-0052 (edge samples), ADR-0090 (radial pairing at a shared contact), ADR-0030 (cancellation). None of the three names an OCCT, truck or Fornjot module (L2). ADR-0008 says it reads OCCT's `IntAna_QuadQuadGeo`; it is mentioned in §8.2 for that reason only and nothing is taken from it |
| rvdende/cadrs, bmander/geomsolver, swperb/anvilcad | MIT/Apache-2.0, MIT | nothing new for performance beyond 60 |
| OCCT Boolean Operations specification (occt3d.com/dev/doc/overview/html/specification__boolean_operations.html) | docs | interference order, `BOPDS_Iterator`, intersection context, options (parallel, fuzzy, OBB, gluing, safe input) and their stated gains |
| OCCT BVH, IntPatch/IntWalk, BRepMesh, BRepClass3d | docs only | package roles as the user guides describe them; no code |
| Parasolid | vendor publications | Jackson 1995 (local tolerances); Siemens release material for v33.1 (concurrent faceting) |
| ACIS | vendor publications | Spatial's descriptions of tolerant modelling and multiprocessing |
| CGAL manual (Kernel, Number types, PMP corefinement) | docs only | Epick, Epeck, `Lazy_exact_nt`, filtered predicates |
| Shewchuk 1997 | paper | "Adaptive precision floating-point arithmetic and fast robust geometric predicates", DCG 18 |
| Brönnimann, Burnikel, Pion 2001 | paper | "Interval arithmetic yields efficient dynamic filters for computational geometry", DAM 109 |
| Devillers, Pion 2003 | paper | "Efficient exact geometric predicates for Delaunay triangulations", ALENEX |
| Attene 2020 | paper | "Indirect predicates for geometric constructions", CAD 126 |
| Cherchi, Pellacini, Attene, Livesu 2022 | paper | "Interactive and robust mesh booleans", ACM TOG 41(6) |
| Zhou, Grinspun, Zorin, Jacobson 2016 | paper | "Mesh arrangements for solid geometry", ACM TOG 35(4) |
| Karras 2012 | paper | "Maximizing parallelism in the construction of BVHs, octrees, and k-d trees", HPG |
| Keeter 2020 | paper | "Massively parallel rendering of complex closed-form implicit surfaces", ACM TOG 39(4); libfive docs |
| Sederberg, Meyers 1988; Sinha, Klassen, Wang 1985 | papers | loop detection by normal cones (SISL's documented subdivision tests) |
| Dokken 1997 (thesis), Dokken 2001 | papers | approximate implicitisation (GoTools) |
| Arvo 1990 | paper | "Transforming axis-aligned bounding boxes", Graphics Gems |
| Piessens et al. 1983 | book | QUADPACK (G7K15) |
| solidcore 03e16c9 (the pin) | Apache-2.0 | function names and locations cited in §1, §10 and §13 (`intersect/curve_class/p7.rs`, `boolean/merge/p1.rs`, `surface/surface/p2.rs`, `mass/integrator/properties.rs`) |

## 4. NURBS evaluation, basis functions, batch and grid evaluation

### 4.1 Stack scratch for small orders (OpenNURBS)

- **What.** Corrected after F7:
  - `ON_EvaluateNurbsBasis` keeps only the 2(order − 1) knot differences in its `stack_buffer[80]`.
    The order² basis triangle `N` is a buffer the caller provides.
  - `ON_EvaluateNurbsNonRationalSpan` puts the order² basis in `stack_buffer[64]`.
  - `ON_EvaluateNurbsSurfaceSpan` uses `stack_buffer[128]` for
    `(order0² + order1²)·8 + Pcount·cvdim·8` bytes, with `Pcount = (der+1)(der+2)/2`.
  - Each goes to the heap past its fixed size.
- **Evidence.** READ (`opennurbs_evaluate_nurbs.cpp`).
- **Correctness and determinism.** The arithmetic and its order are unchanged; only the storage
  moves. Bit-identical.
- **Maps to.** NS2 (point cost). 05 records that NS2 already has degree-sized basis scratch (jet<1>
  was fixed by the SMALL=8 scratch). The fixed costs 05 names, span search, `Result` moves and the
  256-byte `Basis`, are unmeasured.
- **Verdict.** Nothing new to adopt. Measure the fixed cost first; if `Basis` size shows, size it by
  the actual order.

### 4.2 End-knot snapping (OpenNURBS)

- **What.** At an end knot, basis values are snapped to exactly 1. In the order-2 span evaluator, the
  result is snapped to the control point when two control points are equal.
- **Evidence.** READ.
- **Correctness.** The snap returns the exact value where the recurrence rounds near it: equally or
  more correct. It changes bits, so it must be truth-checked.
- **Maps to.** NS2, CU2. It is accuracy, not speed, but cheap.
- **Verdict.** Prototype (truth-checked; probably already true of the scx recurrence at the ends;
  check).

### 4.3 Bernstein bases (OpenNURBS)

- **What.** Corrected after F9: `ON_EvaluateBernsteinBasis` has closed forms up to degree 4, a
  recursion for degrees 5 to 8, and from degree 9 a binomial times `pow`. Only its doc comment
  recommends de Casteljau, as faster and more stable than a Bernstein sum.
- **Evidence.** READ.
- **Correctness.** `pow` comes from the platform libm and differs across x86-64, aarch64 and wasm32.
  That path breaks our cross-target rule and must not be copied.
- **Maps to.** 65 row 11 (span-cached Bezier O(p) evaluation); 65 row 1.
- **Verdict.** No new action. 65's Bernstein-form choices stand.

### 4.4 Batch evaluation (tinyspline `eval_all`/`sample`, verb, truck)

- **What.** tinyspline's `sample` evaluates at uniform parameters, one de Boor net per point. verb and
  truck evaluate per point too. None of them shares the basis across a grid.
- **Evidence.** READ.
- **Verdict.** Nothing to take. 65 row 2 (basis per distinct u and v) is already better than any of
  them. Note for the Surface-enum builder: OpenNURBS and Manifold dispatch once per batch, not per
  point, so the inner loop is monomorphic.

## 5. Root finding and Bernstein solving

- **SISL** (PUBLISHED, its documentation and the papers it cites): recursive subdivision of the
  Bezier or B-spline form, with "simple case" tests (a hull test, then a normal-cone or tangent-cone
  test after Sederberg and Meyers 1988) that stop subdivision when at most one root or one curve
  branch can lie in the piece. This is the design of the pin's global solver and of 65 row 6.
- **OpenNURBS, truck, verb, tinyspline** (READ): Newton or bisection on one coordinate, started from
  samples. None certifies isolation.
- **Verdict.** Nothing beyond 65 rows 1, 6 and 13. The normal-cone stop is already in 65 §5.3 for SSI.

## 6. BVH build and query

### 6.1 Box pair selection

- **What.** OCCT selects candidate sub-shape pairs by bounding boxes (`BOPDS_Iterator`), with OBBs
  as an option. Manifold's collider is a radix BVH over Morton codes (Karras 2012) with stack DFS,
  segment queries and recorders for parallel collection. OpenNURBS uses a Guttman R-tree with at
  most 6 children per node, chosen for memory.
- **Evidence.** READ (Manifold `collider.h`, OpenNURBS `opennurbs_rtree.h`); READ docs (OCCT).
- **Maps to.** 65 row 3 (done: a `select_nth` median build, 9.0 µs / 141 µs / 2.09 ms at n = 100 /
  1,000 / 10,000 against the pin's 10.8 µs / 322 µs / 6.11 ms, MEASURED).
- **Face-pair culling (M6, rank 7).** One tree over both operands' faces, queried as a join, gives
  the candidate face pairs for SSI in one pass. This report did not establish how the pin selects its
  face pairs. Read that first; the rank is worth something only if the pin does more than a box
  join.

### 6.2 Morton radix bulk build

- **What.** OpenNURBS's `CreateMeshFaceTreePacked` builds bottom-up from Morton keys after an LSD
  radix sort of (key, index) pairs, O(n), and its comments say it gives "identical query results".
  Manifold builds a Karras radix tree. Equal codes are broken by index, so both are deterministic.
- **Evidence.** READ; PUBLISHED (Karras 2012).
- **Correctness.** Deterministic. Query results are the same set; the tree shape and therefore the
  visiting order differ, so anything that consumes pairs in visit order must sort them.
- **Gain.** A Morton build is O(n) and faster to build, but the trees are worse for queries than a
  median split (Karras 2012 trades quality for build parallelism). Our trees are small (hundreds to
  thousands of leaves) and queried more than built.
- **Verdict.** Reject for now. 65 row 3 is done and beats the pin. Revisit only if a profile shows
  tree builds dominating (for example, a tree per tessellation).

### 6.3 Lazy trees (verb)

- **What.** `LazySurfaceBoundingBoxTree` splits a surface only when a query descends into it.
- **Evidence.** READ.
- **Correctness.** Deterministic if the split rule is a pure function of the node. Under 04 the
  cached children must be built in a fixed order, not by whichever query arrives first (the bench
  notes found that pre-warming lazily refined caches changes results). A lazy tree whose children
  are the same whatever the order is fine.
- **Maps to.** BR4 cells (boundary trees); the global solver's leaf trees.
- **Verdict.** Prototype only if building full leaf trees shows in a profile. The node must be a
  pure function of its parent.

## 7. Inversion and projection

### 7.1 Span hint (OpenNURBS)

- **What.** `ON_NurbsSpanIndex(order, cv_count, knot, t, side, hint)` uses the last span found to
  narrow one side of the range, then runs a binary search (`ON_SearchMonotoneArray`).
- **Evidence.** READ (`opennurbs_knot.cpp`).
- **Gain.** Small: for the few-knot surfaces in the corpus it saves one or two compares (F7).
- **Correctness.** Bit-identical only if the hinted path applies the same rule at knot ties (our
  `Approach`) as the full search. A difftest must pin knot-tie inputs.
- **Maps to.** NS2 (span search is one of 05's named, unmeasured fixed costs); CU curve inversion;
  65 row 7 (warm starts).
- **Verdict.** Below the top ten: measure span search's share first. If adopted, pass the hint
  explicitly (no ambient state); never store it in the curve.

### 7.2 Presearch grid plus Newton (truck, verb)

- **What.** truck's `presearch` samples a uniform grid, takes the nearest sample, and runs Newton on
  C'·(C − P). verb samples `n·p` points, then runs Newton.
- **Evidence.** READ (`truck-geotrait/src/algo/curve.rs`; verb).
- **Correctness.** Fast, but it can converge to a local minimum when two parts of the curve come
  within a grid cell of the point. There is no certificate. Not correct by our bar as the only
  global step.
- **Verdict.** Reject as a global method (§17). Keep it as a warm start inside the certified
  best-first search of 65 row 7.

### 7.3 Analytic inversion (M1)

- **What.** Planes, cylinders, cones, spheres and tori have closed-form inversion: a point's (u, v)
  comes from a projection and an `atan2`, with no search.
- **Evidence.** The pin has it: `drop_point_fast` on the plane, the surface of extrusion and the
  surface of revolution, with a dispatcher in `surface/surface/p2.rs` (READ). On gear_60, pre-pin,
  the generic `global_point_solve` still ran on a cylinder (MEASURED, pre-pin).
- **Correctness.** Deterministic with our own `atan2` (`fmath`). It returns different bits from a
  converged grid search, so it is truth-checked against the closed-form references in `scx-testkit`
  (59).
- **Verdict.** Adopt as part of rank 1. Step 0.2 says why the pin's path is missed; the answer
  decides whether the fix is in records (keep the cylinder analytic) or in dispatch.

## 8. SSI (marching, subdivision)

### 8.1 Mesh-then-refine (truck, verb)

- **What.** truck intersects the two tessellated meshes (sweep and prune of triangle intervals on an
  axis), joins the segments into polylines, then refines them onto both surfaces. verb does the
  same.
- **Evidence.** READ (`truck-shapeops/src/transversal`, `truck-meshalgo/src/analyzers/collision.rs`).
- **Correctness.** Loops smaller than the mesh are missed, and the result depends on the mesh
  resolution. truck's sort uses `partial_cmp(...).unwrap_or(Greater)`, which is not a total order
  if a NaN appears. Rejected (§17).
- **Maps to.** 65 row 5; 60 R-7 (springrep's mesh broad phase, also rejected).

### 8.2 OCCT IntPatch / IntWalk

- **What.** The docs describe analytic intersection for quadric pairs (IntAna), and a walking
  (marching) algorithm for general surfaces started from seed points, with the line then
  approximated.
- **Evidence.** READ docs only.
- **Verdict.** Nothing beyond 65 row 5. arris ADR-0008 notes it reads `IntAna_QuadQuadGeo` for its
  pairs; we do not read OCCT code, and take nothing from that ADR.

### 8.3 Subdivision with loop detection (SISL)

- **What.** Subdivide both surfaces until a normal-cone test (Sederberg and Meyers 1988; Sinha et
  al. 1985) proves no closed loop is inside a piece, then march.
- **Evidence.** PUBLISHED.
- **Correctness.** It is the certified way to find every loop. It is the shape 65 row 5 already
  recommends.
- **Verdict.** Already in 65 row 5; no change.

### 8.4 Implicitisation (GoTools)

- **What.** Dokken's approximate implicitisation replaces one surface by a low-degree algebraic
  approximation, so SSI becomes the root finding of a function on the other surface's domain. It is
  used to find and separate branches quickly.
- **Evidence.** PUBLISHED (Dokken 1997, 2001).
- **Correctness.** The approximation has a bounded error, so it can only cull or seed. The final
  points must be refined on the exact surfaces.
- **Verdict.** Prototype only for seeding tangential and near-tangential cases in step 5. Low
  priority.

## 9. Interval and certified methods

### 9.1 CGAL filtered predicates

- **What.** A predicate is evaluated in floating point with interval arithmetic or a static error
  bound (Brönnimann et al. 2001; Devillers and Pion 2003). If the sign is certain, it is returned;
  otherwise the predicate is re-evaluated exactly. Epick does this for predicates over exact input
  points; Epeck adds lazy exact constructions (`Lazy_exact_nt`: an interval approximation with the
  expression DAG kept until an exact value is needed).
- **Evidence.** PUBLISHED; READ docs (CGAL manual).
- **Correctness.** Exact signs for given float inputs: deterministic and correct. The filter succeeds
  nearly always, so the cost is close to plain floats.
- **Verdict.** See §16.

### 9.2 Interval tape pruning (libfive)

- **What.** libfive evaluates the implicit function over a box with interval arithmetic. Where a
  `min` or `max` has one argument provably dominant over the box, the other branch is pruned from the
  expression tape for every sub-box below (Keeter 2020). The work shrinks as the subdivision goes
  deeper.
- **Evidence.** PUBLISHED (Keeter 2020; libfive docs). MPL-2.0: code not read.
- **Correctness.** Intervals are enclosures, so pruning only discards what cannot matter. It is
  deterministic if the interval arithmetic rounds outward the same way everywhere (our `fmath`,
  no FMA contraction).
- **Maps to.** 65 row 6 (global solver) and row 10 (enclosures). The analogue for us: at a node of
  the branch-and-bound, terms of the objective whose bound is dominated are dropped for the subtree,
  and patches whose bounds cannot beat the current best are dropped from the candidate list (this
  last part is ordinary branch and bound, which we have).
- **Verdict.** Prototype later, after 65 row 6's profile and Krawczyk at the leaves. Gain uncertain:
  our objectives are small (distance squared), so there is little tape to prune. Most of libfive's
  gain comes from CSG trees with thousands of nodes.

## 10. Point classification and ray casting

### 10.1 Point-in-face on the curve-trimming path (rank 1, rank 2)

- **What the cost is.** Corrected after F1. In the pin, `classify_face_interval`
  (`intersect/curve_class/p7.rs`) classifies one interval of an intersection curve, known to lie on
  a face's surface, against the face's trim. It uses the vertex sector or the edge side where the
  interval's ends are already classified, and otherwise a point near the middle of the interval,
  which goes through `point_3d_classify` to a point solve. Pre-pin, that chain is 40% of gear_60,
  with `global_point_solve` 35.5%, `check_bad_guess_stride` 27.4% and `fails_eval` →
  `eval_second_derivatives` 15.7% under it (MEASURED, pre-pin). The pin's own notes call the cost
  "not an easy win" and say `fails_eval` is faithful to upstream.
- **Levers, in order.**
  1. **A certified uv-box reject.** If the interval's point projects outside every uv box of the
     face's loops, with the box a certified enclosure (§10.3), the point is outside the face with no
     solve. Bit-identical where it applies.
  2. **Analytic inversion** on analytic surfaces (§7.3, M1). Truth-checked.
  3. **Reuse along one curve.** Consecutive intervals of one curve on one face share ends. The
     vertex-sector and edge-side answers at a shared end are decided once and read by both
     intervals.
  4. **A per-face classifier cache (M2, rank 2).** OCCT's intersection context "is used to cache the
     tools to increase the algorithm performance" (READ docs). Per face, build the inversion setup,
     the uv loop boxes and the edge polylines once per boolean, and reuse them for every interval.
     That is the per-solve fixed cost that `check_bad_guess_stride` shows. Kept in a cache owned by
     the boolean, or as BR4 cells; never ambient.
- **First step.** Step 0.2: measure why `drop_point_fast` does not take the gear's cylinder.
- **Verdict.** Adopt, after step 0.

### 10.2 Face-use state from local decisions, carried across uncut edges (no rank: keep it)

- **What the kernels do.** Corrected after F2:
  - truck's `integrate_by_component` (`faces_classification/mod.rs`) seeds from faces next to the
    intersection that already have And/Or status, and floods only the unknown components. It checks
    only `boundary[0][0]` of a component, and falls back per face to a ray cast against a
    tessellated shell with a hashed pseudo-random direction. It is a weak model.
  - Manifold's `Winding03` floods winding numbers across the edges an intersection does not break.
  - The pin's cellular merge (`boolean/merge/p1.rs`, `find_brep_region_of_other_face`) takes a
    single candidate region at once where there is one, otherwise reads radial sectors at mated
    edges (`edge::find_radial_sector_between`), and only for unmated edges falls back to a point
    classification at a fixed edge parameter. `propagate_other_src_region_attribs` then carries the
    result per region. All of `make_faceuse` is 6.7% of gear_60 (MEASURED, pre-pin).
- **The rule, stated correctly.**
  - **State is per face use and per cell, not per face.** A face of A can bound two cells, and the
    answer is "which cell of B", not in or out.
  - **Components are of face uses**, split at every intersection edge and at every non-manifold
    edge. Across an edge with more than two uses, pair the uses radially across material (arris
    ADR-0090) before flooding; flooding without that pairing joins distinct cells.
  - **Seeds are local**: the radial sector or side of the curve at a split edge. A point
    classification is the fallback only for components that touch no split edge, and its point is
    fixed by the inputs (not by a union-find root).
  - **ON faces**, coincident with B's boundary, get their own decision (same or opposite normal),
    and the flood does not cross from them.
  - **Sheets** have no inside. Against a sheet operand there is no in/out to flood; state is side
    of the sheet only where the sheet is part of a closed cell. A lamina face does not separate
    cells, so a lamina and the volume on both sides share a state.
  - The rule needs a complete intersection graph. An edge missed by SSI merges two components; per
    face classification would then be wrong on part of a face instead.
- **Verdict.** Already the shape of the pin's merge. The port must keep it and must not regress it
  to per-face point classification. Not a ranked gain.

### 10.3 Exact box prefilter (rank 4)

- **What.** Manifold's `PointWinding` rejects faces by bounding box before any winding test. OCCT
  selects pairs by boxes before intersecting them.
- **Evidence.** READ.
- **Correctness.** Corrected after F13. The pin's `redistribute_inner_loops` classifies every inner
  loop of the target face against each new edge or loop (11.4% of plate_12, MEASURED, pre-pin). The
  bench notes say the prefilter tried there is new compared with upstream and "needs proving". It is
  exact only if the uv box certainly encloses the trimmed loop, which means:
  - the box covers the pcurve's control points with their rounding (outward-rounded), not its
    sampled points;
  - a loop that crosses a periodic seam gets a box on the unwrapped parameter, or two boxes, never
    a box from the wrapped minimum and maximum.
- **Difftest cases to add.** A loop that crosses the seam of a cylinder; a hole whose box touches
  the new loop's box to the last bit; a loop near-touching but outside.
- **Maps to.** BR4 (uv and face boxes); booleans.
- **Verdict.** Adopt with the enclosure. Results are unchanged when the prefilter is exact; the
  difftest checks it.

### 10.4 Axis-aligned rays (Manifold)

- **What.** Every Manifold query is a vertical (z) ray, so each test is a 2-D interpolation.
- **Evidence.** READ.
- **Correctness.** For curved B-rep faces an axis ray hits tangencies and seams as often as any ray,
  and the cheaper test only applies to planar faces.
- **Verdict.** Reject for the general path. For planar faces the ray-polygon test is already cheap.

## 11. Booleans (B-rep and mesh)

### 11.1 Lower dimension first (OCCT); Manifold's tie rule (not adopted)

- **What.** OCCT computes interferences in the order V/V, V/E, E/E, V/F, E/F, F/F, "avoiding the
  computation of redundant interferences" (READ docs). In Manifold, every decision derives from one
  vertex-edge "shadow" predicate, so the edge-edge (Kernel11), vertex-face (Kernel02) and edge-face
  (Kernel12) decisions cannot disagree.
- **Manifold's tie rule, corrected after F8.** `Shadows(p, q, movedP, movedQ, tol)` is
  `|p − q| > tol ? p < q : movedP − p < movedQ − q`, with `tol` the `TieTol`. `Moved::P` displaces
  each vertex by `(expandP ? 1.125 : −1.125)·tol/4` along its displacement, plus a fixed skew step of
  `tol/4·(1/61, 1/89, 1/113)`. `expandP` is true only for `Add`, so intersection contracts P as well
  as difference does. This is a **tolerance tie-break with a deterministic skew**: values within
  `tol` count as equal, and that relation is not transitive. It is not simulation of simplicity.
- **Evidence.** READ (Manifold `boolean3.cpp`); READ docs (OCCT).
- **Correctness.** The one-predicate structure is good: consistent decisions. The tolerance rule
  under it decides touching cases by convention, which a B-rep must not do (contacts are kept, 60
  R-8), and inside tessellation it would conflict with rank 10's exact predicates.
- **Verdict.** The dimension order is already 60 B2. Manifold's tie rule is rejected everywhere,
  including tessellation; exact predicates take that role (§16).

### 11.2 N-ary union by composition (Manifold; below the top ten)

- **What.** Corrected after F11. Manifold's `BatchUnion` (`csg_tree.cpp`) sorts children by vertex
  count, insertion order breaking ties, then partitions them greedily by first fit into sets whose
  boxes are pairwise disjoint, and "composes" each set: concatenates without a boolean. Boxes are
  compared with ±min(eps) slack, so touching boxes count as overlapping. `kMaxUnionSize` is 1000.
  The sets are then booleaned smallest first.
- **Evidence.** READ.
- **Correctness.** The result is deterministic for a given input order but depends on that order.
  In a B-rep, composition makes a multi-lump body. With strictly disjoint, outward-rounded boxes, no
  lump can lie in another's cavity, since a nested lump's box lies inside the other's. The review
  (F11) asks for a nesting check; it is needed only if composition is extended to operands whose
  boxes overlap but whose faces do not meet (a body placed in another's void), which is where B4
  finds no face pairs (Q7). Manifold's Arvo box transform is plain floating point (§11.4).
- **Maps to.** 60 B4 (n-ary as one decomposition). For patterns the pin's cost per cut grows with
  the body (plate 3.1 → 5.9 ms per hole; gear 5.7 → 13.6 ms per cut, MEASURED, pre-pin). A cut by many
  mutually disjoint tools in one decomposition pays the body's whole-B-rep passes once.
- **Verdict.** Prototype, inside B4, with: a stable order by a body key, not by vertex count alone;
  boxes rounded outward and touching boxes treated as overlapping; and a lump-nesting check if, and
  only if, composition is extended past box-disjoint operands.

### 11.3 Gluing for coinciding arguments (OCCT)

- **What.** Corrected after F10. The OCCT Boolean specification's Gluing section says the option
  speeds up the operation "in some case can go up to 90%", and gives two cases of "about 70
  percent". It warns: "user should guarantee that the arguments are really coinciding. The algorithm
  does not check this itself"; "Setting inappropriate option for the operation is likely to lead to
  incorrect result"; it "cannot be used on the shapes having real intersections, like intersection
  vertex between edges". Modes: `BOPAlgo_GlueOff`, `BOPAlgo_GlueShift`, `BOPAlgo_GlueFull`. The
  Fuzzy section reports one case 45 times faster.
- **Evidence.** READ docs.
- **Correctness.** Correct only when the caller's claim is true. OCCT trusts the claim.
- **Maps to.** 60 B1 (caller-stated facts, verified and refused if false).
- **Verdict.** Prototype as a B1 fact: the caller states the shared faces, the kernel verifies the
  claim (the same surface record and overlapping trims) and refuses if false. The 70-90% does not
  include that verification, whose cost is unmeasured; the gain is real only if verifying is cheaper
  than intersecting.

### 11.4 Transforming boxes, and lazy transforms (Manifold, OpenNURBS; below the top ten)

- **What.** Corrected after F4.
  - Manifold's `collider.h` `Transform` asserts `IsAxisAligned`; `impl.cpp` transforms the collider
    only for axis-aligned transforms and otherwise calls `UpdateBoxes`, which refits. Manifold does
    not move boxes through rotations.
  - OpenNURBS (`opennurbs_rtree.h`) transforms a box by the |L|·halfwidth rule, which its comment
    calls a "conservative enclosure". Under a rotation it inflates every box, and repeated rotation
    compounds the inflation: the gear's 60 rotated tools are that case.
  - Neither rounds outward, so a box transformed in floating point does not certainly enclose the
    rounded transformed control points.
  - Manifold keeps a transform pending on an operand and tests overlap with Arvo's transformed box.
- **Correctness.** A tree kept through a transform is not the tree a rebuild would produce, so query
  visit order differs; anything that consumes results in visit order is not bit-identical to a
  rebuild. Applying a similarity to geometry once, at the end of a chain, instead of at each step,
  is one rounding instead of several: more correct, and truth-checked.
- **Verdict.**
  - Move boxes exactly only for signed-permutation, power-of-two uniform scale and translation
    transforms, rounded outward.
  - For general rigid motions, refit leaves from the transformed geometry, and keep the tree's
    topology only where the query result does not depend on visit order.
  - Lazy transforms: prototype with that rule, truth-checked.

### 11.5 Mesh booleans with exact predicates (Zhou 2016, Cherchi 2022)

- **What.** Zhou et al. build the full mesh arrangement with exact predicates and rational
  constructions. Cherchi et al. reach interactive speed with indirect predicates (Attene 2020):
  intersection points are represented implicitly by the input primitives that define them, and
  predicates on them are filtered then exact.
- **Evidence.** PUBLISHED.
- **Correctness.** Exact for meshes. They show that exact and fast is possible when the predicates
  are filtered.
- **Maps to.** Only mesh paths (tessellation, STL import repair). Not our B-rep boolean.
- **Verdict.** Note. If we build a mesh boolean, start from Cherchi 2022, not Manifold.

### 11.6 Primitives with their topology known (M4, rank 5)

- **What.** Pre-pin, `create_cylinder` → `stitch_and_orient` is 13.2% of plate_12 (MEASURED,
  pre-pin): a primitive's faces are built loose, then stitched and oriented. A primitive's topology
  is known when it is made (a cylinder has three faces, two circles and a seam), so it can be built
  connected and oriented, with no stitch.
- **Correctness.** The same body; the entity order may differ from the pin's. Truth-checked by the
  validity check and by exact volumes.
- **Verdict.** Adopt.

## 12. Tessellation

### 12.1 One sampling per edge (arris)

- **What.** arris ADR-0052, "An edge's samples are a pure function of the edge": a coedge's
  requirement is read along its own pcurve's (u, v) box, not its face's box, and the edge takes the
  largest count any coedge asks for. A kept face's mesh then meets its re-meshed neighbours bit for
  bit, and a consumer can cache meshes per face across edits.
- **Evidence.** READ.
- **The thing to avoid** (F5). Fornjot's `CurveApproxCache` is keyed on `(Handle<Curve>,
  CurveBoundary)`, but `approx_curve_with_cache` takes a surface and samples on `local_on(surface)`.
  The first face to ask fills the cache, and the second reuses samples sized for a different
  surface, so the result depends on face order. That is not cited as a source.
- **Cost.** For a NURBS surface, ADR-0052 takes curvature over the whole surface domain, so that the
  count depends on the surface alone. That is conservative and costs extra samples on small faces of
  large surfaces.
- **Correctness.** Watertight by construction, deterministic, and it avoids half the edge sampling.
- **Maps to.** Tessellation; BR4 cells (an edge cell).
- **Verdict.** Adopt, with the count a function of the edge alone and independent of face order.

### 12.2 Parallel faceting (Parasolid, OCCT BRepMesh)

- **What.** Parasolid v33.1 made faceting concurrent (Siemens release material). OCCT's BRepMesh
  has a parallel mode over faces (docs).
- **Evidence.** PUBLISHED (vendor); READ docs.
- **Verdict.** See §15. Faces are independent once edges are sampled (12.1), so it parallelises
  cleanly.

### 12.3 Delaunay by a library (truck, Fornjot use spade)

- **What.** Both triangulate with `spade` (constrained Delaunay, MIT/Apache) and its robust
  predicates.
- **Evidence.** READ (dependency lists).
- **Verdict.** A candidate dependency for tessellation (popular, permissive), only after the M5
  audit (§16).

## 13. Mass properties

### 13.1 Gauss-Kronrod (springrep; rank 9)

- **What.** Corrected after F6. springrep's `integrate_pieces` (`measure.rs`) runs, per piece, a
  one-shot acceptance test with the G3K7 pair (to rough tolerances) or the G7K15 pair
  (`kronrod_k`). If neither accepts, it falls back to adaptive Gauss halving (`adapt_k`). It is not
  adaptive Kronrod. Its Gauss nodes come from `legendre_rule` (`numeric.rs`), computed at run time
  with `cos` and Newton steps; only the Kronrod constants are hard-coded.
- **Evidence.** READ; PUBLISHED (Piessens et al. 1983).
- **Correctness.** `cos` comes from libm and differs across x86-64, aarch64 and wasm32, so the nodes
  as springrep computes them are not cross-target. For us every node and weight is a hard-coded
  exact literal. A different rule changes the last bits of every integral: truth-checked against
  analytic volumes (plate_12's is 100000 − 5760π).
- **Baseline.** The pin already uses G3K7 in places (`mass/integrator/properties.rs`, cited in 65), so
  the comparison is not with plain Gauss everywhere.
- **Gain.** Unmeasured. The earlier draft's "2-3 orders fewer evaluations" contradicted 65 row 8's
  5-20×, and neither was measured on our integrand. Pre-pin, mass properties are about 30% of
  plate_12, with 14.7M surface-derivative evaluations for nine primitives at about 190 ns each
  (MEASURED, pre-pin).
- **Verdict.** Prototype (65 row 8). First count evaluations per face on plate_12 with the pin's
  rule and with the candidate.

### 13.2 Per-face memo carried through operations (springrep; rank 6)

- **What.** springrep's `FaceMemo` (`src/topo/memo.rs`) holds what is derived from one face's
  geometry: volume share, integrals and moments, pcurve parameter tables, loose box, and which other
  faces are known apart. "A boolean that passes a face through uncut gives the result's face the same
  memo, so a part built by many features measures and samples each face once rather than once a
  feature." The integrals and moments are keyed on an `IntegralKey`: centre, size, tolerance and
  sense.
- **Evidence.** READ.
- **Correctness.** Corrected after F3. In the pin a face's contribution is not independent of the
  body: `brep_volume` integrates about the body box's midpoint (`bbox.mid()`), and
  `boundary_properties` sets its error scale from `face_box.min_distance(origin)` and the estimated
  area (`mass/integrator/properties.rs`). A boolean that removes material moves the box, which
  changes every face's contribution and subdivision depth, even for faces it does not touch. A face
  that looks uncut can also gain inner loops, have its edges split by imprinting, or reverse sense
  as a face use of B in a difference. springrep's `IntegralKey` exists for exactly this.
  springrep's global `AtomicU64` ids and `OnceLock`/`Mutex` fills are ambient state and are not
  taken.
- **What makes it exact.** Integrate each face about a reference fixed by the face itself (its own
  box centre), with an error scale from the face alone, and move each face's results to the body's
  reference by a shift (it needs the face's vector area and first moments, memoized with it),
  summed in a fixed order. Key the memo on the face's surface
  record, its loops, its sense and the accuracy. Then reusing a memo gives the same bits as
  recomputing it in solidcorex.
- **Narrowing (F3).** The shift itself rounds, so the body's totals differ from the pin's in the
  last bits. The change is truth-checked once against exact volumes, then bit-identical between
  memo and recompute from there on.
- **Maps to.** BR3 `carry` (the record of what an operation kept) and BR4 cells, or a cache owned by
  the caller. No global ids.
- **Verdict.** Adopt after the reference change. Gain: the plate pays mass properties for the new
  and changed faces per cut, not all faces.

## 14. Memory layout and caching

- **Dense ids and live iteration (M3, rank 3).** Pre-pin, `editing_stages` is 25.5% of gear_60
  (`delete_faces` 16.8%, `make_faceuse` 6.7%), 12.4% is in `BTreeMap::find_key_index`, and deleted
  slots cost about 3× in arena walks on plate_12 (438 arena faces, 150 live) (MEASURED, pre-pin).
  Fornjot's append-only `store.rs` keeps ids stable (READ). For us: ids dense per body table (51),
  lookups by index rather than by map, and iteration over live entities (a live list or a bitset),
  not slots. Entity order is kept, so it is bit-identical.
- **Morton-ordered vertices and faces (Manifold `sort.cpp`).** Vertices and faces are sorted by
  Morton code for locality; equal codes break by index (READ). Changes entity order, so it is not
  bit-identical for anything that iterates. Reject for B-reps: our entity order is part of the
  result, and our sizes are small.
- **Fast hash maps (truck: FxHashMap).** Faster hashing (READ). Deterministic as long as nothing
  iterates the map in hash order. Adopt only with that rule; prefer dense arrays indexed by id.
- **Lane-wise SoA.** None of the kernels read here uses SoA in its NURBS evaluation. 65 §3.1 stands:
  lane-wise over x/y/z/w is bit-exact, across the summation index is not.
- **Intersection context cache (OCCT; M2, rank 2).** §10.1 lever 4.

## 15. Parallelism (noted only)

- **Manifold**: TBB with a serial fallback for small sizes; collider recorders for parallel
  collection; the WASM build is serial (READ).
- **OCCT**: a parallel option for booleans and meshing (READ docs).
- **Parasolid**: concurrent faceting from v33.1 (PUBLISHED, vendor). **ACIS**: multiprocessing of
  operations (PUBLISHED, vendor).
- **verb**: a worker pool (READ).
- **arris**: ADR-0030 cancellation polls from parallel passes; 60 records its metered parallel
  passes.

Our rule, from the bench notes: compute in parallel and commit serially in index order. Reductions
must sum in a fixed order (a fixed tree over index ranges, not by thread). Per-face tessellation
and per-face mass properties fit this exactly, and so does classifying the intervals of different
faces once each face's classifier cache (rank 2) is built. Pre-warming lazy caches in parallel
changes results and is out (§6.3).

## 16. Exact and adaptive predicates (rank 10), and the audit of borrowed numerics (M5)

- **What.** Shewchuk's adaptive predicates evaluate orientation and in-circle tests in floats with
  an error bound, and extend the precision in stages only as needed. CGAL's filters do the same with
  interval arithmetic or static bounds. Fornjot uses the `robust` crate (Shewchuk's predicates,
  MIT/Apache).
- **Evidence.** PUBLISHED (Shewchuk 1997; Brönnimann et al. 2001; Devillers and Pion 2003); READ
  (Fornjot dependencies).
- **Correctness.** Exact signs, deterministic on every target provided the float operations are
  IEEE with no FMA contraction. Shewchuk's expansions rely on exact two-sum and two-product built
  from ordinary operations; Rust does not contract on its own, so plain operations are the right
  ones. Faster than our current alternative in hard cases, since the alternative is a tolerance and a
  retry.
- **The M5 audit, a gate on ranks 8 and 10.** Neither `robust` nor `spade` has been checked. Before
  either is a dependency: read it for `mul_add`, `fma`, SIMD intrinsics, target-dependent `cfg`
  float paths and libm calls; run its predicates on the three targets against an exact oracle
  (`scx-testkit`'s big integers) on seeded near-degenerate inputs. If either fails, own the ~200
  lines in `fmath` style.
- **Maps to.** Tessellation (constrained triangulation); 2-D trim loop tests (point in polygon of a
  pcurve polyline, loop orientation); SS4's planar cases.
- **Verdict.** Adopt for 2-D and 3-D orientation and in-circle predicates in tessellation and trim
  tests, after the audit. They do not replace tolerance-based decisions on curved geometry.

## 17. Fast, but not correct by our bar

| Technique | Who | Why it is fast | What it costs us |
|---|---|---|---|
| SSI by intersecting tessellations, then refining | truck, verb | one mesh pass instead of subdivision | misses loops smaller than the mesh; resolution-dependent results |
| Inversion by a presearch grid plus Newton, no certificate | truck, verb | a fixed small number of samples | local minima returned as answers |
| Mesh approximation of curved geometry | Manifold | float triangle booleans are simple and parallel | exactness of curved faces is gone; not a B-rep |
| Tolerance tie-break with a skew | Manifold | every touching case decided by one rule | non-transitive "equal"; touching contacts decided by convention |
| Fuzzy values and growing tolerances | OCCT (fuzzy option), Parasolid and ACIS (local tolerances, Jackson 1995) | operations succeed on near-coincident input without exact handling | results depend on a tolerance the caller chose; tolerance grows through a model. Our tolerances are fixed per document (one resolution) |
| Trusted gluing claims | OCCT (gluing) | skips face-face intersection | "likely to lead to incorrect result" when the claim is false; we verify instead |
| Tolerance retry ladders | cadrs (60 R-1) | a failure becomes a retry | hides bugs; results depend on which rung succeeded |
| Snapping near-coincident geometry | springrep, cadrs (60 R-2) | removes near-degenerate cases | moves valid geometry by 1e-6 to 1e-4 mm |
| Thread-local or global caches | cadrs (60 R-3), springrep (global memo ids) | no plumbing | ambient state; results may depend on history |
| Caches filled by whichever caller asks first, sized for that caller | Fornjot curve approximation | one approximation per curve | results depend on face order |
| Parallel reductions in scheduler order; wasm relaxed-SIMD FMA | general | full use of cores and SIMD | results differ run to run and across targets |
| SIMD across the summation index | general | wider vectors | changes the summation order, so bits differ by target |
| libm `pow` or `cos` in a rule's constants | OpenNURBS Bernstein from degree 9, springrep Gauss nodes | short code | bits differ by target |
| Power-basis Horner at high degree | general | fewer operations than de Casteljau | unstable at degree 10 and above |
| Per-span cached reciprocals | general | one division per span | changes bits; allowed only with a truth check |
| Lazy caches refined by whichever query arrives first | verb-style, in parallel | work on demand | results depend on query order |
| Sorting with `partial_cmp().unwrap_or(...)` | truck | no NaN handling | not a total order if a NaN appears; order and results undefined |
| Morton reordering of entities | Manifold | memory locality | changes entity order, which is part of our result |

## 18. Open questions

1. **Why the analytic path misses.** Step 0.2. Does the gear's cylinder reach classification as a
   NURBS surface, or does `point_3d_classify` skip `drop_point_fast`? If records lose the analytic
   form, the fix belongs with exact records (the plane-trim carry is held for the same reason).
2. **Flood fallback point.** For a component of face uses that touches no split edge (§10.2), which
   interior point does the fallback classification use, so that it is fixed by the inputs and far
   from the component's edges? A face's existing interior point, from the face with the lowest id,
   is the simplest candidate.
3. **Memo cells and `carry`.** Does BR3's `carry` record enough to move every cell of a kept face
   (boxes, mass-property terms, edge samples) without recomputing? Which operations count as
   "keeping" a face when its loops change but its surface does not (the mass-property terms then
   change; the surface box does not)?
4. **Transform of cells.** Answered (F4): a moved box is a certified enclosure only for signed
   permutations, power-of-two uniform scale and translation, rounded outward. General rigid motions
   refit; projective transforms never move boxes.
5. **Predicate crate.** Settled by the M5 audit (§16).
6. **The vendor claims.** Parasolid's and ACIS's speed devices are known only from vendor material.
   Is there a published, citable benchmark of either against OCCT on B-rep booleans that would tell
   us how far we are from the commercial bar?
7. **n-ary composition and B4.** Does the composition of box-disjoint operands (§11.2) need its own
   entry in B4's decomposition, or does it fall out of B4 when no face pairs are found? If it falls
   out of B4, it also covers operands whose boxes overlap but whose faces do not meet, and those
   need the lump-nesting check.
8. **Face-pair selection in the pin.** How does the pin choose candidate face pairs today (§6.1)?
   Rank 7 depends on the answer.
9. **What this report did not do.** It did not measure anything. The ranks are INFERRED from pre-pin
   shares (dbaedda4a); step 0 re-profiles on the pin and re-ranks before anything is tasked.
