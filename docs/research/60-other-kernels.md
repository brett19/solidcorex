# 60 - Other kernels: design ideas and completeness

Written for solidcorex. The question: what do other Rust B-rep kernels do that would improve
solidcorex's plan, or make it more complete? Every idea is checked against the plan (01, 09, 04)
and against the owner rules before it gets a verdict. Nothing here is a benchmark or a score
comparison.

Nothing was built or run. Only this file was written.

An adversarial review (`reviews/review-60.md`) found factual errors, corrected below in place. The
owner's ruling of 11-27 (section 6) supersedes the verdicts and decisions here where they differ.

- **Labels:**
  - READ: read from a kernel's code, docs or ADRs, or from solidcorex's docs.
  - MEASURED: counted. This report counts very little; a count is always labelled.
  - JUDGED: a design judgement.
- **Owner rules applied in every check** (04 §1, 11-26 and 11-27):
  - R1: idiomatic Rust.
  - R2: deterministic results (01 §8).
  - R3: truth over pin (04 11-27, 01 §9).
  - R4: one document resolution, with accuracy recorded per entity (04 11-26, 01 §4, 09 §3).
  - R5: keep solidcore's algorithms (04 11-26).
  - R6: honest `&Brep` / `Draft` signatures (01 §0, 04 11-23).
  - R7: no ambient state (01 §6.1, §8.3).
- **Also asked of every idea:**
  - Does it lose something solidcore has and the small kernels skip? Four things are guarded:
    freeform SSI; rolling-ball blends with overflow; healing of real imports; the cellular merge
    with regions and laminas.
  - Is "it passes a suite" its only evidence?

## 1. Summary

### 1.1 Adopt

Section numbers point to the full entries in section 3.

