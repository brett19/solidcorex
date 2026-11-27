# 74 - Step 6: the idiom hunt ahead of widening bodies

Written for solidcorex, 2026-11-27. Step 6 of 01 §10.2 widens bodies:

- the full `scx-bodyq` (contact, continuity, pick);
- `scx-check`;
- `scx-stitch`;
- `scx-hlr`;
- `scx-step`.

It is done when the STEP corpus imports and passes the check, and when drawings work.

This note does for step 6 what research 63 did for steps 2 and 3 and 72 did for step 4. Steps 5 and 6
overlap, so every builder task below names the step-5 rows it waits on. A task that waits on nothing
can start now. For each crate the note gives:

1. what the pin does, and its size;
2. the idiom hazards, with file and line;
3. the proposed Rust shape;
4. the truths and tests, and where the pin is probably wrong;
5. the performance traps.

§7 is the STEP corpus and how "imports and passes the check" is scored. §8 is the task table. §9 holds
the open questions. §10 is the case against this plan. §11 gives the proposed 04 rows.

- **Labels** (as in 57, 63 and 72):
  - READ: read from the pin's code or docs.
  - MEASURED: counted or run here; the method is given.
  - JUDGED: a design judgement.
- **Pin paths** are relative to the pin kernel's `src/` unless they name another pin crate
  (`solidcore-step`, `solidcore-convert`, `step-export`). Line numbers are at the pin commit of 63.
- **Nothing here is measured on the corpus.** The NIST files are fetched at test time (research 58)
  and were not on the machines this note ran on, so §7 has no pass rates. The pin has none either.

## 0. How the counts were made (MEASURED)

- Line counts are from `wc -l` on the pin's files, tests included where a file holds them.
- Hazard counts are from grep for `&mut Brep`, `&mut Kernel`, `unwrap_or`, `.ok()`, `Err(_) =>`,
  `AtomicU`/`AtomicUsize` counters, `set_editing_enabled`, and float literals below 1e-2 outside
  tests. A grep count is an upper bound on the real hazards. Each one cited by line was read.
- Sizes of the crates that already exist on main (`scx-bodyq` 3,266 lines, `scx-check` 1,240,
  `scx-p21` 12,395) are from `wc -l crates/*/src/**/*.rs`.

| Pin piece | Lines | Notes |
|---|---:|---|
| `hlr/` (SolidCraft-derived) | 7,527 | own portable Brep, typed `HlrError`, no `&mut` |
| `blend/hcr.rs` + `api_projection.rs` + `surface_silhouette.rs` (SmHCR) | 4,302 | 31 `&mut Kernel` |
| `eval/outline.rs` + `pieces.rs` | 901 | clean |
| `query/` contact, distance bound, continuity, tangent edge, normal, edge turn | 8,966 | certified, Tangency-authored |
| `prism/` measure + recognition + tests | 2,261 + tests | 24 `.ok()?` |
| `hole_stacks` | 1,013 | absolute mm constants |
| `curve_class` (curve on face, used by the Boolean) | 27,873 | |
| Check (`ValidatePointers` + `AssertValid`, labels) | ~600 numbered tests; `labels.rs` 3,207 | 64 `&mut Brep`, 50 `&mut Kernel`, ~57 swallows |
| `heal/stitch` p1 + p2 + `bounds.rs` | 3,676 + 2,458 + 2,515 | `Stitch` has 9 bools |
| `solidcore-step` | 24.7k | live reader and writer |
| `step-export` | 7.8k | legacy, goldens only |

## 1. Shapes common to step 6, and the rulings

### 1.1 What the pin does on every path (READ)

