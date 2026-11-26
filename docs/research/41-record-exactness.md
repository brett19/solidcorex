# 41: what the B-rep record cannot hold exactly, and why

Scratch branch `cache-recexact-m` (base da240580a4 + the report-39/40 probe + an `ZZ_EXACT` census and
deviation probe). It is local only and on the server2 side ref only; it was never pushed to GitHub or
the fork. Data: `count-kit/41-data/` (`out.jsonl` corpus, `smoke.jsonl`, `an.py`, `an2.py`, `run.sh`).
Labels: MEASURED (this probe), READ (code/docs/git/issues, with file:line or number), INFERRED.

## 0. Answer first

**Kinds the record cannot store exactly** (READ, verified by MEASURED where the corpus has them):

| Kind | Holds it today | SMLib export (app) | ops `to_record` |
|---|---|---|---|
| `OffsetSurface` (Thicken, Shell, Offset) | nothing | **A**: fitted B-spline, within ½·max zone tol | **R** |
| `Step`, `CurveBounded`, `SrfInVolume` surfaces | nothing | **A** | **R** |
| Corner / Draft / SectionBlend / Sweep definitions (`KeptDefinition`) | nothing; the net is written | definition dropped, net **E** | the same |
| Round definition on a plane / cylinder / cone / sphere / torus net | nothing; the analytic is written | definition dropped, surface **C/E** | the same |
| Hermite, Projected, Iso, Offset, Conic, CrvOnSurf, TangentField, Composite, CrvInVolume curves | nothing | **A** (`approximate_curve`, ½·zone) | **R** |
| Record `SurfaceIntersection` (truck era) | in the schema | never written | **R** on SMLib import (`NotYet`) |

Everything else in the kernel is stored either exactly (E) or as an exact conversion (C) or reparameterisation (P). Those are listed in §2.

**Why (READ unless marked).** The record was never designed to hold "every kernel kind". It came from two sources:

1. **2026-10-13, #461 (truck era).** It began as a kernel-neutral cache format for truck. #3093 (2026-10-20) made it exact for *truck's* vocabulary, which had no offset surface.
2. **Schema 4, 2026-10-20 to 22 (#4012).** It was remodelled on SMLib's flat form, `SmBrepData` (solidcore-record/src/brep.rs:8). That form carries only analytic labels and NURBS. The C++ `FlatFromBrep` itself approximates every other kind: solidcore#1441, ported deliberately for parity in f23aa76d34 (convert.rs:14-15).

From then on a variant was added only when a feature needed its definition to survive:
- `ball_patch` (653c16e6f5, 2026-11-02);
- `blend` (#15010, 2026-11-12).

An exact offset kind was proposed in #13789 (2026-11-04). It is open and unscheduled. The issue records an owner decision: the ops path keeps its typed refusal until the variant lands.

No document gives a reason against exact storage:
- The record computes no geometry, but it only has to *hold* a definition, as `blend` does.
- STEP writers already write the net for kinds STEP lacks.
- Older readers already refuse unknown variants as a cache miss.
- File size is never cited.

So this is an **unfilled gap**, not a trade-off. The approximating fallback and the ops refusal are each deliberate.

It matters more now than when designed. body.rs:785-789 makes the record's import the operand of every next operation: "Every operation starts from the record's import". An approximation written into the record is therefore the next feature's input, not a disposable cache entry (INFERRED).

## 1. Kernel kinds (READ)

**Surfaces** (`SurfaceKind`, solidcore surface/surface/p2.rs:392):

| Kind | Class |
|---|---|
| Plane | exact analytic |
| Cone (includes the cylinder class) | exact analytic, held as rational NURBS plus analytic state |
| Sphere | exact analytic, held as rational NURBS plus analytic state |
| Torus | exact analytic, held as rational NURBS plus analytic state |
| BSpline | exact spline |
| SurfOfRevolution | exact spline (a B-spline subclass) |
| SurfOfExtrusion | exact spline (a B-spline subclass) |
| BallPatch | exact spline (net plus sphere) |
| ProceduralBlend | procedural, with a certified net |
| OffsetSurface | procedural |
| Step | procedural |
| CurveBounded | procedural |
| SrfInVolume | procedural |

A surface may also carry a `KeptDefinition` (procedural_blend.rs:1619): Round, Corner, Sweep, Draft or SectionBlend. Each is a procedural definition over a B-spline or analytic net.

**Curves** (`CurveKind`, curve/curve/p2.rs:570):

| Kind | Class |
|---|---|
| Line | exact analytic |
| Ellipse (includes the circle) | exact analytic |
| Hyperbola | exact analytic |
| Parabola | exact analytic |
| BSpline | exact spline |
| Hermite | procedural or derived |
| Projected | procedural or derived |
| Iso | procedural or derived |
| Offset | procedural or derived |
| Conic | procedural or derived |
| CrvInVolume | procedural or derived |
| CrvOnSurf | procedural or derived |
| TangentField | procedural or derived |
| Composite | procedural or derived |

## 2. Per-path classification (READ; class key: E exact, P same geometry new parameterisation, C exact conversion to another form, A approximated, R refused)

**Record variants** (solidcore-record/src/brep.rs):
- Surfaces: Plane, PlaneThrough, Cylinder, Cone, Sphere, Torus, BallPatch, Spline, RationalSpline, PlacedRevolution, Blend{Procedural | ExactRound}.
- Curves: Line, Circle, Ellipse, Polyline, Spline, RationalSpline, SurfaceIntersection.
- `nurbs` holds an optional form; the pcurve is a mandatory `Curve2` spline.

**Surfaces.** In the table, "SMLib export" is export.rs plus convert.rs `flat_from_brep`; "ops export" is face_surface.rs `record_face`; "SMLib import" is import.rs plus `analytic_of_nurbs`; "ops import" is `face_surface`.

| Kernel kind | SMLib export | ops export | SMLib import | ops import | Round trip |
|---|---|---|---|---|---|
| Plane | P: frame plus uv_scale, written as `plane` | P | P: y recomputed as z×x (import.rs:585), range from corners, trims dropped | E | P (frame drifts, origin up to 105 mm in 39) |
| Cone / cylinder | C: label plus exact NURBS form; a cone becomes a cylinder when \|half\|<1e-12 (export.rs:299) | C, proved | numbers re-derived from NURBS within xsect (convert.rs:2014) | labelled, proved on form | P: last-bit drift (radius 2e-11, half-angle 1e-15) |
| Sphere, Torus | C | C | as cone | as cone | P; a non-ring torus is written as its NURBS |
| BSpline | E (all-1 weights become explicit) | E | E | E | E |
| SurfOfRevolution | C to Spline/RationalSpline (label lost) | C | built as BSpline | BSpline | C |
| SurfOfExtrusion | C to Spline | C | BSpline, or Cylinder if it is an extruded circle (import.rs `analytic`) | recognised | C |
| BallPatch | E (sphere plus net) | E | E | E | E |
| ProceduralBlend | E: `Blend{Procedural}` plus net (with_definitions, export.rs:164) | E, or net if `to_record` is unsupported | re-adopted and recertified (import.rs:740) | E | E |
| KeptDefinition Round, net SurfOfRevolution/BSpline | E: `Blend{ExactRound}` | E | E | E | E |
| KeptDefinition Round, analytic net | definition dropped, analytic written (convert.rs:812 `face_definition`) | the same | analytic | analytic | definition lost, geometry C |
| Corner / Draft / SectionBlend / Sweep | definition dropped, net written | the same | net | net | definition lost, geometry = net (live faces evaluate by net: procedural_blend.rs:1339) |
| OffsetSurface, Step, CurveBounded, SrfInVolume | **A**: `approximate_surface(approx_tol3d(zone/2), 5 subdivisions)`, sampled not certified (convert.rs:1018-1030, approximate.rs:147) | **R**: "a face surface with no NURBS form has no exact record" | BSpline | — | **A** |

**3D edge curves:**

| Kernel kind | SMLib export | ops export (`record_of`, from_parameters.rs:370) | Import | Round trip |
|---|---|---|---|---|
| Line | P: STEP params (a degree-1 NURBS line can also become a Line, export.rs `straight_piece`) | P (negated if reversed) | P | P |
| Circle / Ellipse | C: label plus form; degrees become radians; the `along` frame flips | C, proved | NURBS kept (`same_nurbs_curve`) | P/C |
| BSpline | E | E | E | E |
| Hyperbola / Parabola | C to spline (label lost) | C | BSpline | C |
| Hermite, Projected, Iso, Offset, Conic, CrvOnSurf, TangentField, Composite, CrvInVolume | **A**: `approximate_curve`, ½·zone (convert.rs:429) | **R** | BSpline | **A** |
| record SurfaceIntersection | not written | — | **R** on SMLib (import.rs:723) | — |

**UV trims.**
- The record holds splines only. A pcurve that is not exact in the record's parameters is a fitted B-spline (stated: native-brep.md:198).
- The SMLib export runs `create_missing_uv_trim_curves`, so trims may be made at export.
- SMLib import carries trims only on spline faces (report 37); plane trims are rebuilt.

**Other losses** (READ, MEASURED in 39):
- Edge and vertex tolerance becomes max(own, body).
- EXTENT is recomputed from the vertex box.
- Face domains are re-expressed (analytic domains go out as STEP degrees, then radians).
- Face, edge and use order is canonicalised (`Order`).
- Plane frames are re-derived.
- `tube_start` is set to 0.
- The orientation and the `along` frame flip are re-expressed.
- Attributes are not in the geometry record.

## 3. Which operations make the A kinds (READ)

- **OffsetSurface.** Made by Thicken, Shell and Offset faces (core/offset.rs).
  - Shell or offset of an OffsetSurface gives an OffsetSurface. Shell of a B-spline gives a B-spline (MEASURED in the chain, §5b).
- **CurveBounded, Step, SrfInVolume.** Made by kernel internals only, such as step seams and bend volumes. None reached an export in the corpus (MEASURED).
- **Procedural edge curves.** Made by intersections and projections. None reached an export in the corpus: every exported edge was Line, Circle, Ellipse or BSpline (MEASURED). INFERRED: the operations already convert them to B-splines before the body is final.
- **Corner definitions.** Made by fillet corners.
- **Draft definitions** by Draft, **SectionBlend** by section blends, **Sweep** by sweeps.

## 4. Measurement: the corpus census (MEASURED)

**Run.**
- 63 designs (66 ran; 3 fuzz seeds exited rc=1), giving 58 labels with exports.
- 474 record exports. Total 256 s on server2, one process per design.
- Hook: `body_from_smlib_ordered` → `export_check` (as in 39).

**Faces, 7465 in all** (live kind/definition → written kind (+ nurbs form) → rebuilt kind):

| Count | Live | Written | Rebuilt | Class |
|---:|---|---|---|---|
| 4742 | Plane | Plane | Plane | P |
| 1516 | Cone | Cylinder + RationalSpline | Cone | C/P |
| 718 | BSpline | Spline | BSpline | E |
| 174 | SurfOfRevolution / Round | Blend + RationalSpline | SurfOfRevolution | E |
| 130 | Cone / Round | Cylinder + RationalSpline | Cone | C, Round definition lost |
| 73 | BSpline | RationalSpline | BSpline | E |
| 40 | Cone | Cone + RationalSpline | Cone | C/P |
| 21 | SurfOfExtrusion | Spline | BSpline | C (kind lost) |
| 15 | Plane / Corner | Plane | Plane | Corner definition lost |
| 12 | Plane / Round | Plane | Plane | Round definition lost |
| 8 | Sphere | Sphere + RationalSpline | Sphere | C |
| 5 | Torus | Torus + RationalSpline | Torus | C |
| 5 | BSpline / Round | Blend + RationalSpline | BSpline | E |
| 3 | ProceduralBlend | Blend + (Rational)Spline | ProceduralBlend | E |
| 1 | BSpline (extruded circle) | RationalSpline | **Cone** (cylinder) | C, reparameterised |
| 1 | BSpline / Corner | RationalSpline | BSpline | Corner definition lost |
| **1** | **OffsetSurface** | **Spline** | **BSpline** | **A** |

**Edges, 17724:**

| Count | Live | Written | Rebuilt |
|---:|---|---|---|
| 11055 | Line | Line | Line |
| 3203 | Circle | Circle + RationalSpline | Circle |
| 3017 | BSpline | Spline | BSpline |
| 350 | BSpline | RationalSpline | BSpline |
| 99 | Ellipse | Ellipse + RationalSpline | Ellipse |

No procedural curve was live at any export, and no rebuild failed.

**Live UV trims:** 35446 none, 20705 Line, 14741 BSpline.

**Deviation, live against rebuilt at the same (u, v).** An 8×8 grid on every face; for A faces and dropped definitions, a classified grid with at least 400 points inside the face.
- **OffsetSurface** (fins_joined_into_their_hub, Thicken1, face 7, tolerance 1e-5), over 410 points inside the face:
  - max \|d\| 1.04e-6, mean 5.9e-7;
  - normal component max 1.03e-6 (**10% of the face tolerance**), mean 5.7e-7 (6%);
  - volume −1.31e-4 mm³ (4.7e-7 relative); area −4.2e-9; tight box 0.5 mm (from 40).
- **Corner faces (16) and Round definitions on analytic nets (142):** 0.0 at every sample point. The live face is already evaluated by its net, so dropping the definition moves no point. What is lost is the definition itself: re-editing, `Movable::Exactly`, offsets of the definition (INFERRED).
- **Plane:** \|d\| up to 1486 at the same uv (the parameterisation changes), but the normal component is ≤ 1.1e-13. **P.**
- **Plane / Round:** tangential 11, normal 5e-15. **P.**
- **Cone:** ≤ 1.1e-11 (1332 of 1556 bit-identical). **Cone / Round:** ≤ 1.7e-12. **P (last bits).**
- **SurfOfExtrusion:** ≤ 7.9e-15. **Sphere, Torus, ProceduralBlend, SurfOfRevolution/Round:** 0. **BSpline:** 0, except the extruded circle rebuilt as a cylinder, whose uv domain changes ([3.88, 4.42] to [0, 1.15]), so a same-uv comparison does not apply. Its mass agrees to 1e-15.

**Mass, live against rebuilt** (11 quantities):
- All bit-identical in 76/474 exports. Volume exactly equal in 244.
- Relative volume difference above 1e-15 in 30, and above 1e-12 only in the fins body (4.7e-7).
- The second trip is bit-equal to the first rebuild in 392/474. The record is a fixed point after one trip in 86/474.
- The ops-path rebuild (`solidcore_ops::Body::from_record`) gives bit-equal mass to the SMLib rebuild in only 19/474: last bits, two import paths.

So in this corpus the record is approximate in geometry for **one face in one body** (Thicken). All other differences are last-bit reparameterisation (P/C), plus definitions dropped from Corner and analytic-net Round faces.

## 5. Consequences

**(a) Bodies that can never equal their rebuild** (INFERRED from §2 and 4):
- any body with an OffsetSurface, Step, CurveBounded or SrfInVolume face, or a procedural edge curve;
- any body with a plane or analytic face whose last bits drift. In 39, 154/474 never settled in 6 trips;
- any body with a Corner, Draft, SectionBlend or Sweep definition, or a Round on an analytic net. These are equal in geometry but lose the definition.

Only bodies made wholly of B-spline, BallPatch and exact-class blend faces with Line or B-spline edges are E end to end.

**(b) The chained test.** Fins body; thicken → record → rebuild → op, twice (MEASURED, `smoke.jsonl` and `out.jsonl`):
- E chain: the live body, operated on directly. R chain: as the app does, the record's import is operated on, then recorded.
- Offset of the offset face (+0.2 or +0.02): refused at stage 1 in both chains ("no certified bound" on the neighbours' reach).
- Fillet of the nearest two edges: refused ("edge has no blend (Smooth)").
- Shell, closed and 0.2 thick, succeeded at stage 1:

| Stage | E live V | E's own record trip V | R chain V |
|---|---|---|---|
| 0 | 278.10468299 | 278.10455154 (−4.7e-7 rel) | 278.10455154 |
| 1 (shell 0.2) | 114.24154479 | **114.27251103 (+2.7e-4 rel)** | 114.24161822 (+6.4e-7 rel against E) |
| 2 (shell 0.1) | refused in both: "a gap in its offset's loops cannot be bridged" | | |

- After the shell, E has 15 BSpline + 1 OffsetSurface faces (an offset of an offset stays an offset).
- Face 10 of E, against its own record (tolerance 1e-5, 410 points inside): normal max **2.10e-4 (21× the face tolerance)**, mean 1.33e-4. Mean × area agrees with the 0.031 mm³ change.
- The R chain shelled a B-spline, which yields B-splines (16 faces). Its later record is exact, and it stays within 6.4e-7 of the exact E body.

Interpretation (INFERRED):
- The export fits A faces to **half the body's largest zone tolerance** (convert.rs:735 `max_zone_tol`, :832 `approx_tol = zone/2`), not to the face's own tolerance.
- After a shell, some edge or vertex tolerance is large (≥ 4.2e-4 by this bound), so a 1e-5 face is fitted 21× looser than its tolerance.
- The fit is sampled, not certified (approximate.rs:147 `achieved_tol` is the largest *sampled* distance).
- Error therefore does not compound in the app's R chain, because each step starts from the record. It does not "grow" there; it is re-incurred at each record of an A face, at a level set by the body's worst tolerance.
- If live bodies were kept (an exact cache), the first record of a shelled Thicken would move the face by 13× its tolerance.
- "Twice" could not be measured: every second-stage op was refused on this geometry, in both chains alike.

**(c) What new variants would take** (READ: file-formats skill; #15010 and ball_patch precedents; #13789):
- **Schema.** A `Surface::Offset{base, distance, net, bound}` (and `Curve::Offset`), as #13789 proposes. Exact cases at bound 0 fold back to plane, cylinder, cone, sphere or torus. `deny_unknown_fields` plus a test that an older reader refuses it by name.
- **Format.** One `crates/doc/src/format/pending/<name>.rs` with the older-release note: a cache miss and a warning about the newer format, as v55/blend-definition-faces.rs has. No SCBREP bump if `nurbs` stays required (the #15010 precedent).
- **Code.** Export in export.rs (as `with_definitions`) and in solidcore-convert `to_record`, replacing the refusal. Import builds the OffsetSurface and recertifies the net against it. Record motion (`Movable::Exactly`, body.rs:29-50). STEP, mesher and rig write and read the net.
- **Readers that cannot evaluate an offset** use the net. The flat arrays and the C-API keep the net only.
- **Older designs** have no definition and keep the B-spline. The kind appears only after re-evaluation.
- Corner, Draft, SectionBlend and Sweep need `BlendDefinition` variants in the same way. #15010 lists them as "not carried".

With exact storage and a copying import, would live and rebuilt be the same object? (INFERRED)
- **Geometry, yes,** for these kinds.
- **The rest would still differ:**
  - plane frames and uv_scale (the import re-derives them);
  - analytic numbers re-derived from the NURBS (last bits);
  - tolerance as max(own, body); EXTENT;
  - face, edge and use order;
  - trims rebuilt on planes;
  - the extruded-circle recognition (a new domain).
- To make a rebuild the same object, the import must copy, not re-derive. That rule is at body.rs:785-789: the export is "not exact to the bit".

**Docs** (READ):
- docs/native-brep.md:8-9 claims exactness only for "the truck adapter". :302 says "04 made the encoding exact".
- It does not say that the SMLib adapter approximates offset surfaces or procedural curves, or drops Corner, Draft, SectionBlend and Sweep definitions.
- It does state the fitted pcurves (:198).
- docs/compatibility.md says nothing about the record.

## 6. History behind the "why" (READ; from git and gh, read-only)

| Date | Change | What it says or does |
|---|---|---|
| 2026-10-13 | #461, 9f88537927 | Kernel-neutral record, schema 2, for truck. "refuses what it can't convert exactly". Intersections fitted. Purpose: "#303 needs a reusable geometry cache without tying saved files to a modelling kernel". The cache is "optional, disposable" (#303). |
| 2026-10-14 | #872 | Truck offsets are B-splines fitted through samples. |
| 2026-10-20 | #3093, d0afc31632 | "exact, stored once": SurfaceIntersection, PlacedRevolution, PlaneThrough. Exact for truck's vocabulary. |
| 2026-10-20 to 22 | Schema 4, 445fb23859, b6a3b31ff9, b1326e48d6 (#4012) | Follows SMLib's flat form; `nurbs` forms; Circle and Ellipse. |
| 2026-10-30 | f23aa76d34 (solidcore#1441) | Offset approximation in the flat export, deliberate, for C++ parity. |
| 2026-11-02 | 653c16e6f5 | `ball_patch` (flat kind 6). |
| 2026-11-02 | #13822 | Owner decision: blend definitions belong in the record, not the SMLib C API. |
| 2026-11-03 | f10e779234 | ops `record_face` and `record_of` refuse offsets and procedural curves ("the hop approximated it"). |
| 2026-11-03 | dc38349cbf (#11496) | Moved into `solidcore-record`, "geometry as data … no maths". |
| 2026-11-04 | #13789 (OPEN) | Exact offset record proposed; the refusal stays until then (owner decision). Two benchmark cases are blocked (comment of 2026-11-19). |
| 2026-11-12 | #15010 | `Blend{definition}`. Corners, drafts and section blends "go out as their nets". |

**Stated constraints, and whether each forbids an offset kind:**
- No maths in the record (lib.rs:9-12): no. It only holds data.
- The SMLib flat form and C-API readers (brep.rs:8; #15010): this is the source of the vocabulary, but `blend` shows the record may exceed it.
- STEP: no. Nets are written.
- Kernel-neutral (design-container.md:272-278): shapes *how* a kind is stored, not which kinds exist.
- Schema stability: no. An unknown variant is a cache miss.
- File size: not stated.

**Exactness was a stated goal from #3093.** It was never explicitly traded away; its scope narrowed silently to "what the flat form holds" at schema 4. `Movable::Exactly` (body.rs:29-50) exists because `Blend` faces carry definitions. Which kinds got a variant was decided per feature need (truck exactness, SMLib kind 6, fillets lost on save), not by a policy (INFERRED).

## Cleanup

- The server2 worktree was removed with `xtask worktree-rm`; server logs and temporary files were deleted.
- The local worktree was removed the same way; the `cache-recexact-m` branch is kept locally (scratch, never committed upstream).
- `df -h /`: server2 879G, 398G used, 437G free (48%); local 1.7T, 997G used, 643G free (61%).
