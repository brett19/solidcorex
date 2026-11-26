# Signature-flip sizing: which callees force `&mut Brep` on four read entry points

Method: flip an entry point to `&Brep`, `cargo check -p solidcore --lib` on the build server, read the errors, push `&` one
level down into each errored callee, repeat. Final state of every tree is on branch `cache-size-probe` (tags
`e1-done`, `e2-done`, `e3-done`; the work is cumulative). Check time per round about 30 s (warm), 30 rounds in all.

Probe conventions: a write leaf that sits behind a typed call is replaced by `&mut b.clone_standing_for_self()?
/*LEAF*/` so the walk continues (`git grep -n "/\*LEAF\|/\*FRONTIER" -- '*.rs'` lists every cut, 26 sites). Where a
leaf is a direct field write (E0596 on `b.xxx_mut()`), the error is left standing as the leaf. Behaviour is NOT
preserved by the probe; only the signature graph is meaningful.

Not done: Level0 versus Level1/2 split inside `check_brep` was not separated rigorously (flipped the whole
validation module in one round; leaf conditions below say what each write is guarded by). Entry 4 needed no flips.

## Headline

- No entry point has an `&mut` that is needed for every body. Every write found is guarded by "something is missing"
  (a UV trim, a gap cache, a built surface cache) or is a stamped mode-flag toggle that is restored, except the
  trait/struct boundaries (below) where the write is in code the compiler could not see through.
- The same single leaf, `edgeuse::p3::get_or_create_uv_trim_curve` (and `face::p3::create_uv_trim_curves`), is
  the bottom of all four entry points.
- Prior read-only machinery already exists and is half-used: `LazyBrep` (copy on first edit), `BrepClassifier`
  (private clone for shared classify), `QueryTopology::Prepared`/`Requested` + `point_classify_prepared`
  (errors `Unported`/`PreparationRequired` instead of making a trim), `vertexuse::Reading` +
  `compute_uv_sector_read_only`, `normalized_evaluate_read_only`, `max_edge_face_gap` cached-read variant,
  `has_seam_problem`/`_cached` (shared) beside `_mut`. check_face already runs on `clone_standing_for_self()` when a face
  lacks trims (pointers.rs about 470, comment "checking does not change the body").

## Entry 1: mass (`brep_volume_default`, `brep_area`) - CLEAN FIXPOINT, 6 rounds

20 functions flipped, all pure pass-throughs. Compiles with only the leaf cuts below.

```
brep_area / area_about / brep_volume_default / brep_volume          mass/integrator/properties.rs:2300,2392,~2325
  faces_area_about (2870) / brep_properties (1977) / brep_properties_in_box / properties_in_box (per-face loop 2063)
    face_properties -> face_properties_in (2051; trims 1 via `boundary`/`closed_boundary`)
      boundary (272) -> oriented_boundary (280) -> closed_boundary (298)
        topology/face/p1.rs create_oriented_trim_boundary (743)
          create_uv_trim_curves (259, local) -> LEAF face::p3::create_uv_trim_curves   [guard: !have_trimming_curves, p1.rs:763]
          normalized_evaluate_uv (327) -> edgeuse::p3::normalized_evaluate (3405)
             -> LEAF get_or_create_uv_trim_curve                                        [p3.rs:3434: always called, returns early if trim exists]
          fetch_uv_trim_curve (335) / get_or_create_uv_trim_curve (347, local) -> LEAF eu3::get_or_create_uv_trim_curve
    gap_closure (1645) -> LEAF eu3::get_or_create_uv_trim_curve  [properties.rs:1665, guarded by `uv_trim_curve(..).is_none()`]
    qbrep::calculate_bounding_box(k, b, ..) : takes `impl Into<LazyBrep>`; with `&Brep` no error and no copy (loose box never calls get_mut)
```
Write leaves: `eu3::get_or_create_uv_trim_curve` (3 call sites), `face::p3::create_uv_trim_curves` (1). Nothing else.
Surprise (interior mutability, no signature to flip): `Unclamped::where_trims_may_leave` (properties.rs ~447) toggles
`BSplineSurface::set_out_of_bounds_enabled_in_place` / `set_continues_past_domain_in_place` on the face's surface through
`&Surface` (AtomicBool in nurbs/b_spline_surface/p1.rs ~426) and restores on drop. `NurbSlot` is `Arc<OnceLock<Surface>>`
(lazy NURB fill from `&self`). So the body is written during the read even with every trim present (a flag
toggle that is restored; visible to anything running concurrently on the same surface).

## Entry 2: `check_brep` - 95 signatures flipped (e1 to e2), 16 rounds (bulk flip in round 8)

