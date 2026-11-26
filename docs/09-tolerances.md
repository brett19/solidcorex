# 09. Tolerances

What solidcore means by "tolerance" today, where its own specification and its code part, and the
model solidcorex adopts: an **accuracy** per entity and a **resolution** per operation. This is step
K of [01-architecture.md](01-architecture.md) §10. Only the types it needs are introduced earlier.

Path prefixes are as in [07-conventions.md](07-conventions.md). The kernel's tolerance code is
`k/geom/tol.rs`; its specification is solidcore's `docs/validity-and-tolerances.md` (cited as "the
spec"). Research 31 is the earlier survey. Its line numbers are from an older commit and are
re-checked here at the pin where cited.

---

## 1. What solidcore does (READ)

### 1.1 Levels

| Level | Where | Default | Read by |
|---|---|---|---|
| Kernel | `Tolerances` (`k/kernel.rs:47`): `zone_tol_3d` 1e-5, `xsect_tol_3d` 2e-5, `approx_tol_3d` 5e-6, `ang_tol_deg` 0.5, `tol_gap_gain` 1.1, `model_size_estimate` 50, `large_small_size_ratio` 1000 (`:68-81`) | only `KernelOptions::default()` in production (research 31) | `zone_tol3d` and friends map `USE_DEFAULT` to these (`k/geom/tol.rs:229-257`). `xsect` and `approx` fields are never read: derived from zone instead. `ang_tol_deg` is read in 11 places. |
| Context | `Context`: `model_size_estimate`, `large_small_size_ratio`, `zone_tol_3d` | always defaults | `context_zone_tol_3d` (`k/geom/tol.rs:394`) |
| Body | `Brep.tolerance` (`k/topology.rs` ~l.775, `m_sThisZoneTol3d`), default `ZONE_TOL_3D` (~l.2259) | 1e-5 | `brep_*` helpers (`k/geom/tol.rs:665-809`) |
| Face | `Face.tolerance` (`k/topology.rs` ~l.480) | **0** (`Face::new`) | |
| Edge | `Edge.tolerance` (~l.427) | `ZONE_TOL_3D` | |
| Vertex | `Vertex.tolerance` (~l.390) | 0 in `Vertex`, `ZONE_TOL_3D` from the builder | |
| Edge use | none of its own | | |
| Curve, surface | cache their owner's tolerance; ownerless ones fall back to the kernel's (research 31) | | `zone_tol_3d_of_curve` (`k/geom/tol.rs:948`) |
| Angle | kernel only: no body or entity angle tolerance | 0.5° | intersectors use 20° literals; `curve_class` uses `6 * ang_tol_rad()` = 3° (research 31) |
| Record | `Brep.tolerance`, documented as the **intersection** tolerance (`rec/brep.rs:42`) | | convert writes the *least* of the edge and vertex tolerances there (`cv/brep/record.rs:28-29`) |

### 1.2 Derived tolerances

- `size_estimate_to_zone_tol_3d(size)` (`k/geom/tol.rs:331`): a decade table (07 §1).
- `zone_tol_3d(k, small)` (`:378`).
- `xsect_tol_3d_from_zones(a, b) = a + b` (`:434`).
- `contact_tol_3d` = the kernel zone, kernel-wide by design (`:450`, WR-787).
- `within_contact_tol`, widened by γ₃₂ × size (`:483`).
- `coincidence_tol_3d = max(contact, zone1, zone2)`. This is not upstream, which used the sum, and
  is still being migrated. A NaN zone stays NaN (`:519`).
- `zone_reach_3d` (`:547`).
- `calc_edge_zone_tol_3d`, `calc_vertex_zone_tol_3d`, `calc_gap_zone_tol` (`:612-660`).
- `map_to_1d` (`:907`): a 3D tolerance divided by |derivative| gives a parameter tolerance, or
  interval/100 where the derivative is near zero.
- `update_object_tolerance` (`:1863`): the shared raise.
- The comparison zeros `EFF_ZERO`, `scaled_zero` (`:1005-1067`), `is_zero`, `are_same`, `in_tol`
  (`:822-875`). These are numeric, not geometric.

