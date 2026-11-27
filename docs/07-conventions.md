# 07. Conventions

The rules every solidcorex crate follows for units, angles, parameters, orientation, frames, numbers,
determinism, identity and naming.

Brought in line 11-26 with `01-architecture.md` v2 and `04-decisions.md`. Where v2 overrides a rule
below, the rule is edited to v2's position and marked "(changed 11-26, see 01 §N)". The old steps A to K
this document cited no longer exist; each reference now names the v2 step (01 §10).

For each item there are three parts:

- **READ**: what solidcore does today, cited at the pinned commit (03e16c9ee7).
- **Inconsistent**: where solidcore disagrees with itself.
- **Rule (JUDGED)**: the one rule a builder follows, with the reason and the alternatives considered.

Path abbreviations used below:

| Prefix | Path at the pin (`../sc-wt/scx-pin`) |
|---|---|
| `k/` | `solidcore/crates/solidcore/src/` (the kernel) |
| `rec/` | `solidcore/crates/solidcore-record/src/` |
| `cv/` | `solidcore/crates/solidcore-convert/src/` |
| `ops/` | `solidcore/crates/solidcore-ops/src/` |
| `app/` | `crates/` (the application, including the bridge `crates/smlib`) |

---

## 1. Units and scale

**READ**

- The kernel has no unit. `Tolerances` is documented as being "in model units" (`k/kernel.rs:46`).
- Its defaults assume parts of tens of millimetres:
  - `MODEL_SIZE_ESTIMATE = 50` (`k/geom/tol.rs:205`)
  - `ZONE_TOL_3D = 1e-5` (`k/geom/tol.rs:209`)
- The record states millimetres, radians, right-handed and Z-up (`rec/brep.rs:2`). The app's
  `docs/native-brep.md` says the same.
- `size_estimate_to_zone_tol_3d` (`k/geom/tol.rs:331`) maps a model size to a zone with a table of
  decades:

  | Size | Zone |
  |---|---|
  | 0.5 to 500 | 1e-5 |
  | 500 to 5e4 | 1e-4 |
  | 0.005 to 0.5 | 1e-6 |
  | ... | ... |

  The ranges are closed and overlap, so the order in which they are checked decides a size that
  lies on a boundary.
- `BIG_DOUBLE = 1e20` is "larger than any coordinate" (`k/math.rs:59`). It is used for inside-out
  extents and for an unset UV point (`k/topology.rs:650`, the `Vertexuse::uv_point` default).
- `INFINITE_PARAMETER = 1234567` (`k/math.rs:61`). The bridge repeats it as its own constant.

**Inconsistent**

- `size_estimate_to_zone_tol_3d` exists, but production always uses the fixed 1e-5 zone:
  - Only `KernelOptions::default()` is used (research 31).
  - `Context::model_size_estimate` stays at 50.
- The app's `Body::size` says "every kernel tolerance is a fraction of it"
  (`app/kernel/src/body.rs`, the field above `volume`). The kernel does not read it.
- Several absolute guards (`ZONE/10` and others) do not scale with the body (research 31).

**Rule (JUDGED)**

Use millimetres, `f64` and absolute lengths. A length that decides anything comes from the
`Document`'s resolution (changed 11-26, see 01 §4.3; not `Settings`), from an entity's accuracy, or from a named constant in `num`. None is written inline.

- **Why.** The record and the app are already in millimetres, and every golden row is measured at
  1e-5.
- **Alternatives rejected:**
  - Unitless with a per-document scale: the scale would have to flow to every guard, and today
    nobody sets it.
  - Relative tolerances only: they break near the origin and for long, thin parts.

The size-to-zone table stays as a pure function `num::tol::zone_for_size`. Its ranges are rewritten
as half-open (`[0.5, 500)` and so on) so that no size depends on check order.

Sentinels such as `BIG_DOUBLE`, `UNDEF_DOUBLE` and `USE_DEFAULT` do not cross a crate boundary.
`Option<f64>`, or an `Extent::EMPTY` that is inside-out by construction, replaces them.

## 2. Angles

**READ**

- Inside the kernel, angles are mostly radians, but the angle tolerance is held in degrees:
  - `Tolerances::ang_tol_deg = 0.5` (`k/kernel.rs:57`)
  - `ANG_TOL_DEG` (`k/geom/tol.rs:213`), converted by `ang_tol_rad` (`k/kernel.rs:86`, `k/geom/tol.rs:568`)
