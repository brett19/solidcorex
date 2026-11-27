# 56 - Crate granularity: where the library splits

The owner's rule of 11-26 (04 §1) reopens C1 and every crate that 01 §1 merges: "We should do our
best to split the library into as many (within reason) individual pieces that best represent the
dependencies of those particular areas. This means each of them has a smaller surface area to test
individually, and it enables users to select precisely the portions of the library that they
actually want to depend on."

This report proposes the split. It measures every candidate cut at the pin, states a rule for when
not to split, applies that rule to every candidate, and gives the resulting crate graph, the
"adopt only X" subsets, the change to `cargo xtask layers`, and how enums, dispatch and
certificates work across the new crate boundaries. Section 9 proves the graph acyclic by a script
over the complete reference graph of the pin, with every file and item given a crate. Section 11
answers the adversarial review (`reviews/review-56.md`).

- Labels:
  - MEASURED: counted at the solidcore pin (03e16c9ee7) in one of two ways.
    - Section 9, the edge tables of sections 4 and 5, and every count marked "with call edges"
      come from the complete reference graph: a SCIP index of the pin made by
      `rust-analyzer scip`, which resolves method calls, trait methods, fields and same-module
      calls as well as paths. `56-data/README.md` gives the steps to rebuild it.
    - The area and group tables come from research 49's output (`49-data/depmap.py`), rerun at
      the pin. It resolved all 38,056 path references in the 918 files of the kernel crate.
      Those tables say "49's grouping".
    - Groupings and the full assignment are in the appendices and in `56-data/`, so every count
      can be reproduced.
  - READ: read from the code at the pin.
  - JUDGED: reasoned from the above.
- A "reference" is one use of a named item (type, function, method, field, constant, trait) in
  one place, pointing at its definition. An "item edge" is a pair of items (units) with at least
  one reference from the first to the second. 49's tables count path references between files.
- Lines are non-test lines: the lines of a file before its first inline `#[cfg(test)] mod x {`.
  Files declared by `#[cfg(test)] mod x;` are test files and do not count. Inline test functions
  above that line count. The review counted some groups another way, so its totals
  differ from these by a few per cent (S4, M2).
- Paths: `k/` is `solidcore/crates/solidcore/src/`; `ops/` is `solidcore/crates/solidcore-ops/src/`.
- The proof script, its input listing and the steps to rebuild the index are in `56-data/`. The
  index itself is large and is not kept in the repository.

## 1. Summary

- **30 core crates instead of 25.**
  - `scx-geom` becomes `scx-curve` and `scx-surface`. The volume maps are in `scx-surface`, and
    so are projection and the other queries of 01's `scx-geomq`. The draft had separate
    `scx-volume` and `scx-geomq` crates; the complete reference graph merged both (4.2, 4.3).
  - `scx-nurbs` loses its surfaces to `scx-nurbs-surface`.
  - `scx-ssi` is a crate of its own, on `scx-surface`.
  - `scx-check` leaves `scx-bodyq`.
  - `scx-p21` (STEP files as plain data) leaves `scx-step`.
  - `scx-teeth` moves from the helpers into the core.
  - Mass properties stay in `scx-bodyq`, as 01 has them. The re-test after R2 is in 4.4. The cut
    now passes test 1, because mass reaches only `scx-brep`. It fails test 3, because reading a
    saved body heals it, which puts `scx-record` above `scx-bodyq`.
- **The graph is proved acyclic (section 9).** The proof runs over a complete reference graph of
  the pin. That graph comes from a SCIP index made by rust-analyzer, so it counts method calls,
  trait methods, fields and same-module calls, which 49's path data could not see. The graph
  has 48,982 items and 222,584 item-to-item edges. A script gives each of the 918 kernel files a
  crate, applies the moves this report names, and checks every edge against the declared graph.
  The result:
  - no file is unassigned;
  - no edge leaves its crate's declared dependencies;
  - the graph has no cycle.

  The script, its input listing and the steps to regenerate the index are in `56-data/`. Moves
  change the crate of 62,933 non-test lines, which is 7.1% of the ported kernel. Section 9 lists
  them by flow.
- **C1 is reversed to option (a), extended.**
  - Free curves (2-D and 3-D) live below surfaces.
  - The curve kinds that are defined on a surface (on-surface, iso, tangent field, curve in a
    volume, view projection) move into `scx-surface`, under a top enum `AnyCurve`. The draft
    called it `EdgeCurve`, a name the pin already uses twice (B3).
  - The references from curve code to surface code resolve without a trait object or a generic,
    by moves and by the design changes of 9.3 (3.3).
- **No curve-only consumer needs a surface.** The prism core, the cell grid, the sketch
  arithmetic, the `ops/` DXF code and the gear profiles use no surface item (MEASURED). Three
  functions of the prism core read bodies. They go up to `scx-bodyq` (B2, R3, section 3.4).
- **Classification reads that brep's own edits call live in `scx-brep` (R2).** The draft and the
  first revision moved them up to `scx-bodyq`. The complete graph shows that this takes brep's
  `&mut` edits along with them: `transform_in`, `orient_body`, `nest_shells` and the rest.
  Instead, the part of classification that brep reaches sinks into `scx-brep`: 1,100 items and
  25,327 lines. That part is the topology solver's types, the face and shell ray firing, the
  boxes, `LazyBrep` and `QueryTopology`. Brep's edits stay in brep, with one signature each.
  What stays in `scx-bodyq` is the measures, curve classification beyond what brep asks,
  continuity, tangent chains, `face_props` and mass properties: 46,928 lines.
- **Other re-homings that change 01's mapping:**
  - `k/eval/` goes to `scx-curve`, with named exceptions (B3);
  - `intersect/api_intersectors` splits three ways (B4, R5);
  - defeaturing and fillet removal move from `scx-blend` to `scx-local`. Their curve joins go to
    `scx-curve`, and exact-blend recognition goes to `scx-surface` (S5);
  - hidden curve removal (`blend/hcr`) moves to `scx-hlr`. The solid sweep runs it on its own
    input to shape the sweep, so `scx-make` depends on `scx-hlr` (R4);
  - the planar boolean becomes a free function in `scx-boolean`, not a method of the primitive
    builder (P1, 9.3).

  With these moves, `scx-offset` refers to `scx-blend` only to reapply fillets and to build
  circular blend sections (S1, 4.7).
- **Rejected splits:**
  - `scx-algo`;
  - mass properties vs the rest of `scx-bodyq`;
  - volume maps vs surfaces;
  - surface queries vs surface kinds;
  - primitives vs sweep in `scx-make`;
  - imprint vs merge in `scx-boolean`;
  - the inside of `scx-blend`;
  - classification vs measures in `scx-bodyq`;
  - the patch SSI vs marching;
  - the dense grid mesher vs `scx-tess`.

  The reasons are in section 4.
- **`layers.txt` changes** from a total order to the design graph, with the section 5.3 subsets
  as assertions (section 6). The module-order check stays for the three crates over 100,000
  lines.
- **What the complete graph costs the subsets (5.3).**
  - Curves now take `scx-nurbs-surface`, for the `Nurbs` record and the IGES 126 entity.
  - The body model takes `scx-ssi`.
  - Loading a saved body takes stitching and the check, because the pin's reader heals what it
    reads.

  The first two are small. The third is a behaviour question for the port (9.3).

## 2. The rule: when not to split

A boundary is drawn between two parts only if all four tests pass. Otherwise the parts stay in one
crate.

1. **Acyclic with small moves.** After the cut, references run one way. The backward ones at the
   pin must be cleared by moving items, never by adding an abstraction. Moving an item means one
   of these:
   - a small type or constant moves to the lower crate;
   - a function that needs the upper crate's types moves to the upper crate;
   - a helper moves to the crate that uses it.

   The cap is about 20 backward references, with none more than a few items wide. More than that
   means the two parts are one knot. Section 9's script checks the result for the whole graph.
   Merging the two parts is the answer when the moves do not clear the cycle.
2. **No abstraction only to break a cycle.** No trait, generic parameter or trait object exists
   only because a crate boundary was drawn. A trait that the design needs anyway is fine. One
   example is C10's scalar-generic `CurveFn`/`SurfaceFn`, which certified approximation needs.
3. **A named adopter.** Some kind of program must take one side and not the other. It must avoid
   something real: at least about 3,000 non-test lines, a heavy dependency, or a hard test
   surface. Size alone does not count. If every known consumer of one side also uses the other
   side, the split fails this test.
4. **Enums stay whole.** A closed enum lives in one crate together with every kind that can nest
   it. A kind that holds "any surface" or "any curve" lives in the crate that defines that enum.

Exceptions:

- A crate under 3,000 lines is allowed only when it is a leaf of the graph and its dependency set
  differs sharply from its neighbours'. `scx-dxf` is the case here: it depends only on the curve
  stack, while `scx-step` depends on bodies. The small operation crates that 01 already has
  (`scx-taper`, `scx-repair`, `scx-deform`) stay as 01 decided; this report does not reopen them.
- No helper is ever duplicated. A helper that both sides need moves down, never gets copied.

## 3. C1: curves, surfaces and volumes

### 3.1 What each kind holds (READ)

| Kind (pin) | Holds | Home |
|---|---|---|
| Line, Ellipse (and circle), Parabola, Hyperbola | a `BSplineCurve` base | `scx-curve` |
| Conic | 2-D only | `scx-curve` |
| Hermite | points | `scx-curve` |
| BSplineCurve | the NURBS data, `nurb: Option<Curve>` | `scx-curve` |
| Composite | segments, each a `Curve` | `scx-curve` (see 3.5) |
| OffsetCurve | a base `Curve` and a normal: a planar offset, no surface | `scx-curve` |
| ProjectedCurve | `Box<Curve>`, a projection point, vector and type: a view projection, not a curve on a surface | `scx-surface` (see 3.5) |
| CrvOnSurf | a uv `Box<Curve>` and a `Box<Surface>` | `scx-surface` |
| IsoCurve | `Box<Surface>` | `scx-surface` |
| TangentFieldCurve | a `CrvOnSurf` | `scx-surface` |
| CrvInVolume | a `Curve` and a `Volume`. The inner curve can be a `CrvOnSurf` | `scx-surface` |
| Plane, Cone, Sphere, Torus, SurfOfRevolution, SurfOfExtrusion, BSplineSurface | a B-spline or revolution base. Generators are `Option<Curve>` | `scx-surface` |
| OffsetSurface, StepSurface | `Box<Surface>` | `scx-surface` |
| CurveBounded | `Box<Surface>`, plus 3-D and uv boundary `Curve`s | `scx-surface` |
| SrfInVolume | `Box<Surface>` and a `Volume` | `scx-surface` |
| ProceduralBlend | a net and a `BlendDefinition`, whose supports are `[Box<Surface>; 2]` | `scx-surface` |
| BallPatch | a net, a centre and a radius | `scx-surface` |
| Volume (Base, Transform, BSpline, Bend, Unbend, Twist) | `next_map: Option<Box<Volume>>`. No curve and no surface in the data, but the maps' methods build surface images (4.2) | `scx-surface` |

The task brief said that `ProjectedCurve` holds a surface. It does not (READ,
`k/curve/projected_curve.rs`). It holds any `Curve`, and that includes on-surface curves. That
is why it goes up (3.5).

### 3.2 Couplings (MEASURED)

Group sizes, in non-test lines, by 49's grouping (appendix A):

| Group | Contents | Lines |
|---|---|---:|
| free curve kinds and support | including the B-spline curve wrapper | 34,172 |
| curve enum dispatch | `curve/curve` | 27,990 |
| on-surface curve kinds | `crv_on_surf`, `iso_curve`, `tangent_field`, `geodesic_offset` | 4,111 |
| curve in volume | | 1,597 |
| curve projection into a plane | | 1,774 |
| surface kinds and support | | 120,890 |
| procedural blend and sweep surfaces | | 10,279 |
| surface in volume | | 1,304 |
| volume maps | | 9,504 |

The edges between them:

| From → to | References | Back |
|---|---:|---:|
| surface → curve dispatch | 134 | 14 |
| surface → free curves | 118 | 19 |
| surface → on-surface curves | 16 | 15 |
| curve dispatch → on-surface and in-volume kinds | 6 | — |
| volume → curves (dispatch, free, in-volume) | 18 | — |
| volume → surfaces | 9 | 3 |
| volume → NURBS (knots, `CPoint`, `to_euclid` and NLib's own volume type) | 8 | — |

All the surface-side groups together refer to curve items 308 times, through 68 distinct items.
Those items become the public interface that `scx-curve` offers `scx-surface`. The biggest are
`Curve` 49, `BSplineCurve` 35, `CurveKind` 32, `Line` 17 and `TrimMode` 14. Most are public at
the pin already.

### 3.3 The 33 backward references from curves to surfaces (MEASURED list, homes READ and JUDGED)

The free curves account for 19 and the dispatch for 14. The 4 references to the moving variant
names themselves (`CrvOnSurf` and the others) and the 2 to the dispatch's type-name strings are
not counted, because they leave with the variants.

| Use | Count | Resolution |
|---|---:|---|
| `SurfParamType` (line, ellipse, dispatch p2 and p5); `OrientType` (composite) | 5 | small enums, move down to `scx-curve` |
| nesting limit: `check_geometry_nesting`, `NestedGeometry` (offset, composite) and `MAX_GEOMETRY_NESTING` (dispatch) | 5 | the constant moves down. The walk splits: the curve-only walk goes to `scx-curve`, and the walk over every family stays in `scx-surface` |
| `Bracket` (from `surface::side`) in `length_bound` | 1 | moves down to `scx-num`. It is an interval with a rounding rule |
| `conic_turn` (from `surface::implicit`) in `projected_pair_ellipse` | 1 | moves down. It takes a `BSplineCurve` |
| `Plane`, `Surface` in `has_plane_crossings` (dispatch p4) | 2 | moves up with `plane_global_curve_intersect`. Both match on the `Plane` surface kind |
| uv-trim validity on the face's surface: `assert_valid_on(.., trim_surface)` in line, ellipse and B-spline curve; `trim_image.rs` (`Surface`, `SurfaceKind`), the certified radius of a trim's image | 5 | moves up. The bound goes to `scx-surface`; the assertion goes there too, as a check over `AnyCurve` |
| `set_intersection_curve_through_point<S: SurfaceEvaluate>`, `drop_vectors`, `EvaluateFlags` | 3 | moves up to `scx-surface`. It needs only surface evaluation, not projection (S7) |
| `AnalyticSupport` (from `implicit`) in `curve/contact.rs` | 1 | `contact` moves up with `implicit` into `scx-surface` |
| curve extension constrained to a surface (dispatch p6: `Surface`, `OrientType` ×2, `DropPointOptions`, `NoObject`) | 5 | moves up: an `AnyCurve` function in `scx-surface` |
| `Surface`, `SurfaceEvaluate` in the `Curve` enum's own definition | 2 | goes with the enum split (`AnyCurve`) |
| nesting check in `projected_curve.rs` | 2 | goes with `ProjectedCurve` into `scx-surface` |
| `PLANE_TYPE` record string | 1 | moves down: the record's type strings are constants in `scx-curve` |

The rows sum to 33. The draft's table summed to 36: it counted `OrientType` in p6 twice and the
uv-trim uses as 6 (M1).

The cut passes test 1. Every move is a type, a constant or a function, and nothing becomes
generic. With call edges counted, section 9 also moves 20 dispatch and helper functions (1,438
lines) from `scx-curve` to `scx-surface`, and removes 190 references from the dispatch to the
moving kinds (9.3). Most of them are dispatch methods whose on-surface arms leave with
the variants: each one splits into a `Curve` method that stays and an `AnyCurve` function in
`scx-surface` (S6).

The volume edges in 3.2 all come from two kinds of code:

- the exact-image constructors `make_exact_b_spline_1st_out_surface` and
  `make_exact_b_spline_1st_out_curve`. These build a `Plane` or `Cone` from a bent plane, and a
  `Line` or circle from a bent line;
- the checks `is_curve_in_param_domain` and `volume/p1.rs`, which builds a `CrvInVolume`.

Both are images of curves and surfaces under a map. The first revision moved them up and kept
the rest as a `scx-volume` leaf. The complete graph shows that the maps' own dispatch calls the
image constructors, so the maps and the surfaces form a knot. The volume maps are a module of
`scx-surface` (4.2).

### 3.4 Curve-only consumers (MEASURED)

Every reference from these areas into a surface, surface-curve, volume or procedural group:

| Consumer | Lines | Refs into surfaces | Notes |
|---|---:|---:|---|
| `prism/` (all) | 11,900 | 5 | all in `prism/measure.rs` (3) and `prism/sheet.rs` (2), which go to `scx-bodyq` and `scx-make` |
| `eval/` | 13,785 | 0 | num and NURBS only. Some files read bodies or records; their homes are in 3.5 |
| `cell_grid/` | 3,168 | 0 | num only |
| `develop/` | 7,241 | 0 | num only |
| `teeth/` | 11,483 | 0 | num, NURBS, the prism core, `eval`, and `fillet_section::interval::Ivl` (6), which moves to `scx-num` |
| `hlr/` | 4,450 | 0 | the kernel part. `blend/hcr` and `surface_silhouette` are separate, see 4.8 |
| `geom/paper_gaps` | 238 | 0 | |
| `ops/` DXF (`dxf.rs`, `dxf/`) and `sketch_arc.rs` | 798 | 0 | uses only `prism` (profile, arc conversion, transform) and `math` |

The prism core reads topology in three functions (B2, READ and MEASURED): `edge_run`,
`planar_edges` and `planar_edges_tolerance` in `prism/read.rs`. They read a body's edges into a
profile, so they go up to `scx-bodyq` (33 lines). The rest of `read.rs` stays in `scx-planar`.

`recognized_path` in `prism/arc_conversion.rs` is not a body read: it takes a `PlanarPath` (R3).
It reached `scx-bodyq` only through `loop_circle` and `loop_ellipse` in `prism/recognition.rs`,
which use only `Loop2`, `Seg2` and `Vec2`. So those two move down into `scx-planar`, and
`recognized_path` and its caller `path_shapes` stay there. The `ops/` DXF export calls
`path_shapes`, so `scx-dxf` needs only the curve stack.

Other items go down too: `conic_ellipse`, `EllipseScalar` and `RecognitionVector` move from
`prism/recognition.rs` into `scx-planar`, and `bernstein_roots` moves from
`query/tangent_edge.rs` into `scx-algo`.

The curve projections (`conic_projection`, `spline_projection`, `fit_projection`) are a different
case. They depend on the prism core: 15 references to `PlanarSpline`, `Seg2`, `to_local` and
`frame_is_orthonormal`. `prism/read.rs` refers back to `spline_projection` 2 times. They are one
unit with the planar core, so they go to `scx-planar`, not to 01's `scx-geomq` (MEASURED). That
crate is now merged into `scx-surface` (4.3).

### 3.5 Decisions (JUDGED)

- **`scx-curve`** holds:
  - the free kinds of 3.1 and the `Curve` enum over them, 2-D and 3-D as at the pin;
  - the dispatch over those kinds;
  - length bounds, native affine maps, stretch, the curve cache, `proximity` (curve-curve closest
    points), `plane_of`, arcs and paths;
  - curve-curve intersection, because it needs only curves. From `intersect/api_intersectors`,
    `intersect_curves`, `curve_crossings` and `CurveIntersections` come here (B4);
  - curve joins: `join_with` and `join_with_gap` from `blend/fillet_corner/p3.rs` (S5);
  - the curve records: `curve/from_parameters`, `arc_of` and `step_edge`, whole (the B3 ruling);
  - the quadric helpers that `from_parameters` needs, from `surface/implicit.rs`: `conic_turn`,
    `QuadricEquation`, `curve_on_quadrics_with_constants` and `spine_on_offsets`. They use only
    `BSplineCurve` and `gamma`;
  - `drop_limit` from `topology/brep_constructor.rs`, which `step_edge`'s `closed_parameter`
    calls. It drops a point onto a `Curve` and snaps it to an end;
  - `k/eval/`, the evaluation helpers, except as listed in the next item;
  - C10's `Procedural` mechanism, which both `OffsetCurve` here and the procedural surface kinds
    above use (S7).

  Queries over one family live with that family, as 2-D bounded intersections already live in
  `scx-planar`.
- **What leaves `eval/` (B3, section 9).** Nothing leaves the curve records. The first revision
  sent `on_conic` up to `scx-surface` and `closed_parameter` to `scx-brep`; the B3 ruling showed
  that their callers stay, and moved what they need down instead.

  | Item | Home | Why (READ) |
  |---|---|---|
  | `eval/constraint` | `scx-sketch` | sketch residuals |
  | `eval/outline` | `scx-hlr` | silhouette outlines |
  | `eval/pcurve`, and `iso_within` and `on_pcurve` in `eval/{collapse,surface,agreement}` | `scx-brep` | they read a face's pcurves |
  | `eval/hole_stacks` | `scx-bodyq` | body reads |
  | `eval/label` | `scx-record`, except `curve_interval`, which goes to `scx-brep` | record labels |
  | `eval/query` | `scx-planar` | 2-D region queries |
  | `eval/spline`: the `Nurbs` record and its accessors | `scx-nurbs-surface` | it holds an NLib surface |
  | `eval/spline`: `nurbs_closed` | `scx-surface` | it tests a kernel surface |

  The `EdgeCurve` structs at `k/curve/from_parameters.rs:74` and `ops/measure.rs:471` keep their
  names. The new top enum is `AnyCurve` instead, so nothing clashes.
- **`scx-surface`** holds:
  - every surface kind, the `Surface` enum and its dispatch;
  - `SurfaceCurve` with `OnSurface`, `Iso`, `TangentField` and `GeodesicOffset` (if that is a
    kind);
  - the top enum `AnyCurve`, with `Free(Curve)`, `OnSurface(SurfaceCurve)`, `InVolume` and
    `Projected`;
  - the volume maps, the images of curves and surfaces under them, `implicit` (less the quadric
    helpers above) and `contact`, and the kernel-to-NLib conversion `as_b_spline`;
  - the surface queries of 01's `scx-geomq`: point inversion, projection with the tracer,
    curve-surface intersection, recognition, plane fitting and the certified gap bounds (4.3);
  - point refinement on two surfaces (`set_intersection_curve_through_point`), and with it the
    evaluator of C10's intersection-curve kind. Its data and its evaluator both live here (S7).
    It needs only surface evaluation, not projection.
- **`ProjectedCurve` goes to `scx-surface`.** It projects whatever curve it is given, and the
  hidden-line code projects edge curves, which can lie on surfaces. Keeping it in `scx-curve`
  would need `Projected<C>`, a generic only to break a cycle, which the rule forbids.
- **`Composite` and `OffsetCurve` stay free.** At the pin they hold `Curve`, which can be on a
  surface (READ). Whether any caller builds a composite of on-surface segments is not measured.
  If one does, `AnyCurve` gains a `Composite(Vec<AnyCurve>)` arm under rule 4, and the free
  composite stays for 2-D work.
- **Generators and boundaries.** Revolution and extrusion generators, and the 3-D boundaries of
  CurveBounded, hold `AnyCurve`. uv boundaries and the uv curve of an on-surface curve hold the
  free 2-D `Curve`.
- **Every `AnyCurve` is a curve.** Algorithms that take any curve, such as approximation, length
  and faceting, take `impl CurveFn`. That is the scalar-generic evaluation trait that C10 already
  defines for certified approximation, and both enums implement it. This passes test 2 because
  the trait exists anyway.
- **`CurveEvaluate::as_crv_on_surf` goes** (`curve/curve/p2.rs:335`). It names an on-surface type
  from a trait that stays in `scx-curve`. Callers match `AnyCurve::OnSurface` instead (S6).

## 4. The other merged crates

Each candidate below is given as: the references across the cut, whether the cut is acyclic, who
adopts one side, the costs, and a verdict.

### 4.1 `scx-nurbs`: curves, surfaces, fitting, certificates

NLib groups (MEASURED, non-test lines):

| Group | Contents | Lines |
|---|---|---:|
| base | knots, basis, Bézier, arrays, NLib geometry and maths, IGES | 8,880 |
| curves | `crv_*`, `nurbs_crv`, `spiral` | 14,219 |
| curve fitting | `crv_fit` | 4,292 |
| surfaces | `srf_*`, `nurbs_srf`, match, seam | 17,873 |
| surface fitting | | 749 |
| Gordon and n-sided patches | | 1,079 |
| certified enclosures and deviations | `certificate`, `crv_`, `srf_` and `circle_deviation`, `geom/nurbs_enclosure` | 3,523 |

The edges:

| Edge | References | Back | Notes |
|---|---:|---:|---|
| surfaces → base | 186 | 6 | |
| surfaces → curves | 59 | 10 | |
| surfaces → curve fitting | 25 | 0 | |
| curves → surfaces | 10 | — | every one is `WEIGHT_MAX`, `WEIGHT_MIN`, `WEIGHT_TOL` or the `Surface` name in `crv_basic` |
| base → surfaces | 6 | — | Bézier surface helpers and IGES 128 |
| base → curves | 3 | — | |
| curves → surface fitting | 1 | — | |
| kernel curve code → NLib surfaces | **0** | — | |

The verdicts:

- **Curves vs surfaces: split, adopted.**
  - The cut becomes acyclic once the weight constants move to base, and the Bézier surface
    helpers, IGES 126/128 (except `clamp_curve`) and `func_interp` move up. With call edges
    counted, section 9 raises 2 more functions (117 lines) to `scx-nurbs-surface` and sinks 22
    curve helpers (382 lines) from surface files into `scx-nurbs`.
  - `scx-nurbs` needs only `scx-num`. Neither it nor `scx-algo` uses the other (MEASURED).
  - Adopter: a NURBS curve library (num, nurbs). It avoids about 20,000 lines of NURBS surfaces.
    The first revision named the whole curve stack. The complete graph shows that `scx-curve`
    needs `scx-nurbs-surface`, for the `Nurbs` record of `eval/` and IGES 126 (5.3), so the curve
    stack no longer avoids it.
  - Cost: base is public to both crates (`CPoint`, `Side`, `KnotVector`).
  - Names: `scx-nurbs` (base, curves, curve fitting, curve enclosures) and `scx-nurbs-surface`.
- **Fitting: not split.** Surface fitting is 749 lines. Curve fitting and curves refer to each
  other both ways (19 and 7), and the curve fitting checks itself with the certified curve
  deviation. That fails tests 1 and 3.
- **Certificates: re-homed, not a crate.**
  - The pure-NLib enclosures and deviations go to the crate of their family:
    - `curve_derivs_over`, `ders_basis`, `span`, `crv_deviation` and `circle_deviation` go to
      `scx-nurbs`;
    - `srf_deviation` and `nurbs_enclosure` go to `scx-nurbs-surface`. Six functions of
      `nurbs_enclosure` (248 lines) take kernel surfaces and rise to `scx-surface`.
  - `as_b_spline` takes a kernel `Surface`, so it goes to `scx-surface`. It is used 18 times from
    surface code.
  - `nurbs/certificate.rs` certifies a railed loft's guided edge and uses kernel `Curve` and
    `Surface`, so it goes to `scx-make` with the loft.
  - `blend/fillet_section` is not NLib. Its pieces are placed file by file in 4.7 (S4).
  - This finishes what C3 started with `scx-geom::procedural`, which no longer exists.
- **Gordon and n-sided: split by level.** The NLib construction stays in `scx-nurbs-surface`. The
  wrappers that take kernel curves (`gordon.rs`, `n_sided.rs`) go to `scx-surface`.

### 4.2 Volume maps: merged into `scx-surface`

The first revision kept `scx-volume` as a leaf over `scx-nurbs`. The complete graph refutes that
cut (MEASURED, section 9).

| Edge | Item edges | References | Largest parts |
|---|---:|---:|---|
| surface → volume | 68 | 100 | `VolumeKind`, `Volume`, `EvalOptions`, `next_map`, from `srf_in_volume` and `crv_in_volume` |
| volume → surface | 38 | 51 | the bend, unbend and twist maps' `make_exact_b_spline_1st_out_surface`, and `Volume::with_maps`, which name `Surface`, `Plane` and `CrvInVolume` |

The exact-image constructors are methods of the maps themselves, and the map base walks
surfaces. Moving them up as free functions does not remove the edge, because the maps' own
dispatch calls them. That leaves a two-way knot, which fails test 1.

The adopter was also weak: a lattice user who maps points without surface kinds. It would save
about 6,000 lines.

**Merged.** The volume kinds are a module of `scx-surface`. Images under volumes live beside
them.

### 4.3 Surface queries and `scx-ssi`

Groups at the pin (MEASURED, 49's grouping):

| Group | Contents | Lines |
|---|---|---:|
| marching SSI | `surface_intersector`, `adv_surface_intersector`, `surface_tracer` | 19,145 |
| projection of curves onto surfaces | `surface_drop_lib`, `surface_drop_curve` | 11,627 |
| ray tracer | | 1,782 |
| certified gap bounds | `curve_surface_gap`, `distance_bound`, `deviation`, `seam_bound` | 7,988 |
| silhouettes | | 1,356 |
| patch SSI | the separate `ssi` and `solidcore-ssi` crates; `ssi` has no dependency at all | 11,398 + 2,187 |

Who uses the marching SSI, by module (MEASURED, 49's path references):

| User | References |
|---|---:|
| blend modules | 86 |
| surface dispatch (`Surface::intersect` → `do_intersection`) | 11 |
| projection (its tracer) | 14, of which 3 go into the intersector itself |
| silhouettes | 6 |
| error types | 4 |
| `topology/vertex/corner.rs` | 2 |
| `intersect/curve_class` | 2 |

The two uses in `topology/vertex/corner.rs` and the two in `intersect/curve_class` are both
`SurfaceIntersector::new(..)` followed by point refinement. That is a Newton step onto two
surfaces, not marching (READ, `corner.rs:477`, `curve_class/p2.rs:1228`). Point refinement on
two surfaces lives in `scx-surface` (3.5).

- **`scx-geomq` merged into `scx-surface` by the S9 trigger.** The trigger in the first revision
  was: merge if a surface kind needs projection for its own contract. The re-review ruled that
  each calling family must first move whole, and that line intersection is a query, so it does
  not trigger a merge. Both rulings are adopted.
  - The `global_line_intersect_*` family, `drop_curve_owned_by*`, `clip_exact_curves_with_context`
    and the Coons seam test moved whole. The Coons family took `seam_normal_angle` and the
    two-surface continuity measures out of `query/continuity.rs` (B.2).
  - With call edges counted, the kinds still need the old geomq code in 60 item edges (171
    references). Most of these are contract methods, not queries:
    - **point inversion:** `Surface::local_point_solve` and `global_point_solve_in` dispatch to
      `surface_point_solve`, and `step_window` calls `with_out_of_bounds_enabled`. Inverting a
      point to uv is part of every kind's contract;
    - **construction:** `create_corner_blend_in` and the `drop_curve` methods of the B-spline
      surface build their results by dropping curves (`DropSolveContext`);
    - the dispatch `Surface::drop_curve` and `drop_and_trim_curve`, which every kind answers.
  - The other direction is 1,405 references in 619 item edges.
  - The trigger is met by inversion and construction, not by line intersection.
  - **Result.** Projection, the tracer, the gap bounds, recognition (with exact blends), plane
    fitting and `surface_point_solve` are modules of `scx-surface`.
  - The adopter "a surface designer who never projects" is lost. It could not exist: building
    a corner blend or inverting a point projects.
- **SSI: split, adopted, as `scx-ssi`.**
  - Contents: the marching intersector and the patch SSI with its adapter, 18,736 lines of the
    kernel crate, plus the two patch crates.
  - The acyclic cut, with call edges counted:
    - `Surface::global_surface_intersect` and its family rise to `scx-ssi`: 27 items, 1,394
      lines;
    - the intersector's point and index types that the surface dispatch holds sink into
      `scx-surface`: `SrfIndex`, `TsectPnt` and 81 more items, 854 lines;
    - the law-curve intersection takes `&mut FilletLaw` from blend. It is removed by O1 (9.3),
      so `evaluate_law_point` stays in ssi without the blend type (R0);
    - the geometry validity checks that run intersections (`curve_bounded_surface_assert_valid`,
      `standalone_curve_assert_valid`) sit on ssi with the 18 check items they call. That is
      the whole sink check ← ssi of 9.2;
    - from `intersect/api_intersectors`, `intersect_surfaces`, `intersect_surfaces_within` and
      `refuse_points_where_a_plane_crosses` come here (B4, R5).
  - Adopters: a viewer, a STEP translator without booleans, and a surface tool that never
    intersects two surfaces. Each avoids it.
  - **`scx-brep` depends on `scx-ssi` (new).** Brep calls SSI in 5 item edges:
    - `surface_planar_section_curves` in `topology/brep/planar_section.rs`;
    - `create_parallel_projection_curves` in `topology/brep/p1.rs`;
    - `face_intersect_breps_cached` in `topology/face/p3.rs`;
    - the tangency witness of curve classification.

    These are brep's own edits and the classification they call. Moving them up would repeat
    R2's mistake. So the body data model takes `scx-ssi` (5.3).
- **Patch SSI vs marching: rejected for now.** The `ssi` crate is self-contained, so the cut would
  be acyclic. But no adopter is named who wants patch intersection on bare NURBS without kernel
  kinds, so it fails test 3. Revisit when one appears; it stays a module that depends only on
  `scx-nurbs-surface`.
- **A separate curve-query crate: rejected.**
  - Curve-curve queries need only curves, so they live in `scx-curve` (3.5).
  - The curve projections go to `scx-planar` (3.4).
  - What is left would be under the size floor.
- **The topology solver and the classification that brep calls go to `scx-brep` (B1, R2).**
  - The first revision moved 224 brep functions up to `scx-bodyq` as "classification reads".
    The re-review showed that most of them are edits: they take `&mut` Brep, self or model.
  - The complete graph confirms it. Brep's edits call ray firing, shell nesting, the boxes and
    the trim access. Lifting the edits would put `&mut Brep` writes in the read crate, against
    the goal of honest signatures.
  - So the proof sinks the part of `scx-bodyq` that brep reaches, whole, into `scx-brep`: 1,100
    items and 25,327 lines (9.2). It includes:
    - the boxes (`calculate_bounding_box`, `calculate_tight_bounding_box`) and `LazyBrep` (R2);
    - `QueryTopology`, with the prepared and requested trim access, and
      `classification_builds_tree` (R2);
    - `TopologySolver` and the types its brep callers name;
    - `ShellRayWorld`, `VertexSector` and `ClassifyProblem` (R0);
    - about 13,000 lines of `intersect/curve_class` that the face splitter and the ray firing
      use.
  - The first revision's other moves up from brep (`SelfClassification`, `QueryBrep`, the face
    intersection caches) sink back, because an edit reaches each of them. What ends in
    `scx-bodyq` from brep files is three types that no edit names, `SharedBoundary`,
    `RayFireResult` and `MergeCurvesObjects`, and one 3-line constructor (9.2).

### 4.4 `scx-bodyq`: classification, measures, mass, check

Groups (MEASURED, 49's grouping, at the pin):

| Group | Contents | Lines |
|---|---|---:|
| classification | curve and point classification, intersector API, ray tracer | 29,498 |
| measures | `query/` except gaps and tess | about 21,000 |
| mass | `mass/` | 7,329 |
| check | `topology/brep/validation`, `integrity` | 9,241 |

The verdicts:

- **Mass: re-tested after R2, not split.** The re-review asked for a rerun once the boxes and
  `QueryTopology` moved down. With the complete graph, the results are:
  - **Test 1 passes.** Mass reaches only `scx-brep`. bodyq → mass is one reference
    (`g2_within` calls `edge_length`). Brep's edits need 78 mass items (1,546 lines): the
    volume and loop-area signs, the enclosure counts and the loop-area arithmetic. They sink to
    brep with the classification they sit beside. `require_classification_trees` is a guard on
    a cache's state, as the re-review says. No brep edit calls it, so it stays with mass in
    `scx-bodyq`.
  - **Test 3 fails.** The adopter is "mass of saved bodies". Reading a saved body runs the
    healer: `make_topology_from_data` calls `heal_made` and `heal_brep` (READ,
    `topology/brep/p10.rs:546`), so `scx-record` depends on `scx-stitch` (9.2). Stitching
    depends on the check, and the check on `scx-bodyq`. So the adopter takes `scx-bodyq` anyway,
    and so does every other source of bodies: STEP import, the modelling operations and the
    tests. A body built in memory by hand with `scx-brep`'s constructors would avoid it, but no
    such adopter is named.
  - **Verdict.** Mass stays a module of `scx-bodyq`. The integrator's ODE core
    (`mass/integrator.rs`) goes to `scx-algo`.
  - **Trigger.** Split mass out, at no other cost, if reading stops healing. That is, split it if
    `scx-record` comes to depend on `scx-brep` only (9.3, the read-heal question).
- **Check: split, adopted, as `scx-check`.** It depends on `scx-bodyq`.
  - The acyclic cut:
    - `SpatialIndex` moves to `scx-algo` (the box tree);
    - the pure topology helpers move down to `scx-brep`. They are the five `derived_*` functions,
      `edge_tube_reaches`, `vertex_ball_reaches` (the S2 ruling), `GROWTH_PAST_RATIO`,
      `xsect_tol_3d_of_brep`, and the clearance proof (`tube_clears`,
      `check_vertex_clearance` and their types);
    - the clearance proof returns the vertices and edges that reach. The check turns them into
      reports (C3, 9.3);
    - brep's edits that call the reaches stay in brep: `raise_moved_edge`, `raise_joined_edge`
      and `transform_matrix_with_notifications` (the S2 ruling);
    - Brep's own validity queries (`is_valid_and_outward`, `validate_pointers` and the outward
      tests) go up to `scx-check`, so the operations that call them sit above the check (S2
      ruling);
    - the rank-deficient transform is the check's business: `Brep::transform` refuses a
      rank-deficient matrix instead of calling `degenerate_low_work_transform` (T1, 9.3).
  - **The order of stitch and check is reversed (new).** Stitching calls the check:
    `extended_side` runs `check_brep_with_keys`, `end_shell` runs `validate_pointers`, and
    `radial_sweep` runs `is_outward_solid`. So `scx-stitch` depends on `scx-check`, which depends
    on `scx-bodyq`.
  - Adopters: a viewer with pick and measure, or a CAM tool that classifies points. Both avoid
    the check, provided they do not read files (4.4, mass).
  - The check has its own test corpus of broken bodies.
- **Classification vs measures: rejected.** They call each other both ways through `query/face`
  point classification. That is a knot and fails test 1.
- **`heal/face_props` goes to `scx-bodyq`; `heal/gap` goes to `scx-brep`.**

### 4.5 `scx-make`: primitives, sweep, loft, extrude

| Edge | References | Back |
|---|---:|---:|
| primitives → sweep | 32 | 12 |
| primitives → merge | 20 | — |
| sweep → merge | 16 | — |
| loft → sweep | 7 | 1 |
| primitives → loft | 4 | — |
| extrude → planar core | 39 | 2 |

- primitives → sweep: the topology sweep's `FaceRef`, `SweepAccess` and `TopologySweep`.
- back from sweep: `PrimitiveModel` and the sweep options.

The verdicts:

- **Primitives vs sweep: rejected.** Primitives are built by the sweep's topology builder, so no
  adopter of primitives avoids the sweep. That fails test 3. The 12 backward references could
  move, so test 1 alone would pass.
- **Loft: rejected.** The loft is 7,350 lines, but it calls the sweep's stations and skin, and no
  adopter of lofts avoids sweeps.
- **Extrude stays.** The skinned-surface constructions that take only curves (Gordon, n-sided)
  are re-homed in 4.1.
- **`scx-make` depends on `scx-hlr` (R4).** The first revision removed the sweep's uses of
  `Hcr`, on the ground that the caller keeps the session (S10). The re-review showed that the
  operation itself needs them: `sweep/topology_sweep/engine.rs:1218` runs `a.hcr(..)` on the
  original body, and `access.rs:474-519` builds the `Hcr`, computes visibility and reads
  `silhouette_edges` back to shape the sweep (READ).
  - make → hlr is 40 references in 23 item edges (MEASURED, with call edges). The largest are
    `HcrOptions` 15 and `Hcr` 11.
  - The edge is declared. `scx-hlr` needs only `scx-bodyq` (layer 8), so it is acyclic.
  - The silhouette query does not move into `scx-bodyq`. It is HCR's scene visibility over a
    body and its view, not a body read, and moving it would split `hcr` across two crates.
  - The first revision's 9.3 row for `Hcr` is dropped. The store's `Body::Hcr` variant stays.

### 4.6 `scx-boolean`: imprint, merge

Groups, in lines:

| Group | Lines |
|---|---:|
| topology intersector | 14,059 |
| proto topology and cutting | 15,455 |
| merge | 12,278 |
| API | 1,131 |

The references run one way (MEASURED):

| Edge | References | Back |
|---|---:|---:|
| merge → intersector | 21 | 0 |
| intersector → proto | 11 | 0 |
| merge → proto | 6 | 0 |
| API → merge | 10 | 0 |

- **Imprint vs merge: rejected.** The cut is acyclic, but every user of the intersector outside
  boolean also merges: blend (17 to the intersector, 7 to merge), offset (merge 9) and primitives
  (merge 20). It fails test 3. Revisit if a split-face or imprint-only consumer appears.
- Brep's references to the intersector and to merge are brep-level operations (READ). They move
  out with the other operations, as 01 §1.3 already says for `topology/brep`. With call edges
  counted, section 9 moves 13 functions (550 lines) to `scx-boolean` and 32 (1,267 lines) to
  `scx-stitch`, with smaller flows to `scx-check` (234) and `scx-offset` (249,
  `local_operation`).
- **The planar boolean moves into `scx-boolean` (P1).** At the pin it is a method of the
  primitive builder (`boolean_2d`, `boolean_2d_keyed`, `Boolean2d`), and merge's 2-D path builds a
  `PrimitiveCreation` to call it. That made boolean → make a cycle. It becomes a free function
  over two sheets: 61 items (1,990 lines) move from make to boolean with their helpers, and the 3
  references to the builder go (9.3).
- One function of boolean rises to `scx-make`: `piecewise_merge` (37 lines). boolean → stitch is
  15 references in 11 item edges. `owning_topology`'s
  `TYPE` and `CopyBrepMap` move down to `scx-brep`.

### 4.7 `scx-blend`: the inside, and what leaves it

Blend is 191,056 lines with `fillet_section` and 174,700 without it, by the review's count (S4).
The draft gave 191,020 as the figure without it, which was wrong. After the moves below and
section 9's, the crate is 150,425 lines. Of the difference, 8,650 lines are blend-engine helpers
that the procedural blend surface and its certified section call. They sink whole into
`scx-surface` (9.2). Its large modules form a knot (49's grouping):

| Edge | References | Back |
|---|---:|---:|
| `fillet_executive` → `blend_corner` | 120 | 3 |
| `fillet_executive` → `fillet_geom` | 110 | 5 |
| `fillet_corner` → `fillet_geom` | 102 | 10 |
| `fillet_solver` → `fillet_geom` | 98 | 20 |
| `fillet_corner` ↔ `blend_corner` | 41 | 37 |

**An internal split is rejected** because it fails test 1. What leaves:

| Part | Evidence (MEASURED) | New home |
|---|---|---|
| `fillet_section` | used by surfaces 68, bodyq 54, offset 41 and projection 39 times (49's areas) | file by file, below |
| blend-surface geometry items: `Side` and `Crv` in `blend_corner/mixed/algebra.rs`, `Face` in `blend_setback.rs`, `ClassInputs` in `fillet_exact.rs` | used by the procedural blend surface 16, 11 and 9 times | `scx-surface`, the procedural blend's definition |
| exact-blend recognition: `QuadricKind`, `BlendQuadric`, `BlendSpine`, `ExactBlend`, `classify_blend_spine`, `circle_spine_station` | used by defeature and by recognition | `scx-surface` (recognition), 466 lines |
| `fillet_removal`, `identify_fillet`, `feature_executive` (defeaturing), 8,995 lines | used only by offset (9) and by `fillet_executive` (2) | `scx-local`. `scx-blend` depends on `scx-local` |
| `join_with`, `join_with_gap` (`fillet_corner/p3.rs`) | pure curve joins that match on `CurveKind`; defeature uses them 3 times (S5) | `scx-curve` |
| `fillet_executive::create_face_from_surface` | defeature uses it once (S5) | `scx-make`, with face construction |
| `hcr` (hidden curve removal, READ), 2,089 lines | used by sweep 4, primitives 2, brep 2 | `scx-hlr`. `scx-make` keeps its uses and depends on `scx-hlr` (4.5, R4) |

`fillet_section`, file by file (S4, non-test lines):

| File | Lines | Home | Why (READ) |
|---|---:|---|---|
| `interval`, `series`, `tay`, `taylor_model` | 2,049 | `scx-num` | interval arithmetic and Taylor models (research 53) |
| `certificate` | 4,594 | `scx-surface` | blend-section solvers (`GnStation`, `GnSystem`, `gn_solve_station_with`) over kernel surfaces. `NurbsCurve` and the `derivs_over` family (198 lines), which need only curves, go down to `scx-curve` |
| `station`, `setback_station` | 2,417 | `scx-surface` | the stations of the procedural blend surface. Their small matrix and basis helpers (173 lines) sink to `scx-nurbs-surface` |
| `section` | 2,450 | `scx-surface` | the section that the procedural blend evaluates. `GnLawRound` stays with it, because the section evaluates it (B.3) |
| `definition`, `definition_box`, `offset_box`, `form`, `feet_tm`, `recertify`, `smooth_interior`, `smooth_interior_plan` | 4,533 | `scx-surface` | the blend definition and its certified boxes |
| `continuation` | 185 | `scx-surface` | |
| `fillet_section.rs` (the module root) | 128 | `scx-surface` | |

After these moves, with call edges counted, `scx-offset` refers to `scx-blend` 25 times outside
`offset/construction` (MEASURED):

- 19 in `offset/offset_faces.rs`, which reapply fillets: `FilletSession` and its calls,
  `FilletSolverType` 5, `ErrorInfo` 2, `FilletErrorType` 2, `FilletSurfaceGeneratorType` 2 and
  `is_radius_supported`;
- 6 in `offset_geometry_creation.rs`: `CircularCrossSectionFsg` and
  `create_surface_from_curves`.

The rest are five helpers of `offset/construction/sheets.rs` (49 lines) that the blend engine
calls; they sink into `scx-blend`. At the pin the offset area refers to blend 56 times: 41 to the
certified section, 9 to defeature and 6 from `offset_faces.rs` by 49's file count (S1). The
draft's "62 → 6" was stale. C3's dependency stays, but on reapplying fillets and one section
kind.

`scx-local` (defeature, fillet removal) is then adopted alone by a simplification tool for meshing
or analysis that removes holes and rounds. Such a tool avoids the roughly 150,000-line blend engine
(JUDGED adopter).

### 4.8 `scx-tess`, `scx-hlr`, `scx-step`, helpers

- **The dense grid mesher: rejected as a crate.** `nurbs-grid-mesh` is 1,671 lines with no
  dependency. It is under the floor, and its adopter, a viewer, already takes `scx-tess`. It
  stays a module of `scx-tess`.
- **STEP: split, adopted, as `scx-p21`.**
  - `solidcore-record/src/step` is the Part 21 exchange structure, units, Part 42 curves and
    surfaces as data, product structure and the protocol: 7,433 lines. It computes no geometry,
    which its crate's lints enforce (READ). With the Part 21 writer, the crate is about 9,000
    lines.
  - `scx-p21` depends on `scx-num` only.
  - Adopters: product-structure and PDM tools, STEP validators and rewriters, and a fuzzing
    harness for the parser. None of them needs geometry.
  - `scx-step` keeps the mapping to bodies and depends on `scx-p21`, `scx-stitch` and `scx-check`.
  - STEP import does not pull `scx-boolean`. It does pull `scx-ssi`, through `scx-brep` (4.3).
- **DXF.** `scx-dxf` depends on `scx-planar` and the curve stack only (3.4). It is a leaf that
  qualifies for the size exception.
- **Teeth: re-layered.**
  - The kernel's `teeth/` is gear and spline tooth profiles. It uses num, NURBS, the prism core,
    `eval` and the certified section's `Ivl` (MEASURED). `Ivl` moves to `scx-num` (4.7), so the
    draft's "only num, NURBS, prism core and eval" now holds.
  - `scx-teeth` drops to the curve stack, on `scx-planar`, at layer 5.
  - The body building in `ops/shaft_spline.rs` (864 lines) goes to `scx-features`. That crate
    already composes operation calls.
  - Adopter: gear profiles for CAM or drawings without a modeller.
- **`scx-hlr`.** It takes `hcr`, the silhouettes, `eval/outline` and `topology/api_projection`.
  Its only direct dependency is `scx-bodyq` (MEASURED). `plane_projection` from
  `api_projection` moves down to `scx-surface`, and `intersect/views` goes to `scx-make`.
  `scx-make` depends on it (4.5).
- **`scx-algo`: kept whole.**
  - Every part (polynomials, solvers, Krawczyk, box tree, executor) is used from `scx-curve` up.
    No adopter wants trees without solvers.
  - It fails test 3.
- **`scx-record`: above `scx-stitch` (new).** It writes every kind exactly, so it sits above
  `scx-brep`. With call edges counted, it also sits above `scx-stitch`, because the pin's reader
  heals what it reads: `make_topology_from_data` calls `heal_made` and `heal_brep` (READ,
  `topology/brep/p10.rs:546`). Section 9 moves the encoding out of the kind crates into it: 152
  functions, 5,531 lines, from brep, surface, curve and num (7.3). Splitting an exact read from
  the heal is a behaviour question for the port (9.3).

## 5. The crate graph

### 5.1 Core crates

> **Superseded in part.** Re-review 2 of review-56 (ruled 11-27, 04 §3) changes this graph. The
> table below is the first measured graph; appendix C has the graph as ruled, measured on the
> same complete call graph of the pin, with the edges it still needs.

The layer is the longest path from `scx-num`. "Deps" lists direct dependencies only. "Kernel
lines" is the non-test lines of the pin's kernel crate that section 9 assigns to the crate. A
crate that also takes a whole sibling crate from 01 §1.3 (`solidcore-record`, the tessellator,
the patch SSI and the others) is larger by that crate; "—" means the crate takes no kernel code.

| Layer | Crate | Contents | Deps | Kernel lines |
|---:|---|---|---|---:|
| 0 | `scx-num` | as 01, plus the certified section's intervals, series and Taylor models, `Bracket`, pseudo-boxes, placements and `mult_4x4` | std | 16,984 |
| 1 | `scx-algo` | as 01, plus the spatial index from validation, `bernstein_roots`, the Bernstein construction from offset, the integrator's ODE core | num | 17,515 |
| 1 | `scx-nurbs` | knots, basis, Bézier, NURBS curves, curve fitting, curve enclosures and deviations | num | 28,275 |
| 1 | `scx-p21` | STEP Part 21 read and write, Part 42 entities as data, units, product structure | num | — |
| 2 | `scx-nurbs-surface` | NURBS surfaces, surface fitting, match and seam, Gordon and n-sided (NLib level), surface enclosures and deviations, IGES 126/128, the `Nurbs` record of `eval/spline` | nurbs, algo | 19,755 |
| 3 | `scx-curve` | free curve kinds (2-D and 3-D), `Curve`, its dispatch, curve-curve queries and joins, length bounds, curve records, `eval/`, `Procedural`, and the quadric helpers and `drop_limit` of B3 | nurbs-surface | 69,068 |
| 4 | `scx-planar` | 2-D regions, profiles, arc conversion and loop recognition, fill, cell grid, paper gaps, projection of curves into a plane, 2-D region queries | curve | 15,830 |
| 4 | `scx-surface` | every surface kind, `SurfaceCurve`, `AnyCurve`, view projection; the volume maps and images under them; procedural blend and sweep definitions with the certified section; `implicit`, `contact`, `as_b_spline`, point refinement on two surfaces, kernel-level Gordon and n-sided; point inversion, projection with the tracer, curve-surface intersection, recognition (including exact blends), plane fitting, distance and certified gap bounds | curve, nurbs-surface | 195,148 |
| 5 | `scx-sketch` | constraint residuals and solver, sketch arcs | planar | 363 |
| 5 | `scx-dxf` | DXF read and write of 2-D paths | planar | — |
| 5 | `scx-ssi` | surface-surface intersection: marching and patch; the geometry validity checks that intersect | surface | 18,736 |
| 5 | `scx-teeth` | gear and spline tooth profiles | planar | 11,826 |
| 6 | `scx-brep` | as 01, plus `heal/gap`, the derived adjacency, tolerance-growth and clearance helpers, `owning_topology`'s type tag and `CopyBrepMap`; the topology solver, the boxes, `LazyBrep`, `QueryTopology`, and the classification and ray firing that brep's edits call | ssi | 117,878 |
| 7 | `scx-bodyq` | classification beyond what brep's edits call, distance, contact, pick, continuity, tangent chains, face measures, mass properties, `face_props` | brep, planar | 46,928 |
| 7 | `scx-tess` | as 01, with the grid mesher | brep | 1,158 |
| 8 | `scx-check` | the full validity check, integrity, vertex-corner checks, Brep's validity queries | bodyq | 8,044 |
| 8 | `scx-hlr` | as 01, plus `hcr`, silhouettes, outlines | bodyq | 10,948 |
| 9 | `scx-stitch` | as 01, plus brep's stitch entry points | check | 13,201 |
| 10 | `scx-boolean` | as 01, plus the planar boolean (P1) | bodyq, stitch | 45,662 |
| 10 | `scx-deform` | as 01 | check, stitch | — |
| 10 | `scx-record` | as 01, plus the encoding of every kind | stitch | 8,155 |
| 10 | `scx-step` | the mapping between STEP and bodies, assembly placement | p21, stitch, check | — |
| 11 | `scx-make` | as 01, plus the loft certificate and face construction from a surface | boolean, check, hlr, planar, stitch | 33,652 |
| 11 | `scx-repair` | as 01 | boolean, check, stitch | 1,240 |
| 12 | `scx-local` | as 01, plus defeature, fillet removal, fillet identification | make | 8,997 |
| 12 | `scx-mesh` | as 01 | make, tess | 5,565 |
| 12 | `scx-sheetmetal` | as 01 | make | 7,358 |
| 12 | `scx-taper` | as 01 | make | 773 |
| 13 | `scx-blend` | fillets, chamfers, vertex blends, law-curve intersection | local, repair | 150,425 |
| 14 | `scx-offset` | as 01, plus `local_operation` | blend, taper | 36,702 |

- That is 30 core crates. 01 has 25.
- The direct dependencies of the operation crates are 01's. Section 9 shows that their kernel
  code uses fewer:
  - `scx-make` uses `scx-boolean` and `scx-hlr` directly;
  - `scx-taper` uses only `scx-brep`;
  - `scx-repair` uses only `scx-stitch`;
  - `scx-deform` takes no kernel code.

  Their `ops/` code may use the rest. The port narrows each line to what Cargo.toml needs (6,
  rule 4).
- Rows marked "as 01" keep 01's contents with only the changes named.
- `scx-p21`, `scx-dxf` and `scx-step` take no kernel code. Their dependencies are READ from their
  sibling crates and are not part of section 9's proof.

### 5.2 Helper crates

| Crate | Deps | Change from 01 | Kernel lines |
|---|---|---|---:|
| `scx-fates` | brep, record | none | — |
| `scx-features` | ops crates, fates, teeth | takes the gear and spline body building | — |
| `scx-frames` | make | its kernel part, `prism/{member,gusset}`, uses only planar | 948 |
| `scx-weld` | blend | none | 903 |
| `scx-cadir` | brep, stitch | none | — |
| `scx-testkit` | brep, bodyq, check, record | adds check | — |
| `scx-difftest` | everything ported | none | — |

`scx-teeth` moves from the helpers into the core, at layer 5.

### 5.3 Adopt only X

Each row is the closure of its top crates in 5.1.

| A user who wants | Takes exactly |
|---|---|
| a NURBS curve library | num, nurbs |
| NURBS curves and surfaces | num, algo, nurbs, nurbs-surface |
| 2-D and 3-D curves with curve-curve queries | num, algo, nurbs, nurbs-surface, curve |
| a sketcher | the row above, plus planar and sketch |
| DXF exchange of 2-D paths | the curves, plus planar and dxf |
| gear and spline profiles | the curves, plus planar and teeth |
| STEP file structure, product tree and metadata | num, p21 |
| curves and surfaces: evaluated, built, inverted and projected | the curves, plus surface |
| surface-surface intersection | the row above, plus ssi |
| a body data model, built in memory | the row above, plus brep |
| pick, measure, classification and mass of a body built in memory | the body data model, plus planar and bodyq |
| loading saved bodies | the row above, plus check, stitch and record, because the pin's reader heals (9.3) |
| a viewer (load and facet) | loading, plus tess |
| mass properties of saved bodies | loading |
| a STEP translator (read, sew, check) | the body data model, plus p21, planar, bodyq, check, stitch and step. No boolean |
| drawings (hidden lines) | the viewer, plus hlr. No boolean |
| a modeller | the translator, plus record, boolean, hlr, make, and the layer-12 to layer-14 crates it needs |
| defeaturing imported parts for analysis | the modeller's base up to make, plus local. No blend |

What the complete graph changes against the first revision:

- **Curves take `scx-nurbs-surface`.** The reason is 101 references in 57 item edges:
  - `eval/` evaluates NLib surfaces as well as curves, through the `Nurbs` record (`collapse`,
    `uv`, `spline`);
  - the curve helpers of the certified section use the enclosure `IZ`;
  - `BSplineCurve::create_canonical` reads IGES 126.

  A trial that moved `eval/spline`, `eval/collapse`, `eval/uv`, `IZ` and IGES 126 down did not
  clear the edge. The rest of `eval/` (`geometry`, `vec`, `agreement`) calls them and followed
  them down. So `eval/`'s curve and surface halves are one unit for now.

  The NURBS curve library (num, nurbs) still avoids NURBS surfaces, so the 4.1 split keeps its
  adopter.
- **The body data model takes `scx-ssi`** (4.3).
- **Loading takes `scx-bodyq`, `scx-check` and `scx-stitch`.** Reading heals (4.4, 9.3). If the
  port splits an exact read from the heal, loading drops back to the body data model plus record.
  Mass can then split out of `scx-bodyq` (4.4).

## 6. `cargo xtask layers`

Today `layers.txt` is a total order: a crate may depend only on crates listed above it. A total
order of 37 crates (30 core, 7 helpers) allows edges the design forbids. For example, `scx-planar`
could reach `scx-surface` just by being listed below it. The proposal:

```
# The design graph: each library and its direct dependencies (5.1).
# Checked by `cargo xtask layers`.
[library]
scx-num =
scx-algo = scx-num
scx-nurbs = scx-num
scx-p21 = scx-num
scx-nurbs-surface = scx-nurbs scx-algo
scx-curve = scx-nurbs-surface
scx-planar = scx-curve
scx-surface = scx-curve scx-nurbs-surface
scx-ssi = scx-surface
scx-brep = scx-ssi
...
scx-stitch = scx-check
scx-record = scx-stitch
...
# Each subset of 5.3: its top crates, and the exact closure they must have.
[subset]
curve-stack = scx-sketch scx-dxf scx-teeth : scx-num scx-algo scx-nurbs scx-nurbs-surface scx-curve scx-planar scx-sketch scx-dxf scx-teeth
step-translator = scx-step : scx-num scx-p21 scx-algo scx-nurbs ... scx-check scx-stitch scx-step
...
[testkit]
scx-testkit = scx-brep scx-bodyq scx-check scx-record
[difftest]
scx-difftest
[tool]
xtask
[external]
```

The check:

1. **Allowed edges.** The `[library]` lines are the design graph, written by hand. The check
   computes each crate's closure from those lines. Every actual dependency (normal or build) in a
   crate's Cargo.toml must lie in its closure. That lets direct use of `scx-num` skip a detour.
   This is not circular: the closure comes from the file, and the use comes from Cargo.toml.
2. **No cycles.** The design graph has no cycle. Only normal and build edges count. The check
   prints each crate's layer (longest path), and 01's table quotes those layers.
3. **Subsets hold.** For each `[subset]` line, the closure of its top crates must equal the listed
   set exactly. A new edge such as planar → surface keeps the graph acyclic but breaks the
   curve-stack subset, so it fails the check, and the PR has to change 5.3 and the subset line in
   the open.
4. **No unused declarations.** A `[library]` edge with no matching dependency in the crate's
   Cargo.toml is a warning. The design graph then stays an honest picture.
5. **Test-only crates.** `scx-testkit` gets a new `Testkit` role (the check has only Library,
   Difftest and Tool today). Only dev edges may point at it, and they are left out of rule 2.
   Library crates use it only from integration tests (`tests/`), never from unit tests. A unit
   test that used it would compile the library twice and hit a type mismatch.
6. **Unchanged.** The `[difftest]`, `[tool]` and `[external]` rules stay as they are.
7. **Module order inside the three crates over 100,000 lines.** `scx-surface` (195,000 lines),
   `scx-blend` (150,000) and `scx-brep` (118,000) keep a module-order check, as C1(b) planned for
   the old `scx-geom`.
   - For `scx-surface` the order is: kinds and dispatch; on-surface curves and `AnyCurve`; the
     volume maps and the images under them; the procedural kinds and the certified section; then
     the queries (inversion, projection, recognition, gap bounds).
   - For `scx-brep`: the data model and its edits may call the classification module that sank
     into it (4.3), and that module may not call the edits.
   - For `scx-blend`: the knot of 4.7 is one module group. The order of the rest is set when
     the port reaches it.

   The other crates need none: their graph is the crate graph.

`check_layers` in `xtask/src/main.rs` changes from `it < me` to "is `it` in `me`'s closure",
plus the cycle, subset and role checks.

## 7. Patterns across crates

### 7.1 Closed enums that span crates

- `scx-curve` defines `pub enum Curve`, over the free kinds only.
- `scx-surface` defines `pub enum Surface`, `SurfaceCurve`, and the top enum:

  ```rust
  pub enum AnyCurve {
      Free(Curve),
      OnSurface(SurfaceCurve),
      InVolume(CurveInVolume),
      Projected(ProjectedCurve),
  }
  impl From<Curve> for AnyCurve { /* Free */ }
  ```

  Edges in `scx-brep` hold `AnyCurve`, and pcurves hold the 2-D `Curve`.
- **Orphan rule.** `scx-surface` cannot add inherent methods to `Curve`. Everything in 3.3 that
  moves up becomes a method on `AnyCurve` or `Surface`, or a free function in the upper crate.
  Section 9 moves 612 inherent methods this way. Most are record encoding (7.3), the dispatch
  split (3.3), and classification methods of brep types that stay in `scx-bodyq`. The same rule
  forbids a trait impl in a crate that defines neither the trait nor the type; section 9 leaves 21
  such impls, which the port turns into local wrappers or plain functions (8). No extension traits
  are added for this; that would be rule 2's case. Implementing a trait from `scx-surface` (such
  as a record trait) for `Curve` is allowed, because the trait is local.
- **No `#[non_exhaustive]` yet (S11).** Inside the workspace it would force a `_` arm in every
  other crate. That removes the compiler's exhaustiveness check where it matters most, for
  example in `scx-record`'s exact encoding, which has no generic fallback. 01 §7.4 already
  defers it, and the crates are `publish = false`. It is added at publication.
- **Kind coverage tests.** Each kind crate has a fieldless tag enum, `CurveTag` and
  `SurfaceTag`, with `ALL: &[Self]` and a `sample(tag) -> Curve` constructor in its test
  support. A crate that matches on kinds from outside tests that every tag's sample reaches the
  arm it expects. The draft's `KINDS: &[CurveKind]` could not be a const slice of variants that
  carry data.

### 7.2 Where `match` dispatch lives

- **Each enum's own methods live in its defining crate**, matched exhaustively there: evaluation,
  derivatives, bounds, restriction, transforms and closed-form properties. `AnyCurve`'s methods
  forward `Free(c)` to `c` and handle the rest.
- **Queries over pairs of families live in `scx-surface`** (inversion, projection, curve-surface)
  or `scx-ssi` (surface-surface), as free functions matching on pairs. Each has a closed form for
  the analytic pairs and the generic path for the rest.
- **Algorithms that accept any curve or any surface** take `impl CurveFn` or `impl SurfaceFn`:
  approximation, faceting, length and certified bounds. Dispatch is static, and both enums
  implement the trait by matching.
- **Trait objects (S8).** This design adds none. Two exist or are planned elsewhere:
  - The pin's `SurfaceSolveContext<O>` is passed as `&mut dyn` (`b_spline_surface/p1.rs:666`,
    `topology_intersector/p1.rs:4502`, `query/brep/ray/live.rs:375`). It lets a caller lend its
    caches to a surface solve. The trait is part of the design, not a crate boundary: it
    lives in `scx-surface` with the solve, and its callers sit above. It becomes a generic
    `impl SurfaceSolveContext<O>`, or goes when C2's memos replace the lent caches.
  - 01 C2's fallback is a `FaceCell` implementation in `scx-bodyq`. Under this graph it is not
    needed: the classification and boxes that brep's edits call are in `scx-brep` (4.3), so the
    cells need no upward call. C2 should drop that fallback (10). The `FaceCell` trait of 01
    §3.2, the cell read-set contract, stays in `scx-brep`.

### 7.3 Where `Recorded`, `Cert` and the certified mathematics live

- **`Cert`, `Claim`, `Method`, `Basis` and `Recorded`: `scx-num`**, as 01 §10.1 item 5 says.
  Every crate from `scx-nurbs` up attaches them. None defines its own.
- **The intervals, series and Taylor models** under the certified fillet section: `scx-num`.
- **Enclosures of NURBS**: in the crate of their family (4.1).
- **Enclosures of kernel kinds** (`as_b_spline`, the certified trim image): in `scx-surface`.
- **Exact records** of every kind: encoded in `scx-record`, which sees all kinds. Each kind crate
  exposes the plain fields a record needs, and does no encoding itself. Section 9 moves 152
  encoding functions (5,531 lines) to `scx-record` to make this true.

## 8. Costs

- **Public interfaces.** The surface-to-curve cut exposes 68 items (3.2). Most are public types
  already. Internal helpers among them, such as `TrimMode`, `BoundingBoxRequest`, `EvaluateFlags`
  and `CurveMemoKey`, must become deliberate public API, move, or disappear at port time. Each new
  cut has a similar, smaller list. The lists are the item columns of the MEASURED tables above.
- **Enum layering (S6).**
  - There are two curve enums where the pin has one. Splitting the dispatch is not one forwarding
    arm per method.
  - In `curve/curve`, each moving variant has about 45 match sites: `CrvOnSurf` 48,
    `TangentFieldCurve` 46, `ProjectedCurve` 43, `CrvInVolume` 42 (the review's count).
  - Every dispatch method with such an arm splits in two: a `Curve` method in `scx-curve` and an
    `AnyCurve` function in `scx-surface`. Section 9 moves 20 dispatch and helper functions
    (1,438 lines) up for this, and removes the 190 references from the dispatch to the moving
    kinds (9.3).
  - Callers that hold a curve of unknown origin hold `AnyCurve`.
- **Moved code.**
  - Section 9 changes the crate of 62,933 non-test lines, 7.1% of the ported kernel. The first
    revision moved 3.7%.
  - Most of the increase is the whole sink of classification into `scx-brep` (25,327 lines,
    4.3). It moves code down beside its callers, so nothing has to be rewritten for it.
  - 612 inherent methods move without their type. Each becomes a free function in its new crate,
    or a method on a type of that crate. Most are the record encoding (7.3), the dispatch split
    and the classification methods of brep types that stay in `scx-bodyq`.
- **Trait impls left in neither crate (orphan rule).**
  - 21 trait impl blocks sit in a crate that defines neither the trait nor the type. Rust
    forbids that.
  - The proof has already moved 34 others to their trait's or type's crate (9.1, step 7). These
    21 could not move: each calls code of its own crate that the other crate cannot see.
  - 17 of them are in `scx-bodyq`. They are the classification wrappers: `Mutable` and
    `MutableParams` in `curve_class`, `TopologySolver`'s `GlobalSolve`, `ShellRayWorld` for
    `BrepQuery`, `SurfaceSolveContext` and `FaceQueries` for the live ray contexts, and
    `CacheFace` for `BrepCacheFace`.
  - Two are in `scx-boolean` (the arena pair's classification impls). One is in `scx-brep`
    (`ClassObject for EntityRef`). One is in `scx-offset` (`From` for `Curve`).
  - **The fix at port time** is the usual one: a local wrapper type in the impl's crate, or a
    plain function in place of a one-use impl. Both are a few lines each, and neither adds a trait
    or a generic. The proof counts them so that none is missed. They are listed in its `bad`
    report.
- **Compile and link.**
  - More crates give more parallel front-end work and smaller incremental rebuilds below
    `scx-surface`.
  - Two costs grow:
    - generic code over `JetScalar` is instantiated in every crate that calls it;
    - inlining across crates needs `#[inline]` on small hot functions, where the pin's single
      crate got it free.
  - Neither has been measured. 01 §9.4's performance gate catches a slowdown. The first check is
    point evaluation through `AnyCurve::Free`.
- **`scx-surface`, `scx-blend` and `scx-brep` stay large.**
  - They are about 195,000, 150,000 and 118,000 lines.
  - `scx-surface` grew by the volume maps and the surface queries (4.2, 4.3), and `scx-brep` by
    the classification its edits call (4.3).
  - Rule 4 keeps the surface kinds whole. Test 1 keeps the blend engine and the queries with
    them.
  - `scx-surface`'s build time is C1's remaining reason to revisit (01 §1.5).
- **Duplicated helpers: none.** Every helper in 3.3 and 4.x moves; none is copied.

## 9. Proof that the graph is acyclic (MEASURED)

### 9.1 Method

The script is `56-data/prove.py`, and its input listing is `56-data/assignment.py` (appendix B).
`56-data/README.md` gives the steps to rebuild the reference graph and the results to expect.

The first revision ran over 49's path references. The re-review showed that those miss method
calls and calls by bare name within a module, so callers were left behind (R1). This revision
runs over a SCIP index of the pin, made by `rust-analyzer scip`. SCIP resolves every name to its
definition: method calls, trait methods, fields, enum variants and same-module calls. Nothing is
matched by name. The index is large and is not kept in the repository; the README says how to
rebuild it.

The steps:

1. **Units.** Each kernel file is cut into its items: functions (including methods), structs,
   enums, constants, statics, type aliases, traits and trait impl blocks. Test code is dropped:
   files declared by `#[cfg(test)] mod x;`, and the tail of a file from its first inline
   `#[cfg(test)] mod x {`.
2. **Edges.** Every SCIP reference inside a unit, to a definition in the kernel crate, is an edge
   from that unit to the unit that holds the definition.
3. **Crates.** Each of the 918 files gets a crate by the longest matching prefix in B.1. Then the
   item and type moves of B.2 and B.3 apply.
4. **Design changes.** The edges that a design change in this report deletes are dropped (9.3).
5. **Sinking.**
   - First the whole sinks of B.4. For each pair, the part of the upper crate that the lower
     crate's code reaches moves down whole, through its own references, provided it needs
     nothing above the lower crate.
   - Then helpers sink one by one. A unit called from a lower crate moves down to its caller,
     with the part of its own crate that it reaches. That part must need nothing above the
     caller and be at most 2,000 lines.
   - A type sinks only with its impl blocks in the same module, and the crate hubs (B.4) keep
     their types.
6. **Rising.** Until nothing changes, a function or trait impl whose edges leave its crate's
   closure moves to the lowest crate whose closure holds its crate and all its targets. A caller
   does not rise merely because its callee rose, except into the crates named in B.4, whose
   entry points take their own callers along. It took 7 rounds.
7. **Orphan rule.** A trait impl left in neither its trait's nor its type's crate moves to
   whichever of the two sees all it needs and is seen by its callers. Failing that, its type
   comes up to it. What is left is counted (8).
8. **Check.** Every edge between two crates must lie in the declared closure of the referring
   crate (the Deps column of 5.1), and the crate graph of actual edges must have no cycle. Units
   that cannot move and still leave the closure are listed.

Lines are non-test lines, as in the header.

**The two graphs compared** (`56-data/crosscheck.py`, file to file):

| Count | SCIP | 49 | Both | SCIP only | 49 only |
|---|---:|---:|---:|---:|---:|
| file-to-file edges | 24,299 | 13,054 | 13,047 | 11,252 | 7 |
| without module-path segments | 16,655 | 13,054 | 13,000 | 3,655 | 54 |

- The SCIP graph holds 49's almost entirely.
- The 11,252 edges that only SCIP has are method calls, trait methods and fields (3,655), and
  the module segments of paths (the rest), which 49 does not count as uses.
- The 7 edges that only 49 has are re-exports, which SCIP attributes to the defining file.
- By distinct uses of an item from a file, SCIP has 116,224 and 49 has 28,305.
- At the item level the graph has 48,982 units and 222,584 unit edges, which carry 545,129
  references.

### 9.2 Result

- **Unassigned files: none.** All 918 files have a crate, or are listed as not ported below.
- **References outside the declared graph: 0.**
- **Cycles: none.**
- **Trait impls:** 34 moved to their trait's or type's crate; 21 are left in neither (8).
- **Not ported** (39,980 lines; edges to and from them are ignored):

  | Files | Lines | Why |
  |---|---:|---|
  | `error.rs`, `error/` | 22,284 | each crate defines its own typed errors (01 §5) |
  | `surface/surface_cache`, `surface/trim_srf_cache`, `topology/brep_cache` | 9,105 | replaced by C2's cells and memos |
  | `geom/attribute` | 2,153 | replaced by the app's id-keyed maps (01 §2.6) |
  | `kernel.rs`, `geom/context`, `geom/check_memo` | 3,478 | the `Kernel` context, replaced by explicit arguments and memos (01 §5 contrast list, C2) |
  | `geom/a_object`, `geom/object`, `geom/content` | 2,307 | the pin's object model, replaced by typed ids |
  | `profile.rs`, `lib.rs` | 653 | profiling hooks and the crate root |

- **Whole sinks** (B.4):

  | Into ← from | Units | Lines | What |
  |---|---:|---:|---|
  | brep ← bodyq | 1,100 | 25,327 | the classification, ray firing, boxes, trim access and topology solver that brep's edits call (4.3, R2) |
  | surface ← blend | 262 | 8,650 | blend-engine helpers that the procedural blend surface and its certified section call |
  | ssi ← check | 18 | 543 | the curve validity checks that the intersecting geometry checks call (4.3) |

  Besides these, 702 units sank one by one.
- **Moved code: 62,933 non-test lines** of 892,037 ported, 7.1%, in 2,649 units. The proof logs
  2,653 moves, because four units move twice: item and type moves of appendix B 222, sinks
  2,042, rises 293, orphan-rule moves 96. The flows over
  300 lines, with what they are (READ):

  | From → to | Units | Lines | What |
  |---|---:|---:|---|
  | bodyq → brep | 1,156 | 25,842 | the whole sink above, with the boxes, `LazyBrep` and `QueryTopology` (R2), and 77 trait impls that follow their types |
  | blend → surface | 296 | 9,598 | the whole sink above, plus the blend-surface items of 4.7 |
  | brep → record | 65 | 2,221 | body record encoding (7.3) |
  | make → boolean | 61 | 1,990 | the planar boolean and its helpers (P1, 4.6) |
  | surface → record | 58 | 1,985 | surface record encoding |
  | algo → num | 150 | 1,847 | small solver and matrix helpers that num's transforms call (R1's num → algo pairs) |
  | make → surface | 69 | 1,589 | surface constructions in sweep and the loft certificate's surface pieces |
  | bodyq → surface | 68 | 1,490 | face-evaluation and surface-side helpers that the surface kinds call |
  | curve → surface | 20 | 1,438 | dispatch arms of on-surface kinds, uv-trim checks, point refinement (3.3, 3.5) |
  | surface → ssi | 27 | 1,394 | `Surface::global_surface_intersect` and its family (4.3) |
  | curve → record | 27 | 1,277 | curve record encoding |
  | brep → stitch | 32 | 1,267 | brep's stitch entry points, with `stitch_faces` and `stitch_faces_with_identity` together, and `end_shell` (4.4) |
  | hlr → surface | 36 | 1,179 | silhouette helpers that surface kinds call |
  | bodyq → planar | 54 | 867 | the 2-D loop recognition of `prism/recognition` (3.4, R3) |
  | ssi → surface | 83 | 854 | the intersector's point and index types that the surface dispatch holds |
  | surface → curve | 45 | 745 | curve helpers in surface files: the quadric helpers (B3), `conic_turn`, the curve pieces of `implicit` |
  | check → brep | 28 | 731 | the derived adjacency, reach and clearance helpers (4.4, S2) |
  | check → ssi | 19 | 553 | the whole sink above |
  | brep → boolean | 13 | 550 | brep-level copy-and-merge operations (4.6) |
  | stitch → brep | 32 | 548 | stitch helpers that brep's edits call |
  | blend → ssi | 24 | 487 | law-free intersection helpers that the intersector calls |
  | taper → surface | 36 | 470 | the draft surface definition |
  | nurbs-surface → nurbs | 22 | 382 | NLib curve helpers in surface files |

  The other flows are 277 lines or fewer each. They include brep → check (234: Brep's validity
  queries, S2), brep → offset (249: `local_operation`), planar → bodyq (33: the three body reads
  of 3.4) and brep → bodyq (3). The rising rule places each unit by its edges alone. At port time a
  function may go higher, to the crate whose job it is, but never lower.

### 9.3 References removed by design changes

| Label | References | Change |
|---|---:|---|
| S6, `AnyCurve` | 190 | the on-surface variants leave `Curve`, and their dispatch arms move with them: `CrvOnSurf` 54, `ProjectedCurve` 53, `TangentFieldCurve` 36, `CrvInVolume` 26, `IsoCurve` 21 (3.3) |
| C3, clearance reports | 36 | the clearance proof in `scx-brep` returns the vertices and edges that reach, and `scx-check` turns them into reports: `Reports` 29, `report_vertex_exclusion` 7 (4.4) |
| N1, nesting walk | 27 | curve's own wrappers (offset, composite) bound their nesting with a curve-only walk. The walk over every family stays in `scx-surface` (3.3) |
| K1, knot cuts | 18 | an on-surface or in-volume curve is cut at its knot lines or planes by curve intersection, not by a curve classification. So is the extrusion-plane line, at its surface's uv boundaries |
| O1, optional surfaces | 9 | a curve method that took `Option<&Surface>` and forwarded `None` splits into a curve method and an on-surface function. That covers `deform_to_end_point_targets` and the uv-trim checks of the line, ellipse and B-spline curve |
| P1, planar boolean | 3 | the planar boolean is a free function over two sheets in `scx-boolean`. Merge's 2-D path no longer builds a `PrimitiveCreation` to call it (4.6) |
| S6, branch test | 2 | the surface-projected branch test of the global solver goes with the on-surface kinds |
| T1, rank-deficient transform | 1 | `Brep::transform` refuses a rank-deficient matrix. The degenerate low-work transform is the check's operation (4.4) |
| **Total** | **286** | |

The first revision's row for `Hcr` is gone. The sweep keeps its hidden-curve step, and
`scx-make` depends on `scx-hlr` (4.5, R4).

**One behaviour question, not a removal.** The pin's reader heals what it reads.
`make_topology_from_data` calls `heal_made` and `heal_brep`. So `scx-record` sits above
`scx-stitch`, and loading a body takes the check and `scx-bodyq` (5.3). A trial that cut only the
reader's call to the validity check still left three edges: through `heal_made` and `heal_brep`
the reader reaches `HealData`. The proof therefore keeps the reader's healing. At port time:

- if loading is split into an exact read, which needs only `scx-brep`, and a heal that the caller
  runs, then `scx-record` drops to layer 7 on `scx-brep`;
- the viewer and mass subsets then shrink as 5.3 says;
- mass splits out of `scx-bodyq` (4.4).

### 9.4 Pin files that matter, and their homes

- Every one of the 918 kernel files has a home in appendix B.1.
- Outside the kernel crate, the sibling crates go whole, as in 01 §1.3 with the changes in
  section 10.
- The pin's `ops/` files that this report re-homes:
  - `dxf.rs` and `dxf/` go to `scx-dxf`, and `sketch_arc.rs` to `scx-sketch`;
  - `shaft_spline.rs` goes to `scx-features`;
  - the three curve projections go to `scx-planar`.
- No pin file is left unassigned.
- The DXF export calls `path_shapes`, which stays in `scx-planar` with `recognized_path` (R3). So
  `scx-dxf` needs only the curve stack.

## 10. Changes this requires in 01

- **§1.1 crate table.** Replace it with 5.1. In 1.2, take 5.2's changes. The prose "Geometry
  without topology. Layers 0-4" becomes "the crates through `scx-ssi`".
- **§1.3 inventory.** These rows change:

  | Row | Change |
  |---|---|
  | `ssi`, `solidcore-ssi` | `scx-ssi` |
  | `solidcore-record` | `step/` goes to `scx-p21` |
  | `solidcore-step`, `step-export` | `scx-step`, over `scx-p21` |
  | `geom` | `nurbs_enclosure` goes to `scx-nurbs-surface`; `pseudo_box`, `axis2_placement` go to `scx-num`; `tools` goes to `scx-brep`; `grid`, `plane_height`, `facing_trough` and `axial_section` go to `scx-surface`. `context`, `check_memo`, `attribute` and the object model are not ported (9.2) |
  | `curve` | `scx-curve`. `crv_in_volume`, `crv_on_surf`, `iso_curve`, `tangent_field`, `geodesic_offset`, `projected_curve` and `contact` go to `scx-surface`. `conic_projection`, `spline_projection` and `fit_projection` go to `scx-planar`. `from_parameters` and `step_edge` stay whole (3.5) |
  | `eval` | `scx-curve`, except `constraint` (`scx-sketch`), `outline` (`scx-hlr`), `pcurve` (`scx-brep`), `hole_stacks` (`scx-bodyq`), `label` (`scx-record`), `query` (`scx-planar`) and the `Nurbs` record of `spline` (`scx-nurbs-surface`). `scx-curve` depends on `scx-nurbs-surface` (5.3) |
  | `surface` | `scx-surface`, with the volume kinds, `srf_in_volume`, the image constructors, `implicit`, `recognise` and `surface_point_solve`. `side`, `face_*`, `mid_*` and `cylinder_wall` go to `scx-bodyq`; `extend` to `scx-local`. The quadric helpers of `implicit` go to `scx-curve` (B3). The two caches are not ported |
  | `intersect` | `surface_intersector` and `adv_surface_intersector` go to `scx-ssi`. `surface_tracer` and the drop code go to `scx-surface`. `curve_class`, `point_class` and `topology_solver` go to `scx-bodyq`, except the part that brep's edits call, which goes to `scx-brep` (4.3). `api_intersectors` splits as in 4.3. `views` goes to `scx-make` |
  | `topology` | `scx-brep`, with its reads and edits. `integrity` and `brep/validation` go to `scx-check`, except the helpers in 4.4; Brep's validity queries go up to `scx-check` (S2). `vertex/corner` goes to `scx-bodyq`. `api_projection` goes to `scx-hlr`. `api_files`, `part_file` and `database_io` go to `scx-record`. `brep_cache` is not ported |
  | `query` | `scx-bodyq`, except the boxes (`calculate_bounding_box`, `calculate_tight_bounding_box`) and `LazyBrep`, which go to `scx-brep` (R2); the gap bounds, which go to `scx-surface`; `poly` (`scx-brep`) and `tess` (`scx-tess`) |
  | `mass` | stays in `scx-bodyq`, except the 78 items that brep's edits call, which go to `scx-brep`. `mass/integrator.rs` goes to `scx-algo` |
  | `blend` | `fillet_section` as in 4.7. `fillet_removal`, `identify_fillet` and `feature_executive` go to `scx-local`. `hcr` goes to `scx-hlr`, and `scx-make` depends on `scx-hlr`, because the solid sweep runs it (4.5, R4). Exact-blend recognition goes to `scx-surface`; `join_with` to `scx-curve` |
  | `heal` | `face_props` goes to `scx-bodyq`; `gap` goes to `scx-brep`. `api_stitch_face` is deleted (11, S3) |
  | `nurbs` | split between `scx-nurbs` and `scx-nurbs-surface`. `certificate` goes to `scx-make`. `gordon` and `n_sided` go to `scx-surface` |
  | `prism` | `scx-planar`, except `extrude`, `sheet` and `placement` (`scx-make`), `recognition` and `measure` (`scx-bodyq`), `member` and `gusset` (`scx-frames`), and the three body reads of 3.4. `loop_circle` and `loop_ellipse` move from `recognition` to `scx-planar`, and `recognized_path` stays there (R3) |
  | `teeth` | `scx-teeth`, at layer 5 |

  Among the `ops/` modules, `shaft_spline` goes to `scx-features`, and `conic_projection`,
  `spline_projection` and `fit_projection` go to `scx-planar`.
- **§1.4 subsets.** Replace them with 5.3.
- **C1.** Chosen: (a), extended. `scx-curve` holds the free kinds. `scx-surface` holds the
  surface kinds, the volume maps, the surface-defined curves, `AnyCurve` and the surface
  queries.
  - Evidence: 3.2, 3.3 and section 9. Every reference from curve code to surface code resolves
    by a move or by a design change of 9.3. The dispatch split costs about 45 match sites per
    moving variant (8). The old ground for (b), "the curve enum would be split across two
    crates", is answered by making the split explicit as `AnyCurve`.
  - Replace "extended to module level" by 6's module-order check for `scx-surface`, `scx-blend`
    and `scx-brep`.
  - Change if:
    - `scx-surface`'s build time is the worst in the tier-1 loop. Look first at procedural
      kinds, behind the enum, in a lower crate. That needs a generic, so the trigger must be
      real;
    - or the port finds composites of on-surface segments, in which case `AnyCurve::Composite`
      is added.
- **C2.** Keep (b). Add:
  - `scx-check` comes out of `scx-bodyq`; mass stays in it (4.4);
  - `scx-brep` depends on `scx-ssi`, and not on `scx-bodyq` (4.3);
  - the classification, ray firing, boxes and trim access that brep's edits call live in
    `scx-brep`, beside the edits (R2). Brep's edits keep one signature each: `&Brep` for reads,
    `&mut Brep` for writes;
  - the cells take the bounding boxes, which are in `scx-brep`, and the loop and volume signs;
  - brep's validation hooks go, and the check is called by operations and tests;
  - only the `scx-bodyq` implementation fallback of `FaceCell` goes. The `FaceCell` trait of 01
    §3.2, the cell read-set contract, stays in `scx-brep` (7.2);
  - `SurfaceSolveContext` becomes generic or goes (7.2).
- **C3.** Update the table:
  - `offset` to `blend`: 56 between the areas at the pin, which is 41 to the certified section,
    9 to defeature and 6 in `offset_faces.rs`. The certified section moves to num and surface,
    and defeature to `scx-local`.
  - With call edges counted, what remains outside `offset/construction` is 25 references:
    - 19 in `offset_faces.rs`, which reapply fillets: `FilletSession` and its calls,
      `FilletSolverType`, `ErrorInfo`, `FilletErrorType`, `FilletSurfaceGeneratorType` and
      `is_radius_supported`;
    - 6 in `offset_geometry_creation.rs`: `CircularCrossSectionFsg` and
      `create_surface_from_curves`.

    Five helpers of `offset/construction/sheets.rs` (49 lines) sink into `scx-blend`.
    `scx-offset` still depends on `scx-blend`.
  - Add a row: `fillet_executive` to defeature, 2 references, so `scx-blend` depends on
    `scx-local`. Remove "`scx-geom::procedural`".
- **C10's kind set.**
  - "CrvInVolume and SrfInVolume become images under a volume kind in `scx-geom::volume`"
    becomes: images under the volume maps, all in `scx-surface`. Hermite and composite stay in
    `scx-curve`.
  - `Procedural` lives in `scx-curve`. The intersection-curve kind is an `AnyCurve` arm in
    `scx-surface`, with its evaluator (point refinement on two surfaces) beside it (3.5, S7).
- **§7.4.** Unchanged: `#[non_exhaustive]` waits for publication. Add the tag-enum coverage test
  of 7.1.
- **§10.1.**
  - Add the `layers.txt` change (section 6) to step 1.
  - Item 5 is unchanged.
  - Add: the certified section's intervals, series and Taylor models join `scx-num`'s interval
    code.
- **§10.2.**

  | Step | Change |
  |---|---|
  | 2 | becomes `scx-nurbs` evaluation and knots; `scx-nurbs-surface` evaluation and the `Nurbs` record; `scx-curve` (line, circle, ellipse, B-spline curve); `scx-surface` (plane, cylinder, cone, sphere, torus, B-spline surface, with point inversion, closed-form analytic projection and curve-surface intersection). Analytic surface-surface intersection is the first part of `scx-ssi`, which `scx-brep` needs |
  | 4 | `scx-bodyq` classification with mass properties. The part that brep's edits call is ported with `scx-brep` |
  | 5 | adds the volume maps (in `scx-surface`), `scx-ssi` (B-spline) and `scx-teeth` (after `scx-planar`) |
  | 6 | adds `scx-check` and `scx-p21`. `scx-stitch` comes after `scx-check`, and `scx-record` after `scx-stitch` |
  | 8 | `scx-local` before `scx-blend`; `scx-hlr` before `scx-make` |

## 11. Response to review

### 11.1 The re-review

| Finding | What changed |
|---|---|
| R0 | Agreed. The proof now moves types and traits as well as functions, so none is left out of reach. `ShellRayWorld` and `VertexSector` moved to `scx-bodyq` as asked, but the complete graph then sank them whole into `scx-brep`, with `TopologySolver` and `ClassifyProblem`, because brep's edits fire rays through them (4.3). `evaluate_law_point` stays on the ssi trait, without `FilletLaw`, by O1 (9.3) |
| R1 | Agreed. The proof now runs over a SCIP index from rust-analyzer: 222,584 item edges, with method calls, trait methods, fields and same-module calls. File to file it has 24,299 edges against 49's 13,054, and holds all but 7 of 49's (9.1). The script and its input listing are in `56-data/`, with the steps to rebuild the index. The rising rule no longer cascades, except into the crates whose entry points take their callers (B.4) |
| R2 | Agreed. The boxes, `LazyBrep` and `QueryTopology` with its trim access moved to `scx-brep`, and `mult_4x4` to `scx-num`. Going further than the fix asked: the rerun showed that brep's edits also call ray firing, shell nesting and the topology solver. So the part of classification that brep reaches sinks into `scx-brep` whole (1,100 items, 25,327 lines). Brep's edits stay in brep. brep → bodyq is 3 lines, one constructor and three types (4.3, 9.2) |
| R3 | Agreed. `loop_circle` and `loop_ellipse` moved to `scx-planar`. `recognized_path` stays there. `scx-dxf` needs only the curve stack (3.4, 9.4) |
| R4 | Agreed. make → hlr is declared: 40 references in 23 item edges, mostly `HcrOptions` and `Hcr`. The silhouette query does not move into `scx-bodyq`, because it is HCR's own scene visibility. The 9.3 row is dropped (4.5) |
| R5 | No change needed |
| B3 ruling | Adopted. `drop_limit`, `QuadricEquation`, `curve_on_quadrics_with_constants` and `spine_on_offsets` move down to `scx-curve`. `step_edge` and `from_parameters` stay whole in `scx-curve` (3.5) |
| S2 ruling | Adopted. `vertex_ball_reaches` moves down to `scx-brep` with `edge_tube_reaches`. `raise_moved_edge`, `raise_joined_edge` and `transform_matrix_with_notifications` stay in brep. Brep's validity queries go up to `scx-check`, and `Brep::transform` refuses a rank-deficient matrix (T1) (4.4) |
| S9 ruling | Adopted, with a result the ruling did not expect. Every family it names moved whole. Line intersection did not trigger the merge. But with call edges counted, the surface kinds still need the old geomq code in 60 item edges (171 references) for point inversion and for construction. Both are part of a kind's contract, so the author's trigger is met and `scx-geomq` merges into `scx-surface` (4.3) |
| B5 ruling | No change needed |
| Mass | Re-tested after R2, as asked. Test 1 now passes: mass reaches only `scx-brep`. Test 3 fails, for a reason the re-review did not have: the pin's reader heals (`topology/brep/p10.rs:546`), so loading any saved body takes `scx-bodyq`. Mass stays merged. The trigger is "split if reading stops healing" (4.4, 9.3) |
| §10 conflicts | Fixed. The boxes are in `scx-brep`, so C2's cells can take them. Only the `scx-bodyq` fallback of `FaceCell` goes; the trait stays. Brep's edits keep `&mut Brep` in brep. The `blend` row says that the sweep needs `hcr` (10) |

### 11.2 What the complete graph changed beyond the findings

- **Volume maps merged into `scx-surface`.** The surface kinds and the maps call each other in
  both directions: 68 item edges (100 references) and 38 (51). That knot fails test 1 (4.2).
- **The stitch and check order is reversed.** Stitching calls the check, so `scx-stitch` depends
  on `scx-check` (4.4).
- **`scx-record` sits above `scx-stitch`**, because the reader heals (9.3).
- **`scx-brep` depends on `scx-ssi`**, through 5 item edges from brep's own edits (4.3).
- **`scx-curve` depends on `scx-nurbs-surface`**, through 101 references from `eval/`, `IZ` and
  IGES 126 (5.3). A trial that moved those down did not clear the edge.
- **21 trait impls are left in neither their trait's nor their type's crate.** They are a
  port-time cost of a few lines each (8).

### 11.3 The first review

Rows marked "superseded" were changed by the re-review or by the complete graph. The text says
what holds now.

| Finding | What changed |
|---|---|
| B1 | Superseded by R2. The topology solver, and the classification that brep's edits call, are in `scx-brep`. The rest of classification is in `scx-bodyq` (4.3) |
| B2 | Agreed, corrected. Three body reads of the prism core go to `scx-bodyq`, not four: `recognized_path` stays in `scx-planar` (R3). `bernstein_roots` goes to `scx-algo`; `conic_ellipse`, `EllipseScalar`, `RecognitionVector`, `loop_circle` and `loop_ellipse` to `scx-planar` (3.4) |
| B3 | Superseded by the B3 ruling. `eval/` and the curve records go to `scx-curve`, with the exceptions in 3.5. The top enum is renamed `AnyCurve` |
| B4 | Agreed. `api_intersectors` splits: curve-curve to `scx-curve`, surface-surface to `scx-ssi`, the rest to `scx-bodyq` (4.3, confirmed by R5) |
| B5 | Agreed, and generalised. Section 9 checks every reference of all 918 files |
| S1 | Agreed. 56 = 41 + 9 + 6 at the pin. With call edges, 25 references remain outside `offset/construction`, named in 10 C3 |
| S2 | Superseded by the S2 ruling (4.4) |
| S3 | Agreed in part. `api_stitch_face` always returns an error, and is deleted. `vertex_ball_reaches` now goes to `scx-brep`, by the S2 ruling |
| S4 | Agreed. A file-by-file `fillet_section` table is in 4.7. `certificate` and `station` are blend-section solvers and go to `scx-surface`, not NLib |
| S5 | Agreed. `join_with` and `join_with_gap` go to `scx-curve`; `create_face_from_surface` to `scx-make` (4.7) |
| S6 | Agreed. Costed in match sites (8); `as_crv_on_surf` is removed (3.5) |
| S7 | Agreed. `Procedural` is in `scx-curve`. The intersection-curve evaluator needs only surface evaluation, so data and evaluator both live in `scx-surface` (3.5) |
| S8 | Agreed. Both named in 7.2 |
| S9 | Superseded by the S9 ruling: `scx-geomq` is merged (4.3) |
| S10 | Superseded by R4: make → hlr is declared (4.5) |
| S11 | Agreed. No `#[non_exhaustive]` until publication; coverage from a tag enum with sample constructors (7.1) |
| S12 | Agreed. Rule 1 restated, `[subset]` assertions, dev edges into a testkit role excluded from the cycle check, "unused" checked against Cargo.toml, and module order kept for the crates over 100,000 lines (6) |
| M1 | Fixed |
| M2 | Fixed: 134 and 118 |
| M3 | Superseded by the mass re-test. Mass does not need classification once R2 is done; it stays merged by test 3 (4.4) |
| M4 | Fixed: §10 carries the eval row, C10 and C2 |

### 11.4 Where this revision disagrees with the re-review

- **S9.** The ruling expected the merge not to trigger once the named families moved. It does
  trigger. `Surface::local_point_solve` and `global_point_solve_in` dispatch to
  `surface_point_solve`, `step_window` calls `with_out_of_bounds_enabled`, and
  `create_corner_blend_in` and the B-spline surface's `drop_curve` build by dropping curves.
  That is 60 item edges and 171 references. The other way is 619 edges and 1,405 references.
- **Mass.** The re-review expected test 3 to pass after R2. It fails, because the adopter's
  bodies come from a reader that heals (READ, `topology/brep/p10.rs:546`).
- **R0.** The fix put `ShellRayWorld` and `VertexSector` in `scx-bodyq`. With call edges, brep's
  edits fire rays through them, so they end in `scx-brep` with `TopologySolver` and
  `ClassifyProblem`.

## Appendix A: groupings used in sections 3 and 4

Prefixes are under `k/`. A trailing `*` matches by prefix, and a trailing `/` matches a
directory. The longest matching prefix wins. These groups are 49-style areas for the tables in
sections 3 and 4. The crate assignment of section 9 is appendix B. The volume and projection
groups are still separate areas here, so that the tables can show the edges between them; in the
crate graph both are modules of `scx-surface` (4.2, 4.3).

### Section 3

| Group | Files |
|---|---|
| free curves | `curve/{arc,blend,composite_curve*,conic.rs,ellipse*,hermite_curve,hyperbola,line,offset_curve,parabola,path,projected_curve,curve_cache,length_bound,native_affine,stretch,contact,proximity,plane_of}`, `nurbs/b_spline_curve*`, `curve.rs` |
| dispatch | `curve/curve` |
| on-surface curves | `curve/{crv_on_surf,iso_curve,tangent_field,geodesic_offset}` |
| in-volume curves | `curve/crv_in_volume` |
| curve projections | `curve/{conic,spline,fit}_projection` |
| curve records | `curve/{from_parameters,arc_of,step_edge}` |
| volumes | `surface/{volume,b_spline_volume*,bend_volume,twist_volume,unbend_volume}` |
| procedural surfaces | `surface/{procedural_blend,procedural_sweep}` |
| surface in volume | `surface/srf_in_volume` |
| surfaces | the rest of `surface/` and `nurbs/b_spline_surface*` |
| NURBS | `nurbs/` |
| num | `geom/`, `math.rs`, `limits.rs` |

### Section 4.1

| Group | Files |
|---|---|
| base | `nurbs/{basis_adv,basis_basic,bezier,fit_arrays,funcs_adv,funcs_basic,geom,index,knots,math,iges}` |
| curves | `nurbs/{crv_adv,crv_approx,crv_basic,crv_common,crv_geom,crv_nurb,crv_offset,crv_proportion,crv_tool,nurbs_crv,spiral}` |
| curve fitting | `nurbs/crv_fit` |
| surfaces | `nurbs/{srf_adv,srf_basic,srf_common,srf_nurb,srf_offset,srf_tool,nurbs_srf,srf_match,srf_seam}` |
| surface fitting | `nurbs/srf_fit` |
| certified | `nurbs/{certificate,crv_deviation,srf_deviation,circle_deviation}`, `geom/nurbs_enclosure` |
| Gordon | `nurbs/{gordon,n_sided}` |

### Section 4.3

| Group | Files |
|---|---|
| marching SSI | `intersect/{surface_intersector,adv_surface_intersector,surface_tracer}` |
| projection | `intersect/{surface_drop_lib,surface_drop_curve}` |
| ray tracer | `intersect/ray_tracer` |
| silhouettes | `intersect/{surface_silhouette,views}` |
| curve classification | `intersect/curve_class` |
| point classification | `intersect/point_class` |
| topology solver | `intersect/topology_solver` |
| gaps | `query/{curve_surface_gap,distance_bound,deviation,seam_bound}` |

### Section 4.4

| Group | Files |
|---|---|
| classification | `intersect/{curve_class,point_class,api_intersectors,ray_tracer}` |
| mass | `mass/` |
| check | `topology/brep/validation`, `topology/integrity` |
| measures | `query/`, without the gaps and `query/tess` |
| heal helpers | `heal/{face_props,gap}` |

### Sections 4.5 and 4.6

| Group | Files |
|---|---|
| primitives | `primitive/` |
| sweep | `sweep/` |
| loft | `sweep/{loft_rails,loft_sections,section_blend,skin_ends}`, `nurbs/certificate` |
| extrude | `prism/{extrude,sheet,fill,placement,bounds}` |
| merge | `boolean/merge` |
| intersector | `boolean/topology_intersector` |
| proto | `topology/{proto_topology,brep/cutting}` |
| stitch | `heal/{stitch,heal_data,tolerance_defects}` |

### Section 4.7

Each `blend/` submodule is its own group.

## Appendix B: the crate assignment of section 9

The full listing is `56-data/assignment.py`, which the proof reads. These tables are generated
from it. Crate names drop the `scx-` prefix. `-` is not ported.

### B.1 Files

The longest matching prefix wins. The matching rules are as follows:

- a prefix ending in `/` matches a directory;
- a prefix ending in `*` matches by prefix;
- a prefix ending in `.rs` matches only that file;
- any other prefix matches the file of that name and the directory of the same name.

| Crate | Prefixes |
|---|---|
| num | `math.rs`, `limits.rs`, `geom.rs`, `geom/vector2d`, `geom/vector3d`, `geom/matrix`, `geom/transform`, `geom/extent1d`, `geom/extent2d`, `geom/extent3d`, `geom/periodic_extent1d`, `geom/interval_vector`, `geom/certified`, `geom/tol`, `geom/expansion`, `geom/step_matrix`, `geom/polar_box`, `geom/pseudo_box`, `geom/axis2_placement`, `blend/fillet_section/interval`, `blend/fillet_section/series`, `blend/fillet_section/tay`, `blend/fillet_section/taylor_model` |
| algo | `geom/polynomial`, `geom/global_solver`, `geom/hash_table`, `geom/tree`, `geom/krawczyk`, `geom/local_solve1d`, `geom/local_solve_nd`, `geom/solution_array`, `geom/geom_utility`, `geom/assert_array`, `geom/point_set`, `offset/construction/bernstein`, `topology/brep/validation/spatial`, `mass/integrator.rs` |
| nurbs | `nurbs/`, `nurbs.rs` |
| nurbs-surface | `nurbs/srf_*`, `nurbs/nurbs_srf`, `nurbs/iges`, `geom/nurbs_enclosure` |
| curve | `curve/`, `curve.rs`, `nurbs/b_spline_curve`, `nurbs/b_spline_curve_n_lib`, `eval/`, `eval.rs` |
| surface | `surface/`, `surface.rs`, `nurbs/b_spline_surface`, `nurbs/b_spline_surface_n_lib`, `nurbs/gordon`, `nurbs/n_sided`, `geom/grid`, `blend/fillet_section/`, `curve/crv_on_surf`, `curve/iso_curve`, `curve/tangent_field`, `curve/geodesic_offset`, `curve/crv_in_volume`, `curve/projected_curve`, `curve/contact`, `nurbs/b_spline_curve/trim_image`, `intersect.rs`, `intersect/surface_drop_lib`, `intersect/surface_drop_curve`, `intersect/surface_tracer`, `query/curve_surface_gap`, `query/distance_bound`, `query/deviation`, `query/seam_bound`, `surface/recognise`, `surface/surface_point_solve`, `geom/plane_height`, `geom/facing_trough`, `geom/axial_section` |
| planar | `prism/`, `prism.rs`, `cell_grid/`, `cell_grid.rs`, `geom/paper_gaps`, `curve/conic_projection`, `curve/spline_projection`, `curve/fit_projection`, `eval/query` |
| sketch | `eval/constraint` |
| teeth | `teeth/`, `teeth.rs` |
| ssi | `intersect/surface_intersector`, `intersect/adv_surface_intersector` |
| brep | `topology/`, `topology.rs`, `heal/gap`, `eval/pcurve`, `query/poly`, `geom/tools` |
| bodyq | `mass/`, `mass.rs`, `topology/vertex/corner`, `query/`, `query.rs`, `intersect/curve_class`, `intersect/point_class`, `intersect/api_intersectors`, `intersect/topology_solver`, `intersect/ray_tracer`, `surface/face_evaluation`, `surface/face_pair`, `surface/face_pair_cylinder`, `surface/face_surface`, `surface/mid_joint`, `surface/mid_sheet`, `surface/cylinder_wall`, `surface/side`, `prism/recognition`, `prism/measure`, `eval/hole_stacks`, `heal/face_props` |
| record | `topology/api_files`, `topology/part_file`, `topology/database_io`, `eval/label` |
| tess | `query/tess` |
| check | `topology/integrity`, `topology/brep/validation` |
| hlr | `hlr/`, `hlr.rs`, `intersect/surface_silhouette`, `blend/hcr`, `eval/outline`, `topology/api_projection` |
| stitch | `heal/stitch`, `heal/heal_data`, `heal.rs` |
| boolean | `boolean/`, `boolean.rs`, `topology/proto_topology`, `topology/brep/cutting`, `primitive/primitive_creation/p1/boolean.rs` |
| make | `sweep/`, `sweep.rs`, `primitive/`, `primitive.rs`, `prism/extrude`, `prism/sheet`, `prism/placement`, `nurbs/certificate`, `intersect/views` |
| repair | `heal/tolerance_defects` |
| local | `blend/fillet_removal`, `blend/identify_fillet`, `blend/feature_executive` |
| taper | `offset/draft` |
| sheetmetal | `develop/`, `develop.rs` |
| mesh | `mesh_reconstruction/`, `mesh_reduce/`, `mesh_reduce.rs` |
| blend | `blend/`, `blend.rs` |
| offset | `offset/`, `offset.rs` |
| weld | `blend/weld_bead`, `weld_symbol/`, `weld_symbol.rs` |
| frames | `prism/member`, `prism/gusset` |
| not ported | `error.rs`, `error/`, `kernel.rs`, `lib.rs`, `profile.rs`, `geom/context`, `geom/object`, `geom/a_object`, `geom/check_memo`, `geom/content`, `geom/attribute`, `surface/surface_cache`, `surface/trim_srf_cache`, `topology/brep_cache` |

### B.2 Items

Each item below moves alone, from its defining file to the crate shown.

| Items | File | To |
|---|---|---|
| `TopologyToUpdate` | `geom/tol.rs` | brep |
| `ends_on` | `curve/curve_cache.rs` | curve |
| `curve_on_classify` | `surface/surface/p2.rs` | bodyq |
| `TUBE_PAIRS`, `tube_clears`, `check_vertex_clearance`, `vertex_neighbourhood_edges`, `face_contact`, `edge_tube_reaches`, `GROWTH_PAST_RATIO`, `vertex_ball_reaches` | `topology/brep/validation/pointers.rs` | brep |
| `is_outward_solid`, `outward_solid_failure`, `outward_touching_solids_failure`, `is_valid_and_outward`, `valid_and_outward_failure` | `topology/brep.rs` | check |
| `validate_pointers` | `topology/brep_constructor.rs` | check |
| `end_shell` | `topology/brep_constructor.rs` | stitch |
| `with_nurb` | `topology/brep/p3.rs` | surface |
| `classification_builds_tree` | `query/face/p2.rs` | brep |
| `curve_bounded_surface_assert_valid` | `surface/surface/assert_valid.rs` | ssi |
| `standalone_curve_assert_valid` | `topology/brep/validation/assert_valid.rs` | ssi |
| `degenerate_low_work_transform` | `topology/brep/mirror.rs` | check |
| `BIG_ULONG`, `OP_SLOTS`, `add_curve_output_slots` | `topology/api_files.rs` | brep |
| `curve_crossings` | `intersect/api_intersectors.rs` | surface |
| `plane_global_curve_intersect` | `curve/curve/p4.rs` | surface |
| `span_bound`, `node_on` | `nurbs/certificate.rs` | surface |
| `OP_SURFACES`, `CROSSING_WITNESS_REL`, `CROSSING_WITNESS_STEPS`, `intersect_surfaces`, `intersect_surfaces_within`, `refuse_points_where_a_plane_crosses` | `intersect/api_intersectors.rs` | ssi |
| `impl:Default` | `topology/proto_topology/p1/copy_brep_map.rs` | brep |
| `NESTING_LIMIT_REASON`, `MAX_READ_NESTING` | `topology/database_io.rs` | num |
| `TYPE` | `topology/proto_topology/p1/owning_topology.rs` | brep |
| `RecognitionVector`, `conic_ellipse`, `EllipseScalar`, `loop_circle`, `loop_ellipse` | `prism/recognition.rs` | planar |
| `clamp_curve` | `nurbs/iges.rs` | nurbs |
| `is_square_plane_at` | `geom/plane_height.rs` | curve |
| `bernstein_roots` | `query/tangent_edge.rs` | algo |
| `curve_interval` | `eval/label.rs` | brep |
| `plane_projection` | `topology/api_projection.rs` | surface |
| `intersect_curves`, `CurveIntersections` | `intersect/api_intersectors.rs` | curve |
| `MAX_GEOMETRY_NESTING` | `surface/surface/p2.rs` | curve |
| `derived_face_shell`, `derived_face_loops`, `derived_loop_face`, `derived_loop_edges`, `derived_shell_faces` | `topology/integrity.rs` | brep |
| `xsect_tol_3d_of_brep` | `topology/vertex/corner.rs` | brep |
| `join_with`, `join_with_gap` | `blend/fillet_corner/p3.rs` | curve |
| `classify_blend_spine`, `circle_spine_station` | `blend/fillet_exact.rs` | surface |
| `create_face_from_surface` | `blend/fillet_executive/api.rs` | make |
| `boolean_2d`, `boolean_2d_keyed`, `Boolean2d` | `primitive/primitive_creation/p1/primitives.rs` | boolean |
| `seam_normal_angle`, `continuity_over_surfaces`, `continuity_on_surfaces` | `query/continuity.rs` | surface |
| `calculate_bounding_box`, `calculate_tight_bounding_box` | `query/brep.rs` | brep |
| `calculate_bounding_box`, `calculate_bounding_box_in` | `query/face/p1.rs` | brep |
| `calculate_tight_bounding_box`, `calculate_tight_bounding_box_into` | `query/face/p3.rs` | brep |
| `calculate_bounding_box`, `calculate_tight_bounding_box` | `query/edge.rs` | brep |
| `mult_4x4` | `geom/geom_utility/p2.rs` | num |
| `drop_limit` | `topology/brep_constructor.rs` | curve |
| `curve_on_quadrics_with_constants`, `spine_on_offsets`, `curve_segments`, `piece_turn`, `sub_segment` | `surface/implicit.rs` | curve |
| `ShellRayWorld` | `topology/shell.rs` | bodyq |
| `VertexSector` | `topology/face/p4.rs` | bodyq |
| `curve_derivs_over`, `curve_derivs_over_about`, `derivs_over` | `blend/fillet_section/certificate.rs` | curve |
| `segment` | `eval/pcurve.rs` | curve |
| `count_area_sum` | `surface/procedural_blend.rs` | num |
| `solve_quadratic_eqn` | `geom/geom_utility/p1.rs` | num |
| `surface_projected_may_contain_answers` | `geom/global_solver.rs` | surface |
| `stretch_clears_on_surface`, `first_fundamental_form`, `trim_metric`, `trim_turn_on_surface` | `nurbs/b_spline_curve/p3.rs` | surface |

### B.3 Types

Each type below moves together with its inherent methods.

| Types | File | To |
|---|---|---|
| `Obstacle`, `Cell`, `VertexNeighbourhood`, `FaceContact` | `topology/brep/validation/pointers.rs` | brep |
| `ObjectHandle` | `topology.rs` | surface |
| `GnLawRound` | `blend/fillet_section/section.rs` | surface |
| `TsectPnt` | `intersect/surface_intersector/p1.rs` | surface |
| `MergeCurvesObjects`, `RayFireResult`, `SharedBoundary` | `topology/brep/p2.rs` | bodyq |
| `SelfClassification` | `topology/brep/p7.rs` | bodyq |
| `FaceCurveRun`, `QueryBrep` | `topology/face/p2.rs` | bodyq |
| `FaceIntersectCaches`, `FaceIntersection` | `topology/face/p3.rs` | bodyq |
| `FaceLineCache`, `HasSeamProblemOptions`, `SeamCheck`, `UvLineIntervals` | `topology/face/p4.rs` | bodyq |
| `Objects`, `ShellRaySector`, `ShellRayState` | `topology/shell.rs` | bodyq |
| `ClassifyHandle` | `surface/surf_of_extrusion/p2.rs` | bodyq |
| `LazyBrep` | `query/brep.rs` | brep |
| `NurbsCurve` | `blend/fillet_section/certificate.rs` | curve |
| `Side`, `Crv` | `blend/blend_corner/mixed/algebra.rs` | surface |
| `Face` | `blend/blend_setback.rs` | surface |
| `ClassInputs`, `QuadricKind`, `BlendQuadric`, `BlendSpine`, `ExactBlend` | `blend/fillet_exact.rs` | surface |
| `DraftDefinition` | `offset/draft.rs` | surface |
| `SectionBlendDefinition` | `sweep/section_blend.rs` | surface |
| `BezierSpan`, `BezierAux1d` | `curve/curve_cache.rs` | algo |
| `ExtendRefusal` | `surface/extend.rs` | nurbs |
| `Nurbs` | `eval/spline.rs` | nurbs-surface |
| `Bracket` | `surface/side.rs` | num |
| `SurfParamType` | `nurbs/b_spline_surface/p1.rs` | curve |
| `OrientType` | `nurbs/b_spline_surface_n_lib/p2.rs` | curve |
| `conic_turn`, `QuadricEquation`, `BPoly`, `Turn`, `CurveSegment` | `surface/implicit.rs` | curve |
| `CopyBrepMap` | `topology/proto_topology/p1/copy_brep_map.rs` | brep |

### B.4 Sink settings

- Whole sinks, written as the crate that receives the code ← the crate it comes from: brep ←
  bodyq, surface ← blend, ssi ← check.
- Largest helper set that may sink: 2000 lines.
- Crates that keep their code: record.
- Crates whose rising families may take their callers along: record, boolean, stitch, ssi.
- Types may sink, except those under these hubs: `topology/`, `topology.rs`, `surface/surface`,
  `curve/curve`, `surface.rs`, `curve.rs`.

The removals of 9.3 are `REMOVE` in the same file. The dependencies of 5.1 are `DEPS`.

## Appendix C: the graph after Re-review 2 (MEASURED)

Re-review 2 of review-56 judged a 31-crate graph (E1 to E8, ruled 11-27). `assignment.py` now
holds that graph: the crate dependencies of its table, the moves of E1 to E4 and E8, and a
removal under a named design of 01 for each edge that the ruling drops rather than moves. The
proof ran on the same complete call graph of the pin as section 9.

### C.1 Result

- 918 files (50 test), 48,982 units, 222,584 unit edges (545,129 references).
- **Removed by design changes: 323** (286 before). The 37 new ones:

  | Removed edge | Design (01) |
  |---|---|
  | the intersection curve's dispatch arms in `eval/curve.rs` | §7.3: a procedural kind over two surfaces, like the on-surface kinds (S6) |
  | `make_topology` → `heal_made` | §1.1 row 6, §11.1: the reader decodes the exact stored form and does not heal it (E2) |
  | `mirror_in` → `find_and_set_infinite_region`, `BrepQuery` | §2.1: the region is stored; `RegionId(0)` is the infinite one |
  | `transform_in` → `cover_moved_trims` | §2.4, §2.1: a transform keeps ids and the pcurves on its edge uses |
  | the face boundary-tree fill → the body point classifier | §3.2 rule 2: a fill reads its face record and the geometry it names; the tree labels its leaves from the face's own loops |
  | `calculate_bounding_box_in` → `tight_face_box` | §3.1: the face box is a cell; the pin's `tight` flag becomes a separate tight-box read in bodyq |
  | mass `estimate` → the tight body box | §3.1: mass uses the box only for its midpoint, which the body box cell gives |
  | `end_shell` → `validate_pointers` | §2.3: the structure check is `Draft::finish`'s |
  | `assert_valid_with_walk` → the curve-bounded and offset checks | §2.3: geometric validity is the check's |

- Unassigned files: none.
- **References outside the declared graph: 16, in 12 unit edges.** All 16 are in four edges
  that the ruling dropped (C.2).
- **Cycles: none.**
- **Trait impls: 10 moved to their trait's or type's crate; 7 are left in neither** (34 and 21
  before). The seven:
  - `boolean/merge/p4/arena_pair.rs`: `ClassGeometry` and `CurveOnClassifyObjects` for a local
    type, in boolean;
  - `offset/offset_geometry_creation.rs`: `From for Curve`, in offset;
  - `query/face/p2.rs`: `ClassObject for EntityRef`, in brep;
  - `surface/surf_of_extrusion/p2.rs`: `ClassGeometry for ClassifyGeometry`, in bodyq;
  - `topology/brep/p5.rs`: `FaceEdgeBackend for ArenaFaceEdge`, in bodyq;
  - `topology/vertexuse.rs`: `SectorBrep for Changing`, in bodyq.
- Whole sinks: surface ← blend only (262 units, 8,650 lines). Re-review 2 drops brep ← bodyq
  (E4) and ssi ← check (E8). The proof sinks 1,126 units in all; bodyq now rises whole, so an edit
  that classifies goes up with its classification.
- **Moved code: 59,951 non-test lines** of 892,037 ported, in 2,187 units.

### C.2 Edges the code still needs

None of these is added in `assignment.py`. Each is a read that 01 §1.1 placed in bodyq when
bodyq held mass and the surface intersections, so each is a real need, not an incidental call.
With the four added, the proof gives 0 references outside and no cycles.

| Edge | Refs | Pin items | Need |
|---|---:|---|---|
| bodyq → ssi | 5 | `curve_class/witness.rs` `tangency_point` → `global_surface_intersect`; `curve_class/p2.rs` `refine_on_pair` → `SurfaceIntersector::new`, `refine_point`; `topology/brep/p1.rs` `create_parallel_projection_curves` → `Surface::create_parallel_projection_curves_with_context` | curve classification at a tangent contact refines on the two surfaces; projecting onto a body intersects surfaces |
| check → mass | 2 | `valid_and_outward_failure` and `validation/soundness.rs` `volume_sign` → `mass/enclosure.rs` `volume_sign` | the outward test is a signed volume |
| local → mass | 8 | `feature_executive.rs` and `feature_executive/annular.rs` `bounds_material` → `partition_face`, `sheet_face` | deciding which side holds material integrates the faces |
| mesh → mass | 1 | `mesh_reconstruction/native.rs` `volume` → `brep_volume_default` | the mesh reconstruction reads the body's exact volume |

E1 rules the tangency witness to boolean. It sinks back into bodyq, because the curve
classifier (`curve_class/p1.rs:626`) calls it.

### C.3 Places where the pin contradicts the ruling

- **E4, the face cells.** At the pin the boundary-tree fill and the tight face box call the body
  point classifier. The assignment takes the design of 01 §3.2 rule 2 and removes those edges
  (C.1). Moving `intersect/point_class` into brep instead also gives no brep → bodyq edge and no
  cycle (brep 91,527 lines, bodyq 66,435). But it brings into brep the classifier's topology
  trait, the ray preparation that makes trims on read (01 §11.1), and live ray contexts, which
  E4 rules out.
- **E8, `is_outward_solid`.** E8 calls it an orientation read for bodyq. At the pin it runs the
  full validity check (`check_brep`) and then the volume sign, so it stays in check. Its caller
  `radial_sweep` (with `contract_complete_cone`) goes to make, the lowest crate that holds
  everything it calls.
- `scx-check` declares ssi, but its code uses ssi only through bodyq; boolean's code uses only
  ssi and stitch directly.

### C.4 The measured table

"Uses" lists the direct dependencies that the crate's kernel code uses. A dagger marks a use
outside the declared graph (C.2). With the C.2 edges added, bodyq is 73,693 lines and check
8,653; the rest are unchanged.

| Layer | Crate | Deps | Uses | Kernel lines |
|---:|---|---|---|---:|
| 0 | `scx-num` | std | — | 16,992 |
| 1 | `scx-algo` | num | num | 17,515 |
| 1 | `scx-nurbs` | num | num | 28,669 |
| 2 | `scx-curve` | nurbs, algo | nurbs, algo | 63,747 |
| 2 | `scx-nurbs-surface` | nurbs, algo | nurbs | 19,386 |
| 3 | `scx-planar` | curve | curve | 14,915 |
| 3 | `scx-surface` | nurbs-surface, curve | nurbs-surface, curve | 201,369 |
| 4 | `scx-brep` | surface | surface | 84,299 |
| 4 | `scx-sketch` | planar | curve | 363 |
| 4 | `scx-ssi` | surface | surface | 18,118 |
| 4 | `scx-teeth` | planar | planar | 11,826 |
| 5 | `scx-bodyq` | brep, planar | brep, planar, ssi† | 73,663 |
| 5 | `scx-mass` | brep | brep | 4,792 |
| 5 | `scx-record` | brep | brep | 8,155 |
| 5 | `scx-tess` | brep | brep | 1,158 |
| 6 | `scx-check` | bodyq, ssi | bodyq, mass†, ssi | 8,683 |
| 6 | `scx-hlr` | bodyq | bodyq | 10,948 |
| 6 | `scx-stitch` | bodyq | bodyq | 12,766 |
| 7 | `scx-boolean` | bodyq, ssi, stitch | ssi, stitch | 47,303 |
| 7 | `scx-deform` | check, stitch | — | — |
| 8 | `scx-make` | boolean, stitch, planar, check, hlr | boolean, check, hlr | 34,082 |
| 8 | `scx-repair` | boolean, stitch, check | stitch | 1,240 |
| 9 | `scx-frames` | make | planar | 948 |
| 9 | `scx-local` | make | make, mass† | 9,374 |
| 9 | `scx-mesh` | tess, make | make, mass† | 5,565 |
| 9 | `scx-sheetmetal` | make | make | 7,358 |
| 9 | `scx-taper` | make | bodyq | 773 |
| 10 | `scx-blend` | local, repair | local | 150,425 |
| 11 | `scx-offset` | blend, taper | blend | 36,702 |
| 11 | `scx-weld` | blend | blend | 903 |
| 12 | `scx-features` | offset | — | — |

Not ported: 39,980 lines, as in 9.2.

### C.5 The main flows

The flows over 600 lines:

| From → to | Units | Lines | What |
|---|---:|---:|---|
| brep → bodyq | 208 | 12,901 | the edits that classify (188 units, 12,538 lines rise): `make_faces_with_curves`, `make_edge_in_face_with_vu`, `move_seam_notified`, `is_inner_shell_of`, `build_face`, `glue_edges_general_searching`, `split_closed`, `orient_body`, and `mirror_in` by another path |
| blend → surface | 296 | 9,598 | the whole sink, plus the blend-surface items of 4.7 |
| bodyq → brep | 341 | 6,048 | sinks from `query/face`, `query/edge` and `query/loop`, and the face cells (E4) |
| curve → surface | 30 | 2,379 | the surface half of `eval/` (E3) and the dispatch arms of on-surface kinds |
| brep → record | 65 | 2,221 | body record encoding (7.3) |
| make → boolean | 61 | 1,990 | the planar boolean (P1) |
| surface → record | 58 | 1,985 | surface record encoding |
| algo → num | 150 | 1,847 | solver and matrix helpers |
| make → surface | 69 | 1,589 | surface constructions in sweep |
| mass → brep | 84 | 1,578 | the face and loop measures that brep's edits call: `loop_area_sign`, `face_properties_in`, `boundary_properties` |
| bodyq → surface | 65 | 1,417 | face-evaluation helpers the surface kinds call |
| curve → record | 27 | 1,277 | curve record encoding |
| surface → ssi | 21 | 1,203 | `global_surface_intersect` and its family |
| hlr → surface | 36 | 1,179 | silhouette helpers |
| brep → boolean | 19 | 1,037 | the brep-level boolean entry points (E1) and copy-and-merge |
| surface → curve | 70 | 966 | curve helpers in surface files |
| bodyq → planar | 54 | 867 | 2-D loop recognition (3.4) |
| ssi → surface | 83 | 854 | the intersector's point and index types |
| brep → stitch | 26 | 802 | brep's stitch entry points and `end_shell` |
| check → bodyq | 22 | 671 | the clearance proof, which classifies (E4) |
| boolean → bodyq | 15 | 625 | the tangency witness (C.2) and helpers the classifier calls |
| nurbs-surface → nurbs | 33 | 609 | NLib curve helpers in surface files |

The rising rule still places each unit by its edges alone, as in 9.2.
