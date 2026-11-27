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
| MA3 | per-face memo in `Scratch`, keyed by the face record's bits, its sense and the accuracy; the work rule; a test that memo and recompute are bit-identical across a Boolean that keeps faces | MA1, BO4 |
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
| 3 | per-face mass memo | mass, MA3 | the face's own reference and error scale (66 §13.2); keyed on the face record's bits, its sense and the accuracy; a memo hit charges the recorded work | MA3's memo-against-recompute test; T9 truth once |
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
      │        └─> MA3 (after BO4)
      ├─> MK1 ─┬─> MK2 ──> TS1 ──> TS2 ──> TS3
      │        └─> MK3 (after PL1)
      ├─> BQ1 (after MK2, T10)
      └─> BO1 (after MK1, BQ1) ──> BO2 ──┬─> BO3 ──> BO4 (with BQ2) ──┬─> BO6 ──> PF1
                                         │                           ├─> BO7 (after D5)
                                         └─> BO5                     └─> MA3
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
| MA3 | mass | MA1, BO4 | Opus |
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
- **`Scratch` is taken and unused** until MA3's memo.
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

### 14.10 FU3 as built (11-27)

One segment type, `profile::Seg`: `Line { start, end }` and `Arc { centre, start, end, ccw }`, ends equal for a whole circle. `solid::Seg` and `solid::Loop` and `csg::Seg` are gone; a loop is a `Vec<Seg>` and `solid::extrusion` and `profile_area` take `&[Vec<Seg>]`. `solid` reads each piece's own start (as `profile` does) and refuses a loop whose pieces do not chain, `"loop is not closed"`, as it did a loop that does not return.

**What the others carried that `profile::Seg` lacked, and the decision.**
- `csg::Seg::Arc` gave radius, start angle and a signed sweep; `Circle` was a variant. Decided: derive them in double-double from the ends (radius from the start, angles by `atan2`, sweep in the sense, equal ends the whole circle). For: one meaning of an arc in all three modules; the ends are the data a planar layer will hold, so the oracle now judges the arc the profile names and not one a rounded `sin_cos` moves off it by 1e-16. Against: an angle-specified arc is a different input once its ends are rounded; the old `sweep > 2 pi` and `start` range refusals have no input to refuse; an end off the circle is a new refusal (`Size`, beyond a relative `1e-9`). Adversarial: the Lipschitz argument needs only a closed set and the right parity, not that the ends are on the circle, and the junction gap and arithmetic term already bound a stray end, so soundness does not change. A whole circle with a clockwise sense used to be an arc of sweep `-2 pi` whose chord angle was noise about zero; it is now `Kind::Circle` in either sense (angle `2 pi` inside), which is the sound reading.
- `solid::Seg` had implicit starts and `to` equal to the start for a whole circle: the same convention, kept.
- `profile::Seg` had neither; `loop_area` and `swept` already give `2 pi` and the full disc for equal ends, so the doc now says so (the fixtures still spell a circle as two arcs).

**Results.** `solid`: `profile_area` and `extrusion` of four loops (stadium, stadium with a hole, an off-origin reflex loop, three quarters) and `gear` are bit-identical (Debug output of every field compared old to new, release). `csg`: `signed_distance` at 300 seeded points on each of six profiles is bit-identical, including the arc ones; line-only profiles, whole circles started at angle 0 and a ring are bit-identical in enclosures, `gap`, `bounds` and probes too. Arc profiles with an end on a multiple of pi/2 differ in the enclosure by a few ulp, 3e-16 at most seen (it is tighter: the old junction gap was the `1e-16` by which `cos(pi/2)` is not 0, now only the `2^-90` arithmetic term) and in the probes by at most 8e-16. That the old enclosure was wider for the same drawn profile is the old input's rounding, not a defect; the new one is the truth for the ends as given. Tests in `scx-testkit` in release: 165 pass, 0 fail; no difftest uses these modules.

**Tests.** None removed. Two assertions changed with the type: the `csg` error test's arc of sweep `7.0` expecting `Size` is now an arc whose end is off its circle (the sweep is no longer a field), and the `csg` tests that built arcs by angle use a test helper `arc_at` that rounds the ends the way `fmath::sin_cos` does.

**Proposed 04 row.** "FU3: the testkit has one segment type, `profile::Seg`, a line or an arc by its ends, centre and sense (equal ends a whole circle); `csg` and `solid` take it and derive radius and angles in double-double. Change if: an oracle needs an arc the rounded ends cannot name (a radius with no ends)."

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
