# 01. Architecture

The design of solidcorex: what the crates are, how a body is held, where derived data lives, how
tolerances, errors, settings, determinism and validation work, and the order of work.

This replaces the earlier architecture, which restructured solidcore in place step by step. That plan
broke the 11-26 rule against mirroring solidcore's structure; git keeps it. This version has been
revised after an adversarial review (`research/reviews/review-architecture-v2.md`). Choices the
review changed say "revised after review-architecture-v2".

**Labels.**

- **READ**: read from solidcore at the pin or from solidcorex as it stands.
- **MEASURED**: measured by a research report, which is cited.
- **JUDGED**: a design judgement. Most of this document is JUDGED; the label is given where it
  matters.

**Shape of each choice.** Each significant choice gives:

- the options weighed;
- the one chosen, and why;
- what would make us change it ("Change if").

**Binding inputs.**

- The 11-26 rows of `04-decisions.md`, including the trim-gap rows of section 3.
- Research 51 (body structure), 54 (the tolerance middle path) and 55 (certified trim gaps).
- The rules of `07-conventions.md` and `08-glossary.md`.

Where this document differs from one of them, section 12 says so.

---

## 0. Principles

These come from the owner's rules and from the faults in section 11. Every later section applies them.

1. **A library of mechanisms.** solidcorex gives everything needed to write CAD software, split so a
   user takes only what they need. Conveniences go in helper crates on top, never in the core.
2. **One-way dependencies.** Crates form a strict order, which `cargo xtask layers` checks. Modules
   inside a crate follow an order too, written in the crate's `lib.rs` doc.
3. **Signatures tell the truth.**
   - A read takes `&Brep`.
   - An edit takes a `Draft` or returns a new `Brep`.
   - Nothing writes through a shared reference, except fill-once cells that cannot change an answer.
4. **No ambient state.** There are no statics and no thread-locals. Settings come only from the
   arguments.
5. **Tolerances are data.** There is one resolution per document. Accuracies are recorded on the
   entities that need them. No tolerance is raised silently.
6. **The same answer every time.** The answer does not depend on thread count, call order, cache
   state, a save and load or, in the end, platform.
7. **Typed errors.** Each crate has its own error type, and each failure has a cause an app can match
   on.
8. **Knowledge, not structure.** Algorithms come from solidcore. Layout, caching and interfaces are
   our own. Code is transliterated where solidcore is already idiomatic.

---

## 1. Crate split

### 1.1 The core crates

All crates are Apache-2.0 and named `scx-*`.

The core is a spine of six crates (layers 0-5). Read crates, operation crates and exchange crates
hang off it. Couplings measured between solidcore's operation areas set the order of the operation
crates (C3).

| Layer | Crate | Purpose | Depends on | Adopted alone, a user gets |
|---|---|---|---|---|
| 0 | `scx-num` | vectors, points, frames, rigid and affine transforms, extents, intervals and Taylor models, units and tolerance newtypes, `Cert` and `Basis`, constants, platform-independent transcendentals (`fmath`), error-free transforms, the fixed hash and `Rng` | std | a deterministic small-vector, interval and certificate library |
| 1 | `scx-algo` | polynomials and root isolation, 1-D and n-D local solvers, Krawczyk, the global solver, deterministic sorting and grouping, a box tree over `Extent`, the `Executor` trait | num | solvers and spatial search with no CAD types |
| 2 | `scx-nurbs` | B-spline and rational B-spline curves and surfaces as plain data: evaluation, knots, splitting, degree change, fitting, approximation with a per-span `Cert`, Gordon and n-sided patches, IGES entity 126/128 conversion | num, algo | a complete NURBS library |
| 3 | `scx-geom` | every curve and surface kind: analytic, NURBS, procedural (offset, blend with its section and station mathematics, sweep, intersection curve, volume deformations); 2-D curves for pcurves; evaluation generic over the scalar; natural parameterisation; `restrict`; transforms; closed-form properties; `SurfaceFn` approximation | num, algo, nurbs | geometry with no topology |
| 4 | `scx-geomq` | queries between geometry: projection, closest points, curve-curve, curve-surface and surface-surface intersection (including the B-spline patch SSI), ray casting, analytic recognition, plane fitting, distance, certified gap bounds | 0-3 | an intersection and projection toolkit |
| 4 | `scx-planar` | 2-D regions from loops, their Booleans and offsets, profiles and line families, cell grids, bounded 2-D intersections for drawings | 0-3 | planar region work for sketches and drawings |
| 4 | `scx-sketch` | constraint residuals over dual numbers and their solver, sketch arc arithmetic | 0-3, planar | a sketch constraint solver |
| 5 | `scx-brep` | the body: `Brep`, `Draft`, regions, shells, faces with two sides, edges, vertices, ids, cells, recorded accuracies, `Document`, `Changes`, `carry`, structural checks, traversal, `transform` | 0-4 | a body data model that can be built, walked, edited and moved |
| 6 | `scx-bodyq` | reads on finished bodies: point, curve and ray classification, distance, contact, mass properties, the full validity check, continuity, pick, tangent chains, face measures | 0-5 | measuring and checking bodies |
| 6 | `scx-record` | the exact stored form of bodies and documents: versioned, keeping ids, with its own binary and text encodings and exact procedural records | 0-5 | saving and loading |
| 7 | `scx-stitch` | sewing sheets along coincident edges and vertices, gap closing, orientation, and `rebase` to a new resolution | 0-6 | repair of imported data without Booleans |
| 7 | `scx-boolean` | the cellular merge, union, subtract, intersect, sections, imprint, split by surface or sheet, trims of sheets by sheets | 0-6 | Booleans |
| 8 | `scx-make` | primitives, extrude, revolve, sweep, loft, skins, Coons and network patches, sheets between sheets, planar prisms | 0-7 | body construction |
| 8 | `scx-repair` | the repairs that need Booleans: short edges, sliver faces, tolerance defects found by the check | 0-7 | full healing |
| 9 | `scx-blend` | fillet and chamfer (constant and variable), vertex blends, fillet removal, weld beads | 0-8 | blends |
| 9 | `scx-taper` | taper (the draft-angle feature) | 0-8 | tapers |
| 9 | `scx-local` | move, replace and delete faces with the gap healed (defeature), untrim, extend, dome | 0-8 | direct editing |
| 10 | `scx-offset` | offset faces, shell, thicken, thin features, sheet offsets | 0-9 (blend for fillet removal and reapplication) | offsets |
| 10 | `scx-deform` | global deformation of bodies through volume kinds (bend, twist, unbend, B-spline volume) | 0-8 | deformation |
| 10 | `scx-sheetmetal` | flat pattern and fold of sheet metal parts | 0-9, planar | sheet metal |
| 11 | `scx-tess` | faceting with chord, angle and edge-length bounds, watertight along edges, and the dense grid mesher for NURBS faces | 0-5 | meshes for display and export |
| 11 | `scx-hlr` | hidden lines, silhouettes, projected outlines and view projection | 0-6, tess | drawings |
| 11 | `scx-mesh` | mesh to B-rep reconstruction, mesh reduction, faceted bodies and their exact volume, STL and 3MF | 0-7, tess | mesh work |
| 11 | `scx-step` | STEP read and write of geometry, solids and sheets; product structure into placed instances | 0-6, stitch | STEP exchange |
| 11 | `scx-dxf` | DXF read and write of 2-D paths | 0-4, planar | DXF exchange |

### 1.2 Helper crates

These are not core.

| Crate | Purpose | Depends on |
|---|---|---|
| `scx-fates` | turns `Changes` (2.6) into the typed record of what became of each face, edge and vertex. It follows names across a chain of operations and stores that history in its own record | brep, record |
| `scx-features` | feature compositions (hole, pocket, pattern, extrude-cut, extrude to next) built from operation calls. It computes no geometry itself | ops crates, fates |
| `scx-frames` | structural frame members, gussets, end trims, tube copes, section corners | make, boolean, bodyq |
| `scx-teeth` | gear teeth, involutes, shaft splines and hubs | make, boolean, planar |
| `scx-weld` | weld symbols, and bead bodies laid along joints | blend, bodyq |
| `scx-cadir` | the CADIR neutral model read into bodies | brep, stitch |
| `scx-testkit` | fixtures built in code, property generators, oracles, determinism harnesses, `assert_kept`; a dev-dependency only | brep, bodyq, record |
| `scx-difftest` | differential tests against the pinned solidcore, and a converter between pin records and `Brep`. The only crate that links the pin; never published | everything ported |

**Why this shape (JUDGED).**

- **Geometry without topology.** Layers 0-4 have no idea a body exists. A user who wants curves,
  surfaces, intersections, sketches or planar regions stops at layer 4.
  - In solidcore, curves and surfaces carry owner links into topology. `Curve` and `Surface` hold
    `owner: Option<ObjectRef>`, notify identities and a cache slot (READ), so today the two cannot
    be separated.
