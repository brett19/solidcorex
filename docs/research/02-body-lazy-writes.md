# 02 — Body side: lazy writes, memo fields and copies made for reads

Checkout: `sc-wt/cache-arch` at `782ef9afc`. Read-only investigation (grep and
line-range reads; nothing built or run). Paths are relative to `solidcore/crates/solidcore/src`
unless another crate is named. Counts come from `rg` and a small script over non-test code (text
up to the first `#[cfg(test)] mod`, files with `test` in the name skipped); treat them as
approximate to a few units. Anything marked **UNVERIFIED** was inferred, not confirmed in code.

Scope: topology and body. Geometry caches (the surface cache slot, NURB cells, curve memos),
Kernel-level state and per-operation holders are other reports; they are named here only where a
body read reaches them.

## 0. Summary of findings

- A `Brep` has **no edit counter and no generation**. Every memo on it is invalidated by a
  notification (`PostEdit`, `ChangeGeometry`, `Split*`) or not at all.
- "Reads that write" come from **eight write leaves** (section 2.3). Seven are topology functions
  taking `&mut Brep`; the eighth is the surface's shared cache slot, written through `&Brep`.
- About **207 functions** in the read modules take `&mut Brep`; `hlr/` and `eval/` take none.
- **23 production call sites** in the kernel copy a body only so a read can scribble on the copy
  (17 `clone_standing_for_self`, 5 `clone_sharing_surface_caches`, 1 `with_rollback`), plus
  `solidcore-ops`' `measure_body` (10 callers).
- Lazy trim creation is order-dependent and crosses faces: it changes the edge's interval, trims
  the other faces' trims of the same edge, raises edge and vertex tolerances, and may build every
  trim of the face. There is no function that computes a new trim without writing the body.
- A restart protocol already exists (`SolidError::PreparationRequired`, `RayPreparation`,
  `QueryTopology::{Mutable, Prepared, Requested}`), with three loop sites.
- A standing copy's surfaces share the original's cache slot (`Arc<Mutex<_>>`), made-kind cell and
  continuity memo. Transforming such a copy still deletes the original's stored trimmed cache at
  this checkout.

## 1. Field table

Classes: **MEANING** (authored; dropping it loses information), **MEMO** (derivable), **MODE**
(state of an operation in progress), **ID** (identity or bookkeeping), **PORT-DERIVED** (port-only
cached topology, rewritten by `Brep::refresh_cached_topology`, `topology.rs:2156`).

### 1.1 `Brep` (`topology.rs:759-843`)

| Field | Class | Notes |
|---|---|---|
| `shells, faces, loops, vertices, regions, faceuses, loopuses, vertexuses` | MEANING | plain `Arena`, deep-copied by every copy (`topology.rs:885-895`) |
| `edges, edgeuses` | MEANING | `Shared<Arena>` copy-on-write (section 6) |
| `infinite_region, region_list_head, region_order, face_list, edge_list, vertex_list` | MEANING | list order is observable (iteration order of `brep::p1::faces` and friends) |
| `tolerance` | MEANING | read by trim creation (`within_edge_tolerance_limit`, `topology/edgeuse/p3.rs:1916`) |
| `model_size_estimate, large_small_size_ratio` | MEANING | set by `topology/brep/p1.rs:236,245`, copied `topology/brep/p3.rs:750` |
| `key` | ID | temporarily replaced during `kernel_named_edit` (`topology/brep/p3.rs:1488-1527`) |
| `naming` | ID | `NamingScheme` |
| `geometry_owner_keys` | ID | written when geometry is bound; also by `rebind_geometry_owners` at the end of every kernel-named edit (`topology/brep/p3.rs:1545`) |
| `notify_identity` | ID, lazily issued | `notify_serial(context, &mut Brep)` issues on first use (`topology/brep/p3.rs:1049`); `Clone` gives an empty slot (`geom/context.rs:305`) |
| `progress_tag` | ID, lazily issued | `progress_tag(&mut self)` (`topology.rs:1929`); clone is empty (`topology/topology.rs:415`) |
| `removals` | ID | `RemovalLog`; `removal_tag(&mut self)` lazy (`topology.rs:1934`); clone is empty (`topology/topology.rs:367`) |
| `attributes` | MEANING | caller's and kernel's attributes; taken out and put back by `kernel_named_edit` (`topology/brep/p3.rs:1500,1530`) |
| `trim_cover_raises` | MEANING (provenance of a tolerance) | written only by `cover_kept_uv_trim_curves` (`topology/brep/p2.rs:561`) and `copy_trim_cover_raises` (`topology/brep/p3.rs:822`); read by `fit_tolerance` (`topology/edgeuse/p3.rs:410`). Not derivable: it records the value before a raise |
| `validate_only` | MODE | section 1.4 |
| `editing_enabled` | MODE | section 1.4 |
| `surface_owner_cleared` | MODE | section 1.4 |
| `locked_bounding_box` | MODE and MEMO | section 1.4 |
| `deleted_surfaces` | MODE (operation scratch) | pushed by `add_deleted_surface` (`topology/brep/p1.rs:360`; 6 callers in `brep/p1.rs`, `brep/p2.rs`, `blend/hcr.rs`). No function clears it (12 references, all in `topology.rs` and `brep/p1.rs`), so it only grows; snapshots copy it (`topology.rs:1387`) |

### 1.2 Entity records

| Record.field | Class | Notes |
|---|---|---|
| `Vertex.point`, `.vertexuses` | MEANING | |
| `Vertex.tolerance` | MEANING, raised by reads | section 1.5 |
| `Vertex.poly_vertex` | MODE (temporary back-pointer of a poly build) | only production writer is `query/poly.rs:3448` (clears it in `destroy_with_original`); `vertex::set_poly_vertex` `topology/vertex.rs:250`. Cloned with the vertex |
| `Edge.vertices` | PORT-DERIVED | from the edge and vertex uses (`edge::start_end_vertices`); stale inside an operation by design (`topology.rs:412-417`) |
| `Edge.edgeuses`, `.curve` | MEANING | |
| `Edge.interval` | MEANING, **changed by trim creation** | `edge::set_interval` (`topology/edge.rs:394`) called from `create_uv_trim_curve` (`topology/edgeuse/p3.rs:1010,1034`) |
| `Edge.tolerance` | MEANING, raised by reads | section 1.5 |
| `Loop.face`, `.edges` | PORT-DERIVED | `topology.rs:442-448` |
| `Loop.loopuse` | MEANING | |
| `Face.shell`, `.loops` | PORT-DERIVED | `topology.rs:457-465` |
| `Face.surface`, `.faceuse` | MEANING | the surface carries a copy of the face tolerance (`sync_face_surface_tolerance`, `topology.rs:1972`): a second copy of `Face.tolerance` that every tolerance write must keep in step |
| `Face.uv_domain` | MEMO kept by edits | derivable from the trims (`face::p1::calculate_uv_domain_from_uv_trim_curves`, `topology/face/p1.rs:568`) or the surface's natural domain (`store_surface`, `topology/face/p1.rs:1318`). Written only by edits (`face::p2::set_uv_domain`, about 20 call sites). No lazy writer found. Trim creation reads it (`topology/edgeuse/p3.rs:505`) |
| `Face.rectangular_trim` | MEMO, tri-state | set to 1 by constructors (`topology/brep/p8.rs:280`, `topology/brep/p5.rs:3679`), copied (`topology/brep/p3.rs:2442`, `topology/brep/p10.rs:879`), reset to `UNSURE` by `face::p3::notify` on `PreEdit` (`topology/face/p3.rs:505`). No production code turns `UNSURE` into 0 or 1 lazily: `find_if_rectangular_trim` (`topology/face/p1.rs:1082`) returns its answer without storing it. Read at `topology/face/p2.rs:1710`, `topology/brep/p5.rs:3631` |
| `Face.tolerance` | MEANING, raised by reads | section 1.5 |
| `Shell.faces` | PORT-DERIVED | |
| `Shell.region`, `.shell_type`, `.members` | MEANING | |
| `Region.in_brep`, `.shells` | MEANING | |
| `Region.is_void` | MEANING (tri-state, 2 = unsure) | written by edits only (`offset/offset_executive/p2.rs:949,1445`, `topology/shell.rs:375`, `heal/heal_data/p1.rs:143`); no lazy writer found |
| `Faceuse.*` (`shell, face, mate, orientation, loopuses, sibling_links`) | MEANING | `sibling_links`, once `Some`, wins over `Shell::members` (`topology/shell.rs:58-60`): two representations to keep in step |
| `Loopuse.*` (`faceuse, loop_, mate, start, orientation`) | MEANING | |
| `Edgeuse.edge, owner, ccw, cw, vertexuse, orientation` | MEANING | |
| `Edgeuse.uv_trim_curve` | MEMO in principle, MEANING in practice | section 3 |
| `Edgeuse.drop_curve_fail` | MEMO (diagnostic of the last creation) | written with the trim (`topology/edgeuse/p1.rs:768,805`); read as a status by `prepared_trim` (`topology/loopuse.rs:193-196`) and `existing_uv_trim_curve` |
| `Edgeuse.max_edge_face_gap` | MEMO | section 1.3 |
| `Edgeuse.edge_ccw_edge_gap` | MEMO | section 1.3 |
| `Vertexuse.vertex`, `.owner` | MEANING | |
| `Vertexuse.vertex_edge_gap`, `.vertex_face_gap` | MEMO | section 1.3 |
| `Vertexuse.uv_point` | not a lazy memo | the only production write is `blend/fillet_geom/p1.rs` (1 site); `compute_uv_point` returns its value without storing it (no `uv_point =` in `topology/vertexuse.rs`) |

