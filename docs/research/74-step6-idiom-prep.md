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

§1.3 is the capability ledger: where every pin capability in these crates goes. §7 is the STEP
corpus and how "imports and passes the check" is scored. §8 is the task table. §9 holds the open
questions. §10 is the case against this plan. §11 gives the proposed 04 rows. §15 answers the
adversarial review of 11-27, finding by finding; §8, §9 and §11 are the revised plan.

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
  `scx-p21` 12,395) are from `wc -l crates/*/src/**/*.rs`, tests included. Without tests, bodyq is
  2,560 and check 565 (review, 11-27).

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
  - edge/face gap: 20 projections (`calc_max_edge_face_gap`, edgeuse/p1.rs:973);
  - edge length: 5 samples;
  - radial order: one parameter, `extent.evaluate(0.456)` (edge.rs:518, 569, 982);
  - stitch distance: 5 samples (stitch p2:1018);
  - HLR visibility: 5 samples, then bisection (`classify_range`).
- **Retry with a wider tolerance.** StitchIntoSolid doubles from tol/10 up to diag/200. The second
  pass of StitchAndOrient multiplies by 20. `classify_with_retries` (p9.rs:2956) retries at 100x and
  then returns Unknown.
- **A failure read as a pass.**
  - Check T37: an unknown answer passes.
  - Coincidence tests T32 to T34: a checker failure counts as coincident.
  - `surface/assert_valid.rs:2194`: `Err(_) => Ok(true)`.
  - `IsSilhouetteEdge`: a failure is read as true.
  - Orientation: zero ray hits read as inside, by a `wrapping_sub` underflow at
    topology/brep/p3.rs:5360.
- **Rays and RNG.** The point classifier fires ±x, ±y and ±z rays (`CLASSIFY_RAYS`, query/brep.rs:528).
  Check T8 and T12 fire rays seeded from an RNG. `is_inner_shell_of` (shell.rs:1656) uses a seeded
  RNG. Stitch orientation is decided by ray votes along a fixed `ray_vec`.
- **String errors.** `solidcore-step` returns `Result<T, String>`, and retries are chosen by parsing
  the string (`named_face`).
- **Order dependence.** Stitch clusters vertices along a (1,1,1) sweep as a star, which is not
  transitive. The edge pass is greedy. `pick` takes the first hit and stops at the first miss. STEP
  joins depend on read order.

### 1.2 Rulings for step 6 (JUDGED)

These apply the standing rules of 01, 02 and 04 (truth over pin; valid input never refused; one
resolution per document; five error kinds; one budget; a check never changes the body it checks;
step-4 signatures; determinism) to the shapes above. Revised 11-27 after the review (§15).

- **R1 `&Brep` reads.** Every query, the check and HLR take `&Brep`. Pcurves are exact or fitted by
  the writer before `finish`, and `finish` verifies them (04 row 130); nothing is trimmed lazily.
  A query never writes an accuracy; a query that finds a gap reports it. Results:
  - stitch returns `Done<StitchReport>` with lineage in `Changes` (one operand per input sheet,
    the Boolean's two-operand form generalised), not `Made`: its output faces come from inputs,
    and the app's face references need that lineage;
  - `scx-step::read` returns its own `Imported { bodies, products, report }`;
  - neither imports `scx-make` (`Made<R>` lives there, at layer 8).
- **R2 Proved or Undecided, never sampled-and-trusted.** A query that answers a yes/no question
  returns a three-way answer (`Yes`/`No`/`Undecided`) or a bracket. A sample may *refute*, never
  prove. Where the pin samples, the port either certifies the bound (interval enclosure, as stitch
  `bounds.rs` and `distance_bound.rs` already do) or returns `Undecided` with the bracket reached.
- **R3 No retry with a wider tolerance**, with one exception. Tolerance is the document's resolution
  and each entity's accuracy, and stitch accepts a join only when the certified gap is within
  coincidence = max(res, acc₁, acc₂). The exception is the importer's recorded raise to an
  enclosed gap (09 §6, 04 rows 41 and 42): imported data whose certified gap exceeds its stated
  uncertainty is valid input, so `scx-step` raises that entity's accuracy to the enclosed gap,
  checks the tube stays clear, and records a `Raise` in `Report::raised`. That is a raise, not a
  retry: it is made once, to a proved value, and recorded.
- **R4 No ray votes for orientation.** A closed shell is oriented by shared-edge parity, and its
  outward sense by `scx-mass`'s certified per-shell `volume_sign` (MA4). This needs a
  `scx-stitch` → `scx-mass` edge, proposed in §11 row 2 (legal under layers.txt, where mass is below
  bodyq). Open sheet sets have no volume sign and are oriented by parity alone. Nesting uses
  `classify_point`, which has no `Unknown` but returns `Unsupported` when a NURBS face or edge lies
  near the point or ray (crates/scx-bodyq/src/classify.rs:34), so nesting of NURBS shells waits on
  step 5's ray against NURBS.
- **R5 Typed findings, not numbered tests.** The check returns a list of `Finding`s with an entity, a
  kind and, where measured, a certified bound. There are no test numbers and no levels as `u8`. The
  clearance finding (two vertices within coincidence, 01's scx-check row) is one of them.
- **R6 Budgets, not caps.** `MAX_PAIRS 65536`, `MAX_SPLITS 12`, `MAX_CONTACTS 8`, the 128-point grid
  and the 10-loop split pass all become `Budget` spending. When the budget runs out the result is
  `Exhausted` with the bracket or partial list reached, never a silent truncation. A memo hit
  charges the work recorded when it was filled (04 row 104), so the stop point does not move with
  cache warmth. Loops over many edges (the check's gaps, stitch's Hausdorff bounds) give each edge a
  share and count the edges left `Undecided`, so one long spline cannot spend the whole run.
- **R7 Deterministic order.** Iterate in id order. Vertex identification follows the merged-points
  rule (04 rows 108 and 186), not transitive union-find: direct pairs within coincidence, found by a
  spatial index; two vertices of one operand within coincidence are `Refused` naming both (and the
  check reports the same pair); a vertex of one operand and one of another within coincidence are
  identified; vertices joined only through a chain are each anchored to the nearest operand vertex
  (ties by id), else `Unsupported` with the chain. `scx-boolean` already implements this
  (`BooleanRefusal::VerticesWithinCoincidence`, `BooleanUnsupported::Chain`); stitch reuses it.
  Run every test twice and compare bits.
- **R8 Argument convention.** Operations and queries that keep a memo take `&mut Cx<'_>`
  (`Cx::scratch` is `&mut Scratch`, crates/scx-brep/src/op.rs:30, so a `&Cx` cannot fill a cache),
  as `scx_boolean::boolean` does. Leaf queries that keep none take `(&mut Budget, &Stop)`, as BQ1
  ruled for `classify_point`. The body is `&Brep` in both.

### 1.3 Capability ledger (READ the pin's pub items; JUDGED homes)

The app switches from the pin in one go, so every pub capability of the pin's ops modules that 01
homes in these crates needs a row, a home, or a ruled replacement. The pin's ops crate is
`solidcore-ops/src`; its modules are mapped by 01's module table. "Row" is a task in §8.

| Pin capability (module, lines) | scx home | Row or replacement |
|---|---|---|
| `contact`: `meets`, `boundary_distance`, `interiors_disjoint` (231) | bodyq `contact` | BW3 |
| `contact`: `meets_each`, `flush_contact_points` | bodyq `flush_contact` | BW3b (overlap proved by a planar Boolean of the projected loops, not a centroid) |
| `continuity`: `continuity_report`, `edge_comb`, `iso_comb`, `sheet_seams`, `principal_curvatures` (684) | bodyq | BW4 (the G-class verdict), BW10 (combs, curvature, seams) |
| `tangent`: `tangent_face_chain_ordered` (254) | bodyq | BW10 |
| `pick`: ray pick (via measure `fire_ray`) | bodyq `pick` | BW5 |
| `pick`: `edges`, `vertices`, `faces` by points, `face_at`, `faces_on_one_surface` (537) | bodyq `pick_at` | BW8 (saved designs carry pick points) |
| `pick`: `stored_face`, `stored_edge` | bodyq `still_names` | BW8; ids are kept in scx, so this checks the stored point lies on the named entity within coincidence (it measures, so it is bodyq, not record) |
| `query` (215): `edge_at`, `edge_points(sag)`, `edge_interval`, `edge_faces`, `outline_stretches`, `edge_distance`, `face_uv`, `face_normal`, `face_contains`, `faces_at`, `faces_against`, `ray`, `record` | bodyq, tess (sag points), hlr (`outline_stretches`) | BW9, one line per function |
| `measure` (4,176), the non-mass half: `locate_points`, `classify_point(s)`, `levels_along`, `edge_face_gap_samples`/`largest_gap`, `points_meet`, `turn_to_parallel`, `closest_*`, `face_hit(s)_by_ray`, `face_line_*`, `rim_edges`, `bounds`, the 2-D polyline helpers, `segment_2d_intersection`, `tangent_contact_*_2d` | bodyq; the 2-D helpers to planar; the gap to surface (certified) | BW9 (a mapping table, then the missing functions); gaps by SU6 |
| `measure`, the mass half (`area`, `mass_properties`, `inertia_tensor`, `centroid_*`) | mass | already MA rows |
| `face_pair`, `mid_sheet`, `mid_joint`, `cylinder_wall`, `cap`, `face_surface` | bodyq | BW11 |
| `round_face` (418: `extent_in`, `past_end`, …), `joint_turn` | bodyq | BW11 |
| `hole_stacks`, the recognition half of `prism` | bodyq `recognize` | BW6 |
| `curve_gap::curve_face_gap_bound`, `axial_section` (`holds_axis`), `facing_trough`, `vertex_meet` | surface (01) | SU6 (gaps), SU8 (the rest) |
| `check`: `check`, `check_scoped(level, ValidityScope::{Body, Stitched})`, `failed_tests`, `is_valid` (209) | check `check(&Brep, Scope)` | CF1 (`Scope::Stitched`, `Scope::Import`), CF2a to CF4; `failed_tests` is replaced by typed findings |
| `stitch`: `stitch`, `gather`, `sew`, `open_edges`, `merge_faces` (767) | stitch | ST1 to ST3 (`stitch`), ST5 (single-body `sew`, `gather`, `open_edges`, `merge_faces`) |
| `rebase` to a new resolution (01 §4.3, crate table) | stitch | ST6 |
| `hlr`: `prepare`, `hidden_lines(_on)`, `silhouettes`, `screen`, `scaled` (671) | hlr | HL1, HL2 |
| `outline` (143), `project_brep_onto_plane` (kernel eval/outline.rs) | hlr `outline` | HL3 |
| `plane_projection` / `curve_crossings` along a view direction (kernel api_projection.rs; intersect/api_intersectors.rs:229) | hlr (bodies), planar (curves) | HL5 |
| The sweep tool's silhouette (SmHCR scene: sweep/topology_sweep/access.rs:14, store.rs:34; primitive_creation/model.rs:494) | hlr `silhouettes`, called by make | HL5 (make → hlr is 01's edge) |
| `step_assembly` (735): `placed_bodies`, `occurrence`, `part`, `placed`, `solid`, `at`, `axis` | step product structure | SP8 |
| STEP sheets (`OPEN_SHELL`, `SHELL_BASED_SURFACE_MODEL`) | step | SP2 (01: solids and sheets) |
| Per-entity accuracies on export | step writer | SP3 |
| SmHCR itself (`blend/hcr.rs`, `api_projection.rs`, `surface_silhouette.rs`) | none | not ported; each caller above has its row (Q3) |

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
    01's module table sends `distance_bound`, `curve_surface_gap` and `seam_bound` to `scx-surface`,
    not bodyq (row SU6).
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
  tie, and in a batch the first miss ends the call (a batch convention, not an order dependence).
  It also picks by points (`edges`, `vertices`, `faces`, `face_at`, `faces_on_one_surface`) and
  re-identifies stored references (`stored_face`, `stored_edge`, with an allowance of a thousandth
  of the body diagonal). Ray hits come from the ops
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
- `face_surface.rs` builds a face's surface record for callers (`of`, `exact`, `point`, `normal`,
  `closest_point`, `domain`). 01 homes it in bodyq; on scx bodies most of it is the face's own
  `Surface` and its evaluators, so BW11 keeps only what those lack.
- `continuity.rs` reports measured drops as if they were exact.

### 2.3 Proposed shape (JUDGED)

```rust
// Memo-keeping queries take &mut Cx (R8); leaf queries take (&mut Budget, &Stop).
pub fn contact(a: &Brep, b: &Brep, cx: &mut Cx<'_>) -> Result<Contact, BodyqError>;
pub struct Contact { pub gap: Bracket<Length>, pub witness: Option<(Entity, Entity, Point3)>,
                     pub interiors: Tri /* Disjoint | Overlap | Undecided */ }
pub fn flush_contact(a: &Brep, b: &Brep, cx: &mut Cx<'_>) -> Result<Vec<Flush>, BodyqError>;

pub fn continuity(body: &Brep, edge: EdgeId, cx: &mut Cx<'_>) -> Result<Continuity, BodyqError>;
pub struct Continuity { pub proved: Order /* G0..G4 proved all along */,
                        pub measured: [Bracket<f64>; 5] /* drops with bounds */ }

pub fn pick(body: &Brep, ray: Ray, aperture: Length, cx: &mut Cx<'_>) -> Result<Vec<Hit>, BodyqError>;
pub fn pick_at(body: &Brep, points: &[Point3], want: EntityKind, cx: &mut Cx<'_>)
    -> Result<Vec<Option<Entity>>, BodyqError>;
pub fn still_names(body: &Brep, stored: Entity, at: Point3, cx: &mut Cx<'_>) -> Result<Tri, BodyqError>;
pub fn betti(body: &Brep, shell: ShellId) -> Betti;            // exact, topological
pub fn thin_places(body: &Brep, below: Length, cx: &mut Cx<'_>) -> Result<Vec<ThinPlace>, BodyqError>;
pub fn tight_box(body: &Brep, e: Entity, cx: &mut Cx<'_>) -> Result<Aabb, BodyqError>;
```

- `contact` replaces `meets`, `boundary_distance` and `interiors_disjoint` with one call returning
  the bracket. A face box tree replaces the O(Fa·Fb) start. The heap order is (lower bound, face
  ids) so ties break by id. The lower bounds are `scx-surface`'s ported `distance_bound` (SU6).
- `flush_contact` replaces `meets_each` and `flush_contact_points`. Two faces are flush when their
  surfaces coincide within coincidence (closed form for plane and cylinder pairs) and the
  projected loops overlap, proved by a planar Boolean of the loops, not by the tool's centroid.
  The result is every overlap region, not the first 8.
- `continuity` separates what is proved (from `tangent_edge`'s bounds) from what is measured (drops
  with interval bounds). The pin mixed the two.
- `pick` returns every hit within the aperture, sorted by (depth, entity id). The caller chooses.
  No `Ambiguous`. On a body with NURBS faces near the ray it waits on step 5's ray against NURBS.
- `pick_at` replaces the pin's pick by points: the entity of the wanted kind within coincidence of
  each point, `None` where there is none, an error where two are (by clearance there cannot be on
  a valid body).
- `still_names` replaces `stored_face`/`stored_edge`: ids are kept, so it checks only that the
  stored point lies on the named entity within coincidence. The pin's allowance of a thousandth of
  the diagonal is not ported.
- `betti` is exact: b0 from shells, b1 from Euler and orientability, b2 from closed shells. It needs
  no geometry and can start now.
- `thin_places` is new. It is a bracketed contact between a body's own opposite faces, sharing
  `contact`'s machinery, with face pairs that share an edge excluded near that edge. It has no pin
  truth, so its truths are closed forms (slabs, coaxial cylinders, a slot of known width), and it
  is outside step 6's "done when".
- `tight_box` is computed from finished trims, so it needs no `&mut Brep`; its cache is in
  `Scratch`. A failing edge box is an error, not `Default`. A memo hit charges the recorded work
  (R6).
- Recognition (`prism`, `hole_stacks`) moves to `scx-bodyq::recognize`. It propagates errors, and
  its constants come from `cx.doc` (the resolution), not absolute millimetres.
- The rest of the ledger (§1.3: query and measure mapping, combs, tangent chains, the
  `round_face` family) is rows BW9 to BW11.

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
- `stored_face`'s allowance of a thousandth of the body diagonal, which scales with the body
  instead of the document's resolution.

The port's tests should state what is correct, not what the pin returns.

### 2.5 Perf traps

- The O(Fa·Fb) start in contact. A face box tree with a lower-bound heap makes it O((Fa+Fb) log F)
  on bodies far apart.
- `tangent_all_along` splits up to 12 levels per edge. Across a whole body (the check's continuity
  findings) this must spend one budget, not 12 levels per edge each.
- `tight_box` was the HANG-29 site. Cache it per entity in `Scratch` (so the query takes
  `&mut Cx`, R8), never in the body. A memo hit charges the work recorded when it was filled
  (04 row 104), or the budget's stop point moves with cache warmth.

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
  - the edge/face gap is sampled at 20 points (`calc_max_edge_face_gap`, edgeuse/p1.rs:973);
  - the edge length is sampled at 5;
  - the pcurve is never measured against its edge: `curve_max_distance_between` computes it at 51
    samples and then discards the result;
  - radial order is checked at one parameter (0.456; assert_valid/edge.rs:518, 569, 982, 1197);
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
pub fn check(body: &Brep, cx: &mut Cx<'_>, scope: Scope) -> Result<Findings, CheckError>;
#[non_exhaustive] pub enum Scope {
    Import,    // structure + gaps against the recorded raises + clearance: the default at import
    Stitched,  // as Import, for a body a stitch produced (the pin's ValidityScope::Stitched)
    Full,      // everything, including FacesCross
}
pub struct Findings { pub list: Vec<Finding>, pub undecided: Vec<Undecided>, pub spent: Spent }
#[non_exhaustive] pub enum Finding {
    EdgeOffFace { edge: EdgeId, face: FaceId, gap: Bracket<Length>, allowed: Length },
    PcurveOffEdge { fin: FinId, gap: Bracket<Length>, allowed: Length },
    VertexOffEdge { vertex: VertexId, edge: EdgeId, gap: Bracket<Length> },
    VerticesWithinCoincidence { a: VertexId, b: VertexId, distance: Bracket<Length> },
    TubeNotClear { entity: Entity, reaches: Entity },
    RadialOrder { edge: EdgeId, at: f64 },
    FacesCross { a: FaceId, b: FaceId, witness: Point3, depth: Bracket<Length> },
    VertexNotManifold { vertex: VertexId },
    WideVertex { vertex: VertexId, extent: Length },
    SelfIntersectingSurface { face: FaceId, witness: Point2 },
    Degenerate { entity: Entity, what: Degeneracy },
    NotSolid { shell: ShellId },
}
```

- **What a finished `Brep` already guarantees is not re-checked.** `finish`
  (crates/scx-brep/src/draft.rs, causes in finish.rs) refuses a body whose structure is broken or
  whose edge ends, use ends and pcurves are off by its own samples, and `contract`
  (crates/scx-check/src/lib.rs:129) proves closed, oriented and manifold-edge. So the structure part
  of `check` is only what neither does: vertex manifoldness (left open by CK1, 04 row 215) and the
  sheet-or-solid question. `contract` and `check` share that pass, and `contract` calls it.
- **Gaps are certified.** Each gap is a certified upper bound from `scx-surface`'s ported
  `curve_surface_gap` and `distance_bound` (SU6). A finding needs a lower bound above the allowance.
  An upper bound above the allowance with a lower bound below it is `Undecided`, with the bracket.
  Nothing unknown passes, and nothing unknown fails as a defect. `finish`'s own sampled refusals
  (`EdgeEndOffVertex`, `UseEndOffVertex`, `PcurveOffEdge`) only refute; the certified gap is the
  check's job.
- **Clearance is new.** `VerticesWithinCoincidence` is the pair the merged-points rule refuses (R7),
  so an operation's refusal and the check name the same pair. `TubeNotClear` is 09 §6's third
  condition: a raised tube or ball reaches an entity it does not bound.
- **Radial order is checked along the whole edge**, not at 0.456. CK3 (04 row 237) has the same
  one-sample limit at the edge's middle; CF2c fixes both by certifying that the fins' order cannot
  change along the edge (their angular gaps stay above zero over interval enclosures of the edge),
  and `contract` uses it.
- **`FacesCross` is new; the pin never tests it.**
  - Tangent pairs (the faces meet along an edge with a common normal, or their bounds cannot
    separate them near a shared edge) go first to the contact classifier of CK3 and BO7, not to
    subdivision.
  - Other analytic pairs use the step-2 and step-4 SSI plus BO2b's no-touch proof.
  - A crossing counts only when its depth exceeds coincidence, so a tangent blend that touches
    within tolerance is not a finding.
  - NURBS pairs wait on step 5's intersector with certificates (SM5 to SM8). Until then they are
    listed in `undecided` as `NeedsSsi { a, b }`, never passed.
  - This needs the `scx-check` → `scx-ssi` and `scx-check` → `scx-bodyq` edges that 01's crate
    table lists; neither is in crates/scx-check/Cargo.toml yet, so CF3 adds them.
- **`check` never writes.** The importer's raises (R3) are made by `scx-step` before `finish` and
  recorded; any other accuracy growth is a defect of the operation that made the body. Repairs of
  the tolerance defects the check finds are `scx-repair`'s (layer 8), not step 6's. There is no
  `grow_accuracy` in `scx-brep` (it could not see `Findings` anyway).
- **Cost.** A face box tree gates every pairwise test, so only pairs whose boxes overlap within the
  allowance are examined. One `Budget` covers the run, with a share per edge (R6). Coincidence
  uses closed-form projection on analytic pairs.

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
- The check must stay well under the Boolean's cost on the slice's cases (CO3).
- CK1 measured the certified outward test at 2 to 400 times a volume estimate (33 ms on a torus,
  04 row 215). `Full` on every body of an assembly multiplies that by the body count, so import
  runs `Scope::Import` by default (Q8).
- `FacesCross` on tangent blends is where certified separation fails most. Tangent pairs go to the
  contact classifier first (CK3, BO7), not to repeated subdivision.

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
  4. an edge pass, `glue_edges_at_vertex` (p2:1018): greedy, with a 5-sample distance; for a line
     against a line the gap is set to 0, so nothing is raised (p2:1144);
  5. `find_and_set_infinite_region`.
- Glue raises tolerances at p5:1802 (the distance plus the deleted entity's tolerance) and p7:2210.
- Orientation, `orient_trimmed_surfaces` (topology/brep/p3.rs:4945):
  - ray votes along a fixed `ray_vec`;
  - a `wrapping_sub` underflow at topology/brep/p3.rs:5360 (`points.len().wrapping_sub(1)`), so zero
    hits read as inside;
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
pub fn stitch(sheets: &[&Brep], cx: &mut Cx<'_>, want: Want) -> Result<Done<StitchReport>, StitchError>;
#[non_exhaustive] pub enum Want { Sheets, Solid }
// StitchReport: joined edges, open edges left (each with its certified gap or bracket),
// edges left Undecided at their budget share. Lineage: Changes, one operand per input sheet.
pub fn sew(body: &Brep, cx: &mut Cx<'_>) -> Result<Done<StitchReport>, StitchError>;   // one body
pub fn open_edges(body: &Brep) -> Vec<EdgeId>;
pub fn merge_faces(body: &Brep, faces: &[FaceId], cx: &mut Cx<'_>) -> Result<Done<()>, StitchError>;
pub fn rebase(body: &Brep, to: Resolution, cx: &mut Cx<'_>) -> Result<Done<RebaseReport>, StitchError>;
```

1. **Vertices** follow the merged-points rule (R7; 04 rows 108 and 186). Each input sheet is an
   operand. Candidate pairs within coincidence come from one spatial index, built once (a grid
   whose cell is the largest coincidence, or the box tree).
   - Two vertices of one sheet within coincidence of each other are `Refused`, naming both. That
     sheet breaks vertex clearance, and `check` reports the same pair.
   - A vertex of one sheet and one of another within coincidence are identified: the lower sheet
     index's vertex is kept, with a `Measured` spread and a `Coincident` warning.
   - Vertices joined only through a chain are each anchored to the nearest operand vertex (ties by
     id), else the result is `Unsupported` with the chain. There is no transitive glue.
2. **Edges.** Candidates come from identified end vertices plus a box overlap. A pair is accepted
   when `bounds.rs`'s certified Hausdorff upper bound is ≤ coincidence. Accept by (bound, ids) order
   from a heap, not greedily by vertex. An edge with two candidates within the allowance stays open
   and is reported. Each edge has a budget share, and an edge left at it is counted `Undecided`.
3. **Splits.** An edge meeting a vertex interior to its partner is split at the certified foot point.
   Loop until no split is made, bounded by the budget, not by 10 loops.
4. **Accuracy.** Stitch identifies within coincidence, which needs no raise by definition. It never
   raises an accuracy; an import's raises are made by `scx-step` (R3) before stitch sees the sheets.
5. **Orientation.** Shared-edge parity orients each shell. For `Want::Solid`, the certified
   per-shell `volume_sign` (MA4) picks the outward sense, through a `scx-stitch` → `scx-mass` edge
   (§11 row 2). A shell whose sign is `Undecided` is reported, never guessed. Sheets are oriented by
   parity alone. Nesting uses `classify_point`; for shells with NURBS faces it waits on step 5's
   ray against NURBS (R4).
6. **No retry.** `Want::Solid` with open edges left is `Refused(NotClosed { open })`. The partial
   sheets are still available through `Want::Sheets`.
7. **Single-body operations** (ST5): `sew` stitches a body's own open edges by the same rules,
   `gather` collects sheets into one body, `open_edges` lists the open edges, and `merge_faces`
   joins faces on one surface across an edge they share.
8. **`rebase`** (ST6) moves a body to a new resolution. Coarser: the coincidence grows, so vertices
   and edges that come within it are identified by the same rules, and two vertices of the body
   that come within it are `Refused` naming both. Finer: nothing is identified, and each entity
   whose accuracy is now past the resolution keeps it with 09's `AccuracyPastResolution` warning
   (a `WarningKind` ST6 adds; main has only `AccuracyRaised` and `Coincident`).