- **Reads that write.** Classification, tight boxes, the check and stitch take `&mut Brep`. The
  mutation is lazy trims, raised tolerances (#18065 raises edge tolerances during a read) and memo
  caches. The heaviest users are classification p7 (60), p9 (47), p1 (31) and `query/face/p2` (29);
  the check has 64.
- **Samples taken as proofs.** Some of the pin's answers are proved and some are sampled. The proved
  ones are the Tangency-authored queries (`contact.rs`, `distance_bound.rs`, `tangent_edge.rs`,
  stitch `bounds.rs`, `planar_disjoint.rs`, `tolerance_defects.rs`). The check, stitch acceptance,
  orientation and HLR visibility are sampled:
  - edge/face gap: 20 projections (`calc_max_edge_face_gap`, edgeuse/p1.rs:962);
  - edge length: 5 samples;
  - radial order: one parameter, `extent.evaluate(0.456)` (edge.rs:518, 569, 982);
  - stitch distance: 5 samples (stitch p2:1020);
  - HLR visibility: 5 samples, then bisection (`classify_range`).
- **Retry with a wider tolerance.** StitchIntoSolid doubles from tol/10 up to diag/200. The second
  pass of StitchAndOrient multiplies by 20. `classify_with_retries` (p9.rs:2956) retries at 100x and
  then returns Unknown.
- **A failure read as a pass.**
  - Check T37: an unknown answer passes.
  - Coincidence tests T32 to T34: a checker failure counts as coincident.
  - `surface/assert_valid.rs:2194`: `Err(_) => Ok(true)`.
  - `IsSilhouetteEdge`: a failure is read as true.
  - Orientation: zero ray hits read as inside, by a `wrapping_sub` underflow at heal p3:5325.
- **Rays and RNG.** The point classifier fires ±x, ±y and ±z rays (`CLASSIFY_RAYS`, query/brep.rs:528).
  Check T8 and T12 fire rays seeded from an RNG. `is_inner_shell_of` (shell.rs:1640) uses a seeded
  RNG. Stitch orientation is decided by ray votes along a fixed `ray_vec`.
- **String errors.** `solidcore-step` returns `Result<T, String>`, and retries are chosen by parsing
  the string (`named_face`).
- **Order dependence.** Stitch clusters vertices along a (1,1,1) sweep as a star, which is not
  transitive. The edge pass is greedy. `pick` takes the first hit and stops at the first miss. STEP
  joins depend on read order.

### 1.2 Rulings for step 6 (JUDGED)

These apply the standing rules of 01, 02 and 04 (truth over pin; valid input never refused; one
resolution per document; five error kinds; one budget; a check never changes the body it checks;
step-4 signatures; determinism) to the shapes above.

- **R1 `&Brep` everywhere.** Every query, the check and HLR take `&Brep`. Lazy trims are built by
  `finish` (D13 and 04 11-27: pcurves are exact or fitted before `finish`). Raised tolerances are
  never written by a read. A read that finds a gap reports it as a finding. Stitch and STEP build a
  `Draft` and return `Made<Brep>`.
- **R2 Proved or Undecided, never sampled-and-trusted.** A query that answers a yes/no question
  returns a three-way answer (`Yes`/`No`/`Undecided`) or a bracket. A sample may *refute*, never
  prove. Where the pin samples, the port either certifies the bound (interval enclosure, as stitch
  `bounds.rs` and `distance_bound.rs` already do) or returns `Undecided` with the bracket reached.
- **R3 No retry with a wider tolerance.** Tolerance is the document's resolution and each entity's
  accuracy. A join is accepted when the certified gap is within coincidence =
  max(res, acc₁, acc₂). Anything further apart stays open and is reported. The pin's own stitch doc
  already says this ("No retry with a larger tolerance"); its code contradicts it.
- **R4 No rays for decisions that a sign can make.** Orientation is decided by shared-edge parity plus
  the certified volume sign per shell (`scx-mass` MA4). Containment uses `classify_point`, which
  already has no `Unknown` (BQ1). Rays stay only inside `classify_point`, which handles ties.
- **R5 Typed findings, not numbered tests.** The check returns a list of `Finding`s with an entity, a
  kind and, where measured, a certified bound. There are no test numbers and no levels as `u8`.
- **R6 Budgets, not caps.** `MAX_PAIRS 65536`, `MAX_SPLITS 12`, `MAX_CONTACTS 8`, the 128-point grid
  and the 10-loop split pass all become `Budget` spending. When the budget runs out the result is
  `Exhausted` with the bracket or partial list reached, never a silent truncation.
- **R7 Deterministic order.** Iterate in id order. Clustering is by union-find over a box tree,
  visited in id order. Run every test twice and compare bits.

## 2. `scx-bodyq`: contact, continuity, pick, topology

### 2.1 What the pin does (READ)

All of these are Tangency-authored and certified. Their `&mut` is used only for lazy trims, so the
port is mostly mechanical.

- **Contact** (`query/contact.rs`, 950 lines):
  - `meets` (:743), `boundary_distance` (:759) and `interiors_disjoint` (:803);
  - each brackets [lower, upper] by Bézier piece boxes on a heap, with `MAX_PAIRS 65536`;
  - it returns a witness pair;
  - it starts with O(Fa·Fb) face pairs and has no face tree;
  - lower bounds come from `distance_bound.rs` (1,740 lines; `face_distance_lower_bound`, :281).
- **Continuity:**
  - `continuity.rs` (923): G0 to G4 measured by drops. Its own doc admits there is "no bound on their
    error".
  - `faces_continue.rs` (129): yes when the definitions are bit-identical, otherwise Undecided.
  - `tangent_edge.rs` (4,126): `tangent_all_along` (:1733) proves tangency with Chebyshev, Lebesgue
    and Markov bounds, `MAX_SPLITS 12`; it also has `g2_all_along` and `tube_regularity`.
  - `edge_turn.rs` (292) and `normal.rs` (906).
- **Recognition:**
  - `prism/measure.rs` (1,236) and `prism/recognition.rs` (1,025);
  - `hole_stacks` (1,013);
  - the plane and cylinder helpers `face_pair`, `mid_sheet`, `mid_joint`, `cylinder_wall`, `cap`.
- **Pick** (`ops/pick.rs`, 537): the nearest entity within a tolerance; it returns `Ambiguous` on a
  tie, the first miss ends the search, and the result depends on order. Ray hits come from the ops
  `fire_ray` (measure.rs:1377).
- **Tight boxes** (query/brep.rs:263; face/p3.rs:484 `face_directed_max`): the history is WR-588,
  #1799, #21660, #14452, #13753 and HANG-29. Each fix took `&mut` to raise a tolerance or cache a trim.
  The edge box uses `.unwrap_or_default()`.
- **Missing in the pin:** Betti numbers and genus as a query (testkit has `euler` and `genus` on main);
  a sliver and thin-place finder (the nearest are `offset/wall_room.rs` and `thin_features.rs`).

### 2.2 Idiom hazards (READ)

- `flush_contact.rs` never checks overlap. It decides on the tool's centroid alone, `plane_tol` scales
  with size, and `MAX_CONTACTS 8` truncates.
- `prism` swallows errors with 24 `.ok()?`, so a Cancelled becomes "not a prism".
- `hole_stacks` uses absolute constants in mm (`COAXIAL 1e-3`, `NEAR 1e-6`). These should come from
  the document's resolution.
- The old SMLib shell `brep_relationship` (api_queries.rs:612) returns int codes 0 to 4 from a Newton
  gap that is not a bound. Do not port it.
- `face_surface.rs` is a record builder, not a query. It belongs with `scx-record` or nowhere.
- `continuity.rs` reports measured drops as if they were exact.

### 2.3 Proposed shape (JUDGED)

```rust
pub fn contact(a: &Brep, b: &Brep, cx: &Cx) -> Result<Contact, BodyqError>;
pub struct Contact { pub gap: Bracket<Length>, pub witness: Option<(Entity, Entity, Point3)>,
                     pub interiors: Tri /* Disjoint | Overlap | Undecided */ }

pub fn continuity(body: &Brep, edge: EdgeId, cx: &Cx) -> Result<Continuity, BodyqError>;
pub struct Continuity { pub proved: Order /* G0..G4 proved all along */,
                        pub measured: [Bracket<f64>; 5] /* drops with bounds */ }

pub fn pick(body: &Brep, ray: Ray, aperture: Length, cx: &Cx) -> Result<Vec<Hit>, BodyqError>;
pub fn betti(body: &Brep, shell: ShellId) -> Betti;            // exact, topological
pub fn thin_places(body: &Brep, below: Length, cx: &Cx) -> Result<Vec<ThinPlace>, BodyqError>;
pub fn tight_box(body: &Brep, e: Entity, cx: &Cx) -> Result<Aabb, BodyqError>;
```

- `contact` replaces `meets`, `boundary_distance` and `interiors_disjoint` with one call returning
  the bracket. A face box tree replaces the O(Fa·Fb) start. The heap order is (lower bound, face
  ids) so ties break by id.
- `continuity` separates what is proved (from `tangent_edge`'s bounds) from what is measured (drops
  with interval bounds). The pin mixed the two.
- `pick` returns every hit within the aperture, sorted by (depth, entity id). The caller chooses.
  No `Ambiguous`, no first-miss stop.
- `betti` is exact: b0 from shells, b1 from Euler and orientability, b2 from closed shells. It needs
  no geometry and can start now.
- `thin_places` is new. It is a bracketed contact between a body's own opposite faces, sharing
  `contact`'s machinery, with face pairs that share an edge excluded near that edge. It is a separate
  row because it has no pin truth.
- `tight_box` is computed from finished trims, so it needs no `&mut`. A failing edge box is an
  error, not `Default`.
- Recognition (`prism`, `hole_stacks`) moves to `scx-bodyq::recognize`. It propagates errors, and
  its constants come from `cx.doc.resolution`.

### 2.4 Truths and tests

Port the tests with closed-form expected values:

- `tests/contact_between_bodies.rs` (12);
- `continuity_measure.rs` (4);
- `tangent_edge.rs` (18);
- `surface/continued.rs` (6);
- tight-box `issue_1799` (8) and `issue_1054` (8);
- `prism/tests.rs` (73).

New truths:

- `betti` on the testkit recipes (sphere 1,0,1; torus 1,2,1; two lumps; a cavity) against testkit
  `genus`;
- contact on two spheres at a known gap (closed form);
- `thin_places` on a box with a slot of known width.

Where the pin is probably wrong:

- `continuity`'s G2 to G4 drops are unbounded;
- `flush_contact`'s centroid test (two offset plates pass as flush);
- `pick`'s order dependence.

The port's tests should state what is correct, not what the pin returns.

### 2.5 Perf traps

- The O(Fa·Fb) start in contact. A face box tree with a lower-bound heap makes it O((Fa+Fb) log F)
  on bodies far apart.
- `tangent_all_along` splits up to 12 levels per edge. Across a whole body (the check's continuity
  findings) this must spend one budget, not 12 levels per edge each.
- `tight_box` was the HANG-29 site. Cache it per entity in `Scratch`, never in the body.

## 3. `scx-check`: the full validity check

### 3.1 What the pin does (READ)

- The ops `Body::check` runs `ValidatePointers` and then `AssertValid`, with Level 0 and Level 2.
  Level 1 has no gates of its own (INFERRED from the code: nothing branches on it). There are about
  600 numbered tests (`labels.rs`, 3,207 lines). About 150 are pointer tests, which have nothing to
  check in an honest arena.
- The pin's `is_outward_solid` (topology/brep.rs:104-250) runs Level 0 only. It is what main's
  `scx_check::contract` already ports.
- It does no face-face SSI. The only exception is the self-intersection of `OffsetSurface` T2
  (wrapped.rs:84-125).
- Measured quantities:
  - the edge/face gap is sampled at 20 points;
  - the edge length is sampled at 5;
  - the pcurve is never measured against its edge: `curve_max_distance_between` computes it at 51
    samples and then discards the result;
  - radial order is checked at one parameter (0.456);
  - `tangent_sections.rs` is closed form for planes and cylinders (#18639).
- The wide-vertex test T82 is by boxes (512/2048). Loop orientation uses a certified uv area.
- `validate_and_update` grows a tolerance to twice a sample. The pin's own doc (§8) calls this
  wrong.
- Cost: on Menger with 414 faces, Level 2 takes 3.1 s against a 0.48 s Boolean, and coincidence
  alone takes 1.28 s. `check_memo` was added for a 4,400-face case that needed 45,000 vertex-use
  solves.

### 3.2 Idiom hazards (READ)

- 64 `&mut Brep` and 50 `&mut Kernel`.
- About 57 swallows, for example `pointers.rs:696` and `:919`, and `surface/assert_valid.rs:2194`
  `Err(_) => Ok(true)`.
- Two mode flags: `memoize_curve_caches` and `with_held_trim_raise`.
- `level: u8`.
- Test numbers are parsed out of text (ops/check.rs:55).
- `validate_and_update` writes during a check. This breaks #21105, a check never changes the body it
  checks.

### 3.3 Proposed shape (JUDGED)

```rust
pub fn check(body: &Brep, cx: &Cx, scope: Scope) -> Result<Findings, CheckError>;
#[non_exhaustive] pub enum Scope { Structure, Geometry, Full }
pub struct Findings { pub list: Vec<Finding>, pub undecided: Vec<Undecided>, pub cost: Spent }
#[non_exhaustive] pub enum Finding {
    EdgeOffFace { edge: EdgeId, face: FaceId, gap: Bracket<Length>, allowed: Length },
    PcurveOffEdge { fin: FinId, gap: Bracket<Length>, allowed: Length },
    VertexOffEdge { vertex: VertexId, edge: EdgeId, gap: Bracket<Length> },
    LoopWound { lp: LoopId, area: Bracket<f64> },
    RadialOrder { edge: EdgeId },
    FacesCross { a: FaceId, b: FaceId, witness: Point3 },
    VertexNotManifold { vertex: VertexId },
    WideVertex { vertex: VertexId, extent: Length },
    SelfIntersectingSurface { face: FaceId, witness: Point2 },
    Degenerate { entity: Entity, what: Degeneracy },
    /* structure: lone vertex, face not bounding, non-manifold edge, empty, … as in `contract` */
}
```

- `contract` stays as it is: the kernel-edge proof that a body is closed, manifold and outward.
  `check` is the full list. `contract` = `check(Structure)` + the volume sign, and both share the
  structure pass. They are not two implementations.
- Each gap is a certified upper bound (as stitch `bounds.rs` does). A finding needs a certified
  lower bound above the allowance. An upper bound above the allowance with a lower bound below it is
  `Undecided`, with the bracket. Nothing unknown passes, and nothing unknown fails as a defect.
- `FacesCross` is new; the pin never tests it. On analytic pairs it uses the step-2 and step-4 SSI
  plus BO2b's no-touch proof. On NURBS pairs it waits on step 5 (SM5 to SM8). Until then such a pair
  is listed in `undecided` as `NeedsSsi { a, b }`, never passed.
- Vertex manifoldness is new: walk the fins around the vertex (a disk or half-disk link). It is pure
  topology and can start now.
- `check` never writes. A body whose tolerances need to grow is reported. Growing them is a separate
  edit, `scx_brep::edit::grow_accuracy(&Brep, &Findings) -> Draft`, which records each raise in
  `Changes` and `Report`.
- Cost: a face box tree gates every pairwise test, so only pairs whose boxes overlap within the
  allowance are examined. One `Budget` covers the run. Coincidence uses closed-form projection on
  analytic pairs.

### 3.4 Truths and tests

- Port:
  - `a_box_faces_out_and_its_flip_does_not`;
  - `offset_edge_curve_fails_test_13` (gap 0.01, now `EdgeOffFace` with gap bracket ∋ 0.01);
  - `a_repeated_check_leaves_the_body_as_it_was` (bit compare of the body before and after);
  - the face34 tests;
  - the wide-vertex tests;
  - `issue_1515` (torus pole);
  - `tangent_sections` (8).
- New, by construction from testkit recipes, each with exactly one defect:
  - an edge moved off its face by δ;
  - a pcurve offset by δ;
  - a loop reversed;
  - two faces made to cross;
  - a pinched vertex.
  Each is checked at δ = 10·acc (found), δ = acc/10 (clean) and δ = acc (finding or Undecided, never
  clean-and-wrong).
- Where the pin is probably wrong:
  - T37's unknown-passes;
  - T32 to T34's failure-as-coincident;
  - the 20-sample gap (it misses a bump between samples; construct one);
  - the single-parameter radial order (fins that swap order away from 0.456);
  - the never-measured pcurve gap.

### 3.5 Perf traps

- Coincidence was 41% of the pin's Level 2 on Menger. Here it is closed form on analytic pairs and
  box-gated.
- Vertex-use solves (the 45,000 case) become one closed-form projection per fin end, cached in
  `Scratch` by (vertex, edge).
- `FacesCross` on all face pairs is O(F²) without the box tree. With the tree it is O(F log F + k).
- The check must stay well under the Boolean's cost on the slice's cases (see the perf row in §8).

## 4. `scx-stitch`

### 4.1 What the pin does (READ)

- STEP import in the pin does not call stitch. `solidcore-step` orients by shared-edge parity
  (assembly.rs:1507) plus a certified volume sign (kernel.rs:155). Healing on load is
  `solidcore-convert`'s `heal_made` (topology/brep/p10.rs:546), which calls
  `HealData::heal_brep(All)`.
- Stitch is heal/stitch p1 (3,676 lines), p2 (2,458) and `bounds.rs` (2,515, certified interval
  enclosures, #2975).
- `do_stitching` (p2:457):
  1. sets the `editing` and `doing_boolean` flags;
  2. a split pass over a box tree, up to 10 loops;
  3. a vertex pass that sweeps along (1,1,1) and clusters as a star, which is not transitive;
  4. an edge pass, `glue_edges_at_vertex` (p2:1020): greedy, with a 5-sample distance; for a line
     against a line the gap is set to 0, so nothing is raised (p2:1144);
  5. `find_and_set_infinite_region`.
- Glue raises tolerances at p5:1802 (the distance plus the deleted entity's tolerance) and p7:2210.
- Orientation, `orient_trimmed_surfaces` (p3.rs:4945):
  - ray votes along a fixed `ray_vec`;
  - a `wrapping_sub` underflow at p3:5325, so zero hits read as inside;
  - a tie sets `maybe_not_closed`, which is then discarded.
- Lessons in the history: #2506 RadialGlue, WR-21540 (nested-solid voids), #18166 (covers),
  WR-18515, HANG-33.
- `fix_tol_sizes` (heal_data/p2.rs:3016) is a bucket step function. `tolerance_defects.rs` is
  certified.
- No SSI is needed. Stitch uses curve drops, curve-curve distance, curve-surface distance and
  line-surface hits for rays.

### 4.2 Idiom hazards (READ)

- The `Stitch` struct has 9 bools.
- Global mode flags (`editing`, `doing_boolean`).
- Retries with a wider tolerance (§1.1).
- Order-dependent clustering and gluing.
- Raises are not recorded.
- The orientation underflow.
- Insertion sort.
- A full rescan per glue: O(V·E).

### 4.3 Proposed shape (JUDGED)

```rust
pub fn stitch(sheets: &[&Brep], cx: &Cx, want: Want) -> Result<Made<Brep>, StitchError>;
#[non_exhaustive] pub enum Want { Sheets, Solid }
// Report: joined edges, open edges left (each with its certified gap), raises (entity, from, to)
```

1. **Vertices.** Cluster by union-find over a box tree. Two vertices are joined when their certified
   distance is ≤ coincidence. Clusters are transitive; this is the fix for the star. A cluster wider
   than 2·coincidence is reported as `WideCluster`, and left joined only when no pair inside it is
   further apart than coincidence allows by the chain. Visit in id order.
2. **Edges.** Candidates come from shared vertex clusters plus a box overlap. A pair is accepted when
   `bounds.rs`'s certified Hausdorff upper bound is ≤ coincidence. Accept pairs by (bound, ids)
   order from a heap, not greedily by vertex. An edge whose partner is ambiguous (two candidates
   within the allowance) stays open and is reported.
3. **Splits.** An edge meeting a vertex interior to its partner is split at the certified foot point.
   Loop until no split is made, with the budget bounding it, not 10 loops.
4. **Raises.** A join within coincidence but above the entity's own accuracy records
   `Raise { entity, from, to }` in the `Report`. This is the only place accuracy grows.
5. **Orientation.** Shared-edge parity orients each shell. The certified per-shell `volume_sign`
   (MA4) picks the outward sense. A shell whose sign is `Undecided` is reported, never guessed.
   Nesting uses `classify_point`.
6. **No retry.** `Want::Solid` with open edges left is `Refused(NotClosed { open })`. The partial
   sheets are still available through `Want::Sheets`.

### 4.4 Truths and tests

- Port:
  - the `bounds.rs` enclosure tests;
  - ops/stitch `a_cap_on_a_cube` (volume 1500), lid on pocket, two boxes merge;
  - `fork_351c57c` (936);
  - `stitched_solid_cavity_is_a_void`.
- New:
  - a testkit recipe exploded into faces and stitched back gives the same volume (closed form) and
    `check` clean;
  - each face perturbed by δ < coincidence joins, and at δ > coincidence stays open;
  - the order of the input sheets permuted gives identical bits.
- Where the pin is probably wrong:
  - the star clustering (a chain of three vertices each 0.9·tol apart joins in one sweep order and
    not another);
  - zero-hit orientation;
  - the retry ladder (it closes models that should be reported open).

### 4.5 Perf traps

- The O(V·E) rescan. Use one box tree, built once, and update the union-find incrementally.
- Certified Hausdorff on long spline edges. Use the box overlap first, then `bounds.rs` with the
  budget.
- Orientation by sign is one `volume_sign` per shell. That is cheaper than the pin's ray per pass.

## 5. `scx-hlr`

### 5.1 What the pin does (READ)

The pin has three unrelated pieces.

**(A) `hlr/`** (7,527 lines). It is SolidCraft-derived and already idiomatic: its own portable Brep,
a typed `HlrError`, no `&mut`. The entry points are `prepare` (hlr.rs:276), `hidden_lines` (:1037),
`hidden_lines_on` (:1052), `silhouettes` (:1335), `View`, `Options` and `CurveAccuracy`. Its only
caller is ops/hlr.rs. The pipeline:

1. a scene- and body-centred frame (W05a finding 7);
2. quadric recognition;
3. silhouettes:
   - exact for cylinder, cone and sphere;
   - `rev_silhouette` for torus and revolutions;
   - for freeform faces, a 128×128 contour that is off by default; faces left out go to
     `unsupported`, and anything they might hide is Uncertain;
4. screen sampling to a sag of 1e-7·scale;
5. crossings on a grid, refined by Newton;
6. `classify_range`: 5 samples plus 40-step bisection;
7. `classify_point_counted` (hlr.rs:2954): a ray against every face, with no box tree;
8. `cover` at 1e-4 of the scene.

**(B) SmHCR** (`blend/hcr.rs` 2,303, `api_projection.rs` 678, `surface_silhouette.rs` 1,321). These
are upstream ports:

- 31 `&mut Kernel` and `set_editing_enabled`;
- about 34 ignored results and `SM_ERR_NULL_POINTER`;
- the magic numbers 0.534567 and 0.5123;
- a "newest first" order taken from libstdc++;
- interval classification by a single ray;
- a failed `IsSilhouetteEdge` read as true.

The app does not use it, and its only other caller is sweep. It needs B-spline SSI and tracing.

**(C) `eval/outline.rs`** (225) plus `pieces.rs` (676) are clean, with 32 samples.

`intersect/views.rs` (391) needs SSI and is out of scope.

### 5.2 Idiom hazards (READ), in (A)

- About 116 literal tolerances.
- `Atomic` counters.
- A `dyn Fn` cancel instead of `Stop`.
- About 8 out-parameters (`&mut Vec<Hit>`).
- About 40 `unwrap_or`, for example `shifted()` (hlr.rs:612) and `perp()` (silhouette.rs:43).

### 5.3 Proposed shape (JUDGED)

```rust
pub fn prepare(bodies: &[(&Brep, Affine)], cx: &Cx) -> Result<Scene, HlrError>;
pub fn hidden_lines(scene: &Scene, view: &View, opts: &Options, cx: &Cx) -> Result<Drawing, HlrError>;
pub struct Drawing { pub visible: Vec<Curve2>, pub hidden: Vec<Curve2>, pub uncertain: Vec<Curve2>,
                     pub unsupported: Vec<FaceRef> }
```

- Port (A) onto scx bodies: drop its portable Brep and read `&Brep` directly. Its tolerances come
  from one `Accuracy` derived from the scene scale and the document's resolution, named in one
  module. Cancel is `cx.stop`. Counters move to `cx.trace`. `unwrap_or` becomes typed errors.
  Out-parameters become returns.
- Visibility per range is proved where it can be. Each sub-range classified by a point
  classification whose ray is decided by `classify_point` (certified, with no `Unknown`), and a
  sub-range whose ends differ is split until the change is located to the curve accuracy. A range
  is Uncertain only when the budget runs out, never because `trim_samples` was small.
- Freeform silhouettes: replace the 128 grid with the step-5 marcher's normal-cone subdivision
  (SM5). The silhouette is `n·v = 0`, a curve on one surface, the same shape as SSI's field path
  (SM6). Until then, NURBS faces go to `unsupported` as in the pin.
- Port (C) as it is.
- Do not port (B). Its users either go away (sweep's use is step 5's procedural sweep) or are served
  by (A).

### 5.4 Truths and tests

- Port the 88 tests in `hlr/tests.rs`. Most have closed-form expected values:
  - `iso_cube_hides_the_three_back_edges`;
  - sphere outlines as conic arcs;
  - torus equators;
  - thin-torus rays;
  - far from the origin;
  - `serial_and_parallel_same_bits`.
- New:
  - a sphere seen from any direction has a circle silhouette of its own radius (closed form);
  - a cylinder's two generators at ±r;
  - a box behind a box: the hidden length is known.
- Where the pin is probably wrong:
  - the 5-sample miss: a small occluder between samples (construct one);
  - Uncertain depending on `trim_samples`;
  - the 128 grid misses small silhouette loops (construct a bump);
  - `cover` at 1e-4 of the scene, which scales with the scene;
  - `same_analytic` at 1e-13 against recognition at 1e-9.

### 5.5 Perf traps

- `classify_point_counted` fires a ray against every face. Use the face box tree that bodyq already
  has.
- Screen sampling at 1e-7·scale can be dense on long curves. Spend the budget, and sample
  adaptively by curvature.
- Parallel runs must give the same bits as serial runs. Keep the pin's test.

## 6. `scx-step` (and the healing in `scx-import-sc`)

### 6.1 What the pin does (READ)

`solidcore-step` (24.7k lines, live) reads into a record Brep. The pipeline:

1. Parse and index; `roots` (#2583).
2. Units silently default to mm and radians.
3. Uncertainty is taken from the last measure, with `unwrap_or(1e-3).clamp(1e-7, 1)`.
4. Features are re-read at 0.25 × the shortest edge (topology.rs:388-460, #18848).
5. Reader kinds.
6. Vertices use `raise_vertex` at 1.5·gap, refused above 1 mm.
7. Joins are by union-find with `points_meet`.
8. Loops, faces and shells.
9. `Assembly::finish` (assembly.rs:714-1262):
   - slivers retried (#12757);
   - hairpins (#3292);
   - twin edges, seams and frames;
   - `face_pcurves`: closed form, otherwise the `Trims` kernel (not in the pin), otherwise wound
     (#20575);
   - orientation by parity plus the volume sign (#17149, #3730);
   - `record_check` for validation and agreement.

Kinds it reads:

- curves: line, conics, B-spline, polyline, trimmed, surface_curve, offset_curve (refused as an
  edge) and replica;
- surfaces:
  - the analytic kinds and B-spline;
  - revolution (`placed_revolution`);
  - extrusion (an exact spline, or a cylinder);
  - offset (exact for analytic bases only; a spline offset is refused);
  - rectangular_trimmed and replica.

It does not read tessellated, CSG or wireframe models. It never uses PCURVE entities.

The export (write/product.rs) writes AP242, with one uncertainty per body (minimum 1e-6, one context
per body), no validation properties and exact faces. `step-export` (7.8k) is legacy; only goldens use
it. `step_assembly` `placed_bodies` is in ops step_assembly.rs:52.

### 6.2 Idiom hazards (READ)

- `Result<T, String>`, with retries chosen by parsing the string (`named_face`).
- 49 `&mut self` on `Assembly`.
- About 18 counters.
- 22 `.ok()`.
- 65 `unwrap_or`, for example `face.outer.unwrap_or(0)`.
- 14 out-parameters.
- About 96 literals.
- A `Kernel::default()` per call.

Where the pin is probably wrong:

- Units are silent.
- Uncertainty comes from the last measure, not the context's own.
- Joins use exact float equality and depend on read order.
- The FEATURE 0.25 is measured, not derived.
- `closed()` counts edges used ≥2 times, so a non-manifold body passes.
- Some thresholds come from SMLib (`OPEN_DOMAIN_SPAN`, `MAX_TOL 1.0`).
- The extrusion crossing is sampled at 128 points.

### 6.3 Proposed shape (JUDGED)

```rust
pub fn read(file: &scx_p21::File, cx: &Cx, opts: &ReadOptions) -> Result<Import, StepError>;
pub struct Import { pub bodies: Vec<Made<Brep>>, pub products: ProductTree,
                    pub skipped: Vec<Skipped>, pub findings: Vec<(BodyRef, Findings)> }
pub fn write(bodies: &[(&Brep, &ProductRef)], cx: &Cx, opts: &WriteOptions) -> Result<scx_p21::File, StepError>;
```

- **One reader**, on `scx-p21`: entities → `Draft` → `finish`. There is no record Brep in between.
- **Units** come from the file's context. When they are missing, the result is `Refused(NoUnits)`
  unless `ReadOptions::assume_units` is set, and the `Report` records the assumption. Uncertainty is
  the context's `UNCERTAINTY_MEASURE_WITH_UNIT`; with none, the document's resolution. The
  accuracy-ratio warning applies (CTC-05 needs 2.05e-2).
- **Joins.** Vertices are joined only where the file shares them. Where it does not, the joins come
  from `scx-stitch`'s certified rule within coincidence. Read order cannot change the result: build
  in id order.
- **Kinds map to exact forms.** Analytic kinds map to the `scx-surface` kinds now; B-spline to
  `Surface::Nurbs` once scx-nurbs-surface carries it. Revolution, extrusion and offset map to the
  procedural kinds (step 5, 01 §7.3). Until then those faces are `Unsupported(Kind)` by name, and the
  body is skipped and reported, never approximated without a record.
- **Pcurves.** Use closed forms where they exist. Elsewhere use the step-5 fitter to the claimed
  accuracy (04 11-27). STEP's own PCURVE entities may be used as the fitter's starting point, but
  never accepted without a certified gap check.
- **After `finish`:** `scx_check::check(Full)` runs on each body. Its findings are part of the
  `Import`. A body with findings is still returned. Valid input is never refused, and the findings
  say what is short.
- **Healing.** `scx-step` does not heal. A healing edit (stitch, grow accuracy) is the caller's
  choice: `scx-import-sc` calls it on load and records each change. This keeps the reader honest and
  the healing auditable.
- **The writer** ports write/product.rs. It writes the document's resolution and one context per
  document, not per body. Validation properties (volume, area, centroid) are written when `scx-mass`
  proves them. `step-export` is not ported.

### 6.4 Truths and tests

- Port:
  - `topology_tests` (109; "Expected values are analytic", about 21 closed-form volumes);
  - `assembly_tests` (33);
  - `geom_tests` (29);
  - `write_signed_volume.rs` (15) as it is (signed volume from the written entities);
  - `panic_fuzz` (4).
- `bench/step/references.tsv` holds volume truths keyed by SHA, made by generator formulas. Port the
  generator, not only the numbers.
- New:
  - Round trip: write then read a testkit recipe, with the same volume within the certified error,
    the same topology counts and `check` clean.
  - Determinism: read twice, compare bits.
  - Units: the same part in mm and in inches gives the same body after scaling.

## 7. The STEP corpus and how it is scored

### 7.1 Sources (licence bar: MIT/Apache/BSD/CC0/CC BY; NIST public models OK)

| Source | Files | Licence | Use |
|---|---:|---|---|
| NIST MBE PMI test cases (ctc_01-05, ftc_06-11 in AP203; ctc/ftc/stc in AP242) | 33 | US government work, public | the corpus; manifest `scripts/nist-pmi-corpus.sha256` (research 58) |
| Pin fixtures (Apache), excluding the discord fixture | 5 | Apache-2.0 | small cases with known truths |
| MFCAD sample | 1 (a set exists) | MIT | machining features; check the set size before adding more |
| dan335/step-conformance | 17 | CC0, as stated; **verify the licence file before use** | conformance edge cases |
| Generated from testkit recipes via `scx-step::write` | any | ours | round trip with closed-form truths |

Excluded:

- the STEP Tools corpus (all rights reserved);
- any NC, ND, SA or ToS-bound set;
- CAx-IF practice documents, which may be cited but not used as data.

Files are fetched at test time against a SHA manifest and never committed (as `SCX_P21_CORPUS`
already does for p21, with 430,862 instances across the NIST set).

### 7.2 Size (READ)

- About 39 external files plus generated round trips.
- That is small. It is enough for "imports and passes the check" but not for a pass rate with a
  confidence. §9 Q6 asks whether to widen it.

### 7.3 Scoring "imports and passes the check" (JUDGED)

| Measure | Pass when | Truth from |
|---|---|---|
| Opens | `read` returns `Ok` (no `Defect`, no panic) | — |
| Bodies imported | every body is `Made`, or `Skipped` naming an `Unsupported` kind still awaiting a step-5 row | the row list in §8 |
| Check | `check(Full)` gives zero findings and zero `undecided`, except `NeedsSsi` while SM5-SM8 are pending | `scx-check` |
| Contract | `contract` → `Proved` for each body the file declares a solid | `scx-check` |
| Volume / area | within the certified error of the reference | NIST: invariants (mm vs inch file pairs, AP203 vs AP242 of the same part); pin fixtures and round trips: closed form or `references.tsv` generator |
| Determinism | two reads give identical bits | run twice |
| Round trip | read → write → read gives the same body (topology counts, volume within error, check clean) | self |
| Time | recorded per file (not gated in step 6; one baseline row for 01 §9.4) | bench |

"Step 6 done" means:

- every file opens;
- every body is `Made` or `Skipped` for a named pending kind;
- every `Made` body passes the check and the contract;
- and, once the step-5 rows SM1-SM9, the fitter and the procedural kinds have landed, nothing is
  skipped.

The scoring is a `scx-bench` corpus runner. It writes a TSV with one row per file, in the shape of
the pin's `bench/step/run.py` (opened, refused, timeouts, closed, volume within tolerance, time), and
adds two columns: findings and undecided.

NIST publishes no reference volumes, so the NIST truth is invariance: the same part read from AP203
and from AP242 must agree, and so must the same part in two units. Where the file carries validation
properties (AP242 files may), those are used.

## 8. Task table

Model: Opus where a correctness argument (certification, a proof of a bound, an order-independence
argument) is the work; Sonnet where the work is porting tests or mechanical plumbing.

| Id | Crate | Work | Needs | Model |
|---|---|---|---|---|
| BW1 | bodyq | `betti` per shell (exact, topological) + tests against testkit `genus` | — | Sonnet |
| BW2 | bodyq | `tight_box` on finished trims, no `&mut`; port issue_1799/issue_1054 tests | — | Sonnet |
| BW3 | bodyq | `contact` (port contact.rs + distance_bound.rs) with face box tree, heap by (bound, ids), budget for MAX_PAIRS; port 12 tests | BW2 | Opus |
| BW4 | bodyq | `continuity` and `tangent_all_along` port; proved order separated from bounded drops; port 4+18+6 tests | — (analytic); NURBS edges after SM1 | Opus |
| BW5 | bodyq | `pick` returning every hit within the aperture, sorted by (depth, id) | BW2 | Sonnet |
| BW6 | bodyq | `recognize`: prism + hole_stacks port with error propagation and resolution-derived constants; port prism tests (73) | — | Sonnet |
| BW7 | bodyq | `thin_places` (new; self-contact excluding the adjacency) | BW3 | Opus |
| CF1 | check | `check(Structure)`: findings type, structure findings shared with `contract`, vertex manifoldness | — | Sonnet |
| CF2 | check | certified gaps on analytic geometry: edge/face, pcurve/edge, vertex/edge, loop winding, radial order along the whole edge, wide vertex, degeneracy; face box tree; one-defect recipes at δ = 10·acc, acc, acc/10 | CF1 | Opus |
| CF3 | check | `FacesCross` on analytic pairs (step-2/4 SSI + BO2b no-touch), `NeedsSsi` otherwise | CF2 | Opus |
| CF4 | check | gaps and crossing on NURBS faces | CF2, SM1, SM5-SM8, NURBS projection | Opus |
| CF5 | brep | `edit::grow_accuracy(&Brep, &Findings) -> Draft` with raises recorded | CF2 | Sonnet |
| CF6 | check | port truth tests (§3.4) + a "check never writes" bit-compare test over the testkit recipes | CF2 | Sonnet |
| ST1 | stitch | crate, `stitch` signature, vertex clustering by union-find over box tree, id order, `WideCluster` | — | Opus |
| ST2 | stitch | edge pairing by certified Hausdorff (port bounds.rs), heap acceptance, ambiguity left open; splits | ST1 | Opus |
| ST3 | stitch | orientation by parity + MA4 volume sign, nesting by `classify_point`; `Want::Solid` refusal with open list | ST2, CF1 | Opus |
| ST4 | stitch | truth tests: exploded recipes, perturbation at ±coincidence, permutation bit-identity; port pin tests | ST3 | Sonnet |
| HL1 | hlr | port (A) onto `&Brep`: frames, quadric recognition, exact quadric/torus silhouettes, `Stop`/`Trace`, typed errors, named tolerances | — | Opus |
| HL2 | hlr | visibility by `classify_point` with box tree, split to accuracy, Uncertain only on Exhausted | HL1 | Opus |
| HL3 | hlr | port (C) outline/pieces; port 88 tests; new closed-form view tests | HL1 | Sonnet |
| HL4 | hlr | freeform silhouettes by normal-cone subdivision + field path | HL2, SM5, SM6a | Opus |
| SP1 | step | crate on scx-p21: units, uncertainty, product tree, typed `StepError`; refuse missing units | — | Sonnet |
| SP2 | step | analytic kinds → Draft → finish; joins as shared by file; closed-form pcurves; seams; parity + volume sign; port topology/assembly/geom tests on analytic files | SP1, CF1 | Opus |
| SP3 | step | writer (port write/product.rs, one context per document, validation properties); `write_signed_volume` tests; round trip | SP2 | Sonnet |
| SP4 | step | joins the file leaves unshared via `scx-stitch` rules | SP2, ST2 | Sonnet |
| SP5 | step | B-spline curves and surfaces; pcurves by the fitter | SP2, NURBS surface kind, step-5 pcurve fitter | Opus |
| SP6 | step | revolution, extrusion, offset kinds → procedural kinds | SP2, step-5 procedural kinds | Opus |
| SP7 | import-sc | heal on load: stitch + grow_accuracy, each recorded | ST3, CF5 | Sonnet |
| CO1 | bench | corpus runner: manifest fetch, TSV per §7.3, determinism, invariance pairs | SP2, CF2 | Sonnet |
| CO2 | testkit | port the references.tsv generator; dan335 licence verification + manifest | — | Sonnet |
| CO3 | bench | perf rows: check vs Boolean on slice cases; stitch on exploded recipes; STEP read per file | CF2, ST3, SP2 | Sonnet |

### 8.1 Graph and critical path

```
CF1 ──> CF2 ──┬─> CF3 ───────────────┐
              ├─> CF4 (SM1, SM5-8)    │
              ├─> CF5 ──> SP7          │
              └─> CF6                 │
SP1 ──> SP2 (CF1) ──┬─> SP3           ├─> CO1 ──> "done" (all files pass)
                    ├─> SP4 (ST2)      │
                    ├─> SP5 (fitter, NURBS kind)
                    └─> SP6 (procedural kinds)
ST1 ──> ST2 ──> ST3 (CF1) ──> ST4
BW2 ──> BW3 ──> BW7;  BW2 ──> BW5;  BW1, BW4, BW6 alone
HL1 ──> HL2 ──> HL4 (SM5, SM6a);  HL1 ──> HL3
```

- **Can start now** (no step-5 input): BW1, BW2, BW4 (analytic), BW6, CF1, ST1, HL1, SP1, CO2. Then
  BW3, BW5, CF2, CF3, CF5, CF6, ST2, ST3, HL2, HL3, SP2, SP3, SP4, CO1 as their step-6 inputs land.
- **Waits on step 5:** CF4 (SM1, SM5-SM8), HL4 (SM5, SM6a), SP5 (the NURBS surface kind and the
  pcurve fitter), SP6 (procedural kinds), and the NURBS half of BW4 (SM1).
- **The critical path to "imports and passes the check"** is CF1 → CF2 → SP2 → SP5 → CO1 on the full
  corpus. It is gated by the step-5 pcurve fitter, because most NIST faces are B-spline. Within
  step 6 alone the path is SP1 → SP2 → CO1 on the analytic subset, so the runner and every analytic
  file can be green before step 5 finishes. Drawings (HL1 → HL2 → HL3) are off the critical path.
- **Layers.** Add `scx-stitch` and `scx-hlr` after `scx-check` (layer 6) and `scx-step` at layer 7
  (on p21, stitch and check), each when its first task lands (72 §12.4 row 8).

## 9. Open questions

**Q1. Do `contract` and `check` stay two functions?**

- Recommend: yes, with one shared structure pass. `contract` is the cheap kernel-edge proof every
  operation can afford. `check` is the full list.
- Against: two entry points drift. A caller may think `contract` is the full check. Mitigation: the
  doc says so, and a test asserts that `contract` is a subset of `check`.

**Q2. Does STEP import heal, or only report?**

- Recommend: `scx-step` only reports. `scx-import-sc` heals on load with recorded changes (SP7).
- Against: every caller of `scx-step` other than the app then gets bodies with findings and must
  heal them itself. The pin's import heals (via convert) and users expect a solid. Mitigation:
  `ReadOptions::heal` calls the same SP7 path, so the choice is the caller's in one flag.

**Q3. Drop SmHCR (5.1 B)?**

- Recommend: drop it. (A) serves the app, (B) has no app use, and its idioms are the worst in the
  survey.
- Against: sweep calls it, and (B) handles freeform silhouettes now, where (A) leaves them
  unsupported until HL4. Mitigation: sweep is rebuilt in step 5's procedural kinds anyway.
  Freeform drawings wait for HL4, which is on the SM5/SM6 path step 5 already builds.

**Q4. Are Betti numbers and thin places in step 6's scope?**

- Recommend: Betti yes (BW1, small, exact). Thin places yes but last (BW7), since 01 lists contact
  and pick and a thin-place finder is contact on one body.
- Against: neither is in the pin, so neither has a difftest truth, and thin places can grow without
  bound in scope (what counts as thin near a fillet?). Mitigation: BW7's definition is fixed here:
  opposite faces, not sharing an edge, closer than `below`, with a certified bracket.

**Q5. Keep the pin's feature re-read at 0.25 × the shortest edge?**

- Recommend: no. Accuracy comes from the file's uncertainty and the document's resolution, with the
  accuracy-ratio warning. A feature smaller than the resolution is a finding (`Degenerate`), not a
  re-read.
- Against: #18848 was a real file that imported only with the re-read. Mitigation: that file
  becomes a test. If it fails, the finding names the edge, and `scx-import-sc` can heal.

**Q6. The corpus is about 39 files. Widen it?**

- Recommend: not as a gate for step 6. Add the generated round trips (unlimited, with closed-form
  truths) and keep looking for permissive sets (an ABC-style set only if its licence passes the bar;
  most do not).
- Against: 39 files cannot show robustness. NIST's PMI parts are clean exports from a few systems,
  and the failures that matter come from messy files. A green corpus may overstate readiness.

**Q7. How is NIST scored without reference volumes?**

- Recommend: invariance across AP203/AP242 and unit pairs, plus the validation properties where the
  file has them, plus the check and the contract.
- Against: invariance passes when both reads are wrong the same way (a shared reader bug). Mitigation:
  the round trip through our writer, and `write_signed_volume`'s independent volume from the written
  entities.

**Q8. Does `check` run inside every operation?**

- Recommend: no. `contract` runs at the kernel edge (as ruled). `check(Full)` runs at import, in
  tests and on request.
- Against: 02 §10 says the checker becomes load-bearing, and bugs in operations go unseen without
  it. Mitigation: the difftests run `check(Full)` on every result. The perf row CO3 keeps its cost
  visible, so the default can change once it is cheap.

**Q9. Is an `Undecided` gap a pass or a fail for "passes the check"?**

- Recommend: neither. It is counted separately, and step 6 is done only with zero `Undecided`
  outside `NeedsSsi`.
- Against: a certified bound that is merely loose (a long spline edge) blocks done for a reason that
  is not a defect. Mitigation: `Undecided` carries its bracket, and the budget for gap bounds is a
  setting, so a loose case is fixed by refining, not by a pass.

## 10. Against this plan

- **The pin's check samples; ours certifies, which is slower.** Certified Hausdorff on every
  edge/face pair may cost more than the pin's 20 samples, and the pin's Level 2 is already 6.5× its
  Boolean. The plan rests on the claim that closed-form projection on analytic pairs plus the box
  tree makes it cheaper, but that is unmeasured. CO3 must measure before CF2 goes ready, and if the
  claim fails, CF2 needs a cheaper certified bound (a Lipschitz bound per span), not samples.
- **Most of the corpus waits on step 5.** NIST parts are largely B-spline faces with fitted pcurves,
  so "the STEP corpus imports and passes the check" cannot be green until SP5, the fitter and CF4
  land. The analytic subset may be small. If it is near empty, CO1 green on it proves little, and
  the plan's "start now" mainly buys plumbing. That is still worth doing, but the progress it shows
  is smaller than the row count suggests.
- **Not healing in the reader may lose files.** The pin heals on load, and its pass record (not
  measured, but implied by its users) comes partly from healing. With healing only in
  `scx-import-sc`, `scx-step`'s own score will look worse than the pin's. Q2's flag mitigates this,
  but the comparison must say which path ran.
- **No retries may refuse files the pin closes.** R3 leaves open what the pin's tolerance ladder
  glued. Some of those glues were right: a sloppy exporter whose gaps exceed its own stated
  uncertainty. The plan reports them open. The user sees a failure where the pin showed a solid.
  This is the truth-over-pin rule applied, but it should be visible in the corpus TSV as its own
  column (open within 10× coincidence), so the cost of the rule is measured.
- **Transitive clustering can over-join.** A chain of vertices each within coincidence may span far
  more than coincidence, and a union-find joins them all. `WideCluster` reports it, but the rule for
  whether to keep the join is a judgement that has not been tested on real files.
- **Invariance scoring is weak** (Q7). A shared bug passes both sides.
- **`FacesCross` is new and unproven at scale.** The pin never tested face crossing, so there is no
  record of how often real files have it. If NIST files commonly have tiny crossings within
  tolerance at tangent blends, CF3 will flood findings. Its allowance must be coincidence, and a
  crossing whose depth is under coincidence is not a finding. That needs a depth measure, which
  CF3 must include.
- **HL2's proved visibility may be slow on dense drawings.** The pin's 5-sample classify was cheap,
  so the port may regress drawing time. A drawing is interactive, so HL2 needs its own perf row
  (add it to CO3).
- **The task table is wide.** Thirty rows across five crates, with nine startable now, is a lot to
  run at once. Its spine is CF1/CF2 and SP1/SP2. If builders are scarce, run those first, then ST,
  then BW and HL.
- **This note measured nothing on the corpus.** Every perf claim above is from the pin's numbers
  (Menger 3.1 s against 0.48 s) or from judgement.

## 11. Proposed 04 rows (for the coordinator's ruling and Brett's review)

1. **Step 6 plan (research 74).**
   - The full `scx-bodyq`, `scx-check`, `scx-stitch`, `scx-hlr` and `scx-step` take `&Brep` and
     return typed answers; stitch and STEP return `Made<Brep>`.
   - Sampled decisions become certified brackets or `Undecided` (R2).
   - No retry with a wider tolerance (R3).
   - Orientation by parity plus the certified volume sign, with no ray votes (R4).
   - The check returns typed `Finding`s with no test numbers or levels (R5).
   - Caps become budget (R6).
   - Id-order determinism (R7).
   - Tasks: §8.
2. **`scx_check::check(&Brep, &Cx, Scope) -> Result<Findings, CheckError>`.** It shares its structure
   pass with `contract`, never writes, and moves accuracy growth into a separate recorded edit
   `grow_accuracy`. Face crossing and vertex manifoldness are new findings, and NURBS pairs are
   `NeedsSsi` until step 5.
3. **`scx-step` reads and reports; it does not heal.** Healing is `scx-import-sc`'s recorded edit, or
   the reader's `heal` option. Missing units are refused unless assumed explicitly. Uncertainty
   comes from the file's context.
4. **SmHCR is not ported.** HLR ports the pin's `hlr/` and `eval/outline` only. Freeform silhouettes
   come from step 5's normal-cone and field-path machinery.
5. **"Imports and passes the check" is scored by §7.3.** It covers every file opening, every body
   either `Made` or `Skipped` for a named pending kind, `Made` bodies with zero findings and zero
   non-`NeedsSsi` `Undecided`, the contract `Proved` for solids, invariance across AP203/AP242 and
   units, determinism and round trip. The corpus is the NIST PMI set plus the permissive fixtures
   and generated round trips, fetched by manifest and never committed.

## 12. What this note did not do

- It ran nothing on the STEP corpus. The files were not present, and the pin has no recorded pass
  rates, so every pass-rate statement here is a plan.
- It did not measure the check's cost on scx bodies. It cites the pin's numbers only.
- It did not verify the dan335/step-conformance licence file (CO2 does).
- It read no code from other kernels for this note.