`Edgeuse.uv_trim_curve` is derivable from (edge curve, edge interval, face surface, face
`uv_domain`, tolerances, kernel flags), but not reproducibly: its value depends on creation order
(section 3.5), a file read may keep a trim that differs from what creation would make
(`check_and_fix_uv_trim_curves`, `topology/brep/p2.rs:486`), and edits author trims directly
(about 80 `set_uv_trim_curve` or direct-assignment sites in `blend/`, `offset/`, `topology/`).
Dropping it changes results.

### 1.3 Memo fields: writers, invalidation, staleness

| Memo | Derived from | Writers (production) | Writer reachable from a read? | Invalidated by | Stale read possible? |
|---|---|---|---|---|---|
| `max_edge_face_gap` (edge use and its mate) | edge curve over `Edge.interval`, face surface, 20 samples (`topology/edgeuse/p1.rs:1006-1027`) | `calc_max_edge_face_gap(cache=true)` (`edgeuse/p1.rs:986-1045`); reset in `edgeuse::p2::notify` (`edgeuse/p2.rs:928-930`); `CurveOnClassifyCache::apply_gap_updates` (`query/face/p2.rs:1597`); direct writes in `blend/fillet_geom` (6) and `blend/fillet_corner` (2) | Yes: `max_edge_face_gap(k, &mut b, eu, false)` from `create_uv_trim_curve` (`edgeuse/p3.rs:666`), validation (`assert_valid/brep.rs:452`), classification (`query/face/p2.rs:1698`) | `SplitInBrep`, `ChangeGeometry`, `Split` on the edge use | **Yes, by code reading.** `edge::set_interval` sends no notification and resets nothing (`topology/edge.rs:394-397`), and trim creation calls it. A tolerance change does not matter (the gap is a distance) |
| `edge_ccw_edge_gap` | the two edges' interval ends (`edgeuse/p1.rs:1086-1099`) | `calc_edge_ccw_edge_gap(cache=true)` (`edgeuse/p1.rs:1076,1104`); `topology/brep/p2.rs` (1), `heal/heal_data/p2.rs` (1) | Yes: `edge_ccw_edge_gap(k, &mut b, eu, false)` (`edgeuse/p1.rs:945`) | **Nothing**: `edgeuse::p2::notify` resets only `max_edge_face_gap` | **Yes**: after either edge's interval or curve changes |
| `vertex_edge_gap` (the vertex use and its siblings on the same edge) | vertex point, edge curve at the interval end (`topology/vertexuse.rs:594-599`) | `calc_vertex_edge_gap(cache)` (`vertexuse.rs:576-612`); reset in `vertexuse::notify` (`vertexuse.rs:2099-2101`) | Yes: validation (`assert_valid/brep.rs`), `vertex::max_gap_3d` | `PostEdit`, `SplitInBrep`, `ChangeGeometry`, `Split` on the vertex use | Yes, same `set_interval` path |
| `vertex_face_gap` | vertex point, face surface, UV from `compute_uv_point` (`vertexuse.rs:664`) | `calc_vertex_face_gap(cache)` (`vertexuse.rs:655-673`); reset by `set_uv_trim_curve` (`edgeuse/p1.rs:777`); `apply_gap_updates` (`query/face/p2.rs:1592`) | Yes: `vertex_face_gap(k, &mut b, vu, false)` from classification (`query/face/p2.rs:1615`), sweep (`sweep/topology_sweep/store/classification.rs:131`), `face/p4.rs:2411`, validation (`wide_vertices.rs:136,542`) | vertex-use notify; any new trim on the edge use or its mate | Not found. Note the reset happens **before** the projection is attempted (`vertexuse.rs:654-664`), so a failed calculation leaves siblings cleared; `CurveOnClassifyCache` is kept alive on error for that reason (`query/face/p2.rs:1542-1543`) |
| `rectangular_trim` | the face's loops and trims | section 1.2 | No | face `PreEdit` | No lazy writer, so no stale value from a read |
| `Face.uv_domain` | trims or surface | edits only | No | (edit-maintained) | Stale only if an edit forgets it |
| port-derived topology | use links | `refresh_cached_topology` (82 references, 28 files; 30 more in `solidcore-ops`) | No read calls it | n/a | **Stale inside every operation by design.** The few direct readers found by pattern: `offset/offset_executive/periodic_boundary.rs` (2), `topology/brep/trim_extend.rs`, `side_extend.rs`, `topology/brep/p3.rs`, `query/poly.rs` (3), validation (3). **UNVERIFIED** as a complete list: records bound to a local before the field access are not matched. `topology/integrity.rs:254` (`check`) compares cached against derived |