- Vector tests take degrees: `Vec3::is_parallel_to(o, ang_tol_deg)` (`k/geom/vector3d.rs:431`) and
  `is_perpendicular_to` (`:481`). `angle_between` returns radians (`:884`).
- STEP-style conic and revolve parameters are degrees in the kernel, as upstream had them:
  - `Ellipse::start_angle_deg` / `end_angle_deg` read the "STEP domain in degrees"
    (`k/curve/ellipse/p2.rs:1109`, `:1115`).
  - The cone builds its sweep with `sweep.to_degrees()` (`k/surface/cone/p2.rs:317`, `:469`).
  - `circ_weights(as_deg, ae_deg)` (`k/nurbs/crv_common.rs:516`)
  - `u_angle_deg` (`k/nurbs/nurbs_srf.rs:126`)
  - `angle_deg` (`k/sweep/segments.rs:102`)
  - `min_dihedral_ang_deg` (`k/topology/vertex/corner.rs:173`)
  - `sector_ang_deg` (`k/topology/vertexuse.rs:1547`)
  - offset `tol_deg` (`k/offset/offset_executive/edgeuse.rs:1164`)
- In the kernel, 692 lines convert between degrees and radians. Blend has 49 files with
  conversions, surface 33, topology 17 and curve 17.
- The record is radians throughout (`rec/brep.rs:2`). The bridge converts at the boundary with
  `deg2rad` / `rad2deg` (`app/smlib/src/core/convert.rs:103`, `:108`). It also snaps values within
  1e-9 degrees of a quarter turn onto it, with `snap_angular_boundary_deg` (`:113`). That is an
  extra rounding in each direction.

**Inconsistent**

- `are_parallel` / `are_perpendicular` in `k/geom/tol.rs:887`, `:895` take radians and then call
  `rad_to_deg` to reach the degree-taking `Vec3` test.
- `curve_class` uses `6 * ang_tol_rad()`, which is 3 degrees, while the intersectors use 20-degree
  literals (research 31). Angle thresholds have no single owner.
- Research 39 measured a cylinder radius drift of 2e-11 on the record round trip. Part of the cause
  is the degree conversion and snapping.

**Rule (JUDGED)**

Use radians everywhere inside solidcorex, held as `num::Angle(f64)` where a value is an angle
parameter. There is no angle tolerance (changed 11-26, see 01 §4.2; `AngleTol` is removed). Degrees
appear only:

- in `step` (the file format)
- in `ops` parameters the user typed

They are converted once there and never snapped.

- **Why:**
  - It removes 692 conversion lines and a source of rounding on the round trip.
  - It makes "exact stored form" (principle 7) possible for conics.
- **Alternative:** keep upstream's degree domains for conics, for fidelity. Rejected: fidelity is
  kept by the external comparison harness (01 §9.1), not by the internal unit.
- **Results move.** Every conic whose domain was stored in degrees moves by up to one rounding per
  conversion. Changed 11-26: there is no separate step for this. solidcorex is written in radians from
  the start (01 §10.2 step 2 for `scx-geom`, step 3 for `scx-record`). A difference from the pin
  beyond the difftest's tolerance is settled against the truth (02 §3).

## 3. Parameter domains

**READ**

- Every curve kind has a `natural_interval`:
  - `k/nurbs/b_spline_curve/p1.rs:738`
  - `k/curve/line.rs:668`
  - `k/curve/iso_curve.rs:321`
  - and the others
- Every surface kind has a `natural_uv_domain` (`k/nurbs/b_spline_surface/p1.rs:1139`).
- An edge holds `interval`, "the used parameter interval of the curve" (`k/topology.rs` `Edge`,
  ~l.424).
- A face holds `uv_domain`. Its accessor says this is "the domain of the surface the face uses"
  (`k/topology/face/p2.rs:261`, `:266`). It is not the region the trims cover.
- An edge use is evaluated at a normalised parameter in `0..1`, with orientation applied
  (`normalized_evaluate`, `k/topology/edgeuse/p3.rs:3928`). There are 136 call sites of
  `normalized_evaluate*` variants. Separate copies exist in face, edge, brep, stitch and
  vertexuse (`k/topology/vertexuse.rs:951`, `k/heal/stitch/p1.rs:2775`, ...).