| # | Idea | Source | Verdict |
|---|---|---|---|
| T1 | Merge candidate points as the connected components of a relation fixed before any merge, so the result does not depend on discovery order | arris ADR-0022 | adopt with changes (bound the chain) |
| T2 | A `within` that allows a few ulps of the coordinate magnitude, so checks far from the origin stop failing on rounding alone | springrep `src/tol.rs` | adopt |
| T4 | A new vertex inside an operand vertex's recorded ball may inherit that accuracy, without counting as a raise | arris ADR-0076 | adopt |
| T5 | The four kinds of tolerance (identity, fit budget, search radius, parameter) named apart in `scx-num` | next.BREP, idea only; `rvdende_cadrs/crates/cadrs_kernel/src/native/tol.rs` | adopt (the naming only) |
| B1 | Caller-stated incidences (`Facts`) handed to the Boolean, each verified and refused if false | springrep `src/topo/facts.rs` | adopt with changes |
| B2 | Imprint in dimension order (springrep's: VV, EF pierce, EE, VF, FF, then place hits, VE split, imprint, dedup) into one vertex pool | springrep `doc/design.md`, arris ADR-0004/0022, geomsolver `tolerant.rs` | adopt as the spec's statement of order |
| B3 | Tangent contact settled in order: stated facts, then closed form, then tracing; never "no contact" by default | springrep `doc/design.md`, geomsolver README | adopt |
| B4 | N-ary union and cut as one decomposition over every operand | arris ADR-0050 | adopt |
| B5 | Split returns both sides from one decomposition; either side may be empty | arris ADR-0051, brep-conformance `doc/contract.md` B34 | adopt |
| B6 | A same-body shortcut (A op A) and a guard for operands that share entities | springrep `src/boolean/same.rs`, arris ADR-0087 | adopt |
| V1 | Validate every operation's result with a cheap check; keep the full check for tests | cadrs README, anvil-brep `src/lib.rs`, springrep `src/check.rs` | adopt with changes |
| V2 | Check that no two result faces cross away from their shared edges after a Boolean | `rvdende_cadrs/crates/cadrs_kernel/src/native/check_faces.rs`, springrep `src/check.rs` | adopt in the full check and on Boolean results |
| D1 | A parallel pass meters each item and sums in index order, so a single total budget stops at the same item under every executor | arris ADR-0030 | adopt with changes (owner decision) |
| D2 | Fuzz the readers and the checked NURBS constructors | cadrs `tests/fuzz_readers.rs` | adopt |
| Q1 | Distance by certified critical points with a deterministic tie-break and a `unique` flag | arris ADR-0054 | adopt |
| Q2 | A ray hit names the most specific entity, deduplicated along it; a ray lying in a face is refused | arris ADR-0053 | adopt |
| Q3 | Interference as Apart, Touching or Overlapping, read from the Boolean's own rules | arris ADR-0055 | adopt |
| Q4 | A certified tight bounding box, apart from the loose box cell | arris ADR-0093 | adopt |
| Q5 | Betti numbers from the Euler characteristic and the region graph | `springcraft-ai_springrep/src/measure.rs` (`betti`), anvil-brep `src/check.rs` | adopt |
| Q6 | A read-only sliver and thin-place finder | geomsolver `sliver.rs`, springrep `src/probe.rs` | adopt |
| X1 | A mirror, with the surface parameterisation absorbing the handedness | pin `topology/brep/mirror.rs`, arris ADR-0031 | adopt the pin's (section 6) |
| X2 | Affine images of bodies, with each kind's image exact | pin `transform_matrix`, geomsolver `affine.rs` | adopt the pin's (section 6) |
| O1 | A rounded offset built as body ∪ face slabs ∪ edge tubes ∪ vertex balls | cadrs README (`local`, `offset_round`), geomsolver README | adopt with changes, as a second join mode |
| E1 | Name an import's gap closure as a recorded raise, and let a raise pass the operation ceiling | arris ADR-0058/0060 | not covered: 09 §6 refused it (corrected; section 6) |
| E2 | STEP export states no tighter an uncertainty than the body holds | geomsolver README | adopt |
| P1 | Provenance pieces in a guaranteed split order keyed by input entities only | arris ADR-0009 | adopt into `Changes` |

### 1.2 Reject

| # | Idea | Source | Reason |
|---|---|---|---|
| T3 | Derive a tangency sine from the resolution (`LINEAR / RESOLVED`) | springrep `src/tol.rs` | It is an angle tolerance under another name. 01 §4.2 has none, and an angle from accuracy divided by size already covers coincidence |
| R-1 | Retry a failed Boolean at 1e-5, then 1e-4 | cadrs README (Tolerances) | A per-operation tolerance ladder breaks R4. It also hides the failure that should be fixed |
| R-2 | Snap faces a hair off another onto it before the Boolean | springrep `src/boolean/snap.rs`, cadrs README (`CADRS_DEBUG_NO_SNAP`) | It moves valid geometry by up to `SNAP` 1e-6 mm, and up to `SNAP_GRAZING` 1e-4 mm at grazing meetings (corrected; the first draft said 1e-6 only), which breaks R3. Arris rejected it too (ADR-0022, context) |
| R-3 | Thread-local caches of sampled boundaries and grids | cadrs README (Tolerances) | Breaks R7. Cells (01 §3) cover it |
| R-4 | A session that owns bodies, with tolerances set on it | cadrs README, `Kernel` trait | Breaks R6 and R7. 01 has no session (§6) |
| R-5 | Environment switches that change results | cadrs README (Environment variables) | Breaks R2 and R7. 01 §6.3 already forbids switches |
| R-6 | A convex-polytope Boolean in closed form, used first | `rvdende_cadrs/crates/cadrs_kernel/src/native/polytope.rs` | A speed device with a second code path to hold in agreement. Its only evidence is the suites. Reconsider after the performance gate (01 §9.4) |
| R-7 | A mesh-based broad phase for large face-pair counts | springrep `src/boolean/near.rs` | The box tree over `Extent` (01 §1.1, `scx-algo`) is the planned broad phase. A mesh adds a tessellation dependency to the Boolean |
| R-8 | Refuse every touching result that an operation cannot keep | springrep `doc/design.md` (results that touch) | solidcore's regions and radial order already keep contacts. Copying the refusal would lose what solidcore has |
| R-9 | Each operation runs the full check and turns a failure into `InvalidResult` | springrep `src/check.rs` | `scx-boolean` and `scx-stitch` sit beside or below `scx-check` in the graph, so operations cannot all call it. Speed is not the reason: the pin proves outwardness on every ops result (`solidcore-ops/src/body.rs`, `finish_proved`; corrected). The result contract is proved at or above `scx-check` (section 6); V1 is the part kept inside `finish` |

### 1.3 Completeness gaps

Section 4 has the full list. In short:

- **Queries:** Betti numbers, interference, a certified tight box, slivers, swept contact, the
  maximum distance, a ray batch.
- **Operations:**
  - n-ary Booleans, and the cells of an arrangement;
  - unify same-surface faces (the pin has it inside the Boolean and stitch);
  - mirror and affine transforms (the pin has both; 01 had only rigid moves);
  - a rounded offset;
  - screw sweeps and the convex hull of points;
  - conic fillets. (Corrected 11-27, research 61: G2 fillets are in the pin, as the new fillet
    system's certified Gⁿ section, orders 2-4.) Setback vertex blends, full rounds and the
    distance-and-angle chamfer are in the pin (corrected).
- **Output:** edge flags in the display mesh (crease or smooth, seam, the faces each edge joins).

### 1.4 Owner decisions

Section 5 has these in full.

1. D1: is the budget a total, metered in index order, or a split made in advance (01 §6.3)?
2. X2: does the plan gain affine transforms (a `Mapped` surface kind, or images of each kind)?
3. Should G2 and conic fillets be built, when solidcore has no algorithm for them (R5)? (Corrected
   11-27, research 61: solidcore has G2; only the conic is new.)
4. V1: does `finish` also check geometry that is cheap to check, or does each operation call a
   fast check itself?
5. T1: when a component holds two vertices of one operand, is that a refusal, or a defect of the
   input reported by `Refused`?

## 2. Sources

| Repo | Language | Licence | What was read |
|---|---|---|---|
| `springcraft-ai_springrep` | Rust | MIT OR Apache-2.0 | `doc/design.md` (most of it); `src/tol.rs`, `src/topo/facts.rs`, `src/boolean/snap.rs` (heads); the module list |
| `rvdende_cadrs` | Rust | MIT OR Apache-2.0 | `crates/cadrs_kernel/README.md` (whole); the module and test lists |
| `arris-labs_arris` | Rust | MIT OR Apache-2.0 | the ADR list; the decision sections of ADR-0009, 0022, 0030, 0031, 0050, 0051, 0052, 0053, 0054, 0055, 0058, 0060, 0064, 0076, 0087, 0093 |
| `bmander_geomsolver` | Rust (`rust/cad-kernel`) | MIT OR Apache-2.0 (crate) | `rust/cad-kernel/README.md`; the heads of `src/lib.rs`, `src/tolerant.rs`, `src/affine.rs`, `src/sliver.rs` |
| `swperb_anvilcad` | Rust (`crates/anvil-brep`) | MIT OR Apache-2.0 | `src/lib.rs`, `src/tol.rs`; the public items of `src/error.rs` and `src/check.rs` |
| `brep-conformance` | contract | MIT OR Apache-2.0 | `doc/contract.md` v2.0.0, `doc/api.md` |
| `mmiscool_next.BREP.io_RUST_BREP_KERNEL` | Rust | non-permissive | the module comment of `BREP_geometry/src/geometry/tolerance.rs`, for the idea only. Nothing is copied or paraphrased |
| `rsz269_kentron`, OCCT | | AGPL, LGPL | not read |

Each kernel's maturity was not judged. A practice is taken for its argument, not because a suite
passes (READ: ADRs and design docs state the argument; JUDGED: whether it holds for solidcorex).

## 3. Candidates

Each entry gives:

- **Now:** what the plan says today.
- **Other:** what the other kernel does.
- **Change:** the change to 01.
- **Cost / risk.**
- **Check:** the adversarial check against R1-R7, the four guarded features, and the evidence.
- **Verdict.**

### 3.1 Tolerance and coincidence

**T1. Point merging as connected components.**

| | |
|---|---|
| Now | Two entities coincide at `max(res, acc_a, acc_b)` (01 §4.2), or at `resolution + acc₁ + acc₂` (09 §3; the two documents differ, see below). Propagation is order-independent (01 §4.2). 01 does not say how a pool of candidate points becomes vertices (READ). |
| Other | Every hit, crossing and touch is a candidate with a ball: the accuracy of the entities that made it, never a grown one. Two candidates are one point when `‖p − q‖ ≤ t_p + t_q`, or when they name one operand vertex. The relation is fixed before any merge, so the vertices are its connected components, whatever the order the points were found in. A component is numbered by its first member in creation order. Its accuracy is the base plus the spread. Two vertices of one operand in one component are refused (`arris-labs_arris/docs/adr/0022-the-same-within-a-tolerance-is-an-equivalence.md` §1, READ). geomsolver merges a point into a vertex within `tol` of any point already merged there (`bmander_geomsolver/rust/cad-kernel/src/tolerant.rs`, READ), which is order-dependent. |
| Why better | Pairwise "within" is not transitive, so a greedy pool depends on order (JUDGED). Components make the merged set a function of the input set. Arris logged 72,710 Booleans before and after, and found no component chaining outside the band (READ, ADR-0022 §1 grounds). |
| Change | 01 §4.2 gains a rule: "Vertices of an intersection are the components of the coincidence relation over its candidates. Each candidate's ball is its construction bound plus input accuracy. A component's recorded accuracy is the largest distance from its point to a member plus that member's ball (`Basis::Measured`). A component wider than `k × res` (k a named constant) is a `Defect` with the chain in the trace." The component point is a member, picked by a total key, not a centroid. A centroid would move exact points. |
| Cost / risk | A union-find over the candidates after a box-tree search: O(n log n) (JUDGED). Risk: long chains in dense near-degenerate input. The width cap turns that into a visible defect, not a silent drift. |
| Check | R2 better. R3 holds: no point moves, and accuracy only records spread. R4 holds: the balls are recorded accuracies, not a new tolerance. Keeps the cellular merge; this feeds it. Evidence: an argument (transitivity) plus a logged measurement, not only a suite. |
| Verdict | **Adopt with changes** (the width cap, a member as the point). The pin's own pooling is to be read when `scx-boolean` is ported; if it is greedy, this is a change of result under R3 and needs truth tests. |

Note (corrected after review-60 §0): 01 §4.2 writes coincidence as `max(res, acc_a, acc_b)`, and 09
§3 wrote `resolution + acc₁ + acc₂`, claiming that is how the pin combines them. The pin's
`coincidence_tol_3d` is `contact.max(zone1).max(zone2)` (pin `geom/tol.rs`), and its comment records
why it left the sum: at the sum, a groove 1.5e-5 deep in a block at a 1e-5 tolerance comes out as no
groove (pin `docs/booleans.md` §5, §10). The sum survives only as the gap allowed at an intersection
(`xsect_tol_3d_from_zones`). `scx-num` already has `coincident = res.max(a).max(b)`. springrep's
`between` is `max(a + b − LINEAR, LINEAR)`, its own choice, not measured against the groove case.
The first draft recommended the sum; that was wrong. Coincidence stays `max`, and 09 §3 is fixed.

**T2. `within` with ulp slack.**

| | |
|---|---|
| Now | One of solidcore's known weaknesses is that derived checks are too tight far from the origin (task brief, READ). `scx-num` already has the helpers (corrected): `rounding_of_copies(size) = γ₃₂·|size|`, `coincidence_reach_3d`, `within_coincidence_tol`, `within_contact_tol`, `zone_reach_3d` (`crates/scx-num/src/tol.rs`, from the pin, READ). |
| Other | `within(d, t, at)` is `d ≤ t + 16 ε (1 + ‖at‖∞)`. What lies at exactly a tolerance then counts as within it, however its coordinates round (`springcraft-ai_springrep/src/tol.rs`, READ). |
| Why better | A copy moved by exactly the resolution lies the resolution plus a few ulps away. Without slack the test flips with the position, which breaks the metamorphic tests of 01 §9.2 (rigid moves) far from the origin (JUDGED). |
| Change | `scx-num`: `Resolution::admits(d, at: Point3)` (and the same on `coincident`) with the ulp term as a named constant. Every coincidence test goes through it. A clippy or xtask scan flags raw `d <= res.get()`. |
| Cost / risk | One add per test. Risk: an absolute floor of a few ulps hides nothing real at 1e-5 mm (16 ε × 1e3 mm ≈ 3.6e-12 mm, JUDGED). |
| Check | R2 holds: pure arithmetic. R3 better: a moved copy gets the same answer. R4 holds: it is rounding, not a tolerance. |
| Verdict | **Already covered** (corrected): keep only an xtask scan that flags raw `d <= res` comparisons outside the tolerance module. |

**T3. A tangency sine derived from the resolution.** springrep sets `TANGENT = LINEAR / RESOLVED`
with `RESOLVED = 0.1` mm: surfaces within `LINEAR` over 0.1 mm of their meeting are tangent
(`springcraft-ai_springrep/src/tol.rs`, READ). 01 §4.2 has no angle tolerance: coincidence angles
come from accuracy divided by size, and feature thresholds are operation parameters (READ, 04 row
42). `RESOLVED` is a length the user never set, standing in for "size", so `TANGENT` is a global
angle tolerance (JUDGED). **Reject.** The local form 01 already has (accuracy over the length the
contact runs) is the honest one.

**T4. A new vertex inside an operand vertex's ball takes that ball.**

| | |
|---|---|
| Now | New entities take `max(res, construction bound + input surface bounds)`. A raise comes only from a measurement and is reported (01 §4.2, 09 §6). Raises are legitimate only on import, to an enclosed gap (09 §6, READ). |
| Other | A new vertex lying inside an imported vertex's tolerance may take up to that tolerance, past the operation ceiling. Cutting the edge there adds no uncertainty, because the point already lies in that ball (`arris-labs_arris/docs/adr/0076-a-new-vertex-inside-an-operand-vertexs-tolerance-may-take-that-tolerance.md`, READ). |
| Why better | Without it, a fillet that ends inside an imported vertex's ball either fails or records a raise that is not a loss of accuracy. That turns real imports into refusals or warnings (JUDGED). |
| Change | 09 §6 gains a case: "an entity created inside an input entity's recorded ball inherits that accuracy as `Basis::Stated` (from the input). It is not a raise." |
| Cost / risk | None in code beyond a containment test. Risk: it could launder a bad construction. The rule applies only when the new point is inside the ball by measurement. |
| Check | R4 holds: no new tolerance, an existing record is reused. Helps healing of real imports (a guarded feature). Evidence: an argument and one real part (CTC-05), READ. |
| Verdict | **Adopt.** It also answers part of 09 Q5: such a vertex is not in `raised`. |

**T5. Kinds of tolerance named apart.** next.BREP's module comment sorts every tolerance into
identity, fit accuracy, weld or search radius, knot or parameter, angular, and strict-interior. It
records that coupling the base tolerance to part size broke intersection fitting (idea from
`mmiscool_next.BREP.io_RUST_BREP_KERNEL/BREP_geometry/src/geometry/tolerance.rs`, READ, not copied).
cadrs names its round-off margins by meaning: `REL`, `ANGULAR`, `ROUNDOFF`, `TINY`, `DEGENERATE`
(`rvdende_cadrs/crates/cadrs_kernel/README.md`, READ). 09 §3.1 already classifies every use of
"tolerance" (READ). What 09 lacks is the search radius as a type of its own: a sew or pool search
radius is a length that decides only which candidates to examine, never the answer (JUDGED).
**Adopt the naming:** `scx-num` gains `SearchRadius` beside `Resolution` and `Accuracy`, so a
search radius cannot be passed where a coincidence test is meant. The fit budget is already
`budget: Length` (09 §3.1).

**E1. A read gap closed by a recorded raise; the ceiling bounds operations, not bodies.** Arris
closes a read gap by growing the tolerance and records that it did. It then found that its checker
bounded tolerances by the operation ceiling, which made every healed body fail
(`arris-labs_arris/docs/adr/0058-a-read-gap-is-closed-by-a-grown-tolerance-and-the-read-records-it.md`,
`0060-max-tolerance-bounds-what-an-operation-grows-not-what-a-body-holds.md`, READ). 09 §6 already
allows raises only on import, to an enclosed gap, as a `Raise` (READ). `MAX_ACCURACY_RATIO` is a
validity limit (09 §3.1). **Not covered** (corrected): 09 §6 item 3 required the ratio to stay
within `MAX_ACCURACY_RATIO`, and CTC-05 needs 2.05e-2 at a 1e-5 resolution, a ratio of 2050, so 09
§6 refused the raise that heals a real import. Fixed in 09 §6 (section 6).

**E2. Export states the uncertainty the body holds.** geomsolver writes STEP "stating no tighter an
uncertainty than the file holds" (`bmander_geomsolver/rust/cad-kernel/README.md`, READ). 01 does
not say what `scx-step` writes as the uncertainty (READ). **Adopt:** write the largest recorded
accuracy, or the resolution if larger, as the context's uncertainty. Per-entity accuracies go
where AP242 allows. Truthful and cheap.

### 3.2 Booleans

**B1. Caller-stated facts, verified.**

| | |
|---|---|
| Now | `scx-boolean` takes two bodies (01 §1.1). Operations that build a tool body already know how it meets the target: a fillet's strip, a pocket's walls on a planar face (JUDGED). |
| Other | `Facts { on_face: [OnFace { side, edge, face }] }`: edge E of one input lies in face F's surface on the other. The Boolean takes the incidence instead of computing it, checks it at the ends and quarter points, and refuses a false fact with `InvalidInput` (`springcraft-ai_springrep/src/topo/facts.rs`, `doc/design.md`, READ). |
| Why better | Near-tangent and coincident incidences are where intersection is least reliable. The caller knows them exactly. Verification keeps the Boolean honest (JUDGED). |
| Change | `scx-boolean` options gain `facts: &[Incidence]`. A false fact is `Refused(FalseFact { … })`. Verification reads each fact at sample points within `coincident(res, acc…)`. A verified fact has `Basis::Certified` only when the caller's construction certifies it; otherwise it is `Measured` at the samples. |
| Cost / risk | Small API surface. Risk: sample verification is not a proof, so a fact true at five points and false between them is accepted. Mitigation: the verification uses the certified gap bound of research 55 where one exists. |
| Check | R6 holds: the facts are an input, read only. R3 holds: a fact is a statement about exact geometry. It keeps the cellular merge, since facts seed the imprint. Evidence: an argument. |
| Verdict | **Adopt with changes**: certified verification where available, typed refusal. |

**B2. Imprint in dimension order into one pool.** springrep's imprint runs VV, then edge on face,
edge on edge, face on vertex and face on face, then places hits, splits curves at vertices and
deduplicates (`springcraft-ai_springrep/doc/design.md`, imprint order, READ). Arris and geomsolver
apply their merge rules phase by phase in a fixed order
(`bmander_geomsolver/rust/cad-kernel/src/tolerant.rs`, READ). 01 §11.2 keeps solidcore's Boolean
and the cellular merge (`boolean/merge/p4.rs`), and does not state an imprint order (READ). It is
judged that solidcore's topology solver already works in such an order; that is to be confirmed at
the port. **Adopt** as a stated invariant of the `scx-boolean` spec: lower-dimension incidences are
settled first and feed T1's pool. This is a statement of order, not a new algorithm (R5).

**B3. Tangent contact precedence.** springrep settles a tangent contact from stated facts first,
then an exact closed form, then numeric tracing (`springcraft-ai_springrep/doc/design.md`, READ).
geomsolver: "A meeting the kernel cannot resolve is refused, never read as none"
(`bmander_geomsolver/rust/cad-kernel/README.md`, READ). 01 keeps solidcore's SSI including the
B-spline patch SSI (01 §1.1, `scx-geomq`; `scx-ssi` in the 31-crate graph). **Adopt** the
precedence and the rule. An unresolved contact is `Unsupported` or `Exhausted`, never an empty
intersection. Freeform SSI stays the last resort, which keeps the guarded feature.

**B4. N-ary Booleans as one decomposition.**

| | |
|---|---|
| Now | 01 §1.1 lists union, subtract and intersect. No n-ary entry point (READ). Patterns are 82% intermediate bodies (01 §2.4, MEASURED there). |
| Other | `cut_many` and `fuse_many` run one decomposition over the target and every tool. Each face pair is intersected once and each section vertex made once. Each piece is classified against every operand. `cut` and `fuse` are the one-tool case, bit for bit (`arris-labs_arris/docs/adr/0050-the-multi-tool-boolean-is-one-decomposition-over-n-operands.md`, READ). The contract asks for `union_all` and `difference_all` (`brep-conformance/doc/api.md`; B28 is in `doc/contract.md`, READ). |
| Why better | A chain pays N splittings and leaves intermediate ids in provenance. One decomposition names each tool in `Changes` (JUDGED). The cellular merge already holds regions, so N operands map onto regions with an operand set per region (JUDGED). |
| Change | `scx-boolean`: `union_all(&[Placed])`, `subtract_all(&Brep, &[Placed])`; `Changes` origins carry an operand index. The two-operand calls become the n = 2 case. |
| Cost / risk | Moderate: the region labelling generalises from two flags to a set. Risk: the cellular merge's two-operand assumptions run deep in `p4.rs`, to be measured at the port. |
| Check | All rules hold. It builds on the cellular merge with regions instead of losing it. |
| Verdict | **Adopt.** Port two-operand first; n-ary is the first extension. |

**B5. Split: both sides from one decomposition; either may be empty.** Arris splits by a plane with
one decomposition and two selections. Each side owns its section edges and cap. A body entity the
plane misses keeps its id on its side
(`arris-labs_arris/docs/adr/0051-split-by-a-plane-is-one-decomposition-with-two-selections.md`,
READ). The contract's `split` returns two parts, either possibly empty (`brep-conformance/doc/api.md`; B34 is in
`doc/contract.md`, READ). One of solidcore's known weaknesses is that a split that misses returns the whole body
twice (task brief, READ). 01 §1.1 has "split by surface or sheet" with no result contract (READ).
**Adopt:** `split` returns `Split { above: Option<Brep>, below: Option<Brep> }` (or the empty body,
if `Brep` gets one), from one arrangement, with `Changes` per side. This fixes a pin defect under
R3.

**B6. Same-body shortcut and shared operands.** springrep shortcuts A op A
(`springcraft-ai_springrep/src/boolean/same.rs`, READ). Arris copies a later operand that shares
entities with an earlier one, then maps the record back to the caller's ids
(`arris-labs_arris/docs/adr/0087-an-operand-sharing-entities-with-an-earlier-one-is-decomposed-as-its-copy.md`,
READ). In solidcorex, two `Brep`s can share `Arc`'d face records after an edit (01 §2.5, READ), so
"the same face record in both operands" is a real case. The ids of the two bodies are separate
spaces (JUDGED). **Adopt:** A op A, and A op (an edit of A), are tested cases. A shared `Arc`
record is a fast exact-coincidence fact (B1) for free, never an aliasing bug.

### 3.3 Validity and errors

**V1. A cheap check on every result.**

| | |
|---|---|
| Now | `finish` checks the structure of what the draft touched: loops closed, uses consistent, sides and regions, radial orders, orientation. Geometric validity is in the check crate, which only reads (01 §2.3). "Validity of every result" is a test oracle (01 §9.2, READ). |
| Other | cadrs runs a fast check on every result: references, loops closing in 3-D and in (u, v), p-curves on their edges, uses, body kinds, positive volume. Its full check is for tests (`rvdende_cadrs/crates/cadrs_kernel/README.md`, Validity). anvil-brep's rule: "Every solid produced by an operation passes `check::validate` before it is returned" (`swperb_anvilcad/crates/anvil-brep/src/lib.rs`, READ). springrep checks every result and returns `InvalidResult` (`springcraft-ai_springrep/doc/design.md`, READ). |
| Why better | The structural check cannot see a loop that closes topologically but whose curves leave a gap larger than its recorded accuracy. That is the cheapest geometric defect to detect and among the most common an operation makes (JUDGED). |
| Change | `finish` also checks, for touched records only: each loop's consecutive edge ends meet within `coincident(res, acc…)`, and each p-curve's ends map to its edge's vertices within the same. The full check stays in `scx-check`. A failure is `Unfinished` with a geometric cause. |
| Cost / risk | Two evaluations per touched edge use: small against any operation (JUDGED); measure at the vertical slice. Risk: `scx-brep` must evaluate curves and surfaces, which it can at layer 6 in the 31-crate graph. |
| Check | R6 holds: `finish` already consumes the draft. R3 better: a broken result never escapes. Nothing guarded is lost. Evidence: an argument plus three kernels' practice. |
| Verdict | **Adopt with changes** (in `finish`, touched records only). Owner decision 4. |

**V2. No crossing faces after a Boolean.** cadrs checks that no two result faces cross away from
their shared edges, and returns `BooleanFailed { stage: "crossing" }` instead of a flagged body
(`rvdende_cadrs/crates/cadrs_kernel/README.md`, Tolerances; `rvdende_cadrs/crates/cadrs_kernel/src/native/check_faces.rs`, READ).
springrep's check uses an edge-vs-face search (`springcraft-ai_springrep/src/check.rs`, design doc,
READ). 01 §11.1 avoids "the check writes the body" (READ), and `scx-check` reads. **Adopt:** the
face-crossing test is a named, separately callable part of `scx-check`, run in tests on every
Boolean result. Whether `scx-boolean` calls it at runtime is a performance question for 01 §9.4. A
crossing found there is a `Defect`, never a returned body.

