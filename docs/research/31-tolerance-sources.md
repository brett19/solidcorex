# 31 - Where tolerances live: kernel, context, body, entity

Read-only inventory of sc-wt/cache-tree-slot (solidcore main). Paths are relative to
solidcore/crates/solidcore/src (`K/`) unless noted. READ = verified in code; INFERRED = deduced.
Counts are regex counts over non-test library code (see "Method"); treat them as +-10%.

## 0. Method
Non-test set: all `.rs` under `K/` minus (a) any `#[cfg(test)]` item (brace-matched, to its end), (b) files
that a `#[cfg(test)] mod x;` declares, (c) files named `tests.rs`, `*_tests.rs`, `tests_*.rs` or under a
`tests/` dir. 797 of 827 files remain (866,736 lines). Scripts were in /tmp (not in the repo). Entity-value
counts are noisy because `.tolerance` also matches non-zone fields; constants include import and definition lines.

## 1. The levels

### 1.1 Storage (READ)
- Kernel: `Kernel.tolerances: Tolerances` (K/kernel.rs:682, 841): zone 1e-5, xsect 2e-5, approx 5e-6,
  ang 0.5 deg, gap gain 1.1, size 50, ratio 1000 (kernel.rs:47-95). Set only by `Kernel::new(&KernelOptions)`
  (kernel.rs:1008); both app pools use `KernelOptions::default()` (crates/smlib/src/core.rs:81,
  solidcore-ops/src/session.rs:201). No non-test code sets a non-default Tolerances except four temporary kernels
  (1.4). `xsect`, `approx`, `ang` fields are stored but xsect/approx are never read as fields: helpers derive
  them from zone (tol.rs:239-257, 405-431). `ang_tol_deg` is read directly in 11 places (section 2).
- Context: `Kernel.context: Context` (kernel.rs:851; `Context::default()` at 1013) holds `model_size_estimate`,
  `large_small_size_ratio` (both `USE_DEFAULT`), and `zone_tol_3d` (geom/context.rs:243-247). `Context::new`
  derives zone as `size_estimate_to_zone_tol_3d(size)` (context.rs:597); for `USE_DEFAULT` that is the constant
  50 -> 1e-5 (tol.rs:330-334), NOT `kernel.tolerances.zone_tol_3d`.
  Writers: `set_this_model_size_estimate` (context.rs:797) / `set_this_large_small_size_ratio` (:804). No
  non-test caller writes the kernel context (only tests: brep/p1.rs:4292, fillet live.rs:773, tol.rs:1475).
  So the context is always the default 1e-5 / USE_DEFAULT in production (READ for writers; INFERRED "always").
- Body: `Brep.tolerance` (topology.rs:776), `model_size_estimate`, `large_small_size_ratio` (:801-803).
  `Brep::default` writes tolerance = constant `ZONE_TOL_3D`, size/ratio = `USE_DEFAULT` (topology.rs:1025-1034).
- Entity: `Face.tolerance` (topology.rs:480-495, default 0), `Edge.tolerance` (:427, default `ZONE_TOL_3D` at
  :2307), `Vertex.tolerance` (:390, :2229 sets `ZONE_TOL_3D`). Edge uses have NO own value: `edgeuse::tolerance`
  returns the edge's, or constant `ZONE_TOL_3D` with no edge (topology/edgeuse/p1.rs:477-482). Curves and
  surfaces carry a cached owner tolerance (`Curve::set_owner_zone_tol_3d`, edge.rs:~419;
  `Surface::face_zone_tol_3d`, surface/surface/p1.rs:725-735); ownerless ones fall back to the kernel.
- Angle: there is NO body or entity angle tolerance (no field in Brep/Face/Edge/Vertex; READ by topology.rs
  structs). Angle comes from `Tolerances.ang_tol_deg`, constants `ANG_TOL_DEG/RAD` (tol.rs:212-214), or an
  operation parameter: the surface intersector `this_ang_tol_rad` (intersect/surface_intersector/p1.rs:906,
  20 deg), the boolean intersector (set to 20 deg when 0: boolean/merge/p2.rs:208-210), fillet solver
  `ang_tol_rad` (fillet_solver/p1.rs:902), `Option<f64>` params (topology/vertex/corner.rs:216).
- Gap gain and ratio: kernel field or constant only (a body stores a ratio, see 1.2; nothing reads it, 2.3).