- The record keeps each analytic face's parameters in its NURBS form's `(u, v)`, none periodic, so
  no loop is moved by a period (`cv/brep/record.rs:14-16`).
- Seams are edges, and poles are vertices. A pcurve never jumps a period (solidcore
  `docs/validity-and-tolerances.md` §6, §9 invariant 4).
- A periodic direction runs up to 2π and may cross the seam (app `docs/native-brep.md`).
- The bridge turns a circle whose schema-4 interval runs past a full turn back into [-2π, 2π]
  (`app/smlib/src/import.rs` `turned`, ~l.620).

**Inconsistent**

- `Face::uv_domain` is sometimes the surface's whole domain. On an offset face it is [0,1]² while
  the trims span [0.19,0.92]×[0.18,0.82], so the tight box is 13.47 mm too large (research 32).
- The same field is read as "the face's domain" by box and tree code.
- A normalised parameter exists alongside the curve parameter. The same edge is therefore
  evaluated at two parameterisations depending on the caller.

**Rule (JUDGED)**

1. **Parameters.** A curve or surface has one parameterisation, its natural one, and every
   evaluation takes that parameter.
2. **Edges.** An edge stores its `interval` within the curve's natural interval.
3. **Faces.** A face stores its own `uv_box`, which is the hull of its trims: a derived cell, filled
   once. It never stores the surface's domain, which is asked of the surface.
4. **Normalised parameters.** These are a local convenience inside one function (`t ∈ [0,1]` mapped
   by `interval.at(t)`), never a stored or exchanged value.
5. **Periodic surfaces.** A face's trims lie in one period window, `[u0, u0 + 2π)`, chosen by the
   operation that makes the face. A seam the face crosses is an edge, as today.

- **Why:**
  - It removes the box error of research 32.
  - It makes "the domain" mean one thing.
  - The 136 normalised call sites become `interval.at(t)` with no second evaluator.
- **Alternative:** keep `uv_domain` as the surface's and add a `trim_box`. Rejected, because two
  fields with confusable names is the current problem.

## 4. Orientation

**READ**

- A face use's `orientation` is `Same` when "the face use normal is the surface normal"
  (`k/topology.rs` `Faceuse`, l.529, field at ~l.538). The primary face use is the `Same` one.
- A loop use's `orientation` is `Same` for an outer loop and `Opposite` for an inner one
  (`k/topology.rs` `Loopuse`, l.560).
- Loops are stored counter-clockwise (outer) and clockwise (inner) about the surface's natural
  normal ∂S/∂u × ∂S/∂v. Two places say so:
  - `k/topology/face/p1.rs:791`, for `OrientedTrimBoundary`
  - `k/topology/face/p1.rs:1383`: turning a face over keeps "its loops counter-clockwise about the
    new normal"
- An edge use's `orientation` is `Same` when it runs with the edge's curve. `ccw` / `cw` are the
  next and previous edge uses in the loop (`k/topology.rs` `Edgeuse`, l.589).
- In a closed shell, the record's `forward` face normals point out of the material. Its loops are
  stored relative to the natural normal, as the kernel's are (`cv/brep/record.rs:17-19`; app
  `docs/native-brep.md`).
- An oriented edge in the record has `upper = !forward` (`cv/brep/record.rs:27`).
- The app's assembly input is the other way. Loops are given "seen from outside the material".
  `Assembly::finish` reverses them on a face that is not forward
  (`app/kernel/src/assemble.rs:8-11`, `:108`).

**Inconsistent**

- The record and the kernel agree except on a plane whose two scales have opposite signs, where
  convert turns the loops over (`cv/brep/record.rs:17-19`).
- The app's assembly uses the outside-material convention and the record the natural-normal one.
  Both are documented, but a builder must know which one a given function takes.

**Rule (JUDGED)**

1. **Loops.** Loops are stored relative to the surface's natural normal: outer counter-clockwise
   and holes clockwise, seen from ∂S/∂u × ∂S/∂v.
2. **Faces.** A face's side is a separate bit, `outward: bool`, meaning the material lies against
   the natural normal when it is true.
3. **Builder input.** The builder (`brep::Draft`) takes loops in this convention only.
   Outside-material input is converted by one named function, `loops_from_outside`, in `ops`.
4. **Planes.** A plane never has scales of opposite signs. Its frame is right-handed by
   construction (see §5).

