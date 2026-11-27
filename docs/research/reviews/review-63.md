# Review: research 63, step 2-3 idiom prep

This is an adversarial review of `docs/research/63-step2-3-idiom-prep.md`, checked against:
- 04 §1 and the §3 rows to 11-27;
- 01 §1.1, §2, §7 and §10;
- 57;
- 62 §10;
- the pin.

Pin paths are relative to `solidcore/crates/solidcore/src` unless they start with `docs/`, in which
case they are relative to the pin's `solidcore/`. Each finding is marked VERIFIED, with a file:line
or command, or JUDGED. No code was changed.

**Verdict: revise before tasking.** Most of the X rulings and D rows hold. Three findings change the
plan:
- D1 contradicts merged scx-num.
- 63 changes the uv parameterisation of every analytic kind without saying so.
- D10 rests on a wrong reading of the pin and leaves out the trim lessons we have already paid for.

## 1. Findings, by severity

### HIGH

**H1. D1 (±inf ends) contradicts merged `scx-num`, and the graph has no task for it.** VERIFIED.
- A11 says every routine that needs a finite range "takes an Extent1d/Extent2d (always finite by
  construction)". That is false today:
  - `crates/scx-num/src/extent1d.rs` (type doc, ~160-245) says "Unbounded ends are stored as
    `±INFINITE_PARAMETER`" and that a NaN end is held.
  - `Extent1d::UNBOUNDED` is `[-INFINITE_PARAMETER, INFINITE_PARAMETER]`.
  - `shift` and `times` test for the sentinel (`extent1d.rs:~306`, `:~472`).
- D1 therefore needs a step-1 change to scx-num that no task in §10 covers:
  - drop or redefine `UNBOUNDED`;
  - make `new` reject non-finite ends;
  - remove the sentinel branches in shift, times and `is_infinite_param`.
- The pin carries two more lessons beside the 1234567 sentinel (`math.rs:61-65`), and 63 addresses
  neither:
  - `INFINITE_PARAMETER_SQRT = 1111.111` exists because a parabola squares its parameter.
  - `BOUNDED_INFINITE_PARAM = 33.0` is the half-size used where an unbounded range must be sampled.
  - 57 §5.1 lists both for 01 §10.1 item 3.