The face-level notify skips its walk over edge uses on `PostEdit` when no notify callback is
installed (`topology/face/p3.rs:523-531`, #14329). The vertex uses are still walked.

### 1.4 Mode fields

| Field | Writers | Reachable from a read? | Who reads it | Notes |
|---|---|---|---|---|
| `editing_enabled` | 223 `set_editing_enabled` calls in 58 files; 129 reads in 51 files | **Yes**: `TrimSrfCache::implant_face` sets it true for the build and restores it (`surface/trim_srf_cache.rs:822-826`) through `BrepCacheFace::set_brep_editing_enabled` (`query/face/p2.rs:251`) | trim cache reuse (`query/face/p2.rs:578`), `face_point_test_on` (`intersect/point_class.rs:2481`), ray cache (`query/brep/ray.rs:211`), `trim_srf_cache.rs:797` | Save, set, restore by hand at each site; not a guard type. Changes what a read answers: whether a modified face's trimmed cache is reused or rebuilt |
| `validate_only` | `validate_and_update` saves, sets, restores (`topology/brep/validation/assert_valid/brep.rs:272-275`); `copy_brep` copies it (`topology/brep/p3.rs:741`). 5 `set_` calls in total | Yes: validation is a read | exactly one reader: `set_uv_trim_curve` (`topology/edgeuse/p1.rs:808-824`) | Suppresses only that one raise (section 3.4) |
| `surface_owner_cleared` | `begin_`/`end_surface_owner_cleared`: `create_uv_trim_curve` (`edgeuse/p3.rs:646,1097`), `create_uv_trim_curves_notified` (`face/p3.rs:1274,1277`), `clear_surface_owner` on a classification snapshot (`query/face/p2.rs:2028`) | **Yes**: every lazy trim creation | `face_surface_cache` (`query/face/p2.rs:438,457`), `intersect/point_class.rs:2159,2474`, `topology/brep/p9.rs:3058` | `begin` also pushes into the **surface's shared cache slot** (`topology.rs:1180-1182` → `surface/surface/p2.rs:1470`), so the scope is visible through every copy that shares the slot. `Brep::snapshot` refuses while it is non-empty (`topology.rs:1344`). The face's surface owner is set to `None` for the scope and restored (`edgeuse/p3.rs:643-645,1098-1100`) |
| `locked_bounding_box` | `lock_bounding_box` (`query/brep.rs:309`), `unlock_` (`query/brep.rs:326`); callers `topology/brep/p1.rs:1560,1573`, `topology/brep/p4.rs:457,2171`, `sweep/topology_sweep/store.rs:2331`, `boolean/merge/p1.rs:1776-1777` | Callers are operations, but the lock is a scope around reads | `calculate_bounding_box` returns it while set (`query/brep.rs:183`) | A clone keeps it (`topology.rs:818`, deliberately unlike upstream). Nothing invalidates it on edit; only the explicit unlock |
| `deleted_surfaces` | section 1.1 | No | `deleted_surfaces()` | |
| `Vertex.poly_vertex` | section 1.2 | No | poly build | |

### 1.5 Tolerances: every function that raises one

Primitive setters: `edge::set_tolerance` (`topology/edge.rs:2302`; cascades to both end vertices
through `vertex_set_tolerance`, `edge.rs:2318-2325`), `vertex::set_tolerance`
(`topology/vertex.rs:195`), `face::p3::set_tolerance` (`topology/face/p3.rs:422`; cascades to the
face's edges and on to their vertices, and syncs the surface's copy), `brep::p2::set_tolerance`
(`topology/brep/p2.rs:380`), `vertex::refresh_tolerance` (`topology/vertex.rs:1298`). The script
counts about 180 `set_tolerance` / `refresh_tolerance` / `update_object_tolerance` call sites
(`topology` 78, `boolean` 30, `blend` 19, `heal` 15, `offset` 13, `query` 7, `intersect` 7,
validation 4, `sweep` 4, `primitive` 3); the pattern also matches non-B-rep setters (poly,
proto-topology), so it is an upper bound. About 60 files also assign `.tolerance =` directly.

| Raiser | What | Reachable from a read? |
|---|---|---|
| `set_uv_trim_curve` (`topology/edgeuse/p1.rs:811-823`) | edge to `2 * max_dist_to_surface`, cascading to both vertices | **Yes**: every `get_or_create_uv_trim_curve` |
| `create_uv_trim_curve` → `finish` (`edgeuse/p3.rs:1162-1171`) | edge to `1.01 * error` when the drop needed the larger tolerance (`ii > 1`), cascading | **Yes**. Not gated by `validate_only` |
| `create_uv_trim_curves_body` (`topology/face/p3.rs:1465-1467`) | vertex to `2 * gap_3d` with `adjust_tolerances` | **Yes**: the trimmed-cache build passes `true` (`query/face/p2.rs:258`). Not gated by `validate_only` |
| `check_and_set_uv_trim_curve` (`topology/edgeuse/p2.rs:691`) | through `set_uv_trim_curve` | only with caller-supplied UV curves (edits) |
| `CurveOnClassifyCache::apply_uv_trim_updates` (`query/face/p2.rs:1574-1583`) | copies raised edge and vertex tolerances from a classification snapshot back to the body | **Yes**: the heal face properties (`heal/face_props.rs:1764,2281`) |
| `cover_kept_uv_trim_curves` (`topology/brep/p2.rs:536`) | edge to a proved trim distance, cascading; records `(before, after)` | file read only (`brep/p2.rs:495`, `topology/part_file.rs:464`) |
| `cover_joined_faces`, `cover_raised_trims`, `cover_vertex_ends`, `cover_joined_edge_trims`, `settle_face_bounds` (`edgeuse/p3.rs:2026-2610`; raises at `2093,2201,2327,2587`) | stitch covers | No (stitch) |
| `validate_and_update` with `validate_only = false` (`validation/assert_valid/brep.rs:265`), `update_tolerances_after_boolean`, `centre_wide_vertices` (`validation/wide_vertices.rs:22,47`) | vertex to twice its gap, edge to twice its edge/face gap | No (deliberate updates). `check_brep` runs it with `validate_only = true` (`assert_valid/brep.rs:238-240`) |
| `heal::fix_tol_sizes` (`heal/heal_data/p2.rs:3013`), `stitch::update_object_tolerance` (`heal/stitch/p1.rs:2576`), `geom::tol::update_object_tolerance` (`geom/tol.rs:1782`), boolean adapters (`boolean/topology_intersector/p1.rs:414`, `boolean/merge/p4/arena_pair.rs:1442`), fillet `faces_set_tolerance` | operation raises | No. But `PointClassification::ok_to_set_tolerance` (`intersect/curve_class/p1.rs:968`) shows a classification can be allowed to raise: the 7 `set_tolerance` matches in `intersect/` sit behind that flag (**UNVERIFIED** line by line) |

## 2. Reads that write

### 2.1 Inventory by module

Functions outside mutating operation modules whose signature takes `&mut Brep` (script output;
adapter methods included): **207**.

| Module | Count | What forces `&mut` |
|---|---|---|
| `query/face/p2.rs` | 28 | trimmed-cache lookups (`face_surface_cache` family, `:429-676`), `point_classify*` (`:819-974`), classification adapters, `CurveOnClassifyCache` |
| `mass/integrator/properties.rs` | 23 | `gap_closure` makes missing trims (`:1665`); every area, volume and mass entry inherits it |
| `mass/enclosure.rs` | 13 | `pcurve` makes a missing trim (`:2474`) |
| validation (`assert_valid*`, `pointers`, `soundness`, `wide_vertices`) | 54 | gap memos, binormals, trims made by `check_face`, the `validate_only` toggle. 5 of them (`wide_vertices.rs`) and `validate_and_update_tolerances` are real updates |
| `query/brep.rs`, `query/brep/ray*.rs` | 23 | bounding box, ray fire, `point_3d_classify`; `LazyBrep` and `Target` copy on first write |
| `query/api_queries.rs` | 10 | public queries (`find_faces`, `face_internal_point`, `face_classify_uv`, `closest_*`, `face_normal_at_*`, `edgeuse_face_frame`) |
| `query/contact.rs` | 9 | point-on-face tests |
| `query/tangent_edge.rs` | 7 | binormals and face-use normals |
| `query/face/p1.rs`, `p3.rs` | 8 | `point_3d_classify`, `point_on_face`, `edge_curve_on_classify`, `sector_classify_curve_on` |
| `query/edge.rs`, `edge_turn.rs`, `loop.rs`, `face_surface.rs`, `poly.rs` | 8 | radial sectors (binormal), `contains_uv_point` (`ensure_trim_curve`), face/surface intersection |
| `intersect/` (`api_intersectors`, `curve_class/p5,p7`, `point_class`) | 12 | curve/face and curve/body intersection, live global point solve (`point_class.rs:2147-2221`) |
| `topology/brep/planar_section.rs`, `api_projection.rs`, `api_files.rs`, `enclosing.rs`, `topology_traverser.rs` | 12 | sections and projection (`force_uv_curves`, `planar_section.rs:680`); the last three files hold real edits |
| `hlr/`, `eval/` | **0** | `hlr` takes `&Brep` only (20 references); `eval` takes no body |

Sibling crates: `solidcore-tess` takes `&Brep` only (4 references, all in `iso.rs`);
`solidcore-record` has no kernel `Brep`; `solidcore-convert`'s `to_record(k, &mut Brep)` makes
trims (section 5.3); `solidcore-ops` has 54 `&mut Brep` against 128 `&Brep` references and keeps
reads apart through `read_body`, `measure_body` and `with_body` (section 5.2).

### 2.2 Root causes

Direct call counts per module group (script):

| Leaf call | query | mass | intersect | validation | heal | boolean | blend | offset | sweep | other topology |
|---|---|---|---|---|---|---|---|---|---|---|
| `get_or_create_uv_trim_curve` | 2 | 2 | 0 | 0 | 2 | 0 | 7 | 2 | 0 | 25 |
| `create_uv_trim_curves` (face) | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 15 |
| `ensure_trim_curve` | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| `face_surface_cache` family | 17 | 0 | 1 | 0 | 0 | 4 | 1 | 2 | 0 | 7 |
| `evaluate_binormal*` | 12 | 0 | 3 | 6 | 4 | 2 | 42 | 4 | 2 | 24 |
| `normalized_evaluate` (may make) | 0 | 0 | 0 | 4 | 2 | 2 | 3 | 8 | 0 | 30 |
| `compute_uv_point` / `compute_uv_sector` | 3 | 0 | 2 | 1 | 0 | 0 | 7 | 0 | 0 | 10 |
| gap getters (four kinds) | 3 | 0 | 0 | 8 | 4 | 2 | 1 | 0 | 1 | 22 |
| standing or sharing copy | 9 | 0 | 6 | 1 | 0 | 1 | 6 | 1 | 1 | 5 |

Grouped by cause:

- **A. Lazy UV trim creation.** The dominant cause. Entry points: `get_or_create_uv_trim_curve`
  (`topology/edgeuse/p3.rs:1690`), `QueryTopology::Mutable(..).ensure_trim_curve`
  (`topology/loopuse.rs:158`), `normalized_evaluate(.., ok_to_make = true)` (`edgeuse/p3.rs:3402`),
  `compute_uv_point(.., true)` (`topology/vertexuse.rs:1181`), `evaluate_binormal`
  (`edgeuse/p3.rs:2873`; makes the edge use's trim at `:3041` and builds the face's cache at
  `:3125`), and the gap constructors, whose end-point read goes through
  `GetOrCreateUVTrimCurve` (`heal/gap.rs:43-45`), which is why even `calc_*_gap(cache = false)`
  takes `&mut Brep`.
- **B. The trimmed surface cache "with the face".** `surface_cache_of_face`
  (`query/face/p2.rs:556`) builds through `BrepCacheFace` (`:231-371`), whose
  `create_uv_trim_curves` makes **every** trim of the face with `adjust_tolerances = true`
  (`:256-259`), whose `vertexuse_uv_point` calls `compute_uv_point(.., true)` (`:312`), and whose
  `set_brep_editing_enabled` flips the body's mode. The cache lands in the surface's shared slot.
- **C. Gap memos.** Section 1.3.
- **D. Tolerance raises** attached to A and B. Section 1.5.
- **E. Mode scribbles.** `surface_owner_cleared` (A), `editing_enabled` (B), `validate_only`
  (validation), `locked_bounding_box`.
- **F. Plumbing.** Functions that take `&mut Brep` only to pass it down: `LazyBrep::get_mut`,
  `Target::mutable` (`query/brep.rs:1102,1135`), `topology_entity_mut`,
  `topology_entity_for_query`, `brep_mut` adapters (`query/face/p2.rs:1518,1675,1681,1902,1919,2010`,
  `intersect/curve_class/p5.rs:102,125`, `p7.rs:2934`). About 15 of the 207.
- **G. Lazy identity issue.** `notify_serial(context, &mut Brep)`, `progress_tag(&mut self)`,
  `removal_tag(&mut self)`. Reached from edit wrappers, not from queries (callers found only in
  `blend/` and `topology/brep/p3.rs`).
- **H. Interior mutability through `&Brep`.** The surface cache slot is taken, checked out,
  stored and deleted through `&Surface` (`surface/surface/p2.rs:1366-1454`), including from
  functions that take `&Brep`: `face_point_test_on(b: &Brep, ..)` takes the cache out and puts it
  back (`intersect/point_class.rs:2477-2483`), and `point_classify_with_topology` does the same on
  a prepared (read-only) query (`query/face/p2.rs:1127-1137`).

### 2.3 The write leaves

A guard would instrument exactly these.

| # | Leaf | Writes |
|---|---|---|
| 1 | `edgeuse::p1::set_uv_trim_curve` (`topology/edgeuse/p1.rs:715`), with `delete_uv_trim_curve` (`:846`) and `set_member_uv_trim_curve` (`:862`) | `uv_trim_curve` and `drop_curve_fail` on the edge use and its mate; resets `vertex_face_gap` on both start vertex uses; raises `Edge.tolerance` and, by cascade, both `Vertex.tolerance` |
| 2 | `edgeuse::p3::create_uv_trim_curve` (`topology/edgeuse/p3.rs:474`): its own writes | `Edge.interval` (`:1010,1034`); other edge uses' trims of the edge (`trim_other_uv_curves`, `:1184-1214`); `Edge.tolerance` (`finish`, `:1170`); owner-cleared scope and face surface owner (`:643-646,1097-1102`); deletes a trim the cache build attached meanwhile (`:906,1052,1605,1652`) |
| 3 | `face::p3::create_uv_trim_curves_body` (`topology/face/p3.rs:1295`) | loops leaf 1 and 2 over the face; `fix_uv_trim_curves_notified` moves end points of two neighbouring trims (`:1437`); `vertex::set_tolerance` (`:1466`) |
| 4 | `calc_max_edge_face_gap`, `calc_edge_ccw_edge_gap` (`topology/edgeuse/p1.rs:972,1061`), `calc_vertex_edge_gap`, `calc_vertex_face_gap`, `init_vertex_face_gap` (`topology/vertexuse.rs:551,630,740`) with `cache = true` | the four gap memos (mates and siblings included) |
| 5 | `Brep::begin_`/`end_surface_owner_cleared` (`topology.rs:1177,1469`) and `sync_face_surface_tolerance` (`topology.rs:1972`) | mode list, surface slot `cleared_for`, the surface's owner and its copy of the face tolerance |
| 6 | `Brep::set_editing_enabled` reached from `TrimSrfCache::implant_face` (`surface/trim_srf_cache.rs:822-826`), `Brep::set_validate_only` (`validation/assert_valid/brep.rs:272-275`), `Brep::set_locked_bounding_box` (`query/brep.rs:321,327`) | mode flags |
| 7 | `edge::set_tolerance`, `vertex::set_tolerance`, `face::p3::set_tolerance` (`topology/edge.rs:2302`, `topology/vertex.rs:195`, `topology/face/p3.rs:422`) | tolerances; all lazy raises funnel through these three, except the direct assignments in `apply_uv_trim_updates` (`query/face/p2.rs:1576,1581`) |
| 8 | `Surface::take_trim_cache`, `store_trim_cache`, `check_out_trim_cache`, `delete_trim_cache`, `mark_trim_cache_face_modified`, `begin_`/`end_owner_cleared` (`surface/surface/p2.rs:1366-1481`) | the shared slot; **not** behind `&mut Brep` |

`CurveOnClassifyCache::apply_uv_trim_updates` and `apply_gap_updates` (`query/face/p2.rs:1561,1588`)
are a ninth, deliberate write-back: they copy trims, tolerances and gap memos from a private
snapshot onto the body after a classification.

Read-only variants that already exist: `normalized_evaluate_read_only` and
`normalized_evaluate_reading` (`edgeuse/p3.rs:3444,3470`), `compute_uv_sector_read_only`
(`vertexuse.rs:1589`; the `SectorBrep` trait with `Changing` and `Reading` adapters,
`vertexuse.rs:766-895`), `cached_max_edge_face_gap` (`edgeuse/p1.rs:929`), `existing_uv_trim_curve`
(`edgeuse/p3.rs:2774`), `QueryTopology::Prepared` (`loopuse.rs:138`), `PreparedFaceQueries`
(`surface/surface_cache/face_context.rs:218`), `point_classify_prepared` (`query/face/p2.rs:850`).

## 3. Trim creation: current facts

Call chain: `get_or_create_uv_trim_curve` (`edgeuse/p3.rs:1690`) → `existing_uv_trim_curve` (fast
path, read-only) → `create_uv_trim_curve` (`:474`) → `trim_raise_distance` (`:1822`) →
`set_uv_trim_curve` (`edgeuse/p1.rs:715`). Face level: `create_uv_trim_curves_notified`
(`face/p3.rs:1249`) → `create_uv_trim_curves_body` (`:1295`).

### 3.1 Writes to the body

| # | Field | Value from | Condition | Crosses to another face or edge? |
|---|---|---|---|---|
| 1 | `Edgeuse.uv_trim_curve` and `.drop_curve_fail` on the `Same` member of the mated pair; both slots cleared first (`edgeuse/p1.rs:765-805`) | the dropped curve | always on success | No: the mate is the other side of the same face (`edgeuse/p1.rs:496`) |
| 2 | `Vertexuse.vertex_face_gap` reset on the start vertex use of the edge use and of its mate (`edgeuse/p1.rs:772-778`) | reset | always, also when clearing | Same face |
| 3 | `Edge.tolerance` raised to `2 * raise_by` (`edgeuse/p1.rs:811-823`) | `trim_raise_distance`: proved bound `B`, or the drop's sampled error | `!validate_only` and the tolerance is below twice the distance | **Yes**: the edge is shared by every face on it |
| 4 | `Vertex.tolerance` of both ends raised to the edge's (`edge.rs:2318-2325`) | cascade of 3, 5 and 7 | vertex tolerance below the new edge tolerance | **Yes**: read by every incident edge and face |
| 5 | `Edge.tolerance` set to `1.01 * error` (`edgeuse/p3.rs:1162-1171`) | the drop's error | the drop succeeded only at the enlarged tolerance | **Yes**. Not gated by `validate_only` |
| 6 | `Edge.interval` moved to the UV curve's range (`edgeuse/p3.rs:1009-1010,1033-1034`) | `uv_ivl.min()` or `.max()` | the UV curve came back shorter and its end drops within the vertex's intersection tolerance (`:967-992`) | **Yes** |
| 7 | the other edge uses' `uv_trim_curve` trimmed to the new interval (`trim_other_uv_curves`, `edgeuse/p3.rs:1184-1214`) | `Curve::trim` with knot snap at the fit tolerance | with 6 | **Yes**: those trims belong to other faces |
| 8 | a trim attached meanwhile is deleted (`edgeuse/p3.rs:905-907,1051-1053,1603-1606,1651-1653`) | `delete_uv_trim_curve` | a nested cache build attached one during the call | Same edge use and mate |
| 9 | `max_edge_face_gap` memo of the edge use and its mate (`edgeuse/p3.rs:666` → `edgeuse/p1.rs:917`) | 20-sample gap | third attempt, when the first two drops gave nothing | Same face |
| 10 | `surface_owner_cleared` push and pop, the face surface's owner, the slot's `cleared_for`, the surface's copy of the face tolerance (`edgeuse/p3.rs:637-646,1097-1102`) | scope | always past the curve-on-surface shortcut | Reaches every copy sharing the slot |
| 11 | every other trim of the same face, through `pick_seam_curve` → `evaluate_binormal` → `note_face_surface_cache` (`edgeuse/p3.rs:854,3125`, `:326`) | recursion into the face-level creation | a seam curve with two candidates and no cache yet. **UNVERIFIED**: the owner-cleared scope is active at that point, which makes the lookup build standalone (`query/face/p2.rs:438-439,609-610`), so this may be suppressed |
| 12 | end points of the trim and of its clockwise neighbour moved (`face::p2::fix_uv_trim_curves_notified`, called at `face/p3.rs:1437`) | closes a UV gap between consecutive edge uses | face-level only; gap above scaled zero and not at a pole | Same face, but it edits a trim made earlier |
| 13 | `Vertex.tolerance` raised to `2 * gap_3d` (`face/p3.rs:1465-1467`) | distance from the trim's start to the vertex | face-level only, `adjust_tolerances` | **Yes** |

The earlier analysis's six ways are all still present: three tolerance raises (3, 5, 13, with
cascade 4), the interval change (6), the end move (12) and clearing mate trims (1). Rows 7 to 11
are additional. Cross-face writes are 3, 4, 5, 6, 7 and 13.