- **Why:** it is what both the kernel and the record already store, so `scx-brep` and `scx-record` (01 §10.2 step 3) move nothing.
- **Alternative:** store loops relative to the outward normal (as STEP's `FACE_BOUND` orientation
  reads). Rejected: every face turn-over would rewrite its loops, and research 51 wants untouched
  faces shared.

## 5. Frames

**READ**

- `Transform` is a 4×4 matrix with row vectors (`k/math.rs` ~l.160 on).
- Analytic surfaces carry a placement: origin, axis and reference direction.
- The bridge re-derives plane frames on import (`app/smlib/src/import.rs:585`, `plane`):
  - x is set to u normalised and z to u × v normalised, then y = z × x.
  - The parameter range is recomputed from the domain's four corners projected onto the new axes.
  - Research 39 measured the origin of a re-derived plane moving by up to 105 mm.
- The torus `tube_start` is set to 0, and the `along` frame flips on the round trip (research 41).
- No body has a placement. A rigid move rewrites every surface and curve, so a transform leaves 0%
  of faces untouched (research 51).

**Inconsistent**

- The record stores a plane's placement with its parameters scaled (`cv/brep/record.rs:8`, "a plane
  by its placement, its parameters its kernel ones times its scales"). The import then rebuilds the
  frame instead of copying it.
- The C++-shaped bridge reads row vectors. The record docs speak of column-style placements.
  Nothing documents which is meant.

**Rule (JUDGED)**

1. **Placements.** A placement is `num::Frame { origin: Point3, x: UnitVec3, z: UnitVec3 }`, with
   y = z × x derived and right-handed by construction. It is stored as given and never
   re-derived: import copies it bit for bit (principle 7).
2. **Matrices.** These are column vectors (`p' = M p`) in `num`.
3. **Row vectors.** Rows exist only inside `convert` when reading the old kernel's matrices, and
   carry the name `RowMajorTransform`.
4. **Plane scales.** A plane's scaled parameters become an affine `(scale_u, scale_v)` on the
   plane, both positive.

- **Why:** this makes the round trip a fixed point for planes, conics and tori (research 39: 22% of
  474 are bit-identical today).
- **Alternative:** re-derive frames canonically on import, which gives one representation per
  plane. Rejected because it can never be a fixed point for a file written by someone else.
- **Open:** whether a body carries a placement (04 §6). See the owner questions.

## 6. Numbers and constants

**READ**

Constants in `k/math.rs`:

| Constant | Value | Line |
|---|---|---|
| `PI` and multiples | | 35 on |
| `EFF_ZERO` | 1e-12 | 45 (282 references, research 49) |
| `EFF_ZERO_SQ` | 1e-24 | |
| `EFF_ZERO_SQRT` | 1e-6 | |
| `EFF_ZERO_PARAM` | 1.2e-8 | 51 |
| `EFF_ZERO_DEG` | 1e-4 | 53 |
| `UNDEF_DOUBLE` | 1.23456e-61 | 57 |
| `BIG_DOUBLE` | 1e20 | 59 |
| `INFINITE_PARAMETER` | 1234567 | 61 |
| `BOUNDED_INFINITE_PARAM` | 33 | |

Constants in `k/geom/tol.rs`:

| Constant | Value | Line |
|---|---|---|
| `USE_DEFAULT` | 9876543.21 | 201 |
| `UNINIT_TOL` | 1.23456e-61 | 203 |
| `TOL_GAP_GAIN` | 1.1 | 211 |

Comparison helpers in `k/geom/tol.rs`:

- `is_zero` uses `EFF_ZERO` as an absolute value.
- `are_same` uses a scaled zero: `EFF_ZERO × (1 + |x|)` (`scaled_zero`, `:1005-1067`).
- `in_tol` (`:822` on).
- `within_contact_tol` widens by γ₃₂ × size (`:483`).
- The certified comparisons answer `Yes` / `No` / `Undecided` using Higham's γ_k
  (`k/geom/certified.rs:1-80`).
- `coincidence_tol_3d` keeps a NaN zone NaN on purpose (`k/geom/tol.rs:519`).
- `above(a, b)` is false for NaN, "so a refusal written `!above(..)` refuses NaN too"
  (`k/geom/certified.rs`).

Floating types: the kernel has no `f32` in code (three comments about STL input). The record uses
`f32` only for colour and opacity and its binary serialiser (`rec/brep/envelope.rs:10`,
`rec/native/binary.rs:431`).

