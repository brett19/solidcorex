# Review: architecture v2 (`01-architecture.md`, uncommitted rewrite)

Adversarial review of the solidcorex architecture that replaces the in-place restructuring plan.
The brief was to find wrong choices, missing pieces and choices that will be expensive to reverse.
Wording is not reviewed.

**Labels.** READ means read in solidcorex at its current state or in solidcore at the pin
(03e16c9ee7). JUDGED means reasoned. Pin paths are relative to `solidcore/crates/`. Section
references (§) are to `docs/01-architecture.md` unless another document is named.

**Severity.**

- **blocking**: must change before `scx-brep` or the first operation crate is written on it.
- **should change**: wrong or incomplete, and cheaper to fix now than later.
- **consider**: a real risk, but the design can go ahead.

---

## Blocking

### B1. A face has one side, so a face between two regions cannot be stored, and the pin's Boolean cannot be ported

- **Claim attacked.** §2.1 and C4: "A face has one side record". `Face` holds `outward: bool`;
  `Shell` is "a connected set of faces"; there are no faceuses. §12 item 7 drops `FaceSide` and
  `LoopSide`.
- **Evidence.**
  - READ: the pin's Boolean is a merge into one cellular body whose regions are labelled and then
    kept or deleted. `solidcore/src/boolean/merge/p4.rs:1-58` describes "region bookkeeping", "a face
    between the infinite region and a solid region", "a region that a deleted face joins to
    another", and nested-void handling. It reads `MergeRegionData` per region
    (`p4.rs:1629-1695`; the attribute is defined at `solidcore/src/geom/attribute.rs:561` on).
  - JUDGED: a face that separates solid region R1 from solid region R2, or a lamina face inside a
    solid, needs a region (or shell) per side. One `outward: bool` and membership of one shell
    cannot say what lies on the other side. The intermediate state of the merge cannot be
    represented, so the "keep the algorithms" rule (04, 11-26) cannot hold for Booleans. The same
    lack blocks multi-region bodies (multi-material, cellular decomposition, sections kept as
    regions) that a CAD library is expected to expose.
  - JUDGED: the memory fault named against the radial-edge structure (71-80% of bytes, research 51)
    comes mostly from four edge uses per edge, the gap caches stored inline and the trims. Research
    51 §1 says so: "half-edges or plain loop-edge lists and no inline caches would roughly halve the
    topology". Two side references per face cost 8 bytes. Dropping them is not justified by the
    measured fault.
- **Alternative.** Keep loop lists inside the face record (C4 (c)), but give each face two sides:
  `sides: [ShellId; 2]`, with the region reached through the shell. Make the infinite region an
  explicit `RegionId(0)`. Sheets have both sides in the same region, and cellular bodies are
  representable. Store the radial order as the order of `Edge::faces` (see S11 on keeping it out of
  invalidation), not as a derived cell: the merge walks it on every edge it splits, and does so
  while the body is a `Draft`, when no finished cell exists.

### B2. The resolution is not on the body, but `None` accuracies only mean something relative to it

- **Claim attacked.** C7 (b): the resolution lives only in `Document` and "Combining two bodies made
  under different documents is the caller's business". §4.1: `None` means "within the resolution".
- **Evidence.**
  - JUDGED: every `Option<Recorded>` that is `None` is a claim relative to a resolution the body does
    not carry. Take a body built under 1e-5 and passed with a `Cx` whose document says 1e-7. Its
    edges, which are within 1e-5, are now read as within 1e-7. Coincidence tests
    (`max(res, acc_a, acc_b)`, §4.2) then miss real contacts. This fails silently and gives wrong
    topology, with no error. It is the exact failure research 54 calls "the one way this model can
    give a wrong answer" (a recorded accuracy that is too small, `54-tolerance-middle-path.md` change
    4).
  - JUDGED: bodies cross documents routinely: copy and paste, a library part, a STEP import into an
    open document, a body handed to a helper crate, and the difftest itself. `scx-record` stores the
    document "with the bodies" only when they are saved together.
  - JUDGED: the cost of reversing this later is a change to the record format and to every
    operation's checking. Today it costs one field.