The surface work itself runs on an evaluation copy of the face surface with its owner cleared
(`edgeuse/p3.rs:500-504,648-649`), which shares the original's cache slot.

### 3.2 Retry and restart

Inside one creation: four attempts, `DropCurve` then `DropAndTrimCurve`, each at the fit tolerance
and then at an enlarged one (`edgeuse/p3.rs:659-1070`), then 101 dropped points interpolated
(`brute_force`, `:1485`). Rejected candidates `continue`; writes 6 and 7 happen only once a
candidate is accepted (`:994`).

Across a read-only solve:

- The signal is `SolidError::PreparationRequired` (`error.rs:337`) plus a pending request held in
  a `Cell` on `RayPreparation` (`query/brep/ray.rs:53-98`): `PreparationRequest::UvTrim(eu)` or
  `SurfaceCache(face)`.
- Raised by `prepared_trim` (`topology/loopuse.rs:172-191`) when a `QueryTopology::Requested` view
  meets an edge use without trim, by `RayPreparation::surface_cache` (`ray.rs:83-90`), and by the
  boolean's `FaceCacheContext` (`boolean/topology_intersector/p1.rs:4269-4272,4306-4309`).
- Loops that catch it, release the shared view, call `RayPreparation::prepare(k, &mut Brep, ..)`
  (`ray.rs:100-125`) and run the solve again **from its start**:
  1. `ShellRayWorld::solve_shape_ray` default (`topology/shell.rs:1099-1113`);
  2. `with_face_caches` (`boolean/topology_intersector/p1.rs:4723-4741`);
  3. `query/brep/ray/live.rs:769` passes it on. **UNVERIFIED** whether that site loops.