Duplicated tolerance constants (determinism audit, 11-26):

- `XSECT_TOL_3D` is written as the literal `2.0 * 1.0e-5` (`k/heal/gap.rs:90`) and again privately
  (`k/surface/surface/p5.rs:131`).
- `APPROX_TOL_3D` is written as `1.0e-5 / 2.0` (`k/curve/crv_on_surf.rs:178`), with private copies
  at `k/topology/vertexuse.rs:143` and `k/topology/brep_data.rs:114`.
- Private copies of `ZONE_TOL_3D` exist at `k/nurbs/b_spline_surface_n_lib/p2.rs:131`,
  `k/topology/brep_data.rs:112` and `k/surface/trim_srf_cache.rs:136`.
- `DEGENERATE_TOL_3D = ZONE_TOL_3D / 10` exists twice (`k/surface/surface/p1.rs:229`,
  `k/topology/brep/p7.rs:205`).

**Inconsistent**

- `UNDEF_DOUBLE` and `UNINIT_TOL` are the same value, 1.23456e-61, under two names in two files.
- Both are valid `f64` values, so a sentinel can leak into arithmetic unnoticed.
- `EFF_ZERO` is used both as an absolute length (1e-12 mm) and as a relative factor. The 282
  references do not say which.
- The `ZONE/10` guards and `LARGE_SMALL_SIZE_RATIO * b.tolerance()` multiply by the constant,
  never by the stored per-body ratio, which is never read (research 31).
- The zone is defined in at least six places (above). Changing `Tolerances::zone_tol_3d` would not
  change the literal copies.

**Rule (JUDGED)**

1. **Types.** `f64` only. No `f32` in geometry.
2. **Constants.** All numeric constants live in `num::consts`, each with a doc line saying:
   - its unit (mm, rad, a parameter, or relative)
   - who owns the decision it makes
3. **Absolute and relative zeros** are two names: `num::EPS_LEN` (absolute, mm) and `num::EPS_REL`
   (relative, applied by `scaled(x)`). Each of the 282 `EFF_ZERO` sites is classified when its file
   is rewritten (01 §10.1 step 1 for `scx-num`, step 2 for geometry), not before.
4. **No sentinels.** `Option`, `Extent::EMPTY` or a typed `Unset` replace them.
5. **NaN.** Every comparison that decides is written so that NaN takes the refusing branch (the
   `above` pattern). Sorting uses `f64::total_cmp`.
   - Today the kernel has 227 non-test `total_cmp` lines and 8 sorts whose comparator is
     `partial_cmp(..).unwrap_or(Equal)`, for example `k/blend/fillet_section/section.rs:1467`
     (where a NaN is pushed and then sorted, at `:1465`), `k/topology/brep/enclosing.rs:74` and
     `k/topology/face/p1.rs:2471`. Since Rust 1.81 a non-total comparator may panic in `sort_by`,
     so these are possible panic sites as well as meaningless orders.
   - A `partial_cmp(..).unwrap()` or `unwrap_or(Equal)` in a sort is disallowed (clippy) in
     every crate.
6. **Signed zero.** It is not normalised. Code that keys on bits (memos, hashes) uses
   `num::key_bits(x)`, which maps -0.0 to +0.0 and every NaN to one bit pattern.

- **Why:** each of these is a class of silent error that solidcore has hit at least once (the
  research 31 constant multiplications; the sentinel value 1.23456e-61 in two places).

## 7. Determinism

**READ**

- **Random numbers.** The kernel's random draws come from `Rng`, a SplitMix64 generator
  (`k/kernel.rs:97`; `DEFAULT_SEED` at `:103`). Every call starts a fresh generator from the
  configured seed (`Kernel::seeded_rng`, `k/kernel.rs:1611`; field `seed`, doc at `:1318-1321`),
  so a call's draws do not depend on what ran before. This was made so on 11-23 (bad95c31f7,
  #20942). Three production callers:
  - `points_in_face` (`k/topology/face/p2.rs:1719`)
  - `is_inner_shell` (`k/topology/topology_traverser.rs:797`)
  - the inner-shell test's ray directions (`k/topology/shell.rs:1732`)

  Every kernel in the ops pool is built with default options (`ops/session.rs:201`), so all share
  one seed. There is no `rand` crate, no clock seeding, and no hashing of addresses.