### 1.3 Writers and raises

| Writer | What it does | Source |
|---|---|---|
| Primitives | `set_tolerance`, grow-only | research 31 |
| Boolean | result takes the larger body tolerance; `matched_tolerances` (`k/boolean/operand_tolerances.rs:57`) lowers the coarse operand to max(0.25 × the fine body's shortest edge, the fine tolerance); imprint grows vertex, edge and face tolerances to measured distances | `boolean/topology_intersector/p1.rs`, `p2.rs` via `update_object_tolerance`; spec §8 |
| Import | clamps to [1e-8, 1e-4] × size, or takes the stored value verbatim with `force_same_tol`; entities become max(own, body) | research 31, 39 |
| Heal: `Fix_TolSizes` | size buckets; **can shrink** a tolerance | spec §7 |
| Stitch | grows a vertex to 1.001 × the glued gap | spec §7.3 |
| `validate_and_update_tolerances` | grows to 2 × gap, when under 1/800 of the diagonal | spec §7 |
| Tree build | raises 2 × gap during the face-tree build (393 builds write the body; 271 raises across the goldens; none is a valid body needing it) | research 45, 46 |
| Lazy trim creation | raises the edge by the trim's proved distance (default) or sampled distance (a file's translation) | `Caches::prove_lazy_trim_raise` (`k/kernel.rs` ~l.150); `TrimCoverRaises` (`k/topology.rs:869`) records the import's raises |
| Scale | `scale_tolerances` | research 31 |
| Owner-less trim | four temporary kernels carry an edge tolerance into `Curve::trim` | research 31 |

### 1.4 Where tolerances enter keys

`FaceCacheKey`, ops' `Settings::of` (`ops/body.rs:89-100`, which keys the `Outward` proof by
`k.tolerances`, `k.fixes` and three context numbers), `CurveMemoKey`, and the analysis memo all
include `k.tolerances` (research 31).

## 2. The spec, and where the code departs from it (READ)

The spec's rule (§1, l.37): **"a tolerance grows only to a bound, never to a sample."**

- **§5.1** sets out the levels:
  - the body zone, with local tolerances floored at the body's and τv ≥ τe
  - xsect = the sum of the zones
  - the size-class table
  - Test 82 at a ratio of 1000 (2000 for vertices)
- **§5.2** gives what an edge's tolerance must cover:
  τe = max(τ0, max τf, 1.1·g_ee/2, 1.1·g_ef − τ0). A constructed edge's bound is δ + max εi.
- **§8** gives the growth decision:
  - imported data grows only to an *enclosed* gap, recorded
  - a construction's tolerance is set from its certificate
  - a gap above the certificate is a refusal or a recomputed curve
- **§9** lists 11 invariants. Number 9: "a constructed result's tolerances come from its
  certificates". Number 10: "imported data may be given larger tolerances only to an enclosed gap,
  recorded".