### 1.2 Fallback chains (READ, tol.rs)
- Zone, body: `brep_zone_tol_3d(k,b)` = `zone_tol3d(k, b.tolerance())` (tol.rs:715-724): the body's own value,
  no kernel/context fallback (kernel only via the Uninit sentinel). `brep_xsect/approx` = 2x / 0.5x of it.
- Zone, context: `context_zone_tol_3d(k, ctx, _)` (tol.rs:393-402): `ctx.zone` if `Some`, else
  `k.tolerances.zone_tol_3d`. `None` is how 10 non-test sites read the KERNEL value (2.2).
- Zone, kernel: `zone_tol_3d(k, small)` (tol.rs:377-390) = `k.tolerances.zone_tol_3d`; for `SmallSize` times
  the kernel ratio. `SmallSize`/`ForceNormalSize` are never passed outside tol.rs (grep), so the ratio branch is dead.
- `zone_tol3d(k, d)` / `xsect_tol3d` / `approx_tol3d` (tol.rs:228-257): `d` unless `d == USE_DEFAULT`, then the
  kernel's zone (x2, /2). Most calls pass a real number (fabs only); 8 pass `USE_DEFAULT` = kernel read
  (query/api_queries.rs:918, intersect/api_intersectors.rs:702,849, intersect/views.rs:102, blend/weld_bead.rs:235,
  blend/fillet_geom/p1.rs:872, topology/proto_topology/p3.rs:1534-1535, intersect/curve_class/p5.rs:501).
- Entity-owned object (point class): `object_zone_tol_3d` (intersect/point_class.rs:1870-1935) = the topology
  object's own value if the object is vertex/edge/face/edge use; kernel value when no object / non-topology
  object; constant `ZONE_TOL_3D` for a region (:1931).
- Size estimate: `Brep::this_model_size_estimate(k, chase)` (topology/brep/p1.rs:199-208): own, else
  `k.context.this_model_size_estimate` (context.rs:786) = own, else `k.tolerances.model_size_estimate`.
  Ratio: own, else context, else `k.tolerances.large_small_size_ratio` (p1.rs:212-226).
  `brep_size_zone_tol_3d` (tol.rs:669-677): stored estimate -> `size_estimate_to_zone_tol_3d`; if `USE_DEFAULT`
  -> the CONTEXT's zone (1e-5 constant-derived), not the kernel's `zone_tol_3d`.
- Consequence (READ): `kernel.tolerances.zone_tol_3d`, `.model_size_estimate` and the context zone are three
  independent numbers; changing the kernel's zone changes no body tolerance. `new_brep` (topology/brep/p3.rs:632-669)
  sets `b.tolerance = brep_size_zone_tol_3d(k,&b)` = context zone = 1e-5.

### 1.3 Who writes a body's tolerance (READ)
- Construction from nothing: `new_brep(k, USE_DEFAULT, USE_DEFAULT)` (p3.rs:660-669; callers primitive/.../model.rs:169,
  sweep store.rs:482, fillet session.rs:1791...) -> tolerance = context zone = 1e-5 (default context). Primitives then
  `set_tolerance(k,b,tol,true,true)` (p2.rs:381; grow-only) with the caller's tolerance (primitive/.../access.rs:71);
  `tolerance <= 0` computes `ZONE_TOL_3D * bbox diagonal` (p2.rs:396-407). Some API paths write the raw field
  (`brep.set_tolerance(t)`, primitive/api_primitives.rs:657). A primitive with no tolerance argument = 1e-5.
- Raw/entity writers: `Brep::set_tolerance` (topology.rs:1801); `brep::p2::set_tolerance` also raises every face/edge/vertex
  (p2.rs:381-). `edge/vertex/face set_tolerance(..., update_only_if_larger)` (vertex.rs:195, face/p3.rs:422).
- Boolean of two bodies: dest copy absorbs source with `brep_set_tolerance(dest, src.tolerance(), true, true)`
  (topology/brep/p7.rs:2348, 2661, 2809): the result takes the LARGER of the two (grow-only); entities keep their own.
  Before it, ops runs `matched_tolerances` (boolean/operand_tolerances.rs:57-85; ops boolean.rs:168,253,335,423):
  if the coarse operand's tolerance exceeds `max(0.25*shortest edge of fine, fine tol)`, it is lowered (only when a
  trial copy passes the full check) to that. After: `update_tolerances_after_boolean` (ops boolean.rs:84).
  Zero-approx-tol request uses `0.5*min(a.tol, b.tol)` (boolean/merge/p2.rs:199).