- Termination: each request is attempted once (`attempted`; a repeat is `InvalidInput`, "ray
  preparation made no progress", `ray.rs:106-111`); a trim that could not be made goes into
  `failed_trims` and reads as absent. The module header states the cost: the fallback "can revisit
  earlier candidates with raised tolerances and has quadratic worst-case work in the number of
  requests" (`ray.rs:10-15`). A restarted solve does not see the same body as the first attempt.
- Production shell rays and region-nesting rays use the single-pass mutable services in
  `query/brep/ray/live.rs` instead (`ray.rs:10-11`).

### 3.3 Side tables kept because trims may appear during reads

| Table | Where | Holds |
|---|---|---|
| `RayPreparation` | `query/brep/ray.rs:53` | pending request, attempted set, failed trims, per-face `SurfaceCache` |
| `FaceCaches<O>` | `boolean/topology_intersector/p1.rs:4196-4211` | built and state-only face caches, `tree_requested`, a `RayPreparation`, `classified_faces` |
| `CurveOnClassifyCache` | `query/face/p2.rs:1545` | a sharing copy of the body; written back with `apply_uv_trim_updates` or `apply_gap_updates`. Users: `heal/face_props.rs` (7 constructions), `topology/face/p4.rs:1847,2478`, `topology/face/p2.rs:834` |
| `CurveOnClassifyBrep::keeps_made_trim` | `query/face/p2.rs:1534` | whether a trim made during classification must be carried with the classification |
| `LazyBrep`, `Target`, `BrepClassifier` | `query/brep.rs:1062,1115,1152` | a standing copy made at the first write |
| `Brep.trim_cover_raises` | `topology.rs:842` | the tolerance before a read's cover raise |
| `uv_trim_at_start` (local) | `edgeuse/p3.rs:517` | detects a trim attached by a nested cache build during the call |

### 3.4 What `validate_only` suppresses

Only write 3 (`edgeuse/p1.rs:807-824`, the single reader of the flag). It does not suppress the
trim attachment, the gap reset, writes 5, 6, 7 or 13, or the gap memos. It is set only inside
`validate_and_update` (`validation/assert_valid/brep.rs:272`). The rest of `check_brep` runs
without it and protects the body differently: `validate_pointers` checks trimless faces on a
standing copy (`validation/pointers.rs:449-466`).

### 3.5 Is there a pure "measure"?

No. `create_uv_trim_curve` returns the curve unattached (`CreatedUvTrimCurve`,
`edgeuse/p3.rs:339`), and a few callers use it that way (`edgeuse/p3.rs:3179`; one site in
`offset/`; five more in `topology/`), but it still performs writes 5 to 10. Pure pieces that
exist: `trim_deviation_bound(k, &Brep, ..)` (`edgeuse/p3.rs:2678`), `edge_face_gap_bound` (`:2610`),
`other_trims_bound` (`:1929`), `kept_trim_gap` (`topology/brep/p2.rs:580`), `existing_uv_trim_curve`
(`:2774`), `two_singular_end_points` (takes `&Brep`, `:1221`), and the surface's `drop_curve` /
`drop_and_trim_curve`, which take no body. `fit_tolerance` and `trim_raise_distance` take
`&mut Brep` only because the uncached gap calculation does (`edgeuse/p3.rs:437`; cause A).

### 3.6 What one edge's trim depends on outside that edge and face

- The **tolerances of every face on the edge** (`fit_tolerance`, `edgeuse/p3.rs:416-422`) and the
  **edge/face gaps of every edge use of the edge**, which read the other faces' surfaces
  (`:431-444`).
- `Brep.tolerance` (`brep_calc_edge_zone_tol_3d`, `:424,447`; `within_edge_tolerance_limit`, `:1917`).
- `Brep.trim_cover_raises` for the edge (`:410-414`).
- The **positions and tolerances of both end vertices** (`:954-992`, `:811-819`).
- The **other trims of the same edge** and the surfaces of trimless other faces
  (`other_trims_bound`, `:1929-1985`): they decide whether the raise may be lowered.
- The primary edge use's orientation (`:535-540`) and `Face.uv_domain` (`:505`).
- Kernel state: `k.caches.prove_lazy_trim_raise` (`:1836`), `k.fixes`, `k.tolerances`, `k.rng`,
  `k.cancel` (`:1191-1200`).
- The face surface's cache slot state and the owner-cleared scope (whether a nested lookup builds
  with the face).