- **Reads separate from data.** `scx-bodyq` holds everything that reads a finished body and changes
  nothing.
- **A viewer needs no query stack.** `scx-tess` depends only on 0-5. Faceting needs the body and
  geometry, not classification.
- **Exchange without operations.** `scx-step` depends on `scx-stitch`, not on Booleans, so reading
  STEP pulls in neither `scx-boolean` nor `scx-make`. This is revised after review-architecture-v2
  (S3), where the earlier `scx-heal` pulled both.

### 1.3 What maps where: the inventory

This answers review B4. It is revised after review-architecture-v2.

The app talks to solidcore only through `solidcore-ops`: one seam, with 84 modules and 485 `pub fn`s
(READ, research 38 and the review). The tables below map every pin crate, every kernel area and
every `solidcore-ops` module to its home. The app switch is planned against them.

The "app features" column names what an app loses until the crate exists. The sizes are line counts
at the pin, tests included (READ).

**Pin crates.**

| Pin crate | Lines | Home | App features that need it |
|---|---:|---|---|
| `solidcore` (the kernel) | 1.16M | split by area, next table | everything |
| `solidcore-ops` | (84 modules) | split by module, table after next; the app then calls scx crates directly | everything |
| `solidcore-record` | 17,840 | `scx-record` (bodies, document); history records to `scx-fates`; metadata to the app | open, save |
| `solidcore-convert` | 8,603 | not needed: the stored form is the `Brep`'s own. The pin converter lives in `scx-difftest` | none |
| `solidcore-step`, `step-export` | 24,763 + 7,791 | `scx-step` | STEP import and export |
| `solidcore-cadir` | 8,070 | `scx-cadir` (helper) | CADIR import |
| `solidcore-tess`, `nurbs-grid-mesh` | 16,731 + 2,398 | `scx-tess` | display, mesh export |
| `ssi`, `solidcore-ssi` | 11,784 + 2,187 | `scx-geomq::ssi` | every Boolean on B-spline faces |
| `goldens` | 445,047 | the cases feed `scx-difftest` and the corpus. The golden files are not carried | none (validation) |
| `diagnostics` | 1,018 | not ported; replaced by the progress and trace sink in `Cx` (6.3) | none |
| `xtask` | | solidcorex's own `xtask` | none |

**Kernel areas** (`solidcore/src`).

| Area | Lines | Home |
|---|---:|---|
| `math`, `limits` | | `scx-num` |
| `geom` | 50,540 | vectors, extents, transforms, intervals, `certified`, `tol` to `scx-num`; solvers, `polynomial`, `global_solver`, `hash_table`, `tree` to `scx-algo`; `nurbs_enclosure` to `scx-nurbs`; `pseudo_box`, `axis2_placement` to `scx-geom`; `plane_height`, `facing_trough`, `axial_section`, `point_set` to `scx-geomq`; `paper_gaps` to `scx-planar`; `attribute` to `carry` and app tables (2.6); `context`, `object`, `a_object`, `check_memo`, `content` not ported (ambient state and object identity, section 11) |
| `nurbs` | 93,661 | `scx-nurbs`; `certificate` with the per-span form (4.1); `iges` to `scx-nurbs::iges` |
| `eval` | 22,111 | plain-data evaluation to `scx-geom` (geometry is plain data here); `constraint` to `scx-sketch`; `outline` to `scx-hlr`; `pcurve` to `scx-brep`; `hole_stacks` to `scx-bodyq`; `label` to `scx-record` |
| `curve` | 77,316 | `scx-geom::curve`; `crv_in_volume` to `scx-geom::volume`; `conic_projection`, `spline_projection`, `fit_projection`, `plane_of`, `proximity`, `contact` to `scx-geomq` |
| `surface` | 177,539 | `scx-geom::surface`; volume kinds (`b_spline_volume`, `bend_volume`, `twist_volume`, `unbend_volume`, `srf_in_volume`) to `scx-geom::volume`; `recognise`, `surface_point_solve`, `implicit` to `scx-geomq`; `face_*`, `mid_sheet`, `mid_joint`, `cylinder_wall`, `side` to `scx-bodyq`; `extend` to `scx-local`; `surface_cache` and `trim_srf_cache` not ported (cells instead) |
| `intersect` | 83,659 | surface, curve and silhouette intersectors and the ray tracer to `scx-geomq`; `point_class`, `curve_class` to `scx-bodyq`; `topology_solver` to `scx-boolean`; `views`, `surface_silhouette` to `scx-hlr` |
| `topology` | 152,233 | data, constructor, traverser and `trimming_tools` to `scx-brep`; `integrity` to `scx-bodyq::check`; `api_files`, `part_file`, `database_io` to `scx-record`; `brep_cache` not ported (cells); the use records replaced (2.1) |
| `query` | 55,518 | `scx-bodyq`; `tess` to `scx-tess`; `curve_surface_gap`, `distance_bound`, `deviation` to `scx-geomq` |
| `mass` | 9,583 | `scx-bodyq::mass` |
| `boolean` | 33,444 | `scx-boolean`; `operand_tolerances` not ported (no body tolerances) |
| `blend` | 232,788 | `scx-blend`; `fillet_section`'s station, interval, series and certificate to `scx-geom::procedural` (they define the blend surface); `weld_bead` to `scx-weld`; `fillet_removal` stays in `scx-blend` |
| `offset` | 53,744 | `scx-offset`; `draft` to `scx-taper` |
| `sweep`, `primitive` | 26,328 + 20,712 | `scx-make` |
| `heal` | 23,674 | `stitch`, `gap` to `scx-stitch`; `tolerance_defects`, `face_props` to `scx-repair` |
| `prism` | 19,647 | the planar core (profiles, regions, fill, fit, arc conversion) to `scx-planar`; extrusion and sheets to `scx-make`; recognition and measure to `scx-bodyq`; members and gussets to `scx-frames` |
| `cell_grid` | 6,998 | `scx-planar` |
| `develop` | 10,457 | `scx-sheetmetal` |
| `hlr` | 13,775 | `scx-hlr` |
| `teeth` | 24,715 | `scx-teeth` |
| `weld_symbol` | 377 | `scx-weld` |
| `mesh_reconstruction`, `mesh_reduce` | 4,562 + 2,738 | `scx-mesh` |
| `kernel` (`fixes`) | 1,900 | not ported: no switches (6.3) |
| `error` | 35,560 | each crate's own error (5) |
| `profile.rs` (profiler) | | not ported; the trace sink (6.3) |

**`solidcore-ops` modules.** Modules that only convert between records and kernel bodies disappear,
because the app holds `Brep`s directly.