- Import (kernel file/part): `make_topology_from_data` -> `zone_tolerance(b, data, force_same_tol)`
  (topology/brep/p10.rs:652-686): data zone as-is when `force_same_tol`, else clamped to `[1e-8,1e-4] x vertex-box size`
  (default `1e-5 x size`). Entity values `max(entity data, body)` (p10.rs:849, 899, 1152). `BrepData` defaults are
  local constants (topology/brep_data.rs:112-118, 1295-1298); its `model_size_estimate` is not applied to the body (grep: no read
  in p10.rs). Body size/ratio stay `USE_DEFAULT`.
- Healer after import (untrusted data): sets body size estimate from longest edge bucket and `set_tolerance(zone, true,true)`
  = grow-only (heal/heal_data/p2.rs:3037-3075, 1998-2001). INFERRED that it runs for non-trusted app imports.
- Transform with scale: `scale_tolerances` -> `b.set_this_model_size_estimate(bbox diagonal)` then each entity
  `max(zone_of_size, own * max|scale|)` (topology/brep/p5.rs:3743-3775). A pure rotation/translation keeps all.
- Operations (offset, sweep, fillet, trim, heal): grow-only `set_tolerance` (offset_executive/merge.rs:180, step_gaps.rs:160,
  p2.rs:1567; trimming_tools.rs:270; fillet session.rs:5465; sweep store/merge.rs:752). Copies carry it (brep/p3.rs:751).

### 1.4 Kernel-as-carrier of entity values (READ)
Four sites build a throwaway `Kernel` with `zone_tol_3d` replaced by an edge's tolerance because owner-less
`Curve::trim` reads the kernel: topology/edgeuse/p3.rs:1215, blend/fillet_executive/p1.rs:1510, blend/fillet_solver/p1.rs:3701,
p2.rs:3252 (comments say so). Also `cache_kernel` copies `k.tolerances` (boolean/merge/p4/kernel_adapter.rs:60) and
mirror (topology/brep/mirror.rs:340). So the kernel field is already the plumbing for a per-entity value.