- **History.** An earlier trim on the same edge may have raised the edge tolerance (changing this
  trim's fit tolerance and knot snap) or moved the interval (changing the domain this trim is
  dropped over); at face level the neighbour's creation may move this trim's end. The result
  depends on creation order.

`Kernel::with_sampled_trim_raise` (`kernel.rs:1046`) has no caller in the `solidcore/crates`
workspace other than a test (`edgeuse/p3.rs:3824`), and none elsewhere in the repository. Its doc
comment says it is for file import; whichever caller that means is outside this checkout
(**UNVERIFIED**).

## 4. Body copies made for reads

### 4.1 What each copy shares with the original

| Copy | Arenas | Face surfaces | Edge curves and trims | Identity |
|---|---|---|---|---|
| `clone_sharing_surface_caches` (`topology.rs:1194`) | `edges` and `edgeuses` **shared** (Arc); the rest deep | plain `Clone`, then `stand_surface_for`: shares the cache slot and restores the owner (`topology.rs:1210-1213`) | shared until a write copies the arena with the owner-resetting `Clone` | same key, naming, ids; empty `notify_identity`, `progress_tag`, `removals` |
| `clone_standing_for_self` (`topology.rs:1231,1253`) | all deep | `clone_for_evaluation`: **shares** `cache_slot`, `made_kind` and `continuity_memo`; fresh `boundary_memo` and `singularities_memo`; owner kept; definition dropped (`surface/surface/p2.rs:964-976`). A plane's NURB cell is shared (`clone_shared`, `:928`), an extrusion's too (`:951`) | `Curve::clone_for_evaluation`: owners kept. Whether a curve memo is shared is **UNVERIFIED** (curve report) | as above |
| `clone_standing_for_self_keeping_definitions` (`topology.rs:1247`) | all deep | as above with the definition kept (`surface/surface/p2.rs:653`) | as above | as above |
| `clone_for_trial` (`topology.rs:1291`) | all deep | standing copy, then `detach_for_edit` (own slot and NURB cells, `surface/surface/p2.rs:1007`), then `copy_cache_slot_of` (a copy of the trimmed cache). Nested surfaces and surfaces inside edge curves still share (`topology.rs:1283-1285`) | as standing | as above |
| `snapshot` (`topology.rs:1343`) | all deep | `snapshot_copy`: exact copy, **empty** slot, same notification identity (`surface/surface/p2.rs:1021`) | `Curve::snapshot_copy`: "the memo of its cache shared" (`curve/curve/p2.rs:633`) | **same** notify serial and progress tag (`topology.rs:1392-1393`) |
| `brep_copy`, `copy_brep` (`topology/brep/p3.rs:1849,714`) | new arenas, **new ids** | copied through `merge_brep`. No shared slot expected: `copy_cache_slot_of` and `share_cache_slot_of` are called only from `topology.rs` and `vertexuse.rs:871`. **UNVERIFIED** (`merge_brep` not read) | new owners under the copy's naming | new key, `NamingScheme::Caller`; `trim_cover_raises` carried (`brep/p3.rs:729,808-822`) |

### 4.2 Can a write to the copy reach the original?

Yes, through the shared surface slot, for standing and sharing copies:

- A trimmed cache built on the copy is stored in the slot the original reads
  (`Surface::store_trim_cache`, `surface/surface/p2.rs:1388-1397`). That is the stated intent
  (`topology.rs:1185-1192`). The cache's contents come from the copy's trims and tolerances, so
  the original can afterwards be answered from a cache built against trims it does not hold.
- `begin_surface_owner_cleared` on the copy pushes into the shared `cleared_for`
  (`topology.rs:1180-1182`).
- Taking the cache out leaves `CheckedOut` for every sharer (`surface/surface/p2.rs:1366-1379`).
- **Transforming a standing copy still deletes the original's stored trimmed cache.**
  `Surface::transform_kind_at_depth` runs `delete_stored_trim_cache(cache_slot)` on `PreEdit`
  through the surface's own slot (`surface/surface/p2.rs:3692-3706,5262-5269`), and a standing copy
  does not detach it. `swap_uv` does the same through `edit_notify` (`:5247-5255`). The live
  instance is `degenerate_low_work_transform`, which transforms and checks a
  `clone_standing_for_self` with a trial kernel (`topology/brep/mirror.rs:349-357`); it is harmless
  only because the original is transformed right after (`:373`). #20546 (`a6e102b27`, per
  `git log --grep`) is the continuity-memo change and did not alter this path. The transform does
  reset the copy's own memos first (`take_made_kind_for_edit`, fresh boundary, singularities and
  continuity memos, `surface/surface/p2.rs:3657-3660`), so the made-kind cell and continuity memo
  are detached; the cache slot is not. Only `clone_for_trial` detaches it.