**Typed refusal of must-refuse input.** The contract lists inputs that must be refused: a
self-crossing profile, a hole outside its loop, a torus tube wider than its ring, zero sizes and
others (`brep-conformance/doc/api.md`, READ). 01 §5 has `Refused` per crate (READ). **Already
covered** by the type; each operation's spec should list its own refusals from this table, so they
are tested by name.

### 3.4 Determinism and budget

**D1. A total budget metered in index order.**

| | |
|---|---|
| Now | The budget is in deterministic units. It is split per item in advance, so which item runs out never depends on scheduling (01 §6.3, §8.2; doc 07 rule 7, READ). |
| Other | A `Control` holds a poll and a step budget, passed last. A parallel pass gives each item a meter capped at the budget left when the pass starts. Afterwards the items' steps are summed in index order, and the pass stops at the first item whose running sum crosses the cap. The serial build uses the same rule, so both stop on the same item with the same count (`arris-labs_arris/docs/adr/0030-cancellation.md` §3-4, READ). |
| Why better | A split in advance has to guess each item's share, so one heavy item can run out while the others leave budget unused. The total budget then means less than it says (JUDGED). Arris's rule gives one meaning to "budget N" under every executor. It also equals the serial answer. |
| Change | 01 §6.3: "The budget is a total. A parallel pass meters each item, capped at what is left, and the result is the serial order's: the first item whose running sum crosses the cap is where it stops." |
| Cost / risk | Some wasted work past the stopping item in parallel (JUDGED). Risk: none for determinism, provided `work` sums in index order. |
| Check | R2 holds, R6 and R7 hold (the meter is per item, in `Cx`). |
| Verdict | **Adopt with changes**: owner decision 1, since it changes a 01 rule. |