- **Fused multiply-add.** `mul_add` appears at 34 non-test sites, all in the kernel:
  - 15 compute the exact error of a product (error-free arithmetic, Kahan's `ab - cd`):
    `k/nurbs/b_spline_curve/speed.rs:388-396`, `k/geom/expansion.rs:70`, `k/query/exact.rs:27`,
    `k/curve/length_bound.rs:446` and others.
  - 19 are ordinary fused evaluations: `k/prism/transform.rs:51-60`,
    `k/surface/extend/ball.rs:960-962`, `k/surface/extend/local_iso.rs:26-71`,
    `k/surface/surf_of_extrusion/p2.rs:781-783`, `k/offset/construction/sheets.rs:398` and others.

  Rust's `mul_add` is correctly rounded on every target (it falls back to libm `fma` without the
  instruction), and Rust never contracts `a*b+c` by itself. These sites are therefore
  deterministic, given a correct libm `fma`. There is no SIMD, `std::arch` or `target_feature`, and
  `unsafe` is forbidden workspace-wide. The ops crate disallows `mul_add` because ops must not
  compute geometry (`solidcore/crates/solidcore-ops/clippy.toml:41`, `:72`).
- **Hash maps.** Every `HashMap` / `HashSet` uses std `RandomState`, so its iteration order
  changes from one process run to the next. BTree containers outnumber hash ones about 23 to 1 in
  the kernel (647 lines against 28). An audit of every iteration found:
  - no site where hash order reaches a geometric result
  - three in ops where order can decide Ok against Err, because a `?` inside the loop fails on
    whichever entity is visited first (`ops/blend.rs:2307`, `:2321`, `:2395`)

  Raise records use `BTreeMap` (`k/topology.rs:869`, `TrimCoverRaises`).
- **Thread-locals and statics.**
  - The kernel has no `thread_local!`, `static mut` or `LazyLock` outside tests. Its one static
    holds error-site source text (`k/error/site_scan.rs:898`).
  - ops carries per-call context in `thread_local!`s (`ops/session.rs:250-276`, `:314`, `:321`;
    `ops/cancel.rs:25`; `ops/fuel.rs:75`). It also has process statics: the kernel pool `IDLE`
    (`ops/session.rs:59`), `FIXES` (`:60`) and `HOOK` (`:82`).
- **Threads.** The kernel starts no threads. HLR runs a caller-provided `Workers`
  (`k/hlr.rs:663-677`). Results come back in item order, but the error returned is the first
  failure in time (`:915-919`), and a ray budget is shared through atomics (`:2920`). So which
  error comes back can depend on scheduling.
- **Interior mutability.** The kernel carries several memos as `Cell` / `RefCell` behind `&Kernel`:
  - `blend_station_memo`, `blend_w_memo`, `loop_area_memo`, `check_memo`, plus `Mutex` and
    `OnceLock` caches inside curve and surface values (`k/curve/crv_on_surf.rs:328`,
    `k/surface/surface/p2.rs:515`, ...)
  - `blend_net_tolerance`, a `Cell<f64>` that changes how a blend surface evaluates
    (`k/kernel.rs` ~l.1365)
- **Sums.** Mass properties sum face by face in list order, left to right, with no compensation
  (`k/mass/integrator/properties.rs:2376`, `:1615-1650`). Renumbering faces changes the low bits.
  Error-free arithmetic exists only for predicates and bounds (`k/query/exact.rs:17`,
  `k/geom/expansion.rs`).
- **History dependence.** Main on raw bodies is history-dependent at round-off level, up to 3.6e-14
  in a ray t (research 33).
  - Reversing face order changes 380 of 474 exports (research 39).
  - Only 86 of 474 records are a fixed point of the round trip (research 39).

**Inconsistent**

- Most memos claim "a hit returns what a fresh solve returns". `blend_net_tolerance` is different:
  it is a scope that changes results, carried on the same `&Kernel` as the memos.
- The kernel holds no random state, but the ops layer holds per-call state in thread-locals and a
  process-wide fix table.
- Research 33 and 39 show that order of construction leaks into results.

**Rule (JUDGED)**

1. **Same bits.** Same input, `Document` and `Settings` give the same bits for any number of workers,
   cache state, call order, or save and load. On every target too, verified late (04, 11-26; 01 §8.1).
   Agreement with solidcore is within a tolerance, not bit for bit (02 §3).