### 4.3 Classification of call sites

Production call sites (tests excluded; 83 textual references to `clone_standing_for_self(` in 32
files are mostly tests).

**Copies that exist only so a read can scribble** — 23 kernel sites:

| Site | The read |
|---|---|
| `query/brep.rs:1107` (`LazyBrep::get_mut`) | bounding box and other queries on a shared body |
| `query/brep.rs:1140` (`Target::mutable`) | `BrepClassifier` point classification, ray fire |
| `intersect/curve_class/p7.rs:2623`, `:2680` | `edge_curve_on_classify`, `point_3d_classify` |
| `intersect/topology_solver/p1.rs:2527` | `face_classifies_on_face` |
| `intersect/topology_solver/projected_bound.rs:663` | `create_oriented_trim_boundary` |
| `blend/fillet_removal.rs:1982` | ring test (`is_ring`) |
| `blend/fillet_geom/p5.rs:1893` | `check_tangential_edgeuse_at_vertex` |
| `blend/fillet_executive/session/surface.rs:201` | rail classifications |
| `blend/fillet_executive/session/keep_features.rs:803`, `:856` | wall-body classification (counted by `add_wall_body_copy`) |
| `offset/thin_features.rs:110` | feature grouping |
| `sweep/topology_sweep/store/classification.rs:130` | `vertex_face_gap` |
| `topology/brep/local_operation.rs:155`, `:364` | `validation::standing` before and after |
| `topology/brep/local_operation.rs:799` | UV lines of trimless edge uses (`keeps_seams`) |
| `topology/brep/validation/pointers.rs:464` | `check_face` on trimless faces |
| `query/face/p2.rs:1554` (sharing) | `CurveOnClassifyCache` snapshot |
| `intersect/curve_class/p4.rs:507`, `:579` (sharing) | `evaluate_binormal`, `compute_uv_sector` |
| `boolean/merge/p4.rs:794` (sharing) | `points_in_face` |
| `topology/face/p4.rs:2410` (sharing) | `vertex_face_gap` |
| `blend/fillet_executive/session/edge_refusals.rs:91` (`with_rollback`) | "the check only reads: what its queries build ... is put back with the target" |

Plus, in `solidcore-ops`: `measure_body` (`session.rs:764-781`), a full `brep_copy` per call so a
measure can take `&mut Brep` "for its caches". Ten production callers: `check_solid`
(`measure.rs:642`), `check` (`check.rs:117`), `brep_volume` (`body.rs:412`), `band_room`
(`blend/room.rs:121`), `edges_share_wedge`, `edge_faces_along`, `carried_faces`, `tangent_chain`
(`blend.rs:2029,2421,2549,2625`), and two in test support.

**Genuine working, trial or rollback copies:**

| Kind | Sites |
|---|---|
| standing, as operand or working body | `primitive/primitive_creation/model.rs:146`, `blend/hcr.rs:847`, `topology/brep/mirror.rs:349` (trial transform and check) |
| keeping definitions | `offset/thin_features.rs:136,194`, `offset/offset_executive/p2.rs:2221` |
| `clone_for_trial` | `offset/offset_executive/p2.rs:2553` (stitch trial), `:2643` (`kept_by_make_manifold`; a test on a copy, close to a read) |
| `snapshot()` | `blend/feature_executive.rs:628`, `blend/fillet_executive/session/trim_at_creases.rs:1850`, `.../keep_features.rs:501`, `boolean/api_brep.rs:529` |
| `with_rollback` | `blend/fillet_executive/api.rs:102,684,688`, `offset/offset_executive/p2.rs:2509`, `sweep/skin_flat_walls.rs:42`, `topology/brep/p5.rs:3278`, `topology/brep/p7.rs:2308`; `with_rollback_untouched` at `topology/brep/local_operation.rs:125`; `solidcore-ops/src/session.rs:712` (`with_body`, 74 callers) |
| `copy_brep`, `brep_copy` | `offset/offset_faces.rs:1316`, `offset/offset_executive/p2.rs:2025`, `blend/feature_executive.rs:2220`, `blend/hcr.rs:906`, `boolean/merge/p3.rs:976`, `boolean/operand_tolerances.rs:69` (trial); ops: `contact.rs:40`, `split.rs:341`, `stitch.rs:115,125,310,575`, `blend.rs:1795,1837,1886,2245`, `body.rs:272` |

`with_rollback_untouched` exists because an operation that queries before it edits "builds caches
on it, which later readers observe through the cache slots" (`topology.rs:1434-1440`).

## 5. Completion points

### 5.1 In the kernel crate

There is no single exit path. What exists:

- `refresh_cached_topology` is called by hand at the end of public entry points: 82 references in
  28 files; the most in `primitive/api_primitives.rs` (9), `boolean/api_brep.rs` (9),
  `topology/brep/sheet.rs` (8), `offset/offset_faces.rs` (7), `blend/fillet_executive/api.rs` (7),
  `sweep/api_brep.rs` (5), `topology/brep/local_operation.rs` (4). Import and heal have one each
  (`heal/heal_data/p1.rs`). It touches only the port-derived fields.
- `edit_brep` (`topology/brep/p3.rs:1688-1706`) is the nearest thing to a scope: it sets editing,
  runs `with_kernel_callbacks`, restores editing and refreshes. It has 1 kernel caller and 3 in ops.
- `kernel_named_edit` (`topology/brep/p3.rs:1480`), reached through `with_kernel_callbacks`
  (40 calls in 13 kernel files, 10 in ops), ends with `rebind_geometry_owners`, which already walks
  every edge, edge use and face (`:1545-1635`). It is not nestable and does not cover the boolean
  or the sweeps, which use `ArenaNotifyState` (60 constructions in 28 files).
- No final validity check is common to all operations. `local_operation` has its own
  (`refuse_unsound`, `topology/brep/local_operation.rs:364`); the others leave it to ops.

### 5.2 In `solidcore-ops`

This is where a body is "finished" today:

- `Body` is a `Mutex<Held>` (`body.rs:207`). Three access wrappers in `session.rs`: `with_body`
  (`:699`; edit under `Brep::with_rollback`; 74 callers), `read_body` and `read_held` (`:729,741`;
  `&Brep`; 21 callers), `measure_body` (`:764`; a copy; 11 callers).
- `finish`, `finish_proved`, `finish_under` (`body.rs:650-711`; 41 callers) apply the result
  contract (non-empty; a solid stays a closed manifold solid, optionally proved outward). They do
  not refresh and do not make trims.
- `Body::new` and `Body::held` (`body.rs:179-192`; 23 `Body::new` calls) are where a kernel `Brep`
  becomes a held body.
- ops calls `refresh_cached_topology` itself 30 times.

Hook for "make every trim now, in a fixed order": the narrowest waists are `finish*` and
`Body::new` in ops for new results, and the tail of `with_body` (after `with_rollback` keeps the
edit, `session.rs:712-722`) for in-place edits. Inside the kernel the candidates are the
`refresh_cached_topology` call sites (they mark "the public operation returns") or a new method
that replaces them. The `kernel_named_edit` tail is the wrong place: it runs per low-level edit
(every edit of a stitch ends there, `brep/p3.rs:1550-1552`).

A fixed order does not by itself give a fixed result: section 3.6 shows the trims of one edge
interact, so the order must be defined over edges (all uses of an edge together) and the
tolerance raises settled before neighbours are fitted.

### 5.3 How records get their trims