Crate edges: stitch → bodyq (01), → mass (new, §11 row 2), → surface (for SU6's bounds, through
bodyq's dependencies). No stitch → check edge: 01 and 04 row 100 put stitch beside check, and
stitch's orientation and nesting come from mass and bodyq.

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

- The O(V·E) rescan. Use one spatial index (a grid with cells the size of the largest coincidence,
  or the box tree), built once, so candidate pairs are found in O(V) on spread-out input.
- Certified Hausdorff on long spline edges can keep halving. Use the box overlap first, then
  `bounds.rs` with a budget per edge; an edge left at its budget is `Undecided` and counted in the
  report.
- Orientation by sign is one `volume_sign` per shell. That is cheaper than the pin's ray per pass.

## 5. `scx-hlr`

### 5.1 What the pin does (READ)

The pin has three unrelated pieces.

**(A) `hlr/`** (7,527 lines). It is SolidCraft-derived and already idiomatic: its own portable Brep,
a typed `HlrError`, no `&mut`. It also carries its own geometry layer (hlr/surface.rs: ray,
inverse, `analytic(tol)` quadric recognition, grid; curve.rs, spline.rs, trim.rs, with
`&mut Vec<Hit>` out-parameters), which is a second evaluator. The entry points are `prepare` (hlr.rs:276), `hidden_lines` (:1037),
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

The app does not use it. Its other callers are more than sweep: the sweep scene
(sweep/topology_sweep/access.rs:14, store.rs:34; primitive_creation/model.rs:494),
`plane_projection` used by `curve_crossings` (intersect/api_intersectors.rs:229), and `project_brep_onto_plane`
(eval/outline.rs). It needs B-spline SSI and tracing.

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
pub fn prepare(bodies: &[(&Brep, Affine)], cx: &mut Cx<'_>) -> Result<Scene, HlrError>;
pub fn hidden_lines(scene: &Scene, view: &View, opts: &Options, cx: &mut Cx<'_>) -> Result<Drawing, HlrError>;
pub fn silhouettes(body: &Brep, view: &View, cx: &mut Cx<'_>) -> Result<Vec<Silhouette>, HlrError>;
pub fn outline(body: &Brep, plane: &Plane, cx: &mut Cx<'_>) -> Result<Vec<Curve2>, HlrError>;
pub struct Drawing { pub visible: Vec<Curve2>, pub hidden: Vec<Curve2>, pub uncertain: Vec<Curve2>,
                     pub unsupported: Vec<FaceRef> }
```

- **HLR on scx's own evaluators, not a port of (A)'s.** (A) carries a private geometry layer
  (surface, curve, spline and trim modules with their own ray, inverse and recognition), which would
  be a second evaluator. HL1 rebuilds (A)'s pipeline on `scx-surface`, `scx-curve`, `scx-bodyq` and
  `scx-ssi`: scene and body frames, exact silhouettes for plane, cylinder, cone and sphere, the
  torus silhouette, screen curves. Its tolerances come from one `Accuracy` derived from the scene
  scale and the document's resolution, named in one module. Cancel is `cx.stop`, counters go to
  `cx.trace`, `unwrap_or` becomes typed errors, out-parameters become returns. (A)'s algorithms and
  its 88 tests are what is kept.
- **Quadric recognition** (a cone of near-zero angle that is a cylinder, a cone of near-right
  angle that is a plane, a NURBS surface that is a quadric within its accuracy) is one
  `scx-surface` function (SU7), shared with `scx-step`. It returns the exact kind and the recorded
  deviation, so a recognised surface is stated as such and truth over pin holds.
- **Visibility, proved.** "Split a span when its ends classify differently" misses an occluder that
  hides only a span's interior. The sound form:
  1. find every image-space crossing of the curve with the projected silhouettes and edges of the
     other faces, certified complete (scx-planar's bounded 2-D intersections, PL6);
  2. between consecutive crossings visibility cannot change, so classify one point per span, by a
     ray from the eye decided with `classify_point`'s machinery and the face box tree;
  3. a span is `uncertain` only when the budget runs out, never because a sample count was small.
  PL6 covers lines and conics (closed form, certified roots), which is every projected edge and
  silhouette of plane, cylinder, cone and sphere bodies. Torus silhouettes and spline curves need
  step 5's general bounded 2-D intersections, and NURBS faces need step 5's ray against NURBS;
  until then those spans are `uncertain` and those faces `unsupported`, by name.
- **Freeform silhouettes** (HL4): replace the 128 grid with the step-5 marcher's normal-cone
  subdivision (SM5). The silhouette is `n·v = 0`, a curve on one surface, the same shape as SSI's
  field path (SM6a).
- **Port (C)** (outline and pieces) as it is, as `outline` (HL3).
- **Do not port (B).** Each of its callers has a home (§1.3): the sweep tool's silhouette is
  `silhouettes`, called by `scx-make` (01's make → hlr edge); `plane_projection` and
  `curve_crossings` are HL5; `project_brep_onto_plane` is `outline`.
- **Drawings are done** (HL6) when, on a named list of parts (the testkit primitives and their
  Booleans, and the analytic part of the MFCAD gate sample), the visible and hidden sets are
  produced with no `uncertain` span, and on the primitives they match the closed-form truth
  (§5.4). That is the "drawings" of 01 §10.2's step 6.

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
- One classification per span is a ray per span, so the cost grows as spans × faces. Batch the
  spans per face with one shared box tree, and give drawings their own perf row (CO3).
- Screen sampling at 1e-7·scale can be dense on long curves. Spend the budget, and sample
  adaptively by curvature.
- Parallel runs must give the same bits as serial runs. Keep the pin's test.

## 6. `scx-step`

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
pub fn read(file: &scx_p21::File, cx: &mut Cx<'_>, opts: &ReadOptions) -> Result<Imported, StepError>;
pub struct Imported { pub bodies: Vec<ImportedBody>, pub products: ProductTree,
                      pub skipped: Vec<Skipped>, pub report: Report<ImportNotes> }
pub struct ImportedBody { pub body: Brep, pub kind: SolidOrSheet, pub findings: Findings }
pub struct ReadOptions { pub assume_units: Option<Units>, pub close_gaps: bool, pub check: Scope }
pub fn write(bodies: &[(&Brep, &ProductRef)], cx: &mut Cx<'_>, opts: &WriteOptions)
    -> Result<scx_p21::File, StepError>;
```

- **One reader**, on `scx-p21`: entities → `Draft` → `finish`. There is no record Brep in between.
  It reads solids (`MANIFOLD_SOLID_BREP`, `BREP_WITH_VOIDS`) and sheets (`OPEN_SHELL`,
  `SHELL_BASED_SURFACE_MODEL`).
- **Units** come from the file's context. When they are missing, the result is `Refused(NoUnits)`
  unless `ReadOptions::assume_units` is set, and the `Report` records the assumption. Uncertainty is
  the context's `UNCERTAINTY_MEASURE_WITH_UNIT`; with none, the document's resolution. The
  accuracy-ratio warning applies (CTC-05 needs 2.05e-2).
- **Raises to enclosed gaps** (R3; 09 §6; 04 rows 41 and 42). Before `finish`, the reader measures
  each edge-to-face, vertex-to-edge and pcurve-to-edge gap with `scx-surface`'s certified bounds
  (SU6). Where an entity's certified gap exceeds its stated accuracy, the accuracy is raised to the
  enclosed gap, the tube is checked clear, and a `Raise { entity, before, after }` goes in
  `Report::raised` (crates/scx-brep/src/op.rs:243 already has the field) with an
  `AccuracyRaised` warning. Then `finish` accepts the body. Without this, `finish` refuses exactly
  the real files whose gaps exceed their stated uncertainty, and valid input would be refused. A
  gap whose bound is `Undecided` at its budget share skips that body with the bracket named.
- **Joins.** Vertices are joined where the file shares them. Where it does not and
  `ReadOptions::close_gaps` is set, the reader calls `scx-stitch` (the same rules as §4.3; no
  wider-tolerance retry). Read order cannot change the result: build in id order.
- **Kinds map to exact forms.**
  - Analytic kinds map to the `scx-surface` kinds now, through SU7's recognition where the file
    writes a quadric as another kind (a cylinder as a near-zero cone), with the deviation recorded.
  - B-spline maps to `Surface::Nurbs`.
  - Revolution, extrusion and offset map to the procedural kinds (step 5, 01 §7.3).
  - Until a kind's step-5 row lands, faces of that kind are `Unsupported(Kind)` by name, and the
    body is skipped and reported, never approximated without a record. The pending kinds are a
    finite list in 04 (§11 row 5), so a skip cannot hide an ordinary failure.
- **Pcurves.** Use closed forms where they exist, including `Curve2::OnChart` (SM1) for conics on
  quadrics; seam and pole crossings need SM1b. An analytic face whose edges are B-spline or
  intersection curves needs the step-5 fitter (04 row 130), so the analytic subset (SP2) is
  analytic surfaces *and* analytic edge curves. STEP's own PCURVE entities may seed the fitter but
  are never accepted without a certified gap check.
- **After `finish`:** `check` runs with `ReadOptions::check`, `Scope::Import` by default (Q8): the
  structure pass, gaps against the recorded raises, clearance. `Full` is on request until CO3
  measures it. A body with findings is still returned; valid input is never refused.
- **No healing beyond raises and gap closing.** Repairs that need Booleans (short edges, slivers,
  tolerance defects) are `scx-repair`'s (layer 8). `scx-import-sc` stays the converter for designs
  saved by solidcore (04 row 102) and is not on the STEP path.
- **Product structure** (SP8): `step_assembly`'s `placed_bodies` and its occurrence, part and
  placement walk become the `ProductTree` of placed instances (01's scx-step row).
- **The writer** ports write/product.rs. It writes the document's resolution as the context
  uncertainty and per-entity accuracies where they are worse (01's scx-step row), one context per
  document, not per body. Validation properties (volume, area, centroid) are written when
  `scx-mass` proves them. `step-export` is not ported.

### 6.4 Truths and tests

- Port:
  - `topology_tests` (109; "Expected values are analytic", about 21 closed-form volumes);
  - `assembly_tests` (33);
  - `geom_tests` (29);
  - `write_signed_volume.rs` (15) as it is (signed volume from the written entities);
  - `panic_fuzz` (4).
- `bench/step/references.tsv` holds 24 volume truths keyed by SHA. Its sources are mixed (review
  F7, checked here): the step-conformance corpus (dan335, CC0, its generated parts with formula
  volumes) and six SolidCraft app-repo cases (`tests/kernel-cases/import/{2043, 3098, 3208, 3730,
  4005, 4006}/expected.json`) whose licence and provenance are unverified. Port the conformance
  generator and its truths; the six are excluded unless CO2 clears their licence, and where kept
  they are marked pin-derived. The cases they test (opposing surface sense, a tiny knot span, an
  extruded ellipse over a seam, a void shell, a cylinder written as a near-zero cone, a plane
  written as a cone) are rebuilt synthetically with closed-form truths.
- New:
  - Round trip: write then read a testkit recipe, with the same volume within the certified error,
    the same topology counts and `check` clean.
  - Determinism: read twice, compare bits.
  - Units: the same part in mm and in inches gives the same body after scaling.

## 7. The STEP corpus and how it is scored

### 7.1 Sources (licence bar: MIT/Apache/BSD/CC0/CC BY; NIST public models OK)

Revised 11-27 after the review (§15, F7); the source list was checked against the pin's
`scripts/nist-pmi-corpus.sha256`, `bench/step/references.tsv` and `bench/step/fetch.sh`.

| Source | Files | Licence | Use |
|---|---:|---|---|
| MFCAD, a fixed hashed sample | 1,000 (or the whole set if smaller; the count is UNVERIFIED, research 69 D2) | MIT, primary LICENSE (research 69) | **the gate for the analytic subset**: prismatic parts, analytic faces and edges |
| NIST MBE PMI test cases | 33 files of 11 parts: AP203 (revisions rb, rc, rd and `_ap203`) and AP242 (editions e1, e2, e3) | US government work, public | the corpus for the full gate (most faces B-spline); revision pairs are a smoke signal only (§7.3) |
| dan335/step-conformance (commit 96f30bb) | 17 | CC0 as stated; **CO2 verifies the licence file** | conformance edge cases; its generated parts carry formula volumes |
| Pin fixtures (Apache), excluding the discord fixture | 5 | Apache-2.0 | small cases |
| Generated from testkit recipes via `scx-step::write` | any | ours | round trip with closed-form truths |

Excluded:

- the STEP Tools corpus (all rights reserved);
- the cadmpeg fixtures that the pin's `bench/step/fetch.sh` also fetches (licence unknown; may come
  back only if CO2 clears it under the permissive bar);
- the pin ops `tests/data/assembly_extract.stp` (provenance unknown);
- the six SolidCraft `tests/kernel-cases/import/*` truths in `references.tsv` (licence and
  provenance unverified; §6.4);
- any NC, ND, SA or ToS-bound set;
- CAx-IF practice documents, which may be cited but not used as data.

Files are fetched at test time against a SHA manifest and never committed (as `SCX_P21_CORPUS`
already does for p21, with 430,862 instances across the NIST set). The manifests and the scorer
live in `crates/scx-bench` (solidcorex has no `scripts/` directory), named in CO1 and CO4.

### 7.2 Size (READ)

- The MFCAD sample (up to 1,000 parts) is the analytic gate, so step 6 can be judged on real files
  before step 5 lands.
- NIST, the conformance set and the fixtures add about 55 files, most of them with B-spline faces;
  they gate the full corpus once SP5 and SP6 land.
- Generated round trips are unlimited and carry closed-form truths.

### 7.3 Scoring "imports and passes the check" (JUDGED)

Each file lands in exactly one category, so the totals add up and nothing can be relabelled:

| Category | Meaning |
|---|---|
| Imported | `read` returned every body; each finished |
| Imported with raises | as Imported, with one or more recorded `Raise`s (counted, with the largest ratio) |
| Skipped, pending kind | a body was skipped for a kind on 04's finite pending list (§11 row 5), and nothing else failed |
| Skipped, undecided | a body was skipped because a gap bound stayed `Undecided` at its budget share |
| Refused | a typed `Refused` (no units, clearance broken) naming its cause |
| Failed | anything else: `Defect`, a panic, a kind not on the pending list, `finish` refusing after the raises |

Then, per imported body:

| Measure | Pass when | Truth from |
|---|---|---|
| Check | zero findings not covered by a recorded raise, and zero `undecided` other than `NeedsSsi` (itself a finite named list) | `scx-check` at `Scope::Import`, and `Full` on the gate sample once CO3 shows it is affordable |
| Contract | `Proved` for each body the file declares a solid | `scx-check` |
| Volume / area | within the certified error of the truth | closed forms (generated round trips, the conformance set's formula volumes, the rebuilt synthetic cases of §6.4); the file's own validation properties where it carries them |
| Determinism | two reads give identical bits | run twice |
| Round trip | read → write → read gives the same body (topology counts, volume within error, check clean) | self |
| Time | recorded per file; not gated in step 6; one baseline row for 01 §9.4 | bench |

The pin is a smoke signal only: a file the pin reads and we do not is looked at, but the pin's
volume is not a truth. NIST revision pairs (AP203 rd against AP242 e1, …) are different model
revisions, so their agreement is neither necessary nor sufficient; they are reported as a smoke
column, not scored. There are no unit pairs in the NIST manifest.

"Step 6 done" is falsifiable:

- on the MFCAD gate sample: every file Imported or Imported with raises; zero Skipped, Refused or
  Failed; every body passes check and contract; drawings done (HL6);
- on the rest of the corpus: zero Failed; every Skipped names a kind on 04's pending list; every
  imported body passes;
- once the step-5 rows for those kinds land (SP5, SP6, CF4), zero Skipped.

The scorer is a `scx-bench` corpus runner (CO1). It writes a TSV with one row per file in the shape
of the pin's `bench/step/run.py` (opened, refused, timeouts, closed, volume within tolerance, time),
plus the category, the raise count and largest ratio, findings, undecided, and the pin smoke
column.

## 8. Task table

Revised 11-27 after the review (§15). Model: Opus where a correctness argument (certification, a
proof of a bound, completeness, an order-independence argument) is the work; Sonnet where the work
is porting tests, mapping, or plumbing. "Edges" names each new Cargo dependency a row adds; each
must be legal under layers.txt after LY1. Rows dropped since the first version: CF5
(`grow_accuracy`: raises are the importer's, R3) and SP7 (healing in `scx-import-sc`: wrong
crate, Q2).

| Id | Crate | Work | Needs | Edges | Model |
|---|---|---|---|---|---|
| LY1 | layers | Move `scx-make` to the end of the library order in layers.txt, after `scx-boolean` and, as they are added, `scx-stitch` and `scx-hlr` (01: make is layer 8, on boolean, stitch, check, hlr); move bodyq's and check's dev-dependencies on make to testkit recipes; new step-6 crates take test bodies from testkit | — | — | Sonnet |
| SU6 | surface | Port `distance_bound`, `curve_surface_gap`, `seam_bound` and `curve_gap::curve_face_gap_bound` (01's module table): certified edge-to-face, vertex-to-edge, pcurve-to-edge and curve-to-curve gap brackets, analytic and NURBS pieces | — | — | Opus |
| SU7 | surface | `recognize(&Surface, accuracy) -> Option<(Surface, deviation)>`: quadrics written as other kinds or as NURBS; truths rebuilt from the 4005 and 4006 cases synthetically | — | — | Opus |
| SU8 | surface | Port `axial_section` (`holds_axis`), `facing_trough`, `vertex_meet` | — | — | Sonnet |
| PL6 | planar | Certified, complete bounded 2-D crossings of lines and conics (with multiplicity), for drawings; spline and higher-degree curves after step 5's planar widening | — | — | Opus |
| BW1 | bodyq | `betti` per shell; tests against testkit `genus` (in progress) | — | — | Sonnet |
| BW2 | bodyq | `tight_box` on finished trims, cached in `Scratch`, memo hits charge the recorded work; port issue_1799 and issue_1054 tests | — | — | Sonnet |
| BW3 | bodyq | `contact` on SU6's bounds, face box tree, heap by (bound, ids), budget for MAX_PAIRS; port 12 tests | BW2, SU6 | — | Opus |
| BW3b | bodyq | `flush_contact`: coincident surfaces plus loop overlap by a planar Boolean | BW3 | bodyq → planar (01) | Opus |
| BW4 | bodyq | `continuity` and `tangent_all_along`: proved order separated from bounded drops; port 4+18+6 tests. Analytic now; NURBS edges after SM1b | — | — | Opus |
| BW5 | bodyq | `pick` (ray): every hit within the aperture, sorted by (depth, id); NURBS faces after step 5's ray against NURBS | BW2 | — | Sonnet |
| BW6 | bodyq | `recognize`: prism recognition and hole_stacks, errors propagated, constants from the resolution; port 73 prism tests | — | — | Sonnet |
| BW7 | bodyq | `thin_places` (new; closed-form truths; outside "done when") | BW3 | — | Opus |
| BW8 | bodyq | `pick_at` by points and `still_names` (stored references) | BW2 | — | Sonnet |
| BW9 | bodyq | The query and measure map: one line per pin function of §1.3 to its scx function; port the missing ones (2-D helpers to planar) | BW2 | — | Sonnet |
| BW10 | bodyq | Combs, principal curvatures, sheet seams, `tangent_face_chain_ordered` | BW4 | — | Sonnet |
| BW11 | bodyq | `face_pair`, `mid_sheet`, `mid_joint`, `cylinder_wall`, `cap`, `face_surface`, `round_face`, `joint_turn` | — | — | Sonnet |
| CF1 | check | Shared structure pass (`contract` calls it): vertex manifoldness, solid-or-sheet; `Findings`, `Finding`, `Scope` types | — | — | Sonnet |
| CF2a | check | Certified gaps on analytic geometry (edge/face, vertex/edge, pcurve/edge), wide vertex, degeneracy; `Scope::Import` and `Stitched`; one-defect recipes at δ = 10·acc, acc, acc/10 | CF1, SU6 | — | Opus |
| CF2b | check | Clearance: `VerticesWithinCoincidence`, `TubeNotClear` (09 §6.3) | CF1, BW2 | check → bodyq (01) | Opus |
| CF2c | check | Radial order certified along the whole edge; `contract`'s CK3 test uses it | CF1 | — | Opus |
| CF3 | check | `FacesCross` on analytic pairs, with depth; tangent pairs to the CK3/BO7 contact classifier first; `NeedsSsi` otherwise | CF2a | check → ssi, check → bodyq (01) | Opus |
| CF4 | check | Gaps and `FacesCross` on NURBS pairs | CF2a, CF3, SM5-SM8 | — | Opus |
| CF6 | check | Port the truth tests of §3.4; "check never writes" bit compare over the testkit recipes | CF2a | — | Sonnet |
| ST1 | stitch | Crate; `stitch` signature and `Done<StitchReport>` with n-operand lineage; vertex identification by rows 108/186 over a spatial index | — | stitch → bodyq | Opus |
| ST2 | stitch | Edge pairing by certified Hausdorff (port `bounds.rs`), heap acceptance, budget share per edge, ambiguity left open; splits | ST1, SU6 | — | Opus |
| ST3 | stitch | Orientation: parity; outward by MA4 `volume_sign`; nesting by `classify_point` (analytic now; NURBS after step 5's ray against NURBS); `Want::Solid` refusal | ST2 | stitch → mass (§11 row 2) | Opus |
| ST4 | stitch | Truth tests: exploded recipes, perturbation at ±coincidence, permuted input bit-identical; port pin tests | ST3 | — | Sonnet |
| ST5 | stitch | `sew`, `gather`, `open_edges`, `merge_faces` | ST2 | — | Sonnet |
| ST6 | stitch | `rebase` to a new resolution; the `AccuracyPastResolution` warning kind | ST2, CF2b | — | Opus |
| HL1 | hlr | Crate; HLR on scx evaluators: frames, exact quadric and torus silhouettes (SU7), `Stop`/`Trace`, typed errors, named tolerances | SU7 | hlr → bodyq | Opus |
| HL2 | hlr | Visibility: all crossings (PL6), one classification per span, batched per face over a box tree; `uncertain` only on Exhausted | HL1, PL6 | hlr → planar | Opus |
| HL3 | hlr | Port (C) as `outline`; port the 88 tests; new closed-form view tests | HL1 | — | Sonnet |
| HL4 | hlr | Freeform silhouettes by normal-cone subdivision and the field path | HL2, SM5, SM6a | hlr → ssi | Opus |
| HL5 | hlr | Projection along a direction (`plane_projection`, `curve_crossings`) and the `silhouettes` API that make's solid sweep calls | HL1 | — | Sonnet |
| HL6 | hlr | Drawings done: the named part list, closed-form truth on primitives, a perf row | HL2, HL3, HL5, CO4 | — | Sonnet |
| SP1 | step | Crate on scx-p21: units (refuse missing unless assumed), uncertainty from the context, `Imported`, typed `StepError`, solids and sheets | — | step → p21 | Sonnet |
| SP2 | step | The analytic subset (analytic surfaces *and* analytic edge curves): Draft → enclosed-gap raises recorded → finish; closed-form and OnChart pcurves (seam and pole cases after SM1b); joins as the file shares them; orientation by parity and volume sign; `Scope::Import` check; port topology, assembly and geom tests on analytic files | SP1, SU6, CF1 | step → check, mass, surface (mass: §11 row 2) | Opus |
| SP2b | step | Quadrics written as other kinds, through SU7, with the deviation recorded | SP2, SU7 | — | Sonnet |
| SP3 | step | Writer: resolution as the context uncertainty, per-entity accuracies, one context per document, validation properties; `write_signed_volume` tests; round trip | SP2 | — | Sonnet |
| SP4 | step | `ReadOptions::close_gaps` through `scx-stitch` | SP2, ST2 | step → stitch | Sonnet |
| SP5 | step | B-spline curves and surfaces; pcurves by the step-5 fitter | SP2, the NURBS surface kind, the step-5 pcurve fitter, CF4 | — | Opus |
| SP6 | step | Revolution, extrusion and offset kinds to the procedural kinds | SP2, the step-5 procedural kinds | — | Opus |
| SP8 | step | Product structure: placed instances (`placed_bodies`, occurrences, parts) | SP1 | — | Sonnet |
| CO1 | bench | Corpus runner in `crates/scx-bench`: manifest fetch, the categories and TSV of §7.3, determinism, round trip, the pin smoke column | SP2, CF2a | — | Sonnet |
| CO2 | testkit | The conformance generator and truths; licence checks for dan335, cadmpeg and the six kernel-cases truths (in progress) | — | — | Sonnet |
| CO3 | bench | Perf rows, before and after: `check` at each scope against the Boolean on the slice's cases; `Full` against `Import` on the gate sample; stitch on exploded recipes; STEP read per file; drawing time | CF2a, ST3, SP2, HL2 | — | Sonnet |
| CO4 | bench | The MFCAD gate: verify the model count, fix and hash a sample of up to 1,000 parts, manifest in `crates/scx-bench` | — | — | Sonnet |

### 8.1 Graph and critical path

```
LY1 (first: before any new crate takes test dependencies)
SU6 ──┬─> CF2a ──┬─> CF3 ──> CF4 (SM5-SM8)
      │          ├─> CF6
      │          └─> CO1, CO3
      ├─> BW3 ──┬─> BW3b
      │         └─> BW7
      ├─> ST2 ──┬─> ST3 ──> ST4
      │         ├─> ST5
      │         └─> ST6 (with CF2b)
      └─> SP2 (with SP1, CF1) ──┬─> SP2b (SU7), SP3, SP4 (ST2)
                                ├─> SP5 (fitter, NURBS kind, CF4)
                                ├─> SP6 (procedural kinds)
                                └─> CO1 ──> gate on CO4's sample ──> full corpus
CF1 ──> CF2a, CF2b (BW2), CF2c
SU7 ──> HL1 ──┬─> HL2 (PL6) ──┬─> HL4 (SM5, SM6a)
              │               └─> HL6 (HL3, HL5, CO4)
              ├─> HL3
              └─> HL5
BW2 ──> BW3, BW5, BW8, BW9;  BW4 ──> BW10;  BW1, BW6, BW11 alone
ST1 ──> ST2;  SP1 ──> SP2, SP8
```

- **Can start now** (no input that is not on main): LY1, SU6, SU7, SU8, PL6, BW1 (in progress),
  BW2, BW4 (analytic), BW6, BW11, CF1, ST1, SP1, SP8, CO2 (in progress), CO4. LY1 goes first or
  alongside, since ST1, HL1 and SP1 create crates whose tests must not depend on `scx-make`.
- **Startable once a step-6 row lands:** BW3, BW3b, BW5, BW7 to BW10, CF2a to CF3, CF6, ST2 to ST6,
  HL1 to HL3, HL5, HL6, SP2 to SP4, CO1, CO3.
- **Waits on step 5:**
  - CF4 on SM5-SM8;
  - HL4 on SM5 and SM6a;
  - SP5 on the NURBS surface kind, the pcurve fitter and CF4;
  - SP6 on the procedural kinds;
  - the NURBS half of BW4 and SP2's seam and pole cases on SM1b;
  - the NURBS half of BW5, ST3's nesting and HL2's spans on NURBS faces on step 5's ray against
    NURBS;
  - HL2's torus and spline spans on step 5's general bounded 2-D intersections in scx-planar.
- **The critical path to the analytic gate** is SU6 → CF2a, with SP1 and CF1 in parallel → SP2 →
  CO1 on CO4's MFCAD sample. **To the full corpus** it continues through SP5, which is held up by
  step 5's pcurve fitter, and CF4. Drawings (SU7 → HL1 → HL2 → HL6) run beside it.
- **Layers.** After LY1, layers.txt gains `scx-stitch` and `scx-hlr` after `scx-check` (layer 6),
  `scx-step` after them (layer 7), and `scx-make` stays last; each crate is added when
  its first task lands (04 row 102).

## 9. Open questions

Revised 11-27 after the review (§15). Q2, Q5, Q6, Q7 and Q8 changed; the coordinator's provisional
rulings are applied, with one dispute (Q7) argued in §15.

**Q1. Do `contract` and `check` stay two functions?**

- Recommend: yes, with one shared structure pass that `contract` calls (CF1). `contract` is the
  cheap kernel-edge proof every operation can afford; `check` is the full list. The shared pass
  holds only what `finish` and `contract` lack (vertex manifoldness, solid or sheet).
- Against: two entry points drift, and a caller may read `contract` as the full check. Mitigation:
  the doc says so, and a test asserts that every `contract` failure is a `check` finding.

**Q2. Where does STEP healing live?**

- Recommend (the coordinator's ruling): `scx-step` measures enclosed gaps and records the raises in
  its `Report` (R3; 09 §6; 04 rows 41 and 42). Gap closing is `scx-stitch`'s, reached by
  `ReadOptions::close_gaps` (SP4). Repairs that need Booleans are `scx-repair`'s. `scx-import-sc`
  stays the converter for designs saved by solidcore (04 row 102). SP7 is dropped.
- Against: the reader then does real geometry work (certified gaps on every edge) that a pure
  mapper would not, and its cost lands on every import. Mitigation: the gaps are SU6's bounds,
  which `check` also uses, and they are box-gated; CO3 measures it.

**Q3. Drop SmHCR (5.1 B)?**

- Recommend: drop it, with a row for each of its callers (§1.3: HL5 for projection along a
  direction and the tool silhouette, HL3 for `project_brep_onto_plane`), and HLR built on scx's own
  evaluators (HL1).
- Against: (B) handles freeform silhouettes now, where HL1 leaves them `unsupported` until HL4.
  Mitigation: HL4 is on the SM5/SM6a path step 5 already builds, and no app caller uses (B).

**Q4. Are Betti numbers and thin places in step 6's scope?**

- Recommend: Betti yes (BW1, small, exact). Thin places yes but last (BW7), with closed-form truths
  only (slabs, coaxial cylinders, slots), and outside step 6's "done when".
- Against: thin places near blends are the same hard tangent case as `FacesCross`, with no pin
  truth.
  Mitigation: BW7's definition is fixed (opposite faces not sharing an edge, closer than `below`,
  a certified bracket), and it does not gate the step.

**Q5. Keep the pin's feature re-read at 0.25 × the shortest edge?**

- Recommend: no. Accuracy comes from the file's uncertainty and the document's resolution, with the
  accuracy-ratio warning. A gap the file does not meet is a recorded raise in `scx-step`'s report
  (R3); a feature smaller than the resolution is a `Degenerate` finding. #18848's case becomes a
  test only if its file passes the licence bar; otherwise it is rebuilt synthetically.
- Against: #18848 imported only with the re-read. Mitigation: the raise covers the gap it closed,
  and is visible in the report rather than hidden in a re-read.

**Q6. Is the corpus big enough to gate step 6?**

- Recommend (the coordinator's ruling): add a fixed, hashed MFCAD sample (MIT; up to 1,000 parts,
  once CO4 verifies the count) as the gate for the analytic subset. Drop the cadmpeg fixtures,
  `assembly_extract.stp` and the six unverified kernel-cases truths unless CO2 clears them.
- Against: MFCAD parts are synthetic prismatic models from one generator, so they test the analytic
  reader broadly but not the messy exports where import fails. Mitigation: NIST and the
  conformance set cover other exporters, and the generated round trips cover our own writer.

**Q7. How is the corpus scored without reference volumes?**

- Recommend: truth from closed forms (generated round trips, the conformance set's formula
  volumes, the rebuilt synthetic cases), the file's validation properties where present, and the
  check and contract. The pin is a smoke signal only. NIST revision pairs are reported, not scored
  (§15, F7 and Q7: pre-qualifying pairs by agreement and then scoring agreement is circular).
- Against: most NIST bodies then have no volume truth at all, only the check and the contract.
  Mitigation: the certified volume error and the round trip at least bound what the reader built;
  a wrong-but-consistent reading of a NIST file is the remaining blind spot (§10).

**Q8. Which check runs at import, and inside operations?**

- Recommend (the coordinator's ruling): operations run `contract` only. Import runs
  `Scope::Import` (structure, gaps against the recorded raises, clearance) by default, and `Full`
  on request, until CO3 shows `Full` is affordable. Difftests run `Full` on every result.
- Against: 02 §10 says the checker becomes load-bearing, and `FacesCross` is exactly what a sloppy
  exporter gets wrong. Mitigation: the MFCAD gate runs `Full` in the scorer, so the default can
  change on CO3's numbers.

**Q9. Is an `Undecided` gap a pass or a fail?**

- Recommend: neither. It is its own category (§7.3), and done needs zero `Undecided` outside
  `NeedsSsi`, which is a finite named list in 04 (§11 row 5). The budget share used is recorded per
  row, so a result is reproducible (04 row 104).
- Against: a loose but correct bound on a long spline blocks done for a reason that is not a
  defect, and "refine the budget" can grow without bound. Mitigation: the bracket is reported, and
  a bound that does not tighten with budget is a bug in SU6, not a reason to pass.

**Q10. (New) May the analytic gate be met before the NURBS rows land?**

- Recommend: yes. The MFCAD sample is analytic, so SP2, CF2a and CO1 can meet it on step-6 rows
  alone, and "step 6 done" then waits only on the full corpus's step-5 rows.
- Against: a green analytic gate may be read as "STEP done". Mitigation: §7.3 states the two gates
  separately, and 05's row says which is met.

## 10. Against this plan

- **The pin's check samples; ours certifies, which is slower.** Certified Hausdorff on every
  edge/face pair may cost more than the pin's 20 samples, and the pin's Level 2 is already 6.5× its
  Boolean. The plan rests on the claim that closed-form projection on analytic pairs plus the box
  tree makes it cheaper, but that is unmeasured. CO3 must measure before CF2a goes ready, and if
  the claim fails, CF2a needs a cheaper certified bound (a Lipschitz bound per span), not samples.
- **Most of the corpus waits on step 5.** NIST parts are largely B-spline faces with fitted pcurves,
  so "the STEP corpus imports and passes the check" cannot be green until SP5, the fitter and CF4
  land. The analytic subset may be small. If it is near empty, CO1 green on it proves little, and
  the plan's "start now" mainly buys plumbing. That is still worth doing, but the progress it shows
  is smaller than the row count suggests.
- **Recorded raises may not cover what the pin healed.** The pin heals on load, and its pass
  record (not measured, but implied by its users) comes partly from healing. `scx-step` raises
  only to enclosed gaps and closes open edges only with `close_gaps` (SP4), so its default score
  may look worse than the pin's. The corpus TSV must say whether `close_gaps` ran.
- **No retries may refuse files the pin closes.** R3 (outside the importer's recorded raise)
  leaves open what the pin's tolerance ladder glued. Some of those glues were right: a sloppy
  exporter whose gaps exceed its own stated uncertainty. The plan reports them open. The user sees a failure where the pin showed a solid.
  This is the truth-over-pin rule applied, but it should be visible in the corpus TSV as its own
  column (open within 10× coincidence), so the cost of the rule is measured.
- **Most NIST bodies have no volume truth** (Q7). Revision pairs are smoke only, so a
  wrong-but-consistent reading passes the check, the contract and the round trip.
- **`FacesCross` is new and unproven at scale.** The pin never tested face crossing, so there is no
  record of how often real files have it. If NIST files commonly have tiny crossings within
  tolerance at tangent blends, CF3 will flood findings. Its allowance must be coincidence, and a
  crossing whose depth is under coincidence is not a finding. That needs a depth measure, which
  CF3 must include.
- **HL2's proved visibility may be slow on dense drawings.** The pin's 5-sample classify was cheap,
  so the port may regress drawing time. A drawing is interactive, so HL2 needs its own perf row
  (add it to CO3).
- **(11-27 revision) The importer now raises accuracies.** R3's exception is 09 §6 applied, but it
  moves real geometry work into the reader, and a reader that raises can mask a writer bug in our
  own round trips. Round-trip tests therefore assert zero raises.
- **(11-27 revision) Per-sheet refusal can be harsh.** Under rows 108 and 186, a sheet with two of
  its own vertices within coincidence is `Refused`. On a STEP file that skips the body, scored as
  Refused, not the whole file; if MFCAD or NIST shows it often, the rule needs a ruling for
  imported data, not a quiet exception in stitch.
- **(11-27 revision) The table is wider still.** Forty-eight rows across seven crates, sixteen
  startable now. The spine is SU6, CF1, CF2a, SP1, SP2, CO1 and CO4; if builders are scarce, run
  those first.
- **This note measured nothing on the corpus.** Every perf claim above is from the pin's numbers
  (Menger 3.1 s against 0.48 s) or from judgement.

## 11. Proposed 04 rows (for the coordinator's ruling and Brett's review)

Revised 11-27 after the review (§15).

1. **Step 6 plan (research 74, as revised).**
   - The full `scx-bodyq`, `scx-check`, `scx-stitch`, `scx-hlr` and `scx-step` take `&Brep`.
   - Stitch returns `Done<StitchReport>` with lineage per input sheet. `scx-step::read` returns
     `Imported`. Neither uses `Made`.
   - Sampled decisions become certified brackets or `Undecided` (R2).
   - No retry with a wider tolerance, except the importer's recorded raise to an enclosed gap (R3).
   - Orientation by parity plus the certified volume sign, with no ray votes (R4).
   - Typed `Finding`s, with the clearance finding (R5).
   - Caps become budget, and memo hits charge the recorded work (R6).
   - Vertex identification by rows 108 and 186 (R7).
   - Memo-keeping queries take `&mut Cx`, leaf queries `(&mut Budget, &Stop)` (R8).
   - Every pin capability of these crates has a home (§1.3). Tasks: §8.
2. **Crate edges.**
   - LY1 moves `scx-make` to the end of the library order (01's layer 8).
   - New edges:
     - `scx-stitch` → `scx-mass`, for the volume sign;
     - `scx-step` → `scx-mass` and `scx-surface`;
     - `scx-check` → `scx-bodyq` and `scx-ssi`, which 01 lists;
     - `scx-hlr` → `scx-planar` and `scx-ssi`.
   - There is no `scx-stitch` → `scx-check` edge.
   - Step-6 crates take test bodies from `scx-testkit`, never from `scx-make`.
3. **The check: `scx_check::check(&Brep, &mut Cx, Scope) -> Result<Findings, CheckError>`.**
   - The scopes are `Import`, `Stitched` and `Full`.
   - It shares its structure pass with `contract`, and checks only what `finish` and `contract`
     lack.
   - It never writes. Accuracy growth is the importer's recorded raise, and repair is
     `scx-repair`'s.
   - New findings: face crossing (with depth), vertex manifoldness, clearance and tube-clear.
   - Radial order is certified along the whole edge, for `contract` too.
   - NURBS pairs are `NeedsSsi` until SM5-SM8.
4. **STEP import.**
   - `scx-step` records enclosed-gap raises in its report.
   - Gap closing goes through `scx-stitch` (`close_gaps`). Boolean-level repair is `scx-repair`'s.
   - `scx-import-sc` stays the converter for saved solidcore designs.
   - Missing units are refused unless assumed explicitly. Uncertainty comes from the file's
     context.
   - Import runs `Scope::Import` by default, and `Full` on request until CO3 measures it.
5. **"Imports and passes the check" is scored by §7.3.**
   - Each file lands in exactly one category.
   - The analytic gate is a fixed, hashed MFCAD sample, with zero Skipped, Refused or Failed.
   - On the rest of the corpus, every skip names a kind on a finite pending list kept in 04. The
     first list is: B-spline curves and surfaces with fitted pcurves (SP5); revolution, extrusion
     and offset surfaces (SP6); conic pcurves crossing a seam or pole (SM1b); and `NeedsSsi` for
     NURBS face pairs (CF4).
   - Truth comes from closed forms, validation properties, the check and the contract. The pin is
     a smoke signal only, and NIST revision pairs are not scored.
   - Excluded sources: the cadmpeg fixtures, `assembly_extract.stp` and the unverified
     kernel-cases truths, each unless CO2 clears it.
6. **SmHCR is not ported.**
   - Each of its callers has a row: HL3 for the outline; HL5 for projection along a direction and
     the sweep tool's silhouette.
   - HLR is built on scx's own evaluators, with quadric recognition in `scx-surface` (SU7), which
     `scx-step` shares.
   - Visibility is proved: all image-space crossings (PL6), then one classification per span.
   - Drawings are done by HL6.

## 12. What this note did not do

- It ran nothing on the STEP corpus. The files were not present, and the pin has no recorded pass
  rates, so every pass-rate statement here is a plan.
- It did not measure the check's cost on scx bodies. It cites the pin's numbers only.
- It did not verify the dan335/step-conformance licence file (CO2 does).
- It read no code from other kernels for this note.

## 13. BW1 as built (11-27)

`scx_bodyq::betti(&Brep, ShellId) -> Result<Betti, BodyqError>` with `Betti { b0, b1, b2 }`.
Commits on branch `bw1`: the function and its tests, then `recipes.rs` using it.

**What changed from §2.3.**

- It returns a `Result`: a shell the body lacks is `Refused(NoShell)`, and a shell whose cells give
  a negative `b1` (or overflow the elimination) is `Defect(Betti)`. The row said `-> Betti`; a
  failing default for a bad id would break R-rules, so the error is typed.
- `b2` is not "closed shells". The first version counted closed components, and the testkit
  `Touching` kind (five blocks of one shell pinched along four edges of four fins) gave a negative
  `b1`. `b2` is now the nullity of the face-by-edge boundary matrix over the rationals (each face
  once, oriented by its own loops), found by exact integer elimination (rows shortest first,
  `i128`, overflow is the `Defect`). `b1 = b0 + b2 - chi`. A face's side does not matter (a flipped
  column has the same nullity), so a sheet (both sides in the shell) is one disc and gives `b2 = 0`.

**How shared cells are counted (agrees with recipes.rs `genus`).** The complex is one shell's own
faces, wire edges and lone vertices. A vertex or edge used by faces of two shells is one cell in
each shell's complex, so bodies touching at a vertex or along an edge, each its own shell, keep
their own numbers (corner-contact-union and edge-contact-union give two spheres). Two faces of one
shell meeting at a vertex or edge share that cell: `Touching` gives `b0 = 1`, `b2 = K` blocks,
`b1 = n^2 - K + 1`, the cycles of the graph of blocks. A face is a sphere with one hole per loop
(`2 - L`), except a loopless torus (`0`). A collapsed (pole) edge is no 1-cell; a seam is.

**recipes.rs.** Its private `genus` is gone. The test's genus is the sum of `b1 / 2` over the shells of
solid regions, checking `b1` even and the shell count. All eight recipes tests pass unchanged (D5
contacts, gear-60, plate-12 with b1 = 288 for 144 holes). The old and new answers are equal on every
case.

**Tests** (`crates/scx-bodyq/tests/betti.rs`): every shell of Block, Prism, Cylinder, Cone, Sphere,
Cavity, Stack is (1,0,1); Torus and Holed (1,2,1), over three seeds and sizes 1, 2, 4; a cavity's two
solid shells; sheets (1,0,0); Checker's blocks (1,0,1) and Touching's closed form at sizes 1 to 4;
the makers' block, cylinder, cone (truncated and to an apex), sphere and torus; determinism and the
missing shell. Nothing removed.

**Cost** (side-channel timers, release, a 24-core build host, one call): torus 1 face 14 us; torus 400 faces
1.4 ms; block 2,400 faces 8.3 ms; Touching 1,326 faces 3.3 ms; plate-12's solid shell 5.8 ms. The
call is not on any existing path; recipes.rs now makes one per shell where it counted before
(microseconds).

**Open.** `b1` of a non-manifold shell is the complex's, not the "surface's"; a caller wanting handles
of each lump should split the shell first. A face of genus above zero with loops is not
representable in the rule `2 - L`; a loopless non-torus periodic face is taken as a sphere.

**Proposed 04 row.** `betti` is exact rational homology of a shell's cell complex, per shell, cells
counted once per shell; typed refusal for a missing shell.

- The 11-27 revision (§15) measured nothing new either. It re-read the pin's ops modules for the
  ledger and checked the review's citations and corpus claims against the pin's files.

## 14. CO2 as built (11-27)

Trust: licence evidence READ at each source's own page or licence file on 11-27 (quoted below);
hashes, sizes and the volumes MEASURED by running the code. Proposed for review.

### 14.1 Licence verification

The bar is 04 and the corpus rule: usable and shareable by anyone; MIT, Apache, BSD, CC0, CC BY, US
government public domain; nothing NC, ND, SA or bound by terms of service. Each source was read
at the source, not from a secondary claim.

| Source | Files kept | Licence | Evidence | Verdict |
|---|---:|---|---|---|
| NIST MBE PMI models (`nist.gov/document/nist-pmi-step-files`, archive SHA-256 `8fa78429...1911`, the same archive the pin pins) | 33 of 45 members (the 12 PDFs are left out) | US government work, public domain | The group's page (`.../mbe-pmi-0`, "Created August 29, 2016, Updated December 30, 2025") has a "NIST Disclaimer": "The test cases, CAD models, and STEP files can be used without any restrictions." It asks for acknowledgement and forbids the NIST logo in promotion, which a test corpus does not touch. `nist.gov/oism/copyrights`: material not marked copyrighted "may be distributed or copied". | pass |
| dan335/step-conformance, rev `3f6d18b` (its head on 11-27) | 19 generated parts with their 19 truth files, and `freeform_panel.step` | CC0 1.0 | `corpus/LICENSE` is the CC0 1.0 Universal legal code; the README says "The parts in `corpus/` are CC0" and "A part has to be yours to give away under CC0". The code is MIT or Apache-2.0 (not used). | pass |
| step-conformance shop parts (`hinge_jig`, `lego_*`, `door_handle_guide`, `rail_guide`, `sled_base`, `bowl_01`) | 0 | CC0 as claimed | The README says they are Fusion exports from `github.com/dan335/camcraft`, which answers "Repository not found" (private or removed). Origin unverifiable, so the claim rests on the author's word alone. | dropped |
| MFCAD, `github.com/hducg/MFCAD` rev `ef6d58a` | 7 of 15,488 `.step` files, plus the licence text | MIT | `LICENSE`: "MIT License, Copyright (c) 2020 hducg". The files are written by Open CASCADE 6.9 (header) from the authors' generator. Its `.face_truth` files are Python pickles: never fetched, never unpickled. | pass |
| Pin fixtures, from the public mirror `tangency-labs/solidcore` at the commit that last changed each (bytes equal the pin's) | `closed-spline-cylinder`, `rod_doubled_edge`, `stepped_tapered_hole` | MIT OR Apache-2.0 | SolidCraft's `ATTRIBUTION.md` has a row for each ("original synthetic", "written by SolidCraft", "invented block"; no third-party geometry). | pass |
| | `tetra_disk.stp` | Apache-2.0 by the repository's licence | Header: "hand-written test", a unit tetrahedron; no attribution row of its own. The weakest evidence kept; one line to drop if review wants a row. | pass, flagged |
| | `assembly_extract.stp` | unknown | `step_assembly_real.rs`: "an extract of an assembly exported by HOOPS Exchange, with its product names made neutral"; a third party's assembly, no attribution row. | dropped (as the coordinator asked) |
| | `discord-2026-10-30-fillet.step` | unknown | A user's report from a Discord channel; 74 §7 already left it out. | dropped |
| cadmpeg fixtures (in the pin's `bench/step/fetch.sh`) | 0 | unknown | No licence checked or cited; not in 74 §7. | dropped (as the coordinator asked) |
| STEP Tools samples | 0 | all rights reserved | 74 §7 | dropped |

The corpus is 4 sources, 86 manifest rows (the NIST archive, 33 of its members, 40 step-conformance
rows including its licence, 4 pin fixtures, 7 MFCAD files and its licence), 72,129,304 bytes in all.
74 §7 counted 17 step-conformance files; the repository's head has 19 generated parts. The 17
the pin used have the same SHA-256 at the head, so nothing the pin measured moved.

### 14.2 What was built

- `corpus/manifest.tsv`: `name`, `url`, `sha256`, `size`, `licence`, `part` (`file`, `archive`, or
  `member:<archive row>`). Every URL is pinned to a commit or to the NIST archive, whose hash is in
  the row. Nothing downloaded is committed.
- `cargo xtask corpus fetch | verify [prefix ...]`: needs `SCX_CORPUS_CACHE`. Each file is written
  to `<name>.part`, checked for SHA-256 and size, then renamed; an existing file with another
  content is an error, never overwritten. Downloads are only written and hashed; the archive's one
  member is read by `python3 -I` `zipfile` (exact member name, at most `size + 1` bytes). SHA-256
  is in `xtask` itself (FIPS vectors in its tests), so only `curl` and `python3` are needed.
  Against: shelling out to two tools; for: xtask stays dependency-free (`layers.txt`), and an
  archive reader of our own would be more code to trust. The manifest parse exists twice (xtask,
  `scx-testkit::corpus`) because a tool crate may depend on no workspace crate.
- `scx_testkit::corpus`: `parse_manifest`, `truth_volume`, `references_tsv`, and the example
  `corpus_references` (`cargo run -p scx-testkit --example corpus_references -- corpus/manifest.tsv
  "$SCX_CORPUS_CACHE" > corpus/references.tsv`).

### 14.3 The `references.tsv` generator

The pin's `bench/step/references.tsv` is a hand table of 24 rows keyed by SHA-256: 18
step-conformance parts, whose volumes come from the generator's profiles, and 6 SolidCraft
`kernel-cases` `expected.json` volumes. The port computes the 18 from the truth files and adds
`plate_900_holes`: **19 rows**, 17 of them checked against the pin's values. The 6 `kernel-cases`
rows are not carried over: their files and expected values are SolidCraft's own and neither is
verified (as the coordinator ruled), so only truths we can derive stay.

Closed forms (units: mm, mm^3). A solid of revolution has a closed profile in (axis, radius)
coordinates, and its volume is `pi |∮ r² da|`: per segment `(a1-a0)(r0²+r0 r1+r1²)/3`, per arc the
antiderivative of `-R (cr + R sin t)² sin t`. A prism's is height times the area of its largest loop
less the others' (holes); the area of an arc is `½(R² sweep + R(cx Δsin - cy Δcos))`. A profile
that does not close to 1e-9 of its extent is `CorpusError::Open`, not a guess; the JSON reader
refuses escapes and nests no deeper than 32.

Tests (`crates/scx-testkit/tests/corpus.rs`): cylinder, hemisphere and half torus against `pi r² h`,
`2/3 pi r³`, `pi² R r²` (within 1e-13); a plate with a hole listed first; two solids; each refusal
by type; the committed manifest well formed, no name or step file twice, every licence permissive;
all 17 pin volumes within 1e-6 (the pin keeps 3 decimals); and, with the cache present, the committed
`references.tsv` equals what the generator writes. The values are full `f64` text; the pin's are
rounded to 3 decimals.

Choices: volumes in `f64` (not double-double): the sums are a few dozen terms, good to about 1e-13,
which is below any tolerance step 6 scores against; revisit if CO1 certifies volumes tighter.
"Largest loop is the outer" is enough for the plates; a prism with nested islands would need a nesting
test, and the generator refuses nothing there, so a new part kind should come with its own test.

### 14.4 MFCAD sample (the fast gate)

Seven files, 606,239 bytes, taken as every 2,500th name of the sorted list of 15,488 (so fixed, with no
choice of mine in it): 11 to 43 faces each. All seven have only planes (no B-spline, revolution or
cylinder entity), so they need no SM kernel kind and exercise `scx-step` analytic reading, the
check and the contract. Why seven: the set is a plain prismatic one, so more files add
little geometry kind but cost time; 0.6 MB reads in well under a second, which suits a gate that
runs on every push. The set is 1.2 GB; a wider sample is one more manifest row per file. MFCAD has no
volumes, so these files score on check, contract, determinism and round trip, not on volume.

### 14.5 NIST pairs: revisions, not unit pairs

The 33 files are 16 parts. By part: ctc 01-05 have an AP203 geometry file, an AP203 file with PMI and
an AP242 file; ftc 06-11 have an AP203 geometry file and an AP242 file (ftc 08 has two AP242
editions); stc 06-10 have one AP242 file each. So the pairs are **revision pairs**
(AP203 against AP242 editions of one part, plus AP203 geometry against AP203 with PMI), and the
invariance check of §7.3 is "the same part across revisions" only. 74 §7.3 also says "the same part in
two units"; that is not established. A scan for an INCH conversion unit anywhere in each file found
inch in some files of a part and not in others (ctc03, ctc05, ftc06, ftc08, ftc09), but it cannot say
which files declare the geometry's length unit, and the file that differs in unit also differs in
revision, so these are revision-and-unit pairs at best. No pair is known to differ in unit only.
CO1 must read the unit with `scx-p21`'s units (SP1), list the pairs from that, and claim a unit
pair only where the revision is the same; until then no unit pair is claimed.

### 14.6 Checked

`cargo xtask fast`, clippy and fmt: see the hand-back. A second `fetch` fetches nothing; `verify`
matched all 86 files. Fetch of the whole corpus takes a few minutes, dominated by the 14 MB NIST
archive.

### 14.7 Proposed 04 row

| 11-27 | (For the owner's review) The STEP corpus is fetched by `corpus/manifest.tsv` (`cargo xtask corpus fetch`, SHA-256 and size checked, `SCX_CORPUS_CACHE`) from four sources whose licences were read at their own pages: NIST MBE PMI (public domain, "used without any restrictions"), step-conformance's generated parts (CC0), 7 fixed MFCAD files (MIT) and 4 pin fixtures (MIT OR Apache-2.0). Dropped: the shop parts (origin repository gone), `assembly_extract` and the Discord fixture (provenance), cadmpeg and STEP Tools (no licence, all rights reserved). `corpus/references.tsv` holds only volumes derived by closed form from the truth files (19), not the pin's SolidCraft `expected.json` values. NIST pairs are revision pairs; no unit-only pair is claimed | 74 §7 listed the sources by claim; reading them cut two and added one flag (`tetra_disk`) | research 74 §14 |

## 15. Review response (11-27)

An adversarial review of the first version (§0 to §12) raised ten findings (F1 to F10) and gave a
verdict on each of the nine open questions. Each finding was checked against the code on main and
against 04. This section is numbered 15 because §13 and §14 are appended by other tasks; nothing
is renumbered. "Changed" names the sections that now carry the fix.

### 15.1 Findings

- **F1, healing in the wrong crate. Agree.** 04 row 102 makes `scx-import-sc` the converter for
  saved solidcore designs, not a STEP stage. `finish` refuses `EdgeEndOffVertex`,
  `UseEndOffVertex` and `PcurveOffEdge`, so a real file whose gaps exceed its stated uncertainty
  cannot become a `Brep` unless the reader raises first. `Report::raised` already exists in
  scx-brep. Changed: R3 gains the importer's exception (09 §6); §6.3 measures enclosed gaps
  before `finish` and records a `Raise` per entity; `close_gaps` goes through `scx-stitch` (SP4);
  Boolean-level repair is `scx-repair`'s; SP7 is dropped; Q2 rewritten. The "step does not heal"
  contradiction with SP4 is gone: step records raises, and stitch closes gaps only on request.
- **F2, layering. Agree on all six points.**
  - stitch → check: removed. Stitch runs no structure pass; it relies on `finish`, and the
    result's check is the caller's (§4.3).
  - stitch → mass: ruled in as a new edge (§11 row 2). The volume sign is MA4's, and copying it
    into bodyq would be a second integrator. Under layers.txt, mass (layer 4) sits below stitch.
  - `grow_accuracy` in scx-brep: dropped with CF5. Raises are the importer's, from its own gap
    measurements, so no layer has to see `Findings`.
  - `Made<Brep>`: stitch returns `Done<StitchReport>` with lineage per input sheet, and step
    returns `Imported`. `Made` stays scx-make's (R1).
  - layers.txt is stale: verified (make sits before tess, bodyq, check and boolean). LY1 moves it
    last and moves the dev-dependencies of bodyq and check on make to testkit.
  - CF3's edges: check → ssi and check → bodyq are now listed in the table's Edges column, with
    every other new edge.
- **F3, merging points. Agree.** Rows 108 and 186 are the rule, and `scx_boolean::boolean`
  already implements them (`VerticesWithinCoincidence`, `Chain`). Changed: R7 and §4.3 step 1.
  Two vertices of one sheet within coincidence: `Refused`. A pair from different sheets:
  identified. A chain: anchored to the nearest vertex of the operands, else `Unsupported`.
  Candidates come from a grid or box index sized by the coincidence radius, never by visit order.
  The clearance finding `VerticesWithinCoincidence` is in §3.3.
- **F4, capabilities with no home. Agree.** §1.3 is the capability ledger: every pin capability
  the review lists, and the others found while checking, has a row (BW8 to BW11, ST5, ST6, HL5,
  SP1, SP3, SP8) or a named home. The review asked for §1.4; it is §1.3, because the first
  version had no §1.3. One correction: the stored re-identification (`stored_face`,
  `stored_edge`) goes in bodyq's `still_names`, which `scx-record` calls. Its matching is geometry
  over a body, and record sits below bodyq in the layers.
- **F5, HLR. Agree on the hole, the step-5 waits and the second evaluator.**
  - Proof: HL2 now needs every image-space crossing (PL6), certified complete, then one
    classification per span.
  - PL6 is a step-6 row for lines and conics. Torus and spline spans wait on step 5's general
    bounded 2-D intersections in scx-planar, and HL2 says so.
  - The step-5 waits are marked in §8.1: ST3's nesting, HL2's NURBS spans, BW5's NURBS faces.
  - One correction: orientation by parity (R4) needs no ray, so it does not wait; only nesting
    does.
  - HL1 is built on scx's evaluators, not a port of (A)'s geometry layer. Quadric recognition is
    SU7 in scx-surface, shared by HL1 and SP2b.
- **F6, task table. Agree, with one dependency changed.**
  - SP2 is defined on analytic surfaces and analytic edge curves. It needs SM1b for seam and
    pole cases, and SP5 waits on the step-5 fitter.
  - CF1 shrinks to what `finish` and `contract` lack, and `contract` calls it.
  - CF2 splits into CF2a (gaps), CF2b (clearance) and CF2c (radial order), with CF3 for analytic
    crossings and CF4 for NURBS.
  - HL6 is the drawing "done when".
  - LY1, ST5, ST6 and the ledger rows are added.
  - **Dispute:** the review gates the NURBS half on SM9. It waits on SM5 to SM8 instead. The check
    needs certified gaps and crossing curves on NURBS pairs, which the SSI rows give. It never
    calls the Boolean plug-in that SM9 wires.
- **F7, corpus and scoring. Agree, except the revision-pair remedy.**
  - Unit pairs: they do not exist, and "unit pairs" is dropped.
  - references.tsv: its provenance is restated (§6.4), and the six kernel-cases truths are
    excluded until CO2 clears their licence.
  - cadmpeg and `assembly_extract.stp`: excluded.
  - MFCAD: a fixed, hashed sample is the gate (CO4).
  - Pending kinds: a finite list in 04 (§11 row 5).
  - The categories of §7.3 cover refused files and failed finishes, and findings are scored when
    no recorded raise covers them.
  - The scorer lives in `crates/scx-bench` (CO1).
  - **Dispute: revision pairs.** The review proposes pre-qualifying pairs whose volumes agree
    within a tolerance and then scoring only those. That tests nothing. Pairs that disagree are
    removed, and pairs that agree are counted as passing, by the same measure. The revisions are
    different models, so agreement is no truth either way. Revision pairs are therefore a smoke
    column, reported but not scored (§7.3).
  - **Note:** the gate sample is "up to 1,000". The review's "about 15k" models is from research
    69 and is not verified here, so CO4 verifies the count first.
- **F8, perf traps. Agree.**
  - `Scope::Import` is the import default, and `Full` runs on request until CO3 measures it.
  - HL2 batches spans per face over a box tree, and CO3 has an HLR row.
  - Each Hausdorff test in ST2 has a budget share per edge, and an `Undecided` result is
    counted.
  - Vertex candidates come from a spatial index.
  - Tangent pairs go to the CK3/BO7 contact classifier first (CF3).
  - §2.3 states that a memo hit charges the work it records (04 row 104).
  - **Dispute, in part: the argument convention.** The review asks for `&mut Cx` everywhere. R8
    keeps BQ1's ruled `(&mut Budget, &Stop)` for leaf queries that keep no memo, and uses
    `&mut Cx` for queries that write `Scratch` (`tight_box`, `contact`, `pick`, the check). A
    leaf query does not need the whole `Cx`, and widening it would undo a ruling for no gain. The
    rule is one convention with one test: a query that writes the memo takes `&mut Cx`.
- **F9, rulings. Agree with every objection, with one wording note.**
  - R1: lineage-carrying returns, and the trims sentence reworded to row 130 (the writer fits,
    `finish` verifies).
  - R3: the importer's exception.
  - R4: the edge and both limits. Sheets use parity only, and nesting on NURBS waits on step 5.
  - R5: the clearance finding, with typed causes.
  - R6: memo hits.
  - R7: rows 108 and 186.
  - CK3's radial order is certified along the whole edge by CF2c, and `contract` uses it. Until
    CF2c lands, the one-sample test is a named limit that both share.
  - The claim that the pin's pick "depends on order" is withdrawn (§2.2): the first-miss stop is
    a batch convention.
- **F10, citations. Agree; all fixed.**
  - `is_inner_shell_of` is at shell.rs:1656.
  - The underflow is at topology/brep/p3.rs:5360.
  - `glue_edges_at_vertex` is at p2.rs:1018.
  - The line counts without tests are noted in §0.

### 15.2 Open questions

- **Q1. Agree:** CF1 shrinks.
- **Q2. Ruled otherwise; applied** (F1).
- **Q3. Agree:** each caller of (B) has a row.
- **Q4. Agree:** BW7 is outside "done when", with closed-form truths.
- **Q5. Agree:** the raise belongs to scx-step. #18848 becomes a test only after a licence check,
  and is otherwise rebuilt synthetically.
- **Q6. Ruled otherwise; applied:** the MFCAD gate. The size is "up to 1,000" (F7).
- **Q7. Partly disputed.** Closed forms, validation properties, the check and the contract are the
  truth, and the pin is a smoke signal. Revision pairs are smoke too, not pre-qualified and
  scored (F7).
- **Q8. Agree:** `Scope::Import` is the default.
- **Q9. Agree:** the budget is recorded per row, and `NeedsSsi` is a finite list in 04.

### 15.3 Disputes for the coordinator's ruling

1. CF4 waits on SM5 to SM8, not SM9 (F6).
2. Revision pairs are a smoke column, not pre-qualified and scored (F7, Q7).
3. Leaf queries keep `(&mut Budget, &Stop)`. Queries that keep a memo take `&mut Cx` (F8, R8).
4. Stored re-identification goes in bodyq's `still_names`, and scx-record calls it (F4).

## 18. CF1 as built (11-27)

Task CF1 (§8): the structure pass `contract` and `check` share, and the `Findings`, `Finding` and
`Scope` types. Branch `cf1`. Crate `scx-check`; no new Cargo edge.

### 18.1 What `finish` and `contract` establish, and what CF1 adds

Read from `crates/scx-brep/src/finish.rs` (and `check/`), and `crates/scx-check/src/lib.rs`.

| Question | Settled by | CF1 |
|---|---|---|
| References resolve, each fin has its use and each use its fin, loops are connected, no face uses an edge twice in one sense | `finish` (`StructureCause`) | nothing |
| Sides of each pair of radially adjacent fins agree; shells connected; regions have shells; outside not solid | `finish` | nothing |
| One vertex on two loops of one face | `finish` (`VertexOnTwoLoops`) | nothing |
| Collapsed edge has one vertex; a point use only on a collapsed edge | `finish` | nothing |
| Ends, uses and pcurves within their own samples | `finish` (`GeometryCause`) | nothing; the certified gaps are CF2a's |
| At least one face; every face bounds a solid and a non-solid; two fins per curve edge; no lone vertex; regions proved; outward | `contract` tests 1 to 5 | nothing |
| **The faces round a vertex are one piece** | nobody (72 §14.22 left it open; two solids touching at one vertex passed the contract) | **added** |
| **Solid, sheet or mixed** | nobody names it: `finish` accepts all three, `contract` refuses all but the solid | **added** as a fact (`Class`), and as `NotSolid` where a face has material on both sides |

Nothing is duplicated: `finish`'s `VertexOnTwoLoops` is about the loops of one face and says
nothing across faces or shells, and the contract's edge test sees a pinch only where an edge is
shared. The pass reads records only; it needs no geometry, no budget and no `Scratch`.

### 18.2 The pass

- **Vertex manifoldness** is connectedness of the vertex's link. Nodes are the ends of edges at
  the vertex (edge id, and which end: 0 start, 1 end, so a closed edge has two nodes). Arcs are
  face corners: a use ending at the vertex joined to the next use of its loop that is not a
  collapsed point (collapsed uses sit at one vertex, so skipping them gives the corner round an
  apex or pole). Wire edges ending there are nodes with no arc. A vertex with two or more
  connected components is `Pinch { vertex, pieces }`. One walk over the uses; a union-find per
  vertex over its sorted nodes; deterministic (id order throughout).
- **It does not duplicate the edge test.** A radial edge of four fins is one node with four arcs,
  so blocks touching along an edge are one piece at the edge's ends: `Contacts::Radial` is not
  affected, and the pass applies in both modes. An edge of the wrong fin count is the edge test's
  breach, found first.
- **Classification.** Each face is bounding (solid one side), sheet (non-solid both sides) or
  inside (solid both sides). `Class` is `Empty`, `Solid`, `Sheet` or `Mixed` (more than one
  kind). `NotSolid { shell }` is each shell that bounds a solid region and has an inside face.
  A sheet is valid input (an importer's open shell), so `Class::Sheet` is a fact on `Findings`,
  not a finding.

### 18.3 Public API (all in `scx-check`)

```rust
pub fn check(body: &Brep, cx: &mut Cx<'_>, scope: Scope) -> Result<Findings, CheckError>;
#[non_exhaustive] pub enum Scope { Import /* default */, Stitched, Full }
#[non_exhaustive] pub struct Findings { pub list: Vec<Finding>, pub undecided: Vec<Undecided>,
                                        pub class: Class, pub spent: Spent }
#[non_exhaustive] pub enum Finding { VertexNotManifold { vertex, pieces }, NotSolid { shell } }
#[non_exhaustive] pub enum Undecided {}            // no cases yet
#[non_exhaustive] pub struct Spent { pub work: u64 }
#[non_exhaustive] pub enum Class { Empty, Solid, Sheet, Mixed }
#[non_exhaustive] pub enum CheckError { Exhausted { limit }, Cancelled }   // kind() + name()
pub struct Pinch { pub vertex: VertexId, pub pieces: usize }              // the pass's answer
// contract: Breach::VertexNotManifold { vertex, pieces } (name "vertex_not_manifold")
```

`contract` calls the pass after the edge and lone-vertex tests (so existing first-failure order is
unchanged), but only under the new `Contacts::Manifold`, which refuses a pinched vertex with
`Breach::VertexNotManifold` (see 18.4, the deviation). `Refused` and `Radial` admit it. `check` charges one
unit per edge use to a fresh budget of the settings and polls `cx.stop` once up front (the pass
is a short walk); it takes `&mut Cx` for the signature of 74 R8, though this part does not touch
`Scratch`. All three scopes run the same part so far. The list is in kind then id order.

### 18.4 Choices, with cases for and against

- **Findings carry only the variants CF1 produces.** For: the 74 §3.3 variants with
  `Bracket<Length>`, `Entity` payloads and `Degeneracy` need SU6's bracket type, which does not
  exist; inventing one here would fix it before SU6. `Finding` is `#[non_exhaustive]`, so CF2a to
  CF3 add theirs. Against: callers cannot match the full list yet. They cannot match a list that
  does not exist either.
- **`Undecided` is an uninhabited non-exhaustive enum.** For: the field exists with its final
  type, and the structure part is decided by the records. Against: an empty enum is odd; a
  placeholder variant would be a lie.
- **`class` is a field of `Findings`, not a finding.** For: sheets are valid, and stitch and the
  importer both ask the question. Against: 74 §3.3 lists `NotSolid` as a variant; it is kept, for
  the real defect (material on both sides of a face), not for every sheet.
- **`VertexNotManifold` carries `pieces`** (74 §3.3 has only the vertex). For: a pinch of two and
  a vertex in three pieces read differently; free from the pass. Against: one more field to keep
  stable.
- **A wire edge is a piece of its own.** For: a wire touching a sheet or solid at a vertex is not
  a surface there. Against: a wire network is only wires (the vertex has no corner and is not
  examined), and the contract refuses wires earlier.
- **Deviation: `contract` refuses a pinched vertex only under `Contacts::Manifold`, not by
  default.** The first build refused it in every mode, and two `scx-boolean` tests failed
  (`blocks_sharing_a_corner_unite_into_two_regions_and_two_outer_shells`,
  `a_union_of_crossing_creases_names_only_live_vertices`): 72 §12.4's BO7 builds a union whose
  parts touch at a vertex, and its tests prove it with `contract(.., Refused)`. For the
  default: BO7 is ruled and its output must be a contract-valid body; `check` reports the pinch
  either way. Against: `Refused` then refuses an edge pinch and admits a vertex pinch, which
  reads oddly; the variant's doc says so. The alternative (refusing by default and changing
  BO7's tests to a new mode) reverses a ruling, so it is left to the coordinator.
- **The pass is infallible and uncharged inside the contract**, like the contract's other
  structure tests (they charge nothing). `check` charges its own use count.

### 18.5 Tests (`crates/scx-check/tests/structure.rs`, 6; testkit bodies only)

Truths are constructions, never the pin's output.

- `blocks_meeting_at_a_vertex_pinch_it`: the new testkit body `bodies::corner_touch(seed, one)`
  (two blocks sharing one lattice vertex and no edge; separate regions, or one region of one
  shell), seeds 0 to 3. Exactly one finding, `VertexNotManifold` with 2 pieces, at the vertex on
  six edges; class Solid; `contract` passes it under `Refused` and `Radial` (as before CF1, BO7)
  and returns `Breach::VertexNotManifold` under `Contacts::Manifold`.
- `bodies_that_are_manifold_at_every_vertex_have_no_finding`: every generator kind, seeds 0 to 2,
  sizes 1 to 3: none has a pinch (cone apexes, sphere poles, seam vertices, the torus, holes,
  cavities, the edge-touching checkerboard).
- `the_class_is_what_the_regions_say`: solids are Solid with no finding; both sheets are Sheet;
  stacks of 2 to 4 blocks are Mixed with `NotSolid` on exactly the block shells.
- `a_repeated_check_leaves_the_body_as_it_was` (the pin's test of that name, ported as
  construction): debug form of the body before and after is equal, and two runs give equal
  findings.
- `every_scope_runs_the_structure_part`, `a_budget_or_a_stop_ends_it_typed` (work is 24 n^2 for a
  block; `limit = work - 1` is `Exhausted`, a set stop is `Cancelled`).

The 19 `contract` tests still pass unchanged. Testkit: `lattice_blocks` factors `checker` (same
dice draws for it: its seeds build the same bodies; the testkit's own tests pass) and
`corner_touch` uses it. No test was removed.

Not tested: a pinched sheet (two sheets sharing one vertex). It needs a sheet recipe the testkit
lacks; the pass handles it by the same link argument (two fans). A follow-up can add it with
CF6's one-defect recipes.

### 18.6 Measurement

Throwaway timer, release, mean of 50, `check` at `Scope::Import`: block 6 faces 1.5 us; block of 384 faces 93 us; torus 16 faces 2.3 us; cavity 192 faces 39 us (about 0.24 us a face). Against `contract`'s 1.4 ms for a block, the pass is about 0.1%, and it runs in `contract` only under `Contacts::Manifold`.

### 18.7 Proposed 04 row

CF1: `scx_check::check(&Brep, &mut Cx, Scope) -> Result<Findings, CheckError>` exists with the
structure part only. The pass shared with `contract` adds vertex manifoldness (a vertex's link,
nodes the ends of edges there and arcs the face corners, is connected; `Pinch`) and the
solid/sheet/mixed `Class`; it duplicates nothing of `finish` or the contract's other tests, and
`contract` refuses a pinched vertex (`Breach::VertexNotManifold`) under the new `Contacts::Manifold`; the default modes admit it, because BO7 builds such unions. Later
tasks add `Finding` variants (non-exhaustive) and `Undecided` cases. `Scope` is `Import`
(default), `Stitched` or `Full`; all run the structure part.
## 19. SP1 as built (11-27)

The `scx-step` crate on `scx-p21`: the skeleton of the importer (74 §8, row SP1). It reads
units, uncertainty and the entity plan of each solid and sheet from an `Exchange`, and builds no
geometry. 17 tests on synthetic AP203, AP214 and AP242 files (`crates/scx-step/tests/synthetic.rs`).

### 19.1 Public API

```rust
pub fn read(ex: &scx_p21::Exchange, opts: &ReadOptions) -> Result<Imported, StepError>;
pub struct ReadOptions { pub assume_units: Option<Units>, pub close_gaps: bool } // non_exhaustive
pub struct Imported { pub bodies: Vec<ImportedBody>, pub products: ProductStructure,
                      pub skipped: Vec<Skipped>, pub report: ImportReport }
pub struct ImportedBody { entity, name, kind: SolidOrSheet, representation,
                          context: Resolved, shells: Vec<ShellPlan> }
pub struct ShellPlan { entity, role: ShellRole, closed, reversed, faces: Vec<FacePlan> }
pub struct FacePlan { entity, surface, surface_kind: String, same_sense, bounds: usize }
pub struct Resolved { units: Units, assumed: MissingUnits, uncertainty: Uncertainty,
                      several_uncertainties: bool }
pub enum Uncertainty { Absent, Stated(f64 /* mm */) }
pub struct ImportReport { declared, warnings: Vec<ImportWarning>,
                          assumed: Vec<AssumedUnits>, raised: Vec<Raise> }
pub enum StepError { NoUnits, ConflictingUnits, BadAssumedUnits, Structure(Box<P21Error>),
                     Pending(Pending) }   // kind() and name() as in 62
```

The 04 row's field is `ImportReport::raised: Vec<Raise>`, with `Raise { entity (STEP id), gap:
GapKind, before, after }` (millimetres). SP1 defines it and leaves it empty; SP2 fills it. The
read never heals: nothing in this crate changes a number the file wrote.

### 19.2 Choices, with cases for and against

- **Edges: `scx-p21`, plus `scx-num`.** `scx-p21` is the only edge the row names. `scx-num` is
  added because `StepError::kind()` returns `scx_num::error::ErrorKind` (the five kinds of 62),
  as `P21Error` does; it is the lowest layer and p21 already depends on it. For: one error
  convention across the crates. Against: one edge beyond the row. `scx-brep` is *not* added, so
  `Imported` here has no `Brep` and the report is not `scx_brep::Report`: SP2 adds `body: Brep`
  and `findings` to `ImportedBody`, and maps `ImportReport::raised` onto `Report::raised` if
  a caller wants the brep form (the STEP entity id is the useful key before there is an entity
  in a `Brep`). For: the edge list stays as ruled. Against: SP2 reshapes `Imported` slightly,
  and the report is a second type where 74 §6.3 shows `Report<ImportNotes>`.
- **Missing units are refused per file, naming the first representation.** Units belong to a
  representation context, so each representation that lists a body is resolved; the first
  failure (id order) refuses the file. Against refusing the file: one bad part blocks an
  assembly. For: the ruling (the 04 STEP-import row) says refuse, a half-read assembly with a part at the wrong
  scale is the failure the rule exists to prevent, and the caller can assume units.
- **A missing plane angle unit is missing too.** The ruling says units, not only length.
  Against: angle is rarely used by solids and such files exist. For: a cone or a torus read at
  the wrong angle unit is silently wrong; `ReadOptions::assuming_units` covers it in one line.
  Reversible: if the corpus shows it often, the ruling can relax angle alone.
- **The assumption fills only what the file lacks** and is recorded per representation
  (`ImportReport::assumed`, which of the two parts, `Resolved::assumed`). The file's own unit
  always wins. Against: a caller who wants to override wrong units cannot. For: that is a
  different request (a rescale), not a read option.
- **Two different units of one kind in one context are refused (`ConflictingUnits`)**; the same
  unit listed twice is not. p21's `context()` takes the last, silently. For: the file does not
  say which numbers mean what. Against: valid input refused; but a context with two lengths is
  not a valid context (ISO 10303-42 gives one).
- **Uncertainty is the context's own, in millimetres; absent stays `Absent`.** It is never the
  last measure in the file, never clamped (the pin clamps to 1e-7..1 and defaults 1e-3). SP2
  resolves `Absent` to the document's resolution, which needs a `Cx`. Several uncertainties in
  one context: the largest, flagged (`SeveralUncertainties`). Against: the finest would be
  stricter. For: the file claims the coarsest accuracy it states (a smaller one on a different
  entity does not make the geometry better), and a too-small value would make `finish` refuse
  the file, which the raise (SP2) then repairs; the largest avoids a raise made only because of
  the choice. The accuracy-ratio warning (CTC-05, 2.05e-2) needs the document's resolution
  and is SP2's.
- **`close_gaps: true` is `StepError::Pending(CloseGaps)`, not ignored.** An option that
  silently does nothing is a failing default. SP4 removes the variant's use.
- **A body that cannot be read is `Skipped`, not an error**: `SkipReason::BodyKind` (faceted
  brep, with the entity type named) and `Malformed(P21Error)` (the first fault, naming its
  instance, such as an open shell in a solid, a face with no surface, a missing entity). One
  body's fault does not refuse the file (74 §6.3 and the skip list of 74 §11 row 5). Faceted breps are named,
  not read.
- **Shell nesting is one level.** An `ORIENTED_*_SHELL` wraps a plain shell only, so a cycle of
  them cannot recurse (tested). Product structure errors are `StepError::Structure`: the file's
  assembly cannot be trusted.
- **A body in several representations is read once**, from the first (id order). Its units are
  that representation's. A body shared by representations with different units is an assembly
  case SP8 decides.

### 19.3 What the tests prove

Units: mm, metre, inch (conversion-based, 25.4) and degree (the file's own number, not a computed
pi); the three protocols and an unknown schema's warning; missing units (no context, an empty
context, a length-only context) refused with the typed missing parts; assumption fills only
what is missing and is recorded, the file's units win; bad assumed units; conflicting units;
per-representation units and uncertainty (no leak between contexts); uncertainty without a unit
is in the context's length unit; several uncertainties. Skeleton: a cube, a solid with a void
(reversed oriented shell), an open sheet; faceted, open-shell-in-solid, missing-entity and
no-surface bodies skipped with typed reasons while the good body is read; an oriented-shell
cycle; a broken product structure; determinism (two reads equal). No test was removed.

### 19.4 Not done, for later rows

Geometry and topology (SP2); raises (SP2, field defined here); the document-resolution default
and accuracy-ratio warning (SP2); product tree of placed instances (SP8, `Imported::products` is
p21's `ProductStructure` for now); quadric recognition (SP2b); the writer (SP3); gap closing
(SP4).

### 19.5 Proposed 04 row

**scx-step skeleton (SP1).** `scx_step::read(&Exchange, &ReadOptions) -> Result<Imported,
StepError>`. Units are the representation context's; a missing length or plane angle unit, or two
different ones of a kind, is refused unless the caller assumes units, and an assumption is
recorded in `ImportReport::assumed`. Uncertainty is the context's own (`Uncertainty::Absent` when
it names none; SP2 resolves it to the document's resolution). Bodies that cannot be read are
`Skipped` with a typed reason. `ImportReport::raised` is where SP2 records enclosed-gap raises;
`scx-step` never heals, and `close_gaps` is `Pending` until SP4. Edges: `scx-step` → `scx-p21`
and `scx-num`.

## 16. LY1 as built (11-27)

`layers.txt` now lists `scx-make` last, after `scx-boolean`, with a comment where `scx-stitch`,
`scx-hlr` (after `scx-check`) and `scx-step` go when added. `cargo xtask layers` enforces the
order by position (a library may depend, dev-dependencies included, only on libraries above it),
so no xtask change was needed; it passed after the move.

- **Scope wider than the row.** Four crates dev-depended on `scx-make`, not two: `scx-bodyq`,
  `scx-check`, `scx-tess` and `scx-boolean` (tess and boolean sit below make too, so the check
  would fail on them). All four are moved. `scx-difftest` keeps its edge (difftest may use any
  library).
- **Where the bodies come from.** `scx_testkit::bodies::made` (feature `bodies`) holds a copy of
  `scx-make`'s constructors (block, cylinder, cone, sphere, torus, extrude, with `Made`, the
  face records, `MakeError` and the shared writer), with the same API and the same bodies, so
  every call site changed only its `use` path. The testkit gains an optional `scx-planar`
  dependency for extrude, under `bodies` (the same pattern as `scx-brep`, which dev-depends on
  the testkit and is a dependency of it). `tess` and `boolean` now enable the testkit's `bodies`
  feature.
- **For and against the copy.** For: no test changes, meaning kept exactly; step-6 crates take
  bodies from testkit. Against: two copies of ~1,400 lines that can drift. Decided: the copy is
  the fixed test builder and `scx-make` is free to change (loft, revolve, ...); the testkit's
  copy is not to be extended with new constructors, and a body a test needs that make cannot
  build goes into the testkit as its own generator. Trimming the copy's error type to what tests
  use is a later cleanup.
- **Tests.** None changed, none removed; only import paths, and prose in three comments naming
  the constructor. `scx-make`'s own tests still run on the real constructors.

## 21. CO4 as built (11-27)

Trust: the model count, the entity scan, the sizes and the timings are MEASURED on 11-27 by
running code over the pinned files; the reading of the file-name structure is INFERRED (the truth
files that would confirm it were never fetched). Proposed for review.

### 21.1 The count and what MFCAD contains

- **Count.** The git tree of `github.com/hducg/MFCAD` at `ef6d58a` (GitHub's tree API, not
  truncated) lists 15,488 `.step` files and 15,488 `.face_truth` files, plus one `readme`.
  74's "UNVERIFIED" is now verified: 15,488 (1,158,435,999 bytes; 22,436 to 253,345 each, median
  72,751). No two files are byte-equal.
- **Scan.** All 15,488 files were downloaded under a scratch directory (never executed, never
  opened by anything but a byte-pattern count) and every entity name was counted. Every file is
  one `MANIFOLD_SOLID_BREP` with one `CLOSED_SHELL`, schema `AUTOMOTIVE_DESIGN { 1 0 10303 214 1
  1 1 1 }`, one `LENGTH_UNIT`; every surface is a `PLANE`, every curve a `LINE` (with
  `EDGE_CURVE`, `PCURVE`, `SURFACE_CURVE`). **There is no cylinder, cone, sphere, torus, circle,
  ellipse, B-spline, revolution or extrusion in any file.** The "holes", "pockets" and "slots" of
  the MFCAD classes are all polygonal. Face counts run 8 to 59 (median 22).
- **Consequence.** The gate is a planar, linear gate: it exercises `scx-step`'s plane and line
  reading, pcurves on planes, loop and shell building, the check and the contract. It says
  nothing about curved analytic faces; those are covered by the NIST and conformance files
  (74 §14). The earlier phrase "analytic gate" holds only for planes. Where a wording in 74
  or 04 says MFCAD tests "analytic faces and edges", read "planar faces, line edges".
- **File names.** `a-b-c-d-e-23.step`: the tokens before the last are a sorted multiset of
  feature-class ids 0 to 14 (the multiset counts match exactly: 11,628 = C(19,5) files with five
  features, 3,060 = C(18,4) with four, 680 with three, 120 with two); the last token is a
  suffix (23 or 19). Each class appears in 4,844 names. This is an inference from the names
  and counts, used only to stratify; no score depends on it.

### 21.2 The sample: rule, size, and why

**Rule (fixed, no choices in it).** Cell each of the 15,488 files by face count (six bins:
8 to 16, 17 to 20, 21 to 24, 25 to 29, 30 to 36, 37 to 59) and by feature count (2, 3, 4 or 5,
from the name). In each of the 20 non-empty cells take the first 30 files in order of SHA-256 of
the file name (all of them where the cell has fewer). Add four forced extremes: the file with the
most faces (`2-3-3-3-3-23`, 59), the largest file (`3-3-3-3-3-23`, 253,345 bytes), the fewest
faces (`0-0-19`, 8; 20 files tie, the first by name) and the smallest file (`6-7-7-9-19`,
22,436 bytes; already in). The result is **480 files, 37,027,352 bytes**, 8 to 59 faces, 182
with five features. The seven files CO2 took (every 2,500th) were chosen by a rule that ignores
stratification; none of them is in this sample, so they are removed from the manifest and
replaced, not kept as extras.

**Measured coverage of the sample.** Every feature class appears in at least 79 files; all 105
class pairs occur together in at least one file; face counts span the whole range. (The whole
set's classes are equal in number, so this is about as even as five-feature multisets allow.)

**Why 480 and not 1,000 (74 says up to 1,000).**

- For: coverage saturates early. At 30 per cell all 15 classes and all 105 pairs are already
  present; 10 per cell (177 files) already reaches 105 pairs. The set is one generator with
  one entity vocabulary (planes and lines), so more files add repeats, not kinds. A defect
  that hits 1% of the files is caught with probability 0.99 at 480 files (1 - 0.99^480 = 99.2%),
  0.5% with 91%.
- Against: a rarer defect (0.2%, say one odd feature combination) is missed 38% of the time at
  480 and 14% at 1,000; the gate is the only real-file check of the planar path before
  merge. Mitigation: the rule has one constant (30 per cell); 60 gives 925 files, 70 MB, and
  is one regenerated block of manifest rows. The full 15,488 set is a nightly or manual run, not
  the gate.
- Time cost, measured: the first `corpus fetch mfcad` took **79 s** for 481 files (one `curl`
  per file, hash and size checked; about 160 ms each, so 1,000 files would take about 165 s),
  once per cache. A second fetch or `verify` takes **2.6 s** (debug `xtask`, SHA-256 of 37 MB).
  Reading: `std::fs::read_to_string` plus `scx_p21::parse` over all 480 files takes **0.32 s**
  in a release build (37 MB), so the syntax layer is not the gate's cost; the import and check
  time is CO1's to measure once `scx-step` exists, and if it is small the size can go to 60
  per cell.

### 21.3 Built

- `corpus/manifest.tsv`: the 7 seed rows removed, 480 rows added (`mfcad/<name>.step`, URL
  pinned to `ef6d58a`, SHA-256 and size from the downloaded bytes, licence MIT, part `file`);
  the `mfcad/LICENSE` row stays. 560 lines in all. The existing `corpus fetch|verify mfcad`
  selects them; nothing in `xtask` or `scx-testkit` changed (their manifest tests pass).
- Never fetched: any `.face_truth` file. Downloads were kept under a scratch directory and
  deleted when done; nothing downloaded was run.
- No scoring code (CO1). The generator was a throwaway script over the tree listing and the
  scan; the rule above is the record, and the hashes in the manifest are what is checked.
- Not done: a smaller "fast" tier inside the sample (the manifest selects by name prefix, and
  the names carry no tier). If CO1's import time is high, shrink the constant instead.

### 21.4 Proposed 04 row

| 11-27 | (For the owner's review) The MFCAD gate sample is fixed in `corpus/manifest.tsv`: 480 of MFCAD's 15,488 models at `ef6d58a` (the count is verified), chosen by a stated rule (30 per face-count-by-feature-count cell, in order of the hash of the name, plus four extremes), 37 MB, fetched in 80 s once and verified in 3 s. Every one of the 15,488 models is planes and lines only, so the gate is a planar gate; curved analytic reading is gated by NIST and the conformance parts. `.face_truth` pickles are never fetched | The "up to 1,000" ruling left the size open; coverage (all 15 classes, all 105 pairs) saturates well below it, and the scan showed nothing non-planar to stratify on | research 74 §21 |

## 22. BW2 as built (11-27)

`scx_bodyq::tight_box(&Brep, Entity, &mut Cx) -> Result<Extent3d, BodyqError>` (R8: it writes
`Scratch`, so it takes `&mut Cx`), `tight_box_charged(.., &mut Scratch, &mut Budget, &Stop)` for a
caller that boxes many entities under one budget, and `tight_body_box(&Brep, &mut Cx)` (every face,
wire edge and lone vertex; a wire body has no face). All in `crates/scx-bodyq/src/tight.rs`.

### 22.1 What a tight box is

A certified box of the finished entity (never of its untrimmed surface), grown as
`Face::bounding_box` is (accuracy plus the zone, outward) and met with that box, so it is never
looser than the loose one. Each of the six sides is found as the largest value of one coordinate
over the entity. That maximum is on the boundary or at a critical point in the interior, so there
are two searches, both outward-rounded interval searches (no sampling):

- **Edge** (1-D): cells of the interval are dropped when their enclosure is at or below the best
  end, or when the derivative's enclosure excludes 0 (a monotone cell peaks at its end, held by the
  interval's end or the neighbour). A cell within the tolerance is taken whole, else halved. The
  enclosure is the natural range met with the mean-value form about the middle (second order where
  the slope is small), and for a NURBS curve met with the hull of the control values of the spans the
  cell is in (positive weights make each point a convex combination): exact for a coordinate that is
  constant, where both interval forms are only first order. Across a NURBS knot the derivative's
  enclosure holds both sides, so a kink (issue_1054's peaks) stays a candidate until pinned.
- **Face** (2-D): the same over the uv hull with `Surface::along` (new in scx-surface: the existing
  mean-value `range_along` plus the enclosures of the two partials). A cell with a partial that
  excludes 0 has no critical point; a cell `Face::side_of_cell` (new in scx-brep) proves outside the
  face holds no point of it; the rest is halved on the partial that moves the range most. The
  boundary term is the edges' raw boxes, so a trim that is itself the maximum (a cylinder cut at
  z = 0.3) leaves no cell. A side where the edges already reach the loose box's side (within the
  tolerance) skips the search.
- **Tolerance**: a thousandth of the zone, never below 9.1e-13 of the entity's largest coordinate.
  The result exceeds the true box by about that plus the growth.
- **Shell and region**: the hull of their faces, wire edges and lone vertices.
- **Not a default**: a failing edge or face box is `Defect(NoEdgeBox | NoFaceBox | NoUvBox |
  Surface)`; an entity not in the body is `Refused(NoEntity)`; an empty body `Refused(EmptyBody)`.

### 22.2 Memo (04 row 104)

Edge and face answers are kept in the caller's `Scratch` as `(input, tolerance, raw box, work)`,
work in cells enclosed. The entry holds the full input (curve and interval; surface, and for every use
its sense, pcurve and edge input, and the growth), compared with `PartialEq`, so a hash collision only
replaces an entry, and the key follows bits, not ids (the same face of an equal body hits). A hit
charges the recorded work at once; a face's work includes its edges' (hit or filled), so warm and cold
spend the same and a limit stops both at the same place (tested: `Exhausted{limit}` both ways, equal
`used`, bit-equal boxes). Only finished answers are kept. This does not use `scx_record`'s writer
(as `scx-mass` does): that would add a bodyq to record edge, and BW8's `still_names` may need record
to call bodyq the other way round; `PartialEq` on the inputs is enough.

### 22.3 Cases for and against

- Interval B&B over sampling: certified, matches the Boolean's needs, and the pin's WR-651 bug
  (cells over off-dyadic knots staying undecided, boxing to the surface top) cannot occur, because a
  cell is not accepted on a bound it does not hold and a NURBS cell is cut at its knots inside
  `along`. Against: cost grows with the surface's rational weights (see 22.5).
- Edges first, interior second: a trim that is the maximum costs only its edges. Against: a face of
  many edges pays the edge search per distinct edge, which the memo shares across faces.
- `Surface::along` and `Face::side_of_cell` as small additive public functions in the lower crates
  rather than a private copy in bodyq: the partials and the proof that a cell misses the boundary
  tree are what the lower crates already compute. Against: two files outside bodyq change (both
  additive; `range_along` is now `along(..).range`, bit-equal).

### 22.4 Tests (`crates/scx-bodyq/tests/tight_box.rs`, 13)

Truth is closed form or dense sampling, never the pin's output.

- Bodies against closed forms: cylinder, cone, sphere, torus.
- Rectangular patches (cylinder walls, sphere lunes, torus patches; 3 kinds x 3 seeds x 3 sizes)
  against 401 x 401 samples of the surface over the pcurves' corner rectangle: holds every sample,
  slack within the sampling error and the growth.
- A tilted sphere cap (3 placements) against `R cos(max(0, gamma - alpha))`: 1e-5 past the truth (the
  growth), the loose box about 4x the volume.
- issue_1054 (8 tests ported as 4 test functions, each with its cases): wire edge on half the
  quadratic arc stops at the 45 degree point and the whole arc is the control; degree-one peaks:
  an interior kink kept, the knots outside dropped, knots on the interval's ends kept; the seam arc
  stops at its pole; the arc to height 25.
- issue_1799: the 270-degree rational cylinder trimmed to z <= 0.3 (the box is exact: -1..1, 0..0.3);
  #13753's ridge (the line of x = 10 inside a quarter rational cylinder); the ramp trimmed across
  its spans at 16 and 128 spans (#14460).
- Memo: warm equals cold in bits and work; a limit stops both alike; a fresh scratch gives the same
  work; an equal body hits and another seed fills; shells and regions are the hull; absent entities
  and the empty body are refused.

**Not ported, with what each proved:** the barrel cut by a Boolean difference (WR-588 at operation
level: the face's maximum is inside the face while the surface peaks outside it; needs the Boolean,
which sits above bodyq; the cap and ramp cases prove the same principle here); the bicubic sheet
trimmed to a disk (#14452, needs the curve-on-surface projection from step 5 to make the trim edge);
the chamfer face with a dip (#13753, its data file is the pin's; needs the sheet-on-surface builder);
the closed spline round cases (#18809, #21660, projected trims, step 5). The pin's work counters
(control points read) are not ported: the cost is measured below instead.

### 22.5 Measurements (release, one thread, local box under load; method: throwaway timers)

| Case | Cells (work) | Cold | Hit | Tight / loose volume |
|---|---:|---:|---:|---:|
| Cylinder, 6 faces | 144 | 0.5 ms | 17 us | 1.00 |
| Sphere, 4 lunes | 1,504 | 27 ms | 11 us | 0.83 |
| Torus, 16 patches | 384 | 3.1 ms | 44 us | 1.00 |
| Cone, 5 faces | 96 | 0.6 ms | 10 us | 1.00 |
| Tilted cap (R 2, 20 deg) | 228 | 1.8 ms | 2 us | 0.24 |
| Trimmed rational cylinder | 950 | 14 ms | 5 us | 0.24 |
| Ramp, 16 spans | 422 | 5.8 ms | 4 us | 0.89 |
| Ramp, 128 spans | 2,186 | 39 ms | 7 us | 0.89 |

The loose box is a filled cell (about 100 to 400 ns once read), so tight_box costs 10^3 to 10^5 times
the loose box cold, and is memoized per face: use it where a bound is needed (check, contact, pick,
Boolean gates), not per query. Slack over the truth (tilted cap, trimmed cylinder): 1.0e-5, the zone
the box is grown by, i.e. the search adds under 1e-9. Without the control-hull meet the 128-span ramp
cost 8.4e6 cells (12 s): a rational curve with a constant coordinate is first order in every interval
form. A flat-but-not-constant rational coordinate still costs about `sqrt(tol / curvature)` cells.

### 22.6 Deviations and open questions

- Deviation from 74 §2.3: the return is `Extent3d` (scx-num's box), not a new `Aabb`.
- New Cargo edge: dev-dependency scx-bodyq -> scx-nurbs-surface (tests build NURBS surfaces); no
  normal-dependency edge. layers.txt unchanged (nurbs-surface is below bodyq).
- New error variants `BodyqRefusal::{NoEntity, EmptyBody}` and `BodyqDefect::NoEdgeBox`.
- Open: (1) `Face::bounding_box` returns `None` for faces whose rational patches the loose enclosure
  cannot prove positive (a 128-span ramp curve's loose edge box is `None`); tight_box does not need
  it and meets with it only where it exists, but the check and the Boolean's gates that read the loose
  box meet that `None`. (2) A budget share per entity for whole-body callers (R6) is the caller's:
  `tight_box_charged`. (3) Procedural surfaces (step 5) need their own `along`.

## 23. P21U as built (11-27)

`scx_p21::units::context` no longer reads a missing unit as millimetres and radians.

### 23.1 The shape, chosen

```rust
pub fn context(ex: &Exchange, id: u64) -> Context;
pub struct Context { pub length: Unit, pub angle: Unit, pub distances: Vec<(f64, Option<f64>)> }
pub enum Unit { Missing, Named(f64), Conflicting }
impl Context {
    pub fn units(&self, assume: Option<Units>) -> Result<(Units, MissingUnits), UnitsFault>;
    pub fn uncertainty(&self, length: f64) -> Uncertainty { largest: Option<f64>, several: bool }
}
pub enum UnitsFault { Missing(MissingUnits), Conflicting(MissingUnits) }  // moved from scx-step
```

Options considered:

- **`Option<f64>` per unit.** For: smallest. Against: two different units of a kind
  (which scx-step refuses, and the old p21 resolved by "later wins") have no place, so scx-step
  could not drop its reader; its `ConflictingUnits` tests would lose their meaning.
- **`Result<Context, UnitsFault>`.** For: one decision point. Against: all or nothing. The
  product reader must go on reading a file whose context lacks a unit (it only needs the length
  of frames; scx-step wants the structure before it assumes anything), and the uncertainty is
  lost with the units. A caller that wants the `Result` has it: `Context::units`.
- **A per-unit enum (built).** The three states are what a file can say, the caller decides what
  each means, and `Context::units(assume)` is the one place that turns them into numbers, with
  the assumption the caller states. `Default` for `Context` is "names nothing", not mm.

Adversarial pass on the built shape:

- *Does the private default move up a level?* `Units::default()` (mm, rad) still exists. It
  is no longer reachable from `context`; the callers that use it write it (difftest, as the
  pin's default; a test). `Context::units` with `None` never produces it.
- *The uncertainty of a context with no length unit.* A measure that names no unit is read in
  "the context's length unit". With the length missing that is the caller's assumed one, so
  `uncertainty(length)` takes it as an argument (scx-step passes the resolved length) and no
  default is hidden in the context.
- *Several uncertainties.* The old p21 kept the last, scx-step the largest (74 §19). p21 now
  keeps the largest and says `several`; this changes `context`'s result on files that state
  more than one (equal ones count once). Both choices are the file's accuracy claim read as the
  coarsest; the pin's last-wins is a defect class in difftest.
- *Products.* A placement or transformation operator has lengths and no size without a length
  unit. `product::read` now refuses it with `EntityCause::Units(UnitsFault)` (names
  `no_length_unit`, `conflicting_length_units`) instead of reading it in mm. A representation
  with no frames reads as before. `product::read_assuming(ex, Some(units))` supplies the length
  unit the caller assumes (the file's wins); scx-step passes its `assume_units`, so an assumed
  inch now scales mapped-item frames too (before, they were silently mm). Frames read no angle,
  so the angle factor they get is NaN.
- *Left in place.* `unit_factor` reads an SI unit whose base it does not know
  (`SI_UNIT(.MILLI.,.FURLONG.)`) as factor 1: the same kind of guess, one level down, and
  compared against the pin by difftest. It is not changed here; it needs its own typed
  result (a follow-up: `unit_factor` returns `None` for it, as it does for an unreadable
  conversion).

### 23.2 scx-step

`crates/scx-step/src/units.rs` lost its context reader (`listed`, `name`, `finish`, the unit and
uncertainty loops: about 120 lines). `resolve` is `context` plus `Context::units` plus
`Context::uncertainty`; `MissingUnits` and `UnitsFault` moved to scx-p21 and are re-exported
(`scx_step::units::{MissingUnits, UnitsFault}`, `scx_step::MissingUnits` unchanged). The 17
synthetic tests are unchanged and pass.

### 23.3 Tests

scx-p21 units: the "missing or broken units read as mm and rad" test is now "are missing, not
mm and rad" (the same inputs, each asserted `Unit::Missing` and `units(None)` a typed fault; the
unknown-base SI unit moved out of that list to a case that asserts it is `Named(1.0)`, the
residue above); new `two_different_units_conflict`; the uncertainty tests are the same cases
through `uncertainty(length)`; "the last one listed wins" became "the largest wins and `several`
is set, equal ones are one"; "a bad one after a good one clears it" became "a bad one beside a
good one is left out". scx-p21 product: `a_frame_in_a_context_without_a_length_unit` (refused
with `Units(Missing)`; read in an assumed unit); the cycle refusals now give their context a
length unit (they asked about a cycle, not units). Removed: none.

### 23.4 difftest

`t_p21` maps the pin's default at the comparison: `pin_unit` reads a `Missing` unit as the pin's
mm / radians and counts it as the pin's defect (`corpus_pin_units` line), a `Conflicting` one has
no pin value (the pin keeps the last) and is counted and not compared, several uncertainties
likewise; a product scx refuses for want of a length unit where the pin reads in mm is counted,
not a mismatch. The Part 42 comparison reads both in the same explicit units. Result:
`cargo test -p scx-difftest --test diff t_p21`: 8 passed; `corpus_contexts` 13 of 13 exact, `corpus_products` 5 of 5 exact, `corpus_part42` 1954 of 1954 exact, 0 mismatches, and `corpus_pin_units` 0 of every class: the pin's fixtures (the only corpus when `SCX_P21_CORPUS` is unset) all name both units, so the mapping is built and compiles but is not exercised by a real file; a corpus with unitless files will count them.

### 23.5 Proposed 04 row

| 11-27 | (For the owner's review) `scx_p21::units::context` returns a `Context` with `length` and `angle` each `Unit::{Missing, Named(f), Conflicting}` and the file's distance measures; nothing defaults. `Context::units(assume)` turns it into `Units` or a typed `UnitsFault`, and `Context::uncertainty(length)` is the largest stated (`several` when more). scx-step uses it and drops its own reader (17 tests unchanged); `product::read` refuses a frame in a context without one length unit (`EntityCause::Units`) and `read_assuming` takes the caller's assumed length. The pin's mm/rad default, last-wins conflict and last-wins uncertainty are counted as defect classes in `t_p21` | A per-unit enum is the only shape that keeps "missing" and "conflicting" apart and lets the product reader go on; `Result` would lose the uncertainty and the structure of files that lack a unit. `unit_factor`'s unknown-SI-base factor 1 is the same guess one level down and stays a follow-up | 74 §23 |

### 23.6 P21Ub (11-28)

The residue §23.1 left: `scx_p21::units::unit_factor` read a length or plane angle `SI_UNIT` whose
base it does not know (`SI_UNIT(.MILLI.,.FURLONG.)`) as factor 1. It now returns `None`, as it
does for an unreadable conversion. `context` already treats `None` as "not usable", so such a unit
is left out and the kind reads `Unit::Missing` (then `UnitsFault::Missing`, or the caller's
assumption); scx-step, `Context::units` and `uncertainty` needed no change and gained no default.
A measure whose unit is `None` is read in the context's length unit, as before for unreadable units.
Units of kind `Other` (steradian, mass) keep factor 1: nothing reads their factor, and refusing
them would make every real file's solid angle unit "unreadable". A typed cause was not added: the
module has no per-unit cause today, and `Missing` is what the context already says.

Tests (`scx-p21` units): an unknown-base length and plane angle unit is `None` and the context
names the kind `Missing` (in `missing_or_broken_units_are_missing_not_mm_and_rad`, replacing the assertion that it was
`Named(1.0)`); known bases (MILLI METRE, RADIAN) read exactly and STERADIAN stays `Other, 1.0`; an
uncertainty in an unknown-base unit is read in the context's length unit.

difftest: `t_p21` maps the unit-level difference (scx `None`, pin factor 1 for a non-`Other` SI
unit) as equal and counts it as a new defect class, `unknown_base`, appended to the
`corpus_pin_units` line. The context level needed no mapping (it already counts a `Missing` unit
as the pin's default, which is 1 for both mm and rad). The pin corpus has no such unit, so the
count is 0 as the others are.

## 20. SU7 as built (11-27)

`scx_surface::recognize` and `scx_surface::recognize_written`, in `crates/scx-surface/src/recognize.rs`
(module docs hold the method and every bound). No Cargo edges. `nurbs_invert`'s `up` and `halved`
became `pub(crate)` to share the Bezier split and the upward rounding; nothing else outside the new
module changed.

**Shape.**

- `recognize(&Surface, over: &Extent2d, Accuracy, &mut Budget, &Stop) -> Result<Option<Recognized>,
  SurfaceError>`, with `Recognized { surface, deviation: Length }` (`#[non_exhaustive]`).
  - A cone over `over` is the plane square to its axis at the middle height when the height
    half-range is within the accuracy, else the coaxial cylinder of the middle radius when the
    radius half-range is, else itself. Closed form, deviation the half-range plus rounding.
  - A NURBS surface is proposed each kind from points, normals and principal curvatures at a 5 x 5
    grid (plane, sphere, cylinder, cone, torus, simplest first), checked at a 9 x 9 grid with
    edges, refined by Levenberg-Marquardt where that grid or the certificate does not pass, and
    accepted only on a certificate: over each Bezier patch, the kind's implicit form in Bernstein
    form with running error bounds, halved until every piece's bound is within the accuracy. A
    sample beyond the accuracy refutes (R2: a sample may refute, never prove).
  - Other analytic kinds come back as they are, deviation 0.
- `recognize_written(&Written, Accuracy) -> Result<Recognized, SurfaceError>` takes the sizes a file
  writes where the kind refuses them: `Written::Cone` with half-angle 0 is the cylinder; a
  `Written::Torus` of major radius 0 is the sphere of its minor radius (deviation 0), one with
  `0 < major <= minor` is that sphere within `major` when `major` is within the accuracy, else
  `Torus::new`'s `SelfIntersecting`.

**What changed from the §8 row's signature, and why.**

- The extent, budget and stop are arguments. Analytic domains are unbounded, so "a cone of near-zero
  angle is a cylinder" holds only over a face's extent (the pin's flat cone uses the face's `v`
  range too). The certificate is a leaf query (R8), and running out is `Exhausted` (R6), not `None`.
- It returns `Result<Option<..>>`: `None` is "not proved", the errors are the extent's (`restrict`'s
  `EmptyExtent`/`OutsideDomain`), `Exhausted`, `Stopped`, and `Defect` for a Bernstein form that
  cannot be built.
- The written-sizes half is a separate function, because a cone of angle 0 and a spindle torus
  cannot be a `Surface` to pass in (their constructors refuse them).
- The deviation is one-sided: the largest distance from the given surface's points over the extent
  to the recognised surface. That is what a face's edges and the importer need (the face's points
  move by at most that). The reverse direction (does the recognised surface reach beyond the given
  one) is the extent's business, not the surface's.
- No `UvMap` is returned: the recognised surface has its own parameters; callers that keep pcurves
  re-project (SP2b, HL1 use points and normals only).

**Decisions, with cases against.**

- *Certificate by implicit forms, not by inverting samples.* For: proves every point, cost a few
  hundred Bernstein products per patch, deterministic. Against: needs a factor bound per kind
  (`Q = delta * (rho + r)` and the torus's two-factor product), each argued in the module docs; the
  factor's reach `delta_max` is set at most twice the accuracy's radial reach so the loss is about
  `acc / r`.
- *A piece with a corner within 1/64 of the accuracy is not proved.* The bound tends to the piece's
  largest distance as it is halved, but only quadratically, so a surface whose deviation is just
  under the accuracy would halve to float resolution. This makes the effective accuracy at most
  1/64 tighter. Against: a real quadric written at exactly the accuracy edge returns `None`. For:
  the importer's case is an exact rational form (deviation near rounding) or a fit far inside.
- *Least squares, not minimax.* The refined fit minimises the squared distances at the 9 x 9 grid.
  A non-rational approximation is proved at about 0.65 to 1.1 times its first fit's sampled
  distance (cylinder 0.75x, sphere 0.75x, torus 1.5x in the test sweep), not at the minimax
  distance. A minimax step (IRLS) would close the gap; not needed for exact forms.
- *Orientation from the middle point.* The recognised surface's normal is made to agree with the
  given one's at the middle of the extent (reversed by `ParamAxis::U`, or the plane's frame turned
  over). A given surface whose normal flips inside the extent is not a quadric within the accuracy
  anyway except at a crease, which the certificate does not see. Open question 2 below.
- *Prefilter at 64 accuracies (or 1e-6 of the size).* A generic surface is rejected on the 25 jets
  and the five closed-form fits, before any refinement or certificate.

**Tests** (`crates/scx-surface/tests/recognize.rs`, 11 tests, no pin output used):

- kernel case 4005 rebuilt: cone r 5, angle 8e-12, height 10 is the cylinder of radius `5 + 5k`,
  deviation in `[5k, 4.1e-11]`;
- kernel case 4006 rebuilt: cone r 4 at z 3, angle `pi/2 - 7e-12`, radii 4 to 10, is the plane at
  `z = 3` facing `-z` (the cone's normal side), deviation at most `2.2e-11`;
- cone thresholds: cylinder at `1 + 1e-6` of the radius half-range, the cone itself at `1 - 1e-6`;
  plane at `1 + 2e-4` of the height half-range, the cone at `1 - 2e-4`;
- written sizes: angle-0 cone, `R = 0` torus, spindle torus within and beyond the accuracy, a ring
  torus and an ordinary cone;
- the exact rational NURBS of each kind (plane, sphere, cylinder of reversed sense, cone, torus):
  the kind, sizes to 1e-9, twice to the bit;
- a cone of angle 1e-5 as NURBS: a cylinder at 1e-3, a cone at 1e-7;
- a rational cylinder with a control point moved 1e-9 (recognised at 1e-6) and 1e-3 (refuted);
- non-rational quarter-turn cubic approximations of a cylinder, a sphere zone and a torus: the kind
  at 0.01, none at 1e-5, and a sweep near the error;
- a generic bicubic: none at 1e-6, 1e-3, 1e-2;
- budget exhaustion, stop, and an extent outside the domain.

Every accepted case is checked against the closed-form distance at a 33 x 33 grid (the deviation
bounds the sampled distance from above) and for the normal's side. No tests removed.

**Measurements** (release build, a throwaway timer not committed, 2000 and 50 calls):

- generic bicubic reject: 9.5 us per call, budget 25 (the 25 jets);
- exact rational forms, accepted: plane 12 us, cylinder 17 us, cone 20 us, sphere 25 us, torus
  52 us; budget 107 to 112 units (81 dense points, plus one per certificate piece).

**Open questions.**

1. Should `recognize` also simplify other analytic kinds over an extent (a sphere of huge radius as
   a plane, a torus of tiny minor radius)? Not built: no caller needs it, and it would change
   `Surface` kinds the importer keeps.
2. Orientation is decided at one point. Good enough for faces; a caller that needs it proved
   everywhere would compare normals on the certificate's pieces.
3. A minimax refinement, if non-rational fits near the accuracy matter on the corpus (measure on
   the corpus once SP2b lands).

## 24. HL1 as built (11-27)

`crates/scx-hlr`, placed after `scx-stitch` in layers.txt. Cargo edges: hlr → bodyq, and
brep, curve, surface, num, which bodyq already sits on, so no new layer edge below bodyq. No ssi
edge (HL4) and no planar edge (HL2). Dev edges: testkit (`bodies`), nurbs, nurbs-surface, planar.
Module docs hold every formula: `silhouette.rs` (cylinder, cone and sphere; the trimming argument)
and `contour.rs` (the torus contour and its boundary crossings).

### 24.1 Public API

```rust
pub struct View;                                       // x, y, d: UnitVec3
impl View { pub fn new(direction: Vec3, up: Vec3) -> Result<View, HlrError>;
            pub fn screen(&self, p: Point3, origin: Point3) -> Point2; }
pub fn silhouettes(body: &Brep, view: &View, cx: &mut Cx<'_>) -> Result<Silhouettes, HlrError>;
pub fn prepare<'a>(bodies: &[(&'a Brep, Frame)], cx: &mut Cx<'_>) -> Result<Scene<'a>, HlrError>;
impl Scene<'_> { pub fn bodies(&self); pub fn bounds(&self) -> Option<Extent3d>;
                 pub fn centre(&self) -> Option<Point3>; pub fn scale(&self) -> f64;
                 pub fn silhouettes(&self, view: &View, cx: &mut Cx<'_>)
                     -> Result<Vec<Silhouettes>, HlrError>; }
pub struct Silhouettes { pub curves: Vec<Silhouette>, pub unsupported: Vec<Unsupported> }
pub struct Silhouette { pub face: FaceId, pub curve: SilhouetteCurve,
                        pub accuracy: Exactness, pub on_face: OnFace }
pub enum SilhouetteCurve { Line(Line), Circle(Circle), Torus(TorusContour) }   // non_exhaustive
pub enum Exactness { Exact, Recognized { deviation: Length } }
pub enum OnFace { Proved(Vec<Extent1d>), Undecided { over: Extent1d, why: Undecided } }
pub enum Undecided { NurbsEdge { edge }, TorusEdge { edge }, Classification }
pub struct Unsupported { pub face: FaceId, pub why: Why }  // Why { Freeform, Unbounded }
pub struct TorusContour;  // frame, major, minor, sight, branch; point(θ), normal(θ)
pub enum Branch { Plus, Minus }
pub enum HlrError { BadView(BadView), BadPlacement { body }, Exhausted, Stopped,
                    Surface { face, error }, Curve { face, error }, Bodyq, Defect { reason } }
pub const PARALLEL: f64 = 16.0 * f64::EPSILON;   pub const ROUNDING: f64 = 8.0 * f64::EPSILON;
```

`HlrError::kind()` and `name()` as every crate's. `cx.stop` is checked by every inversion and
recognition; the budget is charged one per face and one per classified span besides what
recognition and inversion charge; `cx.trace` gets `Phase("hlr.prepare")`, `Phase("hlr.silhouettes")`
and `Progress { done, total }` per face.

### 24.2 Choices, with cases for and against

- *What one face cannot answer is in the result, not an error.* 74 §5.3 has
  `Result<Vec<Silhouette>, HlrError>`. Here a freeform or unbounded face is `Unsupported` by name
  and a curve whose trim cannot be proved is `OnFace::Undecided` with the edge that blocks it; only
  a refused view or placement, the budget, the stop, a surface refusal and defects end the call.
  For: one NURBS face does not lose a drawing its other faces can make; HL2 needs exactly this
  split (74 §5.3's `uncertain` and `unsupported`). Against: callers must look at two places.
- *Placement is a rigid `Frame`, not `Affine`.* For: silhouettes of a scaled or sheared body are
  not the transformed silhouettes of the body (`n · d = 0` is not affine-invariant), and no part
  in a drawing is placed by anything but a rigid motion; the scene maps the view into each body's
  frame (exact up to rounding) instead of moving the body. Against: a mirrored instance needs a
  mirrored body. Open question 1.
- *One silhouette per face, not per surface.* Faces split from one periodic surface (the testkit's
  walls, lunes and patches) each return their own piece of the same curve, and a generator lying
  on an edge two faces share is held by both (test `a_generator_on_a_shared_edge_...`). For: each
  piece is proved on its own face, nothing global is needed; HL2 merges by curve identity when it
  builds image-space spans. Against: HL2 must dedupe.
- *Trimming by closed-form crossings, not samples.* The pin trims with 256 samples and bisection,
  so a face boundary that dips across the curve between two samples is missed. Here the curve can
  leave the face only where it crosses a boundary edge; each boundary edge is intersected with a
  plane that cuts the surface in the silhouette (`intersect_curve`, complete), or, on a torus,
  classified as a meridian or parallel whose crossings are `θ0` and `φd ± acos(q)` in closed form;
  one point per span is classified by `invert` plus `contains_uv`. For: proved, deterministic, no
  sample count. Against: an edge of another kind on a torus face, and any NURBS boundary edge
  (`intersect_curve` refuses NURBS curves today), give `Undecided` until step 5's general
  intersections.
- *The torus contour is exact, not a 720-point polyline.* `TorusContour` is the closed form
  `o + R ρ̂ + r σ(−da ρ̂ + k a)/s`, two branches parametrised by the meridian angle; viewed across or
  along the axis (exact zeros only) it is four circles or two equators, as `Circle`s. For: points,
  normals and crossings are exact; HL2's crossing search gets an analytic curve. Against: PL6's
  conic intersections do not cover it, so its HL2 spans wait on step 5 (as 74 §5.3 says).
- *Only NURBS faces are recognised.* Analytic faces use their own kinds as `Exact`; a NURBS face
  goes through `scx_surface::recognize` over its uv box (clipped to the surface's domain, since
  the box is padded by pcurve rounding) at the resolution's approximation accuracy, and the
  recorded deviation is carried (`Exactness::Recognized`) and widens every crossing tolerance by
  that much. A near-flat or near-straight analytic cone is not simplified: its closed form is
  already exact, and simplifying would replace a true curve by an approximate one (truth over pin).
- *The scene box is the union of tight body boxes* (`tight_body_box`, BW2), placed. The pin used
  vertex boxes, which for a sphere are its two poles; the test checks the sphere's full extent.
- *The tolerances decide only where to split, and are generous.* `near = max(zone, edge accuracy,
  ROUNDING · size) + deviation` decides whether a plane crossing is on the silhouette (the plane
  also cuts the surface in other curves, told apart by distance); a split too many changes
  nothing because both halves classify alike and merge. `PARALLEL` drops the generator of a
  cylinder or cone viewed within `16 ε` of its axis: the direction `a × d` is noise there and the
  lost line projects to below one rounding error of the height. Cuts within `ROUNDING` of each
  other (relative) are one crossing rounded twice: every ruling of a cone meets the splitting plane
  at the apex, which also ends the generator, and without this the sliver at the apex was kept as
  a piece on every face (found by the generated-cone test). The pin carries about 116 literal
  tolerances in its HLR.
- *A generator stops at the cone's apex.* The axial range is the face box's, clipped to the nappe
  where `r + k v >= 0` (the first build ran on through the apex; the made-cone test caught the
  piece past it).
- *A candidate wholly off the face is not listed.* `OnFace::Proved` is never empty.

### 24.3 Tests (`crates/scx-hlr/tests/silhouettes.rs`, 18; testkit bodies only, no pin output)

Every proved piece is checked at 8 interior points: on the face's surface (closed-form inversion)
and `|n · d|` within 1e-12 to 1e-14; on generated bodies each piece's midpoint inverts inside its
face's uv box.

- views: refusals (zero, NaN, infinite, parallel up) by name; the frame is right-handed into the
  screen and `screen` subtracts the origin;
- sphere, 8 views: one circle, radius and centre exact, frame z parallel to d, `[0, 2π]`;
- sphere lunes (seeds 1-4): the pieces sum to `2π` over the faces;
- cylinder, 8 views: two lines at `±r` across the view, pieces `[0, h]`, none along the axis;
- cone (frustum `[2,1]`, to the apex `[2,0]`, widening `[0.5,1.5]`): two generators exactly when
  `|k da| < |dρ|`, none when greater, ends on the rims (or the apex);
- cylinder and cone walls (seeds 1-4): each generator on exactly one face;
- a generator on a shared ruling (seed 0, view x): held by both faces, both `[0, 2]`;
- torus along its axis: circles of radius `R ± r` on a level frame, the general contour giving the
  same equators on a tilted one;
- torus across its axis: four circles, centres and radii exact;
- torus, general views: two branches, whole turns, the screen offset from the projected spine is
  `r` and normal to it, `normal(θ)` equals the surface's;
- torus patches (seeds 1-4): each branch's pieces sum to `2π`;
- 1e6 from the origin: sphere and cylinder closed forms hold;
- scene: a placed cylinder matches the one made in place (lines and pieces to 1e-12); the sphere's
  box is its full box;
- twice to the bit (Debug equality);
- stop gives `Cancelled`; budgets 0, 1 and 2 give `Exhausted`;
- NURBS: the exact rational 270-degree cylinder, trimmed, is `Recognized` (deviation <= 1e-9) with
  two generators at radius 1 and `Undecided::NurbsEdge` naming its rim; the same net without
  weights is `Unsupported(Freeform)`.

No tests removed (new crate). The pin's 88 HLR tests are HL3's.

### 24.4 Measurements (release, one thread, throwaway timer, 2000 calls over 8 views, warm)

| body | faces | per call | per curved face |
| --- | --- | --- | --- |
| sphere (made) | 1 | 1.1 us | 1.1 us |
| torus (made) | 1 | 1.3 us | 1.3 us |
| cone (made, frustum) | 3 | 10 us | 10 us |
| cylinder (made) | 3 | 14 us | 14 us |
| torus, 4 x 4 patches | 16 | 43 us | 2.7 us |
| sphere, 8 lunes | 8 | 49 us | 6.1 us |
| cylinder, 8 walls | 10 | 83 us | 10 us |
| rational NURBS cylinder (with recognition) | 1 | 29 us | 29 us |

Cost is the boundary edges' plane intersections and one inversion plus `contains_uv` per span;
the torus needs no intersections (closed-form crossings). Recognition is SU7's (17 us for the
cylinder). This is a new crate, so it adds nothing to existing paths; the pin's sampled trim does
256 inversions per curve and is not timed here.

### 24.5 Deviations from 74

- `silhouettes` returns `Silhouettes` (curves plus unsupported faces) and each curve carries
  `OnFace` (above); 74 §5.3 had `Vec<Silhouette>`.
- `prepare` takes `Frame`, not `Affine` (above).
- Silhouettes are 3-D curves with their face; projection to screen curves (`Curve2`) is left to
  HL2, which needs the 3-D curves for depth anyway. `View::screen` gives points.
- No `Options`/`hidden_lines` yet: HL2.

### 24.6 Waits on step 5 (named, typed)

- NURBS boundary edges: `OnFace::Undecided { why: NurbsEdge }` (needs `intersect_curve` for NURBS).
- Torus faces bounded by anything but meridians and parallels: `Undecided::TorusEdge`.
- Freeform NURBS faces: `Unsupported { why: Freeform }` (HL4, SM5).
- HL2's spans on `TorusContour` (general bounded 2-D intersections).

### 24.7 Open questions

1. Mirrored placements: accept a `Frame` with a reflection (and flip normals) or require a
   mirrored body? Nothing needs it yet.
2. Should a NURBS face recognised as a plane say so (today it returns no curves, like a plane)?
3. HL2's dedupe of a curve held by two faces: by curve identity (same surface) or by image-space
   overlap? The former is exact for split periodic faces; the latter also covers distinct but
   tangent faces.

### 24.8 Proposed 04 row

| 11-27 | Proposed (HL1), for review | `scx-hlr` is created with `silhouettes(&Brep, &View, &mut Cx) -> Result<Silhouettes, HlrError>` and `prepare(&[(&Brep, Frame)], cx) -> Scene`. Per face: exact closed-form silhouettes for cylinder, cone, sphere and torus (`TorusContour`, or circles on exact axis zeros); NURBS faces through `scx_surface::recognize` with the deviation carried. Trimming is proved: closed-form crossings of the boundary edges (plane cuts, or a torus's meridians and parallels) and one classified point per span, never samples. What a face cannot answer is named in the result (`Unsupported { Freeform, Unbounded }`, `OnFace::Undecided { NurbsEdge, TorusEdge, Classification }`); errors are only a refused view or placement, budget, stop, surface refusal and defects. Rigid `Frame` placement, not `Affine`. One silhouette per face; HL2 dedupes. Tolerances (`PARALLEL`, `ROUNDING`, per-face `near`) decide only splits | 74 §5.3 asked for exact quadric and torus silhouettes without sampled trims; the pin trims by 256 samples and draws the torus as 720 points. Against: callers read two places; HL2 must dedupe; NURBS rims wait on step 5 | 74 §24 |

## 27. SP8 as built (11-27)

`scx-step` gains the product structure: `Imported::products` is now a `ProductTree` (parts, the
assembly expanded into instances, every body with its placement), replacing the raw p21
`ProductStructure` SP1 passed through. No Cargo edge was added (`scx-num` was SP1's; `Affine`
and `Frame` come from it). 17 new tests in `crates/scx-step/tests/products.rs`, on synthetic
files; the 17 SP1 tests are unchanged.

### 27.1 Public API

```rust
pub struct ProductTree { pub parts: Vec<Part>, pub instances: Vec<Instance>,
                         pub placed: Vec<PlacedBody> }       // bodies_of(i), placements_of(body)
pub struct Part { definition: u64, id, name, description }
pub struct Instance { part: usize, by: Option<Use>, parent: Option<usize>, world: World }
pub struct Use { entity: u64, id, name, reference_designator }
pub struct PlacedBody { body: u64, kind: BodyKind, representation: u64, instance: usize,
                        mapped_by: Vec<u64>, world: World }
pub enum World { Placed(Affine), Unplaced(Unplaced) }
pub enum Unplaced { NoRelationship{occurrence}, NoTransformation{link}, NotAboutChild{link},
                    Several{occurrence}, Degenerate{entity}, Units{representation, fault} }
```

`Affine` is `scx_num::math::Affine` (column vectors, `parent * local`; millimetres). The tree is
built inside `read` (crate-private `products::tree`), so the units a body was read in are known.

### 27.2 What the pin did, and what replaced it

| Pin (`step_assembly.rs`) | Here |
|---|---|
| An occurrence with no relationship: identity | `Unplaced::NoRelationship`; the bodies below are listed with the same cause |
| A relationship without a transformation: identity | `Unplaced::NoTransformation` |
| Which side of a relationship is the child: "rep_1 unless only rep_2 is in the child's closure", so neither or both fall to rep_1 | exactly one side must be in the child's representations, else `NotAboutChild` |
| Several relationships on one occurrence: the first | equal maps are one placement, different ones `Several` |
| Degenerate or singular frame: a `String` error refuses the whole file | `Unplaced::Degenerate{entity}`, the file is read; `inverse` is `Affine::inverse` (regular proved, not a `det == 0.0` test) |
| Absent reference direction: x chosen as global X unless `|z.x| >= 0.9`, then Y | ISO 10303-42's `first_proj_axis`: +X made square to the axis, +Y where the axis is along X (corrected by SP2, §29.4: SP8 first wrote +Z, then +X, from memory) |
| Operator axes: unit-length as written, not made square | ISO `base_axis`: Z from axis3, X and Y made square to it, so a skew operator is not read as a shear (a mirrored one stays a mirror) |
| A body shared by representations with different length units: not said (SP1 left it to SP8) | scaled by the ratio of the lengths, relative to the representation it was read in; a representation with no usable length unit is `Unplaced::Units`, never mm |
| Plain relationships followed one hop from a mapped item, full closure from a definition | full closure in both; a relationship that is an occurrence's own placement is not a "same shape" link (the pin's closure took it for one) |
| A body reached through two related representations: listed twice | once per `(body, mapped_by)` in an instance |
| Errors: `String` | cycles, depth and instance count are p21's typed `P21Error` (`StepError::Structure`); nothing else refuses |

A root is the world frame by definition (identity). That is the one placement that is stated
rather than read.

### 27.3 Choices, with cases for and against

- **Unplaced is a value, not an error.** For: valid input is never refused (one bad frame in a
  large assembly must not lose the file), the caller sees every body and decides, and a failing
  default (identity) cannot hide in a result. Against: a caller who ignores `World` gets no
  warning; `Placed` must be matched. `World::placed()` returns an `Option` so that the match is
  one line.
- **Descendants of an unplaced instance carry the same cause** rather than a `Below` variant:
  the cause names the occurrence at fault, which is what a user needs. Against: the tree does not
  say which ancestor, but `parent` does.
- **ISO's default for an absent reference direction, not the pin's.** The pin chooses by a
  threshold; the standard gives a formula (as recalled: `first_proj_axis`, +Z unless the axis is
  +Z, then +X; the standard's text is not in the repo, and the case of an axis along -Z, where
  +Z is parallel to it and the standard's text has no x, takes +X as the same rule). The two
  agree for an axis along Z and differ for others (axis +X: ISO gives x = +Z, the pin +Y).
  **Corrected by SP2 (§29.4):** the standard's `first_proj_axis` takes `[1, 0, 0]`, and
  `[0, 1, 0]` when the axis's ratios are `[1, 0, 0]` or `[-1, 0, 0]`. That is the pin's rule
  without its 0.9 threshold. `products.rs` now uses the same `default_ref` as the geometry.
  Files that omit the direction on a tilted axis are written by tools that mean the standard's.
  Reversible: one function (`default_ref`). Needs a corpus file with an omitted direction on a
  tilted axis to confirm.
- **Scale ratio when the length unit differs.** For: the numbers of a body are in the unit of
  the representation that lists it, and a file that lists one body in two does mean it at both
  sizes. Against: a shared body read at the first representation's unit gets an instance at
  1/25.4 scale, which a rigid-only consumer must refuse; `Affine::axis_lengths` and `is_rigid`
  say so.
- **Dedupe within an instance by (body, mapped_by).** Two mapped items that place the same body
  in one part stay two (they differ in `mapped_by`); the same shape in two related
  representations is one.
- **Not checked: that the relationship's other side is in the parent.** The pin does not either;
  exporters link the child to the parent's root representation or to a sub-representation, and
  a check would refuse real files. A wrong side shows as `Several` or a placement the caller
  can compare with `parent`.

### 27.4 Tests

Ported from the pin's `step_assembly` tests (rebuilt as synthetic p21 text through `read`, with
units): the three writings of the transformation (conformant; `rep_1` the child with `item_1` in
the parent; `item_1` in the child) all put the child at +754 X; nested composition (a quarter
turn about Z then +10 X lands at (0,10,0), the other order would give (10,0,0)); a mapped item
(origin onto target, closed form (5,-1,0)); a cycle (now a `Structure` error from p21's read).
Not ported: the real HOOPS extract (`assembly_extract.stp`): its licence is unverified (74 §7).
New: instances, parts and occurrences (names, reference designator); a shared part placed once
per use; an occurrence with no placement and its grandchild (unplaced, bodies listed);
`NotAboutChild`; `NoTransformation`; `Several` and its agreeing twin; a degenerate frame; the
standard's default reference direction (axis +X and -Z); an operator scale of 2; a body in an
inch and a millimetre representation (1/25.4); a representation with no length unit
(`Unplaced::Units`, and with an assumed unit scaled by it); one shape in two related
representations; reading twice is equal. No test was removed.

### 27.5 Cost

The walk is linear in instances times the representations of a definition's closure, plus the
bodies listed; the dedupe is `n log n` in one instance's bodies. Measured with a throwaway
timer (release, an assembly of one root and n uses of one part with a body): the whole
`scx_step::read` (p21's product read, the skeleton and the tree) takes 0.27 ms for n = 100, 3.1 ms
for n = 1,000 and 13.9 ms for n = 5,000; p21's `product::read` alone takes 0.18, 1.8 and 6.6 ms.
So the tree and the skeleton together add about the cost of the p21 read again, and the
growth is linear. Before this row the tree was not built, so that is the added work; the added
work per instance is one `Affine` product and, per body, a clone of the (empty) `mapped_by`.

### 27.6 Not done


`Imported` still lists the bodies once (read from the first representation); SP2 builds each
`Brep` once and the caller instances it with `placed`. Colours and styles (`ProductStructure::
styles`) are no longer on `Imported`: nothing consumed them; they are one line to carry through
when a row needs them (SP3 writes them). Units of an instance's own occurrence frames are p21's.

### 27.7 Proposed 04 row

| 11-27 | (For the owner's review) `scx_step::Imported::products` is a `ProductTree`: the file's parts, its assembly expanded into instances, and every body with the map that places it in the world frame (`World::Placed(Affine)`, millimetres, `parent * local`). A placement the file does not give or gives unusably (no relationship, no transformation, a relationship not about the child, two different placements, a degenerate frame, a length unit missing) is `World::Unplaced(cause)`, never the identity, and the file is still read. Frames follow ISO 10303-42 (absent reference direction, operator axes made square); a body listed in a representation of another length unit is scaled by the ratio. Edges: none added | The pin's identity fallbacks, first-of-several, rep_1 guess and `String` refusals are failing defaults; a typed value keeps the assembly readable | 74 §27 |

## 17. SU6 as built (11-27)

`scx_surface::gap` (re-exported at the crate root), branch `su6`. No new Cargo edge; four kinds'
`axes()` became `pub(crate)`.

```rust
pub struct Bracket { lower, upper }            // lower() upper() width() exceeds(allowance) -> Option<bool>
pub enum Aim { Width(f64), Against(f64) }      // a bracket this narrow, or one that leaves the allowance
pub enum Short { Exhausted(Exhausted), Stopped, Precision }
pub enum Pending { NurbsSurface }
pub enum GapError { Undecided { reached: Bracket, why: Short }, Pending(Pending), NotFinite, BadAim { value } }

pub fn curve_surface_gap(curve: &Curve, over: &Extent1d, surface: &Surface, aim: Aim,
                         budget: &mut Budget, stop: &Stop) -> Result<Bracket, GapError>;
pub fn pcurve_edge_gap(surface: &Surface, pcurve: &Curve2, curve: &Curve, over: &Extent1d,
                       aim: Aim, budget: &mut Budget, stop: &Stop) -> Result<Bracket, GapError>;
pub fn vertex_curve_gap(point: Point3, curve: &Curve, t: f64) -> Result<Bracket, GapError>;
pub fn point_curve_gap(point: Point3, curve: &Curve, over: &Extent1d, aim: Aim,
                       budget: &mut Budget, stop: &Stop) -> Result<Bracket, GapError>;
pub fn curve_curve_gap(a: &Curve, a_over: &Extent1d, b: &Curve, b_over: &Extent1d, aim: Aim,
                       budget: &mut Budget, stop: &Stop) -> Result<Bracket, GapError>;
```

The module's doc holds the table from the pin's functions to these.

**One search for every gap.** Each gap is `sup_t d(t)` over a range. One branch and bound finds it.
Pieces sit on a max-heap keyed by an upper bound over the piece, ties to the earlier `t`, so the
result is deterministic. The lower bound is the largest certified `d` at a piece end. The range is
first cut at the curve's knots (and the kind's arc breaks, below), then into four pieces each.

A piece's upper bound is the smallest of:

- the chord bound on the vector `D(t)` whose length is the gap:
  `|D| <= max(|D(a)|, |D(b)|) + h^2/8 sup|D''|`;
- the same bound on `q = |D|^2`, square-rooted, whose `q'' = 2(|D'|^2 + D . D'')` stays small
  where `D` turns at a near-constant length;
- the direct interval enclosure.

Second derivatives come from a forward-differentiation ring over intervals (`Jet2`), evaluated
through the kinds' own `eval<T: Ring>`. The aim is met when the bracket is narrow enough, or when it
leaves the allowance. Otherwise the answer is `Undecided` with the bracket reached: the budget ran
out (R6), the caller stopped it, or the pieces can no longer be cut in floating point (`Precision`).
The budget unit is one point evaluated, the pin's `MAX_PIECES` and `MAX_PATCHES` made the caller's.
The nearest point of a curve to a point is the same engine run on `-d`.

**Choices, with cases for and against.**

- **The gap is to the kind's whole surface, not the face.**
  - For: scx-surface sits below the Brep and has no trim. The whole surface's distance is a closed
    form in the kind's frame (`z`, `|p| - r`, `rho - r`, the torus tube, the cone's generator
    `|perp|` or apex, the larger where the foot crosses the apex), so it is smooth and the brackets
    close.
  - For: the pin's `curve_face_gap_bound` also bounds against the surface's parameter box, not the
    trimmed face.
  - Against: an edge near the surface but off its face's trim reads as close. A caller needing the
    face's gap uses `pcurve_edge_gap`, an upper bound on it because the pcurve lies in the face.
    CF2a's finding needs a lower bound above the allowance, and the whole surface's lower bound is
    conservative for that.
- **Lines, circles and ellipses as interval series.**
  - For: the squares the closed forms need are taken on the series' coefficients, so a circle
    coaxial with a cylinder, or lying in its plane, is a constant to rounding. It closes in under 64
    units, where a jet bound would cut it to the width.
  - Against: this is a second path beside the jets. NURBS curves take the jet path (tested on a
    quadratic and a rational two-piece cubic).
- **The pcurve gap at the same parameter.**
  - For: a pcurve shares its edge's parameter, so `sup_t |S(p(t)) - C(t)|` is what same-parameter
    means, and it needs no nearest-point search.
  - Against: an edge whose pcurve is parametrised off its curve's parameter reads as a large gap.
    That is a defect the checker should report, so this is the intended reading.
  - The kinds' `u` is a rational arc parameter, exact only within one arc. A piece whose uv box
    straddles an arc break falls back to the direct `Surface::box_over` bound.
- **Curve to curve is one-sided Hausdorff**, `sup_t min_s |a(t) - b(s)|`. The symmetric distance is
  the larger of both calls; the stitcher's pairing (BW, ST2) wants the one-sided distance.
  - A piece's bound uses three correspondences and takes the smallest: `s` linear between the
    ends' nearest parameters (only where `b` has no knot between them), or `s` held at either end's.
  - The held correspondences are first order, but they survive feet that tie across a seam
    (`0` against `2 pi` on a full circle).
- **Vertex to edge has no budget.** It is one interval evaluation at the vertex's parameter.

**Not here (deviations from the row).**

- `seam_bound` is not ported. At the pin it encloses the largest normal angle along a seam
  (`seam_angle_*`, 1,412 lines), which is a continuity question, not a gap. No step 6 row consumes
  it through SU6; BW4's `tangent_all_along` is its likely user. A follow-up row SU6b is proposed.
- NURBS surfaces return `Pending(NurbsSurface)` from `curve_surface_gap` and `pcurve_edge_gap`. A
  point's distance to one is `Surface::invert`'s, but the bound over a piece (the chain rule
  through `partials_over_piece`, step 5's SM chain) is not built.

**Tests** (`crates/scx-surface/src/gap/tests.rs`, 21, all on a tilted frame away from the origin,
truths closed form, brackets at most 1e-9 wide unless stated):

- line above a plane;
- circle in its plane (0), and tilted (`R sin a`);
- circle coaxial with a cylinder (exact, under 64 units), and off its axis by 1e-3 and by 1e-7;
- a line tangent to a cylinder;
- sphere cases; a cone by its generator and by its apex; a torus; an ellipse against a plane;
- NURBS curves over a plane;
- a NURBS surface is pending;
- budget run out and stop are `Undecided` with brackets holding the truth;
- allowances, decided or undecided at the allowance itself;
- determinism;
- pcurves on a plane, on a cylinder across its arc breaks, and a latitude on a sphere (the fitted
  pcurve);
- vertex at an edge end; point against a curve;
- curves against curves: concentric circles (to 1e-6, below), coincident circles (0), parallel
  segments with and without overhang, an arc against its chord (the sagitta).

Nothing removed.

**Cost.** Side-channel timers, release build, a 24-core build host, 200 calls each, compared on
the pin's own test case: a line, and a quadratic bow, `6.8e-5` above a box's top face.

| call | per call | answer |
|---|---|---|
| pin `curve_face_gap_bound`, line, floor 1e-7 | 1,925 us | upper `6.80000008e-5` |
| pin `curve_face_gap_bound`, bow, floor 1e-7 | 2,041 us | upper `6.80000008e-5` |
| pin `curve_face_gap_bound_within`, bow, floor 1e-7 | 89,362 us | upper `6.8098e-5` |
| pin `curve_face_gap_bound_within`, bow, floor 1e-9 | 161,274 us | upper `6.8024e-5` |
| `curve_surface_gap`, line, any aim (5 units) | 4.5 us | `[6.79999999993e-5, 6.80000000028e-5]` |
| `curve_surface_gap`, NURBS bow, any aim (5 units) | 26.7 us | `[6.79999999992e-5, 6.80000000122e-5]` |

The scx calls are 70 to 6,000 times cheaper. They also return a lower bound, which the pin does not.
The two do not do the same work. The pin converts the face to a NURBS net and fits a correspondence
on every call; here the plane is a closed form. That makes this the cheap end, not a like-for-like
comparison on hard surfaces.

On curved kinds, at 1e-9 unless stated:

- coaxial and off-axis circles on a cylinder, sphere, cone and torus cases: 5 to 32 units each;
- pcurves: 5 units on a plane, about 8,350 on a cylinder across its breaks, 6,471 on a sphere
  latitude. The fitted pcurve's real deviation is about the width, so the search cuts finely there.
  That sphere case costs 293 units at 1e-5 and 2,917 at 1e-8;
- curve to curve: 94 to 1,011 units;
- concentric circles: 12,089 units (86 ms) at 1e-5 and 319,969 (1.3 s) at 1e-9, which is
  `width^(-1/3)` (Open 1). One unit is about 4 us in release (an inner nearest-point search of
  about 38 evaluations per outer point for curve to curve).

Nothing calls these on an existing path yet, so no existing path got slower.

**Open.**

1. Plateaus are the expensive case. Where the gap is the same all along (concentric circles, 0.5
   apart), every piece must be bounded below the largest point's lower bound, and the cost grows as
   `width^(-1/3)` (the piece bound's error is third order in `h`). Callers that ask `Against`
   an allowance pay far less than callers asking a width. The coincident case (gap 0) is cheap.
2. The face trim. Should scx-bodyq offer the face's own gap (the surface gap restricted to the
   trimmed region) on top of these, or do CF2a and BW3 manage with the whole surface's lower bound
   and the pcurve's upper bound?
3. Should SU6b (the seam angle) come before BW4, or should BW4 build on `tangent_edge`'s bounds as
   §2 says?

**Proposed 04 row.** Certified gaps live in `scx_surface::gap`: every gap is `sup_t d(t)` by one
deterministic branch and bound returning a `Bracket` or a typed `GapError`
(`Undecided { reached, why }` per R2/R6, `Pending`, `NotFinite`, `BadAim`); the leaf calls take
`(Aim, &mut Budget, &Stop)` with one budget unit per point evaluated; edge-to-surface is to the
kind's whole surface, the face's gap is bounded above by the pcurve at the same parameter;
curve-to-curve is one-sided; NURBS surfaces are pending until step 5's piece bounds.

## 30. SU8 as built (11-27)

`scx_surface::{axial_section, holds_axis, facing_trough, points_meet}`, branch `su8`, in
`axial.rs`, `trough.rs` and `meet.rs` (each module's doc holds the table from the pin's
function to this one). No new Cargo edge. SU6's `gap` gained one crate-private
`point_point_gap` (the interval distance of two points, exact zero for the same point); the
axial section uses `vertex_curve_gap` to certify each cut. The pin has no callers inside the
kernel (the app calls them), so the contract is from the doc comments and tests.

```rust
pub fn holds_axis(&Surface, &Plane, over: &Extent3d, tol: Resolution) -> Result<Holds, AxialError>;
pub enum Holds { Yes, Off { by: Length }, Across }
pub fn axial_section<E>(&Surface, &Plane, over: &Extent3d, tol: Resolution, cuts: &[Point3],
    on_face: impl FnMut(Point3) -> Result<bool, E>) -> Result<Vec<AxialPiece>, AxialError<E>>;
pub enum AxialError<E = Infallible> { NotRevolved, AxisOff{by}, AxisAcross, CutNotFinite{index},
    CutOffSection{index, gap: Bracket}, Defect{reason}, OnFace(E) }
pub fn facing_trough(&Cylinder, near: Point3, travel: Vec3, over: &Extent3d, res: Resolution)
    -> Result<Trough, TroughError>;        // TroughError { NotFinite, AlongAxis, OnEdge{offset} }
pub fn points_meet(a: Point3, a_acc: Accuracy, b: Point3, b_acc: Accuracy)
    -> Result<Meet, MeetError>;            // Meet { One{accuracy}, Apart, Undecided{distance} }
```

**Failing defaults removed.** `Revolved` bare numbers become the `Surface` kinds, so a bad
radius, a zero normal, a non-positive tolerance and a box of non-finite corners cannot be
built. `false`/`None` for "the plane misses the axis" becomes `Holds::Off{by}` /
`AxisOff{by}` (how far), `Across` for an axis square to the plane. `facing_trough`'s four
`None`s become three named causes.

**Choices, with cases for and against.**

- *A cut must lie on a meridian (new refusal).* The pin puts each cut on the nearest meridian
  whatever its distance. For: a cut far off the section is a caller bug that would silently
  split the wrong line; the check is one certified `vertex_curve_gap` per cut. Against: valid
  input could be refused if its cuts are computed less accurately than `tol.xsect()`. The
  caller passes a `Resolution` that is the coincidence of its entities (`coincident(..)` fed to
  `Resolution::new`), which widens the allowance.
- *`OnEdge` in length, not cosine.* The pin's `abs(cos) < 1e-9` between the pick and the
  travel; here the pick's offset from the plane through the axis square to the travel is
  within the resolution. For: the document's one resolution decides, and a pick on the axis
  is the same case. Against: at a radius of 10 the pin's threshold is 1e-8 mm and ours 1e-7
  by default, so picks between them now refuse.
- *Reach margin relative, not 1 mm.* `(far + radius) * (1 + 1/16)`. For: 1 mm is nothing on
  a metre part and huge on a 0.01 mm one. Against: results differ from the pin's.
- *`points_meet` is three-way.* The pin decides ties to the last bit (a 3-4-5 triangle with
  accuracies 2 and 3 meets); the enclosure of a distance is a few ulps wide, so the tie and
  one ulp either side are `Undecided` with the bracket. For: R2. Against: an exact tie is
  where the pin said "one". An exact decision needs `scx-num`'s expansions, which are
  crate-private. The joined accuracy is rounded up so it holds the other ball (the pin's
  rounds to nearest).
- *A sphere's circle starts on the plane's own x axis*, not an arbitrary orthogonal vector.

**Tests** (30, closed forms on a tilted frame away from the origin): `tests/axial_section.rs`
(17): holds/off/across, tilt by reach times sine, a plane or NURBS refused, cylinder strips,
one-sided face, missed face, cut merge within `tol`, cones both signs and from the apex,
sphere whole/arc/wrapping arc, torus circles and arc, cut off the section (certified gap),
cut not finite, `on_face` error, determinism. `tests/facing_trough.rs` (7): the pin's three
tests ported, half by outward normal over a 72-pick ring on an oblique travel, reach over
box corners, resolution edge, determinism. `tests/vertex_meet.rs` (6): the pin's two tests
ported (tie to `Undecided`), larger accuracy, joined ball holds the other at five scales,
non-finite, determinism. The pin has no test for `axial_section`. Nothing removed.

**Cost.** Side-channel timer, release build, a loaded build host, 200,000 calls, the pin against
this on the same case:

| call | pin | here |
|---|---:|---:|
| `axial_section`, cylinder, 4 cuts | 269 ns | 581 ns |
| `axial_section`, torus, whole circles | 184 ns | 216 ns |
| `holds_axis`, cylinder | 59 ns | 40 ns |
| `facing_trough` | 44 ns | 75 ns |
| `points_meet` | 0.3 ns (folded to a constant; not a real figure) | 38 ns |

The section is about 2 times slower where it certifies each cut (a `Curve` built and one
interval distance per cut, about 70 ns); `points_meet` pays for the interval distance. All are
under a microsecond and none is on an existing path, so no existing path got slower.

**Open.** (1) Exact ties in `points_meet` (expose an exact sum-of-squares sign from
`scx-num`?). (2) Callers' `Resolution` for the cut allowance (above).

**Proposed 04 row.** SU8: `holds_axis`/`axial_section` take a `Surface`, `Plane`, `Extent3d`
and `Resolution` and return typed results (`Holds`, `AxialError<E>`); cuts are certified
within `tol.xsect()` by `vertex_curve_gap`; `facing_trough` takes a `Cylinder` and decides
`OnEdge` by length against the resolution; `points_meet` returns `Meet::{One, Apart,
Undecided}` from the certified point distance.

## 26. ST1 as built (11-27)

New crate `scx-stitch` (layers.txt after `scx-boolean`; one Cargo edge, stitch → bodyq, plus
stitch → brep and → num, which bodyq already sits on). The first stage of §4.3: the sheets are
gathered into one body, and vertices of different sheets within coincidence are identified as
04 rows 108 and 186 rule. The edge stage (ST2) and orientation (ST3) are not built. A stitch that
needs either one returns a typed pending error, so no result leaves a join undone. Test bodies
come from `scx_testkit::bodies` (no `scx-make`). 16 tests: 11 in `crates/scx-stitch/tests/stitch.rs`
and 5 unit tests of the identification in `src/points.rs`.

### 26.1 Public API

```rust
pub fn stitch(sheets: &[&Brep], want: Want, cx: &mut Cx<'_>) -> Result<Stitched, StitchError>;
#[non_exhaustive] pub enum Want { Sheets, Solid }
pub enum NoItems {}                                   // a stitch skips nothing
pub struct Stitched { pub done: Done<NoItems>, pub report: StitchReport }
#[non_exhaustive] pub struct StitchReport { pub identified: Vec<Identified> }
#[non_exhaustive] pub struct Identified { vertex: VertexId, members: Vec<(Operand, VertexId)>,
                                          kept: (Operand, VertexId), spread: Length }
pub enum StitchError { Refused(StitchRefusal), Unsupported(StitchUnsupported),
                       Exhausted { limit }, Cancelled, Defect(StitchDefect) }   // kind(), name()
pub enum StitchRefusal { VerticesWithinCoincidence { operand, vertices: [VertexId; 2] },
                         OtherDocument { operand } }
pub enum StitchUnsupported { Chain { operand, vertices }, Edges { operand, edge }, Solid,
                             TooLarge, Query { operand, at: Entity, cause: BodyqUnsupported } }
pub enum StitchDefect { Draft, Query { operand, at }, Finish(Box<FinishCause>) }
```

Sheet `i` is operand `i` of `Changes`. Sheet 0 is `sheets[0].edit()`, so its entities keep their
ids. Every other sheet's regions, vertices, edges, shells and faces are copied in, in that order.
Each copied vertex, edge and face is `derive`d from its operand entity. An edge of three or more
fins gets its sheet's radial order back through `adjacency_mut`. Any finished body is taken,
solids included.

### 26.2 Identification (`points.rs`)

1. **Pairs.** One grid holds every sheet's vertices. Its cell is twice the largest reach (reach =
   the vertex's accuracy, at least the zone), and each vertex is tested against the 27 cells
   around it. A pair is within coincidence when its distance is at most the larger reach, which
   is `units::coincident`. This is O(V) on spread-out input, and the pairs are a sorted set.
2. **Clearance.** Two vertices of one sheet within coincidence give `Refused(VerticesWithinCoincidence)`.
   The error names the lowest such pair, the one row 186 rules per operand.
3. **Components** of "within coincidence" are formed by union-find over the pairs, rooted at the
   lowest index (sheet, then id), before any merge.
4. **One vertex per sheet.** In such a component, the kept record is the vertex with the smallest
   reach, then the lowest (x, y, z). It is an operand vertex's own record.
5. **Chains** (R7). Here one sheet has two or more vertices in the component. Each other vertex is
   anchored to its nearest vertex of that sheet (ties by id), and each anchor keeps its own point.
   The result is `Unsupported(Chain)` in three cases: a second sheet also has two vertices; a
   vertex lies beyond coincidence of its nearest anchor; or two vertices of one sheet land on
   one anchor.

### 26.3 Accuracy

Each identified vertex gets a `Coincident` warning. For each other member there is a
`coincident` lineage record onto sheet 0's vertex, or an `Origin::Operand` derive into a new
vertex. Row 186 says "every new vertex and edge end records its measured gap as `Measured`", so
the merged vertex's accuracy covers how far the merge moves things. It is set to the largest of
three distances from the kept point:

- the spread;
- each member's edge-curve ends at it (the interval min at `ends[0]`, max at `ends[1]`);
- each use's surface point there (pcurve ends, or `UseGeom::Point`).

When that is beyond the kept vertex's reach, its accuracy is raised to it as `Measured`, with a
`Raise` and an `AccuracyRaised` warning. Without this, `finish` refuses: `EdgeEndOffVertex`
allows only `re + rv`, and a curve end from another sheet can sit nearly `max(reach)` away.

### 26.4 The edge stage, pending

Until ST2, `stitch` returns `Unsupported(Edges { operand, edge })` before building in two cases:

- (a) Edges of two sheets have both ends in the same two identified vertices. These are edges to
  join.
- (b) A vertex of another sheet, not identified with one of the edge's ends, lies in the edge's
  box. This is an edge to split, or one passing within coincidence.

For (b), the loose `Edge::bounding_box` (grown by the edge's accuracy and the zone) is tested
first, then the certified `tight_box_charged`. This is the reason for the bodyq edge. Either box
is grown further by the vertex's reach beyond the zone. The candidates come from three
axis-sorted arrays, using the slab of the axis that holds the fewest. The check is conservative:
an edge that only passes near a vertex, inside its box, is refused too, until ST2 measures the
gap. Sheets that meet only at vertices, or not at all, stitch whole.

### 26.5 Choices, with cases for and against

| Choice | For | Against |
|---|---|---|
| `Result<Stitched, StitchError>` with `Stitched { done: Done<NoItems>, report }`, not the literal `Done<StitchReport>` | `Done<C>`'s parameter is the skip-cause type (`Skipped<C>` is for skip-mode ops). A stitch skips nothing, so `C` is uninhabited and the stitch's own report sits beside it | One more wrapper to unpack; ST2 may want `Done<C>` to carry a skipped edge, and that would change `NoItems` |
| `cx` last: `stitch(sheets, want, cx)` | Matches `boolean` and `tight_box_charged` | §4.3 wrote `cx` before `want` |
| Kept record by geometry (smallest reach, then lowest point), not "the lower sheet index's" (§4.3, row 186's "a's record kept") | §4.4 and ST4 want permuted input to give identical bits. Choosing by sheet index cannot give that; choosing by geometry does, and it is no less true (each is an operand vertex within coincidence of all members). The smallest reach is the best-known point | Departs from the words of row 186 and §4.3. With equal reach the lowest point is arbitrary, though deterministic. Ids still follow sheet 0, so "identical bits" holds for the geometry, not the ids |
| Raise to the measured gap (row 186), not "never raises" (§4.3 step 4) | The finished body must pass `finish`'s end checks, and an edge end from another sheet can lie nearly `max(reach)` off the kept point, beyond `re + rv`. Row 186 asks for `Measured` gaps on every new vertex and edge end | Step 4's claim that identification needs no raise holds only for the vertex itself, not for the curve ends that move with it. The raise grows boxes and tolerances downstream |
| Chains anchored (R7, rows 108/186), unlike BO1 (row 214, no anchoring) | Stitch points are all operand vertices, and anchors keep their own points, so anchoring is well defined | In practice a chain's middle vertex is also within coincidence of the anchors' edges, so stitch returns `Edges` before anchoring matters until ST2. Anchoring is tested at the unit level |
| Same-sheet pairs refused whenever found, before anything else | Row 186: the refusal is per operand, and `check` reports the same pair | A pair inside a sheet that no other sheet comes near is refused too. That matches the row, but it is stricter than the pin, which only looks at merge candidates |
| Edge stage pending as a typed error, checked before building | No result silently leaves a joinable edge pair or a vertex on an edge unjoined (analytic first) | The box check is conservative: it can refuse sheets whose edges only pass within the box |
| Sheet 0 edited in place | Its ids survive, which is what a caller stitching onto a body expects; lineage records only the others | The result's ids depend on which sheet is first |

### 26.6 Tests

`tests/stitch.rs` (11):

- corner touch gives one vertex: 7 vertices, 8 edges, 2 faces; sheet 0's id kept; coincident
  lineage from operand 1; 3 created vertices, 4 edges and 1 face, each derived from operand 1;
  a `Coincident` warning; no raise;
- 0.5 zone apart joins and 2 zone apart stays apart;
- a 0.005 mm merge with a 0.01 accuracy vertex raises the kept vertex to 0.005, `Measured`;
- a shared edge gives `Edges`;
- a vertex on another sheet's edge gives `Edges`;
- one sheet's vertices within coincidence are `Refused`, operand 1, lower id first;
- another resolution gives `OtherDocument`;
- `Want::Solid` gives `Solid`;
- no sheets give an empty body; one sheet gives itself with empty `Changes` and a clean report;
- run twice gives equal reports, changes and points, and three permutations give the same points;
- a set stop gives `Cancelled`.

`points.rs` (5):

- a chain anchors to the nearest vertex;
- two sheets doubled is `Chain`;
- two of one sheet on one anchor is `Chain`;
- a same-sheet pair is refused, lowest first;
- the kept vertex is chosen by geometry, whatever the order.

No tests were removed.

### 26.7 Measurements

Release build, one thread, best of 5, on the second build machine under load (load average
about 12). Measured with a throwaway timer test, not committed. Unit quad sheets from
`bodies::generate(QuadSheet, 0, n)`.

| Case | Sheets | V | E | Identified | Work | Time | Per vertex |
|---|---|---|---|---|---|---|---|
| two 1x1, corner | 2 | 8 | 8 | 1 | 50 | 0.006 ms | 0.76 us |
| two 10x10, corner | 2 | 242 | 440 | 1 | 6,287 | 0.38 ms | 1.57 us |
| two 30x30, corner | 2 | 1,922 | 3,720 | 1 | 127,447 | 3.7 ms | 1.91 us |
| two 60x60, corner | 2 | 7,442 | 14,640 | 1 | 940,687 | 15.9 ms | 2.14 us |
| checker 4x4 of 4x4 sheets | 8 | 200 | 320 | 9 | 5,117 | 0.39 ms | 1.93 us |
| checker 10x10 | 50 | 1,250 | 2,000 | 81 | 74,399 | 2.8 ms | 2.22 us |
| checker 20x20 | 200 | 5,000 | 8,000 | 361 | 577,949 | 12.3 ms | 2.46 us |

Time is close to linear in V. Work, which charges one unit per slab candidate, grows like
V^1.5 on these flat grids. The edge gate's thinnest-axis slab holds a whole row of vertices,
because every vertex lies in z = 0 and an axis-aligned edge has a near-zero span on one axis
only. The pin has no equivalent gate. ST2 replaces the gate with real edge candidates (box
tree), and a box tree over vertices would make the gate O(V log V) if the gate stays. This is
not a regression of existing work: the crate is new.

### 26.8 Deviations and open questions

Deviations from §4.3 and the ST1 row: the return type (§26.5), argument order, kept record by
geometry, and the raise. ST1 does not build the edge stage, splits, orientation or `Want::Solid`.
These are ST2 and ST3, and each returns a typed `Unsupported` now.

Open questions:

1. Should ST2 keep `Stitched` or move the report into `Done<C>`? If ST2 skips undecided edge
   pairs, `C` becomes their cause and `NoItems` goes.
2. Row 186 says "a's record kept". Should the 04 wording change to "the smallest reach, then the
   lowest point" so that ST4's permuted-bits truth holds?
3. A chain end to end only reaches anchoring once ST2 can tell that the middle vertex's
   nearness to an anchor's edges is explained by the identification. Until then, chains surface
   as `Edges`.

### 26.9 Proposed 04 row

| 11-27 | (For the owner's review) `scx_stitch::stitch(&[&Brep], Want, &mut Cx) -> Result<Stitched, StitchError>`: sheet `i` is operand `i`, sheet 0 is edited in place, and the others are copied in with `Origin::Operand` lineage. Vertices of different sheets within coincidence are one vertex, from one grid over all sheets. Two of one sheet are `Refused(VerticesWithinCoincidence)`. A chain is anchored to the nearest vertex of its doubled sheet, else `Unsupported(Chain)`. The kept record is the member with the smallest reach, then the lowest point, so permuted input gives the same points. That vertex is raised, `Measured`, to the farthest edge or pcurve end the merge moves, where that is beyond its reach. Until ST2, edges to join or split, and a vertex inside another sheet's edge box, are `Unsupported(Edges)`; `Want::Solid` is `Unsupported(Solid)`. `Stitched { done: Done<NoItems>, report: StitchReport }`, since a stitch skips nothing. Edges: stitch → bodyq | Amends row 186's "a's record kept" for stitch, so that §4.4's permuted-bits truth holds; amends §4.3 step 4 ("never raises"), since moved curve ends need the measured gap row 186 asks for | 74 §26 |

## 33. BW6 as built (11-28)

`scx_bodyq::{prism_section, prism_section_charged, prism_equivalent, bores, stack_bores,
hole_stacks}` and their types, in `crates/scx-bodyq/src/recognize/` (`prism.rs`, `piece.rs`,
`bores.rs`, `stacks.rs`), branch `bw6`. No new Cargo edge. One additive re-export in
`scx-testkit` (`bodies::made::{Region, Segment}`, the types `extrude` takes) so the tests can
build bodies without a `scx-planar` edge on bodyq. New error variants: `BodyqRefusal::{FrameTooFar,
BadOption}` and `BodyqDefect::Tolerance`.

### 33.1 Shape

```rust
pub fn prism_section(&Brep, &Frame, &mut Cx) -> Result<Recognition, BodyqError>;
pub fn prism_section_charged(&Brep, &Frame, reach: f64, &mut Scratch, &mut Budget, &Stop)
    -> Result<Recognition, BodyqError>;
pub enum Recognition { Prism(PrismSection), NotPrism(NotPrism) }
pub struct PrismSection { lo, hi: f64, outer: Vec<Piece>, holes: Vec<Vec<Piece>>,
    tolerance: Length, area: Ivl, volume: Ivl }
pub enum Piece { Line { a, b: Point2 }, Arc { center: Point2, radius, start, sweep: f64 } }
pub struct NotPrism { at: Entity, why: Why }   // Why: Sides, Slanted, Kind, Caps, Vertex, Edge, Wire, CapsDiffer
pub fn prism_equivalent(&Brep, &Brep, &mut Cx) -> Result<Equivalence, BodyqError>;
pub enum Equivalence { Same, Different, NoPrism }

pub fn bores(&Brep, &mut Budget, &Stop) -> Result<Vec<Bore>, BodyqError>;
pub struct Bore { point: Point3, axis: UnitVec3, d: f64, t: [f64; 2], slot: Option<Slot> }
pub fn stack_bores(&[&[Bore]], &StackOptions, &mut Budget, &Stop) -> Result<Stacks, BodyqError>;
pub fn hole_stacks(&[&Brep], &StackOptions, &mut Budget, &Stop) -> Result<Stacks, BodyqError>;
pub struct StackOptions;   // new(touching, axis_tolerance) checked; with_slots, with_diameters
```

The pin's `prism::measure::{section, equivalent}` and `eval::hole_stacks::{bores, stacks}` (with
the ops filter by slots and diameter ranges) are the sources. The prism and the bores take the
body as `&Brep`; the prism takes `&mut Cx` because its face sizes are `tight_box`es (BW2), kept in
`Scratch` with their work; bores and stacks keep nothing, so they take `(&mut Budget, &Stop)` (R8).

### 33.2 What was removed, and what replaces it

| In the pin | Here |
|---|---|
| 24 `.ok()?` in the prism code; `None` for "not a prism", a failed read, and a spent budget alike | `Recognition::NotPrism { at, why }` is a proof by one entity; a NURBS face that is no quadric is `Unsupported(Nurbs)`, budget and stop are `Exhausted`/`Cancelled`; a face box or uv box that cannot be had is a `Defect` |
| `tol = size * 1e-10`, `identity_tol = size * 1e-7`, `1e-12` on a normal's length, a `1e-5` volume check | one length, the body's coincidence (`coincident(res, largest accuracy of any face, edge or vertex)`), plus the rounding of the frame's coordinates and any recognised face's deviation; an angle is a length (the angle times the face's tight-box diagonal) |
| `COAXIAL 1e-3`, `NEAR 1e-6`, `SLOT_END 0.6`, `PARALLEL 1 - 1e-5`, opening dot `0.9`, rim `2e-3 r` | the coincidence; a slot end's sides stand a diameter apart within it at the radius; two axes are parallel within the options' `axis_tolerance` over the bores' extent; openings face along the line between axes within it at the radius |
| `MAX_FACES 20000`, `MAX_CROSSES 4,000,000`, `MAX_HOLES`: refuse | the caller's `Budget`: a unit per face and per comparison, `Exhausted` when it runs out (R6) |
| `1 - 1e-4` on slot directions | `parallel(.., axis_tolerance)` as for axes |
| a section only for a convex straight outline or a circle with separated round holes (the separation proof) | any loop of lines and arcs: the volume is the area with the coincidence's corridor, `2 c P + pi c^2` on the area, `2 c` on the height |
| origin refusal by an `error <= tol` inside the section | `Refused(FrameTooFar)` before reading: the rounding of the frame's coordinates must be within a quarter of the coincidence |
| the pin's frame for `equivalent` from the first planar face only | every distinct planar-face direction of `a`, in face id order; `Same` along one, `Different` when `a` is a prism along some and no such direction gives equal levels and loops, `NoPrism` when `a` is none |
| bores `None`/skip when an edge's curve could not be read | a collapsed edge on a cylinder wall is no bore wall (an answer); everything else is an error |

### 33.3 The recognition (what is proved)

A body is a prism along the frame's z when, in this order (each entity read charged one unit):

1. every face has the one solid region on one side and the outside on the other, the solid sides
   all one shell (so no cavity, no touching second solid, no sheet);
2. every face is a cap (a plane square to z) or a wall (a plane along z, a cylinder with its axis
   along z), by the stray over the face's tight box: `|n x z| * diagonal <= c` (cap), `|n . z| *
   diagonal <= c` (plane wall), `|axis x z| * diagonal <= c` (cylinder). A cone or NURBS face first goes
   through SU7's `recognize` over its uv box (cut to the surface's domain, since the certified box
   is rounded outward past a cone's apex): a plane or cylinder within `c` is read as one and its
   deviation is added to the tolerance; a cone that stays a cone is `Kind`; a NURBS surface no
   quadric within `c` is `Unsupported`;
3. exactly one cap faces `+z` and one `-z`, the upper more than `c` above the lower;
4. every vertex is on one of the two levels; every cap edge is a line or a circle about an axis
   along z with both ends at its cap's level; every other edge is a line from one level to the
   other, straight along z (its two ends within `c` in the plane); no edge is collapsed or a wire;
5. the two caps' loops are the same curves within `c`, from any start and cut into any pieces
   (adjacent pieces on one line or one circle merge first, a turn within `c` of whole is the whole
   circle).

Then the body is the region between the levels over the lower cap's section (the walls stand on the
cap loops by 4 and 2; the boundary projects to the section). The section is the lower cap's own
loops as pieces, the outer counter-clockwise and holes clockwise in the frame. Item 5 cannot fail
on a valid body by the argument just made; it is kept as a proof, not trusted (`CapsDiffer`).

### 33.4 Cases for and against

- *One coincidence for the whole body, not one per entity.* For: a bound has to cover every entity it
  reads; one number is deterministic and cheap. Against: one loose edge anywhere widens every test
  (a 1e-3 accuracy on a far fillet widens the tilt allowance on the whole plate). Per-entity
  coincidences would need the tolerance threaded through every comparison; left as a refinement.
- *The tight-box diagonal as the stray's length (the pin's `face_straying`).* For: certified (BW2), and
  a long thin face is allowed a smaller angle than a short fat one. Against: cold cost (below); a
  whole-body diagonal would be one box, but would refuse a long body whose small face is nearly square.
- *`Why` as a closed set of reasons, no free text.* For: callers branch on it (the app shows which
  face). Against: a new reason is an API change (`#[non_exhaustive]`).
- *Not a prism is `Ok(NotPrism)`, not an error.* For: it is the answer to "is it a prism"; errors are
  for "could not tell". Against: a caller has to match twice.
- *`hole_stacks` filters inside.* The pin's ops layer filtered slots and diameter ranges after the
  kernel; here `StackOptions` holds them, since `bores` is public for a caller that wants its own.
- *Lone bores carry their index* (`Lone::bore`), which the pin's did not; the caller can name the
  bore (and so the face) without matching on a float.

### 33.5 Tests (`crates/scx-bodyq/tests/recognize.rs`, 29)

Truth is the closed form (area, volume, level, radius, extent), never the pin's output.

Ported from the pin (the two kernel and three ops cases that concern recognising a body; see
33.7 for the other 71 of `prism/tests.rs`):

- `a_drilled_prism_has_its_section_and_its_volume_enclosed`: `4000 - 360 pi` in the volume,
  `400 - 36 pi` in the area, the corridor no wider than `2 c P + pi c^2`, the pieces' own Green
  areas `400` and `-36 pi`, not a prism along x, `Same` for a twin, `Different` for a hole wider by
  `1e-4` and a hole moved `1e-4`;
- `a_box_is_a_prism_along_each_of_its_axes`: along z, x, y and reversed z, levels, area, volume;
- `a_prism_section_does_not_depend_on_the_query_frame_origin`: offsets 0, 300, 1e3, +-1e4 give the
  same section to 1e-8, `1e20` and `f64::MAX / 4` are `Refused(FrameTooFar)`, a body far from the
  origin read at a frame beside it;
- `a_drilled_box_moved_is_the_prism_it_was`: built in a turned and moved frame, a prism along its z,
  not along the world's, `Same` as its twin and `Different` from a wider one and from one lifted by a
  millimetre.

New: a curved section (a D) as lines and arcs; cone, frustum, sphere, cavity, stack, sheet, a pyramid
frustum and a stepped plate are `NotPrism` with their `Why`; a cone whose taper is `1e-9` is a prism
(SU7 recognises it) and one of `1e-3` is not; the tolerance is the document's (a tilt of `1e-4` is a
prism in a document of resolution 0.01 and not at the default); determinism, warm against cold
scratch, `Exhausted` and `Cancelled`; and for bores: a round hole, two halves of one cylinder, a
boss and a block (none), an obround slot (centre, length, direction), a half-circle notch and a
106 degree bite (not bores), a countersink and a drill point (built face by face; both checked with
`classify_point` so the fronts are known to face the air), budget and stop; for stacks: the pin's ten
cases (`three_coaxial_holes_are_one_stack`, `a_hole_two_tenths_off_is_no_stack`, the counterbore, the
millimetre gap, two stacks, a round hole over a slot, slots on one centre line, the chain of four
slots, a far bore, and `too_many_nearly_coaxial_holes`, now `Exhausted`), option checks, and three
plates with coaxial holes through `hole_stacks` (twice, a hole off by a millimetre, the diameter
filter).

### 33.6 Measurements (release, one thread, loaded build host, side-channel timer, not committed)

| Case (drilled 20 x 20 x 10 plate, hole radius 6) | Pin | Here |
|---|---:|---:|
| prism section, cold scratch | 1,967 us | 135 us |
| prism section, warm scratch (tight boxes memoized) | 1,967 us | 13.7 us |
| `equivalent` of two twins | 46 us | 159 us (cold scratch, both bodies' tight boxes and two sections) |
| `bores` of the plate (one round hole) | not timed (the pin needs a `Record`) | 0.4 us |
| stacks, 2,000 x 2,000 bores one tenth off axis, all in neighbouring cells | refuses at 4,000,000 comparisons after 81 ms | completes: 8,002,000 comparisons, 162 ms (20 ns each, as the pin's) |
| stacks, 4,000 bores per body spread 3 mm apart, the second set 0.1 above | 7.1 ms | 6.4 ms, 12,000 comparisons |

Median of 200 runs for the prism rows. The section is 15 times faster than the pin's cold (the
pin's cost was not profiled; here a face is a closed form and a box). The one row where this is
slower is `equivalent` (3.4 times, cold): it reads both bodies' faces through `tight_box` (the
cold cost; a section is 13.7 us once the boxes are kept) and runs a section for each. Two warm
sections would be about 27 us (2 x 13.7, not measured as such), under the pin's 46 us. The pin's stacks refuse past 4,000,000
comparisons, a cap the budget replaces; the per-comparison cost is the same. Counts of work added
over the pin's: none on an existing path (these are new functions in a crate whose callers are
rows still to be built).

### 33.7 Not ported, deviations, open questions

- **"Research 73's prism tests".** Research 73 is the SSI marcher note and has none. The 73 is 74
  §2.4's count of the pin's `prism/tests.rs`. Of those, the two measure cases are ported, and of
  `solidcore-ops/tests/prism.rs`'s 35 the three section/equivalence cases. The other 71 test
  profiles, arc conversion, ellipse recognition, regions, sheets, extrusion, members and gussets:
  the planar and make rows' code (PL6, make), not recognition. They are not ported here.
- **Not ported:** `a_prism_section_refuses_a_wall_displaced_from_its_caps` (a wall shifted along its
  own axis while the caps stay; `finish` refuses such a body, so the case cannot be built; the
  nearest cases are the tilt tests); ruled spline net walls and extruded surfaces as walls (a NURBS
  face goes through SU7's `recognize` and is a prism wall only if it is a plane or cylinder; the
  degree-one ruled net the pin proves has no scx kind yet, so it is `Unsupported(Nurbs)`). A NURBS
  wall that SU7 recognises is untested here (no cheap way to build a finished body with a NURBS face
  and exact pcurves; the cone case exercises the same path).
- **Deviations from the §8 row:** `prism_equivalent` is `Equivalence` (three-way), not `Option<bool>`;
  `bores` takes a budget and a stop; `hole_stacks` returns `Lone::bore`; `StackOptions` is built by a
  checked constructor, not a literal.
- **Open:**
  1. One coincidence per body (33.4). Per-entity would sharpen a body with one loose edge.
  2. `Why::Sides` lumps three causes (a face with no solid side, a face with solid on both sides, a
     second shell); split it if the app needs to say which.
  3. `prism_equivalent` is "same in a frame at `a`'s first vertex", so two bodies that are the same
     prism placed differently are `Different`, as in the pin. A "congruent prisms" query would need a
     rigid fit of the sections; not asked for.
  4. Slots whose ends are not half circles (a stadium whose ends are arcs of other angles) are not
     recognised, as in the pin.

### 33.8 Proposed 04 row

BW6: `scx_bodyq::prism_section(&Brep, &Frame, &mut Cx) -> Result<Recognition, BodyqError>` answers
`Prism(section, levels, tolerance, area and volume enclosures)` or `NotPrism { entity, why }`, never
`None` for a failure; its tolerance is the body's coincidence plus the frame's rounding, and a frame
too far for that is `Refused(FrameTooFar)`. `prism_equivalent` is `Same`/`Different`/`NoPrism`.
`bores(&Brep, &mut Budget, &Stop)` and `hole_stacks(&[&Brep], &StackOptions, &mut Budget, &Stop)`
take every length from the coincidence or the caller's options, and the pin's face and comparison
caps are the budget. Edges: none. | Amends the §2.3 "constants from `cx.doc`": the bodies carry their
resolution, so the constants come from the body | 74 §33
## 25. BW4 as built (11-27)

Two queries across an edge, in `crates/scx-bodyq/src/{along,tangent,continuity}.rs`:

```rust
pub fn tangent_all_along(body: &Brep, edge: EdgeId, max_angle: f64,
                         budget: &mut Budget, stop: &Stop) -> Result<Tangency, BodyqError>;
pub enum Tangency { Tangent { bound: f64 }, NotTangent { at: f64, angle: Ivl },
                    Undecided { at: f64, angle: Ivl } }

pub fn continuity(body: &Brep, edge: EdgeId, tol: &ContinuityTolerances,
                  budget: &mut Budget, stop: &Stop) -> Result<Continuity, BodyqError>;
pub struct ContinuityTolerances { pub gap: Length, pub angle: f64, pub curvature: f64,
                                  pub curvature_rate: f64, pub curvature_accel: f64 }
pub enum Order { G0, G1, G2, G3, G4 }
pub struct Continuity { pub proved: Option<Order>, pub limit: Option<Limit>,
                        pub measured: [Measured; 5] }
pub enum Limit { Refuted { order, at, drop: Ivl }, Undecided { order, at, drop: Ivl } }
pub struct Measured { pub at: f64, pub bracket: Ivl }
```

New refusals: `BodyqRefusal::Tolerance` (a tolerance that is not finite or is negative, or an
angle not below pi), `OneSided { edge }` (fewer than two fins) and `Collapsed { edge }`. More than
two fins is `Unsupported(NonManifold)`. A NURBS face is `Unsupported(Nurbs { at: Face })`.

### 25.1 Method

- **Normals from the edge's 3-D curve. No pcurve is read.** Each analytic surface has a normal
  field in closed form: a unit `n0(p)` and a signed distance `delta0(p)`, so the foot is
  `p - delta0 n0` and the surface's normal there is `n0` exactly. The plane uses its axis; the
  cylinder its radial direction; the cone its slant normal `c (e - k Z)`; the sphere the
  direction from its centre; the torus the direction from its tube circle. The natural normal is
  `sigma n0`, with `sigma` the surface's sense, checked against `scx-surface`'s normals. The faces
  are oriented coherently across the edge. Face a keeps its natural normal. Face b keeps its own
  where its use runs the other way along the edge, and has it turned where both uses run the same
  way. So a box edge reads pi/2, an equilateral prism's wall edge reads 120 degrees, and a smooth
  edge reads 0.
- **The angle without trigonometry on the proof path.** The proof uses `D = |na - nb|^2` and
  `S = |na + nb|^2`. The thresholds are `(2 sin(theta/2))^2` and `(2 cos(theta/2))^2`, held as
  intervals. A piece holds if `D < thrD` or `S > thrS`, and a point fails on the reverse. `D` is
  well conditioned near 0 and `S` near pi. The reported angle bracket intersects
  `2 asin(sqrt(D)/2)` with `2 acos(sqrt(S)/2)` and is padded by 1e-14.
- **Curvature drops G2 to G4 from the implicit equation.** At the foot `p*` with oriented normal
  `n`, the across direction is `w = n x T`. The height `z(a)` over `p* + a w` solves
  `F(p* + a w + z n) = 0`, with `F` the surface's implicit polynomial. The solve runs in a
  degree-4 power series and gives `z_k = -E_k / g` for `k = 2, 3, 4`. The three values are
  `kappa = 2 z2`, `kappa' = 6 z3` and `kappa'' = 24 z4 - 24 z2^3`. A circle section gives
  `kappa'' = 0`, which the sphere, cylinder and torus tests check. The drop is face a's value
  minus face b's.
- **One code, two rings.** The formulas are written once over a private `Num` trait. With `Ivl`
  they give a certified bracket at one parameter. With `TmN<6>` they give an enclosure over a
  piece that keeps correlation, so a constant angle stays tight over a whole edge where intervals
  would not.
- **Taylor models of the curve.** On a piece `[lo, hi]`, centre `m` and half-width `delta`, the
  coefficients are `C^(k)(m) delta^k / k!` for `k <= 5`. They come from `jet_over::<Ivl, 7>` at
  `m`. The remainder is the jet over the whole piece times `delta^6 / 6!`. A NURBS curve's
  interval is cut at its interior knots first.
- **Driver (§2.5, R6).** The driver works depth first, left to right, on a stack. Each piece
  enclosure and each midpoint bracket charges one budget unit and checks `stop`:
  - If the piece holds, its range joins the bound hull.
  - Otherwise the midpoint is bracketed. A failing bracket ends the proof with a witness.
  - A straddling bracket is recorded as undecided: the worst by its upper end, ties to the left.
    The piece is not split, because no piece holding that point can decide.
  - A holding bracket splits the piece.
  - A piece whose midpoint is not strictly inside is undecided.

  There is no MAX_SPLITS. The floating-point floor and the one budget bound the work.
- **Orders in sequence.** `continuity` proves G0 (`|gap|^2 < gap^2`), then G1, then G2 to G4,
  each over the whole edge. The first order not proved is the `limit`. The drops are measured
  apart from the proof, at 9 evenly spaced stations with both ends included, whatever was proved,
  under the same budget. Each is a certified bracket at its station and a measure along the edge.
- **Identity.** If both fins have one analytic surface oriented alike (a seam, or two faces of one
  surface), the normal is a function of the point. The answer is then `Tangent { bound: 0 }`, or
  G4 with zero drops, with no work. This covers the analytic seams, so **SU6b's `seam_bound` is
  not needed for analytic edges**. A closed NURBS surface's two sides of a seam lie at different
  parameters, where the normals can disagree. That is SU6b's question, and it returns
  `Unsupported(Nurbs)` with every NURBS face.

### 25.2 Cases for and against

- **The normal field of the 3-D curve, not pcurves.** For:
  - It is closed form on every analytic surface.
  - It needs no SM1b pieces.
  - A NURBS edge curve between analytic faces works today; the crease test proves pi/2 on a
    quadratic with interior knots.
  - The pin's pcurve failure modes cannot arise: a pcurve a period off, or 1e-15 off a knot line.

  Against: the foot is the curve point's projection, not the pcurve's surface point. On an edge
  within its accuracy of both faces the difference in normal is second order in the gap, about
  1e-12 rad at 1e-6 mm. On an inexact edge the answer is about the faces near the curve.
- **Proved or Undecided (R2).** For: a sample never proves. Against: an angle exactly at the
  threshold, such as walls 0.3 degrees apart asked at 0.3 degrees, is Undecided. That is the
  truth at f64 precision; the test asserts it.
- **No margin parameter.** Callers pick `max_angle`. The pin's SMOOTH (0.5 degrees) stays in the
  tests.
- **R8 signature `(&mut Budget, &Stop)`, not `cx`.** Both queries are memo-free leaves. Against:
  a caller with a `Cx` passes its parts.
- **`Measured`, not `Drop`.** A struct named `Drop` shadows the prelude trait.

### 25.3 Tests

**`tests/tangent_edge.rs` (15).** `parts_by` checks NotTangent at `want - tol` and Tangent at
`want + tol`, with the bound or bracket holding `want`. Truth is the construction's dihedral,
never the pin's output.

- box, 12 edges, pi/2 to 1e-12;
- equilateral prism, 120 and 90 degrees;
- thin wedge, 175 and 92.5 degrees to 1e-9;
- cylinder caps pi/2, and the seam Tangent;
- frustum base `acos(-0.6)` and top `acos(0.6)`;
- shallow frustums at 1 to 80 degrees;
- oblique cylinder wall on `z = 0`, a hand-built sheet: tilt 30 degrees, an ellipse arc through
  t = 0 and pi. The maximum is 120 degrees to 1e-6, with the verdict table at
  125/119/90/60/59 degrees;
- stadium: line-arc edges Tangent with bound < 1e-9, caps pi/2. This replaces the pin's round by
  fillet;
- walls 0.3 degrees apart: Tangent at 0.5, NotTangent at 0.2, Undecided at 0.3 with `at` inside
  and the bracket holding 0.3;
- sphere, torus and cone seams;
- refusals: Tolerance at -1, NaN, inf, pi and 4; NoEntity; OneSided on a sheet boundary;
- a NURBS face is `Unsupported(Nurbs)`;
- a NURBS crease;
- budget `Exhausted` and stop `Cancelled`;
- determinism: equal answer and equal `used`.

**`tests/continuity.rs` (8).**

- box: G0 proved, G1 refuted, drops 0 / pi/2 / 0 / 0 / 0;
- stadium: G1 proved, G2 refuted, kappa drop 1/2 to 1e-9, rates 0;
- arcs of radius 3 and 2 tangent inside one another: drop 1/6, the difference and not the sum.
  This is the sign check;
- cylinder caps G0 and seam G4;
- kink at the angle tolerance: Undecided G1;
- tolerance refusals;
- one budget for every order: `used - 1` is Exhausted, and `used` gives the same answer;
- with the curvature tolerance at 0.6, the stadium is proved G4 through the Taylor-model series
  solve.

**`src/along.rs` unit tests (3).** These port the pin's in-module `query/continuity.rs` curvature
tests:

- a sphere bends by -1/r both ways;
- a cylinder bends round and not along, and the other way when turned;
- a saddle on a torus's inner equator: -1/2 round the tube, +1/3 along the equator, rates 0.

**Not ported, on purpose, with what each proved:**

| pin test | what it proved | where |
|---|---|---|
| `a_box_corner_turns_a_right_angle` | `edge_turn` at a vertex | BW11 (vertex joints) |
| `a_line_runs_on_into_its_tangent_arc_without_a_turn` | the joint turn of a line into its arc is 0 | BW11. The edge half is here (stadium) |
| `an_l_section_s_inside_corner_is_concave` | `convex_at` sign | BW11 |
| `joint_turn_is_the_angle_between_the_directions`, `a_zero_or_non_finite_joint_direction_is_refused` | `joint_turn` and its refusal | BW11 |
| `a_round_meets_its_faces_smoothly` | a fillet's edges are tangent | replaced by the stadium, since there is no blend op yet |
| `a_procedural_round_is_smooth` | the same on a procedural round | needs blends; NURBS faces pending |
| `a_pcurve_a_period_off_its_face_is_refused` | the pin's pcurve-based normal refuses a mis-windowed pcurve | does not apply: no pcurve is read |
| `a_pcurve_1e_15_off_a_knot_line_is_on_its_own_side` | the side choice at a NURBS knot line | does not apply, and a NURBS surface is pending |
| `a_cylinder_s_circles_and_parameter_lines_bend_by_their_closed_forms` | curve and iso combs | BW10 (combs). The section curvature is covered by the unit tests |
| `a_disc_capping_a_tube_meets_it_at_stations_on_the_circle` | `continuity_over` two separate bodies | BW10 / sheet seams |
| `points_above_and_below_a_sheet_are_on_its_two_sides` (in-module) | a side query on a sheet | not a continuity test; `classify_point` and `sides_along` cover sides |

The row's "6" cites `surface/continued.rs`, which holds 6 surface-growth tests unrelated to
continuity. I took it as a mis-citation and ported the in-module curvature tests in its place.

### 25.4 Costs (release, server build, mean of 200 runs)

| edge | `tangent_all_along` | units | `continuity` | units |
|---|---|---|---|---|
| box edge (at 91 degrees) | 1.7 us | 1 | 226 us | 48 |
| cylinder cap circle | 42 us | 10 | 901 us | 141 |
| frustum base | 106 us | 22 | 1,014 us | 141 |
| stadium line-arc | 9.7 us | 1 | 482 us | 49 |
| oblique ellipse rim | 35 us | 7 | 583 us | 93 |
| NURBS crease (3 spans) | 15 us | 3 | 267 us | 50 |

Before: n/a. These are new functions and no existing path changes.

`continuity` is dominated by its 45 station brackets (5 orders x 9) and, on a full circle, by G0.
G0 must enclose a gap below 1e-6 mm, so it splits the circle into about 30 pieces.

### 25.5 Deviations

- R8 `(&mut Budget, &Stop)` instead of §2.3's `cx`.
- No margin.
- No MAX_SPLITS.
- No pcurves.
- `Measured` for `Drop`.
- The `continued.rs` citation (above).
- A test-only dev-dependency on `scx-planar`, for `Region` and `Segment` to extrude profiles.
  It lies below bodyq in layers.txt. The row lists no edges, and the normal deps are unchanged.
- SU6b is not needed for analytic seams (identity). NURBS seams and every NURBS face return
  `Unsupported(Nurbs)` until step 5's surface pieces exist.

### 25.6 Open questions

1. Should a station drop stay a certified bracket at a point (as built), or should it be a sample
   reported as such? The bracket is what a caller can rely on, but it costs 45 units per call.
2. G0 on a full circle splits into about 30 pieces to enclose a gap below 1e-6 mm. Should G0 use
   SU6's `gap` bracket instead of its own Taylor model?
3. NURBS seams wait on SU6b's `seam_bound`. Should BW4b port it, or should SU6b?
4. Sign convention: kappa is negative where the face bends away from its outward normal (a sphere
   reads -1/r). Is that the convention BW10's combs want?
5. No test yet has two different but coincident surface records across an edge proved G2 by the
   series rather than by identity. One should be added once a builder can make such a pair.
6. Should `tangent_all_along` also take a face pair, for BW10's face chains?

### 25.7 Proposed 04 row

| 11-27 | (For the owner's review) **BW4: `tangent_all_along` and `continuity` in scx-bodyq.** Both read each analytic face's normal field at the edge curve's point (closed form; no pcurve), oriented coherently by the fins' senses. `tangent_all_along(&Brep, EdgeId, max_angle, &mut Budget, &Stop) -> Tangency { Tangent{bound} \| NotTangent{at, angle} \| Undecided{at, angle} }` is proved over Taylor-model pieces under one budget, refuted by a certified bracket, and has no split cap. `continuity(.., &ContinuityTolerances, ..) -> Continuity { proved: Option<Order>, limit: Option<Limit>, measured: [Measured; 5] }` proves G0 to G4 in turn (gap, angle, and the jumps of the normal section's curvature, its rate and its rate's rate from the implicit equation) and reports the first order not proved as Refuted or Undecided. The drops are measured apart from the proof at 9 stations. A seam of one analytic surface is G4 by identity, so SU6b is needed only for NURBS seams. NURBS faces are `Unsupported(Nurbs)`; NURBS edge curves work, cut at their knots. New refusals: Tolerance, OneSided, Collapsed. Edges: none (scx-planar as a test-only dev-dependency) | 74 §25 |

## 29. SP2 as built (11-27)

`scx-step` builds the analytic subset: a body of planes, cylinders, cones, spheres and tori
bounded by lines, circles, ellipses, parabolas and hyperbolas is made from the read's plan
(`crates/scx-step/src/build.rs`, `geom.rs`, `import.rs`). Every other kind is a typed pending
error. Cargo edges added: `scx-brep`, `scx-check`, `scx-curve`, `scx-mass`, `scx-surface`
(the row named check, mass and surface; brep and curve are added because the build fills a
`Draft` and makes the edge curves). `scx-brep` gains one query, `FaceRecord::is_bounded_region`.
21 new tests in `crates/scx-step/tests/analytic.rs` and 3 unit tests in `geom.rs`, all on
synthetic files checked against closed forms; the 34 SP1 and SP8 tests stay, one with a
corrected expectation (§29.4).

### 29.1 Public API

```rust
pub fn build(ex: &Exchange, body: &ImportedBody, cx: &mut Cx<'_>) -> Result<Built, BuildError>;
pub fn import(ex: &Exchange, opts: &ReadOptions, cx: &mut Cx<'_>) -> Result<Import, StepError>;
pub struct Built { entity, name, kind, body: Brep, raised: Vec<Raise>, notes: Vec<Note>,
                   findings: Findings, faces: Vec<(u64, FaceId)>, edges, vertices }
pub struct Unbuilt { entity: u64, why: BuildError }
pub struct Import { read: Imported, built: Vec<Built>, unbuilt: Vec<Unbuilt> }
pub enum Note { TurnedFace{face}, TurnedEdge{edge}, TurnedShell{shell}, PastRatio{entity, ratio} }
pub enum Waits { BSplineCurve, BSplineSurface, Polyline, OffsetCurve, OtherCurve, Revolution,
                 Extrusion, OffsetSurface, DegenerateTorus, TrimmedSurface, OtherSurface,
                 Seamless, VertexLoop, PcurveFit, NonManifold, NurbsGap }
pub enum GeomCause { Placement, Curve(CurveError), Surface(SurfaceError) }
pub enum BuildError { Pending{entity, waits}, Malformed, Geometry{entity, cause},
                      Degenerate{edge}, Undecided{entity, gap, reached}, Gap{entity, cause},
                      Pcurve{face, cause}, NotOrientable{shell}, Flat{shell},
                      Unfinished(FinishCause), Mass{shell, cause}, Check, TableFull }
// scx-brep
impl FaceRecord { pub fn is_bounded_region(&self) -> Option<bool> }
```

Each `BuildError` has `kind()` (01 §5: `Pending` is `Unsupported`, `Undecided` `Exhausted`,
the file's faults `Refused`) and a stable `name()`; each `Waits` names its task. `import`
builds each body the read lists once, gives the ones that fail as `Unbuilt` with their cause,
and adds every built body's raises to `ImportReport::raised` in entity order. The caller
places them with `ProductTree::placed`.

### 29.2 The build, against the pin

| Pin (`solidcore-step` topology read and build) | Here |
|---|---|
| Vertices within a tolerance joined into one | none joined: the file's sharing is the topology. Two vertices at one point stay two, and the edges and faces that use them say whether the body closes |
| An edge's accuracy from the file's uncertainty or a sampled distance | the certified enclosure of curve-to-surface for each face that uses the edge, and vertex-to-curve for each end. Within the zone: no accuracy. Past it: the bound is recorded, a `Raise` where it is past the stated uncertainty (R3), a `Note::PastRatio` past `MAX_ACCURACY_RATIO`. A straddle is `Undecided` (R2) |
| Face sense from `same_sense` | from `same_sense`, checked against the proved region sign of its loops on faces with a marked outer bound or on a plane; a disagreement keeps the loops and turns the face (`Note::TurnedFace`) |
| The outer loop: the `FACE_OUTER_BOUND`, else the first | the marked one, else the one loop whose region alone is bounded |
| Pcurves from the file's `PCURVE`s where given | not read: the exact writer gives each use its closed-form pcurve, with a collapsed edge at each joint at a pole; an edge with no closed form waits (`PcurveFit`) |
| Orientation from the file as written | parity within each shell (an odd cycle refused), then the certified volume sign of each solid shell; a negative one turns the shell (`Note::TurnedShell`), a zero one is `Flat` |
| Unknown kinds: skipped or `String` errors | typed `Pending{entity, waits}`; nothing of the body is kept |

### 29.3 Choices, with cases for and against

- **No vertex joining.** For: joining by distance is a heal (the read never heals, §6), and
  a file that writes two vertices at one point says they are two; a later `scx-stitch` (SP4)
  closes gaps on request. Against: files from careless writers that duplicate vertices fail to
  close (`Unfinished`) where the pin's join would have built them. That is the right result
  under the rulings, and SP4's `closing_gaps` is the remedy.
- **`VERTEX_LOOP` waits.** A cone or sphere whose bound reaches the apex by a vertex loop has
  no edge for the writer's pole joint; building it needs a collapsed edge the file did not
  give and a loop the writer does not take today. For: honest, rare in exporter output (they
  write the seam to the apex). Against: one more pending kind; the fix belongs to the writer.
- **A collapsed edge at every joint at a pole, added before writing.** First tried as a retry
  after the writer said `Winds`; a seam-only sphere then put both seam uses at one u (zero
  area) and the check refused it. Adding it at each pole joint up front gives the writer the
  turn it needs and costs one edge per pole. For: the body is the standard's (a pole is a
  collapsed edge in the kernel's model). Against: the edge count is not the file's;
  `Built::edges` lists the file's edges only, so a caller mapping entities is not misled.
- **Outer loop by region sign when none is marked.** For: `FACE_BOUND` alone is valid
  (ISO allows it) and the first-loop rule is a failing default. Against: costs one sign per
  loop on such faces; a face where no single loop bounds a region (a cylinder band) keeps the
  file's order, which is what the writer needs there.
- **Turned faces, edges and shells are notes, not errors.** For: the topology and the vertices
  are the stronger data; a flag that disagrees with them is the writer's slip, and the note
  says what was changed. Against: a caller that wants a strict read must look at `notes`.
- **Trims are not read; edges are bounded by their vertices.** For: Part 42 says the edge's
  extent is its vertices; trims of a `TRIMMED_CURVE` basis repeat them, sometimes wrongly.
  Against: a file whose vertices are worse than its trims gets a larger raise. The raise is
  recorded.
- **Gap claim `Claim::CurveToIntersection`.** The gap of an edge curve to one face's surface is
  recorded under the existing intersection claim. A `Claim::CurveToSurface` would say it
  exactly; proposed for the owner (it is a `scx-num` change outside this row).
- **Each body built once.** The product tree's placements are maps; building per instance
  would repeat the work and make equal bodies differ by rounding.

### 29.4 Correction to SP8: the default reference direction

SP8 wrote `first_proj_axis` from memory (+Z, then +X). The standard's EXPRESS text gives
`dummy_gri || direction([1.0, 0.0, 0.0])`, and `[0.0, 1.0, 0.0]` when the axis's ratios are
`[1, 0, 0]` or `[-1, 0, 0]`. `geom::default_ref` is that rule, and `products.rs` uses it for
part placements and operator axes. The SP8 test for an axis along +X now expects x = +Y
(it expected +Z); the -Z case is unchanged. This matches the pin's rule without its 0.9
threshold.

### 29.5 Against CF2a

CF2a (§28) merged while this row was built; the build is now checked by its certified gaps under
`Scope::Import`. Every edge and vertex accuracy on an imported body is a certified enclosed gap
or none, so its allowances (the ball and the tube, `max(acc, zone)`) hold by construction. All
21 analytic bodies give no findings and no undecided items. That includes the two raised
cubes: a vertex 1e-3 mm off and one 0.05 mm off, past the ratio at 5,000 zones, where a wide
accuracy is a recorded fact under `Import`, not a finding. A finding on an imported analytic
body is a bug in the gap, the record or the check.
The figures in §29.7 include CF2a's check.

### 29.6 Tests

`analytic.rs` (21): a cube in each style (six faces, twelve edges, eight vertices, volume);
faces and edges written against their sense are read by their loops and vertices (notes); two
vertices at one point are not joined; a vertex off its edges is raised to its gap; a raise past
the accuracy ratio is made and noted; a cylinder's seam is used twice; a ball bounded by its
seam alone; a cone with its seam to the apex is a third of its cylinder; a cone reaching its
apex through a vertex loop waits; a plate with a through hole (marked and unmarked outer); a
solid with a void; a void written inside out is turned by its volume sign; a surface model of
open shells is a sheet; parabola and hyperbola edges bound faces; an inch file is built in
millimetres; a torus is one face bounded by its two seams (volume 2 pi^2 R r^2); building twice
gives the same body; a spline edge waits; `import` builds each body once and lists the rest;
an edge of three faces waits; an edge of no length is refused. `geom.rs` (3): the standard's
default reference direction, a negative focal distance keeps the points, swept kinds wait by
name. No test was removed. The pin's STEP topology tests read real exporter files whose
licences are unverified (§7); their cases are rebuilt as the synthetic files above.

### 29.7 Measurements

A throwaway timer (never committed; release, a loaded machine, best of 5, 3 for the largest) on
generated plates with n x n through holes, `scx_step::import` against the pin's `read_solids`.
The pin's read only reads the records; `import` also encloses every gap, writes the pcurves,
orients, finishes and runs the import check, so it does more work on the same file. The two
`import` columns are before and after main's CF2a (§28) was merged, which replaced the check's
gap questions with certified ones.

| File | Faces | `import`, SU6-era check | `import`, CF2a check | pin `read_solids` | ratio after |
|---|---|---|---|---|---|
| 1 hole | 7 | 2.77 ms | 18.0 ms | 2.29 ms | 7.8 |
| 4 holes | 22 | 13.8 ms | 331 ms | 16.2 ms | 20.5 |
| 25 holes | 106 | 80.4 ms | 1,449 ms | 86.6 ms | 16.7 |
| 100 holes | 406 | 273 ms | 6,307 ms | 386 ms | 16.3 |

Both are linear in the holes. A split with a second throwaway timer on the 4-hole plate:
the check takes 221 ms of the 250 ms import, and the build's own certified gaps take 4 ms. So
the added work is the check's, which proves again every gap the build has just enclosed
(about 15 ms a hole: four circles on their cylinders and planes). A profile was not taken (the
machine does not allow `perf` events).

### 29.8 Open questions

- `Claim::CurveToSurface` for the edge gap (above).
- `VERTEX_LOOP` at an apex: the writer would need to take a loop of one collapsed edge.
- Seamless faces (a loop that winds without a seam): adding a seam changes the file's
  topology; is that SP4's, or the writer's?
- **The import check costs 16-20 times the pin's read** (§29.7), and almost all of it is CF2a's
  gaps proving again what the build has just certified. Options for the owner: the check could
  take a gap the build recorded with its `Cert` as decided; `import` could offer the check as a
  separate call; or CF2a's curved-edge cost (its §28.9, open) could come down. Until one of these
  is decided, `import` runs the full check as the row asks.

### 29.9 Proposed 04 row

| 11-27 | (For the owner's review) `scx_step::{build, import}` build the analytic subset (planes, cylinders, cones, spheres, tori; lines and conics) from the read's plan: the file's sharing is the topology (no vertex joining), each edge and vertex gap enclosed and, past the file's uncertainty, raised and recorded (R3; `Note::PastRatio` past the ratio), exact pcurves with a collapsed edge at each pole joint, parity then the certified volume sign (R4), and the import check. Disagreeing `same_sense` flags are turned and noted, not refused. NURBS, swept, offset, vertex-loop, seamless and non-manifold cases are typed `BuildError::Pending(Waits)`. Edges: brep, check, curve, mass, surface | Joining by distance and trusting flags over topology are heals and failing defaults; certified gaps make every accuracy a proof | 74 §29 |

## 34. BW11 as built (11-28)

The pin's small geometry, nine modules, `crates/scx-bodyq/src/{joint, face_pair,
mid_sheet, mid_joint, cylinder_wall, cap, face_surface, round_face, reach}.rs`, all typed, none
reading a pcurve, none with a fixed angle or length. The row's list is `face_pair`, `mid_sheet`,
`mid_joint`, `cylinder_wall`, `cap`, `face_surface`, `round_face` and `joint_turn`; the vertex-joint
tests BW4 left (§25.3) need the edge-turn module `joint_turn` lives in, so that is ported whole
(`leaving`, `turn_at`, `convex_at`, `joint_turn`; `turn_to_parallel` stays with BW9's measure map).
No new Cargo edge: the code uses `scx-surface`, `scx-curve`, `scx-nurbs` and `scx-num`, already
normal dependencies. New refusals in `BodyqError`: `NoDirection`, `NotPositive`, `OnAxis`,
`OffPlane`, `Corners`, `NotAnEnd`, `Seat`, `WrongSurface`, `NoMaterialSide`; `Unsupported(Probe)`;
`Defect(Moved)`. One word of BW4's `along.rs` changed: `Field::of` is `pub(crate)`.

### 34.1 API

```rust
// the constants of the pin's kernel, as the caller's resolution over the caller's reach
pub struct Near; // Near::new(coincidence, reach), Near::of(res, acc, acc, reach)

// joint.rs (vertex joints; the BW4 tests)
pub fn joint_turn(before: [f64; 2], after: [f64; 2]) -> Result<f64, BodyqError>;
pub fn leaving(&Brep, EdgeId, VertexId) -> Result<Leaving, _>;   // Direction(unit) | Missing(Closed|Collapsed|Stationary)
pub fn turn_at(&Brep, e, f, v) -> Result<Turn, _>;               // Angle(f64) | Undefined { edge, why }
pub fn convex_at(&Brep, e, v, smooth) -> Result<Convexity, _>;   // Convex | Concave | Tangent | Undecided | NoMaterial{face} | NoEnd(why)

// face_pair.rs
pub fn facing_planes(&OutwardPlane, &OutwardPlane, &Near) -> Result<Facing<FacingPlanes>, _>;
pub fn facing_cylinders(&CylinderFace, &CylinderFace, &Near) -> Result<Facing<FacingCylinders>, _>;
pub enum Facing<T> { Pair(T), Apart(Apart) }   // Apart: Tilted | SameWay | OffAxis | SameSide | Air | Thin
pub fn outward_plane(&Brep, FaceId) -> Result<OutwardPlane, _>;  // read from the body
pub fn cylinder_face(&Brep, FaceId) -> Result<CylinderFace, _>;
pub fn material_toward_axis(axis_point, axis, point, outward, &Near) -> Result<bool, _>;
pub fn round_directions(axis, along, &Near) -> Result<Directions, _>;
pub fn axial_range_in(&CylinderFace, &CylinderFace, [Length; 2]) -> Result<[Length; 2], _>;

// mid_sheet.rs, mid_joint.rs, cylinder_wall.rs
pub fn wall_holds(&Wall, &[Point3], slack: Length) -> Result<Holds, _>;      // Yes | No { point: index }
pub fn polygon_area(&[Point3]) -> Result<f64, _>;
pub fn onto_plane(&OutwardPlane, Point3) -> Result<Point3, _>;
pub fn in_frame(&Frame, Point3) -> Result<Point2, _>;  pub fn from_frame(&Frame, Point2) -> Result<Point3, _>;
pub fn planes_meeting(&OutwardPlane, &OutwardPlane, &Near) -> Result<Meeting, _>;   // Line | Parallel | Unplaceable
pub fn move_edge_onto_line(&[Point3], edge: usize, &Line3, coincidence: Length) -> Result<Moved, _>;
                                                                        // Polygon | EdgeNotParallel | NeighbourParallel { edge }
pub fn wall_frame(normal, w, axis, radius, convex, &Near) -> Result<Wrap, _>;       // Frame | NotParallel | NotTouching

// cap.rs
pub fn is_square_plane_at_height(&PlaneAt, origin, normal, height, &Near) -> Result<bool, _>;
pub fn is_point_on_plane(point, normal, height, &Near) -> Result<bool, _>;
pub fn is_face_on_plane(face_normal, corners, normal, height, &Near) -> Result<bool, _>;
pub fn curve_lies_on_face(target, face, &Curve, Extent1d, tol, &mut Budget, &Stop) -> Result<Lies, _>;
                                    // Yes | OffSurface { by } | OutsideTrim | Undecided
pub fn prism_part(target, face, part, planar, section, &PrismCut, &Near, &mut Budget, &Stop)
    -> Result<PrismPart, _>;        // Dropped | ReachesPast(FarCap | EmptySection | SectionOff{edge}) | Kept | Undecided{edge}

// face_surface.rs (a view: kind, domain, point, normal, closest_point, recognized)
pub fn face_surface(&Brep, FaceId) -> Result<FaceSurface<'_>, _>;

// round_face.rs (the pin's recipe: the body is read through probes the caller answers)
pub fn past_end(seat, radius, bore, reach, &Extent, &Near, probe: &mut dyn FnMut(&Probe) -> Met) -> Result<End, _>;
pub fn stands_over(..), beside_clear(..), grow_block(..), far_end(..), into_seat(..), extent(..);
pub fn extent_in(&Brep, &SeatFrame, &mut Cx) -> Result<Extent, _>;     // a moved copy, boxed by tight_body_box
pub fn whole_band(&Brep, &SeatFrame, radius, &Near) -> Result<Band, _>; // Whole | Broken{face,edge} | Undecided{face,edge} | Absent
```

### 34.2 Method, and what the pin's constants became

- **No angle tolerance (01 §4.2), so the angle is coincidence over size.** The pin tested "opposite
  normals", "one axis", "parallel edge" and "square to a direction" within `ang_tol_rad` (half a
  degree, at any size) and "thickness" and "same place" within `zone_tol_3d`. Here every such test
  is [`Near`]: two unit directions make one line when `|a x b| * reach <= coincidence`, i.e. the lines
  stay within coincidence over the stretch the faces span. The reach is the caller's (the face's or
  edge's size); `mid_joint` takes each edge's own length. A half degree at one metre is 9 mm and
  would pair a wall with a skew of nine millimetres; at 100 mm and a coincidence of 1e-5 the limit is
  1e-7 rad. A face a millimetre across tolerates the half degree the pin used (test). The planes'
  actual skew over the reach is returned ([`FacingPlanes::skew`]).
- **Coincidence is `coincident(res, acc_a, acc_b)`**, from the faces' recorded accuracies, not the
  kernel's zone.
- **`Option` for "not a pair" becomes the reason.** `Facing::Apart(Apart::Tilted | SameWay | OffAxis |
  SameSide | Air | Thin)`, `Wrap::{NotParallel, NotTouching}`, `Meeting::{Parallel, Unplaceable}`,
  `Moved::{EdgeNotParallel, NeighbourParallel}`. The pin collapsed them to `None`.
- **`convex_at` is a proof.** Each face's unit normal into the material is its closed-form normal
  field (`along::Field`, BW4) enclosed in intervals at the edge's end, the direction from the edge into
  the face is that face's natural normal crossed with the edge's jet over the end parameter, turned by
  the use's sense (the loop runs counter-clockwise about the natural normal); convex iff the direction
  into face b has a positive part along face a's normal into the material. Both readings (b into a,
  a into b) are enclosed, and `Convex`/`Concave` is returned when a certified sign exists and they do
  not disagree. Faces whose normals into the material are proved within `smooth` are `Tangent`,
  the rest of the unproved `Undecided`. The pin's `None` meant tangent, undecided, closed edge and
  no material alike.
- **`joint_turn` and `turn_at`** use `atan2(|a x b|, a . b)` (good to rounding at 0 and pi alike); a
  direction of no length is refused at any scale (the pin's `angle_between` had an absolute zero:
  `[1e-200, 0]` was "no direction"; here it is a direction, a test).
- **`planes_meeting` places the line's point from the first plane's own point**, not from the planes'
  offsets from the origin: the same line, with no cancellation at a far corner (a test at 1e9).
- **`curve_lies_on_face`** (the pin's `edges_lie_on_face`, sampled): on the surface by
  `curve_surface_gap` (SU6, a bracket, `OffSurface { by }` is a lower bound); inside the trim by
  (a) lying within `tol` of one of the face's own edges (`curve_curve_gap`), else (b) the curve's uv
  image enclosed over pieces by `pcurve_of` and `Ivl`, each cell read by `Face::side_of_cell` (BW2):
  all `Inside` proves, an `Outside` cell refutes, a `Near` cell is halved up to 40 times, then
  `Undecided`. A NURBS target face is `Unsupported(Nurbs)` (`GapError::Pending`).
- **`round_face`** keeps the pin's design: bodyq is below `scx-boolean`, so the body is read through
  [`Probe`]s (a ring or disc about the axis over a stretch) that a caller with a Boolean answers with
  the box of the piece in the seat's frame. The readings are the pin's, with `clear_of(r) =
  1e-6 (1 + r)` as the `Near`'s coincidence, `same_size` as `max(1e-3 r, coincidence)`, and the
  structural ratios named (`RIDGE` 10 and `BESIDE` 1,000 coincidences, `SAME_SIZE`, `SLOPE_TIE`).
  An unread probe is `Unsupported(Probe)` (the pin read it as a shut end or as "not out", the
  failing default R2 forbids); `extent_in` moves a copy by `into_seat` and boxes it with `tight_body_box`.
  `whole_band` proves circles across the axis and lines along it; any other edge curve is `Broken`
  when two of its points differ in height and in place round the axis (a witness: a sample may
  refute) and `Undecided` otherwise. The pin read such an edge from 24 samples as proof.
- **`face_surface`** is a view: the pin built a record face's surface (labels proved on NURBS forms,
  blends, ball patches) because its body held records. A scx face holds its own `Surface`. What
  remains is the pairing of the surface with the face's uv box (the nearest point is searched in
  it), the kind, typed `normal` (`None` at a pole or apex), and `recognized` (SU7, the quadric a
  NURBS face is, with the face's box clipped to the surface's knots).

### 34.3 Cases for and against

- **`Near` (coincidence over reach), not a named constant (`PARALLEL_ANGLE_RAD`).** For: the row says
  constants from the resolution; the answer follows the faces' size; a thin wall on a small part is
  not paired by an angle meant for a large one. Against: every caller supplies a reach, and a
  too-large reach (the whole body) refuses to pair walls a drafted half degree off that the pin
  paired; the row's "drafted wall needs the bisector construction" is not built either way.
- **`Convexity` with five readings, not `Option<bool>`.** For: "tangent" and "no material" and
  "undecided" are three different facts to a chain walker. Against: a caller wanting the pin's
  `None` writes `matches!`.
- **Reading the body through probes (as the pin), not a Boolean in bodyq.** For: layers.txt (bodyq is
  below boolean); the tests drive the logic with a closed-form body of revolution. Against: the
  recipe's cost lives with the caller's Boolean and cannot be bounded here.
- **`whole_band` Undecided, not the pin's sampled yes.** For: R2. Against: a rim held as an
  ellipse-or-NURBS circle (a spline's boundary) is `Undecided` until step 5 gives a certified
  circle test for NURBS curves.
- **Cross-crate calls in hot loops.** `Point3::distance_along` and `Vec3::length` are not
  `#[inline]` in `scx-num`: a first `wall_holds` through `distance_along` ran four times the pin's
  per point (9.5 ns against 2.0) and was brought level by writing the dot product inline. Not
  changed in `scx-num`.

### 34.4 Tests

68 new tests in five files, closed-form truths (a plate 2 thick, a tube of radii 10 and 8, an L
bracket's areas 780 and 580, a box's dihedrals, an L-section's one concave edge):

- `tests/joint.rs` (9): the pin's corner turn (pi/2), line into tangent arc (turn 0, the wall's edge
  pi/2 from both), L-section inside corner concave and outside convex (both ends), the two
  `joint_turn` tests (plus 1e-200 and 1e200 scales, a 1e-9 turn exact to 1e-24, near pi), all 24
  ends of a box convex, a stadium's tangent wall edge `Tangent`, a notch's rim convex and a bulge's
  concave (cylinder normal sign), a sheet's `NoMaterial`, `leaving`'s refusals and a rim circle's
  `Closed`.
- `tests/wall.rs` (31): `face_pair` (plate, normal length, air/thin/same-way/tilted, the angle that
  pairs at three reaches, point-independence of the tilted thickness, hostile input, a block's top
  and bottom and sides read from the body), cylinders (tube, bend, seven non-pairs by reason, tilted
  axis point-independence, hostile, a cylinder face's side), `material_toward_axis`,
  `round_directions`, `axial_range_in`, `mid_sheet` (wall, tube, polygon areas incl. 2^27 from the
  origin, plane drop, frame round trip), `mid_joint` (perpendicular, angled, far origin, parallel,
  near parallel property over 2^-20..2^-60, L bracket areas, trim, each non-move and refusal),
  `cylinder_wall` (two pin cases and the three ways of not touching), the cap tests.
- `tests/cap.rs` (7): every edge of a cylinder on a face of it (circles on two, the seam on one),
  every block edge on exactly its two faces, another body's edges on none of a block's, a curve in a
  face's plane inside, beyond and across its trim, budget and stop, refusals and a NURBS target
  face, `prism_part` kept, dropped, far cap, off-face and empty section.
- `tests/round_face.rs` (18): the pin's 16 `teeth/tests/round_face.rs` readings against its body of
  revolution (free end, shoulder, neck, chamfer rim, carried chamfer, round vs chamfer, long taper,
  counterbore and entry chamfer, stands-over, beside-clear, map into the seat, far end, grow block,
  hostile numbers), `extent_in` of real shafts (square and askew), `whole_band` on real bodies.
- `tests/face_surface.rs` (3): block top, cylinder wall, a flat NURBS patch recognised as the plane.

**Not ported, with what each proved:**

| pin test | what it proved | here |
|---|---|---|
| `solidcore-ops` `face_pair`, `mid_sheet`, `cap` unit tests | the ops adapters convert arrays to the kernel's | the kernel tests above cover the values; there is no adapter |
| `a_band_is_whole_only_with_rims_and_seams`'s sampled-points cases (rim and seam "held as other curves", the helix, an edge "known by nothing") | a record edge's samples decide a band | R2: samples may refute only. Real bodies replace the records; the helix case is the `Broken` witness, the rest `Undecided` |
| `into_seat` of a plane with `x_axis == y_axis` (`Seat` refusal), `whole_band` on it | a frame that spans no plane is refused | a `Frame` is square by construction |
| `past_end(.., None, ..)` on an unread body | an unread extent shuts the end | not representable: the extent is typed, the caller handles its own `extent_in` error |
| the 10-hostile-value loops on `past_end` that assert a shut end | no panic, shut answer | no panic and a typed refusal; asserted per number |
| `face_surface` conversion tests (a label dropped when its numbers are off its form, a ball patch certified on its sphere, a torus label on a short tube arc, a plane "a little off square goes back square") | the record-to-kernel conversion | there is no conversion; SU7's tests hold `recognize` |
| `face_surface_eval` (blends, `circle_round_torus`) | a blend or round's definition certified | blends do not exist here |
| ops `cap::edges_lie_on_face` on a made cylinder via `to_record` | the section edge lies on a face | `cap.rs` tests on `scx-testkit` bodies |

### 34.5 Costs (release, mean of 200 to 10^6 calls, loaded server, pin in the same binary)

| call | pin | here |
|---|---|---|
| `facing_planes` | 23 ns | 42 ns |
| `facing_cylinders` | 17 ns | 28 ns |
| `planes_meeting` | 21 ns | 50 ns |
| `move_edge_onto_line` (4 corners) | 99 ns | 142 ns |
| `wall_holds`, 1,000 points | 2.0 us | 1.9 us |
| `polygon_area` (6 corners) | 15 ns | 16 ns |
| `joint_turn` | 12 ns | 27 ns |
| `turn_at` (a box corner) | 81 ns | 74 ns |
| `convex_at`, the 24 ends of a box | 782 us | 17 us |
| `curve_lies_on_face`, 12 edges x 6 faces of a box | 855 us | 2.4 to 3.0 ms |
| `extent_in` (shaft, moved copy and boxed) | the pin's copy, transform and box | 232 us |

The pure functions cost tens of nanoseconds more (the unit-vector helper divides by the largest
coordinate so a huge or tiny vector keeps its direction). `convex_at` is 46 times faster: it reads
one closed-form normal field, where the pin built the faces' use normals and a binormal through the
evaluator. `curve_lies_on_face` is 2.8 to 3.5 times slower per call (33 to 42 us against 12 us): it
proves rather than samples, and on a box face most calls are decided by the gap alone. Side-channel
timers in a throwaway crate; nothing is committed.

### 34.6 Deviations

- Every tolerance as `Near` (34.2). The row asked for constants from the resolution; an angle needs
  a size, so the API takes one.
- `turn_to_parallel` is not ported (BW9's map lists it).
- `Frame`/`in_frame`/`from_frame` use `scx_num::frame::Frame` (square by construction), not the pin's
  `Frame { origin, x, y }`.
- `MAX_POINTS`, `MAX_CORNERS` and `MOST_PROBES` are dropped: a linear pass over the caller's slice
  needs no cap, and the probe count is fixed by each reading's structure (at most six).
- `cylinder_wall`'s contact "within `10 tol`" is "within coincidence": the pin's callers passed their
  own tolerance and the factor was a hedge.
- `Probe` and `Met` carry `f64` millimetres, as geometry does (`units.rs`): lengths that are
  tolerances are `Length`.
- A change of one visibility word in BW4's `along.rs` (`Field::of`).

### 34.7 Open questions

1. Should `Near`'s reach be the face's own size by default (`Near::of_face`)? The caller now computes it.
2. A drafted wall (planes a degree or two off parallel) has no pairing (the pin had none either): the
   bisector construction is wanted by sheet-metal callers and is not in the row.
3. `curve_lies_on_face` leaves a curve that crosses a periodic face's uv seam `Undecided`
   (its cells meet the boundary there at every depth). A `Near` cell within `tol` of a face
   edge could count as boundary; not done.
4. `whole_band` for a NURBS-held circle needs a certified circle test on NURBS curves (step 5).
5. `Point3::distance_along` and `Vec3::length` in `scx-num` would gain from `#[inline]` for every
   caller loop (34.3); that is a change to a layer below, left to its owner.
6. `convex_at` for a NURBS face is `Unsupported(Nurbs)` with the other NURBS answers; it is
   reached only past the material check, so a NURBS sheet reads `NoMaterial`.

### 34.8 Proposed 04 row

| 11-28 | (For the owner's review) **BW11: vertex joints and thin-wall geometry in scx-bodyq.** `joint_turn`, `leaving`, `turn_at` and `convex_at` (a proof from BW4's normal fields: `Convex`/`Concave`/`Tangent`/`Undecided`/`NoMaterial`) at a vertex; `facing_planes`, `facing_cylinders`, `wall_holds`, `planes_meeting`, `move_edge_onto_line`, `wall_frame` and the cap tests as closed forms; `curve_lies_on_face` (certified gap plus the face's own boundary test, `Yes`/`OffSurface`/`OutsideTrim`/`Undecided`), `prism_part`; `face_surface` as a view of the face's own surface; the `round_face` family read through caller-answered probes, `whole_band` by proof or witness. No fixed angle: "parallel" is `|a x b| * reach <= coincidence` over a `Near` the caller builds from the faces' accuracies and size. Unread probes and unprovable edges are typed, not defaults. NURBS faces are `Unsupported(Nurbs)`. Edges: none | 74 §34 |
## 28. CF2a as built (11-27)

Task CF2a (§8): the certified gaps on analytic geometry, the wide vertex, the degenerate edge,
`Scope::Import` and `Scope::Stitched`, and the one-defect recipes at δ = 10·acc, acc and acc/10.
Branch `cf2a`. Crates `scx-check` (new `gaps` module, `findings` extended) and `scx-surface` (two
additions to SU6's `gap`, one bound fixed). New Cargo edges `scx-check → scx-surface` and
`scx-check → scx-curve` (§28.6).

### 28.1 What finish already refuses, and what the check adds

`finish` (`GeometryCause`) tests each edge's ends against its vertices, each use's ends against its
vertex, and each pcurve against its edge at 9 samples. A defect that holds along the edge (a vertex
moved by 10·acc, a pcurve offset by 10·acc) never reaches the check: finish refuses the body. The
recipe test `a_moved_vertex_is_clean_within_its_ball_and_refused_by_finish_past_it` pins that, and
shows the check clean at acc and acc/10. The check's new power is in two places:

- **The face gap.** finish never measures the edge's curve against its face's surface. An edge
  moved off its face with its pcurve and bounds kept consistent passes finish and is found here.
- **Between samples.** The check's answers are suprema over the whole parameter range, proved by
  branch and bound, where finish's are 9 points.

### 28.2 The allowances

These are finish's, from 09 §3: a vertex is a ball, an edge a tube, and distances add. The reach of
a recorded accuracy is `max(acc, zone)`; `rv`, `re` and `rb` are a vertex's, an edge's and a use's
reach.

| Question | Measured | Allowed | Finding |
|---|---|---|---|
| Edge on its face | the curve's largest distance from the kind's whole surface | `re` | `EdgeOffFace` |
| Use follows its edge | the pcurve's lift against the curve, at the same parameter | `rb` | `PcurveOffEdge` |
| Collapsed edge's use | the lift against the vertex | `rb + re + rv` | `PcurveOffEdge` |
| Edge ends at its vertex | the curve's end point against the vertex | `re + rv` | `VertexOffEdge` |
| Edge not degenerate | the curve's diameter | above `re` | `DegenerateEdge` |
| Not too wide (`Full` only) | a vertex or edge accuracy | below 1000 zones | `WideVertex`, `WideEdge` |

`Scope::Stitched` decides gaps exactly as `Import` does. The allowances are already sums of the
recorded accuracies, so a stitched body's wider tubes carry over with no extra rule; stitching's own
questions (clearance) are CF2b's. Under `Import` and `Stitched`, a wide accuracy is a recorded fact,
not a finding (09 §6; CTC-05 imports at ratio 2050). Under `Full` it is a defect of our own result.

Each answer is a `Bracket` against its allowance. Past the allowance it is a finding, within it the
answer is clean, and a straddle is `Undecided` with a `Why`: `Exhausted`, `Precision`,
`AtAllowance` or `Enclosure` (R2). A NURBS face is `Undecided::Pending`, and a value that is not
finite is `Undecided::NotFinite`.

### 28.3 Edge against face: no trim-restricted gap

The face's own gap (the curve against the trimmed face) is bounded below by the whole surface's
gap (`curve_surface_gap`) and above by the pcurve's (`pcurve_edge_gap`, because the lift lies in
the face). The edge is clean when either upper bound is within `re`. A finding needs the whole
surface's lower bound past `re`. Anything else is `Undecided::EdgeOffFace`. Nothing invents a
trim-restricted gap.

This answers §17's Open 2: CF2a is not the caller that needs one.

- For not building it. Every case where the two bounds disagree is one where the edge is near the
  surface but off its trim, or the pcurve is loose. In the first case the edge does lie within
  `re` of the surface, which is what 09 §3 tolerates. In the second case the loose pcurve is
  already a `PcurveOffEdge` question with its own allowance.
- For not building it. A literal trimmed gap would put every loose body, where `rb > re`, in a
  permanent Undecided band: the pcurve upper bound sits above `re`, and only the trimmed gap could
  close it. The whole-surface bound closes it at once, because a loose pcurve still lies in its
  surface.
- Against. A face whose edge runs along the surface but outside its trim, by more than `re`, is
  not caught. That is a face-region defect (the loop leaves its surface's domain), and it belongs
  with CF3's face checks or the contract's region tests, not with an edge gap.

The order saves work. When one use's pcurve upper bound is within `re`, the face's surface gap is
not computed at all; a seam's two uses share their face's answer. A use whose `rb` exceeds `re` is
searched to `re` first, and to `rb` only if that is undecided.

### 28.4 Degeneracy, collapsed edges and the budget

- **Degenerate.** The lower bound is the largest distance among 9 samples, from points 0 and 4
  to all the others; it exits early when that lower bound already exceeds `re`. The upper bound is
  the largest box-to-box far distance over 16, 32 and then 64 pieces. An upper bound within `re`
  is a finding. Anything else open is `Undecided` (`Enclosure`, or `Exhausted` when no box closes).
  The pin's horn-torus degenerate case (`issue_1515`) is not portable: our `Torus::new` refuses a
  horn torus at construction.
- **Collapsed edges** (poles, apexes): each use's lift is bracketed against the vertex by a local
  branch and bound, one unit per piece. It ends when the question is decided, when the width is
  within `NEAR` times the allowance, on `Precision` when a piece cannot split, or on stop.
- **Budget** (R6). The structure is charged first, one unit per use. Each edge, in id order, then
  gets `remaining / edges_left` as its own sub-budget and is charged back what it used. An edge
  whose share runs out lists what it reached with `Why::Exhausted`. A small budget may leave any
  edge undecided, but never gives a false finding
  (`a_budget_too_small_leaves_the_edge_off_its_face_undecided`).

### 28.5 SU6 changes

- **`Aim::Decide { allowance, width }`.** The search ends when its bracket is on one side of the
  allowance, or straddles it within `width`. CF2a asks at width `NEAR · allowance` (1e-3). SU6's
  `Aim::Against` hangs on a plateau at the allowance; `Decide` bounds that case. The docs now say
  so.
- **`Bracket::new(lower, upper) -> Option<Self>`**, for brackets built outside the search (the
  degenerate diameter and the collapsed lift).
- **The pcurve bound across a seam (a fix).** `PcurveField::over` fell back to the natural interval
  bound whenever the uv box met an arc break. The natural bound converges only linearly. A seam's
  `u` is enclosed a few ulps either side of its break, so every seam piece fell back. On testkit
  bodies the cylinder and cone seams, the sphere's equator and the torus circles each ran out of
  200,000 units, with uppers stuck at 1.1e-5 to 1.4e-4 against `re` = 1e-5. The fix cuts the box at
  the breaks, evaluates the jets on each part and takes their hull. That is valid pointwise: at
  every `t`, the lift lies on one part. The same seams now close in 5 units. This also lowers
  SU6's "about 8,350 units on a cylinder across its breaks".
- **Cached end gaps.** `PcurveField` keeps the point gap as the search mark, so a split no longer
  recomputes its ends' gaps (four `box_over`s per split). That makes curved bodies 1.6 to 1.9 times
  faster.

### 28.6 Deviations

- **Cargo edges.** The §8 row lists "—", but `check → surface` and `check → curve` are new. SU6's
  gaps live in `scx-surface` and take `scx_curve` types; the check cannot call them otherwise.
  Both crates sit above `scx-check` in `layers.txt`, so no layer moved. The alternative, routing
  through `scx-bodyq`, would add a crate between them for no logic.
- **The §3.3 finding shapes.**
  - `DegenerateEdge { edge, extent: Bracket, allowed }` replaces
    `Degenerate { entity, what: Degeneracy }`. Only edges are certified here. A degenerate face is
    CF3's, and an enum of one case would be speculative.
  - `WideVertex { vertex, accuracy, limit }` replaces `{ vertex, extent }`, and a matching
    `WideEdge` is added. The ratio limit applies to both (09 §6), and the accuracy against its
    limit is what the reader needs.
  - `PcurveOffEdge` names the `Fin`, since there is no `FinId`.
  - `VertexOffEdge` adds `allowed` and `at`, the end's parameter.
  - Every gap finding carries its `Bracket` and `allowed`, so a reader sees the margin.
- **`Findings`** derives `PartialEq` but not `Eq`, because brackets hold floats.

### 28.7 Tests

All test bodies come from scx-testkit, and every truth is a closed form. No test was removed.

- `crates/scx-check/tests/gaps.rs`:
  - `an_edge_off_its_face_is_found_past_its_reach_only`: the δ recipe, over seeds 0 to 2 and all
    three scopes.
    - At 10·acc there is exactly one `EdgeOffFace`, whose bracket holds δ.
    - At acc there is no finding; any undecided item is that edge and face, at the allowance.
    - At acc/10 the body is clean.
  - `an_edge_moved_a_hundredth_off_a_face_is_a_hundredth_off`: the pin's
    `offset_edge_curve_fails_test_13`. The bracket holds 0.01 and is narrower than 1e-3 zones.
  - `a_moved_vertex_is_clean_within_its_ball_and_refused_by_finish_past_it` (§28.1).
  - `valid_and_loose_bodies_are_clean`: every testkit kind, seeds 0 and 1, sizes 1 and 3, all
    scopes, plus loose variants at offsets 1e-9 and 1e-6. There are no gap findings and nothing
    undecided. The stack's `NotSolid` is CF1's structural finding.
  - `a_wide_vertex_is_found_under_full_only`: the pin's `issue_1419`, at ratios 999, 1000
    and 2050.
  - `a_budget_too_small_leaves_the_edge_off_its_face_undecided` (R6).
  - `a_set_stop_cancels`.
  - `the_same_body_gives_the_same_findings` (determinism, and the same `Spent`).
- `gaps` unit tests:
  - `a_circle_within_its_reach_is_degenerate`: radii from 0.2 to 0.45 re are found; 0.6 re and up
    are clean in 9 units; 0.5 re gives no finding; a 10-unit budget gives `Exhausted`.
  - `a_pole_lift_is_bracketed_against_its_vertex`: a sphere's pole, with point and line lifts, at
    d = 0, 1e-4 and 1e-6.
- `scx-surface` `gap` tests:
  - `deciding_at_the_allowance_ends_within_the_width`
  - `a_bracket_is_built_only_in_order`
  - `a_seams_pcurve_is_decided_at_once`: generators at 0, π/2 and π on a world and a tilted
    cylinder, each in under 64 units.

### 28.8 Measurements

The timer harness was not committed. Each figure is the release mean of 20 checks under
`Scope::Full`. Bodies were finished at the default resolution (zone 1e-5). "Before" is the merge
base, where `check` ran the structure only.

| body | edges | before (us) | after (us) | after per edge (us) | work units |
|---|---|---|---|---|---|
| block 1 | 12 | 1.1 | 172 | 14 | 276 |
| block 4 | 192 | 13.6 | 2,460 | 12.8 | 4,416 |
| cavity 4 | 384 | 28.2 | 4,608 | 12.0 | 8,832 |
| checker 4 | 140 | 12.0 | 1,931 | 13.8 | 3,412 |
| loose block 4 (1e-6) | 192 | 14.7 | 2,294 | 12.0 | 4,416 |
| cylinder 1 | 3 | 0.3 | 5,343 | 1,781 | 1,109 |
| cone 1 | 3 | 0.2 | 3,564 | 1,188 | 568 |
| sphere 1 | 3 | 0.1 | 10,599 | 3,533 | 287 |
| sphere 4 | 12 | 0.3 | 42,253 | 3,521 | 1,148 |
| torus 1 | 2 | 0.2 | 64,023 | 32,011 | 1,566 |
| torus 4 | 32 | 2.2 | 218,376 | 6,824 | 5,856 |

Planar edges cost about 12 us each: two pcurves at 5 units, two ends and the diameter. Curved edges
cost 0.3 to 32 ms. That cost is mostly full-turn circles, whose pcurve bound shrinks with the
piece's width cubed, so they need 250 to 500 points to bring the upper bound under `re`. On a
sphere or torus each point costs about 40 us of interval jets, which is why the cost per unit is
high there. A cone's generator against its whole surface costs about 158,000 units
(375 ms) when it is asked. The check skips it whenever a pcurve settles the face, which it does on
every valid body here.

No existing path is slower. Operations do not call `check`, and the two SU6 changes only make the
pcurve search cheaper. The pin's Level 2 against the Boolean is CO3's comparison (§10); it was not
measured here.

### 28.9 Open

1. **Curved cost.** A torus at 64 ms for two edges is far above the planar 12 us per edge. Two
   remedies, both certified and neither built: a Taylor-model or mean-value jet for the full-turn
   circle pcurve, which would turn the cubic convergence into a constant on coaxial cases as the
   curve-surface series already do; and a cheaper sphere and torus jet (one arc formula, not four
   parts). CO3 should measure against the Boolean before CF2a goes ready; this is the §10 risk.
2. **The cone generator against its surface** converges slowly at the apex (158,000 units). It
   matters only for an edge whose pcurves do not settle its face. A closed form for the line
   against the cone (the generator's own distance) would fix it.
3. **Face-region defects** (§28.3, "against"): an edge off its face's trim but on its surface is
   for CF3 or the contract, not this check.
4. **Degenerate faces** are not certified here (§28.6).

**Proposed 04 row.** `scx_check::check` certifies each edge against finish's allowances: the face
(whole-surface lower bound, pcurve upper bound, no trimmed gap), the uses, the vertices, the
diameter, and the ratio limit under `Full`. `Stitched` decides gaps as `Import` does. Each answer is
a `Bracket` against its allowance, or a typed `Undecided { why }` (R2). The edges share the budget
in id order (R6). NURBS faces are `Pending`.

## 32. ST2 as built (11-28)

The edge stage of §4.3 (steps 2 and 3) in `scx-stitch`, new module `src/edges.rs`. A vertex
within coincidence of another sheet's edge, strictly inside it, cuts that edge. Then two edges of
different sheets with the same ends are one edge where the certified symmetric Hausdorff distance
of their curves is within coincidence. Every distance is an SU6 `Bracket` (`point_curve_gap`,
and `curve_curve_gap` called once each way, since it is one-sided). No new distance code. A pair
is proved within, proved beyond, or left open and named; nothing is sampled or guessed. ST1's
`Unsupported(Edges)` placeholders and its slab gate are gone. `Want::Solid` stays
`Unsupported(Solid)` (ST3). 22 tests: 17 in `tests/stitch.rs` (9 kept from ST1, 8 new, 2 of
ST1's removed) and ST1's 5 unit tests in `points.rs`.

Cargo: stitch → bodyq is dropped (ST2 needs no bodyq query: the certified edge box is replaced
by the loose box plus the certified gaps). Stitch now names algo, curve, nurbs and surface
directly. Each was already below stitch through bodyq (§4.3 names stitch → surface), so the
layer graph gains no edge. ST3 brings bodyq back for `classify_point`.

### 32.1 Public API (changes from §26.1)

```rust
#[non_exhaustive] pub struct StitchReport { pub identified: Vec<Identified>,
    pub split: Vec<Split>, pub joined: Vec<Joined>, pub left: Vec<Left> }   // + undecided()
#[non_exhaustive] pub struct Split { pub edge: EdgeId, pub vertex: VertexId,
                                     pub from: (Operand, EdgeId), pub gap: Length }
#[non_exhaustive] pub struct Joined { pub edge: EdgeId, pub gap: Length }  // certified upper
#[non_exhaustive] pub enum Left {
    Ambiguous { edge: EdgeId, partners: Vec<EdgeId> },
    Undecided { edge: EdgeId, with: Entity, lower: Length, upper: Length, allowance: Length },
}
pub enum StitchUnsupported { Chain { .. }, Edges { operand, edge, why: EdgePending }, Solid, TooLarge }
#[non_exhaustive] pub enum EdgePending { NurbsFoot, SameFoot, Radial, Fit, Direction, Wire,
                                         NearEnd, Region }               // name()
pub enum StitchDefect { Draft, Query { operand, at }, Pcurve { .. }, Finish(Box<FinishCause>) }
```

`StitchUnsupported::Query` is removed, with its bodyq cause; it had no caller left.
`Edges` now carries an `EdgePending`: the part a proved cut or join waits on.

### 32.2 The edge stage

1. **Gather.** Every sheet edge with a curve is taken, with its finished `Edge::bounding_box`
   (the loose box: the curve's box grown by the edge's accuracy and the resolution). Reach is the
   edge's accuracy, at least the zone. Each draft vertex is a node: its point, its reach and the
   sheets it stands for (more than one once identified).
2. **Cuts.** One `scx_algo::BoxTree` holds the edge boxes. Each node queries it, grown by its
   own reach. The node's own edges, and edges of a sheet it stands for, are skipped. Each other
   candidate gets `point_curve_gap(.., Aim::Against(max reach))` on the edge's budget share.
   - Within: the foot is found by inversion. It must lie strictly inside the interval, or the
     result is `Edges(NearEnd)`. A NURBS edge's foot waits on step 5's solve (`NurbsFoot`), and
     two feet at one parameter give `SameFoot`.
   - Open: `Left::Undecided { with: Vertex }`.
   - Beyond: nothing.

   One pass is enough. A cut point is an existing vertex, so a cut makes no new point to cut at
   (the pin loops up to 10 times).
3. **Cut.** The first piece keeps the edge's id. The others are `add_edge` plus
   `derive(Origin::Edge)`. Each face use is expanded in loop order, reversed for a `Reversed`
   use, and its pcurve's interval is set to the piece's span. The cut vertex is raised, `Measured`,
   to the farthest the cut moves the curve or a use's surface point from it, as ST1's identify
   does (§26.3). Each cut is a `Split` entry.
4. **Candidates.** Pieces are grouped by their unordered end pair. Each pair from different
   sheets whose boxes are not disjoint (grown by the larger reach) is a candidate. One unit is
   charged per candidate.
5. **Proof.** Each lead piece gets a budget share. Both directed gaps are taken, and the second
   is skipped when the first is beyond. The symmetric bracket is the larger of each bound. For
   two segments, the four end-to-segment `point_curve_gap`s give the whole symmetric gap: a
   point's distance is convex along a segment, so each directed gap's maximum is at an end. This
   is the pin's "0 for two lines" without its assumption.
6. **Acceptance.** A piece's possible partners are those proved within or left open. A piece
   with two or more possible partners is `Left::Ambiguous`, and none of its pairs is joined. An
   open pair is `Left::Undecided { with: Edge }`. A within pair whose two pieces are each alone
   goes on a `BinaryHeap` keyed by (bound bits, ids), and the heap is popped in order.
7. **Glue.** The kept piece is chosen by geometry (smallest reach, then lowest curve midpoint,
   then sheet, id and place), as §26.2's kept vertex is, so permuted sheets keep one curve. Each
   absorbed fin's use is pointed at the kept edge, with its sense flipped when the edges run
   opposite ways (from the ends, or from the start tangents for a closed edge, else `Direction`).
   Then `merge_edge` and `write_pcurves` run. A use with no exact pcurve of the kept curve gives
   `Fit` (BR9's fitter). Other cases waiting on later work:
   - a wire edge gives `Wire`;
   - more than two fins gives `Radial`;
   - faces bounding different regions give `Region` (ST3).
8. **Shells.** `finish` requires a joined edge's two fins to name the same shell
   (`SidesDisagree`). So every joined pair unites its two shells (union-find, root the least
   id), and each face's sides, wires and lone vertices are moved to the root before `finish`.

### 32.3 Choices, with cases for and against

| Choice | For | Against |
|---|---|---|
| Keep `Stitched { done: Done<NoItems>, report }`, not `Done<C>` (ST1 open question 1) | An edge left open is not a skipped item. The body is whole and valid, and the open edge is a fact of it (a lamina edge), as an `Undecided` volume sign is a fact. `Skipped<C>` would mean "input not processed", which is false here, and `NoItems` keeps "a stitch skips nothing" true. `Left` sits beside `identified`/`split`/`joined`, where a caller reads the stitch | Two places to look (`done` and `report`). A caller wanting "everything not done" has to read `left`, not a generic skip list. If ST5's `sew` later skips whole edges by request, `C` would come back |
| Ambiguity is decided per piece, before the heap: two possible partners means none is joined | §4.3 step 2: "two candidates within the allowance stays open". Taking the nearest would be a guess between coincident partners. With this rule, the heap only fixes edit order | A heap that consumed its pairs could join the nearest of two well-separated partners. That is more joins, but it is the guess the row forbids |
| One cut pass | Cut points are existing vertices, so no cut creates a point to cut at | Two vertices at one foot give `SameFoot` rather than an attempt at a merge |
| Cuts take half the remaining budget, spread over the edges with candidates; joins share what is left | Each edge gets a fixed share (§4.3), so one hard plateau cannot starve the rest. A pair left at its share is `Undecided`, typed | The halving is a choice, not derived. A join-heavy input with few cuts wastes nothing (the share is what is left), but a cut-heavy input could leave joins less |
| No edge accuracy raise on a join | The pair is accepted only where the certified gap is within the larger reach, so the kept edge already covers the absorbed curve within its accuracy plus resolution | A gap between the two reaches leaves the kept edge's accuracy as the larger only if the kept is the larger reach. The kept edge is the smaller reach, so the kept accuracy may not cover the absorbed curve. `finish` passed every case here; a measured raise like the vertices' is open question 2 |
| Loose box, then certified gaps | The gaps decide; the box only finds candidates. bodyq's certified tight box would cost an interval search per edge for no change in result | Loose boxes of long curved edges give more candidates |
| Two segments via four point gaps | Exact by convexity, and 2.3x faster on the grid (below) | One more path to test: `the_gaps_hold_the_pins_bound_truths` checks it against `curve_curve_gap` on skew segments |
| Shells merged on a join | `finish` requires it; joined sheets are one shell | Every face of the merged sheets is touched, so `finish` checks them all |

### 32.4 The pin's `bounds.rs` and stitch, mapped

| solidcore at the pin (`heal/stitch`) | here |
|---|---|
| `bounds.rs::curves`: both directed gaps under an endpoint correspondence (64 cells of a centred mean-value form) | two `curve_curve_gap` calls, each one-sided with no assumed correspondence, aimed `Against` the coincidence; segments by four point gaps |
| `bounds.rs::curve_to_point`, `point_at_curve` | `point_curve_gap` for a vertex against an edge's inside |
| p2 split pass: an edge box tree, vertex boxes grown by the tolerance, up to 10 loops | one `BoxTree`, one pass |
| p2 `glue_edges_at_vertex`: greedy per vertex, a 5-sample distance (0 for two lines) | candidates by identified ends and box overlap, accepted from a heap in (bound, ids) order; two possible partners left open |
| `MAX_PIECES`-style caps | a budget share per edge, `Left::Undecided` at it |
| tolerance doubling from 1/100 (`stitch_into_shell`) | none: one coincidence, the document's |

The pin's bound truths are ported against their closed forms in
`the_gaps_hold_the_pins_bound_truths`:

- a degree-one hat of height 1 between samples gives a symmetric gap in [1, 1 + 1e-9];
- a rational arc against the x axis gives 2/3, within 1e-9;
- a reversed sub-range gives 0;
- skew segments: the shortcut agrees with `curve_curve_gap`.

The pin bounds a correspondence `t -> t`; the Hausdorff gap needs none, and is never larger.

### 32.5 Tests

`tests/stitch.rs` (17). Kept from ST1 (9): corner touch, coincidence limit, measured raise, the
two refusals, solid pending, none/one sheet, determinism and permutation, cancel.

New (8):

- **a_shared_edge_is_joined**: two unit sheets on a seam. 6 V, 7 E, 2 faces; one join with a
  gap of at most 1e-12, kept on sheet 0's edge (equal reach and midpoint, the lower sheet), with
  two fins, one merge record onto it and a `Coincident` warning.
- **edges_are_joined_within_coincidence_and_not_beyond**: offset 0.5 zone joins; 2 zone stays 8
  edges with nothing left.
- **a_vertex_on_another_sheets_edge_cuts_it_and_the_overlap_is_joined**: a half-offset pair.
  8 V, 9 E, 2 splits, 1 join over [0.5, 1] with two fins, 5 created edges each with an origin,
  no raise (the cuts are exact).
- **an_edge_with_two_possible_partners_is_left_open**: three sheets, two of them on top of each
  other. 3 `Ambiguous`, each with 2 partners, none joined; the doubled pair's 3 other edges join.
- **a_join_no_share_decides_is_left_undecided**: an arc against a coaxial arc 0.9 zone out (a
  plateau). Budget limits from 40 to 200,000 give both `Undecided { with: Edge }` and a join,
  and `Exhausted` otherwise; never a wrong answer.
- **arcs_run_opposite_ways_are_joined_within_coincidence**: half disc against an arc of radius
  1 + 0.5 zone run the other way. One join, gap in [0.5 zone, zone], 5 edges; at 2 zone, none.
- **joins_and_cuts_do_not_follow_the_order**: three permutations give the same points, edge
  midpoints, join and split counts.
- **the_gaps_hold_the_pins_bound_truths**: see §32.4.

Removed on purpose (each proved a placeholder that ST2 replaces):

- `a_shared_edge_waits_for_the_edge_stage`: a shared edge gave `Unsupported(Edges)` named
  `edges_pending`. Now `a_shared_edge_is_joined`.
- `a_vertex_on_another_sheets_edge_waits_for_the_edge_stage`: a T-junction gave `Edges`. Now
  the cut-and-join test.

### 32.6 Measurements

Release build, best of 5, second build machine (load average 8 to 12). Before (main
22c15d8) and after were run back to back by the same throwaway timer, not committed. Work is
`Report::work`. Before, the seam, cut and grid cases are not a stitch at all: main refuses them
with `edges_pending`, so the "before" time there is the time to refuse.

| Case | V | E | Before work | Before time | After work | After time | Joins / splits |
|---|---|---|---|---|---|---|---|
| two 1x1, corner | 8 | 8 | 50 | 0.007 ms | 42 | 0.008 ms | 0 / 0 |
| two 10x10, corner | 242 | 440 | 6,287 | 0.40 ms | 1,887 | 0.51 ms | 0 / 0 |
| two 30x30, corner | 1,922 | 3,720 | 127,447 | 3.8 ms | 15,847 | 4.9 ms | 0 / 0 |
| two 60x60, corner | 7,442 | 14,640 | 940,687 | 17.4 ms | 62,287 | 23.2 ms | 0 / 0 |
| checker 4x4 of 4x4 | 200 | 320 | 5,117 | 0.40 ms | 1,597 | 0.50 ms | 0 / 0 |
| checker 10x10 | 1,250 | 2,000 | 74,399 | 3.0 ms | 10,399 | 3.5 ms | 0 / 0 |
| checker 20x20 | 5,000 | 8,000 | 577,949 | 13.2 ms | 41,949 | 16.6 ms | 0 / 0 |
| two 1x1, seam | 8 | 8 | refused | 0.001 ms | 68 | 0.028 ms | 1 / 0 |
| two 10x10, seam | 242 | 440 | refused | 0.08 ms | 2,183 | 0.79 ms | 10 / 0 |
| two 30x30, seam | 1,922 | 3,720 | refused | 1.1 ms | 16,743 | 6.0 ms | 30 / 0 |
| two 60x60, seam | 7,442 | 14,640 | refused | 5.1 ms | 64,083 | 27.4 ms | 60 / 0 |
| two 10x10, half-offset seam | 242 | 440 | refused | 0.10 ms | 2,401 | 1.1 ms | 19 / 20 |
| two 60x60, half-offset seam | 7,442 | 14,640 | refused | 5.5 ms | 65,501 | 27.9 ms | 119 / 120 |
| grid 4x4 of 4x4 sheets | 400 | 640 | refused | 0.16 ms | 6,010 | 3.0 ms | 96 / 0 |
| grid 10x10 | 2,500 | 4,000 | refused | 1.4 ms | 41,338 | 22.7 ms | 720 / 0 |
| grid 20x20 | 10,000 | 16,000 | refused | 6.5 ms | 170,138 | 99.9 ms | 3,040 / 0 |

- **The V^1.5 charged work is gone.** Work per vertex on the corner cases is 5.3, 7.8, 8.2 and
  8.4 (before: 6.3, 26, 66, 126). The box tree charges one unit per real candidate.
- **No-join stitches are 17 to 33% slower** (17.4 to 23.2 ms on the 60x60 corner). Under
  callgrind, the edge stage is 30% of the stitch there:
  - building the box tree over 14,640 boxes: 29% of the stage;
  - 7,442 queries: 25%, part of it a `Vec` allocated per query;
  - the loose boxes: 6%.

  ST1's slab gate found candidates and refused them; the tree finds them and the gaps decide.
  It is not the same job, so this is a cost of the stage, not a regression of one.
- **Joins.** Before the segment shortcut, `curve_curve_gap` was about 0.1 ms a call (40% of the
  seam's edge stage), and the 20x20 grid took 230 ms. With four point gaps per segment pair it
  takes 99.9 ms, about 33 us per join, mostly the glue edits and `finish` over the touched,
  merged shells.
- **The pin.** It has no equivalent timed path. `stitch_into_shell` works on one body's faces,
  with tolerance doubling from 1/100, through the pin's own construction. A like-for-like run
  needs the same sheets built in the pin, which was not done here.

### 32.7 Deviations and open questions

Deviations from §4.3 and the ST2 row:

- `Stitched` is kept, not `Done<StitchReport>` (§32.3).
- The cut is at the existing vertex, the foot only placing it (step 3's "certified foot point" is
  where the vertex projects; the vertex keeps its own point and is raised to cover the move).
- One pass, not "loop until no split".
- Stitch → bodyq is dropped, with four direct edges to crates already below it.
- `StitchUnsupported::Query` is removed.
- Two `EdgePending` causes are added: `NearEnd` and `Region`.

Open questions:

1. **NearEnd.** A vertex within coincidence of an edge whose foot is at or past an end it is not
   identified with means the two sheets' ends are within coincidence where their vertices are
   not. Should that reopen identification (a chain through an edge), or stay pending? It is
   `Unsupported(Edges(NearEnd))` now.
2. **Join raise.** The kept edge's accuracy is not raised to the certified gap. Should a join
   raise the kept edge, `Measured`, as identification raises vertices (row 186: "every new
   vertex and edge end records its measured gap")?
3. **ST1 open question 3 (chains).** Still open. `points.rs` decides chains before the edge
   stage, so a chain end to end still gives `Unsupported(Chain)`, not `Edges`.
4. **Cut-stage cost.** Should scx-algo's `BoxTree` get a query that fills a caller's buffer, or a
   dual-tree `pairs` against a node tree? Either would trim the 17 to 33% the stage adds to
   no-join stitches. Neither is in this branch (no scx-algo change).
5. **NURBS.** NURBS joins already go through the gaps. Only the cut foot on a NURBS edge
   waits on step 5 (`NurbsFoot`), and `Fit` waits on BR9.

### 32.8 Proposed 04 row

| 11-28 | (For the owner's review) The stitch's edge stage: a vertex within coincidence of another sheet's edge, strictly inside it (by SU6's certified `point_curve_gap`), cuts the edge there, in one pass; the vertex is raised, `Measured`, to the farthest the cut moves the edge. Two edges of different sheets with the same ends are one edge where the certified symmetric Hausdorff distance (two one-sided `curve_curve_gap`s; four point gaps for two segments, exact by convexity) is within coincidence. An edge with two or more possible partners is `Left::Ambiguous` and none is joined; a pair or touch its budget share leaves open is `Left::Undecided` with its bracket. Within pairs are accepted from a heap in (bound, ids) order, and the kept curve is chosen by geometry. Joined sheets are one shell. A proved cut or join the stitch cannot make yet is `Unsupported(Edges { why: EdgePending })`. `Stitched { done: Done<NoItems>, report }` is kept: an open edge is a fact of the result, not a skipped item. Edges: stitch → bodyq dropped (ST3 brings it back); stitch → algo, curve, nurbs, surface, each already below it | Amends §4.3 step 3 (one pass, cut at the existing vertex) and closes ST1's open question 1 (`Stitched` kept) | 74 §32 |

## 31. PL6 as built (11-27)

`scx_planar::crossings(&Curve2, &Curve2, Resolution, &mut Budget, &Stop) -> Result<Crossings,
CrossError>`, in `crates/scx-planar/src/cross.rs`. No new Cargo edge: scx-planar keeps scx-algo and
scx-num (scx-testkit as a dev-dependency); `binomial` comes from `scx_algo::bernstein`.

### 31.1 Public API

- `Curve2` (`#[non_exhaustive]`): `Line { start, end }`, `Conic { start, apex, end, weight }` (a
  rational quadratic Bézier, ends of weight 1, any positive middle weight: ellipse, parabola or
  hyperbola arc), `Bezier { points, weights }` (degree 3 and up; always `Pending`).
- `Crossings { meets: Vec<Meet>, overlaps: Vec<Overlap> }`, sorted by `a`'s parameter.
- `Meet { a: [f64; 2], b: [f64; 2], at: Point2, multiplicity: u32, exact: bool }`: each curve's
  parameter bracket, a point, and the multiplicity of contact. `exact` means the multiplicity is
  certified (a simple crossing, or an exact zero of known order); otherwise it is the certified upper
  bound of a cluster within the resolution. `crosses()` is odd multiplicity.
- `Overlap { a, b }`: a shared stretch; `b` runs high to low when the curves run opposite ways.
- `CrossError`, with `kind()`, `name()`, `Display`, `From<Exhausted>`, `From<Stopped>`:
  `NotFinite { curve }`, `OutOfRange { curve }` (a coordinate or weight outside `[1e-100, 1e9]`
  in magnitude, other than a coordinate 0, where the exact predicates would lose exactness),
  `Degenerate { curve }` (a line whose ends are one point, a weight not above 0, a Bézier whose
  points and weights differ in number) are `Refused`; a conic whose apex is on its chord is
  accepted as a flat conic;
  `Pending(Pending::Bezier { degree } | Pending::FlatPair)` and `Undecided { curve, range }` are
  `Unsupported`; `Exhausted` and `Stopped` keep scx-num's kinds. `Which { A, B }` names the curve.

### 31.2 Method

One curve is written implicitly and composed with the other's rational Bernstein form, so the
crossings are the roots on [0, 1] of one Bernstein polynomial `F` of degree at most 4:

- a line by its orientation `F = w_j orient(B0, B1, P_j)`, its bounded range by the two dot
  products `(P - B0)·d >= 0` and `(B1 - P)·d >= 0`;
- a conic arc by its barycentric equation over its control triangle, `A1^2 - 4 w^2 A0 A2`, with
  `A_i` the orientation against the side opposite `q_i`; its bounded range is `A0, A2 >= 0` on
  the triangle's side (the arc is the part of the conic inside the triangle).

Which curve is implicit is canonical (line before conic before flat conic, then by the bits of the
input), so `crossings(b, a)` is `crossings(a, b)` with the brackets swapped, bit for bit. Both curves
are first scaled by one exact power of two so the largest coordinate is near 1.

Every coefficient is an interval rounded outward with, where it is one orientation or dot product of
input points, its exact sign (scx-planar's exact predicates). Signs then propagate through the sums
and products where they are implied, and a sum or product of two exact values that the rounding does
not change stays an exact point (two_sum / two_prod error 0), so a zero at a dyadic parameter (the
midpoint of a symmetric arc, the crossing of two lines at 1/2) is an exact zero, found exactly.

A range is dropped when it is certified off the implicit curve's bounded range or `F` is certified
one-signed. Its roots are settled when every coefficient between the exact zeros at its ends has a
certified sign and the signs change at most once (Descartes on the Bernstein form): the end zeros are
meets of the exact multiplicity the zero coefficients give, and one change is one simple root, refined
by halving to the resolution and 12 halvings more to settle whether it is inside the implicit range.
A range whose image is within the resolution is a leaf. A run of adjacent leaves is one meet; its
multiplicity is bounded by the least derivative of the root `F` that `order_on` certifies one-signed
over the run (Rolle), and fixed by parity when both end signs are known. A range where no derivative
is certified and every point is proved within the resolution of the implicit support (and that is
inside the bounded range, or a leaf) is an `Overlap`. Anything else is halved, to depth 100; past it,
`Undecided`.

### 31.3 Choices, with cases for and against

- **Implicitisation, not a 2-D Newton or a subdivision of both curves.** For: one univariate
  polynomial of degree at most 4 per pair, so completeness is Descartes plus Rolle, and the bounded
  range is a sign, never a parameter compared with a tolerance. Against: a degree-3 curve would give
  `F` of degree 6 against a conic and needs the cubic's implicit form for the other way round; that is
  step 5's widening (31.6).
- **Exact signs on coefficients.** For: a shared joint, a point exactly on the other curve, and an
  exact tangency at a joint are exact zeros of exact multiplicity, never rounding accidents; the pin's
  `disc < 0` touch-at-the-foot fallback has no counterpart. Against: two extra exact predicates per
  control point (`dot_sign`, new in `cert.rs`), a few ns each in the fast path.
- **Exact point sums and products in the crossings only.** For: zeros that are exact in doubles are
  found exactly (`a: [0.5, 0.5]`) instead of as a bracket of 1-2 ulps. `Coef` and the crossings'
  own differences, cross and dot products (`exact_diff`, `exact_cross`, `exact_dot` in `cross.rs`)
  return a point only when the error-free transform's error is 0; products near underflow, below
  2^-900, keep the outward rounding. Against: one two_sum or two_prod per such operation. `Iv` itself
  keeps its outward rounding, so winding, region and contact keep their arithmetic and bits: putting
  the exactness in `Iv` was measured (31.6) and slowed some region cases by 5-6%.
- **The truth of the input, not of its intent.** With exact signs a near-tangency the rounding of
  `cos` put 1e-17 off is certified apart and dropped, as the contract says (a miss is certified, a
  touch is reported). For: deterministic, and truth over pin. Against: a caller that rounded a
  tangent construction gets 0 or 2 by the rounding's side; HLR must not rely on a tangency it did not
  build exactly (it gets a double meet only when the curves are proved within the resolution).
- **Clusters at the resolution, not exact algebraic multiplicity.** For: bounded work, the resolution
  is the model's own (`Resolution`), and the bound is certified (Rolle). Against: a cluster whose end
  signs are not both known reports the bound with `exact: false`.

### 31.4 The pin, and which side is correct

| solidcore at the pin | here | correct |
|---|---|---|
| `api_queries::segment_2d_intersection`: the first hit of two segments within a tolerance | every meet and the overlap | here: a second crossing or an overlap is not dropped |
| `geom_utility::p2::segment_segment_intersect`: ends on the other within a tolerance, then one closest point, clamped | ends are exact zeros; the interior crossing is certified | here: a clamped closest point of two segments that miss is not a meet |
| `prism::profile::crossing::line_circle`: `disc < 0` is a touch at the foot, however far the miss | a miss is certified apart and dropped; a touch within the resolution is a double meet | here: the pin's fallback reports a touch for any miss, left to a later tolerance filter |
| `crossing::circle_circle`: `h2 < 0` is a touch on the line of centres | same as above | here |
| `crossing::line_line`: parallel when the cross product is within `1e-14` of the lengths' product, then "on one line" within `tol` | parallel only when the exact orientation says so; a near-parallel crossing (1e-9) is found; collinear is an `Overlap` with both brackets | here: the pin drops a real crossing of nearly parallel lines |
| `crossing::on_arc`: within the sweep by `atan2` with slack `tol / r` | inside the control triangle by exact orientations, plus 12 halvings | here: no slack, no `atan2` (libm, not deterministic across platforms) |
| `geom_utility::p1::circle_coplanar_circle_intersect_in_contact`: tolerance and contact-tolerance bands (#15429) | no tolerance but the resolution; coincident circles are an `Overlap` | here for crossings; the contact bands are a modelling policy that belongs to the caller |
| `hlr` crossings: chords sampled, tolerance `1e-9` times the scale | the exact curves, no sampling | here: sampling misses a crossing between samples |

### 31.5 Tests (`crates/scx-planar/tests/crossings.rs`, 18; `cert.rs` +2)

Truth is closed form. Circle-circle (two crossings at the closed-form points, multiplicity 1,
exact); a line tangent to an ellipse quarter at its exact end (multiplicity 2, exact, at (0, 1)),
and the same line against the [pi/4, 3pi/4] piece from rounded cosines (0 or 2, by the rounding's
side); an exact interior tangency (a symmetric arc's middle at a dyadic point: `a == [0.5, 0.5]`,
exact); near tangency at 1e-9 below (two simple crossings at +-8.9e-5), 1e-9 above (none), 1e-15
below (total multiplicity 2 at (0, 1)); an osculating circle (contact 4); an arc against itself, a
split arc against its halves and reversed (overlaps with the right brackets); consecutive arcs of one
circle (only at the joint); line-circle; lines crossing at exactly 1/2, collinear (one overlap),
parallel 1e-9 apart (nothing), nearly parallel (one crossing); corner and tangent joints (one meet,
the tangent one exact of multiplicity 2); parabola and hyperbola arcs; a flat conic; Bezier pending;
the refusals; budget and stop; symmetry and determinism (swapped calls agree bit for bit). In
`cert.rs`: `dot_sign` is exact at a right angle; `exact_sum` and `exact_product` give points only
when exact, and `Iv` operations still widen. No test removed.

### 31.6 Cost

Release build, one thread, the build server under other load; throwaway timers (20,000 calls per
pair, not committed), resolution 1e-6. Units are budget units charged (one per range examined).

| pair | ns per call | results | units |
|---|---|---|---|
| line x line, crossing | 1,113 | 1 meet | 2 |
| line x line, parallel 1e-9 apart | 455 | none | 1 |
| line x line, collinear | 1,457 | 1 overlap | 3 |
| line x arc, crossing | 12,563 | 1 meet | 22 |
| arc x arc, crossing | 16,149 | 1 meet | 22 |
| arc x arc, boxes apart | 1,359 | none | 1 |
| arc x line, exact tangency at the end | 741 | 1 meet (m = 2) | 1 |
| arc x line, 1e-15 near tangency | 24,376 | 2 meets | 43 |
| arc x itself | 9,627 | 1 overlap | 7 |

The pin's closed forms (`line_circle`, `circle_circle`) are a few tens of ns but are not complete
(31.4). Most of a crossing's cost is the refinement of a simple root to the resolution and 12
halvings more (about 20 ranges, each a de Casteljau split of `F`, the range polynomials and the
homogeneous coordinates); a root-polishing step (Newton on `F` inside the certified bracket) would cut
that, and is left for when HL2 measures it in a drawing.

**Exactness in `Iv`, measured and not kept.** A first version made `Iv` add, sub and mul return a
point when exact, for every user in scx-planar. A throwaway timer over `Region::new` and `classify`
(400 grid points) on six regions, release, one thread, three interleaved runs each, gave (ns; `new`
per call, `classify` per point; before is plain outward rounding):

| region | new, before | new, exact `Iv` | classify, before | classify, exact `Iv` |
|---|---|---|---|---|
| rect | 2,118-2,168 | 2,099-2,183 | 445-450 | 444-464 |
| rect, odd coordinates | 2,170-2,212 | 2,269-2,363 | 465-469 | 496-501 |
| rect with 2 holes | 7,494-7,589 | 7,041-7,149 | 1,231-1,244 | 1,134-1,255 |
| slot | 2,696-2,796 | 2,545-2,553 | about 566 | about 529 |
| disc with a hole | about 1,168 | about 1,190 | about 241 | about 240 |
| gear, 24 teeth | 14,364-14,511 | 14,619-14,698 | about 3,220 | about 3,252 |

A counting build gave the share: 35-80% of `Iv` operations there have two point operands, and 48-100%
of those were exact (gear `new`: 936 operations, 569 point-point, 397 exact). Exact cases got
faster (fewer widened intervals downstream), inexact ones slower (the extra transform for nothing);
the odd-coordinate rectangle lost 5-6%, beyond noise. Results were the same bits in all six
(area bits and classifications). So the exactness lives in the crossings only, and `Iv` is as on
main. The one change left on those paths is `orient`'s fast filter, now shared with
`dot_sign`: against main's `orient`, three interleaved runs on a quieter machine gave the same
times within 1-2% both ways on all six (rect-odd `new` 2,042-2,076 against 2,065-2,071 ns, gear
`classify` 2,983-3,006 against 2,983-3,079 ns) and the same bits. The slice's `tess` cases (criterion,
second of two interleaved rounds; the first was under load) gave main against this branch:
box-cyl-hole 49.0 against 49.7 us, plate-4 274.0 against 275.0 us, plate-12 2.782 against 2.777 ms,
gear-60 1.658 against 1.649 ms.

### 31.7 Limits and open questions

- Rational Bézier of degree 3 and up and splines are `Pending::Bezier`; two flat conics are
  `Pending::FlatPair` (neither has an implicit form here). Both wait on step 5's planar widening.
- The isolator (`Coef`, Descartes runs, `order_on`) is private to scx-planar; when step 5 widens
  planar to splines it should move to scx-algo next to `bernstein`, so 2-D and 3-D share it.
- A cluster with an end whose sign the rounding hides reports the Rolle bound, `exact: false`; an
  even bound with no proof the curves are near is `Undecided`. No test reaches the second.
- Two arcs of one circle meeting at a joint may report a stretch no longer than the resolution there
  as an `Overlap` rather than a meet (the test accepts either within 1e-5 of the joint).
- A run of leaves next to a range certified outside the bounded range, with an undecided end sign,
  is `Undecided` rather than guessed.

### 31.8 Proposed 04 row

PL6 (74 §31): `scx_planar::crossings` returns every meet (with multiplicity, exact or a certified
bound at the resolution) and every overlap of two bounded lines or conic arcs, or a typed
`Undecided`; degree 3 and up and flat pairs are `Pending` until step 5. One implicit curve composed
with the other (degree <= 4), exact signs on coefficients, exact point arithmetic where the rounding
is exact; no tolerance but the `Resolution`. The pin's touch-at-the-foot, 1e-14 parallel and `atan2`
slack rules are not ported.

## 38. SP2b as built (11-28)

`scx-step` states a face's surface as the quadric it is when the file writes it as something
else, and records what that cost. Cargo edges added (ruled 11-28 by the coordinator):
`scx-step -> scx-nurbs` and `scx-step -> scx-nurbs-surface`, both below `scx-step` in
`layers.txt` (no change there; `scx-mass` already sits on both). They are the only way a file's
control net, knots and weights become a `Surface::Nurbs` to hand to `scx_surface::recognize`;
no `scx-surface` constructor was added. Code: `crates/scx-step/src/geom.rs` (`surface`,
`nurbs`, `Recognize`), `build.rs`, `import.rs`.

### 38.1 API

```rust
pub struct Recognition { pub face: u64, pub surface: u64, pub built_on: Surface, pub deviation: Length }
Built { .., pub recognized: Vec<Recognition> }       // in the file's face order
Note::Recognized { face: u64, deviation: f64 }
```

`BuildError::kind()` of `Geometry { cause: GeomCause::Surface(e), .. }` is now `e.kind()` (an
exhausted recognition budget is `Exhausted`, a stop `Cancelled`), not always `Refused`.

### 38.2 What it does

- A `B_SPLINE_SURFACE` (distinct knots and multiplicities expanded, net rows along u, weights as
  written) becomes a `NurbsSurface<3>` and goes through `recognize` over its whole domain, at an
  `Accuracy` equal to the body's allowance (the stated uncertainty, never below the document's
  zone: the same number the edge raises are measured against). A recognised quadric is the
  face's surface. `None` (not proved) is `Pending { Waits::BSplineSurface }`, as before: nothing
  is approximated and the body is not built.
- `CONICAL_SURFACE` and `TOROIDAL_SURFACE` go through `recognize_written` (an angle-0 cone is the
  cylinder, a major-0 torus or a spindle torus within the allowance the sphere; a spindle torus
  beyond it is still the torus constructor's refusal).
- The deviation is `Recognition::deviation` and `Note::Recognized`. A face has no stored accuracy
  (it is derived from its edges' and vertices'), so the record lives on `Built`. The edges' and
  vertices' certified gaps stay "edge to the face as built" (`Claim::CurveToIntersection`, the
  surface being the recognised one), so the check and later ops read the gap of the body as it is.
  The deviation is the face's fidelity to the file's surface, for a caller that needs it.
- One budget (`cx.budget()`) is shared by the body's recognitions, separate from the gap budget.

### 38.3 Choices, with cases for and against

- **Recognise over the surface's whole domain, not the face's uv box.** For: the face's extent
  needs the loops in uv, which the build gets only after the surface is chosen; the whole
  domain is a sound superset. Against: a B-spline whose face is a quadric piece but whose whole
  domain is not (a patch bigger than the face) stays pending. Cases from files will tell.
- **Allowance as accuracy.** For: a surface within the file's own uncertainty of a quadric is
  that quadric as far as the file can say; one number for edges and surfaces. Against: with an
  unstated uncertainty it is the zone (1e-5 mm at default), so an exporter's non-rational
  approximation of a torus (a fillet) is correctly not recognised.
- **Deviation not added to the edge gaps.** (Ruled.) For: the Cert claims what the check
  re-proves. Against: a caller reading only edge accuracies sees nothing of the recognition;
  `Built::recognized` is the place.
- **The fit's quadric need not be the one the edges lie on.** A recognised cylinder within the
  allowance of the B-spline is a least-squares fit, so its axis may be off the one the file's
  circle edges use by up to the deviation. The exact pcurve writer then refuses the edge:
  `Pending { PcurveFit }`, typed, not a wrong body. Seen with a control point moved 1e-3 mm
  (about 10x the zone). Open question 2.
- **Cones of near-zero or near-right angle are not simplified.** `recognize` needs the face's
  extent, which an analytic surface does not carry (the unbounded domain), and the reader
  already refuses semi angles at or below 1e-12 of 0 or pi/2 (`ConeShape`). Only the exact
  written sizes the constructors refuse are routed.

### 38.4 Tests

`geom.rs` (3 added, 3 stay): exact rational B-spline forms (`to_nurbs` of a tilted plane,
cylinder, cone, sphere and torus over partial extents) are recognised, with kind and sizes to
1e-9 and deviation under 1e-9; a cylinder form with a control point moved 1e-9 is the cylinder
and moved 1e-3 against an accuracy of 1e-6 is `Waits::BSplineSurface`; an angle-0 cone is the
cylinder (deviation 0), a spindle torus within the accuracy is the sphere of its minor radius
(deviation the major radius), beyond it the torus refusal, and an ordinary cone and torus are
not recognitions. `tests/analytic.rs` (2 added, 21 stay): a cylinder body whose side is the
rational B-spline cylinder is built as the cylinder (counts, volume `25 pi 10`, one
`Recognition` on the file's surface entity, deviation under 1e-9, no raise, no finding); the
same with a control point moved 0.05 mm (beyond the zone) is `Pending { surface, BSplineSurface }`
and moved 1e-9 is built. An angle-0 cone file cannot be written: the reader refuses it
(`ConeShape`), so that route is tested on `geom::surface` alone. None removed. The 34 SP1 and SP8
tests and the 21 SP2 tests are unchanged.

### 38.5 Corpus and measurements

NIST MBE PMI (34 files from `corpus/manifest.tsv`, fetched with `cargo xtask corpus fetch
nist-pmi`): no file has a B-spline face that is a quadric. Of the 34, one (`nist_ftc_07_asme1_rd`)
reaches a B-spline surface before another pending kind; its 20 B-spline surfaces are non-rational
cubic patches (fillet approximations), correctly not recognised at the zone, and it stays
`Pending { BSplineSurface }`. Import time, release, main against this branch, over all 34 files,
is unchanged within noise except that one file (0.2 ms before, 2.8 ms after: the proof that its
first patch is no quadric). The throwaway timers were not committed.

| Case | before (main) | after |
|---|---|---|
| NIST `ftc_07` rd, first B-spline face refused | 0.2 ms (`Pending` at the face) | 2.8 ms (recognition tried, `Pending`) |
| Other 33 NIST files | per file 0.0-428 ms | same within noise (`ftc_09_rd` 428 / 422, `stc_09` 410 / 442) |
| Cylinder body, side written as exact rational B-spline | `Pending` at once | 8.9 ms per import |
| The same body, side a `CYLINDRICAL_SURFACE` | 9.3 ms | 9.3 ms |

So recognition itself is below the noise of the import (the check dominates, 74 §29.7).

### 38.6 Open questions

1. `Recognition` gives the face's whole-domain deviation. Should the check or a later op read it
   (a face-level `Cert`)? Needs a face record that carries accuracy; not in this row.
2. Snapping the fit to the file's edges: where the file's edge curves lie on an exact quadric and
   the B-spline is a slightly off copy, the recognised fit can leave the edges `PcurveFit`. A
   recogniser variant that prefers the quadric the edges lie on (or a pcurve fit within the
   deviation) would build these. Not needed for exact forms.
3. **SP2c**: B-spline edge curves that are lines, circles or ellipses (a control polygon's hull
   bounds a line; conics need a Bernstein certificate like SU7's). They stay
   `Pending { BSplineCurve }`; a B-spline face with B-spline edges cannot build until SP2c or
   step 5. On NIST this is the commoner blocker (20 of 34 files).
4. Recognition over the face's uv box rather than the whole domain, once the loops' uv extent is
   known, and the near-zero and near-right cones with it.
5. The reader's `ConeShape` refuses an angle-0 cone a file writes; `recognize_written` could
   take it (`scx-p21`'s row, not here).