The code departs from the spec in these places, most of which the spec says itself (§8 "Where the
code stands", §10):

| # | Spec | Code |
|---|---|---|
| D1 | Grow only to a bound | Gaps are 20 samples (`calc_max_edge_face_gap`); `Fix_TolSizes`, `validate_and_update_tolerances`, the imprint and stitch all grow to samples |
| D2 | Growth is recorded | Only the import's trim-cover raises are recorded (`TrimCoverRaises`); no other growth is |
| D3 | Imported and constructed data are treated differently | The same code raises both |
| D4 | Construction's tolerance from its certificate | Constructed edges do not carry δ; general SSI is not certified (`ssi.md` §6.1) |
| D5 | Local ≥ body, τv ≥ τe (invariant 5) | Not checked; `Face.tolerance` defaults to 0, below the body's 1e-5 |
| D6 | A check changes nothing (invariant 11) | The tree build writes to the body, and its 2 × gap raise widens the tolerance of the very defect the check is checking (research 45) |
| D7 | Fit to the budget the acceptance uses | `fit_tolerance` (`k/topology/edgeuse/p3.rs:426`) is τe alone; acceptance uses τe + τf; the fitter ignores the gap G (research 34) |
| D8 | One meaning for the body tolerance | Kernel body = zone; record body = "intersection tolerance" but filled with the least entity zone (§1.1) |
| D9 | Tolerances scale with the model | `ZONE/10` guards and `LARGE_SMALL_SIZE_RATIO * b.tolerance()` use constants; the stored ratio is never read; at least six literal copies of the zone (07 §6) |
| D10 | `new_brep` takes the kernel zone | it takes the context zone, which is 1e-5 even if the kernel's is not (research 31) |
| D11 | Healing only grows | `Fix_TolSizes` can shrink |

## 3. The model (JUDGED)

Two quantities, never one field serving both.

**Accuracy** (`num::Accuracy`, a length ≥ 0, per entity)

- **Meaning.** A proved upper bound on how far the stored entity lies from the exact entity it
  stands for. A vertex is a ball of that radius, an edge a tube, and a face a band.
- **For a construction**, it comes from the certificate: δ + max εᵢ for an edge, the net's ε for a
  procedural face, and 0 for exact classes (definitions doc §5).
- **For imported data**, it is the stated value, raised only to an enclosed gap and recorded
  (§6 below).
- **It never grows to let a check pass.**

**Resolution** (`num::Resolution`, a length > 0, per operation)

- **Meaning.** How close two things must be for the operation to treat them as the same point,
  curve or surface: coincidence, contact, snapping, merging.
- **Source.** It comes from `Settings` (`settings.resolution_for(body_size)`), is passed to the
  operation, and is never stored on a body.
- **Default.** It is 1e-5 mm for parts sized 0.5 to 500 mm, which is today's zone, so nothing moves
  by default.

An operation's decisions read both. Two entities coincide when their distance is at most
`resolution + acc₁ + acc₂`. That is how today's `coincidence_tol_3d` and `xsect_tol_3d_from_zones`
combine, made explicit.

An angle resolution `num::AngleTol` (radians) sits in `Settings` beside it. The 20° and 3° literals
become named, documented constants of the operations that use them: they are algorithm thresholds,
not tolerances.

### 3.1 Every use of "tolerance", classified

| Today | Is | Becomes |
|---|---|---|
| `Vertex.tolerance`, `Edge.tolerance`, `Face.tolerance` | accuracy (also misused as resolution where read as "zone") | `accuracy: Accuracy` |
| `Brep.tolerance` (body zone) | resolution, used as an accuracy floor | removed from the body; resolution in `Settings`. An imported body's stated value is kept in the `Record` header as `stated_resolution`. |
| record `Brep.tolerance` ("XSectTol3d") | ambiguous (D8) | `stated_resolution`, written as the resolution the body was built under |
| `Tolerances::zone_tol_3d`, `contact_tol_3d`, `context_zone_tol_3d` | resolution | `Settings.resolution` |
| `xsect_tol_3d_from_zones`, `coincidence_tol_3d` | resolution + accuracies | `num::coincident(res, acc1, acc2)` |
| `approx_tol_3d` (zone / 2) | a fitting budget | an explicit `budget: Length` argument to fitters, derived from the accuracy being certified |
| `fit_tolerance` (`k/topology/edgeuse/p3.rs:426`) | a fitting budget (D7) | `budget = acc_e + acc_f − G` per research 34/35 |
| `ang_tol_deg` | angle resolution | `Settings.angle: AngleTol` |
| `tol_gap_gain` 1.1 | a margin on a *sample* | deleted: a bound needs no margin |
| `model_size_estimate`, size table | resolution policy | `Settings::resolution_for(size)` |
| `large_small_size_ratio` | validity limit (Test 82) | `num::consts::MAX_ACCURACY_RATIO`, documented as a validity limit |
| `EFF_ZERO`, `scaled_zero`, `EFF_ZERO_PARAM` | numeric zeros | `num::EPS_LEN`, `EPS_REL`, `EPS_PARAM` (07 §6), not tolerances |
| `map_to_1d` | resolution in parameter space | `num::ParamTol`, from a resolution and a derivative bound |
| `blend_net_tolerance` | an evaluation switch | an argument choosing definition or net |
| gaps (`max_edge_face_gap` etc.) | measurements | diagnostics only; a proved gap is a bound in a `Cert` |

## 4. How two bodies combine (JUDGED)

Today a Boolean result takes the larger body tolerance, and the coarse operand is lowered
(`matched_tolerances`).

In solidcorex there is no body tolerance to combine:

1. **The operation's resolution** is `settings.resolution_for(max(size_a, size_b))`, one value for
   the whole operation, so both operands are judged alike.
2. **Each entity keeps its accuracy.** Entities that survive unchanged keep their accuracy and their
   `Arc` (research 51).
3. **New entities** (intersection edges, split faces) take the accuracy of their certificate.
4. **Imported operands with stated resolutions** larger than the operation's are refused only if an
   entity's accuracy exceeds `MAX_ACCURACY_RATIO × resolution`. Otherwise the operation proceeds at
   its own resolution. The coarse operand is never lowered or raised.
5. **The result's `stated_resolution`**, when written to a record, is the operation's.

Alternative: keep "larger of the two", for fidelity. Rejected, because it makes a fine part coarser
by touching a coarse one, which is exactly the history dependence of research 33.

## 5. What a certificate is, as data (JUDGED)

A certificate is a stored claim, its bound, and what the claim depends on:

```rust
// in brep (needs entity ids); the bound arithmetic is in num and geomq
pub struct Cert {
    pub claim: Claim,          // what is bounded
    pub bound: Length,         // proved: true distance <= bound, rounding included
    pub method: Method,        // Exact, Interval, Bernstein, Taylor, Krawczyk (definitions doc §4.2)
    pub reads: Reads,          // the entity versions it was computed from
}

pub enum Claim {
    PcurveOnSurface { edge_use: EdgeUseId },        // |S(p(t)) - C(t)|
    CurveToIntersection { edge: EdgeId },           // delta: C within delta of S1 ∩ S2
    NetToDefinition { face: FaceId },               // epsilon of a procedural face
    EndsAtVertex { edge: EdgeId, end: End },
    Outward,                                        // a predicate: valid and facing out
}
```

- A `Cert` lives in a fill-once cell of the entity it is about, never in a side table.
- `reads` names the inputs (curve, surface, vertex positions) by entity id. Because a changed entity
  is a new entity in a `Draft`, a certificate can never outlive its inputs.
- An entity's `accuracy` is `max(cert.bound)` over its certificates. It is derived and stored at
  `finish`, never set by hand except on import.
- Exact classes carry `Method::Exact` and bound 0, with no runtime proof (01 §5).
- An operation declares what it preserves: a rigid move keeps every `Cert`, a uniform scale
  multiplies bounds, and a Boolean keeps those of untouched faces.

Today's analogues: the definitions doc §4.1 (ε for nets), `Verdict` (`k/geom/certified.rs:56`),
ops' `Outward` keyed by settings (`ops/body.rs:62-100`), and `TrimCoverRaises`.

## 6. When a raise is legitimate (JUDGED)

A raise is an increase of an entity's accuracy after it was set. It is legitimate only when:

1. **The data is imported**, not constructed by solidcorex.
2. **The gap is enclosed**, not sampled. A bound comes from an interval, Bernstein or Taylor
   enclosure.
3. **The tube stays clear.** The raised tube or ball reaches no entity it does not bound (spec §8,
   invariant 7), and the ratio stays within `MAX_ACCURACY_RATIO`.
4. **It is recorded** as a `Raise { entity, before, after, cert }` in the body's import report, and
   in the `Record` when the body is written back.

Every other raise is a **defect** (the construction's certificate was wrong) or a **refusal** (the
input cannot be built within its budget). In particular:

- **Creating a pcurve never raises.** It is fitted to its budget, as in design T (research 35).
- **The tree build and the validity check never write** (spec invariant 11).
- **Healing may shrink an accuracy** only to a proved bound. The shrink is recorded like a raise.

## 7. Migration (JUDGED)

### 7.1 What `num` defines now (step B)

These are mechanical moves. Nothing changes value.

- `Length(f64)`, `Accuracy(Length)`, `Resolution(Length)`, `AngleTol(f64)` in radians,
  `ParamTol(f64)`. These are `Copy` newtypes with no arithmetic between `Accuracy` and `Resolution`
  except through `coincident(res, a, b) -> Length`.
- `num::consts`, holding one definition of each of `ZONE_DEFAULT`, `EPS_LEN`, `EPS_REL`,
  `EPS_PARAM` and `MAX_ACCURACY_RATIO`. The six literal zone copies (07 §6) are replaced by
  references to it, which is a no-move change.
- `num::tol::zone_for_size(size)`: today's `size_estimate_to_zone_tol_3d`, unchanged in value,
  with half-open ranges documented (the boundary sizes 0.5, 500, ... are tested to give today's
  answers).
- `num::Tolerances`: a plain value with today's seven fields and values, passed by `&` where
  `k.tolerances` is read today. This is the first piece of `Settings`.

### 7.2 What must wait

- **`Cert`**, `Claim`, `Reads`: until `brep` exists (step E) and `geomq` holds the bound routines
  (step C). They reference entity ids.
- **Removing `Brep.tolerance` and splitting entity tolerance fields into accuracy**: step K.
  - Until then, the entity fields keep `f64` and their current meaning.
  - `Accuracy` wraps them at read sites, so the type documents intent before the values change.
- **Fit-to-budget trims** (research 35): step K.
- **Deleting `tol_gap_gain`**, the tree-build raise and the sampled growth paths: step D for the
  tree-build raise (no consumer, research 45/46), and step K for the rest.
- **Changing the Boolean's combination** (§4): step K.

Research 31 estimates 100 to 150 sites in about 60 files for the step-K changes.

## 8. What results would move (from existing measurements only)

| Change | Measured effect | Source |
|---|---|---|
| Step B / §7.1 types and constants | none: same values, same order | by construction |
| Default 1e-5 bodies under the accuracy/resolution split | none expected: resolution default equals today's zone | research 31 |
| Imports with non-default stated tolerance | may move; not measured | research 31 |
| Remove the tree build's step 2 (corner weld and 2 × gap raise) | 54 golden rows in 7 units (80 on a current main, 04 §5) | research 45 |
| Fit-to-budget trims (design T) | refuses 40 trims that main builds; 3.3× main's time; area up to 2.2e-6 relative, volume 1.8e-9; goldens: `edgeuse_p3` 72 changed, 3 broken; `face_p3` 62 changed, 4 broken; `mass.integrator` 32 changed with status 0→1001 | research 35 |
| Exact record round trip (frames copied, radians, no max(own, body)) | today only 22% of 474 exports keep mass bit-identical; 154 never settle within 6 trips; the fins body is off by 5e-7 | research 39 |
| Keeping certificates (no re-proof) | no result; removes the 19.4 s (38%) of wall time in the certified bound, of which 22.6% repeats identical inputs | research 47 |
| Unfusing the 19 evaluation `mul_add` sites | one rounding at those sites; not measured | 07 §7 |

---

## Questions for the owner

1. **Refusals of design T.** Fit-to-budget refuses 40 trims that main builds today. Accept those
   refusals as correct (the trims cannot meet the acceptance budget), or require a fallback before
   step K lands? The kernel rule "valid input never becomes a refusal" applies only if those inputs
   are valid.
2. **Body tolerance.** Remove the body tolerance entirely (resolution in `Settings`, a stated value
   in the record header only), or keep a per-body resolution so that a coarse imported body stays
   coarse in later operations?
3. **Angle tolerance.** Is a per-body angle tolerance wanted (04 §6)? This document puts the angle
   in `Settings` only.
4. **Combining bodies.** Is the operation's resolution taken from the larger operand's size (§4), or
   fixed for the session?
5. **Recorded raises.** Record raises in the stored `Record` (a format change), or only in the import
   report?
6. **Healing.** May healing shrink an accuracy at all? Today `Fix_TolSizes` does; the model allows it
   only to a proved bound.
7. **Step D first.** Remove the tree build's raise in step D (54 to 80 derived rows), ahead of the
   rest of step K?
