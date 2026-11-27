# 72 - Step 4: the idiom hunt ahead of the vertical slice

Written for solidcorex, 2026-11-27. Step 4 of 01 §10.2 is the "vertical slice, end to end":

- `scx-mass`;
- `scx-bodyq` classification;
- `scx-make` primitives and extrude;
- `scx-boolean` on analytic bodies;
- `scx-tess`.

The crates these call (`scx-planar`, `scx-check`, `scx-stitch`, `scx-hlr`) hold only what the slice
calls. The step is done when the end-to-end difftest by outcome runs and the first performance
measurement of 01 §9.4 is recorded.

This note does for step 4 what research 63 did for steps 2 and 3. For each crate it gives:

1. the shapes found at the pin, with citations and counts;
2. the proposed shape;
3. a sketch of the public types;
4. the truth references `scx-testkit` lacks;
5. builder tasks with their dependencies.

It also settles, by analysis and an adversarial pass (§10), the questions the slice raises across
crates:

- the public signatures;
- how each operation declares lineage through `Changes`;
- which pin tests each crate ports;
- what the outcome difftest compares;
- the tessellation contract;
- which SSI pieces the analytic Boolean has and lacks;
- which performance techniques of 66 and 68 each crate adopts, and under what condition each stays
  correct.

§11 is the task graph. §12 lists the proposed 04 rows and the open decisions with recommendations.

- **Labels** (as in 57 and 63):
  - READ: read from the pin's code or the docs.
  - MEASURED: counted or run here; the method is given.
  - JUDGED: a design judgement.
- **Citations.**
  - Pin paths are relative to the pinned kernel's `src/` (crate `solidcore`, pin 03e16c9ee7), except
    `solidcore-tess/...` and `nurbs-grid-mesh/...`, which are relative to the pin's `crates/`.
  - scx paths are relative to the repository root.
  - `p2.rs:335` means line 335 of that file.
- **Scope.**
  - No crate code was changed.
  - Errors follow research 62 as ruled (01 §5, the 11-27 rows of 04). Every cause named below
    carries its kind: Refused, Unsupported, Defect, Exhausted or Cancelled.
  - The 04 rulings bind. Where this note proposes to change one, it says so and gives the case for
    and the case against (§12.2).

## 0. How the counts were made (MEASURED)

- **Which file belongs to which crate.** As in 63 §0:
  - every pin file was mapped to a crate by research 56's `assignment.py` (its `FILES` table,
    longest prefix first, file-level only; the symbol-level overrides were not applied);
  - files whose path contains `test` were then dropped.
- **What a count means.** The number of matching lines found by a regular expression over the
  remaining files, comment and doc lines included. A count is a size, not an exact number of
  occurrences.
- **The tessellator.** `assignment.py` maps only `query/tess` (the SmTess quadtree port) to
  `scx-tess`. The mesher the app uses is the separate crate `solidcore-tess`, with
  `nurbs-grid-mesh`: 15,377 non-test lines. These were counted with the same expressions and are
  shown as their own column.
- **The script** is a throwaway outside the repository. Per the rule, no counter is committed.