Arris also routes every transcendental through one pure-Rust `libm`, banned in `std` form by clippy
(`arris-labs_arris/docs/adr/0064-transcendentals-go-through-one-pure-rust-libm.md`, READ).
**Already covered** by `fmath` and 01 §8.3. Arris also implements `powi` as repeated squaring; 01
§8.3 bans `powi`, which agrees.

**D2. Fuzzed readers and checked constructors.** cadrs fuzzes every reader and the B-spline
constructors with proptest: nothing may panic (`rvdende_cadrs/crates/cadrs_kernel/tests/fuzz_readers.rs`,
README, READ). B-splines from outside go through checked constructors (knots, degree, finite poles,
positive weights). 01 §5 makes errors typed (READ), but 01 §9 has no fuzzing line (READ).
**Adopt:** `scx-testkit` fuzz targets for `scx-record`, `scx-p21`, `scx-step`, `scx-dxf` and the
`scx-nurbs` constructors, run nightly.

cadrs also hashes results on 1 and 4 threads (`rvdende_cadrs/crates/cadrs_kernel/tests/fingerprint.rs`, READ). **Already covered**
by 01 §8.3 (1 and 3 workers, record hashes).

### 3.5 Queries

**Q1. Distance by certified critical points.** Arris's distance treats a body as its point set.
It returns one pair, the least by a total key (entity ids, then parameters), and sets
`unique: false` when the minimum is a continuum or a rival lies within tolerance. It finds
stationary pairs of the untrimmed carriers, then recurses into boundaries; it never samples
(`arris-labs_arris/docs/adr/0054-the-minimum-distance-is-between-point-sets-by-certified-critical-points.md`,
READ). cadrs uses branch and bound on cell boxes, then Newton (`rvdende_cadrs/crates/cadrs_kernel/README.md`,
READ). 01 has distance in `scx-bodyq` and certified gap bounds in `scx-geomq` (01 §1.1, READ),
without an output contract. **Adopt** the output contract: a total tie-break key, `unique`,
`contained` (point inside), and a certified bound. The algorithm is solidcore's (R5).