2. **Random numbers.** The seed is in `Settings` (kept, changed back 11-26, see 01 §6.3 and §12 item
   6; an earlier draft had moved it to `Document`). Each operation derives its generator as
   `Rng::new(mix(settings.seed, op_id, entity_id))`, never a shared stream, where `mix` is the one
   fixed SplitMix64 hash in `scx-num::hash` (01 §8.1). This makes draws
   independent of worker scheduling.
3. **Iteration.** (01 §8.1 adds: every hash that reaches a result is that fixed hash, every sort key
   breaks ties by id, and `DefaultHasher` and `RandomState` are test-only.) No iteration over `HashMap` / `HashSet` reaches a result. A hash container may be
   used for lookup only. Iteration that matters uses `BTreeMap`, an id-indexed `Vec`, or a sort by
   id. Enforced by a clippy lint on `HashMap::iter` / `keys` / `values` / `into_iter` / `drain`
   outside `#[cfg(test)]`.
4. **`mul_add`** is allowed only in named helpers in `num::eft`, of two kinds:
   - the error-free helpers (`two_prod`, `two_sum`), which keep the 15 error-term sites
   - `fma(a, b, c)`, for a deliberate fused evaluation

   It is disallowed elsewhere by clippy (01 §8.3). Changed 11-26: batched evaluation is out of the
   first design (01 §9.4), so there is no lane code yet. The 19 evaluation sites either move into
   `num::eft::fma` unchanged (no result moves) or are unfused (moves those results by a rounding),
   decided as each is ported on accuracy and speed. Changed 11-27 (04 §1): a moved result is not a
   reason either way; a difference from the pin beyond tolerance is settled against the truth
   (02 §3).
5. **Sums.** Every reduction over a variable-length set runs in id order, left to right. A parallel
   reduction collects per-item values and sums them in that order afterwards. There are no
   cross-lane horizontal sums.
6. **No `thread_local!` and no `static` mutable state** in any crate. The kernel already has none.
   The ops thread-locals (call context, cancel, fuel, fixes) become the explicit `Cx` (document,
   settings, `&mut Scratch`, `&Stop`, trace; 01 §6.3), and the kernel pool becomes a pool of `Scratch`.
   The idle pool, fix switches and hook are not carried over.
7. **Errors from workers.** When several workers fail, the error reported is the one for the
   lowest item index, never the first in time. A shared budget is split per item in advance.
8. **Scratch may memoise, but must never change a result.** A value that changes results (like
   `blend_net_tolerance`) is an explicit argument, not scratch.
9. **Transcendental functions.** `sin`, `cos`, `atan2`, `exp` and `ln` are not correctly rounded by
   the platform libm. Decided 11-26 (04): `scx-num` ships its own, `fmath`, the same on every target
   including wasm, covering the list in 01 §8.3; the platform's are banned outside it.

- **Why:** principle 6 is stated in 01 but is not yet true, and each item above is a known way it
  fails.
- **Alternatives:** tolerate cross-platform last-bit differences and pin goldens per platform.
  Rejected as the default because it doubles the oracle.

## 8. Ids and identity

**READ**

- Ids are typed `u32` newtypes made by the `typed_id!` macro (`k/topology.rs:98-129`). They derive
  `Ord` and `Hash`.
- `Arena<T>` is a `Vec<Option<T>>` (`k/topology.rs:233`). A removed id is never reused, and
  iteration is insertion order (`k/topology.rs:229-232`; validity doc §9 invariant 1).
- A B-rep also has an `ObjectKey` identity, under which it is attributed and notified
  (`k/topology.rs:374`, `key`).
- The record renumbers faces, edges and vertices in a canonical walk (`cv/brep/record.rs:26`).
  `Order` maps each record entity to its kernel one.
- The record uses `u32` indices (`rec/brep.rs:39` on).
- The app's `Body` takes the record's canonical order as the body's own entity identity, for
  history, naming and meshes (`app/kernel/src/body.rs:172` doc).
