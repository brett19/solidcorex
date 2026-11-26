# 52. Moving layer 0 into `scx-num`

Written for solidcorex. This covers solidcore at the pin 03e16c9ee766de5b9b1ced845e5149d62ff493b8, its file
`crates/solidcore/src` (read-only at `sc-wt/scx-pin`), and solidcorex `main` from b31f8c7 to 429af2c. Builds and tests ran on server2.
Trust: the counts and the differential result were MEASURED; the cuts and rulings were JUDGED.

## 1. What moved

The kernel's numeric layer moved into `crates/scx-num` with no dependencies. The table gives each file's lines and `#[test]` count, at the pin and here.

| scx-num file | from (pin) | lines pin / here | tests pin / here |
|---|---|---|---|
| math.rs | math.rs | 1830 / 1826 | 23 / 23 |
| limits.rs | limits.rs | 110 / 119 | 2 / 2 |
| vector2d.rs, vector3d.rs | geom/ | 507 / 512, 1538 / 1550 | 5 / 5, 13 / 13 |
| matrix.rs | geom/matrix.rs | 1367 / 1374 | 11 / 11 |
| extent1d.rs, extent2d.rs, extent3d.rs | geom/ | 1232 / 1230, 1105 / 1104, 1746 / 1672 | 10 / 10, 7 / 7, 14 / 14 |
| periodic_extent1d.rs | geom/ | 697 / 702 | 12 / 12 |
| certified.rs | geom/certified.rs | 383 / 425 | 10 / 11 |
| tol.rs | geom/tol.rs (the part without a body) | 2013 / 930 | 15 / 9 |
| transform.rs | geom/transform.rs (the matrix part) | 1181 / 515 | 9 / 5 |
| small_matrix.rs | geom/geom_utility/{p1,p2}.rs (the `smgu_*` 3x3/4x4 helpers) | — / 233 | — / 3 |
| error.rs, consts.rs, units.rs | new | 330, 47, 87 | 1, 1, 1 |

Each moved file's header names the solidcore file and the pin, keeps the Upstream record word for word and gives the changes in one or two lines.

**Cut, because they reach above layer 0:**
- the `Content` impls;
- `Extent3d::from_pseudo_box` and `circumscribe`;
- `of_placement`;
- the transform members that are not matrix members (pseudo-box and volume dispatch, and the point solves);
- the `tol` context and B-rep overloads.

The six tol tests and four transform tests left behind use bodies, PseudoBox or Volume. They belong to the layer that receives those items.

**Changed signatures:**
- The tol functions take `&Tolerances` in place of `&Kernel`. Each read was `k.tolerances.<field>`, so each now reads the same field, and results cannot change. The difftest builds a `Kernel` holding the same set to confirm this.
- `zone_for_size` is `size_estimate_to_zone_tol_3d` renamed to the glossary name.
- Extent `min`/`max` fields are private behind accessors.
- `is_valid_double` is private.
- `math::det3` is now `pub`. It was `pub(crate)` at the pin, and eval and blend use it.
- `Transform::from_matrix` is new.

**Errors:** `NumError { kind, operation, failure: NumFailure, cause }`. The operation strings and failure variants match solidcore's `SolidError`/`GeomFailure` one for one. The difftest compares the kind, operation, failure and cause chain.

## 2. Tests

- **Unit tests:** 128 in `scx-num`. These are every test at the pin for the moved items, plus new tests for consts, units, error, small_matrix, the zone_for_size boundaries, the small-topo codes and the default tolerances.
- **Tier-1 properties** (`crates/scx-num/tests/properties.rs`): 10 tests, each with 2000 SplitMix64 cases and an independent oracle.
  - next_up/down against the bit layout;
  - dot, cross, triple and 3x3 determinant against i128;
  - the 4x4 determinant against a Leibniz sum, and mult_4x4 against integer products;
  - unimodular inverses, where A·inv = I exactly;
  - the translation round trip;
  - extent union, intersect and contains against min/max;
  - zone_for_size against its table;
  - unitize;
  - the solve_linear_system residual.
  - All pass.
- **`cargo xtask fast`** runs fmt, clippy `-D warnings`, the layers check, the headers check and `cargo test -p scx-num`. It passes.
  - Run 1, cold after `cargo clean -p scx-num`: 1.42 s wall.
  - Run 2, warm: 0.22 s.
  - The tests take 0.02 s.
- `cargo test -p xtask`: 2 pass.
- `cargo clippy -p scx-difftest --all-targets -D warnings`: clean.
- `cargo build --target wasm32-unknown-unknown -p scx-num`: builds (0.40 s).

## 3. Differential test (`crates/scx-difftest`, tier 2)