## 2. Read-site classification (non-test, regex counts)
Classes: (a) entity/body own, (b) context, (c) kernel field (direct or via a resolving helper), (d) constant.
Per quantity:
| quantity | (a) | (b) | (c) | (d) raw lines |
|---|---|---|---|---|
| zone | ~900-1100 (`.tolerance`, `edge/face/vertex::tolerance`, `brep_*`) | 3 callers of `brep_calc_*` w/ ctx + 4 `context_*_tol_3d(Some)`; 10 `context_*(None)` = kernel | 62 (5 `k.tolerances.zone_tol_3d`, ~47 `zone_tol_3d(k,NormalSize / surface)`, 10 `contact_tol_3d`) + 8 `zone_tol3d(k,USE_DEFAULT)` | 131 (~64 real uses in 36 files, rest imports/defs; 4 files redeclare it locally) |
| xsect | via `xsect_tol_3d_from_zones`, `brep_xsect` | 2 | 0 direct (derived from zone) | 5 (heal/gap.rs:90, surface/surface/p5.rs:131) |
| approx | `brep_approx` | 1 (curve_cache.rs:602) | 12 helper calls (`approx_tol_3d(k,..)`) | 9 |
| angle | none exist | none | 11 (`k.tolerances.ang_tol_deg`; 10 in blend, 1 topology) | 53 (~39 uses) |
| gap gain | - | - | 5 (`gap_gain(k)`, + tol.rs calc_gap) | 1 (def) |
| size | `brep_model_size_estimate` 1 (heal p2:3058) | 2 | 1 (context.rs:788) | 4 (defs) |
| ratio | 0 readers | 0 | 1 (brep/p1.rs:219 fallback) | 14 (`LARGE_SMALL_SIZE_RATIO * b.tolerance()`) |
Per module, `a / b / c / d` (zone+others, rough): topology 401/6/16/56; blend 179/0/12/9; query 95/5/4/5; heal 73/5/4/8;
offset 69/0/2/2; intersect 62/0/10/19; boolean 55/0/1/0; surface 51/0/27/46; primitive 38/0/0/16; curve 38/7/7/11;
sweep 33/0/3/12; geom 10/1/1/4; eval 6/0/0/0; nurbs 5/0/3/18; mass 3/0/0/0; prism 1/0/2/0. (d counts include imports.)
Outside the kernel crate: solidcore-ops (a): measure.rs:235,1130,1454,1858 (`brep_zone_tol_3d`), extend, pick, split, trim, untrim,
surface.rs:325; (c): `context_zone_tol_3d(k, None)` in curve_fit.rs:163, curve_plane.rs:43, network.rs:62, sweep.rs:599 (`zone_tolerance()` exposes
it), surface.rs:260,297; prism.rs:165 uses `Tolerances::default().zone_tol_3d` (constant default, not the kernel's value).
crates/smlib: sweep.rs:482 (kernel), many `b.tolerance()` (body).

### 2.1 Notable (d) sites
Deliberate fixed values (guards/steps, scaled `/10`, `/2`): pole/degenerate tests `ZONE_TOL_3D/10` (nurbs/b_spline_curve_n_lib.rs:2900,
b_spline_curve/p3.rs:2312, surface/surface/p1.rs:200, assert_valid.rs:572,787, geom/geom_utility/p2.rs:1257-1374, trimming_tools.rs:734-738,2826,
brep/p7.rs:205, intersect/surface_drop_lib/cylinder_section.rs:93, trim_srf_cache.rs:1139); step sizes (fillet_solver/p1.rs:3204; b_spline_surface_n_lib/p2.rs:1925);
validity thresholds in assert_valid (brep/validation/assert_valid/face.rs:60, edge.rs:687; surface/volume/assert_valid.rs:390); angular guards
(`ANG_TOL_*`: edge.rs:206,466; fillet_solver/p2.rs:1700,2032; surface/p1.rs:3093; intersect/curve_class/p4.rs:2094-2298 `is_parallel_to(.., ANG_TOL_DEG)`;
offset crossing_loops.rs:236; corner.rs:229,346; b_spline_surface_n_lib CONTINUITY).
Looks like body/entity value was intended (a constant stands in because no owner is at hand, or the C++ used a default):
- Owner-less fallbacks: topology/edgeuse/p1.rs:480 (edge use without edge), surface/plane/p2.rs:226 (`NoFace`), query/face/p2.rs:336,
  intersect/point_class.rs:1931 (region), surface/surface/p5.rs:1193 (`face_zone_tol_3d().unwrap_or(XSECT_TOL_3D)`).
- Operations with a body but constant in the API: sweep/api_brep.rs:437,489,542,598,644,1001; primitive/api_primitives.rs:666,918,952;
  api_surface_ops.rs:254; api_curve_ops.rs:203; topology/api_projection.rs:378; query/api_queries.rs:145,256; query/deviation.rs:118;
  nurbs/b_spline_curve/p2.rs:1304, p3.rs:1054 (`if tol > 0 {tol} else {ZONE_TOL_3D}`); brep/p5.rs:2925 (`100*ZONE`); curve/curve/p6.rs:2141.
- Ratio: `LARGE_SMALL_SIZE_RATIO * b.tolerance()` (wide vertices: validation/wide_vertices.rs:51; pointers.rs:994; assert_valid/brep.rs:398,475;
  seams.rs:196; heal_data/p2.rs:3152,3231; edgeuse/p3.rs:1940,2107): body tolerance times the CONSTANT ratio; the stored body/context/kernel ratio is never read.
- `approx_tol_3d = ZONE/2` constants: vertexuse.rs:135 (singularity tests 1773-1793), crv_on_surf.rs:178 (bs approx 1489,1601).
Not (d) but kernel-wide on purpose: `contact_tol_3d(k)` (tol.rs:449) = kernel zone, documented as one value for every site "whatever tolerances earlier
operations grew the edges and faces to" (WR-787); 10 callers (surface/cone/p3.rs, plane/p2.rs, geom_utility/p1.rs, validation/tangent_sections.rs...).

### 2.2 Angle
Kernel field 11 reads (blend/blend_corner/rollover.rs:202,355,402,427,458,537,598,664; fillet_executive/session/two_offsets.rs:427;
blend/feature_executive.rs:1371; topology/brep/seam_turn.rs:111), constants ~39 uses, and operation parameters (intersectors). No per-body or per-entity
angle tolerance exists (READ). Intersector defaults 20 deg are literals (surface_intersector/p1.rs:906; merge/p2.rs:209), while `6*ang_tol_rad()`
sets 3 deg in curve_class (p1.rs:1681,1909; p2.rs:1504).

### 2.3 Size and ratio
`model_size_estimate` is read only to fall back (context.rs:788) and by the healer (heal_data/p2.rs:3058-3075); `large_small_size_ratio` has no
effective reader (`SmallSize` is never used). Their only effect is through body tolerance chosen at creation/scale/heal.

## 3. Imported body with non-default tolerance or scale (trace; READ unless INFERRED)
1. File read, `solidcore-convert` record: `data(k, rec)` (solidcore-convert/src/brep.rs:329-331) `zone = zone_tolerance(rec.tolerance)`, `d.zone_tol_3d`,
   `approx_tolerance`; edges/vertices `zone_tolerance(own)` (brep.rs:439,619), reject over `MAX_TOLERANCE` (:413,613); `own_tolerances` (:644-677) sets body
   = least of entity zones (face = least of its loop's), so no entity is raised. `make_topology_from_data(..., force_same_tol: true)` (brep.rs:290) keeps the data
   zone verbatim (p10.rs:656). Kernel tolerances are not consulted; body size/ratio = `USE_DEFAULT`.
2. App array import `crates/smlib/src/core/convert.rs`: `flat_to_brep_data` (:2183-2187) `zone = TOLERANCE/2`; vertices `zone` or `VERTEX_TOLERANCE/2` (:2258, 2514),
   edges `EDGE_TOLERANCE/2` (:2487), faces `zone` (:2298); `own_tolerances` (:2532-2560) lowers body/faces to the least; `force_same_tol: true` (:2599). Heal on
   untrusted arrays (`healer_enabled: true`, version `HEALER_VERSION-2`, :2590-2598) may only GROW the body tolerance by its size bucket (heal_data/p2.rs:3037+).
   Export writes `2 * max(own, body, faces)` (:1114,1128) and `xsect = 2*zone` (:829-832).
3. Point classification (`ops measure.rs:230-241`): `tol = brep_zone_tol_3d(k,b)` BODY value, then entity-owned values (`point_class.rs:1870-1935`).
   Body tolerance honoured. Leaks: surface/curve evaluation without owner use kernel value (`zone_tol_3d(k, surface)` fallback surface/p1.rs:731-735).
4. Boolean with a default box (ops boolean.rs:168/253): `matched_tolerances` lowers the coarse operand only if its tolerance > 0.25*shortest edge of the fine
   one; result = larger of the two (p7.rs:2661); entity tolerances in the intersector come from entities (`coincident_strip.rs:606-648`, `xsect_tol3d(k, a+b)`),
   `xsect_tol_3d_from_zones` sums them. Ignored in favour of kernel/constants: `contact_tol_3d(k)` (kernel zone, 10 sites) for circle/cylinder overlap
   (tol.rs:449-480); boolean angle 20 deg literal and 3 deg `6*ang_tol_rad()`; `ZONE_TOL_3D/10` pole/degenerate guards; `LARGE_SMALL_SIZE_RATIO` constant
   in the wide-vertex checks of the final validity test. A 1e-3 body therefore has a contact test at 1e-5 and a ratio limit derived from the body tolerance.
   A 1e-8 body is held to 1e-5 in `contact_tol_3d` and the `ZONE/10 = 1e-6` guards (INFERRED effect: guards coarser than the body).
5. Fillet: body value in `band_cuts.rs:364,727` (`brep_zone_tol_3d`), `approx_tol3d(k, 0.5*body.tolerance())` (session.rs:7420, finish.rs:909), edge tolerance in the
   four temp kernels (1.4). Kernel/constant: `k.tolerances.ang_tol_deg` in rollover (8), two_offsets.rs:427, feature_executive.rs:1371; `ANG_TOL_RAD`
   (fillet_solver/p2.rs:1700,2032-2035); `ZONE_TOL_3D` step (fillet_solver/p1.rs:3204), `tol_3d: ZONE_TOL_3D` (identify_fillet.rs:719); owner-less trims read kernel.
Places a body tolerance far from 1e-5 is ignored: all of 2.1 first two lists; contact_tol_3d; angle everywhere; the 4 `zone_tol3d(k,USE_DEFAULT)`-style
reads in api_queries.rs:918 (`face_classify_uv` takes the KERNEL zone for its classification though it passes the face tolerance as source), api_intersectors.rs:702,849,
weld_bead.rs:235; scaling imported bodies uses `scale_tolerances` growth but constants `/10` do not scale.

## 4. Cache identity and proofs today (READ)
- `FaceCacheKey` (query/brep/ray/live.rs:119-142): `k.tolerances` (all 7 fields), context `zone`, stored model size and ratio (bits),
  `doing_boolean`, `k.fixes`, `b.editing_enabled()`. Kept face caches serve only under equal key (live.rs:175-180). It does NOT include the body's own tolerance or
  size: that is part of the body whose caches they are (the holder is per-body, INFERRED from live.rs:144-150 comment "held by the operation that owns the B-rep's queries").
- ops `Settings::of(k)` (solidcore-ops/src/body.rs:78-97): `k.tolerances`, `k.fixes`, context `[stored size, stored ratio, zone]` bits; stored in `Outward`
  proof, checked by `holds(k)` (body.rs:101, 617). Proof is also bound to unchanged body (`BodyGuard`), which covers the body tolerance.
- Other stamps: `CurveMemoKey::of_kernel` (curve/curve/p5.rs:379-421: tolerances, context bits, `doing_boolean`, fixes, plus owner zone tol bits);
  analysis memo key `analysis_settings` (nurbs/b_spline_curve/p3.rs:358-366, `format!("{:?}", k.tolerances)` + fixes); recertify memo key (surface/procedural_blend/convert.rs:853);
  `cache_kernel` copy (boolean/merge/p4/kernel_adapter.rs:60). The curve memo already takes the owner zone as part of the key, i.e. the entity value.
- If tolerances were read only from the body: kernel `tolerances` and context bits would drop out of all five keys (they would be constants of the build, or
  absent); the body's tolerance (already inside the body/guard) would carry identity; ops proofs would no longer be invalidated by kernel-config change (none occurs in
  production: 1.1). Keys of ownerless curve/surface builds would need the owner/body value added (the curve memo has `owner_zone_tol_bits`).
  Today the kernel part of every key is constant in production (INFERRED: one default config, pools make `default()` kernels), so the key content is
  currently redundant for the app and only matters for tests/embedders that vary `Kernel.tolerances`.

## 5. Assessment (INFERRED from the above)
- The code already treats zone tolerance as a body/entity property for the topological work: ~1000 reads use body/entity values, and imports/booleans/transforms/heals
  all write the body value and entity values (grow-only except `matched_tolerances`, import, transform scale). The kernel value is used (i) for geometry with no owner
  (curves, surfaces: ~47 `zone_tol_3d(k,NormalSize)` fallbacks, 10 `context(None)`), (ii) as a carrier (4 temp kernels), (iii) `contact_tol_3d` by design, (iv) angle
  and gap-gain (no body slot), (v) a handful of `USE_DEFAULT`/direct reads.
- Bodies from nothing do not even use the kernel's zone: `new_brep` takes the context zone, 1e-5 by constant (p3.rs:669, tol.rs:330-334). The kernel's `zone_tol_3d` is not
  needed to create them; defaults would come from the constants. What the kernel needs is a value for owner-less geometry calls and for angle/gap gain.
- Size of change to make the body the single source for zone/xsect/approx: ~62 kernel reads + ~12 approx helpers + 10 `context(None)` + 8 `USE_DEFAULT` reads +
  4 temp-kernel sites (which would disappear) + ~7 ops/app sites + ~40 constant sites where a body/entity value was plausibly intended (2.1 list 2; list 1 should stay).
  Roughly 100-150 sites change source, in ~60 files. Hardest: curve/surface code with no body handle (the ~47 fallbacks): needs an explicit tolerance parameter or an owner
  value; this is where the C++ design (global system tolerance) differs.
- Where results could move: (1) owner-less geometry at non-default body tolerance (today 1e-5); (2) `contact_tol_3d` (documented as deliberately body-independent, WR-787:
  changing it re-opens two-hole topology differences); (3) `face_classify_uv` and 3 `USE_DEFAULT` sites; (4) healing and validity thresholds that mix `body.tolerance()` with constant ratio. For default
  1e-5 bodies, no result moves (kernel value == body value == constant). For angle: no body source exists, so keeping it as kernel/constant is the no-change path; a per-body angle would be new data.
- Risk: moderate in volume, low for default bodies, real for non-default imports (where today the mix of body and constant already gives inconsistent behaviour). Cache keys
  become simpler (section 4). Needs a count-before/after on a non-default-scale corpus, not only default bodies.