| Shape (pattern) | mass | bodyq | planar | make | boolean | check | stitch | hlr | `solidcore-tess` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| pin files | 4 | 60 | 28 | 61 | 34 | 16 | 9 | 13 | — |
| pin lines | 8,135 | 87,433 | 15,082 | 49,524 | 57,091 | 11,549 | 15,649 | 12,730 | 15,377 |
| Kernel read (`&Kernel`) | 60 | 749 | 12 | 117 | 341 | 65 | 33 | 44 | — |
| Kernel written (`&mut Kernel`) | 5 | 36 | 0 | 182 | 183 | 59 | 114 | 11 | — |
| Body written by a read or an operation (`&mut Brep`) | 28 | 159 | 0 | 101 | 186 | 74 | 141 | 11 | — |
| Mutate and report (`-> Result<()>`) | 25 | 244 | 26 | 247 | 252 | 18 | 92 | 17 | 5 |
| `Result<bool>` | 11 | 117 | 4 | 34 | 116 | 82 | 18 | 9 | — |
| Out-parameter (`: &mut Vec<`, `[`, `f64`, `bool`, point, `Option<`) | 16 | 45 | 12 | 106 | 71 | 2 | 13 | 19 | 31 |
| Optional out-parameter (`Option<&mut`) | 2 | 27 | 0 | 18 | 59 | 21 | 23 | 2 | 0 |
| Setter (`fn set_*(&mut self`) | 0 | 59 | 0 | 16 | 59 | 0 | 5 | 3 | 0 |
| Mode flags toggled on the body or kernel (`set_editing_enabled`, `set_doing_boolean`) | 0 | 8 | 0 | 65 | 47 | 0 | 15 | 2 | — |
| Behaviour-picking `bool` parameter | 14 | 309 | 40 | 174 | 228 | 52 | 65 | 34 | 27 |
| `unwrap_or` | 27 | 119 | 40 | 86 | 83 | 17 | 28 | 63 | 112 |
| Swallowed result (`.ok()`, `let _ =`) | 2 | 158 | 16 | 73 | 136 | 2 | 24 | 27 | 45 |
| Failing base default (`SM_ERR`, `base_err`) | 16 | 216 | 0 | 148 | 138 | 8 | 38 | 25 | — |
| Interior mutability (`RefCell<`, `Cell<`, `Mutex<`) | 0 | 9 | 2 | 2 | 8 | 0 | 5 | 1 | 8 (`Mutex`, `Atomic`) |
| Ambient state (`thread_local!`, `static mut`, `OnceLock`) | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 9 |
| Trait objects (`dyn`) | 2 | 83 | 1 | 27 | 45 | 15 | 9 | 22 | — |
| Literal tolerance (`1e-N`, `0.001`, `EFF_ZERO*`) | 122 | 879 | 177 | 577 | 617 | 72 | 58 | 235 | 4 (`0.05`) |
| Upstream class names (`` `Sm ``) | 40 | 911 | 1 | 425 | 638 | 133 | 216 | 100 | — |

A dash means the pattern was not counted for that column.

The step-4 slice touches a small part of these totals:

- primitives, linear extrusion of line-and-arc profiles, and the cellular Boolean on planes,
  cylinders, cones, spheres and tori;
- point classification;
- mass properties;
- faceting of analytic faces.

The table shows where each shape lives. As in 63, the slice meets most shapes because they sit on
the paths every body passes through: the kernel arena, the `&mut Brep` reads and the mode flags.

Other counts in this note come from the surveys behind it, all MEASURED by `grep -c` over the named
files. The pin test counts are `#[test]` lines per file.

## 1. Shapes common to the slice, and the rulings

### 1.1 What the pin does on every path of the slice (READ)

- **Reads that write.**
  - Point classification takes `&mut Brep`: `BrepQuery::point_3d_classify(&self, k, b: &mut Brep,
    point, point_zone_tol_3d, do_boundary_intersections)` at `query/brep.rs:1054`. The reason is
    that it creates missing uv trims and raises edge tolerances, as upstream does from `const`
    methods.
  - `BrepClassifier` (`query/brep.rs:1389`) exists only to clone the body on the first solve that
    would write.
  - Mass also takes `&mut Brep`: `brep_volume(k, b: &mut Brep, accuracy, &PropertyAttributes)` at
    `mass/integrator/properties.rs:2810`. It does so only to prepare the body. The `_prepared`
    variants read `&Brep`.
- **Mode flags.**
  - The Boolean toggles `set_editing_enabled` on both operands around each classification
    (`boolean/merge/p1.rs:458`, `classify_point_editing_disabled`).
  - Primitive creation sets `k.context.set_doing_boolean(true)`
    (`primitive/primitive_creation/p1/primitives.rs:1445`).
  - Edit mode is left on after an error in `create_cone`.
- **Out-parameters.**
  - The face-level classifiers fill a pre-built `&mut PointClassification` and return
    `Result<()>`: `query/face/p1.rs:1144` and `query/face/p2.rs:832`.
  - The Boolean threads `&mut Option<Brep>` slots: `manifold_boolean` at `boolean/merge/p4.rs:2257`
    takes 10 parameters and returns `Result<bool>`.
  - The Boolean's own stitch wrapper takes `faces_to_swap: Option<&mut Vec<FaceId>>` and
    `just_unify: Option<&mut bool>` (`boolean/merge/p3.rs:896`).
- **Failing defaults and swallowed errors.**
  - `classify_with_retries` (`topology/brep/p9.rs:2956`) retries at 100 times the tolerance, then
    forces `Unknown` or `Face`.
  - `face_point_classify` returns `Ok(Unknown)` when the classification failed
    (`topology/solver/p1.rs:2452`).
  - The Boolean's `ignored(...)` wrappers discard classification errors and continue
    (`boolean/merge/p4.rs`, near :1483).
  - The tessellator replaces an invalid chord with 0.05 mm (`solidcore-tess/src/tolerance.rs:67`).
- **Stitching what is known.**
  - Every multi-face primitive is built as loose faces and then sewn. `box_faces`
    (`primitives.rs:653`) and `conic_solid` (`:894`) call `stitch_solid` (`:190`), which calls
    `topology::brep::p5::stitch_and_orient_one_solid`.
  - A certified ray fire then finds the infinite region (`find_and_set_infinite_region`,
    `heal/stitch/p2.rs:1963` and `topology/brep/p5.rs:1678`). That ray fire is 34.5% of plate_12
    (68 §5).
- **Ambient state.**
  - The core crate has none on these paths: the classification survey found only `&'static str`.
  - `solidcore-tess` has a global rayon pool (`static POOL: OnceLock`, `par.rs:53`), a thread-local
    guard and `catch_unwind`.
- **Lineage by attribute.**
  - The Boolean has no structural map from result to source. Callers tag faces with attribute ids,
    and the ids propagate through copy, split, merge and delete notifications
    (`geom/attribute.rs`, `boolean/merge/p3.rs:190`, `:229`, `:271`).
  - Region lineage is `MergeRegionData` (`geom/attribute.rs:442`).

### 1.2 The rulings for the slice (JUDGED)

**S1. One signature per operation: operands that need cells are `&Brep`; writers that need none are
`&mut Draft`.**

The brief asks for "`&Brep` reads and `&mut Draft` writes, one signature per operation". 01 §7.1
says "Operations take `&Brep` and return a new `Brep`". This note reconciles the two by what the
operation reads:

- **A Boolean reads its operands' cells.** It needs face boxes, the body box tree
  (`Brep::faces_meeting`), face accuracy and `Face::bounding_box`. These exist only on finished
  bodies (scx-brep's `query.rs`: `impl Face<'_, Finished>`). On a `Draft` they are `Option`
  (BR1). So a Boolean takes `&Brep` operands, builds its result in `a.edit()`, and returns
  `Done`.
- **A constructor reads no body.** `block`, `cylinder` and `extrude` take a frame and sizes and
  return a new `Brep` with a typed record of its faces (S6). They are not writers into a caller's
  `Draft`. Adding a block to an existing body is a union, which is the Boolean's job, and a second
  path would be a second signature.
- **A whole-body edit with no cell reads is a `Draft` method.** BR6's `Draft::transform` is the
  precedent. No step-4 operation is of this kind.
- **Reads take `&Brep` and return owned values.** This covers mass, classification and
  tessellation. None writes, caches on the body, or needs `&mut`.

There is never a `&mut Brep`. The pin's 186 `&mut Brep` lines in the Boolean, 159 in classification
and 28 in mass all go.

**S2. `Cx` and the operation types live in `scx-brep`, module `op`.**

01 §6.3 defines `Cx { doc, settings, scratch, stop, trace }` and 01 §5.2 defines
`Done`/`Report`/`Skipped`. Neither says which crate holds them, and none of them exists on main
(MEASURED: only `Budget`, `Stop` and `Stopped` exist, in `scx-num/src/budget.rs`). `scx-bodyq`
(layer 5) takes `Cx`, so `Cx` must sit at layer 4 or below, and it holds a `Document`, which is
`scx-brep`'s.

- **Proposal.** Put `Cx`, `Settings`, `Scratch`, `Trace`, `Done`, `Report`, `Warning`, `Skipped`
  and `Raise` in `scx_brep::op`.
- **Case against.** It puts operation plumbing in the body crate. A separate `scx-op` crate at
  layer 4 above `scx-brep` would keep `scx-brep` about bodies.
- **Case for.** `Done` holds `Changes`, which is `scx-brep`'s. A new crate would hold some 200
  lines and add one more layer edge for every crate above. The types are plain data, with no
  algorithm.
- **Recommended.** `scx_brep::op`, with the change-if "if `op` grows past plain types and the
  `Scratch` map, split it out". This is an open decision (§12.3, D1).

**S3. Errors, by crate, with kinds.**

Each crate has one error type, with typed causes in the 62 shape. The kinds used by the slice:

| Kind | Means in the slice | Examples |
|---|---|---|
| Refused | the input is wrong; the caller can fix it | a zero or negative size, a non-finite frame, a profile loop that is not closed, crosses itself or nests wrongly, a horn or spindle torus, a chord at or below the resolution, an open shell asked for a volume |
| Unsupported | valid input this build cannot do yet, by name | an SSI pair answered `Closed::General` whose faces' boxes meet; an edge curve kind SU6 does not take (a parabola or hyperbola); a NURBS face in the mesher before step 5; an ellipse or spline profile segment |
| Defect | our bug, detected | `Draft::finish` refusing a result the operation built; a classification that disagrees with itself across two clean rays |
| Exhausted | the work budget ran out | the integrator past its step budget; a Boolean past its total |
| Cancelled | `Stop` was raised | polled at the budget's metering points |

- `Unsupported` causes name the pair, entity or kind ("`SsiPair { a: FaceId, b: FaceId, kinds }`"),
  never a string.
- No error type carries an SMLib status number (`MassFailure` at the pin carries
  `comparison_status`; `error/mass.rs:38`).
- A classification that cannot decide is not an error kind of its own. It is `Unsupported` with
  the directions tried (§3), so it is visible and counted.

**S4. Determinism.**

- No statics, thread-locals or interior mutability in library code (01 rule 4).
- Every set iterated in a result-affecting order is a dense id order, or is sorted by a stated key.
- Ray directions, sample parameters and seeds are fixed by the inputs: a constant table, or
  `Rng::new(mix(seed, op, entity))` from `Settings`.
- Every difftest case runs twice and compares bits (§8).

**S5. Tolerances.**

- Coincidence is `scx_num::units::coincident(res, acc₁, acc₂)`, the maximum (04 row "Coincidence is
  `max(...)`").
- No literal tolerance enters an algorithm. The 2,741 literal-tolerance lines of the table above
  become either `coincident(...)` or a named, derived bound.
- Accuracy asked of an integrator is an `Accuracy`, not a bare `f64`.

**S6. Constructors return a typed face record, not `Changes`.**

A new body has no inputs, so every entity in it is `Origin::Free` and `Changes` would say nothing.
What the feature layer needs is to know which face is the top cap or the wall of profile
segment 3. Each constructor returns its body with a record of its faces by role:

```rust
pub struct Made<R> { pub body: Brep, pub faces: R }
pub struct BlockFaces { pub bottom: FaceId, pub top: FaceId, pub sides: [FaceId; 4] }
pub struct ExtrudeFaces { pub start: FaceId, pub end: FaceId, pub walls: Vec<Vec<FaceId>> } // [loop][segment]
```

This replaces the pin's naming by notify environments (`primitives.rs`) with a value.

**S7. Primitives are built with their topology known.** This is 04's step-4 performance order,
item 1 (row "Two consequences of research 68"):

- no stitch, no orientation pass and no infinite-region ray fire;
- every region decided at construction (`Draft::add_region(solid)`);
- every pcurve exact from `pcurve_of`, with no recorded accuracy.

Correctness is checked by truth, not by matching the pin's entity order (§8).

**S8. Operations do not run the result contract.** This is 04's row on
`scx_check::contract`:

- the slice's tests prove every result;
- the kernel edge (`scx-features`, the app) applies the contract;
- operations end at `Draft::finish`, whose cheap checks run anyway.

## 2. `scx-mass`

### 2.1 Shapes found (READ, MEASURED)

- **Files.**
  - `mass/integrator/properties.rs`: 4,217 lines.
  - `mass/enclosure.rs`: 3,164 lines.
  - `mass/loop_area.rs`: 744 lines.
  - `error/mass.rs`: 366 lines.
  - The ODE core, `mass/integrator.rs`, is already `scx_algo::ode` (moved at the pin commit, with
    research 57's M6 shapes).
- **Entry points.**
  - `brep_volume(k: &Kernel, b: &mut Brep, accuracy: f64, &PropertyAttributes) -> Result<f64>`
    (`:2810`) and `brep_volume_default` (`:2835`).
  - `brep_area(k, b: &mut Brep, accuracy)` (`:2655`) and `brep_area_about` (`:2683`).
  - `face_area` (`:2568`, with a `_prepared` twin over `&Brep`), `face_centroid` (`:2605`) and
    `edge_length` (`:2549`).
  - `brep_mass_properties(k, b, accuracy, density, origin, attrs) -> MassProperties { properties,
    centroid, inertia: Vec3, products: Vec3 }` (`:2902`, `:2876`), and the 5-field request
    `brep_mass_properties_estimated` (`:3050`).
  - `PreciseProperties { area, volume, mass, moments: [Vec3; 8] }` (`:125`) and
    `MassPropertyBehavior` (`:145`).
- **Method.**
  - Boundary integrals over the trim edges, with the ODE integrator and Gauss-Kronrod
    (`KronrodRule`, `:1057`; `kronrod_integrate`, `:1085`).
  - The accuracy is requested, not proved, and no bound is returned.
  - Analytic faces are integrated through their NURBS form (`analytic_base_with_nurb(k)`,
    `:513-521`). There are no closed forms by kind.
- **Reference point.**
  - Integration is about the body box's midpoint (`bbox.mid()`, `:2594`, `:2634`, `:2822`).
  - So every face's contribution, and its subdivision depth, depends on the whole body. 66 §13.2
    found this to be what stops a per-face memo.
- **Gap closure.**
  - `gap_closure` (`:1926`) integrates the strips that close each trim onto its edge's curve.
  - It is 10.6% of gear_60 (68 §5).
- **Certified path.**
  - `enclose_volume(k, &mut Brep, width) -> VolumeEnclosure` (`enclosure.rs:259`),
    `enclose_mass_properties` (`:466`) and `volume_sign` (`:584`). The result contract's outward
    test uses these.
- **Loop area.** `loop_area_sign` (`loop_area.rs:271`) with `LoopAreaMemo`, a `RefCell` held in the
  kernel (`:227`).
- **Errors.** `MassFailure`, 16 variants (`error/mass.rs:38`).
- **Counts (MEASURED, over the four mass files).** The table in §0 gives 28 `&mut Brep`, 16
  out-parameters, 14 `bool` parameters and 122 literal tolerances. None of the 27 `unwrap_or`
  swallows an API error; they are numeric defaults inside a step.
- **Pin tests.**
  - Inline: 57 (properties 23, enclosure 16, integrator 11, loop_area 7). The integrator's 11 moved
    with `scx_algo::ode`.
  - Integration tests:

    | File | `#[test]` |
    |---|---:|
    | `issue_2972_volume_sign_enclosure` | 19 |
    | `issue_2091_precise_volume_spans` | 9, closed forms to 1e-9 … 1e-14 |
    | `edge_properties_hold_their_accuracy` | 5 |
    | `issue_16735_volume_moves_are_the_old_leak` | 5 |
    | `precise_area_holds_its_accuracy` | 1 |
    | `issue_383_tangent_torus_volume` | 1 |

### 2.2 Proposed shapes (JUDGED)

- **Pure reads over `&Brep`.**
  - Mass is layer 5 on `scx-brep`. With S2 it could take `Cx`, but it reads no settings or
    document. It takes what it reads (01 §6.3): the body, the accuracy, `&mut Scratch` for the memo,
    `&mut Budget` and `&Stop`.
  - The boolean (interference) and check (outward test) pass these from their own `Cx`.
- **One request, one result.**
  - `MassSpec { accuracy: Accuracy, density: Option<f64>, about: Option<Point3> }`, with
    `#[non_exhaustive]` and `with_*` methods. This replaces `PropertyAttributes`,
    `MassPropertyRequest`'s five fields and `MassPropertyBehavior`.
  - The result is `Mass { volume, area, centroid, inertia: Sym3 }`, each a value with the accuracy
    reached.
  - `manifold_solid_only` and `sheet_faces_area` become the body's own facts: a sheet has area and
    no volume, and a volume of an open shell is `Refused { OpenShell }`.
- **The face's own reference.**
  - Each face is integrated about a point fixed by the face (its uv box's image centre, rounded to
    a power-of-two grid so it is exact). Its error scale comes from the face alone.
  - It is then shifted to the body's reference using its memoized vector area and first moments,
    summed in face id order.
  - This makes the per-face memo exact (66 §13.2). It changes last bits against the pin, so it is
    truth-checked against closed-form volumes once (row "Truth over pin").
- **Gap closure only where there is a gap.**
  - A trim whose pcurve is exact (no recorded accuracy on the use or its edge, per BR1) maps onto
    its edge's curve exactly, so the strip has zero width. The strip is integrated only for uses
    with a recorded accuracy.
  - Every step-4 body is exact by S7 and by the Boolean's exact pcurves (§5), so gear_60 and
    plate_12 pay no gap closure.
- **The certified sign.**
  - `volume_sign(body, region) -> Result<Sign, MassError>` is the port of `enclosure.rs`'s
    `volume_sign`, over `&Brep`.
  - It is the outward test of `scx_check::contract` (04 row 101's edge `scx-check` → `scx-mass`).
- **No memo inside the kernel.**
  - `LoopAreaMemo` (a `RefCell`) becomes an entry in the caller's `Scratch`, keyed by the loop's
    pcurve bits.
- **Work.** Each face integral records its work in budget units. A memo hit charges the recorded
  work (04 row on the work budget).

### 2.3 Sketch

```rust
// scx-mass
#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub struct MassSpec { pub accuracy: Accuracy, pub density: Option<f64>, pub about: Option<Point3> }

#[derive(Clone, Debug, PartialEq)]
pub struct Mass { pub volume: f64, pub area: f64, pub centroid: Point3, pub inertia: Sym3, pub reached: Accuracy }

pub fn mass(body: &Brep, spec: &MassSpec, scratch: &mut Scratch, budget: &mut Budget, stop: &Stop)
    -> Result<Mass, MassError>;
pub fn volume(body: &Brep, accuracy: Accuracy, scratch: &mut Scratch, budget: &mut Budget, stop: &Stop)
    -> Result<f64, MassError>;
pub fn area(body: &Brep, accuracy: Accuracy, scratch: &mut Scratch, budget: &mut Budget, stop: &Stop)
    -> Result<f64, MassError>;
pub fn face_area(face: Face<'_>, accuracy: Accuracy, scratch: &mut Scratch, budget: &mut Budget, stop: &Stop)
    -> Result<f64, MassError>;
pub fn volume_sign(body: &Brep, region: RegionId, budget: &mut Budget, stop: &Stop)
    -> Result<Sign, MassError>;

#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub enum MassError {
    Refused(MassRefusal),          // OpenShell { shell }, BadAccuracy { value }
    Unsupported(MassUnsupported),  // none expected in step 4; kept for kinds added later
    Exhausted { work: u64 },
    Cancelled,
    Defect(MassDefect),            // StepControl { face, reason } (from SolveError), SignUndecided { region }
}
```

### 2.4 Truth references missing

- **Closed forms for the slice's recipes** (testkit task T9). Volume and area of:
  - the five primitives;
  - a spherical cap;
  - half a torus;
  - a box minus n through-cylinders (plate_12: 144 holes, 100000 − 5760π; see §14.9);
  - gear_60, whose cross-section is a disk less 60 clipped rectangles. Its area is
    `π·50² − 60·∫_{−g/2}^{g/2} (√(2500 − y²) − 45) dy`, closed form, times h = 10. The rectangles
    are apart, since g ≈ 2.12 against a pitch of 4.71 at r = 45.
  - These go in `scx_testkit` as exact formulas evaluated with a stated error.
- **Moments for the primitives at a rotated frame**, from the principal moments.
- The pin's `issue_2091` closed forms (1e-9 to 1e-14) are reused as they are.

### 2.5 Builder tasks

| Task | What | Depends on |
|---|---|---|
| MA1 | `scx-mass` crate: `MassSpec`, `Mass`, `MassError`; face boundary integrals by the pin's method (Gauss-Kronrod with `scx_algo::ode`) over `&Brep`, about the face's own reference with the shift; `volume`, `area`, `mass`, `face_area`, `edge_length`; ports `issue_2091` (9), `edge_properties_hold_their_accuracy` (5), `precise_area_holds_its_accuracy` (1), `issue_383` (1), `issue_16735_volume_moves` (5) and properties' 23 inline tests where they test behaviour, not pin internals | CX1, T9 |
| MA2 | gap closure only for uses with a recorded accuracy; test with a `broken`-style body whose trims carry an accuracy | MA1 |
| MA5 | per-face memo in `Scratch`, keyed by the face record's bits, its sense and the accuracy; the work rule; a test that memo and recompute are bit-identical across a Boolean that keeps faces | MA1, BO4 |
| MA4 | `volume_sign` from `enclosure.rs` over `&Brep`; ports `issue_2972` (19) and enclosure's 16 inline tests | MA1 |

## 3. `scx-bodyq`: classification

### 3.1 Shapes found (READ, MEASURED)

- **No "is this point inside" read.**
  - A point is classified into a `Region`, and "inside" means `region.is_void() == 0`.
  - The result is `PointClassification<O>` (`intersect/point_class.rs:509`). It is a stateful
    struct with setters (`set_class_object`, `set_gap_3d`, `set_deviation`, …) over the class enum
    `PointClassType`. That enum has upstream's discriminants: `Unknown = 100`, `Region`, `Face`,
    `Edge`, `Vertex`, … (`:282`).
- **The body method: one ray, then a local vote.**
  - Fixed directions: `CLASSIFY_RAYS` at `query/brep.rs:529`, ±x, ±y, ±z in that order.
  - Each ray is 2 times the box sphere's radius long. The closest hit wins, then the lowest
    dimension.
  - A hit nearer than `EFF_ZERO·(1 + |start|)` is `HitAtRayStart`, an error that moves on to the
    next direction.
  - The region is found by `local_curve_sector` (`topology/brep/p2.rs:1826`). It steps back
    `10·tol` from the hit and votes by face, edge-radial or vertex sectors.
  - The ray is intersected by the general topology solver (`ray_fire_with_bbox_and_solver`,
    `topology/brep/p2.rs:2156`), not by per-kind code.
- **Point-in-face.**
  - The uv test (`query/face/p2.rs:832`, `point_classify`) is a quadrant-count winding walk over
    each loop's uv trims (`query/loop.rs:332`, `contains_uv_point`).
  - It creates missing uv trims on the fly and attaches them to the body. This is the write behind
    `&mut Brep`.
  - Its `tol_3d` argument is unused.
- **The Boolean's use.** The Boolean classifies the other body's faces mostly by propagation
  across common edges (`boolean/merge/p4.rs:1128`, `classify_edge_faces`). Only faces it does not
  reach get a point, taken at edge parameter 0.45678 (0.456789 on the reverse pass,
  `p4.rs:1480`), classified against the other body (`classify_remaining`, `p4.rs:1397`).
- **Failing defaults.** Listed in §1.1: retries at 100 times the tolerance, `Ok(Unknown)` on
  failure, swallowed errors.
- **Counts (MEASURED, the classification core).**

  | File | Lines | `&mut ` lines | `fn`s taking `&mut` | Swallowed results | `#[test]` |
  |---|---:|---:|---:|---:|---:|
  | `query/brep.rs` | 2,470 | 90 | 11 | 9 | 18 |
  | `query/brep/ray.rs` | 1,079 | 62 | 1 | 0 | 11 |
  | `query/brep/ray/live.rs` | 1,200 | 32 | 6 | 4 | 0 |
  | `query/loop.rs` | 2,384 | 44 | 2 | 23 | 22 |
  | `query/face/p1.rs` | 1,782 | 29 | 3 | 12 | 6 |
  | `query/face/p2.rs` | 3,609 | 153 | 23 | 26 | 20 |
  | `intersect/point_class.rs` | 3,217 | 56 | 16 | 21 | 13 |

  That is about 15.7k lines before the topology solver.
- **Callers.** `point_3d_classify` has 139 lines across the source (25 in `boolean/`, 43 in
  `query/`). `BrepClassifier` has 17, all in `query/`.

### 3.2 Proposed shapes (JUDGED)

- **One read: `classify_point(body: &Brep, p, cx) -> Result<PointClass, BodyqError>`.**
  - `PointClass` is `In(RegionId)` or `On(Entity)`, where the entity is a face, edge or vertex.
  - The outside is `In(outer)`, where `outer` is the body's infinite region. Callers who want
    material ask `body.region(r).is_solid()`.
  - No `Unknown` variant: a point that cannot be decided is an error with a cause.
- **The method on step-4 bodies (JUDGED; replaces the topology solver).**
  1. **On test.** Faces whose box grown by coincidence holds `p` are found with
     `faces_meeting`. `p` is projected onto each by the closed-form projection of step 2. A
     projection within coincidence whose uv is `Inside` or `Near` the face (`Face::contains_uv`)
     gives `On`.
     - Edges and vertices within coincidence refine `On(Face)` to `On(Edge)` or `On(Vertex)`.
     - The lowest dimension wins; ties go to the lowest id.
  2. **Nearest clean hit.** Rays come from a fixed table of 13 directions: the six axes, then the
     seven body diagonals and half-diagonals of the table. Each ray is a `Line` to the body box's
     exit.
     - It is intersected with each face's surface by SU6 (`intersect_curve`, 04 row on SU6),
       which takes a line against all five analytic kinds and returns hits with `Contact`. Only
       faces whose box the ray's slab test passes are tried.
     - The nearest hit decides if it is *clean*: `Transversal`, with uv `Inside` the face, not
       `Near`, and farther than coincidence from `p`.
     - The point then lies in the region on the side of that face facing the ray's start: the
       face's side, then its shell, then the shell's region.
     - No hit at all gives the outer region.
  3. **Fallback.** An unclean nearest hit (tangent, `Near` a boundary, an edge or a vertex) moves
     to the next direction. If all 13 are unclean, the read returns
     `Unsupported(NoClearRay { tried: 13 })`.
- **Why the nearest hit and not parity.**
  - Parity counts every crossing, so one miscounted tangency flips the answer.
  - The nearest clean hit needs one decision. It also gives the region directly, which the pin
    gets by a sector vote.
  - This is the pin's design (closest hit, then the side) without the vote, which the clean-hit
    rule makes unnecessary.
- **Certainty.**
  - "Clean" uses `contains_uv`'s proved answers (BR4's boundary tree: `Near` when the enclosures
    cannot tell) and SU6's tangency rule. A clean answer is therefore never a guess.
  - The 13-direction table makes `NoClearRay` reachable only for points within coincidence of
    some boundary along all 13 directions. Such points are caught by step 1 unless they lie in the
    tolerance band of a non-convex corner.
- **No writes, no clone, no editing flag.** Exact pcurves exist on every face by construction.
- **Cells.** Ray hits on a face read `Face::contains_uv`, whose boundary tree is filled on first
  read (BR4). That is the per-face classifier cache of 66 rank 2, already in the body.
- **`BodyqError`** kinds:
  - `Unsupported` (`NoClearRay`; a NURBS face before step 5);
  - `Exhausted`;
  - `Cancelled`;
  - `Defect` (a face without a box on a finished body).

### 3.3 Sketch

```rust
// scx-bodyq
#[non_exhaustive] #[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointClass { In(RegionId), On(Entity) }

pub fn classify_point(body: &Brep, p: Point3, cx: &mut Cx<'_>) -> Result<PointClass, BodyqError>;

/// The fixed direction table; a constant, documented, never reordered without a difftest.
pub const RAYS: [Vec3; 13] = [/* +x, -x, +y, -y, +z, -z, then seven diagonals */];

#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub enum BodyqError {
    Unsupported(BodyqUnsupported),  // NoClearRay { point, tried }, FaceKind { face, kind }
    Exhausted { work: u64 },
    Cancelled,
    Defect(BodyqDefect),            // NoFaceBox { face }
}
```

### 3.4 Truth references missing

- **A CSG membership oracle** (testkit task T10), `scx_testkit::csg`:
  - implicit primitives (half-space, slab, box, cylinder, cone, sphere, torus) at a `Frame`;
  - union, difference and intersection of them;
  - a predicate `side(p) -> {In, Out, Band}`, where `Band` means within a stated distance of the
    truth boundary, decided with interval arithmetic so `In` and `Out` are certain.
  - This is the truth for classification and for the Boolean's sample points (§8). It is
    independent of both kernels.
- **Points on purpose** (T10): points on each face, edge and vertex of a recipe, and points at
  ±k·coincidence along each face normal for k in {½, 2, 10}.

### 3.5 Builder tasks

| Task | What | Depends on |
|---|---|---|
| BQ1 | `scx-bodyq` crate, `classify_point` as §3.2 with the On test and the 13-ray nearest clean hit; ports, as outcomes against T10: `query/brep.rs`'s interior and exterior test (`notify_named_primitive_box_builds_nonempty_trees_and_classifies_interior`, `:2439`), its ray-hit tests (`:1847-2275`: seam, pole, edge and corner hit once, grazing, nearest first), and `brep_query_cache_lifetime`'s fixed point set (the in-material booleans only; the editing-state tests have no counterpart, and the PR says so) | CX1, T10, MK1, MK2 |
| BQ2 | the side of a face along a curve: given an intersection edge on face `f` and the other body's face, which side of `f` the other body's material is on, from the two surfaces' normals at a point of the edge (with the tangent rule of SS4 where they are parallel). Used by the Boolean's seeds (BO4) | BQ1, BO2 |

## 4. `scx-planar` (the slice's part) and `scx-make`

### 4.1 Shapes found (READ, MEASURED)

- **Primitives.**
  - In `primitive/api_primitives.rs`, all are `(k: &mut Kernel, …) -> Result<Brep>`,
    axis-aligned, with no frame:
    - `create_box(k, origin, l, w, h)` at `:238`;
    - `create_sphere` at `:263`;
    - `create_cylinder` at `:276`;
    - `create_cone(k, origin, r_base, r_top, h)` at `:327`;
    - `create_torus` at `:380`;
    - `create_pyramid` at `:429`;
    - partial variants from `:603` to `:933`.
  - Placement happens afterwards by `topology::brep::p5::transform`, which is how gear_60 rotates
    its boxes.
- **Builders** (in `primitive/primitive_creation/p1/primitives.rs`, 2,343 lines).
  - `box_faces` (`:653`) and `conic_solid` (`:894`) build loose faces, then stitch them (§1.1).
  - `create_sphere` (`:950`) and `create_torus` (`:971`) build one face with no stitch.
  - A cone cap is skipped when its radius is within the tolerance (the apex).
- **Validation.**
  - Zero and negative sizes are `InvalidInput`.
  - A torus with minor ≥ major is refused (`torus_tube_clears_axis`, `:119`).
  - Partial angles are not validated.
  - NaN passes the pyramid check.
  - `unitize_or_z` is a failing default, and `let _ = frame.set_canonical` (`:162`) ignores a
    status.
- **Extrude.**
  - `sweep/extrude.rs:58`: `extrude(k, curves: &[Curve], direction, distance, draft:
    Option<Draft { angle_deg, corner }>, cap: bool, &ExtrudeTolerances { loop_gap, parallel }) ->
    Result<TaperExtrude { Solid(Brep), Refused(CornerRefusal) }>`. A refusal is a value inside
    `Ok`.
  - Without a draft, it calls `create_linear_sweep` (`sweep/api_brep.rs:205`, then
    `primitives.rs:1431`). That makes one face per segment (a plane, a cylinder for a circle, or a
    `SurfOfExtrusion`) and planar caps through `topology::brep::p8::create_planar_face_with_3d_curves`.
  - The prism path (`prism/extrude.rs:37`, `extrude_region(k, &Frame, &Region2, lo, hi)`) uses
    `curve_on_frame` (`prism/profile.rs:1025`).
- **Profiles** (`prism/profile.rs`):
  - `Seg2` (`:161`);
  - `Loop2` (`:215`), whose `holds` (`:781`) is even-odd and returns `Option<bool>`;
  - `Region2` (`:222`);
  - `check_regions` (`:870`).
  - Orientation is forced in `prism/sheet.rs:69-81`.
- **Counts (MEASURED).**
  - make: 182 `&mut Kernel`, 101 `&mut Brep`, 106 out-parameters, 65 mode-flag lines, 148
    failing-default lines.
  - planar: 0 `&mut Brep` and 0 setters. Its shapes are `Option<bool>` answers, 40 `unwrap_or`
    and 177 literal tolerances.
- **Pin tests.**
  - `tests/primitives_solid.rs`: 7. Closed manifold solids, analytic properties, scaling,
    partial spheres and tori, boxes far from the origin, small boxes at 1e4.
  - `api_primitives.rs` inline: 13.
  - `tests/primitive_infinite_region_searches.rs`: 5.
  - `sweep/extrude.rs`: 6. For example `a_profile_with_a_short_cubic_extrudes_to_a_valid_solid`;
    the tests call `check_brep` Level2 at `:261-345`.
  - `prism/tests.rs`: 73.

### 4.2 Proposed shapes (JUDGED)

- **Every constructor takes a `Frame`** (`scx_num::frame::Frame`, P1, right-handed by
  construction). A rotated box is built rotated, with no transform afterwards.
  - Building at the frame rounds once, where build-then-transform rounds twice. So gear_60's
    boxes differ from the pin's in last bits, and are truth-checked (T9).
- **Validated sizes, by type.**
  - Each size is a `Length` that must be finite and greater than the document's resolution.
    Otherwise the result is `Refused { size, value }`. NaN is refused.
  - A torus needs `minor < major`:
    - a horn torus (`minor == major`) is `Unsupported { HornTorus }`, because its apex is a
      singular point that BR2's pole rules do not cover yet;
    - a spindle torus (`minor > major`) is `Refused { SelfIntersecting }`.
  - A cone with one radius zero has an apex. A cone with both radii zero is refused.
- **Known topology (S7).**
  - The block, cylinder, cone, sphere and torus are written face by face into one `Draft`:
    - the shared edges are made once;
    - each face's loops are given in the orientation its side needs;
    - the solid region and the outer region are added first.
  - Each body passes `Draft::finish`'s checks and the T-series `violations` oracle (BR8). The
    `t_bodies` difftest already compares BR8's generators with the pin's primitives, and these
    constructors replace the generators' role for production use.
- **Partial primitives** (angle ranges) are not in step 4. They are `Unsupported` by name until a
  caller needs them. Their angles will be validated when they come.
- **`scx-planar`, slice part: `Region` from loops of lines and circular arcs.**
  - `Region::new(outer: Loop, holes: Vec<Loop>, res) -> Result<Region, PlanarError>` checks:
    - each loop is closed within the resolution;
    - no two segments cross or touch away from their shared ends;
    - each hole lies inside the outer loop and outside every other hole.
  - It orients the outer loop counter-clockwise and the holes clockwise by signed area, rather than
    refusing a reversed loop. Orientation is not the caller's error.
  - The crossing tests are closed-form line-line, line-circle and circle-circle intersections in
    2-D, with certified sign tests. `scx-curve` has no 2-D curve-curve intersection yet (MEASURED:
    no `intersect` in `scx-curve/src`).
  - Containment uses a winding number over exact segments, never even-odd with `Option<bool>`.
  - Ellipse and NURBS segments are `Unsupported { segment }` until step 5.
- **`extrude(region: &Region, frame: &Frame, span: Span, doc) -> Result<Made<ExtrudeFaces>,
  MakeError>`.**
  - `span` is a pair of heights, finite and distinct.
  - Each line segment gives a plane wall and each arc a cylinder wall. The caps are planes.
  - Every pcurve is exact: `Line2` on planes, and `AngleLine2` for the arcs on their cylinders
    (04's exact-pcurve rows).
  - A draft angle is taper's (`scx-taper`, step 8), not an option here, so there is no `cap: bool`
    and no refusal hidden inside `Ok`.
- **`MakeError`** kinds:
  - `Refused`: size, frame, profile, the torus kinds above;
  - `Unsupported`: horn torus, partial primitives, profile segment kind;
  - `Defect`: a `finish` refusal, carrying the `Unfinished` report.

### 4.3 Sketch

```rust
// scx-planar (slice part)
#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub enum Segment { Line(Line2), Arc(Circle2, Extent1d) }
pub struct Loop { segments: Vec<Segment> }                 // closed, by construction
pub struct Region { outer: Loop, holes: Vec<Loop> }        // oriented, nested, by construction
impl Region { pub fn new(outer: Vec<Segment>, holes: Vec<Vec<Segment>>, res: Resolution) -> Result<Self, PlanarError>; }

// scx-make
pub fn block(frame: &Frame, size: [Length; 3], doc: &Document) -> Result<Made<BlockFaces>, MakeError>;
pub fn cylinder(frame: &Frame, radius: Length, height: Length, doc: &Document) -> Result<Made<CylinderFaces>, MakeError>;
pub fn cone(frame: &Frame, radii: [Length; 2], height: Length, doc: &Document) -> Result<Made<ConeFaces>, MakeError>; // a radius may be zero: apex
pub fn sphere(centre: Point3, radius: Length, doc: &Document) -> Result<Made<SphereFaces>, MakeError>;
pub fn torus(frame: &Frame, major: Length, minor: Length, doc: &Document) -> Result<Made<TorusFaces>, MakeError>;
pub fn extrude(region: &Region, frame: &Frame, span: Span, doc: &Document) -> Result<Made<ExtrudeFaces>, MakeError>;
```

The cone's sizes are two `Length`s with a zero allowed for one radius, the exception the type
documents.

### 4.4 Truth references missing

- T9's closed-form volumes, areas and moments, for primitives at arbitrary frames.
- T10's membership oracle, which covers the primitives and extrusions of line-and-arc profiles.
- **Profile fixtures** (T11): closed, open-by-a-hair, self-touching, nested and reversed loops of
  lines and arcs, each with its expected `Region::new` answer.

### 4.5 Builder tasks

| Task | What | Depends on |
|---|---|---|
| PL1 | `scx-planar` crate with `Segment`, `Loop`, `Region::new` and the 2-D line and arc crossings; ports the `prism/tests.rs` cases on line-and-arc regions (closure, nesting, orientation), as outcomes against T11 | T11 |
| MK1 | `scx-make` crate, `block` with known topology at a `Frame`, `Made`, `BlockFaces`, `MakeError`; ports `primitives_solid`'s box tests (`boxes_far_from_the_origin_build_closed_and_valid`, `small_boxes_at_1e4_answer_as_at_the_origin`) and the size-refusal tests of `api_primitives` | CX1, T9 |
| MK2 | `cylinder`, `cone` (with apex), `sphere`, `torus` with known topology; ports `complete_spheres_and_tori_have_analytic_properties`, `primitives_are_closed_solids_with_analytic_properties`, `precise_sphere_properties_scale_and_translate`; `primitive_infinite_region_searches`' 5 become one test that each region is the one built, and the PR names the removed tests and why | MK1, MA1 |
| MK3 | `extrude` of a `Region` with exact pcurves; ports `sweep/extrude.rs`'s line-and-arc cases (the short-cubic case waits for step 5 and is listed as deferred) | PL1, MK1 |

## 5. `scx-boolean` on analytic bodies

### 5.1 Shapes found (READ, MEASURED)

- **Entry points** (`boolean/api_brep.rs`):
  - `boolean(k: &mut Kernel, brep1: Brep, brep2: Brep, op) -> Result<BooleanOutcome>` at `:687`;
  - `boolean_union`, `boolean_difference`, `boolean_intersection` and `boolean_merge` at
    `:702-:747`;
  - variants with options, curves, lists and trees at `:785-:979`.
  - Both operands are consumed. `brep1` is edited in place and returned. `brep2` comes back only
    when kept.
  - `BooleanOperation` (`geom/a_object.rs:120`) has 10 `#[repr(i32)]` variants with upstream's
    discriminants.
  - `MergeOptions` (`boolean/merge/p1.rs:105`) has five public `bool` fields.
- **Pipeline.**
  1. Self-touch gluing.
  2. `manifold_boolean` (`merge/p4.rs:2257`) with a temporary origin shift.
  3. `intersect_insert_relate` (`topology_intersector/p2.rs:1386`):
     - a capture distance (default 0.01, clamped);
     - edge splits at near-miss vertices;
     - face pairs from trim-tight boxes (`query/face/p1.rs:696`, `proved_bounding_box`), tested
       O(F₁·F₂);
     - per pair, the surface-surface dispatch (`surface/surface/p2.rs:4219`).
  4. Imprint (`p1.rs:2478`), coincident faces (`merge/p1.rs`, `topology_intersector/p3.rs:1719`).
  5. Classification by propagation across common edges, with the point fallback of §3.1.
  6. Selection, the copy of kept faces of the other operand, and deletions.
  7. `stitch_and_orient` with tolerance −0.1 (`merge/p3.rs:1008`, `:1169`, `:1258`).
  8. Post-processing: vertex removal and coplanar merge.
- **Tolerances changed by the Boolean.**
  - The ops layer's `matched_tolerances` (`boolean/operand_tolerances.rs:57`) runs `check_brep`
    Level2 on a copy (`:71`) and lowers the coarser operand's tolerance.
  - `update_tolerances_after_boolean` (`:108`) mutates the result.
  - Retry loops are fixed at 10 attempts (`p4.rs:1030`).
- **The pin's own SSI dispatch on analytic pairs.** It covers:
  - plane with every kind;
  - cone or cylinder with cone or cylinder (parallel axes included);
  - cone with sphere, sphere with sphere;
  - torus with sphere or cone only when the torus has a planar generator.
  - Torus-torus, torus-cylinder and sphere-torus go to the general marcher.
  - The dispatch keys on the first surface, so it is asymmetric.
- **Counts (MEASURED).**
  - boolean: 183 `&mut Kernel`, 186 `&mut Brep`, 59 setters, 59 optional out-parameters, 47
    mode-flag lines, 136 swallowed results.
  - `boolean/` alone is 33,434 lines, with the intersectors at
    `topology_intersector/p1.rs` (6,650) and `p2.rs` (3,589).
- **Pin tests.**
  - `boolean_api.rs`: 29. Its `cases()` table checks `integrity::check`, `is_manifold_solid`, exact
    face counts against SMLib's, and `brep_volume` within 1e-6 relative of the analytic value.
  - `boolean_coplanar_merge.rs`: 4.
  - `issue_1462_boolean_attributes.rs`: 7, one origin per face.
  - `issue_1462_boolean_family_attributes.rs`: 7.
  - `boolean_no_dangling_vertex_refs.rs`: 1.
  - 134 inline tests, mostly helper-level.

### 5.2 What the analytic SSI on main answers, and the gaps (READ)

`scx_ssi::analytic::intersect(a, ae, b, be, acc, res) -> Closed<Intersection>`
(`crates/scx-ssi/src/analytic/mod.rs:199`) puts the plane second. It answers in closed form:

| Pair | Answer | `General` (no closed form here) |
|---|---|---|
| plane, plane | line, coincident, empty | never |
| plane, cylinder | ellipse, circle, two lines, one tangent line, empty | an ellipse too long for its precision |
| plane, cone | ellipse, circle, parabola, hyperbola branch, two generators, one tangent generator, apex, empty | a conic too long and not within tolerance of a parabola |
| plane, sphere | circle, touch point, empty | never |
| plane, torus | one or two circles across the axis, two meridian circles along it, empty | **every other plane: the spiric sections** |
| two spheres | circles about the centres' line, touch, coincident, empty | tilted coincident spheres |
| coaxial cylinder, cone, sphere, torus | circles about the axis, coincident, empty | a gap between the tolerance and the tolerance widened by the axes' offset |
| **any other curved pair** | — | **always (SS2 not built)**: cylinder-cylinder off axis, cylinder-sphere off axis, cone-anything off axis, torus-anything off axis |

- Coincident pieces are `Coincident(Vec<Overlap>)` (SS4). `Empty` is final (04 rows on SS1/SS5 and
  the 68 consequences).
- No pcurves come out. The caller uses `pcurve_of`, whose exact forms are `Line2` on planes,
  `AngleLine2` and `AngleWave2` (04 rows on exact pcurves).
- A cone's conic off a plane is `NoClosedForm`.

**What this decides for step 4.** The slice's Boolean covers:

- planar faces against anything;
- spheres against spheres;
- coaxial curved pairs.

Two cases need care:

- **A plane against a torus off its axis** is a gap whenever the faces' boxes meet.
- **A pair the box test separates is never sent to SSI.** So a torus with a far box face is fine.

Everything else is `Unsupported(SsiPair { a, b, kinds })`, by name, and never a grid fallback.

**The pin's `boolean_api` cases against this** (READ from `tests/boolean_api.rs:50-162`):

| Case | Pairs that meet | Step 4 |
|---|---|---|
| box-cyl-hole | planes; plane-cylinder (circles, lines) | in |
| box-box-union | planes | in |
| sphere-and-box | plane-sphere | in |
| coplanar-union | planes, coincident | in |
| tangent-sphere-box | plane-sphere touch point | in, point contact (BO7) |
| seam-sphere-box, seam-box-sphere, off-seam-sphere-box | plane-sphere touch | in, point contact (BO7) |
| seam-sphere-sphere | two spheres, touch | in, point contact (BO7) |
| seam-box-minus-sphere | six plane-sphere touches | in, point contact (BO7) |
| torus-minus-box | plane x = 0 through the axis (meridians); other faces apart | in |
| tpipe | perpendicular cylinders | `Unsupported` (SS2) |
| seam-cylinder-sphere, off-seam-cylinder-sphere | cylinder-sphere off axis | `Unsupported` (SS2) |
| seam-torus-sphere | torus-sphere off axis | `Unsupported` (SS2) |
| seam-torus-box, seam-vertex-torus-box | plane parallel to the torus axis, touching the outer equator (spiric) | `Unsupported` (spiric section) |

- (Counted again by T12, §14.9: the table names 17 cases, not 15: 5 are built, 6 are `Unsupported(NonManifold)` by D5 and 6 are `Unsupported(SsiPair)`.) So 10 of the 15 named cases are in step 4, and 5 are expected `Unsupported` until step 5. The
  expected-`Unsupported` tests are ported as such, so step 5 sees them flip.
- Both performance cases are in:
  - gear_60's rotated box planes are parallel to the cylinder's axis (lines) or square to it
    (circles);
  - plate_12's cylinders are axis-aligned with the plate.

### 5.3 Other pieces the analytic Boolean needs (READ main, JUDGED)

- **Edge against face.**
  - SU6's `intersect_curve` takes a line, circle or ellipse against the five analytic kinds.
  - Step-4 bodies also carry parabola and hyperbola edges after a plane cuts a cone parallel to a
    generator or steeper. SU6 refuses those kinds. Such an edge is `Unsupported(EdgeKind)` until SU6
    grows them.
  - The gap is narrow: no in-scope case makes one.
- **2-D curve-curve intersection** is not needed (§5.4). The lower-dimension-first order finds
  every crossing in 3-D.
- **Merged points** follow 04's row: components within coincidence, fixed before any merge, an
  operand vertex being the merged point, and `Refused` for two operand vertices within coincidence.

### 5.4 Proposed shapes (JUDGED)

- **Signature (S1).** `boolean(a: &Brep, b: &Brep, op: BooleanOp, cx: &mut Cx<'_>) ->
  Result<Done<NoItems>, BooleanError>`.
  - `BooleanOp` is `Union`, `Subtract` or `Intersect`, a plain enum with no discriminant values.
  - `NoItems` is uninhabited: Booleans have no items (01 §5.2).
  - Operands are borrowed, never consumed. The result is built in `a.edit()`, so `a` is operand 0
    and `b` is operand 1.
  - No options in step 4. Imprint, merge and the n-ary forms are step 7 entries with their own
    signatures, not flags.
- **No tolerance rewriting.** There is no `matched_tolerances` and no
  `update_tolerances_after_boolean`. Coincidence is `max(res, acc₁, acc₂)` from the faces'
  recorded accuracies (BR1), and the result records a `Measured` accuracy only where a merge moved
  a point (04's merged-points row).
- **The pipeline, lower dimension first** (66 §11.1, OCCT's order):
  1. **Pairs.**
     - Each face of `a` against `b.faces_meeting(face_box)`, using the box tree of the finished
       operand, in `a`'s face id order, then `b`'s.
     - The per-pair prefilter is the box test only, with no per-call allocation (68's rank 4).
  2. **Vertex and edge interferences.**
     - Each edge of one operand against each face of the other in a meeting pair: SU6, then
       `contains_uv` on the hit's uv.
     - The result is the points where boundaries cross faces.
     - Vertex against vertex and vertex against edge within coincidence come first. They give the
       merged points.
  3. **Face against face.**
     - `analytic::intersect` gives curve pieces. Each is cut at the step-2 points that lie on it
       (by the curve's inversion).
     - Each sub-span is kept if its midpoint is `Inside` both faces. A `Near` midpoint is refined
       by halving, at most a fixed number of times, then `Defect` if it is still `Near`.
     - `Coincident` gives an overlap, handled by the same-or-opposite normal rule.
     - `General` with meeting boxes gives `Unsupported`.
  4. **Imprint.**
     - Intersection edges are made once, with exact pcurves on both faces from `pcurve_of`.
     - Faces of `a` are split in the draft with `edit_face`.
     - The kept faces of `b` are copied in, sharing the same new edges. So the result is connected
       by construction, and there is no stitch and no −0.1 tolerance.
  5. **Classify face uses** (66 §10.2, the pin's shape, kept):
     - components of face uses are split at every intersection edge;
     - seeds come from BQ2's side test at an intersection edge;
     - flood across uncut edges;
     - `classify_point` against the other operand only for components touching no intersection
       edge, at a point fixed by the inputs (the face's first edge at its parameter midpoint);
     - coincident faces decided by normals;
     - no per-face reclassification.
  6. **Select and finish.**
     - Keep or remove by `op`.
     - Merge coplanar neighbours that share a surface record and a side (`merge_face`).
     - Remove vertices of degree 2 between edges on one curve.
     - Set regions.
     - `finish`.
- **Errors.** `BooleanError` kinds:
  - `Refused`: two operand vertices within coincidence (04's row), operands at different
    resolutions;
  - `Unsupported`: `SsiPair`, `EdgeKind`, `NonManifold` until BO7, a NURBS face;
  - `Exhausted`;
  - `Cancelled`;
  - `Defect`: a `Near` sub-span after refinement, a `finish` refusal, a flood that reaches a
    component from two seeds with different answers.
- **Lineage through `Changes`** (BR3, BR7, as built). See §5.6.

### 5.5 Sketch

```rust
// scx-boolean
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOp { Union, Subtract, Intersect }
pub enum NoItems {}

pub fn boolean(a: &Brep, b: &Brep, op: BooleanOp, cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;

#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub enum BooleanError {
    Refused(BooleanRefusal),          // VerticesWithinCoincidence { a, b }, Resolutions { a, b }
    Unsupported(BooleanUnsupported),  // SsiPair { a, b, kinds }, EdgeKind { edge, kind }, NonManifold { at }, FaceKind { face, kind }
    Exhausted { work: u64 },
    Cancelled,
    Defect(BooleanDefect),            // UndecidedSpan { a, b }, Unfinished(Box<Unfinished>), Seeds { component }
}
```

### 5.6 Lineage per operation

Each operation declares its lineage through the `Draft` calls built in BR3/BR7. `finish` derives
`Changes` from them.

**The Boolean (`a` is operand 0, edited; `b` is operand 1):**

| What happens | Declared by | Shows in `Changes` as |
|---|---|---|
| a face of `a` kept whole | nothing (the draft started with it) | unchanged, or `replaced` with `Touch::Boundary` if its loops gained an edge |
| a face of `a` cut into pieces | `derive(piece, Origin::Face(f))` for each new piece; `f` keeps one piece | `split { from: f, pieces }`, ordered by BR3's rule |
| a face of `a` removed | `remove_face` | `removed` |
| a face of `b` kept, whole or cut | `derive(new, Origin::Operand(Operand::from_raw(1), Entity::Face(fb)))` | `created` with that origin; pieces of one `fb` share it |
| a face of `b` coincident with a kept face of `a` | `coincident(Operand::from_raw(1), Entity::Face(fb), fa)` | `coincident` |
| two coplanar kept faces merged | `merge_face(absorbed, survivor)` | `merged`, with `taken` carrying the absorbed face's lineage (BR7) |
| an intersection edge | `derive(e, Origin::Face(fa))` and `derive(e, Origin::Operand(1, Entity::Face(fb)))` | `created` with both origins, sorted |
| a split edge of `a` | `derive(piece, Origin::Edge(e))` | `split` |
| a merged point on an operand vertex | nothing; the operand vertex survives (04's merged-points row) | unchanged |
| a new vertex where an edge crosses a face | `derive(v, Origin::Edge(e))` and the face's origin | `created` |

- **Constructors** declare nothing. Every entity is `Free`, and the typed face record (S6) is the
  naming.
- **Mass, classification and tessellation** are reads and produce no `Changes`. The mesh carries
  each triangle's `FaceId` and each polyline's `EdgeId`, which is the read's own lineage.
- **No region or shell lineage** (BR7: none built).

### 5.7 Truth references missing

- T9 (volumes) and T10 (membership) cover every in-scope case.
- **Recipes** (T12): the 15 `boolean_api` cases above, plate_12, gear_60, and the 4
  `boolean_coplanar_merge` and 7 `issue_1462_boolean_attributes` box cases. Each is given as:
  - frames and sizes;
  - the op;
  - its T9 volume and area;
  - its T10 predicate;
  - its expected outcome (`Ok`, or `Unsupported` with the cause's name).
- **Expected topology** for the in-scope cases: the number of solid regions, shells and the genus,
  stated by hand from the geometry (for example, plate_12 has genus 144: 12 x 12 holes). Face counts are not truth:
  the pin's 7 faces for box-cyl-hole count a seam policy, not the solid.

### 5.8 Builder tasks

| Task | What | Depends on |
|---|---|---|
| BO1 | `scx-boolean` crate, signature, errors; pairs by the box tree; vertex-vertex, vertex-edge, edge-face interferences with merged points (04 row); tests on boxes | CX1, MK1, BQ1 |
| BO2 | face-face: `analytic::intersect`, cut at BO1's points, sub-span keep by `contains_uv`, `Unsupported` for `General`; coincident overlaps (SS4) | BO1 |
| BO3 | imprint into `a.edit()` with exact pcurves; copy of `b`'s faces sharing the new edges; lineage as §5.6 | BO2 |
| BO4 | face-use classification: components, BQ2 seeds, flood, `classify_point` fallback, coincident rule; selection; coplanar merge and vertex removal; ports `boolean_api` `cases()` in scope (box-cyl-hole, box-box-union, sphere-and-box, coplanar-union, torus-minus-box) by outcome, `boolean_coplanar_merge` (4), `issue_1462_boolean_attributes` (7) as lineage tests on `Changes`, `boolean_no_dangling_vertex_refs` (1) | BO3, BQ2 |
| BO5 | the out-of-scope cases as expected-`Unsupported` tests naming their cause (tpipe, both cylinder-sphere seams, torus-sphere, both torus-box seams) | BO2 |
| BO6 | plate_12 and gear_60 as tests by outcome (T9 volume, T10 points, genus) | BO4 |
| BO7 | point contacts: two solids touching at a vertex, and a solid touching itself (seam-box-minus-sphere); the result's non-manifold vertex as scx-brep allows it, or `Unsupported(NonManifold)` by ruling; ports tangent-sphere-box, the three seam/off-seam sphere-box cases, seam-sphere-sphere and `point_contact_unions_check_clean` | BO4; D5 ruled |

## 6. `scx-tess`

### 6.1 Shapes found (READ, MEASURED)

- **Entry points.**
  - `solidcore-tess/src/mesher/mod.rs:246`: `plan(b: &record::Brep, t: &MeshTolerance, size) ->
    Result<(TessPlan, PlanStats)>`.
  - `spec.rs:83`: `mesh(b, &MeshSpec { chord, size, min_angle }, size, size_at)`.
  - `plan.rs:130`: `TessPlan::evaluate() -> (Mesh, Vec<RefusedFace>)`.
  - It reads the record body plus `solidcore::eval::plan` (59 `use solidcore::` lines).
- **The mesh.** `Mesh { positions, normals, triangles: Vec<[u32; 3]>, tri_face, edges:
  Vec<Vec<Vec3>>, seams, smooth_edges, sharp_edges, edge_faces, display_only_edges }`.
- **The contract as built.**
  - Edges are sampled once and shared (`FacePlan.fixed`), so boundary points are bit-identical.
  - The mesh is closed where the body is closed.
  - The chord is a target, not a guarantee: `PlanStats.over_tolerance`, `lenient` and `unmeshed`
    report misses.
  - An invalid chord silently becomes 0.05 mm, and the chord is floored at size·1e-6
    (`tolerance.rs:60-70`).
  - A refused face is returned beside the mesh, not as an error.
- **Ambient state.**
  - A global rayon pool (`par.rs:53`, `static POOL: OnceLock`).
  - `catch_unwind` and a thread-local guard.
  - 8 `Mutex`/`Atomic` lines.
- **Triangulation.** Constrained Delaunay by `spade`.
- **Errors.** `TessError` holds strings.
- **Counts (MEASURED).** 155 `&mut ` lines, 31 out-parameters, 27 `bool` parameters, 112
  `unwrap_or`, 45 swallowed results, 9 ambient-state lines.
- **Pin tests.**
  - `solidcore-tess`: 136. Watertightness by `assert_closed` (`mesher/mesh_tests.rs:273`),
    fold-over, and exact triangle counts, over 36 gzip record fixtures. No golden meshes.
  - `nurbs-grid-mesh/tests/mesh.rs`: 13, among them
    `two_faces_sharing_an_edge_polyline_meet_without_a_crack`.

### 6.2 The contract (JUDGED)

1. **Watertight where the body is closed.**
   - Every edge is sampled once, as a pure function of the edge (66 §12.1, arris ADR-0052).
   - The sample count is the largest that any of its faces needs, read from the edge's own curve
     and its pcurves' boxes, never from face order.
   - Both faces use those points bit for bit, and the mesh shares their vertex indices. A closed
     body therefore gives a closed, consistently oriented mesh.
2. **The chord is a bound on analytic faces, not a target.**
   - Planes have no chord error.
   - A cylinder's or cone's sagitta across a step of angle θ at radius r is `r(1 − cos(θ/2))` in
     closed form. A sphere's or torus's is bounded by its largest curvature radius on the face.
   - So the planner picks steps that meet the chord with certainty.
   - The mesh reports the largest bound used per face. A face that cannot meet the chord is an
     error (`Unsupported { face }` for a NURBS face before step 5), never a silent miss.
3. **No fallback.**
   - `TessSpec::new(chord)` refuses a chord that is not finite, or not above the document's
     resolution (`Refused { chord }`). No 0.05 mm.
   - Angle and edge-length bounds are optional `with_*` settings.
4. **Deterministic.**
   - Serial in step 4, with no statics.
   - Faces are meshed in id order, and triangles are listed by face, then by a fixed order in the
     triangulator.
5. **Lineage.** Each triangle has its `FaceId`. Each edge polyline has its `EdgeId` and its vertex
   indices.
   - The pin's `display_only_edges`, `smooth_edges` and `sharp_edges` are a viewer's
     classification. They are derived on request from the body (the edge's faces and their normals
     along it), not stored in the mesh.
6. **A read of `&Brep`.** `tessellate(body: &Brep, spec: &TessSpec) -> Result<Mesh, TessError>`.
   It takes no `Cx`: it is layer 5 and reads no document beyond the body's resolution. Budget and
   `Stop` are added when step 5's grid mesher iterates.

### 6.3 Sketch

```rust
// scx-tess
#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub struct TessSpec { chord: Length, angle: Option<Angle>, edge: Option<Length> }
impl TessSpec { pub fn new(chord: Length, res: Resolution) -> Result<Self, TessError>; /* with_angle, with_edge */ }

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub positions: Vec<Point3>,
    pub normals: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
    pub face_of: Vec<FaceId>,            // per triangle
    pub edges: Vec<EdgePolyline>,        // { edge: EdgeId, points: Vec<u32> }
    pub chord_bound: Vec<(FaceId, Length)>,
}

pub fn tessellate(body: &Brep, spec: &TessSpec) -> Result<Mesh, TessError>;

#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub enum TessError {
    Refused(TessRefusal),          // Chord { value }
    Unsupported(TessUnsupported),  // FaceKind { face, kind }
    Defect(TessDefect),            // Triangulation { face } (a boundary the triangulator rejects)
}
```

### 6.4 Truth references missing

- **Mesh invariants as a testkit oracle** (T13): closed (each undirected edge used twice with
  opposite orientation), no fold-over (each triangle's normal within 90° of the surface normal at
  its centroid), every vertex on its face's surface within resolution, and each triangle's
  centroid within the stated chord bound of the surface.
- **Mesh volume against T9.** The mesh's signed volume converges to the body's. Its gap is bounded
  by chord times area, which is a checkable truth.

### 6.5 Builder tasks

| Task | What | Depends on |
|---|---|---|
| TS1 | `scx-tess` crate, `TessSpec`, `Mesh`, `TessError`; edge sampling as a pure function of the edge with closed-form sagitta counts | CX1, MK2 |
| TS2 | constrained Delaunay in uv with exact orientation and in-circle predicates (D4); planar faces | TS1 |
| TS3 | cylinder, cone, sphere and torus faces: uv steps from the chord bound, interior points, poles and the apex; ports `assert_closed`, fold-over and the chord checks as outcomes on MK2 primitives and BO4 results, and the crack test's statement (shared polylines meet) on analytic faces; the NURBS fixtures wait for step 5 and are listed as deferred | TS2, T13 |

## 7. `scx-check`, `scx-stitch`, `scx-hlr`: what the slice calls

- **`scx-check`** (step 4 part).
  - The slice calls the result contract only, in tests (S8, 04's row).
  - `contract(body: &Brep, cx) -> Result<Proved, ContractError>`:
    - every edge has two fins on each side of material (manifold);
    - every shell is closed;
    - every solid shell is outward by MA4's certified `volume_sign`.
  - `Proved` is a newtype over `Brep` that only `contract` builds (04's row: "returns a proved body
    type").
  - The full check (`check_brep` Level0-2, `topology/brep/validation/assert_valid.rs:365`) is step
    6.
  - The pin's only non-test call of the full check on the slice's paths is the Boolean's
    `matched_tolerances` (`operand_tolerances.rs:71`), which §5.4 drops.
  - Task **CK1**: `scx-check` crate with `contract` and `Proved`; tests on every MK and BO result.
    Depends on MA4, BQ1.
- **`scx-stitch`.**
  - Nothing in the slice calls it. Primitives are built with known topology (S7), and the Boolean
    builds its result connected (§5.4 step 4).
  - The pin's stitch calls on these paths (`primitives.rs:190`, `merge/p3.rs:896`, 12,766 pin
    lines in the crate) all go.
- **`scx-hlr`.**
  - Nothing in the slice calls it. MEASURED: the pin's primitives, extrude, Boolean and mass call
    no hlr function; the one mention is a doc line at `eval/outline.rs:22`.
  - The `make → hlr` edge of 01 §1.1 exists for the solid sweep (it reads the tool's silhouette),
    which is not in step 4.
- **Proposal: create `scx-stitch` and `scx-hlr` when their first function is ported (step 6), not
  empty in step 4.**
  - 01 §10.2 row 4 says these crates "exist from here, holding what the slice calls". What the
    slice calls of them is nothing.
  - An empty crate adds a `layers.txt` entry, a manifest and a CI build that prove nothing.
  - The `make → hlr` and `boolean → stitch` edges then wait for their first use.
  - Case against: an early crate fixes the layer order and catches a dependency mistake when the
    edge is first added. The `layers.txt` order can be written now, without the crates, as a
    comment.
  - This is an amendment to 01 §10.2's row 4 wording, proposed as an 04 row (§12.1).

## 8. The outcome difftest and the first performance measurement

### 8.1 What it compares (JUDGED)

The new difftest file is `crates/scx-difftest/tests/diff/t_slice.rs`. Each case is one T12 recipe,
built in scx (MK, then BO) and in the pin (its `api` primitives and Booleans). Each is judged
against truth first and the pin second:

| Check | Truth | Pin | A mismatch means |
|---|---|---|---|
| outcome | T12's expected `Ok` or `Unsupported(cause)` | the pin's `Ok` or error | scx wrong if it differs from T12; an scx `Unsupported` where the pin succeeds is listed, not a failure |
| volume | T9 closed form; scx within its requested accuracy (1e-9 relative), plus the stated formula error | the pin's `brep_volume(1e-6)` at the same check | scx outside: failure; pin outside, scx inside: a `pin_defects.rs` entry with an input predicate |
| area | T9, as volume | as volume | as volume |
| topology | solid regions, shells, genus (Euler-Poincaré: V − E + F − (L − F) = 2(S − G)) and maximal smooth patches (faces joined across edges where both sides lie on one surface record with one side; A12) from T12 | the same invariants from the pin's body | failure; face, edge and vertex counts are printed beside the pin's, never compared (seam policy) |
| classification | T10 on 1,000 seeded points in the box grown 10%, plus T10's points on purpose; points in T10's `Band` must classify `On` or the side T10 gives at the band's edge | the pin's `point_3d_classify` on the same points | scx wrong: failure; pin wrong: a `pin_defects.rs` entry |
| validity | `scx_check::contract` returns `Proved`; testkit `violations` is empty | the pin's `is_manifold_solid` | failure |
| mesh | T13 invariants at chord 1e-2 and 1e-3 of the box diagonal; mesh volume within chord·area of T9 | not compared (the pin's mesher is a separate crate with targets, not bounds) | failure |
| determinism | each case run twice: the body's tables (the `scx-record` encoding once it lands; a canonical dump of the tables until then), `Changes`, `Report::work` and the mesh are equal in bits | — | failure |
| lineage | for the 7 `issue_1462` box cases: each result face's origins name exactly the operand faces T12 lists | the pin's attribute ids | failure |
| work | `Report::work` and cells filled, recorded per case | — | a rise of more than 10% against the committed baseline fails unless the change says why (01 §9.4) |

- **Seeds.** `Rng::new(mix(seed, case, 0))`, with one fixed seed per case written in the test.
- **Inputs counted** like the other difftests: points, recipes and meshes, so the harness's "N
  inputs, 0 mismatches" line stays comparable.
- **No face-count comparison.** The pin's `smlib_faces` column records SMLib's seam and merge
  choices, not the solid. 04's "truth over pin" row rules this out as a check. Counts are printed so
  a reviewer sees a change.

### 8.2 The first performance measurement (01 §9.4)

- **Cases.** gear_60 and plate_12, with 68's recipes:
  - the rotated boxes are built at their frames (MK1) instead of transformed;
  - then `brep_volume(1e-6)` and area become `scx_mass::mass` at accuracy 1e-6.
- **Bench target** `slice` in `crates/scx-bench`:
  - criterion wall time and gungraun instruction counts;
  - scx and the pin on the same machine and run;
  - the phases split as 68 does (build, Booleans, volume, area).
- **Recorded** in `05-status.md` as the first §9.4 finding: scx against the pin per case, with the
  ratio. The target is no more total time than the pin and no case over 1.25 times. A miss is a
  finding, not a blocker (row "Perf regressions: judgement").
- **Expected direction (INFERRED, to be measured).** These are inferences from 68's shares, not
  measurements:
  - no stitch or ray fire for primitives removes up to 33% of plate_12;
  - no gap closure removes about 10% of gear_60;
  - the analytic SSI and SU6 replace the general solvers;
  - against that, scx's correctly rounded `fmath` and certified bounds cost more per call (71: up
    to 4.5 times the pin on the quartic).
- Task **PF1**: the `slice` bench target and the 05 record. Depends on BO6, MA1.

### 8.3 Builder tasks

| Task | What | Depends on |
|---|---|---|
| DT1 | `t_slice.rs` harness: recipe runner on both sides, the checks of §8.1, pin-defect entries with predicates, run-twice determinism | T9, T10, T12, T13 |
| DT2 | primitives and extrusions in `t_slice` (MK cases) | DT1, MK2, MK3, TS3, CK1 |
| DT3 | Boolean cases in `t_slice` (BO cases, the expected `Unsupported` ones included); the work baseline committed | DT2, BO6, BO5 |

## 9. Performance techniques, per crate, with the conditions that keep them correct

04's step-4 order is 68's: primitives with known topology, then the point-in-face uv-box reject,
then the mass memo, then a cheap pair prefilter.

| Rank (68) | Technique | Crate, task | Correct only if | Checked by |
|---|---|---|---|---|
| 1 | primitives with known topology, no stitch and no ray fire | make, MK1-MK3 | each region is the one the geometry gives; every fin is on the side its region needs | `contract`, T9 volumes, T10 points (DT2) |
| 2 | uv-box reject before the loop walk | brep (BR4 cells, built), used by bodyq BQ1 and boolean BO2 | the box encloses the trimmed loop: outward-rounded over the pcurve's enclosure, not samples; a loop across a periodic seam gets an unwrapped box or two boxes (66 §10.3) | 66 §10.3's cases: a loop across a cylinder's seam, a hole whose box touches to the last bit, a near-touching loop outside |
| 2 | per-face loop cache for the Boolean | brep cells (BR4: the boundary tree filled on first read) | cells are keyed and invalidated by the face record (BR4), never by an ambient id | BR4's cell tests; bit-identical repeat in DT3 |
| 3 | per-face mass memo | mass, MA5 | the face's own reference and error scale (66 §13.2); keyed on the face record's bits, its sense and the accuracy; a memo hit charges the recorded work | MA3's memo-against-recompute test; T9 truth once |
| 3 | gap closure only where a gap is recorded | mass, MA2 | a use without recorded accuracy has an exact pcurve (BR1, exact pcurves rows); no step-4 operation records one except a merge that moved a point | MA2's accuracy-carrying body; T9 |
| 4 | cheap pair prefilter | boolean, BO1 | the face boxes enclose (BR4: grown by accuracy and resolution); no allocation per call; one finiteness check per box | DT3's pairs-tried count against the pin's 572 (gear_60) and 288 (plate_12), side-channel only |
| 5 | trim-deviation bound carried with copied trims | boolean, BO3 | a copied face keeps its pcurve bits, so its recorded accuracy (none, when exact) is the same record | BO3: copied faces' records equal by bits |
| 6 | no allocation in hot predicates | all | scratch reuse only, with no result change | bit-identical repeat |
| 7 | dense ids and live iteration | brep (BR1, built) | entity order kept | built |
| 8 | edge samples as a pure function of the edge | tess, TS1 | the count from the edge alone, not face order (66 §12.1; Fornjot's face-order cache is the trap) | T13 closed; mesh repeat in any face order |
| 9 | filtered exact predicates | tess TS2, planar PL1 | float with a stated error bound, exact fallback by error-free transforms (`scx-num` `eft`) | the 2-D predicate tests; cross-target bits (67) |
| — | an analytic empty is final | boolean BO2, bodyq BQ1 | SSI's and SU6's empties come from exact bounds (04 rows on SS1/SS5, SU6) | the SS and SU6 difftests, already run |
| — | Gauss-Kronrod | mass, MA1 | ported from the pin, not re-adopted (68 §5); nodes and weights hard-coded literals | T9 |

Not adopted in step 4:

- Lazy transforms and moved boxes (66 §11.4). Constructors build at frames, so nothing is
  transformed.
- N-ary union by composition (66 §11.2). Step 7.
- OCCT-style gluing (66 §11.3). It needs 60 B1's caller-stated facts, which step 4 does not have.
- Faceting faces concurrently (66 §12.2): out of step 4.

## 10. Adversarial pass

Each proposal is argued against here. Where the argument wins, the body above was revised; those
places are noted.

**A1. "One signature" is broken by having both `&Brep` operations and `Draft` methods.**

- **Against.** A user sees `boolean(&Brep, &Brep)` returning a new body and `Draft::transform`
  editing in place, and must learn when each applies.
- **For.** The rule is one sentence: an operation that reads cells takes finished bodies, and an
  edit that reads none is a `Draft` method. Each operation still has exactly one signature, and no
  operation has a `&mut Brep` or a fallback pair.
- **Alternative rejected.** `boolean_into(draft: &mut Draft, tool: &Brep)` would make the
  operand's cells `Option` throughout the Boolean, which is every `faces_meeting` and every
  `bounding_box` call.
- **Kept.**

**A2. Constructors returning `Made` instead of `Done` is a second result type.**

- **Against.** The feature layer then handles `Done` and `Made`.
- **For.** A constructor has no `Changes` worth the name and no report: no items, no warnings, and
  nothing can be skipped. `Done` with empty fields would hide the one thing a constructor returns
  that matters, the role record.
- **Revision.** `Made<R>` holds only `body` and `faces`. It is not a second report type.
- **Kept.**

**A3. The nearest clean hit can be wrong where parity would be right.**

- **Against.** If the nearest face is clean but a face nearer still was missed (SU6 missing a
  root), the side is wrong. Parity would also be wrong in that case, so nothing is lost by the
  nearest hit; but the claim "never a guess" depends on SU6's completeness.
- **For.** SU6 isolates roots in Bernstein form with an empty answer final only from exact bounds
  (04's SU6 row). That is the same completeness every Boolean step already rests on.
- **Revision.** BQ1's tests include a ray grazing a face just inside coincidence and a ray through
  a seam, to see the clean rule refuse them.
- **Kept, with that test.**

**A4. 13 rays then `Unsupported` will fire in practice.**

- **Against.** Points on purpose, such as the Boolean's fallback point on a face's first edge
  midpoint, are on the boundary of the *other* body only by coincidence. But a recipe like
  seam-box-minus-sphere puts many points near tangencies.
- **For.** The fallback runs only for components touching no intersection edge. Those are faces
  wholly inside or outside, whose edge midpoints are far from the other body's boundary unless the
  bodies touch there, and touching gives an intersection edge or point, hence a seed.
- **Revision.** DT1 counts `NoClearRay` per run (side-channel) for the first measurement.
- **Kept.**

**A5. Dropping the pin's stitch makes scx unable to accept loose faces at all.**

- **Against.** Importers and repair produce loose faces.
- **For.** That is `scx-stitch` in step 6, where its first caller (`scx-step`) arrives. Nothing in
  step 4 produces loose faces.
- **Kept.**

**A6. Deferring `scx-stitch` and `scx-hlr` contradicts 01 §10.2.**

- **Against.** The plan said they exist from step 4.
- **For.** It said they exist "holding what the slice calls", and the slice calls nothing of them.
  An empty crate proves no edge.
- **Revision.** This is proposed as an amendment (§12.1), not done silently.

**A7. Building at a frame changes last bits against the pin and makes the difftest noisy.**

- **Against.** Every gear_60 box differs from the pin's in the last bits, so volumes differ too.
- **For.** The difftest judges volume against T9 truth at the stated accuracy, not bits against
  the pin (row "Truth over pin"). One rounding is more correct than two (66 §11.4).
- **Kept.**

**A8. Gap closure skipped "where exact" relies on a claim the body does not prove.**

- **Against.** A pcurve recorded as exact could be wrong by a builder bug, and the strip would then
  hide nothing but also measure nothing.
- **For.** `Draft::finish`'s cheap checks and BR8's `violations` test that pcurves lie on their
  edges within resolution. A wrong exact pcurve is a `Defect` found there, not one mass must also
  find.
- **Revision.** MA2's test includes a `broken` body with a pcurve off its edge, to show `finish`
  refuses it before mass sees it.
- **Kept.**

**A9. The chord as a bound is impossible for NURBS, so the contract changes in step 5.**

- **Against.** Step 5's grid mesher meets a target, not a bound.
- **For.** Bounds on NURBS chord error exist from the certified surface enclosures (scx-surface's
  span certificates), at a cost. The contract can stay "bound" with `Unsupported` for a face whose
  bound cannot be met.
- **Revision.** The contract's text says "a bound on every face meshed; a face that cannot be
  bounded is an error"; step 5 decides whether to add a target mode as a separate entry.
- **Open as D6.**

**A10. An own constrained Delaunay is work a library already did.**

- **Against.** `spade` is MIT/Apache, popular, used by truck and Fornjot, and robust.
- **For own.** 01 keeps `[external]` empty. `spade`'s predicates are the `robust` crate's, whose
  cross-target bits (67) are unaudited. Step 4's faces are small uv polygons with holes, where a
  sweep-line CDT with `scx-num`'s error-free transforms is a few hundred lines.
- **Open as D4**, recommending own with `spade` as the change-if (a measured cost or a bug rate
  that own code does not meet).

**A11. Putting `Cx` in `scx-brep` is the wrong layer.**

- **Against, and for.** Argued in S2.
- **Open as D1.**

**A12. A face-count check is what caught real pin bugs; dropping it weakens the difftest.**

- **Against.** `boolean_api`'s exact face counts against SMLib's found split-face bugs.
- **For.** A split-face bug shows in scx as a coplanar merge missed, which the genus check does not
  see. So the counts are useful as a printed diff.
- **Revision.** DT1 also checks the count of *maximal smooth patches*: faces joined across edges
  where both sides lie on one surface record with one side. T12 states that count by hand for each
  in-scope case. That is truth, and it catches a missed merge without encoding a seam policy.
- **Revised in §8.1.**

**A13. 10 of 15 cases is not "Boolean on analytic bodies".**

- **Against.** The pin covers all 15.
- **For.** The 5 missing need SS2 (non-coaxial curved pairs) or the spiric sections, which 01
  places in step 5 with the general marcher. Building SS2 early in step 4 would widen the slice
  into step 5's geometry.
- **Revision.** None, except that the expected-`Unsupported` tests (BO5) make the gap visible.
- **Kept.**

## 11. Task graph

```
CX1 ──┬─> MA1 ─┬─> MA2
      │        ├─> MA4 ──> CK1
      │        └─> MA5 (after BO4)
      ├─> MK1 ─┬─> MK2 ──> TS1 ──> TS2 ──> TS3
      │        └─> MK3 (after PL1)
      ├─> BQ1 (after MK2, T10)
      └─> BO1 (after MK1, BQ1) ──> BO2 ──┬─> BO3 ──> BO4 (with BQ2) ──┬─> BO6 ──> PF1
                                         │                           ├─> BO7 (after D5)
                                         └─> BO5                     └─> MA5
T9, T10, T11, T12, T13 (testkit) ──> DT1 ──> DT2 ──> DT3
PL1 (after T11)
```

| Task | Crate | Depends on | Model |
|---|---|---|---|
| CX1 | brep (`op`): `Cx`, `Settings`, `Scratch`, `Trace`, `Done`, `Report`, `Warning`, `Skipped`, `Raise` | D1 | Sonnet |
| T9 | testkit: closed-form volumes, areas and moments of the slice's recipes | — | Sonnet |
| T10 | testkit: CSG membership oracle and points on purpose | — | Opus |
| T11 | testkit: profile fixtures | — | Sonnet |
| T12 | testkit: the recipe list with expected outcomes, invariants and patch counts | T9, T10 | Sonnet |
| T13 | testkit: mesh invariants oracle | — | Sonnet |
| MA1 | mass | CX1, T9 | Opus |
| MA2 | mass | MA1 | Sonnet |
| MA5 | mass | MA1, BO4 | Opus |
| MA4 | mass | MA1 | Opus |
| PL1 | planar | T11 | Opus |
| MK1 | make | CX1, T9 | Sonnet |
| MK2 | make | MK1, MA1 | Opus |
| MK3 | make | PL1, MK1 | Opus |
| BQ1 | bodyq | CX1, T10, MK2 | Opus |
| BQ2 | bodyq | BQ1, BO2 | Opus |
| BO1 | boolean | CX1, MK1, BQ1 | Opus |
| BO2 | boolean | BO1 | Opus |
| BO3 | boolean | BO2 | Opus |
| BO4 | boolean | BO3, BQ2 | Opus |
| BO5 | boolean | BO2 | Sonnet |
| BO6 | boolean | BO4 | Sonnet |
| BO7 | boolean | BO4, D5 | Opus |
| CK1 | check | MA4, BQ1 | Sonnet |
| TS1 | tess | CX1, MK2 | Opus |
| TS2 | tess | TS1, D4 | Opus |
| TS3 | tess | TS2, T13 | Opus |
| DT1 | difftest | T9, T10, T12, T13 | Opus |
| DT2 | difftest | DT1, MK2, MK3, TS3, CK1 | Sonnet |
| DT3 | difftest | DT2, BO5, BO6 | Sonnet |
| PF1 | bench | BO6, MA1 | Sonnet |

- **Can start now:** T9, T10, T11, T13, and CX1 once D1 is ruled.
- **The critical path** is CX1 → MK1 → MK2 → BQ1 → BO1 → BO2 → BO3 → BO4 → BO6 → PF1.
- **Layers.** Each crate is added to `layers.txt` in the order `scx-planar` (after `scx-curve`),
  `scx-tess`, `scx-mass`, `scx-bodyq` (after `scx-ssi` and `scx-brep`), `scx-check`, `scx-boolean`,
  `scx-make` when its first task lands.

## 12. Proposed 04 rows and open decisions

### 12.1 Proposed rows (for the coordinator's ruling and Brett's review)

1. **Operation signatures (S1).** Operations that read their operands' cells take finished `&Brep`
   operands and return `Done`. Whole-body edits that read no cells are `Draft` methods. There is
   never a `&mut Brep`. Constructors take a frame and sizes and return `Made<R> { body, faces }`, a
   new body with a typed record of its faces by role.
   - Clarifies 01 §7.1 for the brief's "`&mut Draft` writes".
   - Change if: an operation needs both cells and in-place writes on one body.
2. **Point classification (§3.2).**
   - `classify_point(&Brep, p, cx) -> Result<PointClass { In(RegionId), On(Entity) }, BodyqError>`.
   - An On test within coincidence first, then the nearest clean hit over a fixed 13-direction
     table, by SU6 and `contains_uv`.
   - `Unsupported(NoClearRay)` after 13 unclean rays. No retry at a widened tolerance, and no
     `Unknown`.
3. **The analytic Boolean's scope (§5.2).**
   - Step 4 answers planar faces against all kinds, two spheres and coaxial pairs.
   - Every other pair whose boxes meet, including a plane against a torus off its axis, is
     `Unsupported(SsiPair)` by name.
   - The five out-of-scope `boolean_api` cases are ported as expected-`Unsupported` tests.
4. **The Boolean's construction (§5.4).**
   - Lower dimension first.
   - Imprint into `a.edit()` with the new edges shared by both operands' faces, so there is no
     stitch.
   - No operand tolerance rewriting (the pin's `matched_tolerances` and
     `update_tolerances_after_boolean` are not ported).
   - Lineage as §5.6.
5. **Mass (§2.2).**
   - Reads over `&Brep` with `&mut Scratch`, `&mut Budget` and `&Stop`.
   - Each face integrated about its own reference and shifted.
   - Gap closure only for uses with a recorded accuracy.
   - `LoopAreaMemo` moves to `Scratch`.
6. **Tessellation (§6.2).**
   - `tessellate(&Brep, &TessSpec) -> Result<Mesh, TessError>`.
   - Watertight by edge samples that are a pure function of the edge.
   - The chord is a bound on analytic faces.
   - No 0.05 mm fallback: a bad chord is `Refused`.
   - Serial, with no statics.
   - Viewer edge classes are derived, not stored.
7. **Outcome difftest (§8.1).**
   - Checks: outcome, volume, area, invariants (regions, shells, genus, maximal smooth patches),
     T10 classification, contract, mesh invariants, determinism, lineage and work.
   - Judged against truth first. The pin's disagreements become `pin_defects.rs` entries with
     predicates.
   - Face counts are printed, not compared.
8. **Amend 01 §10.2 row 4 (§7).** `scx-stitch` and `scx-hlr` are created with their first ported
   function (step 6), not empty in step 4. The `boolean → stitch` and `make → hlr` edges wait for
   their first use.

### 12.2 04 rulings this note leans on but does not change

These rows are used as they stand:

- coincidence as the maximum;
- the work budget;
- the result contract in `scx-check`;
- merged points;
- no fixed-tolerance overlap;
- 68's consequences (empty is final; the perf order);
- SU6;
- SS1/SS5 and SS3;
- exact pcurves;
- BR1, BR3, BR6, BR7 and BR8.

Proposed row 8 amends 01's plan text, not an 04 row:

- **Case for:** an empty crate proves nothing.
- **Case against:** an early crate checks the layer order. The order can be written in
  `layers.txt` as a comment instead.

### 12.3 Open decisions, with recommendations

| # | Decision | Options | Recommended |
|---|---|---|---|
| D1 | Where `Cx`, `Settings`, `Scratch`, `Done` and `Report` live | `scx_brep::op`; a new `scx-op` crate at layer 4 | `scx_brep::op` (S2), change if it grows past plain types |
| D2 | Mass's arguments | `Cx`; the pieces it reads | the pieces (01 §6.3: mass reads no settings or document) |
| D3 | Horn torus | build it; `Unsupported` until BR2's pole rules cover an apex on the axis | `Unsupported { HornTorus }` |
| D4 | Constrained Delaunay | own, with `scx-num` error-free predicates; `spade` after an M5 audit and an `[external]` listing | own; `spade` is the change-if |
| D5 | Point contacts in the Boolean's result (BO7) | a non-manifold vertex, if scx-brep's regions and sides allow it; `Unsupported(NonManifold)` in step 4 | ask BR2's owner whether a vertex shared by two solid regions passes `finish`; if yes, build it in BO7; if not, `Unsupported` and BO7 moves to step 7 |
| D6 | The chord contract in step 5 | a bound everywhere, with `Unsupported` where it cannot be met; a separate target mode | decide in step 5; step 4's contract is "bound" |
| D7 | Partial primitives (angle ranges) | in step 4; later | later, `Unsupported` by name (no slice caller) |
| D8 | Parabola and hyperbola edges against faces | widen SU6 in step 4; `Unsupported(EdgeKind)` | `Unsupported`; no in-scope case makes one |

### 12.4 Rulings after the adversarial pass (11-27, coordinator, for Brett's review)

A second, independent reviewer read §12.1 and §12.3 against the pin and scx-brep on main. These rulings replace the proposals where they differ.

1. **Signatures (row 1): kept.** Operations take finished `&Brep` operands and return `Done`. Whole-body edits that read no cells are `Draft` methods. Constructors return `Made<R>`. There is never a `&mut Brep`.
   - b's copied faces get new ids in a's draft, with lineage through the operand tag.
   - Record-identical operands are not special-cased in step 4.
   - Change if: DT3 shows table copy-on-write above 10% of op time in a feature chain. Then operands the operation consumes take `Brep` by value (`into_draft`).
2. **Classification (row 2): changed.** The signature is `classify_point(&Brep, p, &mut Budget, &Stop) -> Result<PointClass, BodyqError>`.
   - **On test first.** A point is `On` when its distance to an entity is within coincidence. A uv that is only near, with no edge within coincidence, is not On.
   - **Rays.** Then the nearest clean hit over a fixed table of directions in general position: irrational components, none in a coordinate plane. A rhombic dodecahedron centred on p defeats an axis and diagonal table.
   - **Fallback.** If no ray is clean, the nearest boundary point decides. It is found by closed-form projection onto the nearby analytic faces, edges and vertices, pruned by the box tree. The answer comes from the face side, or at an edge or vertex from its normal cone (angle-weighted pseudonormal).
   - There is no `Unknown`, and no `Unsupported` for analytic bodies.
3. **Boolean scope (row 3): kept.** The five out-of-scope `boolean_api` cases are ported as expected-`Unsupported` tests, each naming the step that lifts it.
4. **Boolean construction (row 4): changed.** Lower dimension first, imprint into `a.edit()` with shared edges, no stitch, and no rewriting of operand records.
   - **Same operand.** Two vertices of one operand within coincidence are `Refused`, as the merged-points row has it. Vertex clearance is a property of one body.
   - **Across operands.** A vertex of a and a vertex of b within coincidence are identified: a's record is kept, b's accuracy is folded in as `Measured`, and `Changes` records a coincident lineage for b's vertex. Mated parts and coplanar-union share corners, so refusing them would refuse valid input.
   - **This amends the 11-27 merged-points row,** which is now read per operand.
   - **Measured gaps.** Every new vertex and edge end records its measured gap against its curves and p-curves as `Measured` accuracy when it exceeds the resolution, not only after a merge. This covers the grazing-cut gap the pin's `update_tolerances_after_boolean` patched (pin #16187, Test #82).
   - **Change if:** an import operand's accuracy swallows a feature of the other operand (the pin's `matched_tolerances` case, #3292). That gets a T12 recipe in step 6.
   - **One resolution.** An operation takes one `Document`, so its operands share one resolution. Joining bodies from two documents goes through an explicit rebase operation, which is a later task, and is not an operation-time `Refused`.
5. **Mass (row 5): kept, with D2.** It takes the pieces it reads.
6. **Tessellation (row 6): changed.** The signature is `tessellate(&Brep, &TessSpec, &mut Budget, &Stop) -> Result<Mesh, TessError>`, with `Exhausted` and `Cancelled` as everywhere else.
   - The chord is a bound on analytic faces, proved per triangle by a refinement test on the kind's maximum curvature over the patch.
   - For a torus, that is max(1/r, 1/|R−r|). §6 is to be read with that correction, not its largest radius.
   - Edge sagittas bound edge chords only. Interior triangles need the per-triangle test.
7. **Outcome difftest (row 7): changed for T10.** A point p is in the `Band` when the interval enclosure of the CSG function over p ± δ contains 0, with δ ≥ √3·coincidence.
   - Points outside the band are judged In or Out. Band points accept any answer and are counted, not judged.
   - Samplers keep at least 95% of points certain.
   - The determinism dump writes through scx-record's format.
   - The work baseline is side-channel counts against the pin's pairs tried (572/288), with a 10% gate.
8. **Row 8: kept.** `scx-stitch` and `scx-hlr` are created with their first ported function. The planned layer edges are a comment in `layers.txt`.

**Open decisions**
- D1: `scx_brep::op`.
- D2: the pieces.
- D3: `Unsupported { HornTorus }`.
- D4: our own constrained Delaunay on scx-num's predicates.
- D6: decided in step 5.
- D7: later, as `Unsupported` by name.
- D8: `Unsupported(EdgeKind)`.
- **D5: changed.** finish already accepts a vertex shared by two solid regions and an edge with more than two fins (check.rs has no vertex-manifold rule and no fin limit). So BO7 builds vertex and edge contacts in step 4.
  - A touch inside a face (a sphere tangent to a box face) needs a point loop, which scx-brep does not have. It is `Unsupported(NonManifold)` until a point-loop row exists.
  - New task **BR10**: finish gains a geometric radial-order check for edges with more than two fins. The order is only the caller's duty today, so a wrong order passes.

## 13. What this note did not do

- It measured no scx performance. §8.2's direction is INFERRED from 68's shares.
- It did not read `intersect/ray_tracer.rs`, `query/point_identity.rs` or `query/planar_disjoint.rs`
  at the pin. The classification survey found the body classifier without them.
- It did not count the pin's 136 `solidcore-tess` tests by kind. §6.5 ports the invariant
  checks, not the exact triangle counts, which are the pin's output, not truth.
- It did not check whether `scx-brep`'s `finish` accepts a vertex shared by two solid regions. That
  is D5's question.

## 14. Built (11-27)

### 14.1 T9 as built (11-27): `scx_testkit::solid`

**What.** `crates/scx-testkit/src/solid.rs` (always on, plain `f64` data, no geometry type) and
`crates/scx-testkit/tests/solid.rs` (11 tests, 1.5 s in debug).

- `Moments { volume, first, second }`: `int 1`, `int x`, `int x x^T` at unit density about the
  placement's origin. `plus`, `minus`, `translated`, `rotated`, `placed(&Frame)`, `centroid`,
  `inertia_about_centroid`. `Solid { moments, area }` with `place(&Frame)`.
- Recipes at the standard placement (axis `z`, base on `z = 0`; sphere, cap and torus centred on
  the origin): `block`, `cylinder`, `cone(r0, r1, h)` (a frustum; `r1 = 0` a cone), `sphere`,
  `sphere_cap(r, h)`, `torus_sector(R, r, sweep)` and `torus` (sweep `2 pi`; `pi` is half a
  torus), `plate(dx, dy, dz, &[Hole])` (a block less through-cylinders), `gear(R, h, count,
  root, gap, phase)` (`gear_60`), and `extrusion(&[Loop], h)` with `Loop { start, segs: Vec<Seg> }`,
  `Seg::{Line, Arc}`, plus `profile_area`.
- `SolidError::{Dimension, Range, Profile, Apart}` for a recipe with no solid (holes that
  overlap or leave the plate, a slot wider than the pitch, an open loop, an arc whose ends differ
  in radius, a clockwise outer loop).
- `REL_ERR = 1e-13`, the stated error, against the largest term in the result.

**Derivations** are in the doc comments: bodies of revolution by slice polynomials integrated
exactly (`V = pi int rho^2`, `S_xx = pi/4 int rho^4`, ...); the torus sector by the tube's
disk integrals; planar profiles by Green's theorem with closed-form `int cos^a sin^b` on arcs
(the reduction formulas); the gear by the formula of §2.4 for volume and area, and by the same
profile machinery for its moments (a slot is a loop of three lines and one arc, turned about
`z` and subtracted).

**Checked by** (each an independent route, `tests/solid.rs`):
1. textbook formulas (block, cylinder, cone and frustum, sphere, hemisphere and cap, torus,
   half torus; the degenerate cases `cone(r, r) = cylinder`, `cap(2 r) = sphere`, two half
   tori = a torus, a circle profile = a cylinder, a rectangle = a block, plate + holes = block);
2. double-double evaluation of the stated error (sphere, cylinder, torus, `plate_12`, `gear_60`;
   all within `REL_ERR`);
3. Monte Carlo with `scx_num::hash::Rng`, all ten integrands (`1, x, y, z, xx, ...`) of the body
   placed at the world frame and two seeded random frames, within 5 sigma of the sample
   variance (300,000 to 400,000 samples; five primitives, three torus sweeps, the cap, `plate_12`,
   `gear_60`);
4. nested quadrature of a gear slot (3-point Gauss on 4,000 panels) against the profile
   machinery's slot moments;
5. polygon moments (shoelace) of arcs cut into 40,000 chords for profiles with a hole, a
   clockwise arc, a reflex arc and a loop far from the origin (1e-8);
6. rigid motion: volume and area bit-equal, the centroid moves as a point, the inertia turns as
   `R I R^T`, and placing twice equals placing by the composed frame; the gear turned by one
   pitch is the same gear.

**Decisions** (cases for and against).
- *Moments as `int x x^T` about the placement's origin, not an inertia tensor.* For: they add and
  subtract under Boolean recipes and move under a rigid motion by two short formulas, and the
  tensor is a view. Against: a reader wants inertia; `inertia_about_centroid` gives it.
- *`f64` with a stated bound, not double-double throughout.* For: the difftest compares with
  `f64` bodies at 1e-9 relative, four orders looser than `REL_ERR`; the double-double check in
  the tests shows the bound holds. Against: a cancelling sum (a plate with many large holes)
  loses digits against its remainder; the bound is stated against the largest term for that
  reason. A region small and far from its arcs' centres (the gear's slot, 45 from the centre,
  1 wide) loses `(distance / size)^3` more; the slot's moments agree with quadrature to 1e-10.
  The gear's total is unaffected, as the slots are 3% of it.
- *Planar integrals taken about the first loop's start.* For: lines stop cancelling when the
  region is small and far from the origin. Against: arcs still cancel against their centre.
- *Slots and holes assumed apart and inside, refused otherwise (`Apart`).* For: the closed form
  is then exact; the recipes (T12) all satisfy it. Against: an overlapping case has a closed form
  too, but not a simple one, and a refusal is honest.
- *The 3-point Gauss rule on lines in `extrusion`.* The integrand `x^p y^q dy` is a polynomial of
  degree at most 3 along a line, so the rule (exact to degree 5) is exact: it is arithmetic, not
  quadrature error.
- *A gear's "box" is cut to the disk.* The pin's box (10 x gap x 12, from `x = 45`) is wider than
  the disk and taller than the gear; only the part inside the cylinder removes material.

**72 §2.4's `plate_12` figure was right; this "deviation" was wrong (corrected by T12, §14.9).** The text gives `plate_12` as `100000 - 5760 pi`. The benchmark's `plate_holes_12` is 12 x 12 = 144 holes (68's "twelve" is per side; the benchmark table says "12×12 holes" and 150 live faces = 6 + 144), so `144 * 40 pi = 5760 pi` and `100000 - 5760 pi` is the volume (81904.42; the pin's 81904.95 is its own error). T9's `plate` function is general and correct; its test's twelve-hole plate is a fine test, named `plate_12` by mistake. T12 uses 144 holes. The area is `2 (10^4 - 576 pi) + 4000 + 5760 pi`.

**Open.** Cones with a negative-slope frustum (`r1 > r0`) work (slice radius is linear); a
cone with an apex inside the height range (a double cone) is two recipes. A membership oracle
for these shapes is T10's, not here: the tests carry their own small ones.

### 14.2 T10 as built (11-27): `scx_testkit::csg`

**What.** `crates/scx-testkit/src/csg/` (`mod.rs`, `ivl.rs`, `profile.rs`, `probe.rs`) and
`crates/scx-testkit/tests/csg.rs` (21 tests, 1.2 s). Plain `f64` data; no library type.

- `Csg::leaf(&Frame, Primitive) -> Result<Csg, CsgError>`, then by-value `union`,
  `intersection`, `difference` and `complement`. `Primitive` is `HalfSpace`, `Slab`, `Block`
  (corner at the origin), `Cylinder`, `Frustum` (`radius1 = 0` is the cone), `Sphere`, `Torus`
  and `Extrusion { profile, height }`. `Profile::new(&[Vec<Seg>])` takes loops of
  `Seg::{Line, Arc, Circle}`. Loops may nest, and odd winding is inside.
- `side(p, delta) -> Side::{In, Out, Band}` is §12.4 row 7: `Band` iff the outward-rounded
  interval enclosure of the tree's `f` over the box `p ± delta` holds 0. `enclose` returns that
  enclosure. Union is min, intersection is max, difference is `max(a, -b)` and complement is
  `-f`, all exact on intervals. A leaf maps the box into its frame by `R^-1` as intervals, so
  any non-singular axes work (a sheared ellipsoid is tested).
- `census(points, delta) -> Census { inside, outside, band }` with `certain_fraction()` is the
  sampler helper row 7 asks for. `box_points(lo, hi, grow, count, seed)` gives the 1,000
  points in the box grown 10%.
- `probes(&ProbeSpec) -> Vec<Probe>` gives the points on purpose. Each `Probe` has a `point`,
  a `feature` (leaf, plus `Face`, `Edge`, `Vertex` or `Meet { other }`), a `dir` and an
  `offset`.
  - Seeded points are taken on each face, edge and vertex (with seams, poles and cap centres
    on purpose), and on the meets of two leaves' surfaces, found by walking parameter lines and
    bisecting the other leaf's sign change.
  - A point is kept only if the tree is certainly `In` and certainly `Out` among its offsets of
    ±10 coincidences, so faces buried in another leaf and the shared face of a coplanar union
    give none.
  - Each kept point gives an offset 0 probe and offsets ±{½, 2, 10}·c along each normal and the
    normals' sum. At a meet the normals' difference is added too: a meet's normals are each
    leaf's own, and the solid may lie in any of the four quarters they bound. Without it, the
    rim of a hole gave no meets.
- `CsgError` (`#[non_exhaustive]`) has `Frame`, `Size { what }`, `ProfileShape` and
  `ProfileOpen { gap }`. A non-finite point, a negative width or a magnitude of 1e150 or more
  is `Band`: the oracle makes no claim there.

**Tested against** truth independent of the oracle:
- Plain-`f64` signed distances written in the test: box, cylinder as a 2D box in (ρ, z),
  frustum by its meridian segments, sphere and torus. Every certain answer must be farther than
  `delta` on its side, at 40 random frames × 3 widths × 300 points per kind.
- The analytic `Sphere::closest` and `Torus::closest`.
- Exact `Rat` signs for a half-space at width 0, at points within 3 ulps of its plane.
- Mirror symmetry, and turning a frame about its axis.
- Equivalent trees never contradict each other (no point is `In` for one tree and `Out` for
  another), and at least 85-90% agree with certainty:
  - a block, six half-spaces, three slabs and an extruded rectangle;
  - a cylinder, an extruded circle, two half arcs, a clockwise full arc and a straight frustum;
  - a rounded rectangle and the union of two blocks and four cylinders;
  - a ring extrusion and a cylinder minus a cylinder.
- Commutativity and De Morgan, with enclosures equal in bits.
- Profile winding against an exact `i128` crossing test (points on an edge are skipped), with
  loops reversed.
- Arc distances against the closed form.

**Decision: interval enclosure, no Lipschitz bound** (row 7 as ruled). For: it is sound by the
inclusion property alone. A boundary point has `f = 0` by continuity, so every point within
`delta` of the boundary is `Band`, and the ∞-norm box contains the ball. Against: the band is
wider than `delta`, as measured below. Each `f` is a max of pieces, not a distance.

**Decision: an extrusion's `g` is the profile's signed distance at the box centre, ± (half the
box diagonal + its error bound).** For: signed distance is 1-Lipschitz (a theorem, not a
per-primitive proof), and it is one double-double evaluation, tight to the box. Against: soundness
rests on the stated error bound `E = gap + 2^-90 (S + |c|) + 2^-52 |d|` in `profile.rs`, rather
than on interval rounding. The sign is the winding parity from double-double `atan2` sums. It is
checked against the exact crossing test, but it is not itself exact.

**Decision: the frustum's slant term is divided by `sqrt(1 + s^2)`**, so its width is a
distance's. For: before that, a flat cone's band reached about `|s|` widths. Against: one more
interval division (sound, and the divisor is ≥ 1).

**Measured** with throwaway timers (opt-level 3, as the testkit builds in dev):
- `side` costs about 3.1 µs a call on a four-leaf tree (block minus cylinder, plus frustum,
  plus sphere) over 100,000 points.
- `probes` gives 3,239 probes in 12 ms on that tree.
- In the recipe test, `census` at `delta = sqrt 3 · 1e-6` keeps at least 95% certain (asserted).
  The timing run's 100,000 points had no `Band` at all.
- Band reach, as the largest `|sd| / delta` over `Band` points at random rotated frames, scaled by
  `sin(θ/2)` for the sharpest edge angle θ: block 1.47, cylinder 1.72, frustum 2.01, sphere 2.69,
  torus 2.76.
  - Unrotated, the block's band stays within `sqrt 3 · delta` and the sphere's within 1.01 of it.
  - Rotation can grow the box's bounding box to `sqrt 3 · delta` a side, whose corner is
    `3 · delta` away.
  - A max of planes reads `d sin(θ/2)` beyond an edge of angle θ, so a flat frustum's rim widens
    the band by `1 / sin(θ/2)` (10.6 widths measured, unscaled). This is sound; it costs certain
    points only near sharp edges.

**Deviations from 72.**
- §3.4 says `Band` is "within a stated distance of the truth boundary". As built, `Band` *contains*
  every point within `delta`, but it reaches farther (above). The guarantee is one-sided, which is
  what row 7 needs.
- §8.1's classification row says band points "must classify `On` or the side T10 gives at the
  band's edge". T10 gives no such side. Row 7 supersedes this: band points are counted, not judged.
  DT1 should follow row 7.
- Extrusions of line-and-arc profiles are added, as the task names them, and the cone is a
  `Frustum` with `radius1 = 0`.

**Open.**
- *Three segment types.* `csg::Seg` (an angle-swept arc and a full circle), T9's `solid::Seg`
  and T11's `profile::Seg` (an arc by centre, two exact ends and a sense) each describe line-and-arc
  loops. Case for unifying on T11's: exact ends, already the fixtures' form. Case against now: it
  touches three tasks' files, and `csg` would derive angles by `atan2` anyway. Recommendation: one
  follow-up moves `csg` and `solid` onto `profile::Seg`, with `Circle` as two arcs.
- *Extrusion caps get no meet walks.* Meets of a cap with another leaf are found only from the
  other leaf's walks. Enough for the tests here; a cap-only meet (a pin tip on a lid) would be
  missed.
- *Profile normals assume simple loops.* An edge between pieces takes the next piece's normal
  with this piece's outward sign. A self-touching loop could flip it, and the probe is then
  dropped by the ±10c filter rather than kept wrong.
- *Band width per edge angle.* If DT1 finds the 95% share hard to keep near sharp edges, a
  meridian signed distance for the frustum (as the extrusion does) would remove the
  `1 / sin(θ/2)`.

### 14.3 T11 as built (11-27)

`scx-testkit::profile` (`crates/scx-testkit/src/profile.rs`, tests in `tests/profile.rs`).

- **Shape.** `Seg { Line, Arc }` of `[f64; 2]` points; `Fixture { name, outer, holes, resolution,
  verdict }`; `Verdict { Accepted { area, outer_reversed, holes_reversed }, Refused(Cause) }`;
  `Cause { Open { loop_index, gap }, Crossing, Touching, HoleOutside, HoleInHole }`. `fixtures()`
  returns 23 fixtures in a fixed order. PL1 adapts each with one function and compares outcomes.
- **Arcs** are centre, start, end and a sense, never an angle pair, so ends are exact data. Every
  coordinate is an integer or a power-of-two fraction and every arc is on a Pythagorean circle
  (3-4-5), so the gaps, touches and crossings are exactly as stated.
- **Kinds covered:** closed (square, D, circle, rounded top, closed within resolution); open by a
  hair (outer, at an arc end, a hole; gap 2^-20 against resolution 1e-9, and a 2^-40 gap that
  closes); touching (vertex on an edge, figure of eight at a vertex, arc tangent to a line, hole
  corner on the outer loop); crossing (bow tie, line through an arc); nested (square and round
  holes, two holes, hole outside, hole in a hole); reversed (outer, hole, both).
- **Verdicts are by hand**, from the picture in each comment, and checked for self-consistency by
  `self_check` against closed forms (shoelace plus circular segment, independent of any library):
  loops chain, arcs' ends are on their circle, declared gaps are the measured ones, areas and
  orientation flags agree. It does not decide Crossing or Touching; those stay hand-decided (each
  is one visible incidence).
- **Decision: a reversed loop is `Accepted` with a flag**, per 72 §4.3 (orientation is not the
  caller's error). The flags let PL1 check it reversed the right loops.
- **Decision: one cause per fixture.** Cases for a multi-cause list: more complete. Against: the
  library's order of checks becomes the oracle's, and a fixture would pin it. Each fixture is
  built to have exactly one fault, so the answer is order-free. The one exception is closure:
  an open loop is judged first (the other checks assume closed loops), and no refused-for-
  another-cause fixture is open.
- **Decision: area is in the verdict**, so PL1 can check the built region's area against truth.
- **Dependency on 72 §12:** the `Region::new` checks of §4.3 only (closure, no crossing or
  touching, holes inside and apart, orientation fixed up).

### 14.4 T13 as built (11-27)

`scx-testkit::mesh` (`crates/scx-testkit/src/mesh.rs`, tests in `tests/mesh.rs`).

- **Shape.** `Mesh { vertices: Vec<[f64; 3]>, triangles: Vec<[u32; 3]> }`, outward right-hand
  windings; `violations(&Mesh, Limits) -> Vec<Violation>` (empty is a pass); `volume_gap(&Mesh,
  reference, bound) -> VolumeGap` with `holds()`; `chord_bound(chord, area)`.
- **Violations:** `IndexOutOfRange`, `NonFinite`, `Boundary` (an edge used once), `NonManifoldEdge`
  (more than two uses), `Orientation` (two uses in the same direction), `NonManifoldVertex` (the
  triangles around a vertex do not form one fan), `Degenerate`, `Fold`. Bad indices and
  non-finite data return alone, since the rest cannot be computed.
- **Gap bound.** `|V_mesh - V_ref| <= chord * area`: the mesh differs from the solid only in a
  shell of the surface's area and at most chord thick. Tested on a subdivided octahedron with
  vertices on a sphere (chord = the largest edge-midpoint sagitta): the bound holds at each of 4
  refinements, the gap shrinks, and a bound 100x tighter fails. The caller states the bound; the
  oracle does not pick one.
- **Decision: fold means neighbouring normals opposed** (dot of unit normals below `fold_cos`,
  default -0.99), not 72 §6.4's "normal within 90 degrees of the surface normal", which needs the
  surface. The plain-mesh oracle cannot see a surface. Cases against: a fold within one face's
  flat region is caught only across an edge; a slow whole-mesh inversion is caught by the signed
  volume instead. The surface-normal and on-surface checks of §6.4 stay as TS3's own tests
  (they need the surface); the oracle covers the surface-free ones.
- **Decision: degenerate by area ratio** (doubled area below `min_area_ratio` of the longest edge
  squared, default 1e-9) as well as exact repeats, so slivers are found. `0.0` flags only zero area.
- **Volume** is summed with Neumaier compensation about the vertex mean, so a mesh at 1e7 keeps
  its volume to 1e-8 in the test.
- **Not covered:** self-intersection of non-adjacent triangles (a test needs a triangle-triangle
  intersection oracle; deferred to TS3 if wanted), and multiple shells or genus (the oracle
  takes the whole mesh as one surface; `NonManifoldVertex` is per vertex).

### 14.5 CX1 as built (11-27)

`scx_brep::op` (D1), `crates/scx-brep/src/op.rs`: `Cx`, `Settings`, `Scratch`, `Trace` (with `Event`, `NoTrace`), `Warning` (with `WarningKind`), `Raise`, `Skipped<C>`, `Report<C>`, `Done<C>`. All plain types; `Settings`, `Scratch`, `Report` and `Done` are `Send + Sync` (asserted).

- **`Cx` is a bundle with public fields and `Cx::new`.** A caller destructures it and passes `&mut Budget` and `&Stop` (mass, classification, tessellation) or `&mut Scratch`. `Cx::budget()` gives a fresh `Budget` from the settings' limit.
- **`Settings`** is `#[non_exhaustive]` with `new()` (default seed, unlimited budget), by-value `with_seed` and `with_budget`, and `rng(op, entity)` = `Rng::new(mix(&[seed, op, entity]))`. It has `Default` because every value is valid, so it is not a failing default. No executor field: the executor lives in `scx-algo`, above `scx-brep`; it joins `Settings` when an operation takes one (change-if).
- **`Scratch`** is a `BTreeMap<(TypeId, u64), Box<dyn Any + Send + Sync>>`: typed `get` and `insert`. Case for the typed map: no crate above needs to be named here, and iteration is in key order. Case against: `dyn Any` hides what is memoised. Kept because 01 §6.3 says only "memo", and each user crate can wrap its keys. It does not charge work on a hit; the caller stores the work with the value.
- **`Report::new`** canonicalises: warnings by (entity, kind), skipped by (item, round), raised by entity; equal keys keep their given order (stable sort). The fields stay public per 01 §5.2, so a hand-built `Report` can bypass it; `Report::empty(work)` is the clean one.
- **`Warning`** is `{ entity, kind: WarningKind }` with `AccuracyRaised` and `Coincident` (the 12.4 row 4 cross-operand identification). **`Raise`** is `{ entity, before, after: Accuracy }`; 09's `cert` field is left out until `Cert` is wanted by a consumer.
- **`Done::new`** is `const`; chaining is `op2(&done.body, ...)`.
- Tests: construction and splitting a `Cx`, chaining three `Done`s, seed draws fixed by inputs, `Report` order independent of input order.

### 14.6 PL1 as built (11-27)

`scx-planar` (`crates/scx-planar`, layer after `scx-curve`; depends on `scx-num` and `scx-algo`).
Tests: `tests/fixtures.rs` (T11 through one adapter), `tests/pin_ports.rs` (the pin's
`prism/tests.rs` line-and-arc cases as outcomes), `tests/profiles.rs`, and unit tests in `cert`.

- **Shape.** `#[non_exhaustive] enum Segment { Line { start, end }, Arc { centre, start, end,
  sense: Sense } }` with `line`, `arc`, `start`, `end`, `reversed`. `Loop` (closed, oriented, by
  construction) with `segments` and `signed_area`. `Region::new(outer: Vec<Segment>, holes:
  Vec<Vec<Segment>>, res: Resolution) -> Result<Region, PlanarError>`, with `outer`, `holes`,
  `resolution`, `outer_reversed`, `holes_reversed`, `area` and `classify(p) -> Result<Place,
  PlanarError>` (`Inside`, `Outside`, `Boundary`).
- **Errors.** `PlanarError` (`non_exhaustive`), one cause per refusal: `NotFinite`, `OutOfRange`,
  `Empty`, `Open { in_loop, joint, gap }`, `Degenerate { at, why: Short | Radius | EndOffCircle }`,
  `Crossing { a, b }`, `Touching { a, b }`, `HoleOutside`, `HoleInHole { outer, inner }` are
  `Refused`; `Undecided { in_loop }` (an interval test that could not decide) is `Unsupported`.
  Segments are named by `SegmentRef { in_loop: LoopId, segment }`.
- **Order of checks.** Numbers (finite, in range) and empty loops; closure; degenerate segments;
  contacts; nesting; orientation. The first failing check is the cause, and within contacts
  `Crossing` beats `Touching` and the least `(a, b)` pair is reported, so the verdict does not
  depend on loop order (tested over rotations, reversal and permuted holes on every fixture).
- **Results.** All 23 T11 fixtures give their verdict, accepted areas within 1e-12 relative and
  the reversal flags as stated; the Open fixture's gap is reported exactly.
- **Decision: segments are endpoint data, not 72 §4.3's `Arc(Circle2, Extent1d)`.** For: joints
  are then exact points that two segments share bit for bit (closure and adjacency are equalities
  after snapping), T11's `Seg` maps one to one, and no angle is ever rounded into the input.
  Against: a consumer wanting the circle and the parameter range has to derive them (one atan2 per
  end), and MK3 will want `AngleLine2` pcurves. The derivation is cheap and the input stays exact;
  a `Circle2` view can be added when MK3 needs it.
- **Decision: reuse `scx_num::frame::Sense`** for an arc's direction (`Forward` is
  counter-clockwise). For: one direction type in the workspace. Against: `ccw: bool` reads more
  plainly; but a bool invites the wrong default.
- **Decision: NotFinite and OutOfRange before Open.** "An open loop is judged first" holds among
  geometric causes; a NaN makes every later test meaningless, including the gap. Against: a NaN in
  an open loop reports NotFinite, not Open. Tested (`the_checks_run_in_order`).
- **Decision: closure snaps.** A gap proved at most the resolution (interval test on `gap^2 <=
  res^2`) is closed by moving each segment's end onto the next segment's start, so later tests see
  exactly shared joints. For: adjacency is then an equality, and the joint's contact is not
  re-decided by tolerance. Against: an arc's end moves by up to the resolution off its circle; the
  EndOffCircle test allows exactly that.
- **Decision: contacts are decided by events, certified.** Lines use an exact orient2d (Shewchuk's
  filter, then 16 error-free products summed as an expansion); arcs use interval arithmetic with
  one-ulp outward rounding over a radius band (the hull of the two end distances). A pair is
  Crossing only when a proper crossing is proved, Apart only when every event (end distances,
  meeting points, the critical pairs on the line of centres) is proved more than the resolution
  apart; anything else is Touching. No fixed epsilon: the only length is the resolution.
  Against: an undecidable near-tangent case is refused as Touching rather than accepted; this is
  the conservative side, and such profiles are within the resolution of touching anyway.
- **Decision: a joint neighbourhood for adjacent segments.** Events within `rho = 4 sqrt(R_max
  res) + 4 res` of a shared joint are ignored, since two segments tangent at a joint (a fillet, a
  slot) come within the resolution of each other over a length of about `sqrt(R res)`. For:
  tangent joints are accepted (rounded rectangle, slot, S-bend tested). Against: a real fold-back
  shorter than `rho` beside a joint is missed; a fold along a whole segment is still found
  (`collinear_edges_apart_are_apart`'s spike).
- **Decision: concentric only when the centres are bit-equal.** Arcs on one circle then overlap by
  mid-direction tests (four quarter arcs accepted, a retraced arc Touching). Distinct but
  near-equal centres go through the general path, which may answer Touching where the truth is
  Apart by less than the band. Against: an exact-but-different representation of one centre is
  conservative, never wrong.
- **Decision: coordinates are 0 or of magnitude in `[1e-100, 1e9]`** (`OutOfRange` otherwise), so
  every `two_prod` in orient2d is error-free and the line tests are exact. Against: a profile in
  metres at 1e-100 resolution is impossible anyway; 1e9 mm is 1000 km.
- **Decision: `classify` answers `Boundary` within the resolution** of any segment (certified),
  else the winding number decides. The pin's `holds` returned `None` only exactly on the loop; here
  a point within the resolution is not decided either way, which is what the resolution means.
  The pin ports use a resolution below each case's distance.
- **Decision: winding by angle sum**, not ray casting. Each arc adds its chord's angle plus a full
  turn when the point is in the circular segment, read from the same float cross that atan2 reads
  so a point on the chord line is consistent. No ray means no half-open vertex rule.
- **Decision: orientation by signed area with a bound.** Shoelace about the first vertex plus
  `r^2 (t - sin t) / 2` per arc (series below `|t| = 1e-2`), compensated, with a bound of `16 eps`
  times the sum of the terms' magnitudes and the arcs' angle sensitivity. A loop whose area is
  not proved non-zero is `Undecided` (a region of zero area is degenerate in practice).
- **Decision: candidate pairs from `scx-algo`'s box tree** (`BoxTree<Extent2d>::self_pairs`), not a
  sort-and-sweep and not brute force. Measured (release, throwaway timer, medians of 7, whole
  `Region::new`): 1000-gon 0.7-1.1 ms, 10000-gon 6.9 ms; comb of 1001 / 10001 segments 0.60 /
  6.4 ms upright and 0.61 / 6.4 ms on its side; 2500 round holes in a square 5.0 ms. A sweep on `x`
  was the same or slightly faster upright (comb 10001: 5.3 ms) but 64 ms on its side, where every
  box spans the same `x` (`O(n^2)`); brute-force box pairs were 53-62 ms at 10000 segments
  everywhere. The tree is `O(n log n)` to build and its self-join follows the overlapping pairs in
  both axes, so no orientation of the profile is a worst case. Nested holes use the same tree.
- **Complexity, open:** the hole-inside-outer test is one winding number per hole over the whole
  outer loop, `O(h n)`: a 1000-gon with 2388 round holes takes 47 ms (with 488 holes, 10 ms).
  One point per hole suffices because contacts are already excluded. A ray test against the
  tree would make it `O(h log n)`; not done, since profiles of thousands of holes in a
  thousand-segment outer loop are rare.
- **Decision: no segment-count limit.** The costs above are near linear; a limit would refuse
  valid input.
- **Deviation:** 72 §4.2's `Unsupported { segment }` for ellipse and NURBS segments is not a
  variant: `Segment` has no such kinds yet and is `non_exhaustive`, so step 5 adds the variants
  and their refusal together.
- **Open:** a full circle as one arc (start equal to end) is refused as `Degenerate::Short`;
  T11 and the ports use two half arcs. Accepting it needs an explicit full-turn flag on `Arc`.
- **Pin cases not ported** (not line-and-arc regions or not this layer): chord coordinates;
  ellipse, conic, Bezier and hyperbola areas and holds (step 5); the distance query; placement
  between regions; readers, sheets, prisms, arc building, circle and ellipse recognition.
- **Removed tests:** none.

### 14.7 MK1 as built (11-27)

`crates/scx-make` (new, in `layers.txt` after `scx-record`; depends on `scx-brep`, `scx-curve`, `scx-num`, `scx-surface`; `scx-testkit` as a dev-dependency). Public API:

```rust
pub fn block(frame: &Frame, size: [Length; 3], doc: &Document) -> Result<Made<BlockFaces>, MakeError>;
pub struct Made<R> { pub body: Brep, pub faces: R }
pub struct BlockFaces { pub bottom: FaceId, pub top: FaceId, pub sides: [FaceId; 4] }  // Copy
#[non_exhaustive] pub enum MakeError { Refused(Refusal), Defect(Defect), Full(TableFull) }  // kind(), name(), Display, Error
#[non_exhaustive] pub enum Refusal { Size { dimension: Dimension, value: f64, floor: Length } }
#[non_exhaustive] pub enum Defect { Curve(CurveError), Absent(Absent), Interval, PlaneFrame, Pcurve(PcurveWriteError), Finish(FinishCause) }
#[non_exhaustive] pub enum Dimension { X, Y, Z }
```

**What it does.** The block has its corner at the frame's origin and extents along the frame's axes (the placement T9's `solid::block(..).place(&frame)` answers for; the pin's `create_box` also puts its lower corner at `origin`). It writes 8 vertices, 12 edges and 6 planar faces into one `Draft`: each edge once, each face's ring counter-clockwise about its outward normal, front in the outside's shell and back in the solid's, both regions decided at construction, pcurves from `Draft::exact_loops` (BR5, every `bound` `None`), then `finish`. No stitch, no orientation pass, no ray fire. Vertices are `origin + (x dx + y dy + z dz)`, so each rounds once.

**The pattern MK2 reuses.**
- A `const` table of roles over corners (`Role { ring, axis, outward }`) and a record struct of `FaceId`s in the table's order. `BlockFaces` documents its order (bottom, top, sides counter-clockwise from the face on `y = 0`).
- `assembly::Assembly` (crate-private): `new(doc)` (outside shell, solid region and shell), `vertex(point)`, `line(a, b)` (shared edge by end ids, with the sense of a use from `a`), `edge(curve, interval, ends)` (an unshared curved edge), `face(surface, &[loops of (EdgeId, Sense)])` and `finish()`. MK2 adds a collapsed-edge method (poles, apex) and seam edges to it; nothing else in it is block-specific.
- `Made<R>` is the S6 shape. No `Changes` is returned: a new body has no inputs.
- `MakeError` is `#[non_exhaustive]` and has no `Unsupported` yet because no cause exists; MK2 adds `Unsupported(..)` with `HornTorus` (D3) and `Dimension` gains `Radius` and the like.

**Decisions, with cases.**
- **The size floor is the document's zone, strictly above.** For: 72 §4.2 says "greater than the document's resolution", a box whose size is within the zone has its two ends one place, and `finish` would refuse it as a `Defect` where a `Refused` names the size. Against: the pin's floor is a constant (`SM_EFF_ZERO`), so a size between the two is a different answer at a coarse document. Kept: the answer follows the document, and the test pins both sides at the default and at a 0.01 mm document. NaN and infinity are refused with the same variant (a typed `value`, not a string).
- **`Defect::Finish` carries the `FinishCause`, not `Box<Unfinished>`.** For: `Unfinished` holds the whole draft (not `PartialEq`, large; `result_large_err`), and a constructor's own draft has nothing for a caller to repair. Against: the draft would help to debug. A `Defect` is our bug and the test suite has no case of it; the cause names the lowest failing entity.
- **A straight edge is `origin + t (q - p)` over `[0, 1]` from the lower vertex id to the higher.** Same as the testkit generators. Against: a unit-speed line would read lengths off the interval. Kept: the line is exact with no rounding of a direction, and `Line::through` is the type's own form.
- **Vertices are created on first use, in the order of the role table**, so ids are the same on every run and no table is indexed (the workspace denies indexing). Asserted by a bit-for-bit repeat.
- **No result contract run.** S8: ends at `finish`.

**Pin tests.**
- `boxes_far_from_the_origin_build_closed_and_valid`: ported with all 17 rows (sides 2e-4 to 5e3 at 1e3 to 1e12). Each asserts the 6/12/8 counts, two regions and shells, two opposite fins on each edge, front in the outside shell and back in the solid's, every pcurve a `Line2` with no bound, every vertex a corner within 4 ulp of the offset, and the volume and area (from the divergence theorem over the faces, about the frame origin, as there is no mass crate yet) against `solid::block`. The `check_brep` Level2 verdict is replaced by `finish` plus these. Not ported: the pin's second half, the quarter `create_cylindrical_box` rows at 1e3 to 1e8. A partial primitive is `Unsupported` by name until a caller needs it (§4.2). It is a removal on purpose: the rows proved that a cylinder's tangent test did not scale with the corner's position, which belongs to MK2's cylinder at a far frame (one row there).
- `small_boxes_at_1e4_answer_as_at_the_origin`: ported; cubes of 1e-4 and 5e-5 at 1e4 answer as at the origin, and both answers are a built 6/12/8 body (the pin only said "the same answer", which a shared refusal would also satisfy).
- `api_primitives`' size refusals: for the box, `invalid_inputs_are_rejected_before_the_kernel`'s box line becomes `a_size_that_is_not_finite_and_above_the_resolution_is_refused_naming_it` (zero, negative, NaN, both infinities, the zone itself and half of it, each on each axis, the variant naming the dimension, the value and the floor, kind `Refused`, name `size`). The sphere, cylinder, cone, pyramid and torus refusals (`a_sphere_radius_not_above_zero_is_refused_naming_the_radius`, `a_torus_whose_tube_crosses_its_axis_is_refused_naming_the_radii`, the horn torus one, `validators_match_upstream`'s cone line) belong to MK2, which owns those constructors. The pyramid is not in the step-4 constructor list (§4.2), so its refusal is not ported.
- Added, with no pin counterpart: `the_size_floor_is_the_documents_resolution`; `a_rotated_block_matches_the_closed_form_and_its_roles` (a skew frame with unequal sizes: the eight corners, each face's outward normal and plane offset by role, volume, area and centroid against `solid::block(..).place(..)` to 1e-13); `every_pcurve_lies_on_its_edge` (plane of the pcurve at t equals the edge's curve at t, and a use's ends are its vertices); `making_is_deterministic`.

**Measurements** (throwaway timer, not committed; release build, 20,000 repetitions after 2,000 warm-up, a 2 x 3 x 4 block at the world frame, build plus `finish`): **18.3 us per body**, the same on two runs. The build/finish split and the pin's `create_box` were not measured. Added work against the pin: none (the pin stitches and fires a ray; this does neither).

**Deviations from 72.** None in signature (§4.3 gives `block(&Frame, [Length; 3], &Document)`). `MakeError::Defect` carries a cause where §4.2 says "carrying the `Unfinished` report" (above).

**Open questions.** (1) Whether `Defect::Finish` should keep the draft for a debugging build. (2) `Dimension` as an enum of axes grows a variant per constructor's size; a `&'static str` would not grow, but would be untyped, and MK2 will say which it wants once it has seen five constructors' sizes.

**Proposed 04 row.** "`scx-make` (MK1): constructors take a frame and `Length` sizes, are built with their topology known and return `Made<R> { body, faces }` with a typed face record; a size must be finite and above the document's zone or it is `Refused { Size }`; the result's only check is `Draft::finish`, whose refusal is a `Defect`. Change if: a caller needs the draft after a defect, or a constructor needs a size the zone does not bound."

### 14.8 MA1 as built (11-27)

`crates/scx-mass` (layer 5, after `scx-record` in `layers.txt`): `volume`, `area`, `face_area`, `mass` and `edge_length`, each taking the body, face or edge, the accuracy, `&mut Scratch`, `&mut Budget` and `&Stop`; `MassSpec`, `Mass`, `MassErrors`, `Estimate`, `Sym3`, `RelAccuracy`; `MassError` with `MassRefusal`, `MassUnsupported`, `MassDefect`.

**The method.** The pin's `boundary_properties`: Green's theorem in each face's uv, `∫∫ F du dv = ∮ -G du` with `G = ∫_base^v F ds`, the outer integral over each trim piece by the 7-point Kronrod rule and the inner by the 15-point rule, each held to a tenth of the accuracy. Pieces are split at a NURBS trim's knots and where a trim crosses a surface break line (an analytic kind's arc joins, `scx_curve::arcs::Arcs`; a NURBS surface's knots), found by 16 samples and bisection. The inner integral is split at the surface's v breaks. The integrand set is the pin's monomials: area and its first moment, then `x^a y^b z^c` with `c >= 1` up to degree 3 (divergence form along z), 20 values, so volume, first and second moments come from one pass.

**Decisions, with cases for and against.**

- **Kronrod is private to `scx-mass`, not `scx_algo::ode`** (deviation from the §2.5 row). For: the pin's Gauss-Kronrod is adaptive quadrature, not an ODE; `scx_algo::ode` is the Runge-Kutta integrator, and putting a vector-valued adaptive Kronrod with a per-component floor there would add an API with one caller. Against: a second numerical integrator in the workspace. Change if a second crate wants adaptive quadrature; then it moves to `scx-algo` as is.
- **`RelAccuracy`, not `scx_num::units::Accuracy`** (deviation from §2.3's sketch). `Accuracy` is a length; the pin's mass accuracy is relative (each piece held to `accuracy/10` against the larger of its own value and its share of the face's scale). For: a length would need a size to mean anything, and the size differs per component (an area, a volume, a moment of power 5). Against: one more accuracy type. `RelAccuracy::new` refuses anything not finite or outside `(0, 1]` (`BadAccuracy`), where the pin clamps.
- **`density: f64` (1 unless given), not `Option<f64>`.** For: `None` meant unit density, and a plain value with a `with_density` is the same thing without a case to read; a bad one is `BadDensity` when measured. Against: none found.
- **Every result carries its error estimate** (`Estimate { value, error }`, and `Mass::error: MassErrors` field for field), instead of §2.3's `reached: Accuracy`. For: the estimate is what the integrator has; one `reached` per result would either be the worst component (hiding where it is) or need a size to be relative to. Against: more fields. The estimate is the Kronrod-Gauss difference summed over the pieces, an estimate, not a proved bound; the tests check that it covers the actual miss everywhere they measure one.
- **`Mass` holds integrals, not derived values.** `Mass { about, density, volume, first, second, area, area_first, error }` at unit density, with `mass()`, `centroid()`, `area_centroid()` and `inertia()` (centroidal, times the density) as methods returning `Option` where the volume or area is zero. For: a sheet or an empty body then has no made-up centroid, and the moments about `about` are kept exactly for callers that shift them. Against: §2.3's flat `centroid`/`inertia` fields are one call further away.
- **`Exhausted { limit }`, not `{ work }`.** The budget's own error carries the limit; the work done is `budget.used()`, which the caller holds.
- **The face reference is the face box's middle**, rounded to the grid `binade(d)/16` (`d` the box's largest side, `binade` read from the bits so it is exact), not §2.2's uv box image centre. For: the uv box's image can lie far outside the face (a cone's apex region, a torus sector), and the box is recorded on the face. Against: a box is not intrinsic to the surface. Each face is integrated about its reference, its floors from its own box, and shifted to the body's reference by the exact binomial expansion; the body reference is the body box's middle on the same grid, or `MassSpec::about`.
- **The volume sign of a face is `[back solid] - [front solid]`** from its shells' regions, so a void's walls, a cavity and a mirror count without a separate orientation step; a face between two solids or two voids is skipped. The area counts every face not between two solids (sheets and void walls included). This is the region-based sign of a finished `Brep`, not MA4's certified `volume_sign`, which stays MA4's.
- **No `OpenShell` refusal.** A finished `Brep` cannot hold a face whose sides disagree or a solid region that is not closed (`finish` refuses `SidesDisagree`), so there is nothing to refuse; a sheet has its area and zero volume.
- **Joins and windings.** Point uses (poles) are skipped and the next piece is joined by a straight uv path; a gap of whole periods (a seam) is no path. A remaining gap is joined by a straight path in `face::join`, the hook where MA2 closes onto the edge's curve for uses with a recorded accuracy. A loop whose v winds by more than half a period (`VWrap`), or loops whose net u winding exceeds half a period (`NetWinding`, a face round a closed spline with no seam trim), are `Unsupported`, not guessed.
- **`Scratch` is taken and unused** until MA5's memo.
- **Not ported:** the pin's banding of a long trim into twelve strips (`MIN_BANDS`, only a split of the work; the step control bisects where needed), the edge tube terms of `edge_properties` (no caller), `PropertyAttributes` (per-entity attribute masses, no caller), `AreaAbout`, and the prepared (non-mutating) twins (every read here takes `&Brep`).

**Tests** (25: 8 unit, 17 integration; about 10 s in debug, most of it the knot-span and random-frame cases):

- `kronrod` (5): rules exact to their degrees, a constant exact, the estimate covers the error, a non-finite integrand named, stop and budget per piece. `integrand` (1): the shift equals the expansion. `breaks` (2).
- `tests/truth.rs` (4), against `scx_testkit::solid` (T9): every generated kind (with the checker and touching boards: blocks meeting only along edges, as separate regions and as one) at sizes 1 to 3 at the world frame to 1e-9, and at four random rigid frames (origins to 10) to 1e-8: volume, area, the three first moments and all nine second moments, each within the accuracy of its size and each estimate covering the miss; `volume`/`area` against the closed forms at 1e-10; and the curved kinds at every accuracy 1e-2 to 1e-10 (the port of `precise_area_holds_its_accuracy`).
- `tests/knot_spans.rs` (3), the port of `issue_2091`: a block whose top face is a degree-2 NURBS surface with the same boundary, a bump C0 at a double v knot and C1 at a simple u knot; the volume at 1e-9 to 1e-12 against its closed form (the B-spline integrals), the area against tensor Gauss-Legendre span by span, and the centroid moving up.
- `tests/behaviour.rs` (10), the behavioural inline tests and `issue_16735`/`edge_properties`: a 100 x 100 x 2 plate exact; a 1 x 2 x 3 box's centroidal inertia at density 2.5; a radius-2 sphere's; a mirrored cone keeps its volume and mirrors its moments; `a_box_has_no_leak` (4 x 5 x 6 at three origins out to 1e6, about three points); sheets have area and no volume; the refusals by name; stop and budget; the same bits twice; edge lengths of arcs and segments at 1e-4 to 1e-11 at origins out to 1e6.

**Removed or left, on purpose** (pin test, what it proved, why not here):

- `issue_2091`'s `piece_end_is_evaluated_in_its_own_span` and `piece_steps`: the Runge-Kutta piece's last point was taken from the next knot span. The Kronrod nodes are interior, so no piece end is ever evaluated; there is nothing to test. Its multi-span patch is ported as the bump above, non-rational (the pin's is rational); its loft (`create_skin_primitive`) has no counterpart yet.
- `precise_area_holds_its_accuracy`'s quarter annuli come from a rotational sweep, which this workspace does not have; the accuracy sweep runs on the curved kinds instead.
- `issue_383_tangent_torus_volume`: a Boolean of tangent tori; waits for BO.
- `issue_16735`'s strip tests (`a_cubes_strips_close_in_one_piece_per_edge_use`, `an_edge_off_its_trims_...`, `a_cylinders_strips_keep_the_grid`, the corpus parity): the gap closure's strips, MA2's.
- `edge_properties_hold_their_accuracy`'s tube components: the tube terms are not ported (above); the lengths are.
- Inline tests of pin internals (`vector_attribute_subclasses_...`, `trims_and_caches`, `prepared_measures_*`, `face_area_makes_missing_uv_trims_...`, `a_missing_link_...`, `trims_a_period_round_a_closed_spline_...`, `a_correspondence_keeps_only_its_bends`, `properties_terms_match_the_total_and_refuse_a_shell_vertex`, `an_unsure_region_is_refused_by_name`, `brep_area_about_a_point_or_each_face`): attribute stores, lazy trims, prepared twins and region states this crate does not have. The partition, fin and merged-cube area rules are the `counts_in_area` rule, exercised by the cavity and stack kinds.

**Difftest** `t_mass`: the seed-0 block, cylinder, cone, sphere and torus against the pin's `brep_volume_default`, `brep_area` and `brep_mass_properties_default` at 1e-6, 1e-9 and 1e-11, each side judged against the closed form (both must hold the accuracy; the misses are printed side by side), 0 mismatches.

**Cost against the pin** (`volume` only, release, local machine, best of 5 x 200 calls, throwaway timers; seed-0 bodies against the pin's primitives of the same solid, face counts equal). Per face, ours against the pin's: block 4.6 against 70.6 us at 1e-6 and 1e-9; cylinder 74 against 1,641 us (1e-6) and 287 against 3,682 us (1e-9); cone 125 against 1,571 and 479 against 3,866; sphere 155 against 6,622 and 2,357 against 62,113; torus 29 against 15,452 and 72 against 57,446. So 0.004 to 0.12 times the pin's time. The work charged to the budget is one unit per integrand evaluation (798 for the block; 50,344 for the sphere at 1e-9, the costliest: the poles' pieces). The difftest misses are all within 2.3e-12 of the closed forms, both sides (`t_mass` prints them).

**Open.** Whether `volume_sign` (MA4's certified sign) should replace the region sign used here once it exists; whether `Kronrod` should move to `scx-algo` when a second caller appears; the rational multi-span patch of `issue_2091` and the quarter annuli wait for a loft and a sweep.

### 14.9 T12 as built (11-27)

**What.** `crates/scx-testkit/src/recipe/` (`mod.rs`, `make.rs`, `boolean.rs`, `convex.rs`) and `crates/scx-testkit/tests/recipe.rs` (21 tests, 16 s in debug, nearly all of it the Monte Carlo test). Plain data, no library type. `recipe::all(resolution) -> Vec<Recipe>` returns 68 recipes in a fixed order: 29 constructors, 39 Booleans; outcomes 42 built, 12 refused, 13 unsupported, 1 open.

- **`Recipe { name, source, base: Part, steps: Vec<Step>, outcome, truth, lineage, invariants }`.** A `Part` is a `Shape` (block, cylinder, cone, sphere, torus, extrusion of a `profile::Seg` loop with holes) at a `Frame`; a `Step` is `Op` and a `Part`; the Boolean is the left fold of the steps over the base. `Part::solid()` gives T9's closed form at the frame; `Part::csg()` and `Recipe::csg()` give T10's tree. A constructor recipe has no steps.
- **`Outcome`:** `Built(Topology)`, `Refused(Refusal)`, `Unsupported(Unsupported)` or `Open(reason)`. Causes by name: `Refusal::{Size(Dimension), NoBody, SelfIntersecting, Span}`; `Unsupported::{HornTorus, SsiPair { kinds }, NonManifold { at }}`. No `Defect` and no `Exhausted`: no recipe is meant to give them.
- **`Truth { volume, area: Option, slack }`.** From T9 (`solid`, `plate`, `gear`, `torus_sector`, `sphere_cap`) and sums of them; where the Boolean overlaps (box-box-union and its siblings) by hand from the overlap's volume and area. It is stated for `Unsupported` recipes too, as the body the lifting step will build.
- **`Topology { solid_regions, shells, genus, cells }`** with `Cells { vertices, edges, patches: Vec<PatchClass { kind, loops, genus, count }>, cubic }`: the patch complex of the module docs (seams are not edges; a patch is a 72 A12 maximal smooth patch). `euler_defect()` is `V - E + sum (2 - 2 g - L) - 2 (S - G)`; `cubic` says every vertex has three edges.
- **`Lineage { faces: Vec<FaceOrigin { area, origins: Vec<(operand, BoxFace)> }>, new_edges, halved_edges }`** for the four two-block `issue_1462` recipes (union, difference, intersection, the flush union), `BoxFace` named by plane in `BlockFaces` order (`Bottom`, `Top`, `Front` y=0, `Right`, `Back`, `Left`).
- **`Invariant`**: `Valid`, `Manifold`, `Volume`, `Area`, `Topology`, `Classification`, `Mesh`, `Deterministic`, `Lineage`, `NoDanglingVertices`; each recipe lists what applies (built: the common set plus area and lineage where stated; the crease recipes add `NoDanglingVertices`; the two pinched bodies drop `Manifold` and `Mesh`; refused and unsupported recipes: `Deterministic`).

**The recipes.**
- *Constructors (29):* block (plain, far and turned, small at 1e4 in units of the resolution, twice the resolution), cylinder (two frames), frustum both ways, apex both ways, sphere, torus, extrusions of T11's square, D, rounded top and square-with-square-hole; refusals: block size zero, negative, NaN, at and below the resolution, cylinder radius zero and infinite height, sphere radius negative, torus minor zero, cone with both radii zero, spindle torus, zero span; `Unsupported(HornTorus)`.
- *`boolean_api` (17, not 15):* box-cyl-hole, box-box-union, sphere-and-box, coplanar-union, torus-minus-box are **built**. The six `NonManifold` (tangent-sphere-box, seam-sphere-box, seam-box-sphere, off-seam-sphere-box, seam-sphere-sphere, seam-box-minus-sphere) are each a touch inside a face, with the contact points listed. The six `SsiPair` are tpipe (cylinder, cylinder), the two cylinder-sphere seams, torus-sphere and the two torus-box seams (plane, torus).
- *Coplanar and lineage (10):* the flush union (the merged-caps case of `issue_1462`), box-box-difference and -intersection, and five `coplanar-offset-*` fronts at 0, 0.1, 0.5, 4 and 10 resolutions; the first three merge to a 6-face box, the last two stay apart (10 patches).
- *`boolean_no_dangling_vertex_refs` (3):* the crease at 0.3 (`Open`), 3 and 10 resolutions.
- *Topology cases (6, new):* corner contact and a line contact (a diamond prism whose edge lies on a cube's face), two disjoint cubes, a cube with a cavity, a block and a cylinder cut by their own copy (empty).
- *Benchmark bodies (4):* `plate-4`, `plate-12` (144 holes), `gear-60`, and the interleaved-grid unions at n = 4 and 8 of `boolean_coplanar_merge`.

**Checks of the data** (`tests/recipe.rs`), each an independent route:
1. Euler-Poincare on every stated cell count (under both test resolutions), and `3 V = 2 E` where `cubic`; a prism's counts from its plan polygon (`2 n, 3 n, n + 2`); and for all-planar cubic bodies the edge count recomputed from patches, loops, shells and genus.
2. Closed forms against textbook formulas written out again (every constructor, every Boolean that has an area), against SMLib's recorded volumes in the pin's table at 1e-6, and the T-pipe's quadrature against the pin's numerical integral (1e-9).
3. **T10's oracle against T9's truth on the whole Boolean tree**, by Monte Carlo in the tree's bounding box at 5 sigma (plus the band's share and a floor of one point in n at a share of 0 or 1): 59 recipes with a truth. Sabotaging a patch count or a volume fails the Euler, the closed-form and the oracle tests.
4. Contact points lie in `Band` of both operands (the enclosure over a 1e-9 box holds zero), 11 points.
5. The clipping route (`recipe::convex`): the overlap of two blocks by Sutherland-Hodgman, exact against the closed form for 400 random aligned pairs (volume, area, union), against the oracle for turned pairs, and it supplies the crease recipes' truth. Used only where no two faces are coplanar (it returns `None` otherwise).
6. Lineage: each operand face is the origin of at most one result face, the surviving faces number 12, 12, 9, 6 as the pin's tests have them, the areas sum to the truth's area, and the face list is as long as the patch count.
7. The refused recipes' inputs really are invalid in T9 and T10 (or at or below the resolution), the profiles equal T11's fixtures, and `frame_turned` gives right-handed orthonormal axes.

**Decisions** (cases for and against).
- **A recipe is data with no operation, so a check is a function of the answer.** For: the DT and BO tasks compare their output with `Outcome`, `Truth`, `Topology` and `Lineage` and need nothing from the testkit but the data. Against: the harness that runs a `Recipe` through the pin is DT1's, so a recipe's `Part` has two readers' conversions to write. Kept: there is one conversion each, and the pin's primitives are axis-aligned with a placement afterwards, so the pin's side is a transform of the same parts.
- **Counts are patch counts of a patch complex, with Euler checked on them.** For: the only count that is truth (72 A12), and the cell counts give the consistency check the task asks for. Against: hand counts are one more thing to get wrong. Mitigation: every one passes Euler (V, E, F, loops and genus are redundant), and the grids' 66 and 226 patches equal the pin's face counts, which were not used to derive them.
- **Pinches are counted per sheet.** The two touching cubes have 16 vertices, not 15: Euler's relation then holds with `S` = 2. For: it keeps one relation for every recipe. Against: DT1 must count vertices the same way for those two recipes. DT1 does not compare vertex or edge counts, only patches, genus, regions and shells.
- **`Open` for the sub-resolution crease.** For: at 0.3 resolutions the crease touches the cube's edge and the merged-points row may build, refuse or identify, and a recipe must not claim what the plan has not decided. Against: one recipe with no claim is a weak test. Kept: its invariants still apply if built. Likewise the near-coplanar fronts at 0.5 resolutions state `Built` (they merge) with a slack on the volume of 1000 resolutions, since the merged body differs from the exact union by the gap times the area; the pin's own threshold (5e-9, a fraction of its zone) is not scx's.
- **Contacts built, and the empty body as `Built` with no region.** For: D5 says a vertex and line contact is built (BO7, BR10); the empty result has to have some representation. Against: both depend on rows not yet built. If BO4 returns a typed `Empty`, `block-minus-itself` and `cylinder-minus-itself` change in one place each.
- **`Unsupported::SsiPair` names a pair of kinds, not an order.** The library's dispatch is asymmetric (the plane second, the first surface keys it), so recipes give the kinds in `Kind` order and a test says compare as a set.
- **Resolution-dependent recipes take the resolution as a parameter of `all`.** Sizes near it are in its units (`block-x-at-resolution`, the offsets, the creases). A test lists exactly which recipes change between a 1e-5 and a 1e-3 document.
- **No `#[non_exhaustive]` on the recipe enums.** A new outcome or cause should break the DT harness's match.
- **No fillets and no parabolic or hyperbolic edges.** 72 plans neither for step 4 (fillets are step 8; D8 `EdgeKind` has no in-scope case). A cone cut by a plane parallel to a generator would be the recipe for D8; it waits for a use.

**Findings that change 72.**
1. **T9's `plate_12` "deviation" was a mistake in T9, not in 72.** The benchmark is 12 x 12 = 144 holes (the bench results table, 150 live faces = 6 + 144, 81904.42 = `100000 - 5760 pi`). §14.1 and §5.7 are corrected above. T9's tests compute the formula for a twelve-hole plate and call it `plate_12`; the function is right and the label is wrong. Nothing else in T9 changes.
2. **The `boolean_api` table names 17 cases, not 15**; by 72's own rulings 5 are built, 6 `Unsupported(NonManifold)` (D5: every touch is inside a face) and 6 `Unsupported(SsiPair)`. §5.2's "10 in, 5 out" understates both: the point-contact cases `BO7` was to build are the `NonManifold` ones, and what BO7 builds is the corner and line contacts T12 adds.
3. **The crease's overlap is a sliver** (volume about 1e-15 at 3 resolutions), so it has no volume to test; its value is the validity and no-dangling-reference invariants.

**Open.**
- *The pin's near-coplanar threshold is not scx's.* If scx's merge distance differs from the resolution (it is the `max` of resolution and accuracies, plus the rounding of copies), the offsets' split at 0.5 and 4 resolutions still holds; the recipes avoid 1.
- *Lineage names the pin's faces by plane.* The harness maps the pin's attribute ids to `BoxFace` through the pin's face order (index 0 bottom and 1 top are known from the pin test; the sides are for DT1 to read).
- *The plate's pitch is not recorded* (68 gives none): the recipe takes cell centres at 100/12. Volume, area and counts do not depend on it.
- *Edge counts of the lineage cases beyond the pin's* (`new_edges`, `halved_edges`) are stated only for the three boxes-at-a-corner Booleans.

**Proposed 04 row.** "T12 (72 §14.9): `scx_testkit::recipe::all(resolution)` lists 68 step-4 recipes (29 constructors, 39 Booleans) as data: inputs, expected outcome (built with hand-stated topology, `Refused`, `Unsupported` with its cause, or `Open`), closed-form volume and area (T9, and clipping for two blocks), lineage for the four two-block `issue_1462` cases and the invariants to check. Every stated topology satisfies Euler-Poincare and every truth agrees with T10's oracle at 5 sigma. T9's `plate_12` correction: 144 holes. Change if: BO4 represents an empty result as a typed `Empty`, or BO7 refuses the corner and line contacts."

### 14.10 FU3 as built (11-27)

One segment type, `profile::Seg`: `Line { start, end }` and `Arc { centre, start, end, ccw }`, ends equal for a whole circle. `solid::Seg` and `solid::Loop` and `csg::Seg` are gone; a loop is a `Vec<Seg>` and `solid::extrusion` and `profile_area` take `&[Vec<Seg>]`. `solid` reads each piece's own start (as `profile` does) and refuses a loop whose pieces do not chain, `"loop is not closed"`, as it did a loop that does not return.

**What the others carried that `profile::Seg` lacked, and the decision.**
- `csg::Seg::Arc` gave radius, start angle and a signed sweep; `Circle` was a variant. Decided: derive them in double-double from the ends (radius from the start, angles by `atan2`, sweep in the sense, equal ends the whole circle). For: one meaning of an arc in all three modules; the ends are the data a planar layer will hold, so the oracle now judges the arc the profile names and not one a rounded `sin_cos` moves off it by 1e-16. Against: an angle-specified arc is a different input once its ends are rounded; the old `sweep > 2 pi` and `start` range refusals have no input to refuse; an end off the circle is a new refusal (`Size`, beyond a relative `1e-9`). Adversarial: the Lipschitz argument needs only a closed set and the right parity, not that the ends are on the circle, and the junction gap and arithmetic term already bound a stray end, so soundness does not change. A whole circle with a clockwise sense used to be an arc of sweep `-2 pi` whose chord angle was noise about zero; it is now `Kind::Circle` in either sense (angle `2 pi` inside), which is the sound reading.
- `solid::Seg` had implicit starts and `to` equal to the start for a whole circle: the same convention, kept.
- `profile::Seg` had neither; `loop_area` and `swept` already give `2 pi` and the full disc for equal ends, so the doc now says so (the fixtures still spell a circle as two arcs).

**Results.** `solid`: `profile_area` and `extrusion` of four loops (stadium, stadium with a hole, an off-origin reflex loop, three quarters) and `gear` are bit-identical (Debug output of every field compared old to new, release). `csg`: `signed_distance` at 300 seeded points on each of six profiles is bit-identical, including the arc ones; line-only profiles, whole circles started at angle 0 and a ring are bit-identical in enclosures, `gap`, `bounds` and probes too. Arc profiles with an end on a multiple of pi/2 differ in the enclosure by a few ulp, 3e-16 at most seen (it is tighter: the old junction gap was the `1e-16` by which `cos(pi/2)` is not 0, now only the `2^-90` arithmetic term) and in the probes by at most 8e-16. That the old enclosure was wider for the same drawn profile is the old input's rounding, not a defect; the new one is the truth for the ends as given. Tests in `scx-testkit` in release: 165 pass, 0 fail; no difftest uses these modules.

**Tests.** None removed. Two assertions changed with the type: the `csg` error test's arc of sweep `7.0` expecting `Size` is now an arc whose end is off its circle (the sweep is no longer a field), and the `csg` tests that built arcs by angle use a test helper `arc_at` that rounds the ends the way `fmath::sin_cos` does.

**Proposed 04 row.** "FU3: the testkit has one segment type, `profile::Seg`, a line or an arc by its ends, centre and sense (equal ends a whole circle); `csg` and `solid` take it and derive radius and angles in double-double. Change if: an oracle needs an arc the rounded ends cannot name (a radius with no ends)."

### 14.11 MK2 as built (11-27)

`crates/scx-make` gains `cylinder`, `cone`, `sphere` and `torus` (new modules `cylinder.rs`, `cone.rs`, `sphere.rs`, `torus.rs`). Dev-dependencies: `scx-nurbs` (for `Approach` in the tests) and `scx-testkit` with `bodies` (the BR8 oracle). Public API added:

```rust
pub fn cylinder(frame: &Frame, radius: Length, height: Length, doc: &Document) -> Result<Made<CylinderFaces>, MakeError>;
pub fn cone(frame: &Frame, radii: [Length; 2], height: Length, doc: &Document) -> Result<Made<ConeFaces>, MakeError>;
pub fn sphere(frame: &Frame, radius: Length, doc: &Document) -> Result<Made<SphereFaces>, MakeError>;
pub fn torus(frame: &Frame, major: Length, minor: Length, doc: &Document) -> Result<Made<TorusFaces>, MakeError>;
pub struct CylinderFaces { pub bottom: FaceId, pub top: FaceId, pub wall: FaceId }
pub struct ConeFaces { pub bottom: Option<FaceId>, pub top: Option<FaceId>, pub wall: FaceId }
pub struct SphereFaces { pub face: FaceId }
pub struct TorusFaces { pub face: FaceId }
// MakeError gains Unsupported(Unsupported), kind Unsupported:
#[non_exhaustive] pub enum Unsupported { HornTorus { major: f64, minor: f64 } }
// Refusal gains ZeroRadii and SelfIntersecting { major, minor }; Defect gains Surface(SurfaceError);
// Dimension gains Radius, Height, BaseRadius, TopRadius, Major, Minor.
```

**What it does.** Each body is one `Draft` written with its topology known, as MK1's block is: no stitch, no orientation pass, no ray fire, pcurves from `Draft::exact_loops` with every `bound` `None`, then `finish`. A periodic surface is one face whose loop runs round its seam, so each wall's loop is counter-clockwise in uv over one period and its front is the outside.

| body | faces | edges | vertices | loop of the curved face |
|---|---|---|---|---|
| cylinder; frustum (both radii above zero) | 3 | 3: two rim circles, the seam ruling at the frame's x axis | 2 | bottom rim forward, seam up, top rim reversed, seam down |
| cone to an apex (one radius exactly zero) | 2 | 3: the rim, the seam ruling, the apex's collapsed edge | 2 | rim, seam, apex, seam back (apex on top); top rim reversed, seam down, apex, seam up (apex at the base) |
| sphere | 1 | 3: the seam meridian through the frame's x axis, a collapsed edge at each pole | 2 (the poles, on the frame's z axis) | meridian down, south pole, meridian up a period on, north pole |
| torus | 1 | 2 closed: the outer equator and the meridian through the frame's x axis | 1 (at `major + minor` on the frame's x axis) | equator, meridian, equator reversed, meridian reversed |

`Assembly` gains `collapsed(v)` (a pole or apex, used once by the face round it) and `circle(frame, radius)` (a closed edge over `[0, 2 pi]` and its one vertex at the frame's x axis). A seam needs nothing new: the same edge is used twice in one loop with opposite senses, and `exact_loops` shifts the second use a period on. The cylinder and the frustum share `walled`. The size check is now one crate-private `error::size`, which `block` also uses (no change in its answers).

**Decisions, with cases.**
- **`sphere` takes a `Frame`, not §4.3's `centre: Point3`** (a deviation). For: §4.2 says every constructor takes a frame; the frame places the seam and the poles, which a caller cutting the sphere later (a Boolean's seam cases, BO5/BO7) needs to control, and it costs a caller with only a centre one `Frame::new`. Against: a sphere is symmetric, so the frame's rotation is invisible in the solid, and the sketch is shorter. Kept: the B-rep is not symmetric even when the solid is.
- **`Dimension` stays one enum** (MK1's open question 2). For: one `Refusal::Size` variant across all constructors, a typed name a caller can match, and five constructors add only six variants (`Radius`, `Height`, `BaseRadius`, `TopRadius`, `Major`, `Minor`). Against: some variants mean nothing for some constructors (`X` for a sphere), so the type does not say which a constructor can return; per-constructor enums would. Kept: the doc of each constructor names its dimensions and the test pins them; per-constructor enums would multiply `Refusal` variants for no caller.
- **A cone's radius is exactly zero (an apex) or a size above the zone.** A radius in `(0, zone]` is `Refused { Size }`, not an apex. For: a radius within the zone would be a cap smaller than the resolution, which `finish` refuses as a defect, and silently turning it into an apex would change the body the caller asked for. Against: the pin skips a cap "within the tolerance" and builds the apex. Kept: refusal names the size, and 0 is the documented exception. Both zero is `Refused { ZeroRadii }`.
- **Equal radii build a cylinder wall.** `Cone::new` refuses a zero half-angle (a cylinder is its own kind), so `cone(r, r)` builds the `Cylinder` surface and keeps `ConeFaces`. Against: a caller asking for a cone gets a cylinder surface. Kept: it is the same solid, exact, and refusing would push the branch onto every caller.
- **`ConeFaces` holds `Option` caps**, not an enum of frustum, apex-on-top and apex-at-base. For: one record, field access unchanged across the three, `None` means "no face here" with nothing hidden (not a failing default: no value stands in for an absent one). Against: the type allows both `None`, which no cone returns. Kept: an enum makes the common read (`faces.bottom`) a match for no gain.
- **Torus kinds and their band.** The sizes are checked first (so the pin's `(2e-12, 0.5)` is `Size { Major }`, not a tube-crosses-axis refusal). Then `|minor - major| <= zone` is `Unsupported { HornTorus }` (D3), on both sides of equality; `minor > major + zone` is `Refused { SelfIntersecting }`. For: radii within the resolution of each other are one length, so a ring torus whose tube is closer to the axis than the zone is a horn torus at this document and cannot be made safely. Against: a caller asking for `minor = major - zone/2` gets `Unsupported` for a valid ring torus. Kept: the alternative is a body whose tube touches its axis within the resolution. The pin built the horn torus; this is on purpose.
- **Collapsed edges at the poles and the apex.** The pin's sphere is `(1, 1, 2)` (no pole edges); this one is `(1, 3, 2)`. `exact_loops` refuses a loop that crosses a pole without one (`PoleNeedsEdge`), and the BR2 rules give each pole a uv segment through it. The Euler count over these bodies is taken over the curve edges, `V - E + F - (L - F) = 2 - 2 genus`, since a collapsed edge is a pole, not a side.
- **`Defect::Surface(SurfaceError)`** for a surface constructor refusing sizes already validated. It cannot happen with the checks above; the alternative was `expect`, which the workspace forbids.

**Pin tests.**
- `primitives_are_closed_solids_with_analytic_properties`: ported for the cylinder r5 h20 and the cone r5 to 2 h20 at the origin, with the two apex cones and a frustum widening upward added, each again at a tilted frame at (120, -40, 7.5). Each asserts counts, collapsed-edge count, caps by role, the closed-solid structure (two regions and shells, two opposite fins per curve edge and one per collapsed edge, front in the outside, no pcurve bound), the Euler count, BR8's `violations` empty, volume, area and centroid against `solid::cylinder` and `solid::cone` placed (1e-12 relative), and membership against T10 (`Csg::leaf` with `Cylinder` or `Frustum`): every face sampled on an 11 x 11 uv grid where `contains_uv` is `Inside`, the point `Band`, a step along the front normal `Out`, one back `In`. Not ported: the pyramid spec (not a step-4 constructor, §4.2); the box spec (MK1's); the tight bounding box (below).
- `complete_spheres_and_tori_have_analytic_properties`: ported for the sphere r3 at (1, -2, 0.5) and the torus R5 r1.5 at (0, 1, -1), each also at a tilted frame, with the same assertions (genus 1 for the torus) against `solid::sphere` and `solid::torus` and T10's `Sphere` and `Torus`. Not ported, on purpose: "sphere r3 (no pole)" (the pin's `create_sphere_no_pole_with_tolerance` built fitted patches to avoid its own pole handling; scx has one exact sphere, poles included), and the partial sphere and partial torus (partial primitives are not in step 4, §4.2). The pin's `tight: true` bounding box is asserted only as holding the sphere: `Brep::bounding_box` is the conservative box `scx-brep` keeps for pruning (MEASURED: on the r3 sphere at the world frame its half-widths are 4.40 mm across x and y and 3.63 mm along z), and there is no tight-box query yet.
- `precise_sphere_properties_scale_and_translate`: ported for radii 2 at the origin and 5 at (1, -2, 0.5), area `4 pi r^2` and volume `4/3 pi r^3` to the pin's 1e-9 (MEASURED: within 2.2e-14 relative). The pole-free halves of its rows are not ported (above).
- `primitive_infinite_region_searches` (5 tests) becomes `each_region_is_the_one_built`: the cylinder r4 h14 at (10, 20, -1), the thin cylinder (h 5e-4), the far cylinder at (600, 0, 0), the cones 5 to 0 and 5 to 2 of height 7, the box 8 x 14 x 3, and the huge box 1000 x 1e7 x 4e7 at a 1e-9 mm document. Each has exactly two regions, the outside not solid and one solid; every face's front is the outside's shell, its front normal at its middle points away from the solid, and the volume is the truth's. The pin's search counts go: there is no search. Not ported: the test that adds a box to a B-rep holding another, which is a union, the Boolean's job (S1, BO4).
- `api_primitives` refusals: `a_sphere_radius_not_above_zero_is_refused_naming_the_radius` and the size lines of `invalid_inputs_are_rejected_before_the_kernel` become `sizes_not_above_the_resolution_are_refused_by_name` (-3, minus the zone, 0, the zone, NaN, both infinities, on every size of all four constructors; 0 is not a refusal for a cone radius); `validators_match_upstream`'s cone lines become `a_cone_with_both_radii_zero_is_refused`; `a_torus_whose_tube_crosses_its_axis_is_refused_naming_the_radii` and `the_horn_torus_is_still_built` become `spindle_tori_are_refused_and_horn_tori_unsupported` ((1, 2), (1, 1e9) and (2, 2 + 4 zone) refused; (2, 2) and 2 +- zone/2 `Unsupported`; 2 - 2 zone builds). The horn torus flips from built to `Unsupported` on purpose (D3). `complete_solids_construct`'s counts are in the property tests above.
- MK1's handed-over far-cylinder row becomes `far_cylinders_build_closed_and_valid`: cylinders of each radius and length of the pin's cylindrical-box rows ((1, 1, 2), (1e-3, 1, 2), (1, 1e-3, 2e-3)) at 1e3, 1e6 and 1e8, plus both apex cones, a sphere and a torus at a tilted frame there, all closed and BR8-clean. The tube itself (inner and outer radius) is a profile's extrusion and waits for `extrude`. The BR8 oracle's default tolerances (1e-7 mm, 1e-9 in uv) are below one ulp at 1e8, and the tilted apex cone there shows a `UvGap` at them (a seam pcurve inverted from rounded vertices), so these rows use `max(default, 8 eps c)`; `finish` accepts every row at the document's own resolution.
- Added: `a_cone_of_equal_radii_is_a_cylinder`, `making_is_deterministic` (bit-for-bit vertices and equal records).

**Mass stand-in.** scx-mass (MA1) is not on main, so the tests read volume, area and centroid off the body by the divergence theorem: tensor Gauss-Legendre (16 nodes, 8 x 8 panels, aligned with the quarter arcs) over a curved face's uv box, after checking the face is its whole box, and Green's theorem over a planar cap's pcurves. To be replaced by scx-mass when MA1 lands.

**Measurements** (throwaway timer, not committed; release build, 20,000 repetitions after 2,000 warm-up, at the world frame, build plus `finish`; two runs on a loaded machine, so ranges): cylinder r5 h20 **10.8-11.9 us**; frustum 5 to 2 **11.3-12.8 us**; cone 5 to 0 **8.0-9.0 us**; cone 0 to 5 **8.5-8.8 us**; sphere r3 **9.2-10.3 us**; torus 5, 1.5 **10.0-11.3 us**; the block in the same runs 16.8-20.7 us (MK1 measured 18.3). Each is cheaper than the block: fewer faces and edges. Added work against the pin: none (the pin stitches and, for its infinite-region search, fires rays; this does neither). The block's change to the shared `error::size` adds no work.

**Deviations from 72.** `sphere` takes a `Frame` (above). `MakeError::Defect` carries a cause, as in MK1. The spindle refusal uses 72's name `SelfIntersecting`; the horn band is the zone.

**Open questions.** (1) A tight bounding-box query (the pin's `tight: true`) for the property tests' box check: a query task, not a constructor's. (2) Whether the BR8 oracle's tolerances should scale with position by default. (3) Partial primitives stay `Unsupported` by not existing; no caller needs them yet.

**Proposed 04 row.** "`scx-make` (MK2): a periodic surface is one face whose loop runs round a seam edge used twice; poles and apexes are collapsed edges; a cone radius is exactly zero (an apex) or a size, both zero refused; a torus with radii within the zone of each other is `Unsupported { HornTorus }`, beyond that `Refused { SelfIntersecting }`; `sphere` takes a frame; `Dimension` stays one enum. Change if: BR2's pole rules cover an apex on the axis (the horn torus), or a caller needs partial primitives."

MK2b (11-27): MK1's, MK2's and MK3's volume, area and centroid checks now use `scx_mass::mass` (scx-mass a dev-dependency, below scx-make in `layers.txt`), still against the T9 closed forms at the same tolerances, each also required to hold the Estimate's error within it, and the local divergence-theorem `mass`/`volume_and_area` stand-ins and their Gauss-Legendre helpers are deleted, as they proved nothing beyond what scx-mass's own truth tests prove. Cost: with scx-mass unoptimised in the dev profile `--test extrude` took 42 s and `--test round` 12 s (a sphere's `mass` at 1e-13 alone 2-4 s, 56 and 19 calls; main's stand-ins 0.47 s and 0.20 s); `[profile.dev.package.scx-mass] opt-level = 3` brings them to 1.7 s and 0.75 s with no tolerance changed (about 20 us to 350 ms per call), and splitting the T9 sweep into a plain-frame and a tilted-frame test (run side by side) to 0.9 s and 0.8 s. Defect: `Defect(StepControl)` comes back where the accuracy asked is out of reach (a 2e-4 cube at 1e3 about its far corner at 1e-12 and 1e-13, though 1e-9 is met; a 0.1 cube at 1e8 even at 1e-9; a 5e3 cube at 1e12 at 1e-13, and at 1e-12 the call did not return within minutes), so the far-box row asks accuracy equal to its tolerance and MK3's extrusions each their own; a defect means a kernel bug, so scx-mass should meet the accuracy or answer with a typed `Refused`/`Exhausted`. Smallest repro: `block` at frame origin (1e3, 1e3, 1e3), side 2e-4, `mass` with `about` that point, `RelAccuracy` 1e-12; the fix is routed to MA3 and should make it a typed refusal (or met).

### 14.12 MK3 as built (11-27)

`scx_make::extrude` and `scx_planar::Segment::Circle`. `scx-make` now depends on `scx-planar` (below it in `layers.txt`). Public API added:

```rust
// scx-make
pub fn extrude(region: &Region, frame: &Frame, span: Span, doc: &Document) -> Result<Made<ExtrudeFaces>, MakeError>;
pub struct Span { pub start: Length, pub end: Length }            // Span::new; either order
pub struct ExtrudeFaces { pub start: FaceId, pub end: FaceId, pub walls: Vec<Vec<FaceId>> } // [loop][segment as given]
Refusal::{Profile(PlanarError), Span { start: f64, end: f64, floor: Length }, Resolution { region: Resolution, document: Resolution }}
#[non_exhaustive] pub enum Unsupported { Profile(PlanarError), Segment(SegmentRef) }   // MakeError::Unsupported
Defect::Surface(SurfaceError)
impl From<PlanarError> for MakeError   // Undecided -> Unsupported::Profile, the rest -> Refused(Profile)
impl From<SurfaceError> for MakeError
// scx-planar
Segment::Circle { centre: Point2, radius: f64, sense: Sense }  // Segment::circle(..)
PlanarError::CircleWithOthers { at: SegmentRef }
```

`Unsupported`, `MakeError::Unsupported`, `Defect::Surface` and `From<SurfaceError>` are the same shape MK2 adds for its constructors, so the merge only joins the enum variants.

**What it does.** A region point `(x, y)` at height `z` is `origin + x X + y Y + z Z`, placed once. Each loop gives a ring of vertices at each height, one per segment start (a whole circle has one, at its seam `centre + r X`). Between them:
- the rim edges: a line is the shared straight edge; an arc is a circle edge from its counter-clockwise-first end over `(0, 2 pi)`, framed with `x` towards that end, radius the mean of its two ends' distances; a whole circle is a circle edge over `[0, 2 pi]` from its seam vertex to itself;
- a vertical edge at each ring vertex;
- one wall per segment, its loop along the bottom rim, up at the segment's end, back along the top, down at its start. A line's wall is a plane whose normal is to the right of travel; an arc's or circle's is a cylinder about its centre's vertical, `Forward` (convex) for a counter-clockwise arc and `Reversed` for a clockwise one. Because the region runs the outer loop counter-clockwise and the holes clockwise, the right of travel always faces away from the solid;
- the caps: the top a plane with normal `Z` taking each loop as the region gives it, the bottom with normal `-Z` taking each loop reversed.

All of it goes into one `Draft` (both regions decided at construction, pcurves from `exact_loops`), then `finish`. No stitch, no ray fire. A round bar is 3 faces, 3 edges and 2 vertices, the cylinder's topology.

**Decision: `Segment::Circle` now, as a variant (04's PL1 row).** For:
- An extruded round bar then has `cylinder`'s topology (one wall, one seam) and the face record's meaning is stable: one wall per circle, not two half-walls whose split is an artefact of how a circle had to be spelled.
- A plate hole is one face, as in `solid::plate` and T12's `plate_12`.
- `csg::Seg` had a whole circle, and FU3's `profile::Seg` reads equal ends as one.

Against: work in PL1's contact and winding code. That work was small:
- contact treats a circle as an arc whose angular range holds every direction, with its seam as both ends. This is sound because the seam is a point of the circle, so testing it against the other segment tests a point of the circle;
- winding gives a whole turn about a point certified inside the circle;
- the signed area is `pi r^2` in the circle's sense.

Adversarial pass:
- Could a two-arc circle simply be merged by `extrude`? That would hide the split in one consumer and leave it in every other one (`classify`, offsets, the record).
- Could an `Arc` with equal ends mean a whole circle, FU3's convention? Then a degenerate arc and a circle would be the same data, and the one that is an error could not be refused. PL1 refuses equal ends as `Short`, and that stays.

**Deviation: the variant has a `sense`.** 04 wrote `Segment::Circle { centre, radius }`. Without a sense, orienting a hole could not flip it (`reversed`), and a loop's direction would have to be stored beside it. With one, `Loop` stays self-describing and `reversed()` stays an involution on every segment. A circle in a loop with other segments is `CircleWithOthers`: a circle has no joint for its neighbours to meet. Its radius must be proved above the resolution (`Degenerate::Radius`).

**Decision: the region's resolution must equal the document's** (`Refusal::Resolution`).
- A finer region has not proved its segments apart at the document's zone.
- A coarser one may have snapped an arc end off its circle by more than the zone, so `finish` would refuse a valid profile as a `Defect`.

Against: a caller building regions at a finer resolution for a coarse document must rebuild them. Kept, because it is the honest answer and the error names both values.

**Decision: `Span` in either order.** The end below the start extrudes along `-z`. The start cap is at the start height, with its front facing away from the end. Heights must be finite and more than the zone apart (`Refusal::Span`), and the difference must be finite too (`-MAX` to `MAX` is refused). For: the pin's direction-and-distance pair says the same thing, and a frame already gives the direction. Against: a signed distance from 0 would be one number, but it could not say where the start cap is.

**Decision: `walls` are in the caller's order.** If the region reversed a loop, the walls are mapped back, so `walls[k][i]` is the wall of segment `i` of loop `k` as given to `Region::new`, where loop 0 is the outer loop and loop `1 + j` is hole `j`. For: the caller names segments by what it gave. Against: the region's own order is one fewer step. The mapping is a reverse of a `Vec`.

**Decision: `Segment` is `#[non_exhaustive]`, so a kind `extrude` does not sweep is `Unsupported::Segment(at)`**, never treated as a line. It is unreachable today, and 72 §4.2 names it ("profile segment kind").

**Decision: an arc's radius is the mean of its ends' distances.** The region allows ends that sit off a common circle by up to the resolution (`EndOffCircle`), and the mean puts each end within half of that. Against: the centre-to-start distance would make one end exact. The mean was chosen because it is symmetric under reversal.

**Pin tests (`sweep/extrude.rs`).**
- `a_long_direction_extrudes_the_distance`: ported as a frame given a `1e6`-long z hint. The bounds are exactly `(0, 0, 0)` to `(2, 2, 3)` and the counts 6/12/8. Its zero-direction refusal is removed on purpose: the direction is a frame's unit z axis, so a zero direction cannot be written.
- `a_draft_goes_the_way_the_direction_points`: the undrafted half is ported as `a_reversed_span_goes_the_other_way`. The draft half is deferred to `scx-taper` (step 8).
- `a_draft_across_the_profile_or_an_open_profile_is_refused`: the open half is ported as `an_open_profile_is_refused` (`Refused(Profile(Open { Outer, .. }))`, kind `Refused`, name `profile`, with a source). The draft half is removed on purpose, because there is no draft argument.
- `a_profile_with_a_short_cubic_extrudes_to_a_valid_solid`: deferred to step 5 (NURBS profile segments).
- `an_uncapped_profile_with_a_short_segment_keeps_every_wall`: ported, capped, as `a_short_segment_keeps_its_wall`. For lengths 3e-5, 1.7e-4 and 3.9e-4 it gives 7/15/10, two edges of that length, and volume 120 to 1e-13. The uncapped (open sheet) half is removed on purpose: there is no `cap: bool` and no sheet constructor. It proved that a wall is kept when its segment is short, and the capped case still proves that.
- `a_rounded_rectangle_drafted_slightly_inward_keeps_every_corner_wall`: ported undrafted with the pin's `a, b, r`. At heights 2, 2.01 and 2.5 it gives 10/24/16 and volume `h (4ab - (4 - pi) r^2)` to 1e-12. The draft half is deferred to `scx-taper`.

**Added tests.**
- `volume_and_area_match_the_closed_forms` (T9): 7 profiles (square, rounded rectangle, round bar, ring, a concave-arc bite, a slot with a slot hole and a circle hole, a plate with 12 circle holes) at the world frame and at a skew frame far from the origin. Spans `[0, 3]`, `[5, -2]` and `[-1, 0.25]`. The volume and area come from the body's faces by the divergence theorem, against `solid::extrusion(..).place(..)`. The worst relative error is 7e-14 on volume and 5e-14 on area; the bound is 1e-12.
- `points_off_every_face_are_on_the_right_side` (T10): the same profiles and two spans. On every face there are points: cap points on a grid where the region's `classify` says `Inside`, three interior points on each plane wall, three on each cylinder wall along its rim arc. Offsets of `±1e-3` along the front normal are `Out` and `In` against `csg::Primitive::Extrusion`, with `delta` 1e-6.
- `every_profile_fixture_comes_through` (T11): all 23 fixtures at a document of the fixture's resolution. Each accepted fixture extrudes to a closed body of volume `area x 1.5`, to 1e-10. Each refused fixture is `MakeError::Refused(Refusal::Profile(e))`, with `e` of the fixture's cause.
- `the_record_names_every_face_by_role`: an outer loop and two holes given counter-clockwise, so the region reverses both. Each wall is checked on its own segment's plane or cylinder axis, with the cylinder's sense following convexity. The record holds every face once. Each cap is at its height with its front away from the other, for both span orders.
- `a_round_bar_has_one_wall_and_a_seam` (3/3/2, exact volume and area).
- `circle_holes_give_one_wall_each` (18/48/32, against `solid::plate`).
- `whole_circle_edges_run_one_turn`.
- `bad_spans_are_refused`.
- `a_region_at_another_resolution_is_refused`.
- `an_undecided_profile_is_unsupported`.
- `extrusion_is_deterministic` (Debug output of the body, compared).
- Every body is also checked by `assert_closed_solid`: two regions, two opposite fins per edge, front outside and back solid, and every pcurve exact. The pcurve is `Line2` for a line on either a plane or a cylinder, `Circle2` for a circle on a plane, and `AngleLine2` for a circle on a cylinder.
- Mutation check (local, not committed): scaling an arc wall's radius by `1 + 1e-6` fails 5 of these tests.

`scx-planar/tests/circles.rs` covers:
- a disc's area and `classify`;
- a clockwise circle turned and flagged;
- circle holes in a square;
- a ring;
- tangent and crossing circles;
- a circle within the resolution of a line;
- a line through a circle;
- a tangent line, and one on the far side from the seam;
- `CircleWithOthers`;
- radius zero, negative, below the resolution, NaN and infinity;
- hole outside, hole in a hole, and a hole round the outer;
- a circle against a slot's arcs.

No test was removed.

**Measurements** (throwaway release timer, not committed; build plus `finish` per extrusion, height 10 at the world frame; `Region::new` timed separately):

| segments | lines only, us | us/segment | lines and arcs alternating, us | us/segment | `Region::new` (lines), us |
|---|---|---|---|---|---|
| 4 | 21.4 | 5.35 | 34.3 | 8.57 | 4.7 |
| 16 | 72.9 | 4.56 | 134.8 | 8.43 | 14.1 |
| 64 | 293 | 4.58 | 544 | 8.50 | 56 |
| 256 | 1293 | 5.05 | 2201 | 8.60 | 221 |
| 1024 | 5690 | 5.56 | 8953 | 8.74 | 900 |
| 4096 | 23940 | 5.84 | 39079 | 9.54 | 3780 |

- Growth is linear, rising slowly from the `BTreeMap` of shared lines and from `finish`.
- An arc costs about 1.7 times a line: a circle frame, and a cylinder with `AngleLine2` pcurves.
- The 4-segment square takes 21.4 us, against MK1's block at 18.3 us.
- Added work against the pin: none. The pin builds through `create_linear_sweep` with planar faces from 3-D curves and a stitch.

**Deviations from 72.**
- `Segment::Circle` has a `sense` (above).
- `walls` are in the given order (72 says only `[loop][segment]`).
- The equal-resolution refusal is new.
- The `MakeError` kinds follow §4.2. The profile segment kind is `Unsupported::Segment`, and an undecided profile is `Unsupported::Profile`.

**Open questions.**
1. Should `extrude` take a region at a finer resolution than the document's? It would be sound only if the region's closure snaps were within the document's zone, and the region does not record that.
2. `ExtrudeFaces::walls` as a `Vec<Vec<FaceId>>` allocates per loop. A flat `Vec` with loop offsets would be one allocation, but it would be read by index arithmetic.
3. The pin's draft tests return with `scx-taper`. Whether taper reuses this ring builder is for step 8.

**Proposed 04 row.** "MK3: `extrude(&Region, &Frame, Span, &Document)` makes one wall per segment of each loop (plane for a line, cylinder for an arc or a whole circle) and a planar cap at each height, with topology known and exact pcurves. `ExtrudeFaces { start, end, walls[loop][segment as given] }`. The region must be at the document's resolution. `Span` is two heights in either order, finite and more than the zone apart. Planar refusals come through as `Refused(Profile)`, and `Undecided` as `Unsupported(Profile)`. `scx-planar` gains `Segment::Circle { centre, radius, sense }`, a loop on its own. Change if: a caller needs a region at a finer resolution, or a sheet (uncapped) extrusion."

### 14.13 DT1 as built (11-27)

**What.** `crates/scx-difftest/tests/diff/t_slice.rs` with two tests, `t_slice` (all 68 T12 recipes, about 40 s in release, one thread per recipe) and `t_slice_controls`; `pin_defects::slice()` holds the pin's contradictions of truth. Each recipe runs at the default resolution `c = 1e-5`, in the pin and in scx, and each is judged against truth (T9's closed forms, T10's oracle, T12's outcome, topology and lineage), never against the other side.

- **Checks** (`Check`): `Outcome`, `Volume`, `Area`, `Topology`, `Classification`, `Valid`, `Manifold`, `Mesh`, `Deterministic`, `Lineage`, `NoDanglingVertices`, `Work`; a recipe gets `Outcome` and `Work` always, and its T12 invariants when it is `Built` or `Open`.
- **scx's side** answers each check `Pass`, `Fail(why)` or `NotYet(gap)`. `NotYet` is typed, so DT2 and DT3 switch on it: `Make(Primitive)` (MK2 cylinder, cone, sphere, torus; MK3 extrusion), `Boolean` (BO), `Mass` (MA1), `Classify` (BQ), `Mesh` (TS), `Contract` (CK1), `Work` (BO6), `Lineage` (BO). Today scx builds only blocks (`scx_make::block`): outcome (built, or refused with the named `Size` dimension), topology (regions, shells, genus from Euler, and the patch complex where each patch is one face), determinism (`scx_record::encode` equal in bytes over two builds) and no dangling vertices pass on the 17 block checks; 466 are `NotYet`.
- **The pin's side.** A fresh `Kernel` per run; parts built axis-aligned at the origin and placed with `p5::transform` (or built in place when the frame is the world's); extrusions by `create_linear_sweep` of lines and `create_arc`s; Booleans folded over the steps; every call under `catch_unwind`. Facts read off the body: `brep_volume_default` and `brep_area` at 1e-6, the topology (solid regions, shells cut apart at pinches, genus from the Euler characteristic of each shell, patches by surface kind), `is_manifold_solid`, `integrity::check`, dangling vertex references, `point_3d_classify` of every point at `c`, lineage (below) and every vertex's bits.
- **Patches without the pin's own merge flags.** Each pin face is put on the first operand surface ("sheet", built from the recipe in the part's frame) on which all of nine uv samples lie within 1e-6; a patch is faces joined across two-use edges on one sheet. A face on no sheet is counted (`off_sheet`) and is a topology finding.
- **Lineage** as the `issue_1462` tests do it: each block operand's faces tagged `Attribute::long(10007 + 10 operand + BoxFace, Copy)` before the first step, the result's faces grouped into patches with their areas summed, compared with T12's list as multisets at 1e-6 relative.
- **Points.** 1,000 seeded points in the box grown 10% plus T10's probes (4 per face, at coincidence `c`, extent 5), capped at a seeded subset of 1,000; T10 decides each at `sqrt(3) c`. Certain points are judged; `Band` points are counted, not judged (row 7). The recipes where fewer than 95% of the box points are certain are printed with their share.
- **Run twice.** The pin's whole `PinRun` (outcome, facts, vertex bits) on two fresh kernels must be equal; scx's two bodies must encode to equal bytes.
- **The summary** matches each pin finding to exactly one `SliceDefect` whose `check` is the finding's and whose `applies(recipe)` selects it, and that entry's `holds(recipe, points, run)` must still see the recorded contradiction. An unmatched finding fails, a matched entry that no longer holds fails, an entry that applies where the pin passes fails (except for classification, below), and an entry that meets no finding fails as stale.
- **Controls** (`t_slice_controls`, on `block-far-turned`): a truth volume one part in 1e4 off gives a `Volume` finding and an unrecorded failure; an entry that applies to nothing fails as stale.

**Measurements.** The first version took 284 s, about 220,000 points, with the pin's classification of up to 11,500 probes per plate on both runs dominating. Capping the probes at 1,000 brings the total to 98,387 inputs (68 recipes and 98,319 points) in 40-43 s of wall time on 68 threads. The slowest recipes take about 17 s each under that contention: the sphere, torus and seam cases, where each pin classification is expensive. Alone, the blocks take under 1 s. One run of the test makes every pin call twice. Nothing is added to the library.

**Pin defects** (`pin_defects::slice`, 43 findings on 41 recipes, all recorded; 27 recipes agree with truth):

| Check | Selected by | Recipes | What the pin does | Holds while |
|---|---|---|---|---|
| Outcome | a constructor block with an extent in `(0, c]` | block-x-at-resolution, block-x-below-resolution | builds the block (fev 6/12/4 and 2/4/4): its floor is `SM_EFF_ZERO`, not the resolution, where truth refuses `Size(X)` | the pin answers a body |
| Manifold | one difference step whose tool is the base | block-minus-itself, cylinder-minus-itself | builds the empty body (no faces, edges or vertices), which its own `is_manifold_solid` rejects | fev `[0, 0, 0]` and `is_manifold_solid` false |
| Topology | two operands with parallel planes `(1e-6 c, c]` apart | coplanar-offset-0.1c, coplanar-offset-0.5c | keeps the near-coplanar faces apart: 7 and 8 plane patches where the merged points rule makes 6 | more patches than truth |
| Classification | every recipe truth does not refuse | 37 recipes | puts 4 to 52 of about 1,300 certain points on the wrong side. Every one lies within `1.5 sqrt(3) c` of the boundary; most are `2c` outside a face, which the pin calls In. A few, on the cylinder's top and the cylinder-sphere seams, are `2c` inside, which the pin calls Out. | some point misplaced, and all within 1.5 times the band |

The 19 recipes the classification entry selects without a mismatch on their points are counted in the summary ("sampled entries not met"). These are the turned frames, the cones, sphere, torus, two extrusions, and the sphere and torus Booleans.

**Band** (box points certain, where below 95%): block-small-at-1e4 0.896, block-size-twice-resolution 0.185, block-x-at-resolution 0.179, block-x-below-resolution 0.183. All are blocks whose extent is a few `c`, where most of the grown box is within the band.

**Decisions** (cases for and against).
- **The resolution is the scx default, `c = 1e-5`, and the pin gets the same value as its point-classification zone.** For: truth's band and the merged points rule are both stated in `c`, and the pin's Boolean zone is close to it. Against: the pin's own merge threshold is not `c` (§14.9 open). Its near-coplanar defect is therefore recorded as a defect of the pin, with the offsets chosen away from `c`.
- **Patches are recognised geometrically, from the operands' surfaces, not from the pin's `faces_continue`.** For: truth counts patches by surface record, and the operands' surfaces are known exactly from the recipe. The pin's flags would test the pin against itself. Against: a face is assigned by samples, so one that strays from its sheet by more than 1e-6 is miscounted. That shows as `off_sheet`, a finding, and none occurs.
- **The area's slack is the recipe's volume slack over `cbrt(V)`.** T12 states one slack, on the volume, and the area's in the length scale. For: dimensionally right, and it is zero except for the merge recipes. Against: `cbrt(V)` is a crude length for thin bodies. It only widens an allowance that is otherwise 1e-6 relative.
- **A classification entry that selects a recipe without a mismatch on its points is counted, not failed.** For: the check is sampled. The contradiction is in a 1.5c shell about the boundary, and whether one of 2,000 seeded points lands in it depends on the recipe's faces, not on whether the pin's defect is there. A rule that fails such recipes would force a predicate on the seed. Against: an entry wider than the defect hides nothing today, but could hide a new mismatch of another kind. It cannot, because `holds` requires every misplaced point within 1.5 times the band, so a far misplacement fails. The other checks are not sampled and keep the strict rule.
- **The probes are a seeded subset of 1,000.** For: they cut the time sevenfold, the subset is deterministic, and every recipe still has about 1,000 on-purpose points. Against: fewer probes near the plates' 144 holes. The probe subset is drawn uniformly over the probes, so each hole keeps a share.
- **One thread per recipe.** For: the recipes are independent, and the pin's `Kernel` is per run. Against: 68 threads oversubscribe a small machine. The wall time is the slowest recipe's either way.
- **The scx side refuses the frame by `Fail`, not a panic.** A recipe frame `scx_num::frame::Frame::new` does not accept fails every check with the reason.

**Deviations from §8.1.**
- Band points are counted, not judged (row 7). §8.1 asked that they classify On or on the side T10 gives at the band's edge.
- T10's probes are capped at 1,000 per recipe (above).
- Lineage is checked on T12's 4 `issue_1462` recipes, not 7: T12 states 4.
- The pin's validity is `integrity::check` (Valid) and `is_manifold_solid` (Manifold), checked separately.
- `NoClearRay` per run (A4's revision) and `Report::work` are scx-side counts. They are `NotYet(Classify)` and `NotYet(Work)` until BQ and BO6 land; the work baseline is DT3's.
- Meshes are `NotYet(Mesh)` on the scx side and not run on the pin's, as §8.1 says.

**Open.**
- *Why the pin misclassifies at 2c.* The pin answers In for a point `2c` off a face, at a zone of `c`. Its classifier may widen the zone to its edge tolerance (about `2c`), or a ray may graze. It is a side record; nothing is filed upstream.
- *The empty body.* If BO4 types an empty result, `block-minus-itself` and `cylinder-minus-itself` change on scx's side; the pin's entry stays.
- *DT2's switch.* As MK2 and MK3 land, `scx_gap` loses a `Make` arm and `scx_build` gains a shape; the pin side and the defects need no change.

**Proposed 04 row.** "DT1 (72 §14.13): `t_slice` runs every T12 recipe in the pin and in scx at `c = 1e-5`, judges each against truth (outcome, volume, area, topology with patches, classification of about 2,000 seeded and on-purpose points, validity, lineage, no dangling vertices, run-twice determinism) and records the pin's contradictions as `pin_defects::slice` entries with input predicates (4 entries cover 43 findings). scx answers typed `NotYet` for everything but blocks. About 40 s in release. Change if: a pin contradiction appears that no entry covers, or truth's band leaves fewer than 95% of points certain on a recipe that is not a few `c` across."

**DT1b (11-27).** The cylinder, cone, sphere, torus and extrusion recipes now build in scx through scx-make, and scx's volume and area come from scx-mass. `NotYet::Make`, `NotYet::Mass` and the `Primitive` enum are removed; every constructor recipe is judged against truth as the blocks are. The scx split is 93 pass, 0 fail, 390 not yet (Boolean 297, Classify 16, Mesh 16, Contract 32, Work 29); DT1 had 17 pass and 466 not yet. The run takes 47.7 s in release. No new scx failure was found. Decisions (cases for and against):
- *Seams and poles are not cells.* scx's closed surfaces carry a seam edge (both fins on one face) and collapsed pole edges (`EdgeShape::Collapsed`). T12's complex has neither. The scx side drops both before it counts: V and E are the kept edges and their ends, and a face's boundary count is the number of connected pieces of its kept edges. A torus face with no kept boundary counts genus 1. For: this is T12's definition, and without it all 8 curved constructors failed on cells and the apex cones and sphere on genus, which is a harness artifact and not a defect. Against: a real fault that adds a stray seam would pass this check. Validity and the scx body's own checks still see it.
- *Mass is asked and judged at `1e-9` relative* (`SCX_ACCURACY`), with T12's slack as for the pin. scx's error estimate is not added to the allowance. For: §8.1 asks for 1e-9, and an estimate that exceeded it would be scx's failure. Against: none. A miss would show as a finding.
- *Refusals map by meaning.* A cone's base and top radii map to truth's `Radius`. The torus major radius has no truth dimension, so a refusal on it never matches. `ZeroRadii` matches `NoBody`, and `HornTorus` matches the horn-torus `Unsupported`. A `PlanarError` from the extrusion profile is `Refused(Profile)` and matches no truth outcome. For: truth names the quantity, not the parameter. Against: a refused radius on the wrong end of a cone would pass. No recipe has radii that would tell the two ends apart.

**DT1c (11-27).** scx's classification check now runs `scx_bodyq::classify_point` on the same points the pin classifies: T10's box points and its seeded probe subset. It is judged as the pin is. Every point T10 calls certain must land on its side, and band points are counted, not judged. `NotYet::Classify` is removed. All 16 constructor recipes pass, and no scx point is misplaced. Of 28,163 points, 19,847 are certain; 6,494 of the 8,316 band points classify On, and none fails. The scx split is 109 pass, 0 fail, 374 not yet (Boolean 297, Mesh 16, Contract 32, Work 29). The scx classify side costs 0.19 s of CPU over the 16 recipes, about 7 us a point and 4 to 22 ms a recipe (throwaway timer, not committed); the run takes 48 s, which is still the pin's time. Decisions (cases for and against):
- *A solid region is `In`; any other region is `Out`; a face, edge or vertex is `On`; an error is `Failed`.* These are the pin's four answers, so the same `misplaced` rule judges both kernels. For: one rule, and an `On` or an error on a certain point is a misplacement in both. Against: `Failed` hides which `BodyqError` it was. A failure lists every misplaced point with both answers, and the error can be read from there.
- *The budget is unlimited and the stop never set.* For: the check asks for the answer, not the cost, and the cost is measured separately. Against: a runaway query would hang the run instead of failing. The per-recipe cost above shows no such query.
- *A control proves the check is not vacuous.* `t_slice_controls` builds the block in scx, passes it on T10's sides and fails it with In and Out swapped.

**Open.** BQ1 answers a point with no clean ray from the nearest boundary point, not with `NoClearRay`, so A4's per-run `NoClearRay` count (§8.1 revision) has nothing to count. Whether the fallback count should be exposed as a side-channel for the work record is BO6's question.

### 14.14 BQ1 as built (11-27)

New crate `crates/scx-bodyq`, placed last in `layers.txt` (after `scx-tess`) and after `scx-tess` in the workspace. Dependencies: `scx-brep`, `scx-curve`, `scx-nurbs` (for `Approach`), `scx-num`, `scx-surface`; dev-dependencies `scx-make` and `scx-testkit` with `bodies`. Public API:

```rust
pub fn classify_point(body: &Brep, p: Point3, budget: &mut Budget, stop: &Stop) -> Result<PointClass, BodyqError>;
#[non_exhaustive] pub enum PointClass { In(RegionId), On(Entity) }
pub const RAYS: [Vec3; 13];
pub enum BodyqError { Refused(BodyqRefusal), Unsupported(BodyqUnsupported), Exhausted { limit }, Cancelled, Defect(BodyqDefect) }
pub enum BodyqRefusal { NotFinite }
pub enum BodyqUnsupported { Nurbs { at: Entity }, NonManifold { at: Entity } }
pub enum BodyqDefect { NoFaceBox { face }, NoUvBox { face }, Surface { face, .. }, NoNormal { at }, Ray, NoBoundary }
// each with kind() and name(); From<Exhausted> and From<Stopped>
```

**What it does.** The three steps of §12.4 row 2, all reads of `&Brep`:

1. **Fast paths.** A body with no box (no faces) and a point outside the body's box answer `In(OUTSIDE)` with no work: every box is grown by its entity's accuracy and the resolution, so a point outside it is on nothing.
2. **On.** The faces whose box holds `p` (`faces_meeting`), then their edges, then those edges' vertices. Each is projected by its closed form (`Surface::invert` over the face's uv box, `Curve::invert` over the edge's interval). Coincidence is `coincident(resolution, accuracy)` of that entity. A vertex wins over an edge, an edge over a face; within a dimension, the lowest id. A face counts only when its foot's uv is `Inside` it. A foot only `Near` the face's boundary is `On` through the edge there, which is within coincidence on a valid body, since the face's accuracy covers its edges.
3. **Rays.** Each ray runs from `p` to `1.0625` times the farthest corner of the body's box, plus 1 mm. The faces it meets are taken from `faces_meeting` of the ray's box, slab-tested and sorted by where the ray enters their boxes (then id). Each is intersected by `intersect_curve` with the ray as a `Line`. The *horizon* is the nearest hit's `t` plus its coincidence, and faces entering past it are not read. The nearest hit is **clean** when all of these hold:
   - it is a `Crossing`, its uv is `Inside`, it lies farther than coincidence from `p`, and `|n . d| > GRAZE` (1e-6);
   - no other hit lies within coincidence of it;
   - it is not an `Overlap`;
   - no edge or vertex of a face reached comes within coincidence of the segment up to the horizon. A vertex is tested by segment distance. An edge is tested by `intersect_curve` of the edge against a thin `Cylinder` of radius `c` about the ray, `[0, 2 pi c] x [0, horizon]`.

   A clean hit gives the region of the face's shell on the start's side: `Back` when `n . d > 0`. A ray with no hit gives the outside. The first clean ray in table order decides.
4. **Fallback.** No ray is clean, so the nearest boundary point decides. The search window doubles from `max_length / 64` of the body's box until the nearest candidate lies within it. The candidates are:
   - face feet with uv `Inside`;
   - edge feet inside the interval;
   - vertices.

   Ties are ranked by distance, then dimension (vertex, edge, face), then raw id. How the nearest point decides:
   - **At a face:** the sign of `(p - q) . n`.
   - **At an edge:** the sum of its fin faces' normals at their feet of `q`, each turned toward the first region.
   - **At a vertex:** the angle-weighted pseudonormal (Baerentzen and Aanaes). For each corner of each face round the vertex, the angle is `atan2((t_out x -t_in) . n, t_out . -t_in)`, mapped into `(0, 2 pi]`. Collapsed edges are skipped to the next real use. At a cone's apex, where there is no normal, the mean of the normals at four rulings is used.

   The regions round the point:
   - one region: that region;
   - two: each face must have one on each side;
   - otherwise `Unsupported { NonManifold }`.

**Work.** One unit of budget per inversion. `intersect_curve` charges its own (two per isolated piece). `stop` is checked before each face and edge. The cells `scx-brep` fills on first read (box tree, boundary trees) are not charged, so the same body and point charge the same work, warm or cold (tested).

**Decisions, with cases.**
- **The ray table: 13 directions from a seeded search**, maximising the worst of three margins:
  - at least 10.2 degrees from every coordinate plane and every diagonal plane (`x = +-y`, `y = +-z`, `z = +-x`);
  - at least 15.5 degrees from every axis, face diagonal and body diagonal;
  - every pair at least 25.6 degrees from one line, so none are opposite.

  For: bodies are drawn in frames, so their faces, edges and symmetry planes lie in those planes and along those lines. The rhombic dodecahedron (§12.4's example) defeats every axis and diagonal ray from its centre, and beats none of these. The test builds it and checks both. Against: a body drawn in a turned frame can still put an edge on one of these rays. Kept: then that ray is unclean and the next decides. A fixed table keeps the answer and the work deterministic; a random table would not.

  The table is normalised to the last bit, and the unit length is tested.
- **The clean rule includes the edge tube**, not only the uv tests. For: at an edge, both faces' uv tests can place the crossing just `Outside`, a crack the ray slips through. A crossing `Inside` but within coincidence of an edge may also have its face end just past it. Either would be a wrong region read with confidence. Against: one more intersection per edge of the faces reached, and an unclean ray where a careful reading would have decided. Kept: the cost is bounded by the faces before the horizon (MEASURED below), and an unclean ray only moves on to the next.
- **`GRAZE = 1e-6` on `|n . d|`.** Below it the sign of `n . d` is not read. For: a hit this shallow is a crossing only by the solver's arithmetic. Against: it is an absolute bound on a unit dot product, so it is not tied to the resolution. Kept: the table keeps rays 10 degrees from the planes faces are drawn in, so a real body's faces are graze-free for most rays. The pin's grazing cases (4.9999 on r5: clean; 4.999 999: within coincidence, unclean) both hold.
- **`Near` is not `On` without an edge** (§12.4 row 2, as ruled). The first build counted a `Near` foot as `On` the face; it now follows the ruling. On a valid body the two agree, because the edge is then within coincidence and wins first. Against the ruling: on a body whose edge and trim disagree by more than coincidence, a point there falls through to the rays. Kept: that body is invalid, which `finish` refuses.
- **Fallback limits.** The fallback takes a face foot only when its uv is `Inside`, and an edge foot only inside its interval. Otherwise the nearest point is taken to be on the boundary (an edge or a vertex). That holds when the closed-form foot is the surface's or curve's only local minimum over the face. Two cases are not covered:
  - a trimmed torus face whose nearest local minimum lies inside the trim while the global one lies outside;
  - an ellipse arc with an interior local minimum while the global one lies outside the interval.

  In both, the fallback reads a boundary point that is not the nearest, and its side can be wrong. Step 4's made bodies have whole tori (one face, no trim) and no ellipse edges, and the fallback runs only when all 13 rays are unclean. Kept, as an open question for when the Boolean cuts tori and cylinders.
- **The cone apex's normal is the mean over four rulings** at the uv box's middle `v`. For: the cone's normal is constant along a ruling, so the mean of four rulings a quarter turn apart points along the axis, as the normal cone round the apex does. Against: on a cone face that does not go round the full turn (after a Boolean), four rulings over its `u` span are a sample, not the whole cone. Kept: step 4's cones go round the full turn. The test reads points just above and below the apex.
- **Wire edges and lone vertices are ignored.** They bound no face, so the face-led candidate lists never name them. For: step 4 has none; a wire neither separates regions nor bounds one. Against: a point on a wire edge is `In`, not `On`. Kept, as a scope limit that `PointClass` can extend later (it is `non_exhaustive`).
- **Placed after `scx-mass`.** It reads `scx-surface`'s intersection, and the Boolean (BO1, BO4) is its first caller. Nothing below it depends on it.
- **Rays tried in order until one is clean**, not a vote. Every clean ray gives the same region; the test checks every one on every testkit kind, over 1,000 points.

**Pin tests.**
- **`query/brep.rs` `notify_named_primitive_box_builds_nonempty_trees_and_classifies_interior` (`:2439`):** the unit box's (0.5, 0.5, 0.5) is in material and (2, 2, 2) is outside, both ported (`the_unit_box_inside_and_out`). The tree-count half has no counterpart: the cells are `scx-brep`'s.
- **The ray-hit tests (`:1847-2275`)** are ported as one-ray outcomes through the crate-private `cast` (`src/tests.rs`):
  - entering 1e-3 from a 100-cube's edge is clean;
  - the nearest hit decides, from outside, from the axis, and on the bar into the post;
  - seam, pole, edge and corner hits are unclean;
  - a ray 5e-4 past a face is clean (it meets nothing);
  - a ray 1e-6 past a face is unclean (its edges are within coincidence);
  - a ray along a face is unclean;
  - the shallow and close-pair cases are clean;
  - the thin cylinder r4e-4 and the torus hole are clean;
  - the cone apex up the axis is unclean.

  The pin's "each face there hit once" becomes "no hit there decides", and its hit lists and counts have no counterpart: the method does not report hits.
- **`brep_query_cache_lifetime`:** the six fixed points' in-material booleans `[T, T, T, T, F, F]` are ported, on the bar into the post built through `Draft` (the test helper `bar_into_post`). Added: the tangent rulings at `x = 40` are `On`, (36, 8, 10) under the post's wall is material, and 600 seeded points agree with T10. **Not ported, no counterpart:** the editing-state tests (the trims seen while editing is on, the open-loop refusal mid-edit, a cache reused across an edit). `classify_point` takes a finished `&Brep`, so there is no editing state to classify during.
- **Hand-built `bar_into_post`.** There is no Boolean yet, so the union is written as its faces: 10 vertices, 15 edges, 7 faces. The bar's sides touch the post along its rulings, which gives tangent contacts at `x = 40, y = 0` and `y = 16`. To be replaced by `scx-boolean`'s union (BO4) when it lands, as a cross-check.

**Other tests.**
- The make primitives (block, cylinder, three cones, sphere, torus) at the world frame, a turned frame and a far frame (~1e4 mm), each against its T10 leaf: 1,000 box points each, and T10's probes.
  - A probe at offset 0 must be `On`.
  - A probe two coincidences clear of every feature must be `In`.
  - Every answer must agree with T10's `side`.
- Every testkit kind (seeds 0-2, sizes 1-2): 1e-3 off each face's middle, each side is that face's shell's region.
- The fallback alone agrees with T10 on the primitives, and with `classify_point` on every kind. It reads edges and vertices right: off block corners and edges, at the cone apex, at the sphere poles, and at the cylinder's rim and seam.
- Determinism: the same answer and the same work on repeat.
- Refusals: NaN gives `NotFinite`; a set stop gives `Cancelled`; `Budget::new(1)` gives `Exhausted`. A point outside the box costs nothing (`Budget::new(0)`).

Removed tests: none (new crate).

**Measurements.** Throwaway timer, not committed; release build, 2,000 seeded points in each body's box after 20 warm-up, the same LCG point set on both sides; the pin's `BrepQuery::point_3d_classify` with boundary intersections, query built once; two runs each on a loaded machine.

| body | bodyq, us per query | pin, us per query | ratio |
|---|---:|---:|---:|
| block 40 x 16 x 10 | 0.70-1.10 | 5.2-7.3 | about 6x |
| cylinder r8 h20 | 2.7-3.1 | 432-466 | about 150x |
| sphere r5 | 1.51 | 1,766-1,818 | about 1,200x |
| torus 10, 2 | 2.5 | 3,297-3,663 | about 1,400x |
| bar into post (hand-built here, the pin's own `boolean_union` there) | 1.58-1.59 | 375-381 | about 240x |

The work charged per query on these is 2.0 to 5.8 units, a few inversions and one or two ray intersections.

Against body size, on the testkit kinds:

| kind | faces | us per query | work per query |
|---|---|---|---|
| Block | 6 to 6,144 | 1.8 to 27 (26.5 on the repeat) | 5.6 to 3.0 |
| Sphere (lunes) | 1 to 32 | 1.5 to 25 | 3.5 to 45 |
| Torus | 1 to 1,024 | 2.9 to 10 | 5.5 to 7.3 |
| Cylinder | 3 to 34 | 3.1 to 4.8 | 4.8 to 11 |
| Holed | 10 to 41 | 2.6 to 6.5 | 6.7 to 6.9 |

The work is flat on blocks; the time grows with faces at about 4 ns per face. That is the `faces_meeting` walk over the ray's box: a long ray's box meets many face boxes before the slab test rejects them. On the sphere's lunes the work grows with the lune count: every lune's box holds the poles and most rays enter all of them before the horizon. Neither is a regression against the pin, which is slower on every row. Both are candidates for PF1:
- a ray-ordered walk of the box tree in place of `faces_meeting` of the ray's box;
- a tighter lune box.

Added work against the pin: none. The pin builds trim trees and fires up to six rays through its topology solver; this fires one clean ray in the common case, and builds nothing.

**Deviations from 72.**
- None in the method.
- The signature is §12.4 row 2's.
- `PointClass` is `non_exhaustive`.
- `BodyqDefect::Ray` names a ray the line constructor refused (a zero-length or non-finite ray, which cannot happen from the table).

**Open questions.**
1. The fallback's local-minimum limit on trimmed tori and ellipse arcs (above): BO-era bodies need a bounded search there, or a proof that some ray is always clean off a measure-zero set.
2. Wire edges and lone vertices, if a caller needs them.
3. Whether `GRAZE` should scale with the face's curvature and the resolution, not stay absolute.

**Proposed 04 row.** "`scx-bodyq` (BQ1): `classify_point(&Brep, p, &mut Budget, &Stop) -> Result<PointClass, BodyqError>`, `PointClass::{In(RegionId), On(Entity)}`, no Unknown. The steps are:
- On, by closed-form feet: vertex, then edge, then face (uv `Inside` only), then lowest id.
- The first clean ray of a fixed 13-direction general-position table: a crossing, `Inside`, past coincidence, `|n . d| > 1e-6`, alone within coincidence, and no edge or vertex within coincidence of the ray to the horizon.
- Else the nearest boundary point's face side, or the angle-weighted pseudonormal at an edge or vertex.

Answers and work are deterministic and independent of which clean ray decides. Change if: a body defeats every ray off a measure-zero set, or a trimmed torus or ellipse arc puts a nearer interior local minimum under the fallback."

### 14.15 TS1 as built (11-27)

New crate `crates/scx-tess` (dependencies `scx-brep`, `scx-curve`, `scx-num`, `scx-nurbs`, `scx-surface`; dev-dependency `scx-make`). Public API:

```rust
pub struct TessSpec { /* chord, resolution, angle, edge_length */ }
impl TessSpec {
    pub fn new(chord: Length, resolution: Resolution) -> Result<Self, TessError>;
    pub fn with_angle(self, radians: f64) -> Result<Self, TessError>;
    pub fn with_edge_length(self, length: Length) -> Result<Self, TessError>;
    pub const fn chord(&self) -> Length; // also resolution(), angle(), edge_length()
}
pub struct Mesh {
    pub positions: Vec<Point3>, pub triangles: Vec<[u32; 3]>, pub normals: Vec<[Vec3; 3]>,
    pub face_of: Vec<FaceId>, pub edges: Vec<EdgePolyline>, pub chord_bound: Vec<(FaceId, Length)>,
}
pub struct EdgePolyline { pub edge: EdgeId, pub points: Vec<u32>, pub chord: Length }
pub struct EdgeSamples { /* params(), points(), chord(), segments() */ }
pub fn sample_edge(edge: Edge<'_>, spec: &TessSpec, budget: &mut Budget, stop: &Stop) -> Result<EdgeSamples, TessError>;
pub fn wireframe(body: &Brep, spec: &TessSpec, budget: &mut Budget, stop: &Stop) -> Result<Mesh, TessError>;
#[non_exhaustive] pub enum TessError { Refused(TessRefusal), Unsupported(TessUnsupported), Exhausted { limit: u64 }, Cancelled, Defect(TessDefect) }
#[non_exhaustive] pub enum TessRefusal { Chord { value }, Angle { value }, EdgeLength { value }, Resolution { spec, body }, TooFine { edge, needed } }
#[non_exhaustive] pub enum TessUnsupported { EdgeKind { edge, kind: CurveKind }, FaceKind { face, kind: SurfaceKind }, NoBound { edge } }
#[non_exhaustive] pub enum TessDefect { Sagitta { edge }, Unindexed { edge } }
```

`TessError` has `kind()` (an `ErrorKind`) and `name()` (a stable snake-case name), `From<Exhausted>` and `From<Stopped>`. `tessellate` is not here: TS2 and TS3 add it on top of `sample_edge`, and `wireframe` is its edge half (`triangles`, `normals`, `face_of` and `chord_bound` empty).

**What it does.** `sample_edge` reads only the edge: its curve and interval, its ends, and the surfaces of the faces that use it. The count is the largest of the curve's own need and each face's need, and a maximum is the same in any order, so two faces meeting at an edge get the same samples bit for bit (§6.2 item 1, §9 row 8). `wireframe` lists the live vertices in id order, then each edge's interior samples in edge id order; each polyline runs from its start vertex's index through its samples to its end vertex's index, so a closed edge starts and ends at one index and a collapsed edge is its vertex twice.

- **Line:** one segment, or `ceil(len / edge_length)`.
- **Circle**, radius `r`, step `h`: the sagitta `2 r sin²(h/4)` in closed form. The step is the least of `4 asin(sqrt(c / 2r))`, a quarter turn, the angle bound, `edge_length / r` and `L_f / r`, and `n = ceil(span / step)`. A post-check evaluates the sagitta at `n` and bumps `n` up to 4 times; a miss after that is `Defect(Sagitta)` (never seen).
- **Ellipse**, semi-axes `a ≥ b`: the affine image of a circle, so a chord over `h` stays within `a (1 − cos(h/2))` of the arc, each way; the tangent turns at most `a/b` per radian. The count is the circle's with `a`, and the angle step divided by `a/b`.
- **NURBS:** split at every distinct interior knot. Per knot span, interval enclosures (`jet_over::<Ivl, 2>`) give `M1 ≥ |C'|`, `m1 ≤ |C'|`, `M2 ≥ |C''|`, and a chord over a parameter step `s` stays within `M2 s² / 8`. The raw count of a span is the largest of `h sqrt(M2 / 8c)`, `h M1 / L_f`, `h M1 / edge_length` and, with an angle set, `h M2 / (m1 angle)` (infinite when `m1` may be 0). `grow` bisects a span while that lowers its count; `merge` joins neighbouring leaves left to right while the joined count is no more than the two apart. A non-finite count is `Unsupported(NoBound)`. The reported chord is the largest `M2 s² / 8` over the pieces.
- **A face's need** is a longest segment `L_f = min(sqrt(8c / κ_f), angle / κ_f)`, with `κ_f` the face's largest normal curvature: `1/r` for a sphere, `max(1/r, 1/(R − r))` for a torus (§12.4 row 6), none for a plane, cylinder or cone. A NURBS face (or any other kind) is `Unsupported(FaceKind)`.
- **Least counts:** a non-line edge has at least 2 segments, a closed one at least 3 (a NURBS edge reaches them by splitting its widest piece). Past `u32::MAX / 4` segments is `Refused(TooFine)`.
- **Budget and stop** (§12.4 row 6): one unit per segment and per NURBS enclosure; the stop is checked per edge.

**Decisions, with cases.**
- **Budget and `Stop` now, not at step 5** (against §6.2 item 6, with §12.4 row 6). For: §12.4 row 6 rules the signature with both, and an edge count can be huge at a fine chord. Against: a closed-form count is cheap and cannot loop. Kept: one signature from the start.
- **Normals per triangle corner** (`Vec<[Vec3; 3]>`, against §6.3's per position). For: a position shared across a sharp edge carries one normal per face, and sharing is what makes the mesh watertight. Against: three times the memory, and a viewer wants per-vertex arrays. Kept: a viewer can split positions from this, while a shared normal could not be split back.
- **Layer: after `scx-make`, last of the library crates** (against §11's order, which puts `scx-tess` before `scx-mass` and `scx-make`). For: a crate may depend, even as a dev-dependency, only on crates above it, and the tests build MK2 bodies. Against: a mesher does not need constructors, and a later crate (a viewer query, `scx-check`) may want to sit between. Kept: no normal dependency of tess is below `scx-brep`, so the order can be relaxed later without code changes.
- **`κ_f` none for a plane, cylinder or cone.** For: they are developable, and along an edge their normal curvature never exceeds the edge curve's own curvature, which the curve's count already holds. Against: a cylinder's curvature across the edge is `1/r`. Kept, with a condition for TS3: its per-triangle test on these kinds must use the developable bound (the chord across a ruling), not `L_f`.
- **No quarter-turn floor for NURBS.** For: the enclosed turn `M2 / m1` is loose, and a free floor on it raised a rational circle's count to 136 against the exact circle's 23. Against: a conic has the floor (the pin's `QUARTER_TURN`), so a NURBS circle and an exact one differ. Kept: a caller wanting a turn limit sets an angle, and that bound is then honoured for every kind.
- **`LEAF = 2.0`, `GAIN = 0.95`, `MAX_DEPTH = 12`** for `grow`. For: an enclosure costs about 2-3.5 us and a segment about 30 ns (below), so halving enclosures is worth a few more segments; at `LEAF = 1` the rational circle r 1, c 1e-3 took 92 segments and 252 enclosures, at 2 it takes 108 and 124; at r 10 and r 100 the segment count is the same and the enclosures fall from 748 to 508 and 1788 to 1020. Against: TS3's interior points grow with the edge count, so fewer segments may pay later. Kept, to re-tune when TS3 measures faces.
- **The merge rule** (join while `ceil(joined) ≤ ceil(left) + ceil(right)`). For: it never adds a segment, and joined runs give equal steps. Against: it is greedy and left to right, so not optimal. Kept: deterministic, and it removes the split cost of `grow`.
- **Least counts 2 and 3.** For: a curved edge in one segment, or a closed edge in two, gives faces that fold onto themselves in TS3. Against: for a tiny curved edge a single segment already holds the chord. Kept: TS3 triangulates from these.
- **`TooFine` is a refusal**, not `Exhausted`. For: the count is past what a `u32` index can hold, whatever the budget. Against: it is a size the caller cannot always predict. Kept: the caller chose the chord, and the error names the edge and the count.
- **Parabola and hyperbola are `Unsupported(EdgeKind)`.** No constructor makes them yet; closed forms can follow when one does.

**Tests.** Unit (12, `src/edge.rs`): circle counts hold the chord and one fewer would not; no circle segment turns past a quarter; angle and length bounds refine a circle; ellipse counts hold the chord, the densely sampled gap within the bound and at least half of it; a rational circle holds the chord against the exact circle, with fewer than 2.5 times its segments; a cubic holds the chord by dense sampling, its interior knot a sample; a stationary NURBS end bounds the chord but refuses an angle (`NoBound`); a count past the index space is `TooFine`; a parabola is `EdgeKind`; a line is one segment unless a length asks; the budget is charged per segment and per enclosure; a face's need is the same in either order. Integration (9, `tests/wire.rs`, on the block, cylinder, frustum, apex cone, sphere and torus, at the world frame and a tilted one): polylines run from their start vertex's index to their end vertex's, closed edges close, collapsed edges are `[v, v]`, every position is a vertex or one edge's sample; every sample lies on each adjacent face through its pcurve at the same parameter, within the zone; every circle edge lies on its circle, its largest midpoint radial gap within the reported chord, and its count equals the closed form (cylinder and frustum r 10: 71, frustum r 4: 45, apex cone r 6: 55, sphere seam r 7: 30 (face-limited), torus equator r 15: 193 (face-limited, `L_f = sqrt(8 c 3)`), meridian r 3: 39); two runs are equal bit for bit; a finer chord never gives fewer points; spec refusals (chord 0, negative, NaN, infinite, the zone; angle and edge length); another resolution is `Refused(Resolution)`; angle and edge-length bounds refine counts; `Budget::new(100)` is `Exhausted`, a set stop is `Cancelled`, and the torus charges exactly 193 + 39. No pin test is ported or removed: the pin's tessellation tests are TS3's (§6.5).

**Measurements** (throwaway timer, not committed; release build, 2,000 repetitions of `wireframe`, world frame; the machine is shared, so a run can vary by tens of percent):

| body | c 0.1 | c 0.01 | c 0.001 |
|---|---|---|---|
| block | 8 pts, 1.0 us | 8 pts, 0.5 us | 8 pts, 0.6 us |
| cylinder r10 h20 | 46 pts, 2.0 us | 142 pts, 4.2 us | 446 pts, 12.3 us |
| frustum 10-4 h15 | 38 pts, 1.4 us | 116 pts, 3.6 us | 364 pts, 9.7 us |
| sphere r7 | 11 pts, 0.5 us | 31 pts, 1.1 us | 94 pts, 2.5 us |
| torus 12, 3 | 73 pts, 2.3 us | 231 pts, 6.6 us | 730 pts, 18.2 us |

About 25-30 ns per point on conic edges. NURBS (unit tests' curves): 0.1-1.5 ms per edge, about 2-3.5 us per enclosure; a rational circle r 1 takes 48 segments at c 0.01 (exact: 23), 108 at 1e-3 (exact: 71), and at r 100, c 1e-3, 804 (exact: 703). A cubic takes 17, 43 and 128 segments at c 0.1, 0.01 and 1e-3 with 18, 50 and 104 enclosures. Added work against the pin: none comparable yet; the pin samples edges inside face planning, and the conic counts here are closed forms with one sagitta check.

**Deviations from 72.** Budget and `Stop` (above, §12.4 row 6 over §6.2); per-corner normals; the layer; `EdgePolyline` carries its chord; `wireframe` and `sample_edge` are public, `tessellate` waits for TS2.

**Open questions.**
1. TS3's per-triangle test: the developable bound for cylinder and cone, and for sphere and torus the triangle's smallest enclosing circle radius against `L_f`.
2. A curvature bound for rational NURBS edges: `|C''|` holds the parametrisation's own acceleration, which is why a rational circle takes about 1.1-2.1 times the exact circle's segments. A bound on curvature and arc length (step 5) would close that.
3. `FaceKind` has no integration test: no body with a NURBS face can be made on main yet.

**Proposed 04 row.** "`scx-tess` (TS1): edge samples are a pure function of the edge, the count the largest of the curve's need and each adjacent face's need `L_f = sqrt(8c / κ_f)`; circle and ellipse counts in closed form, NURBS from interval enclosures of `C'` and `C''` per knot span; the chord is a proved bound reported per polyline; no NURBS quarter-turn floor; budget and stop on every entry; normals per triangle corner; `scx-tess` sits after `scx-make`. Change if: TS3's per-triangle test needs a different face need, or step 5 brings a curvature bound for rational edges."

### 14.17 MA2 as built (11-27)

`scx-mass` closes a face onto its edges for each use whose pcurve carries a recorded accuracy (`Use::bound`), through `face::pieces` (where MA1's `join` runs); a use with none is unchanged. New module `crates/scx-mass/src/closure.rs`; new testkit generator `scx_testkit::bodies::loose`. No public mass API changed.

**The method (the pin's #16735, `gap_closure`).** A pcurve with a recorded bound lies within the bound of its edge, not on it, so two faces of an edge leave a gap and the divergence theorem's fields leak through it, to first order in the bound and differently at each reference. Each such use gets a **strip**, the ruled surface from `P(t) = S(p(t))` to the edge's curve `E(t)`, and each corner of the loop beside one a **fan** over the polygon `P_end, E_end, vertex, E_start, P_start` from its centroid. The faces then share the edges and vertices and bound one closed surface. Both are fluxes of the volume integrands only (`Set::volume_start`): the area is the face's own and gets nothing. The strips are integrated along `t` by the face's own step control (K7, a tenth of the accuracy, the same pieces as the use's Green integral plus the edge's NURBS knots); the ruling is taken by the 3-point Gauss rule, exact for the degree 4 integrands.

**Decisions, with cases for and against.**

- **The correspondence is `t = t`, not a nearest-point grid.** The pin marches 64 nearest points from the trim to the edge and keeps a table. A use's pcurve runs over the edge's own interval and its recorded claim is `|S(p(t)) - C(t)|` at the same `t` (`Claim::PcurveOnSurface`), so `E(t)` is the partner the record states. For: no search, no table, no `correspondence_bends`/`polynomial_strip` special case; a planar use is one K7 piece (7 evaluations); the closed surface is exactly the one the bound speaks of. Against: a recorded bound that is true but whose parameters slip (a pcurve fitted with a different speed) gives an oblique strip, a larger but still closed one; the nearest point would give a thinner strip. The tube term below covers the width the bound states, and `loose` slips parameters on purpose (a half-offset along the pcurve on every second use) and closes exactly on planar faces. Change if a producer records a bound that holds only by nearest point.
- **Closed uses are those with `Use::bound`, not edge or vertex accuracy.** The use's bound is the one that says the pcurve may be off its edge; `finish` allows a use end off its vertex by `use + edge + vertex` accuracy. For: the use's own bound is the pcurve's gap and nothing else's. Against: a use with no bound beside a vertex with a recorded accuracy (a merge that moved a point) has its end off that vertex by up to the vertex's bound, and is not closed there. The rule in 72 §12.1 row 3 names "a gap recorded"; the pcurves such a merge moves would record theirs. Open: confirm when BO's merge exists.
- **A corner is closed when either neighbour is.** The unclosed neighbour contributes its pcurve end as both its pcurve and its edge point (it is exact). No corner with neither is made, so a body with none pays nothing.
- **The pin's gap cones are not made.** A straight uv piece `join` adds (a pole's line, or the gap between two recorded pcurve ends) is part of the face's boundary; the pin closes it onto the vertex by a cone and a corner on each side. Here the corner polygon closes with the chord from `P_start` to `P_end`, which cancels the face's own straight piece exactly on a plane and differs by a sliver of order `gap^2 kappa` elsewhere, inside the ball term below. For: one fan per corner, no extra sides. Against: not watertight to the second order in the gap on a curved surface.
- **The closure's error enters the result.** (1) The strips' Kronrod-Gauss estimates add to the face's error as every piece's do. (2) The tube between the recorded surface and the closed one is added by the divergence of each component's field over that volume: `pi tol^2 L` per closed use (`L` the edge's 16-sample polyline length, `tol` the use's bound) and `4/3 pi tol^3` per corner, times `c R^(a+b+c-1)` for the monomial `x^a y^b z^c n_z`, `R` the face box's longest side (0 for `c = 0`: its field is divergence free). For: the stated error is what the closed surface can differ from the recorded one by, as the pin's doc states (`sum pi tol^2 L`), not only the integration's. Against: it is conservative (the cylinder at offset 1e-3 states 1.1e-4 for a miss of 0, since the planar and vertical strips are exact); and the 16-sample length and `R = size` are estimates, as the crate's other errors are. The area terms get none.
- **`Set::Area` does no closure.** `area`, `face_area` read no strips and no corner; the volume group does.

**The testkit body (`bodies::loose(kind, seed, size, offset)`).** A valid body of `kind` with every line, constant-`v` angle line and circle pcurve moved off its edge: along the loop's left normal by `offset` (a circle by its radius), and by half as much along the pcurve on every second use of a loop; each moved use records the largest distance it measured by 24 samples, rounded up 0.1 percent (`Basis::Measured`), and the others stay exact. `finish` accepts it under the default resolution because the bound says the pcurve may be off; with the bounds forgotten it needs a resolution of the offset (the tests do that to read the unclosed body), which is the check the `Use::bound` exists to satisfy.

**Tests** (`crates/scx-mass/tests/closure.rs`, 5):

- `a_loose_block_closes_to_its_closed_form`: offsets 1e-2, 2e-2, 1e-4: volume, first and second moments equal `solid::block` within 1e-12 (the strips and fans lie in each face's plane, so they close exactly, corners included: the slip leaves the pcurve ends apart, so the fans are not degenerate); the unclosed body leaks at least half the offset; the result about another reference is the same.
- `a_cubes_strips_close_in_one_piece_per_edge_use` (the port of the pin's `a_cubes_strips_close_in_one_piece_per_edge_use`): the closure adds exactly 24 x 7 evaluations to `volume`.
- `an_exact_body_pays_nothing`: a valid block's `volume` is 798 evaluations, MA1's figure.
- `a_loose_curved_body_holds_within_its_stated_error` (the cylinder's strips in `a_cylinders_strips_keep_the_grid`, a cone beside it): within 1e-12 of the closed form, the stated error covers the miss, the unclosed body leaks at least a tenth of the offset, and the closure's tube makes the stated error more than ten times the exact body's.
- `an_exact_pcurve_off_its_edge_is_refused_by_finish` (A8, 72 §10): `broken(PcurveOffEdge)` is refused by `finish`, so a pcurve with no recorded accuracy that is off its edge is a defect found there, not a gap mass closes.

**Not ported, on purpose.** `an_edge_off_its_trims_...`: it reads the pin's tolerant-edge builder, which has no counterpart; the cylinder's circles (exact beside moved lines) and the cone's apex corner cover an edge whose two uses differ. The corpus parity test needs the benchmark corpus (step 6).

**Measurements** (bit-identical before and after: the 84 lines of `mass` over every generated kind at sizes 1 to 3 at accuracy 1e-6 and 1e-9, printed with `{:?}` and the work charged, before and after the change, are equal). Cost where a use records an accuracy: 7 K7 evaluations per planar use (24 for a cube's `volume`, 168 of 966), a corner fan costs none charged (its integral is exact). Cost where none does: a `Use::bound` read per use and nothing else.

**Deviations from 72.** None in signature; §2.2's "gap closure onto the edge's curve" is by `t = t`, not nearest point (above).

**Open questions.** (1) Whether a vertex's or an edge's recorded accuracy alone (no use bound) should close its uses (above). (2) The 1.1e-4 stated against a miss of 0 on the cylinder: a tighter tube bound needs the strip's width per use, which the use's bound already is; what would tighten it is the divergence of the field over the actual swept volume, not `c R^(d-1)`. (3) `loose` moves no pcurve of a sphere's lunes or a torus's patches (angle lines along `v`, and the `finish` pole and seam rules refuse larger offsets); a closure on a pole needs a producer that records one.

**Proposed 04 row.** "`scx-mass` (MA2): a use whose pcurve records an accuracy is closed onto its edge's curve by a strip, `E(t)` against `S(p(t))` at the same `t`, and a fan at each corner beside one, in the volume terms only; the strips' integration error and the tube `pi tol^2 L` (a ball at a corner) are in the stated error; a use with none costs nothing and the results are bit-identical. Change if a producer records a bound that holds only by nearest point, or a vertex's accuracy alone must close its uses."

### 14.18 MA3 as built (11-27)

`Defect(StepControl)` from `mass`, `volume` and `edge_length` on a body far from the origin, found by MK2b (a 2e-4 block at 1e3 at 1e-12 and 1e-13; a 0.1 block at 1e8; a 5e3 block at 1e12, which also ran on for minutes at 1e-12). Fixed in `face::integrate` and `edge_length`; no public signature changed. Tests are `crates/scx-mass/tests/far.rs` (6), written failing first (four failed with `StepControl`).

**Cause.** A point `S(u, v)` is computed in the body's own coordinates, so its last bit is `eps` of its distance from the origin, not of the face's size. The integrands are taken about the face's reference, so the difference `S - reference` is exact but carries that rounding as noise of relative size `eps * |far| / size`: 1.1e-9 for the 2e-4 block at 1e3, 2.2e-7 for the 0.1 block at 1e8, 4.4e-8 for the 5e3 block at 1e12 (with `eps` 2.2e-16). A piece is accepted when its Kronrod-Gauss difference is within a tenth of the accuracy or within `64 eps` of the integrand's size taken about the reference; the noise is above both once a tenth of the accuracy is below it (the step control gave up from 0.05 to 0.3 of the noise, by body), so the piece is bisected to the depth bound (48) or the piece bound (100,000 per call, nested two deep), which is a `Step` defect, or minutes of work. It was not the `about` shift (exact binomial in the values) and not the budget's accounting (the work was charged; the default budget is unlimited).

**Decision: a second attempt held to the rounding, entered only where the rounding can be the obstacle; the shortfall is in the stated error.** Let `noise = 8 eps * (largest |coordinate| of the box) / (its longest side)` (`face::reachable`; zero at the origin; the same for an edge). When `noise <= accuracy / 10` the face is integrated exactly as before. Otherwise the first attempt is the old one limited to 200,000 evaluations in all (`FIRST_WORK`; a call may take 100,000 pieces, nested two deep, which was the minutes); if it ends in `StepControl` the face is integrated again with each piece held to `noise`, and `noise * scale` (the area-and-size scale the pieces' floors use) is added to each component's error. A first attempt that converges is returned as it is. The 84-line `mass` print of §14.17 (every generated kind at sizes 1 to 3, accuracies 1e-6 and 1e-9) is identical before and after, results and work; MK2b's far-box test in `scx-make` (its accuracy is the tolerance, above the rounding) passes unchanged.

**The constant 8 is measured.** An accuracy sweep (a quarter decade apart, 1e-1 to 1e-15) on every generated kind at 1e4, 1e6 and 1e8, with the step control as it was: it gave up at a tenth of the accuracy up to 0.32 of `eps far / size` (the holed slab) and converged from 1.0, so the boundary is body-dependent (cubes 0.005, a prism 0.2 to 0.4, a holed slab about 1). A fixed floor of the tested size would have changed results MK2b's test depends on; entering the second attempt only on failure leaves every converging request alone.

**Cases for and against.**

- *Retry on failure, state the shortfall (taken)* against *a floor applied from the start*, which I built first and measured: a floor of one eighth of the rounding left the prism failing, a floor of 8 changed the error of the MK2b far-box rows (its test fails: stated error above its tolerance). The retry changes nothing that converged. Against: the first attempt's work is spent before the retry (at most 200,000 evaluations a face: the whole `far` file, six tests, takes 6.5 s in debug); and a request that converges within the allowance at noise level returns a value whose error estimate is its own, not the floor. A per-call piece cap, which I tried first (300), made MK3's tilted round bar at 1e-12 give up and take the floor, which that test's tolerance refuses: that case converges, on more pieces than a cap of that size allows, so the bound is on work, not on pieces.
- *Meet as far as possible, state the rest in the error (taken)* against *a typed refusal `Unreachable`*. For meeting: every result carries its error (72 §14.8), so a caller who needs the accuracy reads it; a refusal needs a threshold and the threshold is body-dependent (above). Against: a caller who ignores `error` gets less than asked, silently. Change if a caller wants "accuracy or nothing": `MassRefusal::Unreachable { reachable }` from `reachable` at the entry, a few lines.
- *The noise from the box, not from the surface's own rounding.* Against: a plane at 1e12 and a sphere round differently, and the box only bounds both. For: one rule, no per-surface error model (the evaluation error `scx-surface` will state, 01 §10, is the right input later).
- *Value bias is not removed.* A smooth rounding (a plane's) moves the value without moving the difference of rules, so a request that converges can still be off by the rounding with an error estimate far below it; this was so before and stays (bit-identical results). The second attempt, entered only on failure, is the case that states the floor.

**Measurements.** The far test file (every case that failed, six tests) runs in 6.5 s in debug; the work is bounded by the first attempt's allowance and then by the budget. Reachable accuracies: identical results and work.

**MK3's extrude profiles.** The failing profiles were not available here (`scx-make` is above `scx-mass` in the layers, so the crate cannot build them); the far test runs every testkit kind, prisms and holed slabs among them, at 1e6, 1e9 and 1e12 at 1e-10 and 1e-13, and none is a defect now. If an extrude still gives `StepControl` at a reachable accuracy near the origin, it is another cause, and the test for it belongs in `scx-make`.

**Deviations from 72.** None in signature.

**Open questions.** (1) The typed refusal above, if a caller wants one. (2) The three bodies in the report were not the coordinator's exactly (a transformed testkit block, not `scx_make::block`); the numbers are the same kind. (3) `finish` refuses most testkit kinds at 1e12 with the default resolution (`EdgeEndOffVertex`), so far tests of curved kinds stop at 1e9.

**Proposed 04 row.** "`scx-mass` (MA3): where `8 eps` times the largest coordinate over the box's longest side is above a tenth of the accuracy, a face or edge is first integrated as before within 200,000 evaluations; if that gives up, it is integrated again with each piece held to that rounding and the rounding times the scale is added to the stated error, so far from the origin the result is returned with its error instead of `Defect(StepControl)` or a run of minutes. A request that converges is unchanged bit for bit. Change if a caller needs a typed refusal rather than a stated shortfall."