| Home | Modules |
|---|---|
| `scx-num` | `affine` |
| `scx-nurbs` | `bspline`, `curve`, `curve_fit`, `curve_knots`, `surface_cvs`, `network` |
| `scx-geomq` | `conic_projection`, `spline_projection`, `fit_projection`, `curve_plane`, `curve_gap`, `axial_section`, `facing_trough`, `vertex_meet` |
| `scx-planar` | `profile`, `cell_grid`, `paper_gaps`, planar-sheet Booleans from `boolean`, the 2-D half of `prism` |
| `scx-sketch` | `constraint`, `sketch_arc` |
| `scx-brep` | `body` (copy, transform, counts, the result contract), `provenance` and `history` as `Changes` and `carry`, `point_identity`, `pcurve` |
| `scx-bodyq` | `measure`, `check`, `contact`, `continuity`, `pick`, `query`, `tangent`, `face_pair`, `face_surface`, `round_face`, `cap`, `mid_sheet`, `mid_joint`, `cylinder_wall`, `joint_turn`, `hole_stacks`, the recognition half of `prism` |
| `scx-record` | `file`, `blend_definition` (exact blend records), `label`, `record_check`, `order` (gone: ids are kept, so a record index is the id) |
| `scx-stitch` | `stitch` |
| `scx-repair` | `heal` |
| `scx-boolean` | `boolean`, `split`, `trim` |
| `scx-make` | `sweep`, `surface` (Coons and bounded patches), `blend_surface`, the extrusion half of `prism` |
| `scx-blend` | `blend` |
| `scx-local` | `untrim`, `extend`, `dome` |
| `scx-offset` | `offset` |
| `scx-sheetmetal` | `develop` |
| `scx-tess` | the faceting calls of `measure` and `hlr` |
| `scx-hlr` | `hlr`, `outline` |
| `scx-mesh` | `mesh`, `faceted` |
| `scx-step` | `step_assembly` |
| `scx-dxf` | `dxf` |
| `scx-features` | `stop` (extrude to next) |
| `scx-frames` | `frame_member`, `frame_gusset`, `frame_trim`, `tube_cope`, `section_corners` |
| `scx-teeth` | `shaft_spline` |
| `scx-weld` | `bead` |
| `scx-testkit` | `test_corpus` |
| `Cx` and errors (6.3, 5) | `session` (gone: no session), `cancel` (the `Stop` in `Cx`), `fuel` (the budget in `Settings`), `error` (each crate's error) |

Every module and area is placed. A home split across crates names each part.

When a crate is started, its first task is to confirm its rows against the code and record any
module that moved.

### 1.4 Choosing a subset

A user picks crates, not features. For example:

- **geometry only:** `scx-geom` and `scx-geomq`;
- **a sketcher:** `scx-planar` and `scx-sketch`;
- **a viewer:** `scx-record` and `scx-tess`;
- **a modeller:** add `scx-bodyq`, `scx-make`, `scx-boolean` and the layer-9 and layer-10 crates it
  needs;
- **an app with history:** add `scx-fates` and, if it suits, `scx-features`.

No facade crate re-exporting everything is provided at first. A facade is easy to add and easy to get
wrong: it becomes the place where everything depends on everything.

### 1.5 Choices

**C1. Curves and surfaces in one crate or two.**

- Options:
  - (a) `scx-curve` below `scx-surface`;
  - (b) one `scx-geom` with `curve`, `pcurve`, `surface`, `procedural` and `volume` modules in a
    checked order;
  - (c) one crate per kind family.
- Evidence: at the pin, surface refers to curve 314 times and curve refers to surface 48 times
  (MEASURED, research 49). The back-references are curve kinds defined on surfaces. Under (a) those
  kinds would have to move into the surface crate, splitting the curve enum across two crates.
- **Chosen: (b).** The module order is checked by `cargo xtask layers`, extended to module level.
- Change if:
  - a user wants curves without surfaces; or
  - the crate's build time becomes the worst in the tier-1 loop.

**C2. `scx-brep` and `scx-bodyq` as two crates.**

- Options: (a) one crate; (b) the data model in `scx-brep`, and reads over many faces in
  `scx-bodyq`.
- **Chosen: (b).**
- `scx-brep` depends on `scx-geomq`, because its cells (boxes, boundary trees) need projection.
  Under the vertical slice (10.2) only the analytic part of `scx-geomq` exists at first, so this
  dependency does not hold the slice back.
- Change if:
  - more than a handful of `scx-bodyq` items need private access to `scx-brep`; or
  - the slice shows `scx-geomq` must be finished before `scx-brep` can be tested. Then the cells'
    fill functions move behind the `FaceCell` trait (3.2), implemented in `scx-bodyq`.

**C3. Operation crates and their order.** This choice is revised after review-architecture-v2 (S3).

- Options:
  - (a) one `scx-ops`;
  - (b) a crate per family, never depending on each other;
  - (c) a crate per family, in an order set by the measured couplings, with shared parts moved down.
- Each measured coupling is broken as follows (MEASURED, research 49; the pieces are READ):

| Coupling | References | Resolution |
|---|---:|---|
| `primitive` to `boolean`, `sweep` to `boolean` | 23, 18 | the uses are the merge and topology intersector (READ); `scx-make` sits above `scx-boolean` |
| `primitive` to `sweep` | 36 (12 back) | both in `scx-make` |
| `blend` to `boolean` | 29 | `scx-blend` sits above `scx-boolean` |
| `offset` to `blend` | 36 (62 in the area table) | 31 of the uses are `fillet_section`'s station, interval, series and certificate (READ). They define the blend surface, so they move to `scx-geom::procedural`. The rest (fillet removal and reapplication through the fillet executive) make `scx-offset` depend on `scx-blend`, stated |
| `offset` to `heal` | 18 | stitching only (READ), so `scx-offset` uses `scx-stitch` |
| `offset` to `boolean` | 15 | `scx-offset` sits above `scx-boolean` |

- **Chosen: (c).** A family shares solvers and data. A crate per operation would duplicate those, and
  one crate would rebuild solidcore's 38 cross-references.
- Change if: a new coupling appears in a port. The shared piece then moves down a layer. Two
  families are never merged.

---

## 2. Body model

### 2.1 Entities

This section is revised after review-architecture-v2 (B1, S11, K5).

| Entity | Holds | Stored in |
|---|---|---|
| `Region` | a volume: `RegionId(0)` is the infinite region outside everything; others are solid or void, with a `solid: bool` | region table |
| `Shell` | a connected set of face sides bounding one region, plus its wire edges and lone vertices; the region it bounds | shell table |
| `Face` | surface, `sides: [ShellId; 2]` (front, back, front taken about the natural normal), loops of `EdgeUse`s, cells | `Arc<FaceRecord>` per face |
| `EdgeUse` | the edge, sense, pcurve (a 2-D curve and interval on the face's surface), the pcurve's recorded bound | inside its face's loop |
| `Edge` | **geometry part:** curve and interval (none for an edge collapsed to a point), end vertices, recorded accuracy. **Adjacency part:** the face sides around it in radial order | edge table, two parts |
| `Vertex` | point, recorded accuracy | vertex table |

Each face has two sides, and each side names the shell, and so the region, on it:

- A solid's boundary face has its outer side in `RegionId(0)` or a void, and its inner side in the
  solid.
- A face between two solid regions has a region on each side, which the cellular merge needs.
- A sheet or lamina face has both sides in the same region.

The pin's Boolean labels regions in one cellular body and then keeps or deletes them
(`boolean/merge/p4.rs:1-58`, READ). That works here unchanged. A face's two sides cost 8 bytes.

**Special cases** (from review K5):

- **A seam edge** appears twice in one face's loop. Its adjacency lists the face twice, told apart by
  sense, and the mate is in the same face.
- **A wire edge** has no face sides. It belongs to its shell's wire list.
- **A lone vertex inside a face** (a cone apex, an imprinted point) is a loop of one `EdgeUse` on an
  edge collapsed to that vertex.

**C4. The layout of use records.** This choice is revised after review-architecture-v2 (B1, S11).

- It answers the 04 open question: "The layout of the use records ... and where gap data lives".
- Evidence: at the pin, use records are 71-80% of a body's bytes (MEASURED, research 51). The cause is
  four uses per manifold edge, gap caches stored inline, and trims (research 51 section 1).
- Options:
  - (a) radial edge, as at the pin;
  - (b) half-edges with twin, next and previous pointers;
  - (c) loop lists inside the face record, two face sides, and the radial order stored on the edge.
- **Chosen: (c).**
  - There is one use per face per pass of the edge, so a manifold edge has two uses.
  - No pointers span records.
  - The radial order is stored in the edge's adjacency part, so the merge can walk it while the body
    is a `Draft`, before any cell exists.
- Gap data:
  - pcurve to surface and curve: on the `EdgeUse`;
  - an edge's own accuracy: on the edge's geometry part;
  - a vertex's accuracy: on the vertex.
  - Each is an `Option`, with `None` meaning "within the resolution stamped on the body" (B2,
    section 4).
- Mate uses are found by scanning the other face's loops. These hold 4 to 20 uses on typical faces
  and about 130 on the largest measured (review K4).
- Change if: the mate scan shows in the top five costs of a Boolean or blend. A body-level use index
  then becomes a body cell rebuilt at `finish` (3.1). Stored pointers are never added.

### 2.2 Ids

- `RegionId`, `ShellId`, `FaceId`, `EdgeId` and `VertexId` are typed `u32` newtypes.
- They are dense within one body's line of descent and never reused: an edit that adds a face takes
  the next number. Deleted slots stay as `None`.
- **The stored form keeps ids as they are.** Runs of deleted slots are written as counts. This is
  revised after review-architecture-v2 (S6).
  - Face order alone moves mass in 173 of 474 bodies (MEASURED, research 39). Id-ordered sums,
    id-keyed draws and lowest-index errors would change across a save and load if saving renumbered.
  - Compaction is an explicit operation, `Brep::compact(&self) -> (Brep, IdMap)`, never a side
    effect.
- **There is no `BodyId`.** A body is a value, and the app names its bodies however it likes.
- Ids are not names. An id says which entity in this body; names across edits come from `Changes`
  and `scx-fates`.

### 2.3 Finished and editable bodies

```rust
pub struct Brep { /* Arc'd tables; resolution stamp; Send + Sync; Clone bumps refcounts */ }
pub struct Draft { /* the same tables, written copy-on-write; the change events */ }

impl Brep {
    pub fn edit(&self) -> Draft;              // shares everything
    pub fn into_draft(self) -> Draft;         // writes in place where uniquely owned
    pub fn face(&self, f: FaceId) -> Option<Face<'_>>;   // a borrowed view, never a copy
    pub fn resolution(&self) -> Resolution;   // the stamp (4.3)
    pub fn shed_cells(&mut self);             // 3.4
}
impl Draft {
    pub fn finish(self, doc: &Document) -> Result<(Brep, Changes), Box<Unfinished>>;
}
pub struct Unfinished { pub draft: Draft, pub cause: StructureCause }
```

- A `Brep` cannot change.
- `finish` does three things:
  - checks the structure of what the draft touched: loops closed, uses consistent, sides and
    regions consistent, radial orders complete, orientation;
  - stamps the document's resolution;
  - rebuilds the body cells it must (3.1).
- On a structural failure, `finish` returns the draft with the cause, so tools can show the broken
  structure (review K6). The error is a `Defect` when the caller is one of our operations (5).
- Geometric validity (self-intersection, gaps) belongs to `scx-bodyq::check`, which only reads.

**C5. Whole-table copy-on-write.** This is kept as ruled on 11-26, from research 51.

- App bodies have a p99 of 94 faces (MEASURED, research 51).
- Chunks of 64 replace whole tables, behind the same `Table` trait, only when bodies pass about
  10,000 faces.
- Change if: models with more than 10,000 faces enter the benchmark set.

### 2.4 Moving a body

This section is revised after review-architecture-v2 (S1).

**C14. Placement.**

- It answers the 04 open question "Do bodies carry a placement".
- Options:
  - (a) no placement, and a transform rewrites geometry;
  - (b) a rigid placement on every `Brep`;
  - (c) (a), plus placed operands where a move without rewriting pays.
- **Chosen: (c).**
  - `scx-brep::transform(&Brep, Rigid) -> Brep` rewrites geometry and keeps every id, so names and
    `Changes` are trivial: every entity is `Touch::Geometry`. Cells refill lazily.
  - Operations that take a second body, and `scx-bodyq` reads, accept
    `Placed<'_> { body: &Brep, frame: Frame }`. Pattern tools and assembly instances then move in
    O(1) where it pays. They are mostly Boolean tools: 82% of pattern bodies are intermediate
    (MEASURED, research 38).
  - The Boolean's internal origin shift (`p4.rs:27`, READ) stays as a precision device inside
    `scx-boolean`.
- Under (b), every geometric parameter of every operation (split planes, pull directions, sweep
  paths, rays, pick points) would need mapping into body coordinates across more than 15 operation
  crates.
- Change if: the app's corpus shows rigid moves of bodies that are then read many times without a
  Boolean, for a measured share of regeneration over 5%.

### 2.5 How edits share

- Unchanged face records are shared by `Arc` between the old and new body, cells included.
- A cell filled through a descendant is visible through the ancestor (research 51). That is harmless,
  because a cell is a pure function of its record.
- **Every write path empties the written record's cells** (3.2). This is revised after
  review-architecture-v2 (B5).
- The rule that operations do not rewrite what they do not change (04, 11-26) is what makes sharing
  pay:
  - a fillet replaces only the faces it trims or creates;
  - a shell keeps edge and vertex ids;
  - a transform keeps all ids.
  - `scx-testkit::assert_kept(before, after, expected)` lets operation tests state how many face
    records must survive by pointer.

### 2.6 Changes, provenance and attributes

This section is revised after review-architecture-v2 (B3).

A draft records events as the operation edits. They are not reconstructed afterwards. `finish`
returns them as `Changes`. This is the mechanism; the typed fates record is the helper's job (04,
11-26).

```rust
pub struct Changes {
    pub faces: EntityChanges<FaceId>,
    pub edges: EntityChanges<EdgeId>,
    pub vertices: VertexChanges,       // created (with Origins), removed, merged
}
pub struct EntityChanges<I> {
    pub replaced: Vec<(I, Touch)>,              // record replaced, id kept
    pub created: Vec<(I, Origins)>,             // Origins: sorted, non-empty
    pub merged: Vec<(I, Vec<I>)>,               // survivor, absorbed
    pub coincident: Vec<(Operand, I, I)>,       // an operand's entity landing on another's
    pub removed: Vec<I>,
}
pub enum Touch { Boundary, Geometry }
pub enum Origin { Face(FaceId), Edge(EdgeId), Vertex(VertexId), Operand(Operand, Entity), Free }
```

- Every list is sorted by id, so `Changes` is deterministic.
- `Origins` is a list because a face made by merging coincident faces of two operands has two
  origins.
- **`carry`**, in the core, propagates application data held outside the body:

  ```rust
  pub trait Carry<T> { fn split(&self, v: &T) -> T; fn merge(&self, vs: &[&T]) -> T; fn coincident(&self, a: &T, b: &T) -> T; }
  pub fn carry<I: Id, T: Clone>(table: &IdMap<I, T>, changes: &EntityChanges<I>, rule: &impl Carry<T>) -> IdMap<I, T>;
  ```

  - Attributes (colour, tags, application values) live in the app's id-keyed `IdMap`s and are carried
    by rule.
  - The body stores none. This replaces the pin's attribute store (`geom/attribute.rs`) and its
    provenance hook (`solidcore-ops/src/provenance.rs`, READ).
- Names and attributes are keyed by id plus `carry`. Only meshes and other pure derived data are keyed
  by record pointer (`Weak<FaceRecord>`), because a name must survive the trimming that replaces a
  record.
- **Tested early.** The tests of `provenance.rs` and `history.rs` are ported against `Changes` and
  `carry` as soon as `scx-brep` exists, before any operation crate (10.2).

---

## 3. Caches and derived data

This section is revised after review-architecture-v2 (B5, S9, S11, S12).

### 3.1 Where each kind lives

| Kind | Example | Lives in | Invalidation |
|---|---|---|---|
| Geometry cell | a B-spline surface's box tree, a curve's analysis, a procedural surface's approximant index | `OnceLock` fields inside the `Arc`'d payload of the geometry kind | none needed: the payload is immutable and shared by every face using it |
| Face cell | face box, uv box, boundary tree for point-in-face, sampled boundary, derived face accuracy | `FaceRecord::cells: FaceCells` | emptied by every write path; a new record starts empty |
| Edge or vertex cell | edge box, curve polyline | `cells` field of the geometry part of the row | emptied by every write to that part |
| Body cell | body box, face box tree, use index, vertex stars, mate positions | `Brep::cells` | rebuilt or emptied at `finish`; inside a draft, cross-entity data the operation needs is kept by the draft itself |
| Memo | a proof result reused across calls, a repeated projection | `Scratch`, owned by the caller | keyed by the bits of every input; evicted only to bound memory |
| Helper derived data | meshes, display lists | the helper's table, keyed by `Weak<FaceRecord>` | the key dies with the record |
| Application data | names, attributes | the app's `IdMap`s, propagated by `carry` | none: carried by rule (2.6) |

### 3.2 Rules that make invalidation trivial

1. **Cells are a separate field, reset on every write path.**
   - `Draft::face_mut` is the only mutable accessor. It replaces `cells` with
     `FaceCells::default()` on every call, whether the record was cloned by `Arc::make_mut` or is
     being written in place after `into_draft`.
   - Edge and vertex rows work the same way.
   - A test edits records with filled cells through both `edit()` and `into_draft()` and checks that
     every cell is empty.
2. **The read set is in the type.**

   ```rust
   pub trait FaceCell: Send + Sync + Sized {
       fn fill(rec: &FaceRecord, edges: &EdgeGeomTable, verts: &VertexTable) -> Self;
   }
   ```

   - No `Cx`, `Scratch`, `Settings` or `Document` is reachable, and neither is the edge adjacency
     part.
   - A cell is therefore a pure function of its record and the geometry of the edges and vertices
     it names, by construction.
   - Edge cells read their own geometry part and its two vertices.
3. **Marking follows the read set.**
   - A write to an edge's geometry part, or to a vertex, replaces the record of every face that uses
     it.
   - A write to a vertex also empties the cells of its edges.
   - A write to an edge's adjacency part (its radial list) marks nothing, because no cell may read
     it.
   - This answers the 04 open question "Which edge and vertex fields may a face's cached values
     read": the geometry part, never the adjacency.
   - It is revised after review-architecture-v2 (S11). Otherwise splitting a face would replace every
     neighbour across its edges.
4. **Cross-entity data is a body cell.** Anything reading several faces (the use index, vertex
   stars, mate positions) is a body cell, never a face or edge cell. The radial order is stored data
   (2.1), not a cell.
5. **Filling never changes an answer.** Two threads racing to fill a cell compute the same value, so
   `OnceLock::get_or_init` is correct. Each cell has a test that fills it on two threads and compares
   the bits.
6. **No validation by content.** It costs 13-34 times a plain read (MEASURED, research 51), and
   because records are immutable it is not needed.
7. **No incremental cell repair.** Body cells are rebuilt, not patched. The pin's staleness came from
   incremental tree updates (research 26, 30). If chunked bodies need incremental rebuilds, they get
   their own test that compares against a full rebuild.

**Cells and the resolution** (review K7). A cell cannot read the resolution. A sampled boundary or
boundary tree is built at a fixed relative density with certified distance bounds per piece, so a
query near a boundary adds the resolution itself and is correct at any resolution.

### 3.3 Memos

**C6. Where memos live.**

- Evidence: proofs are 38% of app regeneration, and 22.6% of that work repeats identical inputs
  (MEASURED, 04 section 5).
- Options:
  - (a) memos as face cells;
  - (b) memos in a caller-owned `Scratch`, keyed by input bits;
  - (c) memos in a global or per-thread cache.
- **Chosen:**
  - (b) for work keyed by inputs that span entities or bodies;
  - geometry or face cells for anything keyed by one payload or record;
  - (c) is ambient state and banned.
- `Scratch` is `&mut`, and each parallel worker gets its own.
- Every tier-1 test runs with a fresh scratch and with a warmed one, which shows that memo hits never
  change answers.
- Change if: the repeated work turns out to come mostly from one record, in which case cells cover it.

### 3.4 Memory

- Undo and per-feature caching keep old `Brep`s, and so their records' cells. Face caches are 5-160
  times the body's size (MEASURED, research 51).
- `Brep::shed_cells(&mut self)` replaces the cells of every uniquely owned record and table with
  empty ones, and leaves shared records alone. An app calls it on bodies it keeps only for undo.
- This is revised after review-architecture-v2 (S12).

---

## 4. Tolerances as data

This section implements the 11-26 owner rules and research 54. Doc 09 keeps the detailed rules; this
is the shape.

### 4.1 Types

All of these are in `scx-num`, so layers 2 and 3 can produce certificates. This is revised after
review-architecture-v2 (S10).

```rust
pub struct Length(f64);            // millimetres; private field, arithmetic as needed
pub struct Resolution(Length);     // the document's; > 0, finite
pub struct Accuracy(Length);       // a recorded bound on an entity

pub struct Cert { pub claim: Claim, pub bound: Length, pub method: Method, pub reads: Reads } // doc 09
pub enum Basis { Measured, Certified(Cert), Unproven(Cert), Stated }
pub struct Recorded { pub acc: Accuracy, pub basis: Basis }
```

Where each record lives:

- `EdgeGeom::accuracy`, `Vertex::accuracy` and `EdgeUse::bound` are `Option<Recorded>`. `None`
  means "within the body's resolution stamp".
- **A surface's accuracy has one home: the procedural kind's payload in `scx-geom`.** That payload
  holds the definition, the approximant, and a certificate per span, each with its span's uv region
  (04, 11-26, second trim-gap row).
  - A span's bound may be used only for a trim whose uv box lies inside that span's region.
  - Pieces are cut at the surface's patch lines. Across a line where the surface is only C¹, the
    plain second-order form is used. Across a C⁰ line the piece is cut (04, 11-26, third trim-gap
    row).
  - Any edit to the net makes a new payload with its bounds certified again. Records are immutable,
    so the net cannot change without a new bound.
  - Exact kinds carry nothing. The face record carries no surface accuracy.
- **Face accuracy is derived:** the maximum of the surface's span bounds over the face's uv box and
  the records of its edges and vertices. It is a face cell and never stored (research 54).
- `Unproven` covers research 55's case. A fallback trim on an offset or procedural-blend surface
  records a conservative bound marked unproven until a certified bound exists (04, 11-26).
- `Stated` covers imported entities that cannot be measured. They keep the file's value.

### 4.2 Rules

- Coincidence of `a` and `b` is `max(res, acc_a, acc_b)`. The gap allowed at an intersection is
  `acc_a + acc_b` (research 54).
- Interior tests read only the surface's bounds. Boundary tests read the edge or vertex involved.
- **There is no angle tolerance.** This is revised after review-architecture-v2 (S13), reading 04 row
  42.
  - Coincidence angles come from accuracy divided by size.
  - Feature thresholds (is this edge smooth enough to chain into a blend?) are explicit parameters of
    the operations that use them, with documented defaults in each spec.
  - The pin's literals of 20°, 25° and 3° become named algorithm constants in `consts`.
  - The `AngleTol` newtype in `scx-num` is removed; `Angle` remains for angles as values.
- Size checks use `LARGE_SMALL_SIZE_RATIO × res`. Body tolerances, `matched_tolerances` and "the
  result takes the larger tolerance" do not exist.
- Propagation:
  - a new entity takes `max(res, construction bound + input surface bounds)`;
  - a surviving entity keeps its record;
  - a raise comes only from a measurement, and is reported.
  - The rule is order-independent (research 54).
- **Valid input is never refused.** An operation that loses accuracy records it. An accuracy past
  the resolution gives `Warning::AccuracyPastResolution` in the report, never an error.

### 4.3 The resolution

- The resolution is held in the `Document` (section 6). It is chosen at creation from units and a
  size class, can be overridden, and is changed only by `scx-stitch::rebase`, which re-evaluates the
  bodies.
- There is no `resolution_for(size)` called per operation. Doc 09 section 1.2's per-size zone becomes
  the rule that picks the size class at creation.

**C7. Where the resolution lives at call time.** This choice is revised after review-architecture-v2
(B2).

- Options:
  - (a) on each `Brep` only;
  - (b) only in `Document`;
  - (c) the `Document` is the authority, and each `Brep` carries a stamp of the resolution its
    records were made under, as a check copy.
- **Chosen: (c).**
  - `Draft::finish(doc)` stamps it.
  - Every operation and every `scx-bodyq` read compares `body.resolution()` with
    `cx.doc.resolution()`. On a mismatch it returns `Refused(ResolutionMismatch { body, document })`.
  - `rebase` is the only way to change a stamp. `scx-record` stores it per body.
- Every `None` accuracy is a claim relative to a resolution. Under (b), a body made under 1e-5 and
  read under 1e-7 would silently miss contacts. Research 54 calls that the one way this model gives
  a wrong answer.
- The stamp is a fact about the data, not a tolerance a user sets, so it does not break "no per-body
  tolerance".
- Change if: an app needs bodies of two resolutions in one operation without rebasing, for example an
  assembly check across documents. That would need a stated cross-resolution rule from research
  first.

---

## 5. Errors

### 5.1 Shape

This section is revised after review-architecture-v2 (S4).

Each crate has one error type, named after the crate, with typed `#[non_exhaustive]` causes.

```rust
#[non_exhaustive]
pub enum BlendError {
    Refused(BlendCause),       // the input is not valid: an id not in the body, a radius no ball can roll
    Unsupported(BlendCause),   // valid input this method could not complete; the app may retry differently
    Items(Vec<(usize, BlendCause)>),  // per input item (5.2), sorted by item index
    Exhausted { budget: u64 }, // the work budget ran out
    Cancelled,
    Defect(Defect),            // our bug: an invariant broke; carries where
    Lower(LowerError),         // brep, bodyq, boolean ... errors, as source()
}
```

- **`Refused`** is only for input that is not valid, including `ResolutionMismatch`. "Operations
  never refuse valid input" (11-26) means: if an answer exists and our method reaches it, give it
  with recorded accuracy.
- **`Unsupported`** is honest about a case the method does not cover, such as a vertex blend it
  cannot patch or a degenerate SSI branch. Each such cause is also an entry on a list of methods to
  improve.
- **`Defect`** is `Defect { at: &'static str, what: DefectKind }`. Apps report it and never handle
  it. Library code never panics; the workspace lints already deny it.
- **`Exhausted` and `Cancelled`** come only from crates that take a budget or a `Stop`.
- Lower layers have the subset that applies to them. For example, `GeomqError` has `Refused`,
  `Unsupported`, `Exhausted`, `Defect` and `Num`.
- Every error implements `std::error::Error` with `source()`. Display strings are for people and are
  not a stable interface; the variants and causes are.
- **`fn kind(&self) -> ErrorKind`** maps one for one onto a shared `#[non_exhaustive]` enum in
  `scx-num`: `InvalidInput`, `Unsupported`, `Exhausted`, `Cancelled`, `Defect`. Apps that only
  route errors use it.

The solver pattern already in `scx-algo` is kept: `SolveError<E>` with the `EvalError`
classification (04, 11-26). Its "out of fuel" and "cancelled" classes map to `Exhausted` and
`Cancelled`.

### 5.2 Partial success

**C8.** This choice is revised after review-architecture-v2 (S5).

- Options:
  - (a) all or nothing, always;
  - (b) apply what works, always;
  - (c) all or nothing by default, with an explicit option to skip failed items.
- **Chosen: (c), defined as follows.**

```rust
pub struct Done<C> { pub body: Brep, pub changes: Changes, pub report: Report<C> }
pub struct Report<C> { pub warnings: Vec<Warning>, pub skipped: Vec<Skipped<C>>, pub raised: Vec<Raise>, pub work: u64 }
pub struct Skipped<C> { pub item: usize, pub round: u32, pub cause: C }
```

- **Items** are what the operation treats as units. For blends that is chains of edges, not single
  edges. Booleans, whole-body offsets and tapers have no items.
- **The default** returns the first failure in item order. It adds any other failures found without
  extra work, and no body.
- **`Skip`** is a fixpoint:
  1. Drop the failing items, in index order.
  2. Run again.
  3. Repeat until no item fails or a stated round cap is reached (default 3).
  - Each dropped item is reported with its cause and round. The cost is up to that many reruns, and
    it is documented as such.
  - It is deterministic, because each round is.
- Lost accuracy is never a failure. It appears in `raised` and as warnings.
- `work` is the budget used, in its deterministic units (6.3, 9.4).
- Change if: app use shows `Skip` as the default nearly everywhere.

### 5.3 Layer 0's strings

- Layer 0's `NumError` was ruled to match solidcore's strings and variants one for one (04, 11-26),
  so the difftest can compare errors exactly.
- That holds while the difftest drives the move. Once a layer is ported, its strings are free to
  change, and the facade in `scx-difftest` maps variants instead.

---

## 6. Settings and state

### 6.1 No ambient state

- Nothing in any crate is a `static` with interior mutability, a `thread_local!`, a lazily
  initialised global, or a `Cell` or `RefCell` reachable from a public type. The only exceptions
  are the fill-once cells of section 3.
- This is checked (8.3).
- For contrast, at the pin the `Kernel` carries:
  - settings;
  - 26 fix switches;
  - memos in `Cell` and `RefCell`;
  - a result-changing `blend_net_tolerance`;
  - size scalars that earlier calls overwrite.
- About 89% of the 1,232 functions that take `&mut Kernel` only pass it through (MEASURED, research
  03).

### 6.2 The document

```rust
pub struct Document {                // scx-brep
    resolution: Resolution,
    units: Units,                    // the user's display units; the kernel computes in mm
    size_class: SizeClass,
}
impl Document {
    pub fn new(units: Units, size_class: SizeClass) -> Self;   // picks the resolution
    pub fn with_resolution(self, r: Resolution) -> Self;       // override at creation
}
```

- A `Document` is a value describing the space bodies live in. It does not hold bodies.
- An app keeps its bodies however it likes and stores the `Document` beside them; `scx-record` has a
  slot for it.

**C9. Session or no session.**

- Options:
  - (a) a `Session` object owning bodies, settings and caches;
  - (b) a `Document` value plus explicit arguments.
- **Chosen: (b).** A session becomes the place memos, switches and counters collect, and it forces
  the app's ownership model onto the library.
  - Undo is old `Brep` values.
  - History is `Changes`.
  - Caches are cells or `Scratch`.
- Change if: we need a CAD-level mechanism that cannot be written without shared mutable state
  across calls.

### 6.3 The calling convention

```rust
pub struct Cx<'a> {
    pub doc: &'a Document,
    pub settings: &'a Settings,     // algorithm choices, seed, budget, executor
    pub scratch: &'a mut Scratch,
    pub stop: &'a Stop,             // cancellation
    pub trace: &'a dyn Trace,       // progress and diagnostics; a no-op by default
}

pub fn fillet(body: &Brep, edges: &[EdgeId], spec: &FilletSpec, cx: &mut Cx<'_>)
    -> Result<Done<BlendCause>, BlendError>;
```

- Operation crates and `scx-bodyq` take `Cx`.
- Layers 0-4 take only what they read: a `Resolution` or `Length`, `&mut Scratch` where they memo,
  and a budget and `Stop` where they iterate. Geometry users never build a `Cx`.
- **`Settings`** is a `#[non_exhaustive]` struct with `Default`.
  - It holds the random **seed** (per 07; revised after review-architecture-v2, S13). Each draw is
    `Rng::new(mix(seed, op, entity))` with the fixed mixing hash of 8.1.
  - It holds the work **budget** and the **executor** (C11).
  - A setting that changes an answer is documented as doing so, and is part of every memo key that
    answer feeds.
- The pin's fix switches are not carried over: each fix is either right (always on) or wrong
  (absent).
- **Budget and `Stop`.**
  - The budget is counted in deterministic units (solver iterations, marching steps), never time.
  - It is split per item in advance (07 rule 7). Which item runs out then never depends on
    scheduling.
  - `Stop` is polled at the same points as the budget, inside marches and chains, not only at item
    boundaries (review K3).
- **`Trace`** receives progress and structured diagnostics (review K8). It cannot affect results,
  because it receives values and returns nothing.

---

## 7. Interface style

### 7.1 Ownership and borrowing

- Reads take `&Brep` (or `Placed<'_>`) and return borrowed views or owned plain data.
- Operations take `&Brep` and return a new `Brep`. Cloning is cheap; `into_draft` is there for
  writing in place.
- Geometry is plain data. `Curve` and `Surface` are enums of owned values, `Clone`, `Send` and
  `Sync`, with no links back to bodies.
- Large payloads (B-spline nets, procedural definitions) sit behind `Arc`, together with their
  geometry cells (3.1).
- Evaluators in solver callbacks keep `&mut self` (04, 11-26).

### 7.2 Builders and options

- An operation's options are a `#[non_exhaustive]` struct with `Default` and `with_*` methods. For
  example: `FilletSpec::constant(r).with_on_item_failure(Skip)`.
- Bodies are built with `Draft` methods (`add_region`, `add_shell`, `add_vertex`, `add_edge`,
  `add_face`, `add_loop`) and checked at `finish`. There is one way to make a body.
- `LocalSolve1d`'s setters become an options value passed to a pure solve call.

### 7.3 Curve and surface kinds

**C10. Closed enums or traits, and whether users can add a kind.** This choice is revised after
review-architecture-v2 (S10, K1).

- Options:
  - (a) closed `#[non_exhaustive]` enums, as at the pin (14 curve kinds and 13 surface kinds, READ);
  - (b) trait objects;
  - (c) enums plus a `Custom` variant.
- **Chosen: (a), with extension by certified approximation, and the procedural kinds built on one
  internal mechanism.**
  - An enum lets every query use each kind's closed form, keeps records exact, and keeps foreign
    code out of our solvers.
  - The procedural kinds (offset, blend, sweep, intersection curve, volume images) share one
    `Procedural` mechanism. That mechanism holds the definition, evaluated generically over the
    scalar, and the approximant with its per-span certificates (4.1).
- A user's own surface implements a scalar-generic trait:

  ```rust
  pub trait SurfaceFn { fn eval<T: JetScalar>(&self, u: T, v: T) -> Point3<T>; fn domain(&self) -> UvBox; }
  pub fn approximate(f: &impl SurfaceFn, tol: Length) -> (BSplineSurface, Vec<SpanCert>);
  ```

  - Evaluated over interval jets, it can be certified (research 55 section 3).
  - A user who provides only an `f64` evaluator (`SurfaceFnF64`) gets `Basis::Unproven`, never
    `Certified`.
  - (b) is rejected because certification needs evaluators generic over the scalar, which a `dyn`
    interface cannot give.
- Change if: a user needs a custom kind to stay exact through operations. Because the procedural
  mechanism exists, `Custom` is then one more user of it, not an arm in every match.

**The kind set** is settled when `scx-geom` starts, from the pin's kinds and their uses. Starting
points (JUDGED):

- line, circle, ellipse, parabola and hyperbola stay separate, each with its natural
  parameterisation;
- Hermite and composite curves become NURBS if they exist only for fitting;
- the pin's `CrvInVolume` and `SrfInVolume` become images under a volume kind in `scx-geom::volume`;
- offset, blend, swept and intersection kinds are procedural and stored exactly. This answers the 04
  question on exact offset and blend records with "yes, by design".

### 7.4 Platforms, semver and features

- **Targets.** Every core crate builds for wasm32 and runs its tier-1 tests there. No core crate needs
  threads.
- **no_std is not a goal.** Once `fmath` replaces libm, `scx-num` and `scx-algo` could become `no_std
  + alloc` at small cost.
  - Change if: an embedded or GPU-host user appears.
- **Dependencies.** The core uses std only. `scx-record` has its own format, and a `serde` feature
  may come later as an optional dependency.
- **Features** are few and additive, and none changes a result.
- **Semver.**
  - Every crate is `publish = false` until the app switch.
  - At publication: fields are private or the type is `#[non_exhaustive]`, enums are
    `#[non_exhaustive]`, and traits are sealed except those meant for users (`SurfaceFn`, `CurveFn`,
    `EvalFunction`, `Carry`, `Executor`, `Trace`).
  - `cargo semver-checks` runs in CI.
  - `scx-num`'s newtypes lose their public fields now.

---

## 8. Determinism and parallelism

### 8.1 The rule

The same inputs give the same bits for any thread count, cache state, call order, or save and load.
Across platforms they should too, verified late (owner, 11-26).

Doc 07's rules apply, with these additions (revised after review-architecture-v2, S7):

- **One fixed hash.** Every hash that reaches a result is SplitMix64 mixing (as the pin's `Rng`
  uses, doc 07), in `scx-num::hash`. `DefaultHasher` and `RandomState` are not used outside tests.
- **Total sort keys.** Every sort key breaks ties by id. Otherwise the stable sort is used. An
  unstable sort with equal keys could reorder items between toolchains, since std's unstable sort
  changed algorithm in Rust 1.81.
- Reductions run in id order. `total_cmp` sorts floats. The worker error reported is the lowest
  item index's. Scratch never changes a result. `mul_add` appears only in `num::eft`.

### 8.2 The parallel model

- **Fork-join over sorted item lists.** Each worker writes its item's slot, and the merge reads
  slots in index order.
- **Fixed reduction trees.** Blocks have a fixed size that does not depend on thread count, and are
  summed in order.
- **Errors.** The error reported is the lowest item index's. The pin's HLR reports the first in time
  (READ, doc 07).
- **Cells.** Fills race safely, because cells are pure (3.2).
- **Budgets** are split per item in advance (6.3).

**C11. The executor.** This choice is revised after review-architecture-v2 (S8).

- Options:
  - (a) `std::thread::scope` inside the library;
  - (b) rayon;
  - (c) an `Executor` trait in `Settings`, with defaults.
- **Chosen: (c).**

  ```rust
  pub trait Executor: Sync { fn scope(&self, n: usize, f: &(dyn Fn(usize) + Sync)); }
  ```

  - `scx-algo` ships `Serial` (the default, and the only one on wasm without threads) and `Scoped`
    (`std::thread::scope` with a fixed worker count).
  - An app that already runs features on several threads plugs in its own pool (rayon, or the
    app's own), so the library and the app do not end up with N×M threads.
  - The pin's HLR already takes caller-provided `Workers` (READ, doc 07).
  - The external dependency list stays empty.
- Results are identical under every executor, because the model above does not depend on scheduling.
- Change if: the trait cannot express a parallel site we need, for example nested parallelism with
  work stealing.

### 8.3 Enforcement

This section is revised after review-architecture-v2 (S7).

- **Clippy `disallowed_methods` and `disallowed_types`.** Each method has an `fmath` or `eft`
  equivalent where one is needed.

  | Banned | Where it is allowed |
  |---|---|
  | `f64::mul_add` | `eft` only |
  | `f64::powi`, `powf`, `exp`, `exp2`, `ln`, `log2`, `log10`, `sin`, `cos`, `sin_cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`, `hypot`, `cbrt` | `fmath` only. `powi` is in the list because LLVM gives no precision guarantee for it; it is used today in `scx-num/src/math.rs` (READ, review) |
  | `HashMap` and `HashSet` iteration, `DefaultHasher`, `RandomState` | tests only |
  | `partial_cmp` on floats | nowhere |
  | `sort_unstable_by`, `sort_unstable_by_key` | only with a key documented as total, checked by an xtask scan for the tie-break |
  | `Cell`, `RefCell`, `thread_local!` | nowhere |
  | `Instant`, `SystemTime` | tests only |

- **An xtask scan** catches what clippy cannot: `static` items with interior mutability, and
  `OnceLock` outside cell fields and geometry payloads.
- **Tests:**
  - Every operation test runs cold (from a fresh decode of the stored form, with an empty scratch)
    and warm (cells and memos filled), and compares the stored bytes.
  - Tier 1 runs one case per parallel site at 1 and at 3 workers.
  - Tier 2 runs every case at both counts.
  - Each cell has a test that fills it on two threads (3.2).
  - Every case round-trips through the stored form and repeats, comparing bytes, which covers save
    and load.
- **Cross-platform, late:**
  - `fmath` gives the same bits on every target;
  - FMA is never contracted;
  - a golden table of record hashes is checked on x86_64, aarch64 and wasm32.
  - Until then, `libm_table` pins today's values on one target (READ).

---

## 9. Validation

Doc 02 holds the testing plan. This section is how it fits the architecture.

### 9.1 Against solidcore

- **Layers 0-4 are compared function by function.** `scx-difftest` does this today for `scx-num` and
  `scx-algo`: 324 tests, 0 mismatches, with a default tolerance of 1e-12 relative (READ, 04).
- **Bodies are compared by outcome**, through the pin-record converter in `scx-difftest`:
  - topology up to isomorphism;
  - area and volume within tolerance;
  - the two-way distance between results under resolution plus recorded accuracy;
  - validity.
- **This starts with the vertical slice** (10.2), not after every layer.
- Bit-for-bit agreement is not a goal past layer 2 (owner, 11-26). Each difference is traced to a
  pin defect, which is recorded, or to ours, which is fixed.
- **When `fmath` lands**, layer 0's agreement becomes tolerance agreement.

### 9.2 By properties

- validity of every result;
- analytic measures on fixtures;
- Boolean algebra;
- metamorphic tests: a rigid move or uniform scale of the inputs gives the moved or scaled result
  within resolution plus accuracy;
- determinism (8.3);
- kept-face counts (2.5);
- `carry` against the ported provenance tests (2.6).

### 9.3 Kept fast

- Tier 1 is change-specific and runs in seconds. `cargo xtask fast` takes about 2.2 s warm today
  (READ).
- Property tests run a handful of cases in tier 1 and thousands nightly, from the same generators.
- Fixtures are built in code by `scx-testkit` and shared.
- A regression test is added per defect fixed.

### 9.4 Performance target and gate

This section is new after review-architecture-v2 (S14).

- **Target (JUDGED).** At the app switch, the app's design corpus regenerates in no more total time
  than on the pin, and no single case is more than 1.25 times slower.
  - It is measured locally on the corpus at each layer's completion, from the vertical slice on.
  - A case that misses is a finding, recorded in `05-status.md`.
- **Gate in CI.** Each difftest and property case records `Report::work` (the deterministic budget
  units the operation used) and the count of cells filled, as reported by the test harness's own
  `FaceCell` wrappers.
  - These are compared with a committed baseline, and a rise of more than 10% fails unless the change
    states why.
  - The figures are outputs the library already returns, or test-side wrappers. The library keeps no
    counters of its own, under the rule to measure with side-channel instrumentation.
  - Comparison of work against the pin uses temporary local instrumentation, as research does now.
- **Batched evaluation is out of the first design.**
  - Reasons: it changes every evaluator's signature; and how much of scx's time evaluation takes is
    unmeasured until the slice runs.
  - The plan keeps room for it: `eval_many(&self, params: &[f64], out: &mut [Point3])` can be added
    to `Curve` and `Surface` later without breaking anything, with a default that loops.
  - Change if: the slice's profile shows evaluation above 30% of Boolean or tessellation time.

---

## 10. Order of work

The app switches in one go at the end, with no adapter. This order is revised after
review-architecture-v2 (S2): a vertical slice comes first, and bodies are compared with the pin
early.

### 10.1 Changes to scx-num and scx-algo under the rules

1. **Retire the `Tolerances` struct** for `Resolution` plus named constants. Re-express the `tol.rs`
   free functions over `Resolution` and `Accuracy`. `Tolerances` survives only in the difftest
   facade.
2. **Private newtype fields**, with the arithmetic each newtype needs. Remove `AngleTol` (4.2).
3. **Remove sentinels** (`UNDEF_DOUBLE`, `Vector3::UNDEF`, `USE_DEFAULT`, `UNINIT_TOL`,
   `INFINITE_PARAMETER`, READ) in favour of `Option`, typed defaults or named bounds.
4. **Add `fmath`**, covering every function in the 8.3 list.
5. **Move `Cert`, `Claim`, `Method`, `Basis` and `Recorded` into `scx-num`**, and add the fixed hash
   and `Rng`.
6. **Solver options:** `LocalSolve1d` becomes an options value and a pure call.
7. **`hash_table`:** review its iteration order and decide whether it stays public.
8. **Add the box tree and the `Executor`** to `scx-algo`.
9. **Error strings** stay matched to the pin while the difftest compares them (5.3).
10. **The recorded defects** (interval overflow to NaN, Ferrari's quartic on clustered roots) stay
    pinned and reported while layers are being moved, per 04 row 79. Each is fixed after its layer
    is complete, as its own change, with the difftest recording the difference on purpose.

### 10.2 Layers in order

| Step | Work | Done when |
|---|---|---|
| 1 | 10.1 | `cargo xtask fast` clean; difftest 0 mismatches, or each explained |
| 2 | **Vertical slice, geometry:** `scx-nurbs` evaluation and knots; `scx-geom` for line, circle, ellipse, B-spline curve, plane, cylinder, cone, sphere, torus, B-spline surface; `scx-geomq` closed-form analytic intersections and projection | function-by-function difftest on those |
| 3 | **Vertical slice, body:** `scx-brep` (regions, sides, cells, `Changes`, `carry`, stamp); the provenance and history tests ported; `scx-record`; the pin-record converter | round trip keeps ids and bytes; cells empty after both edit paths; provenance tests pass |
| 4 | **Vertical slice, end to end:** `scx-bodyq` mass and classification; `scx-make` primitives and extrude; `scx-boolean` on analytic bodies; `scx-tess` | **the end-to-end difftest by outcome runs from here on**; first performance measurement (9.4) |
| 5 | Widen geometry: all NURBS work, fitting with per-span certificates, procedural kinds, volume kinds, B-spline SSI, `scx-planar`, `scx-sketch` | difftest per function; certified bounds checked |
| 6 | Widen bodies: full `scx-bodyq` (validity check, contact, continuity), `scx-stitch`, `scx-step` | the STEP corpus imports and passes the check |
| 7 | `scx-boolean` full, `scx-make` full, `scx-repair` | Boolean suite, algebra properties, difftest by outcome |
| 8 | `scx-blend`, `scx-taper`, `scx-local` | each family by outcome, kept-face counts |
| 9 | `scx-offset`, `scx-deform`, `scx-sheetmetal` | as above |
| 10 | `scx-hlr`, `scx-mesh`, `scx-dxf` | drawings, mesh round trips, DXF corpus |
| 11 | Helpers: `scx-fates`, `scx-features`, `scx-frames`, `scx-teeth`, `scx-weld`, `scx-cadir` | every row of 1.3 has a home that passes its tests; the app's design corpus runs on solidcorex |
| 12 | Cross-platform | `fmath` hashes equal on x86_64, aarch64 and wasm32 |
| 13 | App switch | in one go, checked against the 1.3 inventory and the 9.4 target |

- Steps 5 and 6 can run in parallel, as can the families within 8, 9 and 10.
- The design corpus can run partly from step 7, on the features whose crates exist. It does not wait
  for step 11.

---

## 11. Faults avoided and designs kept

### 11.1 Faults in solidcore's design that this avoids

| Fault | Evidence | Here |
|---|---|---|
| Ambient mutable state on the kernel (memos in `Cell`/`RefCell`, a result-changing `blend_net_tolerance`, size scalars read by later calls) | research 03 | `Cx`; no session (6) |
| Process statics and thread-locals (idle pool, fix switches, hook) | READ, solidcore-ops | none; `Executor` in `Settings` (8.2) |
| 26 fix switches | research 03 | none (6.3) |
| `&mut Kernel` passed through 89% of the time | research 03 | layers 0-4 take only what they read |
| One crate with 19 of 26 areas in one cycle; operation areas referring to each other 38 times | research 49 | layered crates, couplings resolved (C3) |
| One error type referenced from 559 files | research 49 | an error per crate with typed causes (5) |
| Geometry pointing at topology | READ, curve and surface `p2.rs`; research 01 | geometry is plain data (7.1) |
| Lazy writes through shared references; trims made on read raise tolerances, so results depend on order | research 02, 27, 47 | pure cells reset on write; operations make the trims of what they create (3.2) |
| The validity check writes the body it checks | research 12, 20 | `check` takes `&Brep` (2.3) |
| The tree build raises gaps and welds corners | research 30, 45 | trees are pure cells |
| Trees rebuilt on every query; incremental updates going stale | research 26, 30 | cells, rebuilt never patched (3.2 rule 7) |
| Every copy is deep and drops face caches 5-160 times the body's size | research 51 | shared records with cells; `shed_cells` (2.5, 3.4) |
| Reads that cannot take `&Brep` | research 04, 51 | by construction |
| Operations rewrite what they do not change | research 51 | the 11-26 rule, `assert_kept`, id-keeping `transform` (2.4, 2.5) |
| Four uses per manifold edge; use records 71-80% of bytes | research 51 | two uses inside face records, two sides per face (2.1) |
| Per-body tolerances, `matched_tolerances`, "the result takes the larger" | research 31, 54 | one resolution, recorded accuracies, a check stamp (4) |
| Stored form not a fixed point (86 of 474 after one trip) | research 41 | exact stored form that keeps ids (2.2, 10.2) |
| Attributes and provenance carried by notify callbacks inside the kernel | READ, `geom/attribute.rs`, `solidcore-ops/src/provenance.rs`; research 38 | `Changes` events and `carry` over app tables (2.6) |
| Sorts with `partial_cmp(..).unwrap_or(Equal)`; HLR's first-in-time error; sums in list order | doc 07 | total keys, lowest-index error, fixed trees (8) |
| Sentinel numbers | READ, scx-num as moved | `Option` and typed values (10.1) |
| Results that depend on platform libm | research 53 | `fmath` (8.3) |
| Proofs repeated on identical inputs | 04 section 5 | memos in `Scratch`; certificates kept on geometry payloads (3, 4) |
| A blend net extended without recertifying (`grow_face`) | 04, 11-26 trim-gap row | a net edit makes a new payload with new bounds (4.1) |

### 11.2 solidcore designs kept on purpose

| Kept | Why | Evidence |
|---|---|---|
| The algorithms: closed forms, fitting, marching intersection, blend solvers, healing | the knowledge is the value | owner, 11-26 |
| The region-labelled cellular merge for Booleans | it works, and the two-sided faces of 2.1 hold it | READ, `boolean/merge/p4.rs` |
| Dense ids never reused | cheap and stable within a line of descent | research 51 |
| Recorded gaps rather than recomputed ones | removing gap storage made later passes recompute | research 20 |
| Interval arithmetic and Taylor models for certified bounds | pure, already moved; certified trims depend on them | research 53, 55 |
| The ops layer computes no geometry, and errors are data | kept as `scx-features`' rule | READ, solidcore-ops |
| One natural parameterisation per kind | parameters keep their meaning | doc 07 |
| The mesher and HLR working over plain data | already parallel and safe | research 03 |
| The evaluator error classes | they decide solver control flow correctly | 04, 11-26 |
| A deterministic work budget (fuel) and cooperative cancellation | sound; moved into `Settings` and `Cx` | READ, `solidcore-ops/src/fuel.rs`, `cancel.rs` |
| A caller-provided worker pool for HLR | generalised to `Executor` | doc 07 |
| The 1e-5 mm zone value, as the resolution for the default size class | apps' models were built under it | READ, `tol.rs` |

---

## 12. Differences from earlier documents

Each of these needs a 04 row when this design is accepted.

1. **04 row 73, "Tolerance functions take a `Tolerances` value".** Kept for the difftest era. The end
   state retires the struct (10.1 item 1).
2. **04 row 72, `NumError` strings matching solidcore.** Kept while the difftest compares them, then
   free (5.3).
3. **04 row 79, "Defects found while moving are pinned by a test and reported, not fixed in the
   move".** Kept. Fixes come after a layer is complete, as their own changes (10.1 item 10). The
   earlier draft scheduled them in step 1. This is revised after review-architecture-v2 (S13).
4. **04 row 42, "no per-body tolerance or angle tolerance".** Read as "no angle tolerance at all".
   `AngleTol` is removed, and feature thresholds become operation parameters (4.2).
   - Research 54 change 5 had kept an angle in settings for feature thresholds. This design does not.
   - Change if: the owner reads row 42 as "no per-body angle tolerance" only. A settings-level feature
     threshold would then be allowed, but parameters are still preferred: each operation's threshold
     means something different.
5. **04 rows 42 and 11-26: "no per-body tolerance", against the resolution stamp on `Brep` (C7).** The
   stamp is a check copy of the document's resolution, not a tolerance, and cannot be set apart from
   the document except by `rebase`.
6. **Doc 07: the seed in `Settings`.** Kept (6.3). The earlier draft had moved it to `Document`.
   - Review K2 suggested a fixed constant instead. This design keeps 07.
   - Change if: two apps or documents with different seeds giving different results on the same
     geometry becomes a reported problem.
7. **Doc 07: `BodyId` assigned by the session.** There is no session and no `BodyId` (2.2, 6.2).
8. **Doc 08: settings as "the immutable per-session value".** Settings is a value passed in `Cx`.
   There is no session, though it is still immutable for a call.
9. **Doc 09's per-size `resolution_for(size)`.** Already overtaken by the 11-26 owner rule. The size
   rule now picks the size class at creation only (4.3).
10. **Research 51: "the stored form compacts ids".** Replaced: the stored form keeps ids, and
    compaction is explicit (2.2).
11. **The earlier 01's "brep and topo as one library first".** Replaced by `scx-brep` and `scx-bodyq`
    (C2).
12. **Doc 08 glossary.**
    - `FaceSide` is kept, as an index into a face's `sides`.
    - `LoopSide` and `VertexUse` are not needed.
    - `EdgeUse` is kept.
    - `Region`, `Placed`, `Changes`, `carry` and `Unsupported` are new terms.
13. **04 open questions given JUDGED answers here:**
    - placement: none, with placed operands (2.4);
    - the fields face cells read: an edge's geometry part and its vertices, never its adjacency
      (3.2);
    - use layout: loop lists, two sides per face, radial order stored (C4);
    - the outcome type: `Changes` and `carry` in the core, fates in a helper (2.6);
    - exact offset and blend records: yes, through procedural kinds (7.3).