- **Alternative.** `Brep` carries `resolution: Resolution`: the resolution its records were made
  under. It is a fact about the data, not a tolerance. Then:
  - `Draft::finish` stamps it from the `Document`;
  - every operation and `scx-bodyq` read checks `body.resolution() == cx.doc.resolution()` and, on a
    mismatch, returns a typed `Refused(ResolutionMismatch { body, document })`;
  - `scx-heal::rebase` is the only way to change it;
  - `scx-record` stores it per body.

  This keeps one resolution per document (the owner rule) and makes a mix a typed error instead of
  a wrong answer. It is the "Change if" of C7, taken now.

### B3. `Changes` cannot carry naming or application data, and helper data keyed by `Weak<FaceRecord>` dies on every modification

- **Claim attacked.** §2.6: `Changes { replaced, removed, created: Vec<(FaceId, Origin)>, .. }` is
  "the mechanism", and fates and naming are a helper over it. §3.1, last row: "Helper data | meshes,
  names, display lists | keyed by `Weak<FaceRecord>` | the key dies with the record".
- **Evidence.**
  - READ: the app's naming and history run on attributes that the kernel carries through each edit
    event: split, merge, copy and **coincident** (`solidcore-ops/src/provenance.rs:1-35`, trait
    `Provenance` with `split`, `merge` and `coincident`; research 38 lines 90-102: names are built
    from origins carried by notify handlers).
  - READ: the pin carries general attributes (colour, tags, application values with their own
    propagation rules) at `solidcore/src/geom/attribute.rs:561-760`. The old 01 put "attributes" in
    `brep` (git `HEAD:docs/01-architecture.md` §2, row 5). The new design drops them without naming
    a fault.
  - JUDGED: `Origin` is one value per created face. A union in which faces from both operands
    coincide and merge needs two origins on one face. A face kept by id that absorbs an operand-1
    face has no entry at all. `replaced` does not say whether the geometry changed or only the
    boundary, which research 51 §6 says the app needs (`Modified { geometry: bool }`). Edges are an
    undefined `EntityChanges<EdgeId>`, although fillets name edges.
  - JUDGED: a name or a colour keyed by `Weak<FaceRecord>` disappears whenever the face is trimmed,
    because any change replaces the record. That is exactly when a name must survive. The key is
    right for meshes and wrong for names.
- **Alternative.**
  - `created: Vec<(FaceId, Origins)>` with `Origins` a sorted, non-empty list.
  - Add `merged: Vec<(FaceId, Vec<FaceId>)>` and `coincident: Vec<(Operand, FaceId, FaceId)>` (a
    face of one operand landing on one of the other).
  - Add `replaced: Vec<(FaceId, Touch)>` with `Touch::{Boundary, Geometry}`.
  - Give edges the same shape, with vertices kept as they are.
  - Specify that every operation fills these as events while it edits, not reconstructed after.
  - Put a small, generic, deterministic propagation in the core:
    `fn carry<T>(table: &IdMap<FaceId, T>, changes: &Changes, rule: &impl Carry<T>) -> IdMap<FaceId, T>`.
    It gives application data (attributes) a home with no store inside the body.
  - Key names and attributes by id plus that propagation. Key meshes by record pointer.

  Test it by porting `provenance.rs`'s tests against `Changes` before any operation crate exists.

### B4. There is no inventory of what the app switch needs; a large part of the pin has no crate

- **Claim attacked.** §1.1 crate table and §10.2 order of work, under the rules "a library exposing
  all the mechanisms needed to write CAD software" and "the app switches in one go" (04, 11-26).