**Q2. Ray hits name the most specific entity.** Arris returns one hit per vertex, edge or face,
deduplicated along it, so parity from outside is always even. The normal at an edge hit is the
lowest-id face's. A tangent hit is flagged and never counts for parity. A ray lying in a face is
refused (`arris-labs_arris/docs/adr/0053-a-ray-hit-names-the-most-specific-entity.md`, READ). The
contract asks for a ray batch (`brep-conformance/doc/api.md`; B26 is in `doc/contract.md`, READ). 01 has ray
classification in `scx-bodyq` (READ). **Adopt** the contract, including `raycast(&[Ray])`, which
shares the body cells across the batch.

**Q3. Interference.** Arris: `Apart(distance)`, `Touching { contact }` or
`Overlapping { volume, a_in_b, b_in_a }`. The verdicts are read from the Boolean's own rules, so
the volume is the common's to the bit
(`arris-labs_arris/docs/adr/0055-interference-is-distances-contact-and-commons-volume.md`, READ). The
contract asks for `interference { volume, gap }` (B27, READ). 01 lists "contact" in `scx-bodyq`
(READ), which sits below `scx-boolean`. **Adopt**, homed in `scx-boolean`, since it builds the
common. `scx-bodyq` keeps the distance half.

**Q4. A certified tight box.** Arris separates a loose tolerance envelope, for broad phase, from a
tight box whose sides lie within the linear tolerance of reached geometry. The tight box uses
Bernstein hulls, lifted trims and interior extrema, and returns `BoundsUncertified` when unsure
(`arris-labs_arris/docs/adr/0093-a-tight-box-is-outer-within-the-linear-tolerance.md`, READ). The
contract has a least-box clause (B33, READ). 01 has a body box cell (§3.1, READ). **Adopt:** the
cell stays loose and conservative; `scx-bodyq::tight_box` is a separate certified read.

