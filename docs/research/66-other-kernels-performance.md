# 66 - Other kernels: how the fast ones get their speed, and what we can take

Status: **proposed for review; adversarial review pending.**

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

- **READ**: from the kernel's source (permissive licences only) or its published documentation. The
  repo path, or the page, is given.
- **PUBLISHED**: from a paper, thesis or vendor publication, cited in §3.
- **MEASURED**: a figure already in 05, or in the pin's benchmark profile of 2026-10-27 (17 recipe
  cases, profiled by sampling; the figures only are quoted here). Nothing was measured for this
  report.
- **INFERRED**: a judgement or estimate made here. Every gain below is INFERRED unless marked
  otherwise.

"Bit-identical" means the change leaves every result bit unchanged. "Truth-checked" means the result
may change, and the change must be shown equally or more correct against the truths (04 §1, truth
over pin).

**Licence rule.** Source was read only for MIT, Apache-2.0, 0BSD, zlib and the OpenNURBS licence.
OCCT (LGPL-2.1 with exception), CGAL (GPL/LGPL), SISL and GoTools (AGPL) and libfive (MPL-2.0)
were used through their documentation, papers and talks only. No line of their code was read or
paraphrased. Parasolid and ACIS are closed: published papers and vendor documents only.

## 1. Summary

The fast kernels do not get their speed from clever arithmetic. They get it from **doing less
work**, in four ways:

1. **Decide once per region, not once per face or point.** Manifold classifies one point per
   connected component of the unbroken mesh and floods the result. truck spreads in/out status
   across faces that share uncut edges. The pin classifies every face interval and every inner loop
   with a full point solve. On gear_60 that classification is 40% of the time, and the real SSI is
   3.5% (MEASURED, the pin's profile).
2. **Make the decisions consistent by construction, so nothing needs repair or retry.** Manifold
   derives every edge-face decision from one vertex-edge predicate with one tie rule. OCCT computes
   interferences in increasing dimension, V/V up to F/F, so that "redundant interferences" between
   higher shapes are skipped (READ, OCCT docs). 60 B2 already adopts the order. The speed comes from
   never retrying.
3. **Keep derived data and move it, rather than rebuilding it.** Manifold and OpenNURBS transform a
   tree's boxes instead of rebuilding the tree. Fornjot and arris approximate each edge once, as a
   pure function of the edge. springrep carries a per-face memo (volume, moments, boxes) through
   every boolean that leaves the face uncut.
4. **Small, local, allocation-free inner loops.** OpenNURBS evaluates the basis in stack buffers
   sized by degree, and starts the span search from a hint. These are the same bit-identical gains
   the pin's profile found (+27% from hoisting bounds checks, no per-call allocation).

What the fast mesh kernels add on top (Manifold, truck's and verb's SSI by mesh) is **approximating
the geometry**: they trade exactness for speed. That trade is not open to us (§17). The fastest
exact kernels that are correct by our bar (CGAL's filtered exact predicates, Shewchuk's adaptive
predicates) show the other route: a cheap floating-point answer with a certified error bound, and
the slow exact path only when the bound fails.

### Ranked table of adoptable techniques

Rank = expected gain × confidence, both INFERRED. "Gain" is on the cases where the technique applies.
"Confidence" is that it applies cleanly under 04 §1.

| # | Technique | From | Maps to | Gain | Conf. | Verdict |
|---|---|---|---|---|---|---|
| 1 | Classify once per connected region of faces cut out by the intersection graph, then flood | Manifold `Winding03`, truck `integrate_by_component` | booleans step 4-5; 60 B2/B4; the pin's 40% classification share | large (classification is 40% of gear_60) | high | **adopt** |
| 2 | Exact box prefilter before point classification and inner-loop redistribution | Manifold `PointWinding`; OCCT `BOPDS_Iterator` boxes | BR4 boxes; booleans; the pin's `redistribute_inner_loops` (11.4% of plate_12) | medium-large on many-hole bodies | high | **adopt** |
| 3 | Per-face memo of derived values (volume, moments, boxes) carried through operations that leave the face uncut | springrep `FaceMemo` | BR3 `carry`, BR4 cells; mass properties (30% of plate_12) | large on feature chains | high | **adopt** (as cells on `Brep`, not global ids) |
| 4 | Degree-sized stack scratch and a span hint for evaluation | OpenNURBS `ON_EvaluateNurbsBasis`, `ON_NurbsSpanIndex` | NS2 point cost; CU curve inversion; 65 rows 2, 7, 11 | 1.2-1.8x gap to the pin closes partly | high | **adopt** (bit-identical) |
| 5 | Adaptive Gauss-Kronrod with a fixed G7K15 rule for face integrals | springrep `numeric.rs` | 65 row 8; mass properties | 2-3 orders fewer evaluations (INFERRED in the pin's profile) | medium-high | **prototype** (truth-checked) |
| 6 | Transform the box tree and boxes instead of rebuilding, with outward rounding | Manifold `collider_.Transform`, OpenNURBS R-tree transform, Arvo's box transform | BR6 transform; BR4 cells | medium per transformed body | medium-high | **adopt for similarities; rebuild otherwise** |
| 7 | Edge samples as a pure function of the edge, computed once and shared by both faces | Fornjot `CurveApproxCache`, arris ADR-0052 | tessellation; BR4 cells | medium; watertight for free | high | **adopt** |
| 8 | N-ary union: compose operands whose boxes are disjoint without a boolean, then boolean the rest smallest first, tie-broken by input order | Manifold `csg_tree.cpp` | 60 B4 (n-ary as one decomposition) | large on patterns (many bosses or holes) | medium | **prototype** |
| 9 | Filtered exact predicates (float with a static or semi-static error bound, exact fallback) for 2-D trim and tessellation decisions | CGAL Epick, Shewchuk, the `robust` crate (used by Fornjot) | tessellation; pcurve loop tests; SS4 classification at a point | small speed gain; large robustness gain | high | **adopt** for orientation and in-circle tests |
| 10 | Interval evaluation with pruning of what a region cannot reach | libfive (Keeter 2020) | 65 rows 6 and 10; the global solver | uncertain | low-medium | **prototype later** |

Below the top ten, with lower gain or confidence: lazy transforms on operands (§11.4, prototype with
6); a Morton radix bulk build (§6.2, reject for now, because 65 row 3 is done and beats the pin by
1.2-2.9x); one curve-to-surface refinement shared across faces (§8.3); OCCT's gluing mode for
operands that share faces (§11.3, prototype as a stated-fact path under 60 B1).

### What should change in builders' current briefs

- **NS2 surface point cost and grid eval.** Use OpenNURBS's pattern: one stack buffer sized by
  `order0² + order1² + count`, heap only past a fixed degree. Add an optional span hint (the last
  span) to the point API and to grid evaluation. The search and its result are unchanged, so it is
  bit-identical. Both bear on 05's open candidates, span search and the 256-byte `Basis`.
- **Curve inversion.** Warm starts and span hints are fine. Do **not** take a presearch grid plus
  Newton as the global step. That is truck's and verb's design, and it returns local minima (§7.2).
  The global step stays certified (65 row 7).
- **global_solve branch cost.** No change to the profile-first plan. Note libfive's interval
  pruning (§9.2) as the idea to test after Krawczyk at the leaves.
- **Closed-form cubic/quartic, elevation accuracy, curve restrict/reverse/transform.** No kernel
  read here does better than 65 rows 9 and 14. OpenNURBS uses closed-form Bernstein bases only up to
  degree 4 and de Casteljau elsewhere, which agrees with keeping Bernstein form. For transform:
  transform the cached boxes outward-rounded rather than recomputing them (rank 6), but only for
  similarities.
- **Surface enum.** Keep dispatch at batch granularity (one match per grid or batch, not per
  point). That is how OpenNURBS and Manifold keep their inner loops monomorphic. No other change.

## 2. What each kernel is, in one line

| Kernel | Kind | Speed comes from (INFERRED from what was read) |
|---|---|---|
| Manifold | triangle-mesh booleans, float | consistent symbolic predicates, one classification per component, a Morton-sorted radix BVH, TBB parallelism |
| truck | Rust NURBS B-rep | tessellate-then-refine SSI, component status propagation; little else tuned |
| Fornjot | Rust B-rep, shut down | "performance has not been a priority"; one curve approximation per edge |
| OpenNURBS | NURBS geometry library (Rhino's file format) | stack buffers, span hints, Bernstein closed forms, a packed R-tree |
| OCCT | full LGPL B-rep kernel | docs: dimension-ordered interferences, box pair selection, cached intersection context, parallel and gluing options |
| Parasolid, ACIS | commercial B-rep | published: local tolerances, multi-threaded operations and faceting |
| CGAL | exact computational geometry | filtered exact predicates, lazy exact constructions |
| SISL, GoTools | SINTEF spline libraries | published: recursive subdivision with simple-case tests; approximate implicitisation |
| verb, tinyspline | NURBS libraries | verb: lazy box trees, mesh-then-refine SSI. tinyspline: plain de Boor |
| libfive | implicit (f-rep) modeller | interval arithmetic with tape pruning, on CPU and GPU |
| springrep, arris, cadrs, geomsolver, anvil | small Rust kernels (60's table) | springrep: per-face memos, Gauss-Kronrod, banded LU. arris: per-edge sampling. cadrs: retry ladder and polytope fast path (rejected in 60) |

## 3. Sources

| Source | Licence | What was read |
|---|---|---|
| elalish/manifold @57cf361 | Apache-2.0 | `src/boolean3.cpp` (Shadow, Kernel11/02/12, Winding03), `src/collider.h`, `src/sort.cpp`, `src/impl.cpp`, `src/csg_tree.cpp`, README |
| ricosjp/truck @117470c | Apache-2.0 | `truck-geotrait/src/algo/curve.rs`, `truck-shapeops/src/transversal/` (intersection curves, `faces_classification`), `truck-meshalgo/src/analyzers/collision.rs` |
| hannobraun/fornjot @7e85668 | 0BSD | `crates/fj/src/core/storage/store.rs`, curve approximation cache, dependency list (`robust`, `spade`) |
| mcneel/opennurbs @2bd2e66 | OpenNURBS licence (permissive), zlib parts | `opennurbs_evaluate_nurbs.cpp`, `opennurbs_knot.cpp`, `opennurbs_rtree.h/.cpp` |
| pboyer/verb @f3ba23c | MIT | lazy bounding-box trees, surface-surface intersection, closest point |
| msteinbeck/tinyspline @bb3b02c | MIT | evaluation, `eval_all`, `sample`, `bisect` |
| springcraft-ai/springrep @f2ad60e | MIT/Apache-2.0 | `src/topo/memo.rs`, `src/numeric.rs` |
| arris-labs/arris @4408202 | MIT/Apache-2.0 | ADR-0052 (edge samples), ADR-0030 (cancellation); 60 covers the rest |
| rvdende/cadrs, bmander/geomsolver, swperb/anvilcad | MIT/Apache-2.0, MIT | nothing new for performance beyond 60 |
| OCCT Boolean Operations specification (occt3d.com/dev/doc/overview/html/specification__boolean_operations.html) | docs | interference order, `BOPDS_Iterator`, options (parallel, fuzzy, OBB, gluing, safe input) |
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

## 4. NURBS evaluation, basis functions, batch and grid evaluation

### 4.1 Stack scratch sized by degree (OpenNURBS)

- **What.** `ON_EvaluateNurbsBasis` runs the triangular Cox-de Boor recurrence on the local knot
  window into a stack buffer of 80 doubles, and goes to the heap only for large orders.
  `ON_EvaluateNurbsNonRationalSpan` uses `stack_buffer[64]`, sized by order².
  `ON_EvaluateNurbsSurfaceSpan` uses `stack_buffer[128]`, sized by order0² + order1² + count.
- **Evidence.** READ (`opennurbs_evaluate_nurbs.cpp`).
- **Correctness and determinism.** The arithmetic and its order are unchanged; only the storage moves.
  Bit-identical.
- **Maps to.** NS2 (point cost). 05 says the fixed cost is not the scratch (jet<1> was fixed by the
  SMALL=8 scratch), and names span search, `Result` moves and the 256-byte `Basis`. OpenNURBS keeps
  only the order-sized triangle live and returns nothing bigger than the point. That points at the
  `Basis` size as the cost to cut: size the buffer by the actual order, not the maximum.
- **Verdict.** Adopt (bit-identical).

### 4.2 End-knot snapping (OpenNURBS)

- **What.** At an end knot, basis values are snapped to exactly 1. In the order-2 span evaluator, the
  result is snapped to the control point when two control points are equal.
- **Evidence.** READ.
- **Correctness.** The snap returns the exact value where the recurrence rounds near it: equally or
  more correct. It changes bits, so it must be truth-checked.
- **Maps to.** NS2, CU2. It is accuracy, not speed, but cheap.
- **Verdict.** Prototype (truth-checked; probably already true of the scx recurrence at the ends;
  check).

### 4.3 Closed-form Bernstein bases to degree 4; de Casteljau above (OpenNURBS)

- **What.** `ON_EvaluateBernsteinBasis` has closed forms up to degree 4. The comments say de
  Casteljau is faster and more stable than a Bernstein sum.
- **Evidence.** READ.
- **Correctness.** Each form is deterministic. Moving between them changes bits.
- **Maps to.** 65 row 11 (span-cached Bezier O(p) evaluation); 65 row 1.
- **Verdict.** No new action; it agrees with 65's Bernstein-form choices.

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
  segment queries with outward rounding, and recorders for parallel collection. OpenNURBS uses a
  Guttman R-tree with at most 6 children per node, chosen for memory.
- **Evidence.** READ (Manifold `collider.h`, OpenNURBS `opennurbs_rtree.h`); READ docs (OCCT).
- **Maps to.** 65 row 3 (done: a `select_nth` median build, 9.0 µs / 141 µs / 2.09 ms at n = 100 /
  1,000 / 10,000 against the pin's 10.8 µs / 322 µs / 6.11 ms, MEASURED).

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

- **What.** `ON_NurbsSpanIndex(order, cv_count, knot, t, side, hint)` uses the last span found as a
  hint to narrow the search.
- **Evidence.** READ (`opennurbs_knot.cpp`).
- **Correctness.** The span found is the same. Bit-identical, as long as the hint only narrows the
  search and the knot multiplicity rule (`side`) is applied the same way.
- **Maps to.** NS2 (span search is one of 05's named fixed costs); CU curve inversion; 65 row 7
  (warm starts). Newton iterates stay in one span most of the time.
- **Verdict.** Adopt. Pass the hint explicitly (no ambient state); never store it in the curve.

### 7.2 Presearch grid plus Newton (truck, verb)

- **What.** truck's `presearch` samples a uniform grid, takes the nearest sample, and runs Newton on
  C'·(C − P). verb samples `n·p` points, then runs Newton.
- **Evidence.** READ (`truck-geotrait/src/algo/curve.rs`; verb).
- **Correctness.** Fast, but it can converge to a local minimum when two parts of the curve come
  within a grid cell of the point. There is no certificate. Not correct by our bar as the only
  global step.
- **Verdict.** Reject as a global method (§17). Keep it as a warm start inside the certified
  best-first search of 65 row 7.

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
  pairs; we do not read OCCT code.

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

### 10.1 One classification per component (Manifold, truck)

- **What.** Manifold's `Winding03` builds a union-find (DisjointSets) over the edges of a mesh that
  intersections do not break. It runs one winding-number query per connected component, then floods
  the result: "Vertices on the same connected component have the same winding number." truck's
  `integrate_by_component` spreads And/Or status over connected components of unknown faces through
  shared boundary edges.
- **Evidence.** READ (`boolean3.cpp`; `truck-shapeops/src/transversal/faces_classification`).
- **Correctness.** A face region that the intersection graph does not cut lies wholly inside or
  wholly outside the other body, so one exact classification of one interior point decides all of
  it. Exact, deterministic, and fewer opportunities for two nearby classifications to disagree. The
  rule needs the intersection graph to be complete: an edge missed by SSI would merge two regions.
  That is also true of per-face classification, which then gets the answer wrong on part of a face.
- **Maps to.** Booleans (step 4-5); 60 B2 (one imprint pool); 60 B4. The pin's gear_60 spends 40% in
  `classify_face_interval → point_3d_classify → global_point_solve` (MEASURED). With classification
  per region, gear_60 needs one query per region (about the number of new faces) instead of one per
  face interval.
- **Determinism detail.** Pick the representative point by a fixed rule (for example, the lowest
  face id in the region, at its fixed interior point), not by the union-find's root, whose identity
  depends on union order.
- **Verdict.** Adopt. Largest expected gain in this report.

### 10.2 Box prefilter (Manifold, OCCT)

- **What.** Manifold's `PointWinding` rejects faces by bounding box before any winding test. OCCT
  selects pairs by boxes before intersecting them.
- **Evidence.** READ.
- **Correctness.** Exact if the box is a certified enclosure: box disjoint means not contained. The
  pin's `redistribute_inner_loops` classifies every inner loop of the target face against each new
  edge or loop (11.4% of plate_12, MEASURED), with no prefilter upstream either.
- **Maps to.** BR4 (uv and face boxes); booleans.
- **Verdict.** Adopt. Results are unchanged when the prefilter is exact; check with the difftest.

### 10.3 Axis-aligned rays (Manifold)

- **What.** Every Manifold query is a vertical (z) ray, so each test is a 2-D interpolation.
- **Evidence.** READ.
- **Correctness.** For curved B-rep faces an axis ray hits tangencies and seams as often as any ray,
  and the cheaper test only applies to planar faces.
- **Verdict.** Reject for the general path. For planar faces the ray-polygon test is already cheap.

## 11. Booleans (B-rep and mesh)

### 11.1 Lower dimension first, with one tie rule (Manifold, OCCT)

- **What.** In Manifold, every decision derives from one vertex-edge "shadow" predicate:
  `Shadows(p,q,movedP,movedQ,tol) = |p-q|>tol ? p<q : movedP-p < movedQ-q`. Shadow01 builds the
  edge-edge (Kernel11), vertex-face (Kernel02) and edge-face (Kernel12) decisions, so they cannot
  disagree. Ties within `TieTol` are broken by a deterministic perturbation (vertex displacements of
  tol/4 along face-plane offsets, plus a per-axis shift of tol/4·(1/61, 1/89, 1/113)); union expands
  P and difference contracts it. OCCT computes interferences in the order V/V, V/E, E/E, V/F, E/F,
  F/F, "avoiding the computation of redundant interferences".
- **Evidence.** READ (Manifold `boolean3.cpp`); READ docs (OCCT).
- **Correctness.** The perturbation is symbolic: it decides ties, it does not move the output
  geometry. That keeps the decisions consistent at the cost of a convention for touching cases. For
  a B-rep kernel the decisions are about curves on surfaces, and contacts must be kept (60 R-8), not
  resolved by a perturbation. The dimension order is 60 B2.
- **Verdict.** Already adopted as 60 B2 (dimension order). The symbolic tie rule: prototype only for
  the planar and polygon decisions inside tessellation and 2-D trims, where it is simulation of
  simplicity by another name. Reject for curved contacts.

### 11.2 N-ary union by composition (Manifold)

- **What.** `csg_tree.cpp` partitions the children of an n-ary union into sets that are pairwise
  disjoint by box (with Arvo's box transform for lazily transformed children) and "composes" them:
  concatenates without a boolean. Children touching within the tie tolerance are booleaned. The rest
  are booleaned smallest first from a heap, tie-broken by insertion order. `kMaxUnionSize` chunks the
  disjoint check.
- **Evidence.** READ.
- **Correctness.** Composing box-disjoint bodies is exact: no face of one can meet a face of
  another. Smallest-first order is deterministic with the insertion-order tie. For a B-rep the
  result of a different pair order may differ in entity order and in which edges merge, so the
  order must be a fixed function of the inputs (it is).
- **Maps to.** 60 B4 (n-ary as one decomposition). Composition fits inside B4: disjoint operands add
  shells to the decomposition without any face pairs. For patterns (12×12 holes, 60 gear cuts) the
  pin's cost per cut grows with the body (plate 3.1 → 5.9 ms per hole; gear 5.7 → 13.6 ms per cut,
  MEASURED). A cut of 144 tools in one decomposition, with the tools disjoint from each other,
  pays the body's whole-B-rep passes once.
- **Verdict.** Prototype, inside B4. The box test must be outward-rounded and strict (touching
  boxes are not disjoint).

### 11.3 Gluing for coinciding arguments (OCCT)

- **What.** OCCT's gluing option "allows speeding-up the intersection of the arguments" when they
  share coinciding faces, by skipping the F/F intersection of shared parts.
- **Evidence.** READ docs. The docs give no figures.
- **Correctness.** Correct only when the caller's claim is true. OCCT trusts the claim.
- **Maps to.** 60 B1 (caller-stated facts, verified and refused if false).
- **Verdict.** Prototype as a B1 fact: the caller states the shared faces, the kernel verifies the
  claim cheaply (the same surface record and overlapping trims) and refuses if false. Never trusted
  unverified.

### 11.4 Lazy transforms (Manifold)

- **What.** A transformed operand keeps its transform pending; overlap checks use Arvo's transformed
  box, and the mesh is transformed only when the boolean needs it.
- **Evidence.** READ (`csg_tree.cpp`).
- **Correctness.** Exact as long as the box test is an outward enclosure. In a B-rep, applying a
  similarity to geometry changes bits (each control point is rounded); applying it once, at the end
  of a chain, rather than at each step, is one rounding instead of several, which is more correct.
- **Maps to.** BR6 (transform). It changes when the rounding happens, so the result is
  truth-checked.
- **Verdict.** Prototype together with rank 6.

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

## 12. Tessellation

### 12.1 One sampling per edge (Fornjot, arris)

- **What.** Fornjot's `CurveApproxCache` (`approx_curve_with_cache`) approximates each shared edge
  once, so both faces receive the same points. arris ADR-0052: "an edge's samples are a pure function
  of the edge", so a kept face's mesh meets its re-meshed neighbours bit for bit, and a consumer can
  cache meshes per face across edits.
- **Evidence.** READ.
- **Correctness.** Watertight by construction, deterministic, and it avoids half the edge sampling.
  arris found one dependence to remove: the sample count must not read the face's whole (u, v) box.
- **Maps to.** Tessellation; BR4 cells (an edge cell).
- **Verdict.** Adopt, including arris's rule that the count depends on the edge alone.

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
- **Verdict.** Note as a candidate dependency for tessellation (popular dev-dep, permissive). Check
  its determinism across targets before adopting: it must not use FMA or platform intrinsics in its
  predicates.

## 13. Mass properties

### 13.1 Gauss-Kronrod (springrep)

- **What.** springrep integrates face terms with fixed Gauss-Legendre rules and an adaptive
  15-point Kronrod rule with its embedded 7-point Gauss rule (G7K15), and a G3K7 pair. Its tests
  check that the pair integrates polynomials exactly to their degrees.
- **Evidence.** READ (`src/numeric.rs`); PUBLISHED (Piessens et al. 1983).
- **Correctness.** Deterministic with a fixed subdivision rule. More accurate per evaluation than
  nested Cash-Karp RK. Results change, so truth-checked (analytic volumes for primitives; plate_12's
  analytic volume is 100000 − 5760π).
- **Maps to.** 65 row 8. The pin's profile: mass properties are about 30% of plate_12, with 14.7M
  surface-derivative evaluations for nine primitives at about 190 ns each (MEASURED).
- **Verdict.** Prototype (65 row 8); springrep confirms the design in a Rust kernel.

### 13.2 Per-face memo carried through operations (springrep)

- **What.** springrep's `FaceMemo` holds what is derived from one face's geometry: volume share,
  integrals and moments with the key they were computed for, pcurve parameter tables, loose box,
  and which other faces are known apart. "A boolean that passes a face through uncut gives the
  result's face the same memo, so a part built by many features measures and samples each face once
  rather than once a feature."
- **Evidence.** READ (`src/topo/memo.rs`).
- **Correctness.** Exact reuse: the face is the same, so the values are the same. Two parts conflict
  with our rules: memo ids from a global `AtomicU64` (ambient state, 04 §1), and the `Mutex` and
  `OnceLock` fills, whose first writer wins (fine when the value is a pure function of the face; not
  fine if fills under different keys race).
- **Maps to.** BR3 `carry` (the record of what an operation kept) and BR4 cells. A kept face's cells
  travel with it. Mass properties of a body after a boolean then cost only the new and cut faces.
- **Verdict.** Adopt, as cells on `Brep` carried by BR3, keyed by the face record, with no global
  ids. Gain: the plate pays mass properties for one hole's faces per cut, not all of them.

## 14. Memory layout and caching

- **Morton-ordered vertices and faces (Manifold `sort.cpp`).** Vertices and faces are sorted by
  Morton code for locality; equal codes break by index (READ). Changes entity order, so it is not
  bit-identical for anything that iterates. Reject for B-reps: our entity order is part of the
  result, and our sizes are small.
- **Append-only stores (Fornjot `store.rs`).** Blocks of entities that never move, so ids are stable
  (READ). Our arenas already append. The pin's profile found deleted slots cost about 3× in arena
  walks on plate_12 (438 arena faces, 150 live, MEASURED). Iterate live entities only (a live list
  or a bitset), not slots.
- **Fast hash maps (truck: FxHashMap).** Faster hashing (READ). Deterministic as long as nothing
  iterates the map in hash order. Adopt only with that rule; prefer sorted vectors or dense arrays
  indexed by id where ids are dense (the pin's 12.4% in `BTreeMap::find_key_index`, MEASURED, was a
  port artefact of per-entity lookups).
- **Lane-wise SoA.** None of the kernels read here uses SoA in its NURBS evaluation. 65 §3.1 stands:
  lane-wise over x/y/z/w is bit-exact, across the summation index is not.
- **Intersection context cache (OCCT).** "Used to cache the tools to increase the algorithm
  performance" (READ docs): projectors and classifiers per face are built once per boolean. Maps to
  BR4 cells. Adopt as cells, already planned.

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
and per-face mass properties fit this exactly; face-pair intersection fits once classification is
per region (§10.1). Pre-warming lazy caches in parallel changes results and is out (§6.3).

## 16. Exact and adaptive predicates

- **What.** Shewchuk's adaptive predicates evaluate orientation and in-circle tests in floats with
  an error bound, and extend the precision in stages only as needed. CGAL's filters do the same with
  interval arithmetic or static bounds. Fornjot uses the `robust` crate (Shewchuk's predicates,
  MIT/Apache).
- **Evidence.** PUBLISHED (Shewchuk 1997; Brönnimann et al. 2001; Devillers and Pion 2003); READ
  (Fornjot dependencies).
- **Correctness.** Exact signs, deterministic on every target provided the float operations are
  IEEE with no FMA contraction (Shewchuk's expansions rely on exact two-sum and two-product, which
  are built from ordinary operations). Faster than our current alternative in hard cases, since the
  alternative is a tolerance and a retry.
- **Maps to.** Tessellation (constrained triangulation); 2-D trim loop tests (point in polygon of a
  pcurve polyline, loop orientation); SS4's planar cases.
- **Verdict.** Adopt for the 2-D and 3-D orientation and in-circle predicates in tessellation and
  trim tests. Check that the crate we choose does not contract FMA or use intrinsics, or own the
  ~200 lines in `fmath` style. They do not replace tolerance-based decisions on curved geometry.

## 17. Fast, but not correct by our bar

| Technique | Who | Why it is fast | What it costs us |
|---|---|---|---|
| SSI by intersecting tessellations, then refining | truck, verb | one mesh pass instead of subdivision | misses loops smaller than the mesh; resolution-dependent results |
| Inversion by a presearch grid plus Newton, no certificate | truck, verb | a fixed small number of samples | local minima returned as answers |
| Mesh approximation of curved geometry | Manifold | float triangle booleans are simple and parallel | exactness of curved faces is gone; not a B-rep |
| Fuzzy values and growing tolerances | OCCT (fuzzy option), Parasolid and ACIS (local tolerances, Jackson 1995) | operations succeed on near-coincident input without exact handling | results depend on a tolerance the caller chose; tolerance grows through a model. Our tolerances are fixed per document (one resolution) |
| Tolerance retry ladders | cadrs (60 R-1) | a failure becomes a retry | hides bugs; results depend on which rung succeeded |
| Snapping near-coincident geometry | springrep, cadrs (60 R-2) | removes near-degenerate cases | moves valid geometry by 1e-6 to 1e-4 mm |
| Thread-local or global caches | cadrs (60 R-3), springrep (global memo ids) | no plumbing | ambient state; results may depend on history |
| Parallel reductions in scheduler order; wasm relaxed-SIMD FMA | general | full use of cores and SIMD | results differ run to run and across targets |
| SIMD across the summation index | general | wider vectors | changes the summation order, so bits differ by target |
| Power-basis Horner at high degree | general | fewer operations than de Casteljau | unstable at degree 10 and above |
| Per-span cached reciprocals | general | one division per span | changes bits; allowed only with a truth check |
| Lazy caches refined by whichever query arrives first | verb-style, in parallel | work on demand | results depend on query order |
| Sorting with `partial_cmp().unwrap_or(...)` | truck | no NaN handling | not a total order if a NaN appears; order and results undefined |
| Morton reordering of entities | Manifold | memory locality | changes entity order, which is part of our result |

## 18. Open questions

1. **Region classification and missed edges.** §10.1 relies on a complete intersection graph. How
   should a region classification check itself? One idea: classify two points in a large region
   and refuse if they disagree. That costs a second query per region; it is cheaper than per face
   but adds work over the plain rule.
2. **Representative point.** Which interior point of a region does the classification use, so that
   it is fixed by the inputs and stays far from the region's edges? A face's existing interior
   point, from the face with the lowest id in the region, is the simplest candidate.
3. **Memo cells and `carry`.** Does BR3's `carry` record enough to move every cell of a kept face
   (boxes, mass-property terms, edge samples) without recomputing? Which operations count as
   "keeping" a face when its loops change but its surface does not (the mass-property terms then
   change; the surface box does not)?
4. **Transform of cells.** For which transforms is a transformed box still a certified enclosure
   with outward rounding (similarities: yes; general affine: the box grows, still valid; projective:
   no)? Do the cell transforms need their own truth tests?
5. **Predicate crate.** Is there a Shewchuk or CGAL-style filtered predicate crate whose code is free
   of FMA contraction and intrinsics on all three targets, or do we own it?
6. **The vendor claims.** Parasolid's and ACIS's speed devices are known only from vendor material.
   Is there a published, citable benchmark of either against OCCT on B-rep booleans that would tell
   us how far we are from the commercial bar?
7. **n-ary composition and B4.** Does the composition of box-disjoint operands (§11.2) need its own
   entry in B4's decomposition, or does it fall out of B4 when no face pairs are found?
8. **What this report did not do.** It did not measure anything. The gains in §1 are INFERRED from
   the pin's profile shares; ranks 1 to 3 should be checked against a profile of the solidcorex
   boolean once step 4 has one.