- **Evidence.**
  - READ: the app talks to solidcore only through `solidcore-ops` (research 38 line 170, "one
    seam"). That crate has 84 modules and 485 `pub fn`s at the pin.
  - READ: kernel areas with no solidcorex crate or row:
    - `develop` (sheet metal flat pattern and fold, 6,950 lines, research 49 line 419);
    - `prism` and its planar core (profiles, regions read from a sheet, members, gussets);
    - `cell_grid`, `hatch`, `teeth` (splines and hubs), `weld_symbol`;
    - `mesh_reconstruction` (mesh to B-rep) and `mesh_reduce`;
    - the volume kinds (`b_spline_volume`, `bend_volume`, `twist_volume`, `SrfInVolume`,
      `CrvInVolume`: global deformation);
    - `eval::constraint` (dual-number residuals for sketch constraints, re-exported by
      `solidcore-ops/src/constraint.rs`);
    - `nurbs::gordon` (curve networks, `solidcore-ops/src/network.rs`);
    - untrim and extend.
  - READ: exchange beyond STEP solids, also with no crate:
    - STEP product structure and placements (`solidcore-ops/src/step_assembly.rs`);
    - DXF (`solidcore-ops/src/dxf.rs`);
    - CADIR (`solidcore-cadir`);
    - the native file's history and metadata records (`solidcore-record` lib doc);
    - mesh export.
  - READ: research 49 also lists `defeature` as an operation crate (line 156). No row says whether
    `scx-local` covers it.
  - JUDGED: with no inventory, the "one switch" cannot be planned, and the design corpus (the owner's
    validation, 04 row 45) can run only after `scx-features` at step 12, which is the last step
    before the switch.
- **Alternative.** Add to 01, before step 2, a table mapping every pin crate, every kernel area and
  every `solidcore-ops` module to a solidcorex crate, to a helper, or to "moves into the app". Name
  the crates this implies. At least these are needed:
  - `scx-sketch`: 2-D profiles, regions from loops, constraint residuals and a solver over dual
    numbers;
  - `scx-sheetmetal` (develop);
  - `scx-mesh`: reconstruction, reduction, STL/3MF;
  - `scx-deform`: volumes;
  - a STEP assembly layer;
  - `scx-dxf`.

  Order them in §10.2, and give each a "the app's features that need it" column, so the switch has
  a checklist.

### B5. "Invalidation is impossible" is not true as specified

- **Claim attacked.** §3.1 ("impossible: the record never changes; an edit makes a new record with
  empty cells") and §3.2 rule 1. C4's fallback: "cache the mate position as a cell on the face".
- **Evidence (JUDGED, each a concrete path to a stale cell).**
  1. §2.3 and C5 say a draft "calls `Arc::make_mut` on each face record it changes", and
     `into_draft` "writes in place where uniquely owned". `make_mut` clones the record, filled
     `OnceLock`s included, because §2.5 itself notes that `OnceLock<T>: Clone` copies the value.
     In place, the cells are simply kept. Either way the edited record keeps cells computed from its
     old content, unless the draft clears them. Nothing in the design says it does.
  2. "Radial order" is listed as an edge cell (§3.1). It reads the adjacent faces' surfaces and
     normals, which breaks rule 1 (an edge cell reads its own row). The draft marks faces on changed
     edges, not edges on changed faces. Replacing an adjacent face's surface therefore leaves the
     radial order stale.
  3. The C4 fallback, a mate position cached on a face, reads the other face's loops. When the
     other face is replaced, this face is not marked. The fallback is a stale-cache bug as written.
  4. Edge cells that read vertex data (an edge box widened by vertex accuracy) need vertex writes
     to mark edges. Only faces are marked.
  5. §3.1, body cells: "a chunked body may rebuild them incrementally". Incremental rebuilds are
     where the pin's staleness came from (research 26, 30).
- **Alternative.**
  - Put the cells in a separate field, `cells: FaceCells`. The only mutable accessor,
    `Draft::face_mut`, replaces it with `FaceCells::default()` on every call. Do the same for edge
    and vertex rows.
  - Fix the read set per cell kind in its type: `trait FaceCell { fn fill(rec: &FaceRecord, e:
    &EdgeTable, v: &VertexTable) -> Self; }`, with no `Cx`, `Scratch` or `Settings` reachable. This
    makes rule 1 hold by construction instead of by review.
  - Make every cross-entity derived value (radial order, mate positions, vertex stars) a body cell,
    rebuilt at `finish`. Inside a draft, maintain it as part of the draft.
  - Add a test that edits a record with filled cells through both `edit()` and `into_draft()` and
    checks that the cells are empty.

---

## Should change

### S1. Rigid placement on the body (§2.4): change to no placement, with placed operands

- **Evidence.**
  - JUDGED: every geometric parameter of every operation must be mapped into body coordinates:
    split planes, taper pull directions, sweep paths, sketch profiles for extrude-cut, rays, pick
    points, offset directions, section planes. Once 15+ operation crates are written frame-aware,
    the "Change if" (frame mistakes recur) fires too late to reverse cheaply.
  - JUDGED: the benefit is narrow. The workload measured is "a transform keeps 0%" (research 51
    §3). How often the app transforms a body without then combining it was not measured. Pattern
    instances, the dominant moved bodies (research 38: 82% of pattern bodies are intermediate), are
    Boolean tools, and C-§2.4 transforms the tool's geometry into the target's frame anyway. Its
    cells are lost either way.
  - JUDGED: there are other costs:
    - boxes reported in model coordinates are the box of a rotated box, so they are looser;
    - two representations of the same model (placed, or baked) give different bits downstream, so
      the metamorphic tests (§9.2) compare only within tolerance;
    - the stored form and STEP must bake or carry it;
    - assemblies need instance placements anyway, which gives two placement levels.
- **Alternative.**
  - No placement on `Brep`. `transform(&Brep, Rigid) -> Brep` rewrites geometry but keeps every
    id, so names and `Changes` are trivial. Cells refill lazily.
  - Operations that take a second body, and `scx-bodyq` reads, accept `Placed<'_> { body: &Brep,
    frame: Frame }`. That gives patterns and assemblies the O(1) move where it pays, without a frame
    on every read path.
  - Keep the merge's own temporary origin shift (`p4.rs:27`) as an internal precision device.

### S2. The order of work keeps end-to-end bodies away for months

- **Claim attacked.** §10: "Each layer is finished … before the next leans on it". Steps 2-4
  (`nurbs`, `geom` with all kinds procedural included, and `geomq` with SSI) come before any body.
- **Evidence.**
  - READ: the pin's `curve` (77k lines), `surface` (164k) and `intersect` (83k) are the bulk of
    the kernel (old 01 §2), plus the `ssi` crate.
  - JUDGED: body-level difftest by outcome (§9.1) starts at step 7. The brep interface meets its
    first real client, a Boolean, at step 9, so interface faults in layers 5-8 are found last.
  - JUDGED: §1.1 makes `scx-brep` depend on `scx-geomq` because cells need projection. That
    dependency is what forces the whole of geomq to come first.
- **Alternative.**
  - A vertical slice first: num, algo, the NURBS evaluation kernel, and geom for plane, cylinder,
    cone, sphere, torus, line, circle and B-spline. Then the analytic closed-form intersections, brep
    with the pin-record converter, bodyq mass and classification, make primitives and extrude,
    a Boolean on that subset, and tess.
  - Widen kinds and algorithms afterwards, with the end-to-end difftest running from week one.
  - Decouple brep from geomq so the slice can start early: make cells an extension point
    (`FaceCell` trait as in B5, implemented in `scx-bodyq` and `scx-tess`, stored in a fixed-size
    typed slot table on the record), so `scx-brep` needs only `scx-geom`. This also gives meshes a
    pure, shared home in place of the `Weak` side table.

### S3. The layer table contradicts measured couplings it does not say how to break

- **Claim attacked.** §1.1 puts `scx-make` (primitives, sweeps) below `scx-boolean`, and says "no two
  layer-9 crates know each other".
- **Evidence.**
  - READ: research 49 lines 158-160 and 243 count the couplings:
    - `primitive` → `boolean`: 23;
    - `sweep` → `boolean`: 18;
    - `offset` → `blend`: 36 (62 in the area table);
    - `offset` → `heal`: 18;
    - `blend` → `boolean`: 29.
  - READ: §1.1 gives `scx-step` the dependency "0-7, heal", but `scx-heal` is layer 9 and depends on
    0-8. Reading STEP therefore pulls `scx-boolean` and `scx-make`, which breaks §1.2's "adopt only
    the pieces you want" for exchange.
- **Alternative.**
  - For each measured coupling, state the shared piece that moves down, and where. For example: the
    sweep and primitive Boolean uses go to `scx-boolean` callers in `scx-make` placed above
    boolean, or to a `scx-merge` core below both; offset's blend pieces go to a `scx-rollball`
    solver crate.
  - Split `scx-heal` into `scx-stitch` (sew, gap closing; layer 6-7, no Booleans) and the
    Boolean-using repairs. `scx-step` then depends on `scx-stitch` only.

### S4. The error taxonomy has no class for valid input that the algorithm could not handle

- **Claim attacked.** §5.1: `Refused` is "only for input that is not valid"; `Defect` is "our bug".
- **Evidence.** JUDGED: the commonest real failure of a fillet, offset or Boolean is a case the
  algorithm does not cover: a vertex blend it cannot patch, a degenerate SSI branch. That is neither
  invalid input nor a broken invariant. Authors will file it under `Refused`, which tells the app its
  input is wrong (a lie), or under `Defect` ("apps report it, never handle it"), which forbids a
  fallback. Running out of the work budget (§6.3) has no variant either. The shared `ErrorKind` lists
  `Impossible`, but no variant maps to it.
- **Alternative.** Add `Unsupported(Cause)`: valid input that the method could not complete, which
  the app may retry with other parameters. Add `Exhausted { budget }` for the work budget. Keep
  `Refused` strictly for invalid input, and map `ErrorKind` one for one.

### S5. Partial success (C8): items interact, and the semantics of `Skip` are undefined

- **Claim attacked.** C8 (c): by default, report all failing items; with `Skip`, apply the rest.
- **Evidence.**
  - JUDGED: blend items are not independent. Whether edge 3 can be filleted depends on which
    neighbours are filleted (chains, vertex blends, setbacks). Removing the three failures can make
    another item fail, or make a failed one succeed. Reporting "the three causes" needs the
    operation to continue past a failure, which the pin does not do and which may be meaningless
    after a failed corner.
  - Booleans, offsets of whole bodies and tapers have no items, so `Items` applies only to a few
    operations.
- **Alternative.** Keep (c), but define it:
  - The default returns the first failure in item order, plus any others found without extra work.
  - `Skip` is a fixpoint: drop the failing items in index order, rerun, and repeat until stable or
    a stated round cap is reached. It reports each dropped item with its cause and the round. This
    is deterministic and documented as costing up to k reruns.
  - State that items are chains (for blends), not edges.
  - Make `Done<C>` generic over the crate's cause type.

### S6. Compacting ids in the stored form makes a reloaded body give different bits

- **Claim attacked.** §2.2: "The stored form compacts them and returns an old-to-new map". §8.1:
  "reductions in id order"; random draws keyed by entity.
- **Evidence.**
  - READ: research 39 line 19 measures that face order alone moves mass in 173 of 474 bodies.
    Compaction renumbers, so id-ordered sums, id-keyed draws and lowest-index errors all change
    between the live body and the reloaded one.
  - JUDGED: the owner's "same operation executed twice yields the same answer" should hold across
    save and load. A regeneration after reopening a file would otherwise differ from the live
    session. The fixed-point requirement (§10.2 step 6, "after one trip") hides this, because the
    first trip is exactly where the change happens.
- **Alternative.** The stored form keeps ids as they are, with tombstone runs written as counts. At
  app sizes (p99 94 faces) the cost is negligible. Offer compaction as an explicit operation with an
  old-to-new map, never as a side effect of saving.

### S7. Determinism enforcement has gaps

- **Claim attacked.** §8.1 and §8.3.
- **Evidence and alternatives.**
  - **The hasher is unnamed.** "`Rng::new(hash(seed, op, entity))`": std's `DefaultHasher` is
    documented as not stable across Rust releases. Name a fixed hash (SplitMix64 mixing, which the
    pin's `Rng` already uses, 07 line 362) and ban `DefaultHasher` and `RandomState` outside tests.
  - **The lint list is short.** It omits `f64::powi` (LLVM's `powi` gives no guarantee on
    precision; READ: used in `crates/scx-num/src/math.rs:412` and `:1282`), `log2` and `exp2`, both
    libm (`math.rs:412`), and `f64::sin_cos`. Add them to `disallowed_methods`, with `fmath`
    equivalents.
  - **Sort stability across toolchains.** The std unstable sort changed algorithm in Rust 1.81. For
    "agree across builds", `sort_unstable_by` with keys that compare equal on distinct items may
    reorder between toolchains. Require total keys (break ties by id) or stable sorts, as a clippy
    lint on `sort_unstable_by*`.
  - **The budget is not split.** 07 rule 7 says "a shared budget is split per item in advance"; §6.3
    does not. If workers draw from one budget, which item runs out depends on scheduling. State the
    split.
  - **The tests are weak.** "Runs twice and compares bytes" (§8.3) runs twice in the same process,
    so the second run sees cells the first filled. Specify one cold run (fresh decode) and one warm
    run. The thread-count comparison runs only in tier 2 with `parallel` on. Run one tier-1 case per
    parallel site at 1 and 3 workers, so the fast loop catches scheduling dependence.
  - **Purity is a review rule.** Enforce it by the fill signature (B5).

### S8. Own threading (C11): define the executor interface now

- **Evidence.**
  - JUDGED: `std::thread::scope` creates OS threads on every call. An operation with several
    fork-join phases pays tens of microseconds per spawn on operations of 5 ms upward (research 51).
  - JUDGED: worse, an app that already evaluates features on several threads (research 38: the
    `Send` model eval) gets N×M threads. The fallback ("take a `&dyn Spawn` … if an app needs to
    share one pool") is the case from day one.
  - READ: the pin's HLR already takes a caller-provided `Workers` (07 line 400).
- **Alternative.** Put a `trait Executor { fn scope(&self, n: usize, f: &(dyn Fn(usize) + Sync)); }`
  in `Settings` from the start. Ship a serial default and a `std::thread::scope` implementation, and
  let apps plug in rayon. This keeps the external list empty and avoids later churn through every
  parallel site.

### S9. Caches intrinsic to geometry have no home

- **Claim attacked.** §3.1 lists face, edge, body, memo and helper data only. §8.3 flags `OnceLock`
  outside cell fields.
- **Evidence.**
  - READ: the old 01 §4 had "Intrinsic to a geometry object | a surface's plain tree, a curve's
    analysis | a fill-once cell in the surface or curve".
  - JUDGED: a geometry-only user (§1.2) projecting many points onto one B-spline surface rebuilds its
    tree every call, or must hash the whole control net into `Scratch` ("keyed by the bits of every
    input"), which costs per lookup in proportion to the net. Several faces sharing one `Arc`'d
    surface also rebuild the same tree once per face.
- **Alternative.** Allow fill-once cells inside the `Arc`'d payload of geometry kinds (a B-spline net,
  a procedural definition). They are pure, because the payload is immutable, and they meet rule 3.
  List them as a sixth row of §3.1.

### S10. `Cert` sits above the crates that produce it; a black-box `SurfaceFn` cannot be certified

- **Claim attacked.** §4.1: `Cert` is "in `scx-geomq`". §1.1: `scx-nurbs` gives "fitting and
  approximation with a certificate". C10: `approximate(&f, tol) -> (BSplineSurface, Cert)` in
  `scx-geom`.
- **Evidence.**
  - JUDGED: layer 2 and layer 3 cannot name a layer-4 type.
  - READ: research 55 §3 items 1-2 says certification needs "one surface evaluator, generic over
    that [interval jet] scalar". A user's `SurfaceFn` over `f64` gives only sampled bounds, which
    research 54 change 4 calls the unsafe case.
- **Alternative.**
  - Move `Cert`, `Claim`, `Method` and `Basis` to `scx-num` (where `certified.rs` already is) or to
    `scx-nurbs`.
  - Make `SurfaceFn` generic over the scalar (`fn eval<T: TayScalar>(..)`), and certify only through
    that route. A user who supplies only `f64` gets `Basis::Unproven`, never `Certified`.
  - Store certificates per span (research 55 §3 item 4).
  - Pick one home for a surface's `Recorded`: on the procedural kind (§7.3) or on the face record
    (§4.1). The design names both.

### S11. Conservative marking includes the edge's face list, so neighbours lose cells on every adjacency change

- **Claim attacked.** §3.2 rule 2 ("a change to any field of an edge or vertex row replaces the
  record of every face that uses it") together with `Edge` holding "the sorted faces that use it"
  (§2.1).
- **Evidence.** JUDGED: splitting a face, or replacing a face by a new id, rewrites the face list of
  every edge on its boundary. That replaces every untouched neighbour across those edges, although
  no cell of the neighbour reads the face list. This works against the kept-face goal (04, 11-26;
  `assert_kept`) in every Boolean and fillet.
- **Alternative.** Split the edge row into a geometry part (curve, interval, end vertices, accuracy),
  whose change marks the faces, and an adjacency part (face sides in radial order, B1), whose change
  marks nothing. Cells may not read the adjacency part, and the typed fill signature (B5) makes that
  checkable.

### S12. Undo and history keep every old record's cells alive

- **Evidence.**
  - JUDGED: §6.2 puts undo in old `Brep` values. Face caches are 5-160 times the body (research 51
    §1). Every replaced record in the history keeps its filled cells for as long as any old body
    holds it.
  - JUDGED: an app keeping a body per feature (regeneration caching) therefore holds every
    generation's trees. Fill-once cells give no way to shed them.
- **Alternative.** Provide `Brep::shed_cells(&mut self)`. It replaces the cells of uniquely owned
  records with empty ones, and leaves records that are shared in place. Alternatively, make large
  cells `Arc` values that a helper can drop under memory pressure. State the policy in §3.

### S13. Section 12 omits differences from the owner's rows and the conventions

- **Evidence.**
  - READ: 04 row 79 says "Defects found while moving are pinned by a test and reported, not fixed in
    the move". §10.1 item 5 schedules the fixes in step 1.
  - READ: 04 row 42 says "no per-body tolerance or angle tolerance". §6.3 keeps `AngleTol` in
    `Settings`. Research 54 change 5 supports keeping it for feature thresholds, but that reading
    needs its own row.
  - READ: 07 line 431 puts the seed in `Settings`; §6.2 puts it in `Document`.
  - READ: 08 line 161 calls settings "the immutable per-session value".
- **Alternative.** List all four in §12, each with its 04 row.

### S14. No performance target or gate

- **Evidence.**
  - READ: the old 01 §9 had a speed plan (batched evaluation, closed forms, work reuse). The new
    design has none.
  - JUDGED: the owner's standing rule asks for before-and-after counts on every change. A rewrite
    with no stated parity target against the pin can end slower with no row failing.
- **Alternative.**
  - Add the target: no corpus case slower than the pin by more than a stated factor at the switch.
  - Add the gate: per-layer counted work (solver iterations, cells filled) against the pin on the
    difftest cases, in CI.
  - Keep the old plan's batched evaluation as a stated later option.

---

## Consider

- **K1. C10: the argument against `Custom` is partly wrong.** §7.3 says a custom kind would need "a
  generic slow path". The procedural kinds (offset, blend, swept, intersection) already need exactly
  that path: evaluation by definition, with an approximant for queries. Keep the closed enum, but
  build the procedural kinds on one internal `Procedural` mechanism. A later `Custom` is then a
  small change, not one arm in every match. A small correction: the pin has 13 surface kinds, not 14
  (READ: `solidcore/src/surface/surface/p2.rs:392-420`).
- **K2. A seed per document.** Identical geometry in two documents gives different results wherever
  a random draw decides, such as ray directions near degenerate cases. A fixed algorithm constant
  keeps "same input, same answer" independent of which document the body sits in.
- **K3. `Stop` checked at item boundaries only.** One SSI march or blend chain is not an item, so
  cancellation can take seconds. Poll at the same points as the work budget, as the pin's fuel does
  (`solidcore-ops/src/fuel.rs` module doc).
- **K4. The cost of the mate scan.** It is acceptable at the measured sizes: 4-20 uses, rising to
  about 130 on plate8's top face. Geometry dominates Booleans and blends. If it ever shows, use a
  body-level use index (B5), not a face cell.
- **K5. Seam and wire cases are unspecified.**
  - A seam edge appears twice in one face's loop, so `Edge::faces` holds the face twice, and the mate
    is in the same face, told apart by sense.
  - Wire edges (no faces) and a lone vertex inside a face (a cone apex, an imprinted point) need a
    stated form: an edge collapsed to a point, or a vertex loop.
- **K6. `finish` consumes the draft on error.** Return it in the error, so tools can show the broken
  structure.
- **K7. Cell sampling and tolerance.** A boundary tree or sampled boundary that must reach the
  resolution's accuracy cannot read it (rule 1). State the sampling rule (a fixed relative density,
  or certified distance bounds), so point-in-face near a boundary is right at any resolution.
- **K8. Diagnostics and progress.** There is no progress callback and no structured trace, and the
  pin's profiler (`solidcore/src/profile.rs`) has no counterpart. A CAD library needs at least a
  progress sink in `Cx`, as a no-op by default.
- **K9. A viewer pulls the query stack.** `scx-tess` depends on layers 0-6. The pin's
  `solidcore-tess` meshes records directly. Check that a viewer which only meshes a file does not
  need `scx-bodyq`.

---

## The five uncertain choices: verdicts

1. **Rigid placement on the body: change.** Use no placement on `Brep`, an id-keeping
   `transform`, and `Placed<'_>` operands for Boolean tools and `scx-bodyq` reads (S1).
2. **Closed `#[non_exhaustive]` enums with extension by certified NURBS approximation: keep.** Make
   `SurfaceFn` generic over interval jets so the approximation can be certified, with `f64`-only
   input giving `Unproven` (S10). Build the procedural kinds on one internal mechanism so `Custom`
   stays cheap to add (K1). Trait objects are rejected: research 55 needs evaluators generic over the
   scalar, which a `dyn` interface cannot give.
3. **Uses as loop lists in face records, mates by scanning: keep the loop lists, change the rest.**
   Give each face two sides with shell references (B1). Store the radial order on the edge, outside
   invalidation (S11). Replace the face-cell mate fallback with a body-level use index (B5). With
   B1, cellular and non-manifold bodies and the pin's region-based merge are supported. Without it,
   the merge's intermediate bodies, multi-region bodies and lamina faces are lost. Void regions
   survive in both forms.
4. **No session; resolution passed on every call: keep no session, change the resolution.** Stamp
   the resolution on each `Brep` and check it against the document on every call, so a mismatch is
   a typed refusal (B2). Bodies from different documents then fail loudly, not silently.
5. **Partial success all-or-nothing with explicit `Skip`: keep (c), with the semantics defined
   (S5).** The default reports the first failure in item order. `Skip` is a deterministic fixpoint
   over chains. Add `Unsupported` and `Exhausted` (S4) so failures are classified honestly.

---

## Checked and found sound

- Geometry as plain data with no owner links (§7.1). READ: the pin's `Surface` holds `owner:
  Option<ObjectRef>` and owner-chain fields (`solidcore/src/surface/surface/p2.rs:422-440`). The
  claim is accurate and the fault is real.
- An immutable `Brep`, an editing `Draft`, and `finish` as the one structural check (§2.3).
- Per-face `Arc` records with whole-table copy-on-write, and chunking only past about 10,000 faces
  (C5, research 51).
- Dense ids never reused within a lineage (§2.2), apart from the compaction in S6.
- Memos in a caller-owned `Scratch`; global and per-thread caches banned (C6).
- No fix switches; each fix is on or absent (§6.3).
- A work budget in deterministic units, not time (§6.3).
- An error type per crate with `source()` and a small shared `ErrorKind` (§5.1), apart from S4.
- The `SolveError<E>` and `EvalError` classification carried over from `scx-algo`.
- Fixed reduction trees, the lowest-index error and `total_cmp` (§8.2). The pin facts cited (mass
  sums in list order, HLR's first-in-time error, the static idle pool) are true at the pin (07
  §7).
- `fmath` for cross-platform agreement, verified late (§8.3).
- The tolerance shape (§4): one resolution, recorded accuracies with a `Basis`, face accuracy
  derived, `Unproven` for the research 55 cases, and never refusing valid input. B2 adds only where
  the resolution is recorded.
- Curves and surfaces in one crate with a checked module order (C1). The 314 and 48 references
  (research 49) and the curve kinds defined on surfaces justify it.
- `scx-brep` separate from `scx-bodyq` (C2), though S2 suggests removing brep's dependency on geomq.
- One crate per operation family, never merged (C3), once S3's couplings are resolved.
- No facade crate at first (§1.2).
- Undo as old `Brep` values and history as `Changes` (§6.2), subject to S12 and B3.
- `scx-record` as its own format, with no serde in the core (§7.4).
- wasm32 as a tier-1 target, with parallel code behind a feature with a serial path (§7.4).
- `#[non_exhaustive]` options structs with `with_*` methods, and one way to build a body (§7.2).