**Q5. Betti numbers.** springrep computes them from χ (`measure::betti`, READ). anvil-brep has
`check::genus` (`swperb_anvilcad/crates/anvil-brep/src/check.rs`, READ). The contract's B19 asks for
the point set's Betti numbers (READ). **Adopt** in `scx-bodyq`: b0 from solid regions, b2 from
voids, b1 from χ per region. The region model (01 §2.1) makes b0 and b2 direct reads, which the
small kernels must reconstruct (JUDGED).

**Q6. Sliver and thin-place finder.** geomsolver finds faces thinner than a bar and edges shorter
than it, with the faces beside them (`bmander_geomsolver/rust/cad-kernel/src/sliver.rs`, READ).
springrep's `probe` finds narrow faces and thin places (`springcraft-ai_springrep/src/probe.rs`,
module list, READ). 01 has sliver and short-edge repair in `scx-repair` (READ), but no read-only
finder. **Adopt** in `scx-check`, as a read that `scx-repair` calls. An app can then show slivers
without repairing them, which keeps 01 §11.1's split between checking and writing.

**Swept contact.** springrep's `overlap::swept_contact` finds the first contact when a body moves
along a direction (`springcraft-ai_springrep/src/overlap.rs`, READ). 01 has nothing like
it (READ). Listed as a completeness gap (section 4). Not a design change.

### 3.6 Transforms

**X1. Mirror as its own type.** Arris keeps `Isometry` proper and adds `Reflection`. Each surface
kind's image is the same kind over a right-handed frame: quadrics map `u ↦ 2π − u`; plane and NURBS
keep their parameters and flip the normal against the reflected one. The seam stays at `u = 0`
(`arris-labs_arris/docs/adr/0031-mirror.md`, READ). The contract asks for similarity including
mirrors (B25, B29, READ). 01 §2.4 had `transform(&Brep, Rigid)` only (READ), but the pin already
mirrors and transforms by any regular affine matrix (corrected): `Body::transform`
(`solidcore-ops/src/body.rs`) and `mirror` and `transform_matrix` (`solidcore/src/topology/brep/mirror.rs`:
an SVD into scale and rotation, a mirror where a factor is left-handed, edge uses reversed and shells
swapped for a reflection; non-similarity images become NURBS; tolerances scale), READ. Under R5 the
mirror is the pin's. **Adopt:**
`scx-num::Similarity { rigid, scale, mirror: bool }`, and
`scx-brep::transform(&Brep, Similarity) -> (Brep, IdMap)`. A mirror reverses the sense of plane and NURBS faces; quadrics keep their sense and their loops
are reversed (ADR-0031 §2 table; corrected, the first draft said every face). 01 must say whether
ids are kept: they can be, as `Touch::Geometry` plus
reversed sense (JUDGED). Uniform scale scales recorded accuracies by the same factor, as 01 §9.2's
metamorphic test already assumes.

**X2. Affine images.** geomsolver maps a solid by any invertible affine map. Kinds closed under the
map are written as themselves; others are wrapped as `Mapped` and met in their preimage, where the
closed forms are (`bmander_geomsolver/rust/cad-kernel/src/affine.rs`, READ). The contract asks for
affine stretch and shear (B32, READ). 01 has affine transforms in `scx-num`, and volume maps with
images under them in `scx-surface` (research 56 §5.1, READ), but no affine body transform. The pin
has one (`transform_matrix`, above; corrected).
**Adopt with changes:**

- `scx-deform::affine(&Brep, Affine)`:
  - NURBS take the exact image;
  - a plane stays a plane;
  - a quadric that maps to a quadric kind solidcore holds takes that kind;
  - anything else becomes an image under a volume map that already exists.
- Accuracies scale by the map's largest singular value.

Owner decision 2: whether the surface kinds gain a general `Mapped` wrapper, or affine stays inside
`scx-deform`'s volume images.

### 3.7 Offsets and blends

**O1. A rounded offset by slabs, tubes and balls.**

| | |
|---|---|
| Now | `scx-offset` ports solidcore's offset, which offsets faces and re-intersects them (01 §1.1, 11.2). solidcore has about 70 offset refusals (task brief, READ). |
| Other | A rounded offset is the body ∪ each face's slab ∪ a tube round each convex edge ∪ a ball at each vertex. An inset is the body less the inward slabs, tubes and balls. A shell's cavity is built from its definition the same way (`rvdende_cadrs/crates/cadrs_kernel/README.md`, `local`, `offset_round`; `bmander_geomsolver/rust/cad-kernel/README.md`, Offsets, READ). |
| Why better | It is the exact Minkowski sum with a ball, which is the true rounded offset by definition. Every piece is an exact surface: an offset surface, a pipe, a sphere (JUDGED). |
| Change | `scx-offset` gains `Join::Round` beside the sharp join. The sharp join keeps solidcore's algorithm. Round is never a fallback for sharp, because it is a different shape, and a silent swap would break R3. |
| Cost / risk | Many Booleans with tangent contacts: a tube meets its two slabs tangentially along whole edges, the hardest case for the Boolean. B1's facts state those contacts exactly, which is what makes this tractable (JUDGED). Cost per body is O(faces + edges + vertices) Booleans, unless done as one n-ary union (B4). |
| Check | R5 holds: sharp offsets stay solidcore's. R3 holds: each mode is the true shape of its definition. Evidence: two kernels' practice and an exact definition. |
| Verdict | **Adopt with changes**: a second mode, built on B1 and B4, never a fallback. |

cadrs falls back from a local fillet to a tool-body fillet subtracted by Boolean
(`rvdende_cadrs/crates/cadrs_kernel/README.md`, `fillet`, `blend`, READ). A fallback that builds
the same rolling-ball surface by another route is legitimate under R3. One that changes the
section, or drops the overflow handling, is not. **Reject as a plan item:** solidcore's rolling
ball with overflow is a guarded feature, and the fallback's only evidence is a suite count.

### 3.8 Provenance

**P1. Guaranteed split order.** Arris ships no name grammar. Its contract is the order of an
origin's outputs: pieces ascend by a key built from the input entities bounding them, then by an
interior (u, v) point with parametric ties counted equal
(`arris-labs_arris/docs/adr/0009-no-name-grammar-a-guaranteed-split-order.md`, READ). 01 §2.6 has
`Changes` with `Origins` and `carry`, and `scx-fates` follows names (READ). It does not say how two
pieces of one face are ordered (READ). **Adopt:** `Changes` lists a split's pieces in that order.
The key reads input ids only, so a rebuild that keeps the inputs keeps `Split(k)`. This also makes
`carry` deterministic across executors (R2).