`to_record` lives in `solidcore-convert` (`solidcore-convert/src/brep/record.rs:130`), not in
`solidcore-record`, which has no kernel `Brep`. It takes `&mut Brep` and first runs, for every
edge use on a face that holds no trim of its own, in `brep::p2::edgeuses(b, None)` order,
`get_or_create_uv_trim_curve(k, b, eu, false, false, true)?.status?` (`:131-136`); only then does
it read `&Brep`. That is the per-edge-use path: no neighbour end fix-up and no vertex raise from
the face-level routine, but the edge raise, interval change and cross-face trimming all apply.
ops reaches it through `Body::to_record` → `with_record` → `with_body` (`body.rs:491-517`), so a
record export is an edit under rollback and holds the body's lock exclusively.

## 6. `Shared<Arena>` copy-on-write

Definition: `struct Shared<T>(Arc<T>)` (`topology.rs:176-224`).

- `share()` bumps the refcount; `Clone` is a **deep** copy (`Arc::new((*self.0).clone())`);
  `copy(share)` picks one.
- `Deref` reads through the Arc. `DerefMut` is `Arc::make_mut`: the first write through a handle
  whose Arc is shared clones the whole arena; with a refcount of 1 it writes in place.
- Only `edges` and `edgeuses` use it, and only `Brep::copy(true)` shares them, which only
  `clone_sharing_surface_caches` calls (`topology.rs:1195`). Standing copies, trials and snapshots
  call `copy(false)`.

Implications:

- **Whoever writes first pays, including the original.** While a sharing copy is alive, the first
  `edge_mut` or `edgeuse_mut` on the *original* clones the original's arena with `Arena::clone`,
  which is the derived `Clone` of `Edge` and `Edgeuse` and therefore `Curve::clone`, the copy
  constructor that resets every curve's owner. This is documented as the reason `snapshot` does
  not share (`topology.rs:1331-1336`: the result fails the check with Test #61, Bad Edge). The
  current sharing sites keep the copy in a local or a short-lived cache, so the original is not
  written meanwhile, but nothing enforces it.
- **The copy's first lazy write copies both arenas' worth of curves.** Any trim creation on a
  sharing copy (the purpose of four of its five sites) triggers a full edge-use arena clone, and a
  tolerance raise a full edge arena clone.
- **Granularity is the arena.** There is no per-entity sharing and nothing records which entity
  changed. A per-entity edit counter cannot be derived from the Arc: `make_mut` tells you only
  that some writer touched some edge. A counter would have to live in the record (and be bumped at
  the setters in section 2.3) or in a side table keyed by id; ids are stable (`Arena` never reuses
  a slot, `topology.rs:226-229`).
- **Threads.** `Arc::make_mut` is safe when each thread owns its own `Brep` value: a writer
  clones away and readers of the other value keep the old arena. Sharing one `&Brep` across
  threads never reaches `DerefMut`. The other eight arenas are plain `Arena`s and are copied
  eagerly.
- Whether `Brep` is `Sync` is **UNVERIFIED** (nothing was compiled). The interior-mutable parts
  found are thread-safe types: the surface `CacheSlot(Arc<Mutex<SlotState>>)`
  (`surface/surface/p2.rs:509`) and `MadeKindSlot(Arc<OnceLock<..>>)` (`:549`). The only
  compile-time assertion found is `Kernel: Send` (`kernel.rs:1157-1160`).

## 7. Facts most likely to break "between operations a body is immutable and any number of threads read it through `&Brep`"

1. **Reads through `&Brep` already mutate shared state.** The trimmed cache is taken out of the
   surface's slot and put back by read-only paths (`intersect/point_class.rs:2477-2483`,
   `query/face/p2.rs:1127-1137`). A second thread arriving meanwhile sees `CheckedOut` and gets a
   different answer path: `Unported` from `face_point_test_on`, or the slow classification. The
   mutex makes it memory-safe, not deterministic.
2. **There is no measure-only trim.** Every path that produces a missing trim writes the edge's
   interval, the other faces' trims, the tolerances and the mode list (section 3.1). "Gather" for
   a body with missing trims cannot run on `&Brep` today.
3. **Trim creation is order-dependent across faces** (section 3.6). Two workers each making "their
   face's" trims touch the same edge record, and the result depends on who went first. A
   pre-pass must be per edge, in a fixed order, with raises settled first.
4. **Tolerances are raised by reads and cascade** (edge → vertices; face → edges → vertices), and
   a raise changes later answers: fit tolerance, knot snap, intersection tolerances. Three raisers
   are reachable from queries and only one honours `validate_only` (sections 1.5, 3.4).
5. **Gap memos go stale without notice.** Invalidation is by notification only; `set_interval`
   sends none, and `edge_ccw_edge_gap` is never reset by any notify (section 1.3). An immutable
   body freezes whatever stale value it holds, and a thread that recomputes gets a different
   number from one that reads the memo.
6. **Mode flags change what a read answers and are toggled by reads**: `editing_enabled` inside
   the trimmed-cache build (`surface/trim_srf_cache.rs:822-826`), `surface_owner_cleared` inside
   every trim creation, `validate_only` inside validation. They are fields of the shared body,
   not of the reading thread's scope.
7. **Standing copies are not isolated.** They share the cache slot, the made-kind cell and the
   continuity memo with the original (`surface/surface/p2.rs:964-976`). A worker scribbling on its
   own standing copy still stores caches into, checks caches out of, and (on a transform or
   `swap_uv`) deletes caches from, the slot the other threads read (section 4.2).
8. **A cache built on a copy answers for the original.** The slot's cache embeds the copy's trims
   and tolerances at build time; `face_was_modified` is the only staleness signal, and it is set
   by notifications on whichever body was edited.
9. **The restart protocol reruns the solve from its start on a changed body**
   (`topology/shell.rs:1099-1113`, `boolean/topology_intersector/p1.rs:4723-4741`), with stated
   quadratic worst case and "raised tolerances" between attempts (`query/brep/ray.rs:10-15`). In a
   gather phase the request would have to be queued and the whole gather repeated after apply.
10. **There is no version on the body or its entities.** No counter, generation or dirty set
    exists, so nothing can cheaply prove "this body is the one the cache or memo was computed
    for". `RayPreparation` compares a `*const Brep` address for that purpose (`ray.rs:69-77`),
    which breaks as soon as bodies are moved, swapped (`with_rollback_untouched` swaps in place,
    `topology.rs:1453`) or copied.
11. **Port-derived topology is false inside operations** (`Edge.vertices`, `Face.shell`,
    `Face.loops`, `Loop.face`, `Loop.edges`, `Shell.faces`) and true only after a hand-placed
    `refresh_cached_topology`. A body handed to workers mid-operation carries stale copies of its
    own topology.
12. **`&mut Brep` is the calling convention of the read stack, not an exception**: about 207
    functions across `query/`, `mass/`, `intersect/` and validation, including every mass
    property, every point classification, `check_brep` and `to_record`. Most need it for causes A
    and B only, but the signature propagates to every caller.
13. **Some reads write back on purpose.** `CurveOnClassifyCache::apply_uv_trim_updates` copies
    trims, tolerances and gaps from a private snapshot onto the body
    (`query/face/p2.rs:1561-1602`), and `to_record` leaves its trims on the body. These are
    apply-phase work hidden inside reads; under an immutable body they have to be returned as
    results.
14. **Copy-on-write punishes the original.** With a sharing copy alive, the original's next
    edge write clones its arena with the owner-resetting `Clone` (section 6). Any design that
    hands workers `clone_sharing_surface_caches` copies while a writer proceeds reintroduces
    Test #61.
15. **Identity is issued lazily on the body** (`notify_identity`, `progress_tag`, `removals` all
    need `&mut self` on first use), and a `Snapshot` deliberately carries the *same* serial as
    the live body (`topology.rs:1392-1393`). Two values with one identity is safe only while
    exactly one is live.