Chain: `check_brep` -> `check_brep_scoped` -> `check_brep_counted` -> `check_brep_with_keys(_scoped)` ->
`check_with_keys_counted` (assert_valid.rs 363-451) -> `validate_pointers_traced` (pointers.rs 397) and `assert_valid_with`
(assert_valid.rs 542) -> assert_valid/{brep,edge,face,loops,seams,vertex}.rs (about 40 fns) and pointers.rs (13 fns:
check_edges, face_contact, check_vertex_exclusion/clearance, check_edge_geometry, edge_geometry ...). Callees that flipped
cleanly outside validation: `trimming_tools::check_face/check_loop/edge_uv_extremes/face_*`, `edgeuse::p1::max_edge_face_gap`,
`calc_max_edge_face_gap`, `vertexuse::vertex_edge_gap/vertex_face_gap/calc_*/set_*/compute_uv_point/sector/is_sector_missing_seam`,
`edgeuse::p2` binormal step-off family (5), `p3::evaluate_binormal(_uv)`, `shell::is_inner_shell_of`, `face::assert_tests` (6),
`edge_form`, `corner::is_within_xsect_tol_3d_of_faces`, heal/gap.rs gap family (7 names, 11 signatures: `Gap::end_points`
-> LEAF get_or_create_uv_trim_curve at heal/gap.rs:1120), `query::face::p2::point_classify` (via Prepared stub),
`point_3d_classify`. Caveat on the count: it includes `validate_and_update_tolerances` and `validate_and_update(_body)`,
the tolerance UPDATE path (a real writer when `validate_only` is false); it only flips because `set_tolerance` is cut
there (assert_valid/brep.rs 399, 477), so about 3 of the 95 are not clean. check_brep itself passes `validate_only = true`.