### 3.9 Already covered, briefly

| Idea | Source | Where 01 has it |
|---|---|---|
| Immutable bodies; edits build new ones | springrep `doc/design.md`, cadrs README | 01 §2.3 `Brep` / `Draft` |
| Face memos carried to untouched result faces | springrep `src/topo/memo.rs` | 01 §2.5 `Arc` sharing, cells included |
| An edge's samples a function of the edge alone, so cached face meshes stay watertight | arris ADR-0052 | 01 §3.2 rule 2: edge cells read only their own geometry and vertices. Add a test: a kept face's mesh meets a re-meshed neighbour bit for bit |
| Touching results: edges with 2k uses, vertex loops | springrep `doc/design.md` | 01 §2.1 radial order, regions, laminas, lone-vertex loops |
| Persistent names from history | `rvdende_cadrs/crates/cadrs_kernel/src/naming.rs`, springrep `FaceName` | 01 §2.6 `Changes`, `carry`, `scx-fates` |
| Kernel-reported work counters | brep-conformance `doc/contract.md`, known limits | 01 §5.2 `Report::work` |
| Cancellation by poll | arris ADR-0030 | 01 §6.3 `Stop` |
| Recognition of imported B-splines as analytic surfaces | `rvdende_cadrs/crates/cadrs_kernel/src/native/xbuild.rs` | 01 §1.1 `scx-geomq` recognition. The fit deviation is a `Measured` accuracy under 09 §3 |
| A size box beyond which a double no longer resolves the tolerance | cadrs `tol::SIZE` | 01 §4.3 size class and `MAX_ACCURACY_RATIO` |

## 4. Completeness list

These are operations and queries that the contract or these kernels offer and that 01 §1.1 and
§1.3 do not name. Each is given the crate it would belong to in the ruled 31-crate graph (research
56 appendix C.4, 04 §3). "Pin has it" means solidcore has an algorithm to port (JUDGED from the
task's list of what solidcore lacks; to be confirmed at each crate's start, 01 §1.3).

| Gap | Asked by | Crate | Pin has it | Note |
|---|---|---|---|---|
| Betti numbers | contract B19; springrep, anvil-brep | `scx-bodyq` | no | Q5 |
| Interference (apart, touching, overlapping, volume) | contract B27; arris ADR-0055 | `scx-boolean` | partly (contact) | Q3. Needs the common and a `scx-boolean` → `scx-mass` edge |
| Certified tight bounding box | contract B33; arris ADR-0093 | `scx-bodyq` | yes (corrected): 01 §1.1 has the tight face and body boxes; the kept box is held out (04 §3, 11-24) | Q4, covered |
| Ray batch | contract B26; arris ADR-0053 | `scx-bodyq` | ray firing yes, batch no | Q2 |
| Maximum distance between bodies | `rvdende_cadrs/crates/cadrs_kernel/src/native/distance.rs` | `scx-bodyq` | no | Branch and bound like the minimum |
| Swept contact along a direction | springrep `src/overlap.rs` | `scx-bodyq` (uses `scx-ssi`, an edge C.2 already needs) | no | |
| Sliver and thin-place finder | geomsolver `sliver.rs`, springrep `src/probe.rs` | `scx-check` | as repair inputs | Q6 |
| n-ary union and cut | contract B28; arris ADR-0050 | `scx-boolean` | no | B4 |
| Cells of an arrangement of solids | `rvdende_cadrs/crates/cadrs_kernel/src/native/cells.rs` | `scx-boolean` | yes, internally (regions) | Expose the cellular merge's regions |
| Split with both sides, either empty | contract B34; arris ADR-0051 | `scx-boolean` | yes, with the double-body defect | B5 |
| Unify same-surface faces as a public operation | springrep `src/boolean/unify.rs`; `rvdende_cadrs/crates/cadrs_kernel/src/native/unify.rs` | `scx-repair` | inside the Boolean and stitch (pin `boolean/merge/p3.rs`, `heal/stitch/p1.rs`) | Port the pin's code, not another kernel's |
| Mirror and similarity | contract B25, B29; arris ADR-0031 | `scx-brep` (`transform`) | yes (corrected): `mirror`, `transform_matrix` | X1 |
| Affine stretch and shear | contract B32; geomsolver `affine.rs` | `scx-brep` (`transform`) | yes (corrected): `transform_matrix`, non-similarity images as NURBS | X2; section 6 |
| Rounded offset (join round) | cadrs, geomsolver | `scx-offset` | no | O1 |
| Screw (helical) sweep | contract `screw`; cadrs `sweep` (helix) | `scx-make` | sweep yes, exact screw no | |
| Sweep along a path with frames | contract `sweep_path` | `scx-make` | sweep yes | The task lists `sweep_path` as missing at the pin's ops seam |
| Convex hull of points | geomsolver README | `scx-make` | no | |
| G2 fillet, G2 setback | contract B39 | `scx-blend` | yes (corrected 11-27, research 61): the Gⁿ section, `blend/fillet_section/`; G2 Setback corners in `blend/blend_setback/` | Ported (research 61 §7) |
| Conic fillet | contract `fillet_conic` | `scx-blend` | no | Adopted as new work (research 61 §7) |
| Setback vertex blend | contract `fillet_setback` | `scx-blend` | yes (corrected): `blend/blend_corner.rs` | Name the setback form in the spec |
| Full round | contract `full_round`; cadrs `fillet` | `scx-blend` | yes (corrected): two rounds whose rails meet (`blend/fillet_geom/p1.rs`, test `a_full_round_whose_rails_meet_along_part_of_one_of_them`) | |
| Chamfer by distance and angle | springrep `blend` | `scx-blend` | yes (corrected): `blend/blend_refusal.rs`, `ChamferAngleTooLarge` | |
| Section area | contract B21 | `scx-mass` on the section sheet `scx-boolean` returns (corrected: `scx-bodyq` sits below `scx-boolean`) | sections yes | A read on the section sheet |
| Display mesh with edge flags (the faces each edge joins, crease or smooth, seam, split) | contract B31, B42 | `scx-tess` | faceting yes | Output contract of `scx-tess` |
| Sew from text with a tolerance | contract `sew` | `scx-stitch` | stitch yes | Under 09 §6: the tolerance is a search radius (T5), and gaps closed are recorded raises |
| Thicken a sheet | contract `thicken` | `scx-offset` | listed in 01 | covered |
| Curvature at a point | contract B37 | `scx-bodyq` (via `scx-surface`) | evaluation yes | A read |
| Project a curve onto a body | contract `project` | `scx-bodyq` (corrected: already there in 01 §1.1, with the `scx-bodyq` → `scx-ssi` edge) | projection yes | covered |

Also offered by these kernels, judged out of scope for the core:

- cadrs's generated cuts (`sweep_generate`: gear hobbing) belong with `scx-teeth` if wanted.
- cadrs's evolved solids are niche.
- cadrs's GPU spike is a platform question for later.

## 5. Changes to 01 that need an owner decision

1. **Budget semantics (D1).** 01 §6.3 splits the budget per item in advance. The proposal makes it a
   total, metered per item and summed in index order. Both are deterministic. The proposal gives
   "budget N" one meaning under every executor, at the cost of some wasted parallel work.
   Recommendation: adopt.
2. **Affine transforms (X2).** Option (a): `scx-deform::affine`, using exact images where a kind is
   closed and volume-map images otherwise. Option (b): a `Mapped` surface kind in `scx-surface`, as
   geomsolver does. (b) touches every surface match, so it is a 7.1 closed-enum change across
   crates. Recommendation: (a).
3. **G2 and conic fillets.** The contract asks for them and solidcore has none. Building them means
   new algorithms, outside "keep solidcore's algorithms" (R5). Recommendation: leave them out of
   01 until the blend port lands, then research them on their own.
   - **Corrected 11-27 (research 61).** The premise was wrong for G2. solidcore has G2/Gⁿ (orders
     2-4, certified), the new fillet system's section (the fork's step 3), and it is ported with
     the blend. Only the conic is new; it is adopted as new work (research 61 §7).
4. **Cheap geometric checks in `finish` (V1).** This adds loop-end and p-curve-end gaps for touched
   records to `finish`. The alternative is a `scx-check::fast` that each operation calls. The first
   is impossible to forget; the second keeps `scx-brep` free of evaluation. Recommendation: in
   `finish`.
5. **Two operand vertices in one merge component (T1).** (Corrected.) The first draft said this
   means an operand edge shorter than the coincidence distance, which `LARGE_SMALL_SIZE_RATIO`
   already fails. Both halves were wrong: a chain of candidates can join two vertices that are
   neither within coincidence of each other nor joined by an edge, and `LARGE_SMALL_SIZE_RATIO`
   is 1000 × res, the small-topology zone, not the coincidence distance. Section 6 has the ruled
   answer.

Smaller 01 edits that need no decision, only writing in:

- 01 §4.2 and 09 §3 stated coincidence two ways (T1's note). Corrected: 01's `max` is right, as at
  the pin; 09 §3 is fixed.
- Named crate contracts:
  - B5's split result;
  - P1's split order;
  - Q1-Q4's query outputs;
  - E2's export uncertainty;
  - D2's fuzz targets in 01 §9.

## 6. Ruling (11-27)

The owner ruled on every item on 11-27, after review-60. Where an item is adopted with the review's
changes, the review's version is the one adopted. The rulings are written into 01, 07, 09 and 04 §3.

**Tolerance.**

- **Coincidence** stays `max(res, acc_a, acc_b)`, as at the pin. 09 §3's sum was wrong and is
  fixed. The sum is only the gap allowed at an intersection.
- **T1** is adopted with the review's changes:
  - the relation is under `max`;
  - components are numbered in item-index order;
  - an operand vertex in a component is the merged point;
  - the spread is recorded as accuracy with a warning, not a `Defect`;
  - vertex balls grow to cover the edge and p-curve ends the merge moves.
- **Decision 5** (two operand vertices in one component):
  - direct case (the two are within coincidence of each other): `Refused`, naming both vertices,
    and `scx-check` reports the same condition;
  - chain case: each candidate is anchored to its nearest operand vertex; when that does not
    settle it, `Unsupported`.
- **T2** is covered already (`scx-num` `tol.rs`). Adopted only: an xtask scan that flags raw
  `d <= res` comparisons outside the tolerance module (01 §8.3, 07 §6, planned).
- **T4** is replaced by the review's version: a vertex inside an operand vertex's ball inherits its
  accuracy with `Basis::Measured`, and no `AccuracyPastResolution` warning is raised when no
  accuracy is lost.
- **T5**: a `SearchRadius` type beside `Resolution` and `Accuracy`.
- **T3** rejected.

**Booleans.**

- **B1–B6** adopted:
  - B2 states springrep's actual order (section 1.1);
  - B4's n-ary result maps onto the regions of the cellular merge;
  - B5's split returns both sides, either of which may be empty;
  - B6 means sameness within tolerance, plus shared `Arc` records as a free exact fact.

**Validity and errors.**

- **V1** (decision 4): the cheap checks go in `finish`:
  - curve and p-curve ends are checked against vertex balls;
  - `Unfinished` gets a geometric cause;
  - every writer records measured gaps before `finish`.
- **V2** adopted.
- **Result contract**: defined in `scx-check`, not in `scx-brep`. The pin proves outwardness on
  every result (`finish_proved`). Placement: `scx-check::contract` holds it, and it is applied
  where a result leaves the kernel (`scx-features` for feature results, the app for direct
  operation calls). Operations do not call it, so no new graph edge is needed (01 §2.3).
- **R-9** stays rejected, with the corrected reason in section 1.2.

**Determinism and budget.**

- **D1** (decision 1): one total budget, metered in item-index order. Work counts must not depend
  on cache warmth: a memo or cell hit charges the same work as computing it. The advance split is
  dropped.
- **D2** adopted.

**Queries.**

- **Q1, Q2, Q3, Q5, Q6** adopted.
- **Q4** dropped as covered. The tight box is certified and not kept.
- **Section area** goes in `scx-mass`, on the section sheet.
- **Project onto a body** is already in `scx-bodyq`.
- **Interference** goes in `scx-boolean`, with a new `scx-boolean` → `scx-mass` edge. There is no
  cycle: `scx-mass` is layer 5.

**Transforms** (X1, X2, decision 2):

- the pin's `transform_matrix` (`topology/brep/mirror.rs`) is ported as `scx-brep`
  `transform(&Brep, Affine)`;
- kinds closed under the map stay themselves;
- the others get an exact rational NURBS image;
- procedural kinds get an approximation with recorded accuracy;
- there is no `Mapped` kind;
- mirrors follow the review: quadrics keep their sense and reverse their loops.

**Offsets and blends.**

- **O1**: the rounded offset is a second join mode, never a fallback.
- **G2 and conic fillets** (decision 3) are deferred. The pin already has the distance-and-angle
  chamfer, setback corners and the full round; the completeness rows are corrected.
  - **Corrected 11-27 (research 61; 04 §3 supersedes the deferral).** G2 is not deferred: it is
    solidcore's new-system Gⁿ section and is ported. The conic is adopted as new work.
    `scx-blend` ports the new fillet system only (research 61 §7).

**Exchange and provenance.**

- **E1**: 09 §6's `MAX_ACCURACY_RATIO` must not refuse a real healed import (CTC-05, ratio 2050).
  On imports it becomes a warning. It stays a validity limit for results of our own operations.
- **E2, P1** adopted.

**Rejects.** All stand (T3, R-1 to R-9). R-2 is corrected: springrep's `SNAP_GRAZING` is 1e-4.
