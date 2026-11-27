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
- The 11-27 revision (§15) measured nothing new either. It re-read the pin's ops modules for the
  ledger and checked the review's citations and corpus claims against the pin's files.

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

See the hand-back. (Throwaway timer, release, not committed.)

### 18.7 Proposed 04 row

CF1: `scx_check::check(&Brep, &mut Cx, Scope) -> Result<Findings, CheckError>` exists with the
structure part only. The pass shared with `contract` adds vertex manifoldness (a vertex's link,
nodes the ends of edges there and arcs the face corners, is connected; `Pinch`) and the
solid/sheet/mixed `Class`; it duplicates nothing of `finish` or the contract's other tests, and
`contract` refuses a pinched vertex (`Breach::VertexNotManifold`) under the new `Contacts::Manifold`; the default modes admit it, because BO7 builds such unions. Later
tasks add `Finding` variants (non-exhaustive) and `Undecided` cases. `Scope` is `Import`
(default), `Stitched` or `Full`; all run the structure part.