Write leaves left as compiler errors (final state r22, all in the check path):
| leaf | where | writes when |
|---|---|---|
| `b.set_validate_only(..)` | assert_valid/brep.rs:283,285 (`validate_and_update`, from `tolerances` check) | always, toggled and restored (`Stamped<bool>`, so `write_stamps` may move) |
| `fix_reversed` / `fix_reversed_on` (edge curve, UV trim `_mut`) | pointers.rs:2204, 2278 in `edge_geometry` | only when `reversed_needs_fix`: a reversed curve |
| `edgeuse_mut().uv_trim_curve` fix | pointers.rs:2237 | only with `fix_tolerances` (healer's `validate_geometry`; `check_edge_geometry` passes false) |
| `calc_max_edge_face_gap` cache writes | edgeuse/p1.rs:987,989,1040,1043 | `cache` true: `max_edge_face_gap(..)` calls it when the gap is uninitialised or `force_calc` (check passes false) |
| `calc_vertex_edge_gap` / `calc_vertex_face_gap` cache writes | vertexuse.rs:570-605, 649-667 | same: uninitialised or `force_calc` |
| `init_vertex_face_gap` | vertexuse.rs:735 | only `force_calc` (corner.rs:137; the check passes false) |
| `Curve::assert_valid` family takes `&mut Curve` | assert_valid/edge.rs:101 (`curve_in_walk`, `kind_mut()`) | Parabola/Hyperbola `make_nurb` when the NURB is missing; the other per-kind checks not individually sized |
Cut leaves (typed, replaced by stub): `vertex/edge::set_tolerance` (brep.rs 399/477 only when `!validate_only`; pointers.rs:2224 only
`fix_tolerances`), `face::p3::create_uv_trim_curves` (trimming_tools.rs:350, check_face's trim making), `get_or_create_uv_trim_curve`
(assert_tests.rs:362, face/p3.rs:957, edgeuse/p3.rs:3050 `evaluate_binormal_uv`, 3185 `create_uv_trim_curve`).
Trait/struct boundaries (frontier, unflipped): `BrepFaceServices::new/with_caches` (struct holds `RefCell<&mut Brep>`, query/brep/ray/live.rs 614;
reached at pointers.rs:1642 `face_contact` exclusion and seams.rs:123 `surface_assert_valid`: builds the face trimmed surface cache and creates UV trims);
`CurveOnClassifyBrep`/`ClassGeometryMut` only implemented for `BrepSeamObjects<_, &mut Brep>` (face.rs:288 `has_seam_problem_in_brep_mut`;
face/p2.rs `classify_curve_on_face_in_place`); ray solve `p9::find_infinite_region` and `shell::ray_brep_solve` (region nesting, brep.rs:115, shell.rs:1838);
`SectorBrep` {Changing(&mut), Reading(&)} in vertexuse.rs 757 (probe uses Reading).

## Entry 3: point classification - 11 more signatures (e2 to e3), 8 rounds, frontier reached

`BrepQuery::point_3d_classify` (query/brep.rs 825) flipped by rewriting `classify` to take `&Brep` (the existing shared form is
`BrepClassifier`, which clones the whole body via `Target::mutable()` at the first face solve or ray fire: that clone is the
hidden write). Flipped: `bounding_box`, `solve_mutable_brep_point(_kept)` (ray/live.rs), `ray_fire_with_bbox_and_solver` closure type
(topology/brep/p2.rs 2136), `face_surface_cache`/`surface_cache_of_face` (query/face/p2.rs), `find_sector_of_face`, `radial_sector_of_curve_classify`,
`face_classify_uv` (api_queries.rs 938; one hop to `point_classify`, which only needed the `Mutable`->`Prepared` cut). Stops at 4 errors, all frontier:
`LiveBrepObjects::new/with_face_caches` (live.rs 72, 92: wraps `BrepFaceServices`) and `BrepCacheFace { b: &mut Brep, f }` (query/face/p2.rs 594, 612), the
implementation of trait `surface::trim_srf_cache::CacheFace<O>` whose methods write: `set_brep_editing_enabled` -> `Brep::set_editing_enabled`,
`create_uv_trim_curves` -> `face::p3::create_uv_trim_curves(.., true, ..)`, `vertexuse_uv_point(&mut self)` -> `compute_uv_point` (Changing).
Real leaf for trims: `QueryTopology::Mutable` arm of `ensure_trim_curve` (topology/loopuse.rs 159) -> `get_or_create_uv_trim_curve` (only if trim missing).

## Entry 4: bounding box - 0 flips

`calculate_bounding_box` and `calculate_tight_bounding_box` already take `impl Into<LazyBrep>`; `&Brep` compiles. The loose box never writes
(no `get_mut`). The tight box has ONE edit site, `query/face/p3.rs:851` `lb.get_mut()?` passed to `point_classify`: on `&Brep` it clones the
body at the first face (tight-box PointClassify may attach UV trims and `SetUVTrimCurve` raises tolerances: module docs p3.rs 46-56). With trims
present and `point_classify_prepared` it needs no copy. `lock_bounding_box`/`set_locked_bounding_box` are real writers, not reads.

## Write leaves ranked by entry points reached (condition on a body with all UV trims and gap caches filled)

| leaf | E1 | E2 | E3 | E4 | writes on a fully prepared body? |
|---|---|---|---|---|---|
| `eu3::get_or_create_uv_trim_curve` (+`create_uv_trim_curve`, `set_uv_trim_curve`, tolerance raise) | yes | yes | yes | yes | No: `existing_uv_trim_curve` early return (edgeuse/p3.rs 1701). Also nothing if `uv_space_ok` false |
| `face::p3::create_uv_trim_curves(_body)` (+`begin/end_surface_owner_cleared`, `sync_face_surface_tolerance`, surface owner clear) | yes | yes | yes (CacheFace) | via E3 | Not when called under its "any missing" guard (p1.rs:763); the function itself writes unconditionally, so every caller needs the guard |
| `QueryTopology::Mutable` / `ensure_trim_curve` | - | yes | yes | yes | No, only on a missing trim |
| surface/trim cache build on the body (`BrepFaceServices`, `face_surface_cache`, `BrepCacheFace`, `set_editing_enabled`) | - | yes | yes | yes | Unknown: depends on whether the face's trimmed cache is kept; not sized (trait boundary) |
| gap caches (`calc_*_gap` cache=true, `init_vertex_face_gap`) | - | yes | - | - | No when `is_init()` and not `force_calc` |
| `set_validate_only` | - | yes | - | - | Yes (restored toggle) |
| `fix_reversed(_on)` | - | yes | - | - | No unless a curve is reversed |
| `Curve::assert_valid(&mut Curve)` | - | yes | - | - | Parabola/Hyperbola only when NURB missing; rest unsized |
| `set_tolerance` edge/vertex | - | cut, not in check | - | - | not reached by `check_brep` (needs `!validate_only`) |

Needed on a body for E1/E2/E4 reads to write nothing: every edgeuse has its UV trim (or its mate does), `max_edge_face_gap` and both
`vertex_*_gap` caches initialised, no reversed curves, and for E2/E3 the face trimmed surface caches kept (unsized) and `validate_only` toggle accepted or removed.

## Shared pass-throughs (flip first)

`eu3::normalized_evaluate` (E1,E2 via pointers.rs/vertexuse, E3), `face/p1.rs::normalized_evaluate_uv` (E1, E2 via edge.rs), `query::face::p2::point_classify`
(+`QueryTopology::Prepared` cut: E2,E3,E4), `trimming_tools::check_face` and `face::p3::create_uv_trim_curves` wrapper `face_create_uv_trim_curves` (E2, E1 via p1), `bounding_box`/`calculate_bounding_box` callers, `vertexuse::compute_uv_point`
(E2,E3). Real work to enable reads is three things: (1) a "has all trims" precondition guard in front of the 2 trim leaves, (2) gap-cache read variants
(exist for edge/face), (3) making `BrepFaceServices`/`CacheFace` take `&Brep` plus a prepared cache.

## Surprising

- `validate_and_update_tolerances` and the whole tolerance-update path flip with the same signatures as the check: only `validate_only` separates them.
- `BrepFaceServices` holds `RefCell<&'a mut Brep>`: borrow checking is deferred to runtime (`borrow_conflict` errors).
- Interior mutability in surfaces (AtomicBool flags, `OnceLock` NURB, `BrepQuery.bbox: OnceCell`) bypasses `&`.
- `LazyBrep`/`BrepClassifier`/`Reading` silently clone the body or surface on `&Brep` inputs (a perf cost, not an error).

## State

Local a local worktree, branch `cache-size-probe` (about 31 local scratch commits "probe rN", tags
base, e1-done, e2-done, e3-done), never pushed to fork/origin; side ref `push/cache-size-probe` on the build server's solidcore clone. Server worktree removed (see final message).