Each public function, constant and operator is called on both sides with the same inputs, and the outputs are compared by `to_bits`, Debug for enums and the error fingerprint. Panics are caught and compared as outcomes.

**Inputs:**
- 34 edge f64 values: ±0, denormals, MIN_POSITIVE, 1e-13 … 1e300, MAX, ±inf and ±NaN;
- pools of vectors, extents, matrices, frames and tolerance sets built from them;
- every pair when that is at most 8000 inputs, otherwise the diagonal plus 3000 seeded draws.

**Result:** 267 tests and 616,874 inputs with 0 mismatches, in 0.07 s of test time.
- This excludes the control's 1600 inputs. The log's total of 618,474 inputs includes them.

**Negative controls:**
- comparing `max2` with `min2` reports 1410 of its 1600 inputs as different, and fails as expected;
- a second control shows the harness tells signed zeros, NaN payloads and panics apart.

**Coverage:** 470 public functions were found by script, and 466 are diffed. The remaining four have no public counterpart at the pin:
- `certified::next_up`/`next_down` are `pub(crate)` there. They are covered by the property test and indirectly through `certified`.
- `math::det3` is `pub(crate)` there. It is covered through the determinants that use it.
- `Transform::from_matrix` is new, and unit-tested.

Pin paths that differ:
- `UNIT`, `gamma`, `NOZ` and `NOW` are in `solidcore::nurbs::math`;
- the small matrices are in `geom::geom_utility::{p1,p2}`.

## 4. Found, not changed (bit-identity wins in this step)

**Zone-size definitions.** Further zone-tolerance definitions remain at the pin outside the moved files. scx-num now has one, `consts::ZONE_DEFAULT = 1e-5`. The others are at:
- geom/tol.rs:209 and kernel.rs:70;
- nurbs/b_spline_surface_n_lib/p2.rs:131;
- topology/brep_data.rs:112, with APPROX at :114 and LARGE_SMALL at :118;
- surface/trim_srf_cache.rs:136;
- heal/gap.rs:90 (XSECT);
- surface/surface/p5.rs:131;
- curve/crv_on_surf.rs:178;
- topology/vertexuse.rs:143;
- DEGENERATE at surface/surface/p1.rs:229 and topology/brep/p7.rs:205;
- prism/tests.rs:40 (a test);
- mesh_reconstruction/topology.rs:64, which uses a different value, 5e-7.

These are replaced as their layers move.

**`EFF_ZERO`.** It is used about 157 times, for lengths and for ratios alike. `EPS_LEN` and `EPS_REL` both equal it for now, and splitting the uses is deferred to the layers that use them.

**Degree-valued items:**
- vector3d: `is_parallel_to`, `parallel_sense`, `is_perpendicular_to` and `deg_to_rad`;
- math: `rad_to_deg` and `EFF_ZERO_DEG`;
- tol: `ANG_TOL_DEG`, `are_parallel` and `are_perpendicular`;
- `Tolerances::ang_tol_deg`.

`AngleTol` is in radians. No conversion was made.

**FMA:** none of the moved files uses `mul_add`, so none was added.

**Sentinels** (`UNDEF_DOUBLE`, `Vec3::UNDEF`, `INFINITE_PARAMETER`) were kept as they are.

## 5. Conflicts with docs 07 and 09

- **07 §5:** `math::Transform` stays row-vector with translation in row 3, as at the pin, and its frames are copied. Changing to column vectors would change every operation's order.
- **09 `coincident`:** 09 defines it as res + acc1 + acc2. Today's code uses max(contact, z1, z2). The 09 form was not added, and the current one stays.
- **09, angle tolerances:** these stay in degrees, as section 4 lists.
- **09 §1.2:** the half-open rewrite of the zone table was not done. The ranges are documented, and a test pins today's answers at each boundary.

## 6. Interval-arithmetic plan (plan only)

The pin's interval code is in `blend/fillet_section/`:
- interval.rs: 607 lines, 6 tests; uses geom::certified::next_up;
- taylor_model.rs: 553 lines, 3 tests; uses geom;
- series.rs: 888 lines, 8 tests; uses error, geom and math;
- tay.rs: 590 lines, 5 tests; uses kernel and surface::procedural_blend.

About 70 files refer to them. interval.rs and taylor_model.rs depend only on layer 0, so they can join `scx-num`, or a sibling `scx-interval`, unchanged once the blend layer is ready. series.rs needs the error mapping. tay.rs belongs with the surfaces. Nothing in this plan was measured at the pin: who calls these functions and how hot they are.

## 7. Plan-document errors noticed

- The small matrices are not under `geom/` as single files. They live in `geom_utility/p1.rs` and `p2.rs`.
- `NOZ`, `NOW`, `UNIT` and `gamma` are re-exported from `nurbs::math`, not from `math`.