- JUDGED: `±inf` in a `ParamDomain` is a sound direction, but it is not "fails loudly":
  - `inf * 0` and `inf - inf` give NaN, and NaN comparisons are silently false.
  - The rule has to be that no arithmetic touches a `ParamDomain` end. Every consumer clamps to a
    finite working `Extent` first. The working extent comes from the face's uv box for surfaces,
    and from a stated bound otherwise.
  - The pin shows why the bound matters: `offset/offset_geometry_creation.rs:~6885` (#18922) found
    3.4e-11 rounding on a plane copy sized at 1.2e6.

**H2. The uv parameterisation of analytic kinds changes, but no D row says so.** VERIFIED.
- At the pin, sphere, torus, revolution and extrusion evaluate through their base NURB:
  - `surface/surface/p2.rs:1711` (`kind_with_nurb`) and `evaluate` at `:2190-2240`;
  - `surface/cone/p1.rs:1570` evaluates "at the NURB parameters".
- Their uv is the rational NURB parameter. 63's evaluators (§4) use natural parameters, with angle
  periodic in 2π, as the pin's `eval/label.rs` does. That is a change of convention, and it reaches
  further than 63 notes:
  1. **Exact pcurves.** On a natural-parameter cylinder, an oblique plane section is
     `v = a cos u + b sin u`. No `Curve2` kind in 63 holds that exactly, so it becomes a fitted
     pcurve. At the pin the same section is exact: the UV curve is rational of degree 2p in the
     NURB parameters (`intersect/surface_drop_lib/cylinder_section.rs`, WR-680). The pin's open
     item asks for more of these, not fewer (`docs/booleans.md:~1024`: "Exact pcurves for sphere,
     cone and torus sections, so that exact edges stop carrying fitted trims").
  2. **Conversion.** Reading pin bodies needs a uv reparameterisation of every pcurve on an
     analytic face. Reading STEP needs none, because STEP uses the angle, which counts for natural
     parameters.
  3. **Difftests.** uv-valued comparisons with the pin need a mapping, or they fail by design.
  4. **D4.** A non-similarity turns a cylinder into a rational NURBS. Its uv then changes from
     angle to NURB parameter, so every pcurve on that face must be refit or reparameterised, with
     recorded accuracy. 63 does not mention this.
- JUDGED: natural parameters may still be right, because STEP, the closed forms and periodicity
  are all simpler with them. Two alternatives must be weighed and the choice recorded:
  - a `Curve2` kind for trigonometric curves, which keeps sections exact;
  - general quadric kinds under an affine map, which would let D4 keep the kind.
- Add a D row for the uv convention. Add the reparameterisation as a truth for converters (a pin
  uv and the scx uv map to the same 3-D point).

**H3. D10's premise is wrong: the pin fits pcurves, and 63 leaves out what fitting needs.** VERIFIED.
- The §6.1 row "Pcurves exact or refused, with nothing fitted" cites `eval/pcurve.rs:1-13`, `:320`.
  That module is SolidCraft's record path only.
  - `eval/pcurve.rs:316-320`: `exact_pcurves` returns `None` for "the uses whose pcurves the face's
    kernel makes ([`Trims`])". It hands off to the kernel, and the kernel fits.
  - `topology/edgeuse/p2.rs:652` `rebuild_uv_trim_curve` and `:696`
    `check_and_set_uv_trim_curve`. The kernel fits a trim and records its max distance. It accepts
    the trim when an 8-sample distance is under 100× tolerance or a tenth of the edge length.
    Otherwise it grows the edge tolerance, or it uses a re-dropped curve that is more than twice as
    close.
  - `docs/booleans.md` §7 (~924-940): exact edges carry trims fitted by `Surface::drop_curve`, and
    SSI uv curves are uncertified cubic Hermite.
  - Commit `c5a17eb9e2`: "tangent_chain builds a missing pcurve instead of refusing the edge".
- The direction of D10 (fit, and record the accuracy) is right. The row has to absorb our own trim
  research (27, 28, 34, 35, 37, 41, 47), which 63 does not cite:
  - **Who fits, and when.** The writer fits, eagerly, before `finish`. 04 §4 withdrew lazy fitting,
    and research 27 showed fitted trims are order-dependent when made on demand.
  - **The target.** Fit to the accuracy the body claims and never raise it in advance (27 D2). The
    acceptance test must use edge tolerance plus face tolerance plus the existing gap G (34: the
    pin's fitter targets the edge tolerance only). Record an accuracy only when the result is worse
    than the resolution, or tolerances creep up.
  - **The basis and its cost.** Say whether the recorded accuracy is certified or measured.
    - 35: a certified design refused 40 trims and cost 3.3× creation time, 76% of it in the G
      bound.
    - 47: a certified trim has a median of 12.8 ms against 69 µs without one, and 1% of trims take
      73% of trim time.
    - This is a hot path, so the before/after rule applies to the plan itself.
  - **Layering.** T6 is a testkit oracle. A test can use it to check the library's figure, but it
    cannot be how the library measures. The library needs its own measure in `scx-brep` or below.
  - **Poles, seams and periodicity.** The pin's validity lessons are not carried: Edgeuse 7 (no
    jump across a period), Edgeuse 20 (no dog-leg at a pole), Face 24 (a pole is a vertex). See
    `docs/validity-and-tolerances.md`. The pin also never measures a pcurve against its edge
    (`:229-231`, `:441`). That is a missing truth, and T6 should be it.
  - **Timing.** Curve fitting is step 5 in `scx-nurbs` (01 §10.2). The fitted half of BR5 cannot
    land in step 3. The graph shows BR5 depending on CU3 and SU2 only.
  - **Records.** 37 (plane trims lost on read) and 41 (what records cannot hold exactly) bear on
    RE1. The plane-trim carry stays held until records store exact forms (04, 11-25).

### MEDIUM

**M1. D8 and A10 refuse valid bodies.** VERIFIED against 01 §2.1.
- 01 §2.1 says an edge collapsed to a point has no curve, and a lone vertex (a cone apex) is a loop
  of one use on a collapsed edge.
- 63 makes `EdgeGeom { curve: Curve }` required and has `finish` refuse `MissingGeometry`, so it
  rejects that body. Make the collapsed edge its own variant, or make `curve` optional and valid
  only when collapsed.
- "Refuses" is also the wrong kind. A missing curve from our own operations is `Defect` (62 D11).
  It is `Refused` only for user-built input.
- 63's `EdgeUse` also lacks the parameter interval that 01 §2.1 gives it.

**M2. 63 has not caught up with 62's ruled error design.** VERIFIED by reading 62 §10.
- The scope line says 62 "is in progress", so no cause is kinded. Against 62 §10 and the 04 11-27
  rows, they should be:
  - `CurveError::TooLarge { cap }` is a fixed cap, so `Exhausted` (M1).
  - `Degenerate` is `Refused` (62 §3.1).
  - `OutsideDomain` is `Refused`.
  - `ReadCause::Truncated`, `BadMagic` and `BadTag` are `Refused`. `BadVersion` is JUDGED to be
    `Unsupported`. `Invalid(Box<FinishCause>)` inherits or re-judges (D11), and the `Box` matches
    D14.
- No frame is marked transparent or context (D12).
- Several sketches are infallible but iterate: `invert -> SurfaceInversion`,
  `intersect_curve -> Vec<CurveSurfaceHit>`, and the NURBS paths. They take no budget and cannot
  report `Exhausted` or `Cancelled`. That contradicts 63's own X1 ("budget and Stop") and the 04
  budget row.
- Decide per routine whether it is bounded (closed form, so infallible is fine) or iterative (so it
  returns a `Result` and is metered).

**M3. SU5 puts the plane section in the wrong crate.** VERIFIED.
- 01 §1.1 gives `scx-ssi` "the plane section of a surface". 63's SU5 puts `section_by_plane` in
  `scx-surface`.
- Either is defensible, since a plane section is closed-form per kind. The choice needs a D row and
  an edit to 01. As it stands, SS2's dependency on SU5 runs across the layer the other way.

**M4. A1's benchmark is not shown to be fair.** JUDGED.
- The numbers are `Vec<Vec3>` 26.4-27.1 ns against `[Vec3; 3]` 17.2 ns, and nested Vec 55.5-57 ns
  against `[[Vec3; 3]; 3]` 7.9 ns.
- 63 does not give the code, does not say the inputs were `black_box`ed, and does not say every
  output was consumed.
- 7.9 ns for a 3×3 jet with no allocation is plausible, but it is also what partial constant
  folding or dead-store removal would give. The nested-Vec figure is mostly allocator cost.
- The conclusion (fixed arrays) is probably right. Rerun it before D3 is cited as measured:
  - `black_box` every input point and parameter;
  - fold every output component into a checksum that is itself `black_box`ed;
  - commit the bench next to the research.
- The other hot-path costs are unmeasured:
  - **Jets.** A const-generic order `D` is cheap. `SurfaceJet` storage is unspecified (a triangle
    or a full square), and a full square at order 2 wastes a third.
  - **Generic scalars.** Evaluators generic over `T: Ring` behind `match` dispatch must
    monomorphise to the same f64 code. State a before/after on the f64 path when it lands.

**M5. The task graph's critical path and parallel claims.** VERIFIED against 63 §10's own arrows.
- The critical path P1 → SU1 → SU2 → SU4/SS1 → SS2 leaves out:
  - NS1 (SU2 needs it) and NS2 plus T3 (SU4 needs them);
  - the `scx-nurbs` landing itself.
- SS2 depends on SU5, which hangs off SU1 and CU1, not SS1.
- Arrows that are missing:
  - RE1 is marked "none", but it needs D1's Extent change.
  - BR1 needs P2 and D1.
  - The D1 scx-num change (H1) is not a node at all.
- Tasks marked parallel that are not independent:
  - CU2 is "parallel with stubs", but it needs CU1's names.
  - T6 is "from day one", but it needs generic-scalar evaluators (CU1, SU1, NS2).
  - T2, T3 and T5 need the kinds' types or `Affine` support in testkit.

### LOW

- **L1.** VERIFIED. A13 says "Kinds in scx have public fields". 01 §7.4 says fields are private or
  `#[non_exhaustive]` at publication, and 63's own sketches use private fields. Reword A13.
- **L2.** VERIFIED. `RegionKind { Outside, Solid, Void }` is 57 §5.1's proposal, but 01 §2.1 still
  says `solid: bool`. Edit 01, or cite 57.
- **L3.** VERIFIED. 63's `EvalError` collides with the existing `scx_algo::error::EvalError`.
  Rename one.
- **L4.** VERIFIED. 57 F3 rules `Side { Lo, Hi, Inside }` for boundaries, testkit has
  `Side { Below, Above }` (`crates/scx-testkit/src/nurbs.rs:26`), and X5 adds an evaluation side.
  Three `Side`s will meet in one prelude. Give the evaluation one a distinct name, or one shared
  enum.
- **L5.** VERIFIED. 57 F6 rules `enum Inversion { Inside(f64), Outside(f64), Failed }`. 63's
  `SurfaceInversion` should either use that shape, minus `Failed` once inversion returns `Result`,
  or say why not.
- **L6.** VERIFIED. D11 (our own record format from v1) is already 01 §7.4. Cite it rather than
  re-deciding it.
- **L7.** VERIFIED. 63's citations and counts matched where checked:
  - X5 against `nurbs/b_spline_surface_n_lib/p2.rs:176-186` and `curve/curve/p2.rs:335-345`;
  - scx-record at 3,248 lines;
  - `&mut Kernel` at 6 uses;
  - `INFINITE_PARAMETER` at 206 uses, led by `bend_volume` (36), `extent3d` (35) and
    `transform` (18).

## 2. X1-X12 against 57 and 62

- **X1-X4, X6-X9, X11 and X12** are consistent with 57's ruled shapes. Examples: options plus a pure
  call, no setters, no out-params, and iterators over views.
- **X5** (evaluation side) is consistent with the pin, but see L4 for the name.
- **X10** (errors) is the gap. It is written as if 62 were open (M2). 62 §10 is ruled, and X10 must
  kind every cause, say how wrappers kind theirs (inherit or re-judge), mark each frame transparent
  or context, and put a budget on every iterative entry point.

## 3. Verdicts on D1-D12

| D | Verdict | Why |
|---|---|---|
| D1 ±inf ends | **Revise** | Add the scx-num Extent change as a task before RE1 and BR1. Rule that no arithmetic touches a domain end, and that consumers clamp to a finite working extent. Address `INFINITE_PARAMETER_SQRT` and `BOUNDED_INFINITE_PARAM` (H1) |
| D2 | Holds | Note that the facade adapts to scx, never the reverse (57 §5.2) |
| D3 fixed-array jets | Holds, pending a fair rerun | M4 |
| D4 non-similarity gives NURBS | **Revise** | Every pcurve on the face must be refit or reparameterised, and its accuracy recorded. Take the NURBS extent from the face's uv box, not from an infinite domain (H1, H2) |
| D5 | Holds | Needs the companion uv-convention row (H2) |
| D6 | Holds | |
| D7 | Holds | Reconcile with 57 F6's `Inversion` (L5) |
| D8 required edge geometry | **Revise** | A collapsed edge has no curve. A missing curve from our own operations is `Defect` (M1) |
| D9 | Holds | A bitset is fine |
| D10 fitted pcurves | **Holds in direction; rewrite the row** | The premise is wrong (H3). Add eager fitting by the writer, the fit target with G, the bound basis and its cost, the pole and seam rules, and step-5 timing for the fitter. T6 is a test oracle, not the library's measure |
| D11 own record format | Holds | Already 01 §7.4 (L6). Kind the `ReadCause`s (M2) |
| D12 Frame in scx-num | Holds, with notes | Store `z` rather than recomputing it (testkit's `Frame` at `crates/scx-testkit/src/analytic.rs:38` does). Drop `y` as a stored field, or keep it as the only one derived. Under the 11-27 mirror rule a mirrored frame is left-handed, so "right-handed by construction" needs either a handedness flag or a ruling that a mirror maps a frame to NURBS or to a flipped frame |

## 4. Missing truths and lessons, collected

- Trim research 27, 28, 34, 35, 37, 41 and 47 (H3).
- The pin has no measure of a pcurve against its edge. Make T6 that truth (H3).
- WR-680's exact cylinder section, as a truth for whichever uv convention is chosen (H2).
- A truth that the uv reparameterisation preserves the 3-D point, for converters (H2).
- `INFINITE_PARAMETER_SQRT` for parabolas, and `BOUNDED_INFINITE_PARAM` for sampling (H1).
- Edgeuse 7 and 20 and Face 24, the pole and seam rules (H3).
- The held plane-trim carry (04, 11-25).