- `Brep` is not `Clone` (#2715). Copies go through `brep_copy` / `brep_copy_mapped`, which return a
  map.

**Inconsistent**

- Three identities exist for one face:
  - the kernel arena id
  - the record's canonical index
  - the app's naming
- They agree only after a round trip, and the round trip is a fixed point in only 86 of 474 cases
  (research 39). Entity identity therefore depends on the export path.

**Rule (JUDGED, following the research 51 ruling)**

1. **Dense ids.** Ids are `u32`, typed per entity kind, and never reused within a body's lifetime.
   A draft appends and marks removed.
2. **Compaction** is an explicit operation, `Brep::compact(&self) -> (Brep, IdMap)`, never a side
   effect, and never done by saving. Changed 11-26 (01 §2.2, replacing research 51's "the stored form
   compacts ids"): the stored form keeps ids as they are, with runs of deleted slots written as counts.
3. **Canonical order** is id order, a function of the stored form alone, so writing a body twice gives
   the same bytes.
4. **Body identity.** Changed 11-26 (01 §2.2, §6.2): there is no `BodyId` and no session. A body is a
   value, and the app names its bodies as it likes. There is no `ObjectKey` in `scx-brep`.
5. **Naming** (which face of the previous version is this) comes from `Changes` and `carry` in the
   core and `scx-fates` in a helper (01 §2.6), not from an outcome map returned by `ops`.

## 9. Naming of files and functions

**READ**

- The kernel has 918 source files. 150 of them are named `p1.rs` ... `p11.rs` after upstream's
  translation units:
  - `k/surface/surface` has 11
  - `k/topology/brep` has 10 (`p1` to `p10`)
  - `k/intersect/curve_class` has 9
  - `k/curve/curve` has 9
- Each file's module doc says what it holds. For example, `k/topology/brep/p4.rs:1-5` holds edge
  and vertex deletion, stitching and shrink.
- Functions keep upstream's names in snake case. The doc's first words give the upstream name in
  backticks, for example `GetMate` (`k/topology/edgeuse/p1.rs:497`) and `NormalizedEvaluate`.
- Golden units are named after upstream units (`topology.brep_p4`, `edgeuse_p3`, `face_p3`).
- Fields are documented as `m_...` with the upstream member.

**Inconsistent**

- Files are named by upstream structure, but the content has drifted: "part 4" holds four
  unrelated topics.
- Mutable and read-only twins exist (`normalized_evaluate`, `normalized_evaluate_read_only`,
  `normalized_evaluate_existing_uv`, `k/topology/edgeuse/p3.rs:3928-4003`).
- Upstream names such as `GetUVDomain` map to fields whose meaning changed (§3).

**Rule (JUDGED)**

1. **Files** are named by what they hold (`brep/edit/delete_edge.rs`, `topo/trim/fit.rs`), never
   `pN`.
2. **Upstream names.** The upstream routine name stays in the doc comment's first line, as now
   (`/// Upstream: SmBrep::DeleteEdge`), so a search finds it. A generated table
   `docs/upstream-map.md` lists upstream name → solidcorex path, made by a script from those doc
   lines.
3. **Test units.** Changed 11-26: solidcorex has no golden units (02 §9). A `scx-difftest` case is
   named after the pin's unit it compares, so that unit's history stays readable.
4. **One function per meaning.** A reader takes `&Brep` and returns a value. There are no
   `_read_only` / `_existing` twins: the draft-time version is a different function on `Draft`,
   with a different name (`fit_trim` against `trim`).
5. **Rust style.** Names are `snake_case` verbs for functions and nouns for types. No `get_`
   prefixes. `is_` / `has_` for booleans. Units appear in names only at the boundary
   (`angle_deg` in `step` and `ops` parameters, never inside).

---

## Questions for the owner

1. **Transcendental functions.** Answered 11-26 (04): `fmath`, own implementations (rule 9 of §7).
2. **Degrees.** Answered by v2: radians from the start, with the difference from the pin inside the
   difftest tolerance (§2).
3. **Placement.** Answered by 01 C14 (for the owner's review): no placement on a `Brep`. `transform`
   rewrites geometry and keeps ids, and operations that take a second body accept `Placed` operands.
4. **Face domain.** Replace `Face::uv_domain` (the surface's domain) with a derived `uv_box` (the
   trims' hull)? This moves the tight box of offset faces (research 32) and every result that reads
   the domain.
5. **Upstream names.** Keep the upstream name in every doc comment indefinitely, or only until the
   port of that layer is complete and the pin moves on?
6. **Size table.** Answered by 01 §4.3: kept as `zone_for_size`, the rule that picks the size class at
   `Document` creation, not a per-operation call.
