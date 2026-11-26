# 55 - A certified bound on the trim-to-edge gap on offset and blend faces

A fallback trim is recorded with its accuracy only when that accuracy is proved (54, point 4).
Today the proof is missing on two surface kinds: offset surfaces and procedural blends. This
report compares ways to prove an upper bound on the trim-to-edge gap there, recommends one,
says what solidcorex needs to adopt it, and measures it on the 24 refusals of report 35 that
fall in these kinds.

- Labels:
  - READ means read from the code at the solidcore pin (03e16c9ee7), or from solidcorex docs.
  - MEASURED means run: in the prototype `prototypes/trim-gap-bound/`, or in solidcore at the
    pin with temporary instrumentation that was never committed.
  - JUDGED means reasoned from the above.
- Paths: `k/` is `solidcore/crates/solidcore/src/`. `csg.rs` is `k/query/curve_surface_gap.rs`,
  `p3.rs` is `k/topology/edgeuse/p3.rs`.
- Raw data, the probe patch and the run logs are kept outside this repository.

## The quantities

A face's surface `S`, an edge's curve `C(t)` on `[t0, t1]`, and the edge use's UV trim `c(t)`.

- **B**, the trim gap: how far the trim's image `S(c(t))` is from the edge. Two forms are in
  use. The *same-parameter* gap is `max_t |S(c(t)) - C(t)|`. The *Hausdorff* form allows a
  reparametrisation `psi` and bounds `max_t |S(c(t)) - C(psi(t))|`; the pin's
  `trim_deviation_bound` (p3.rs:3163) gives this one (READ, csg.rs:4106 `Correspondence`).
  Every bound of the same-parameter form is also one of the Hausdorff form (JUDGED).
- **G**, the edge gap: how far the edge is from the surface, `max_t dist(C(t), S)`. Report 35's
  trim builder T spends the budget `L - G` on the trim, so it needs G certified too.
- **L**, the limit: the edge's tolerance plus the face's.
- **eps**, an approximated surface's certificate: `max |B(u, v) - S(u, v)|` between the
  stored net `B` and the definition `S`, at the same parameters.

T accepts a trim when its certified B is at most L (35).

## Verdict

**Recommended method: an interval Taylor model with adaptive bisection, in centred
second-order form on pieces within one C^2 patch of the surface, through the offset formula;
for a blend, the same bound on its net plus the blend's stored certificate eps.** On a piece
that crosses a patch line where the surface is only C^1, the centred form is not valid: the
plain second-order form is used there for the surface itself, and the first-order form for an
offset of it (see "Corrections after review").

- **Offsets.** Bound `g(t) = Q(c(t)) - C(t)`, with `Q = S + d N` and
  `N = S_u x S_v / |S_u x S_v|`, piece by piece. On a piece `[a, b]` of width `h`, take
  `max(|g(a)|, |g(b)|) + h^2/8 sup|g''|`. Enclose `sup|g''|` in centred form,
  `|g''(m)| + h/2 sup|g'''|`, from jets carried through the chain rule. Bisect the piece with
  the largest bound until the bound is within a thousandth of the largest proved value (tight),
  or until it is at most the tolerance (within).
  - The normal's variation, the base's derivatives and the lower bound on `|S_u x S_v|` are
    not derived separately. They fall out of one jet evaluation of `Q` over the piece.
  - In the prototype the tight bound is 1.0001 to 1.0010 times the sampled maximum on every
    case it resolves, including the torus offset to its cusp (MEASURED).
  - solidcore already has this enclosure for offsets (`offset_and_base_partials_over_piece`,
    `k/blend/fillet_section/offset_box.rs:70`, used through `NetSurface::of_exact`,
    csg.rs:139). The trim bound B uses it. The edge bound G does not: it takes
    `NetSurface::of` (csg.rs:2524), which has no net for an offset (READ). Routing G through
    `of_exact` is the whole change for offsets (JUDGED; MEASURED below).
- **Blends.** For the same parameters, `|S(c(t)) - C(t)| <= |B(c(t)) - C(t)| + eps` (the
  triangle inequality at one `(u, v)`). The net is a B-spline, so the first term is the bound
  solidcore already has. eps is stored on the blend (`m_dBound`,
  `k/surface/procedural_blend.rs:271`, "the net's certified distance from the definition").
  So the composed bound costs one net bisection and an addition (JUDGED).
  - In the prototype the composition always holds. It is tight when eps is small next to the
    gap, and loose by the ratio eps/gap otherwise: 276x for a coarse 2 x 2 net, 4.1x for
    an 8 x 8 net (MEASURED).
  - A direct bound on the definition is tight (1.0006) where the definition is closed form
    (MEASURED). The kernel's procedural blend is not: its section is an implicit solve, so the
    direct route needs a certified section solver and is not recommended now (JUDGED).

**On the 24 refusals of report 35, the two changes certify all 24.** The offsets give G at
1.0004 to 1.0010 and B at 1.0003 to 1.0005 of sampled. B sits at 0.91 L, but that figure is
fixed by the setup (L was set at 1.1 times the sampled G, and B tracks G), not a measure of
headroom. The blends give B at 0.3%
to 1.6% of L. In the kernel probe each bound costs 14 to 415 ms tight, and 13 to 54 ms when
asked only whether it is within L (MEASURED, section 2; these are solidcore timings, not the
prototype's).

**Rejected.**

- **Plain Lipschitz** (`|g(m)| + h/2 sup|g'|`). It is first order, so its piece count grows
  as 1/tol rather than 1/sqrt(tol). It needs 6 to 15 times the pieces of the second-order
  form where it resolves at all. It fails to resolve at small gaps and at the cusp
  (ratios 2.9 to 82 at the cap) (MEASURED).
- **The direct interval extension** of `|g|`. The dependency problem makes the enclosure as
  wide as the piece's motion of `S(c(t))`, not its gap. It never resolved (ratios 60 to
  50 000) (MEASURED).
- **A precomputed global Lipschitz or curvature constant** for the offset (normal variation
  `|dN| <= |II| / |S_u x S_v|_min` times the base derivatives). It is valid but is the
  worst case over the whole face, which is the plain Lipschitz form with a looser constant
  (JUDGED from the Lipschitz rows).

## Corrections after review

Review 55 found four faults in the prototype and two in this report. Fixed in the
prototype (`src/gap.rs`, tests in `tests/patch_lines.rs`) and here.

- **C1. The centred form needs C^2 on the piece.** `|g''(m)| + h/2 sup|g'''|` bounds
  `sup|g''|` only where `g''` is continuous. Across a C^1 patch line of a net it is not: on
  a piece whose middle is on the flat side, `g''(m) = 0` while the far side bends. The
  prototype now marks a piece whose trim enclosure meets a patch line, and there takes the
  plain `sup|g''|` (`d = 0`). The review's own example (true gap 0.4865) did not break the
  old code: the jets' overestimation of `g'''` covered the jump. The same construction
  with the peak 0.001 past the line did: the old centred bound on `[1.0975, 1.1015]` was
  0.0022008, below the sampled 0.0022010. With the fix, every tested piece holds and the
  bound resolves within 1.01 (MEASURED). Decision row 11-26 asks for pieces to be cut at
  the lines. The prototype uses the per-piece fallback instead; both are sound, and cutting
  is tighter near a line (JUDGED).
- **C2. An offset across a C^1 line is only Lipschitz.** The normal is only C^0 there, so
  `g'` jumps, and no second-order form holds, the plain one included. Old code, on an offset
  at `d = 2` of a net with `z_uu` jumping by 0.2: on `[1.099, 1.101]` the centred bound was
  0.204292 and the plain one 0.204310, both below the sampled 0.204478 (MEASURED). A piece
  that crosses a line now takes the first-order form `|g(m)| + h/2 sup|g'|`, and the bound
  then holds and resolves within 1.01 (MEASURED). This also corrects decision row 11-26,
  which gives the plain second-order form across a C^1 line. That is right for the surface,
  not for an offset of it (JUDGED).
- **C3. What a capped run returns.** The code returned the largest piece bound with
  `resolved = false`, and its documentation said infinite. The documentation now matches
  the code. The value is proved but did not meet the stop. Section 3, point 5 says why it is
  kept finite (JUDGED).
- **C4. Trims that touch the domain's boundary.** The interval enclosure of such a trim
  crossed the edge at every depth, so the old code never resolved. On a trim from `u = 0` to
  `u = 1` of the wavy patch it reached 995 pieces and stopped with the bound infinite
  (MEASURED). A uv coordinate proved monotone on a piece is now narrowed to the range of
  its end values, which is exact. That trim now resolves within 1.01. The sphere's pole rows
  moved with this fix (section 1).
- **C5. B/L = 0.91 is 1/1.1 by construction** (section 2). The edges' L was set at 1.1
  times the sampled G, and B tracks G. The ratio shows the bound is tight; it is not room
  under L (JUDGED).
- **C6. Which cost is which.** "Within is barely cheaper (2 to 16%)" is the prototype, with
  the tolerance at 1.1 times the gap. "Within is 9 to 12 times cheaper" (owner question 3) is
  the kernel probe on the vase offsets, against L. Both are now labelled.

Re-run of `gap-bench` after the fixes (MEASURED):
- Offset section: identical to every printed digit except time. None of these cases has a
  patch line or touches the boundary.
- Pole section: changed by C4 (section 1's table).
- Blend section: net bounds, eps and the composed figures unchanged. Net pieces rose 9 to
  25% (282, 579, 1301 to 307, 660, 1625), because torus-net pieces that touch a line no
  longer take the centred form.
- Section 2's kernel figures come from the solidcore probe, not the prototype, so these
  fixes leave them unchanged.

## 1. Methods per surface kind

### Offsets

All four were built in the prototype (`src/gap.rs`) and run on the same cases. The cases are a
wavy bicubic patch offset both ways, a torus `(3, 1)` offset out (`d = 0.5`), in
(`d = -0.99`), and to its cusp (`d = -1`, where the offset collapses onto the spine circle),
and a sphere offset along trims passing `delta` from the pole. Ratios are bound over a
200 000-point sampled maximum (MEASURED, `gap-bench`).

| case | Centred | Second | Lipschitz | Direct |
|---|---|---|---|---|
| Bézier d=0.05, 5 trim / 7 edge spans | 1.0009; 1303 pieces; 106 ms | 1.0010; 1790; 73 ms | 1.0010; 11 891; 650 ms | 60.8 at cap |
| Bézier d=0.05, 20 / 28 spans | 1.0008; 7887; 642 ms | 1.0010; 16 774; 688 ms | 82.3 at cap | 50 809 at cap |
| Bézier d=-0.2 | 1.0006; 1378; 112 ms | 1.0005; 1879; 77 ms | 1.0010; 10 415; 569 ms | 66.2 at cap |
| torus d=0.5 | 1.0009; 355; 9 ms | 1.0009; 1194; 15 ms | 1.0010; 16 904; 282 ms | 135 at cap |
| torus d=-0.99 | 1.0001; 743; 19 ms | 1.0009; 3237; 41 ms | 2.99 at cap | 3441 at cap |
| torus d=-1 (cusp) | 1.0009; 860; 22 ms | 1.0010; 4354; 56 ms | 3.26 at cap | 3873 at cap |

"At cap" is 32 768 pieces with the bound still above the stop: the method's answer is then
that bound, or a refusal.

- Centred against plain second order: 1.4 to 5 times fewer pieces. Each centred piece costs
  about 1.8 times the interval operations (third-order jets), so the wall time is lower
  except on the two 5-span Bézier cases, where it is about 1.45 times higher (MEASURED).
- **Within** the tolerance is barely cheaper than tight at 1.1 times the sampled gap (2 to 16%
  fewer pieces with Centred). At 10 times it saves 16 to 50% (MEASURED, the `within:` rows of
  the prototype's bench). This is the prototype, with the tolerance at 1.1 times the gap. The
  "9 to 12 times cheaper" of owner question 3 is the kernel's bound on the vase trims, where
  the within stop is asked about L and the kernel's tight bound spends most of its time past
  that point; the two figures measure different code at different settings.
  The saving is small because the bisection spends most of its pieces near the maximum,
  where both stops need them (JUDGED).
- **Assumptions.**
  - The trim and the edge are polynomial on each piece: the pieces are cut at both curves'
    knots.
  - The surface is C^2 on each piece where the centred form is used. A piece that crosses a
    C^1 patch line takes the plain second-order form (`g''` jumps but stays bounded, and
    the enclosure over the piece takes both sides), or the first-order form for an offset
    (the normal is only C^0 there, so `g'` jumps). The first version of this report said
    the centred form held across such lines; it does not (corrections, C1).
  - The offset is evaluated by the formula enclosed, `S + d N`. In solidcore that is
    `EvaluatePlainOrSimple`, and `of_exact` refuses every offset that may evaluate otherwise:
    an offset of an offset, an extended base, `allow_extension` (READ, csg.rs:139-177).
- **Failure modes.**
  - *Degenerate normals (a pole, or a collapsed edge of the base).* The bound needs
    `|S_u x S_v|` proved positive on each piece. Where it is not, the prototype encloses the
    normal as unbounded and bisects. Measured on the sphere:

    | `delta` from pole | Centred | Second |
    |---|---|---|
    | 1e-1 | 1.0009; 507 pieces | 1.0010; 698 |
    | 1e-3 | 1.0002; 1145 | 1.0006; 1595 |
    | 1e-4 | 1.0008; 1338 | 1.0010; 1954 |
    | 1e-6 | 1.0008; 1891 | 1.0009; 2727 |
    | 0 (through the pole) | refused | refused |

    The piece count grows slowly, by a few hundred per decade of `delta`, and every row off
    the pole resolves (MEASURED). The first version of this table doubled per decade and hit
    the cap at 1e-6. That measured mostly something else: the trim rises to
    `v = pi/2 - delta`, next to the domain's edge, and its interval enclosure crossed the
    edge, which the prototype then treated as not enclosed (corrections, C4). At `delta = 0`
    the trim meets the pole at its turning point, where the normal is degenerate and the
    enclosure cannot be narrowed, and both forms refuse. solidcore refuses
    such a box outright when the base's first partials might be a pole's
    (`10 EFF_ZERO (1 + |S|)` scaled, csg.rs:371-411), because the offset's evaluator then
    steps off the pole and no longer follows the formula (READ). That refusal is correct and
    should be kept (JUDGED).
  - *Rolling-ball cusps* (`d = -1/kappa`, the offset folding onto a focal curve). The bound
    does not degrade (1.0009 at `d = -1` on the torus, MEASURED). The bound needs the base's
    normal, which is regular there, not the offset's, which is not (JUDGED). What breaks at
    a cusp is the face, not the bound: past it the offset turns inside out. That is a
    modelling refusal for the offset builder, not for this bound.
  - *Near-singular parametrisations* (base partials nearly parallel, but not a pole). These
    are the pole case with a small `|S_u x S_v|`. They cost more pieces as the angle shrinks,
    slowly on the sphere's rows (JUDGED from the pole rows).
  - *Large offset distance on a fine base.* `d` multiplies the normal's enclosure width, so a
    large `|d| kappa` needs more pieces (the torus `d = -0.99` row against `d = 0.5`). It
    stays resolved (MEASURED).
- **Cost.** Per piece: one third-order jet evaluation over the piece and one second-order
  evaluation at its middle, each a base evaluation with its partials to order 3 or 4, plus
  a normalisation. 9 to 640 ms per trim in the prototype's unoptimised code (MEASURED).

### Procedural blends

Built in the prototype as a torus "blend" (a constant-radius rolling-ball blend is a torus)
approximated by C^1 Hermite nets of 2 x 2, 4 x 4 and 8 x 8 patches (`src/eps.rs`,
`cases::blend`). The edge lies on the definition (MEASURED, `gap-bench` blend section):

| net | eps bound / sampled | eps cost | B on net | composed B + eps | composed / sampled gap |
|---|---|---|---|---|---|
| 2 x 2 | 4.82e-3 / 4.77e-3 | 1286 boxes; 3.4 s | 4.51e-3 | 9.33e-3 | 276 |
| 4 x 4 | 3.10e-4 / 3.07e-4 | 18 012 boxes; 48 s | 2.76e-4 | 5.85e-4 | 17.3 |
| 8 x 8 | 1.10e-4 / 1.93e-5 (cap) | 65 536 boxes; 171 s | 2.95e-5 | 1.39e-4 | 4.1 |

The sampled gap on the definition is 3.38e-5. B on the net is tight in every case (ratio to
the net's own proved lower value 1.001).

- **Composition (recommended).** B on the net plus eps.
  - *Assumptions.* eps is a same-parameter bound over the region the trim covers, so
    `|B(w) - S(w)| <= eps` at every `w = c(t)`. The blend's evaluation is the definition at
    those parameters. In solidcore the net's `(p, q)` map affinely to the definition's
    `(s, t)` (`m_adA`, procedural_blend.rs:274), and the bound is recertified as
    `|B - H| + |H - S|`, control nets then `gn_certify_span`
    (`k/blend/fillet_section/certificate.rs:4217`) (READ). Whether that recertified figure is
    same-parameter everywhere, extended pieces included, is an owner question (below).
  - *Failure modes.* Loose when eps is large next to the gap: the stored eps is the worst
    case over the whole blend, not along the trim. It fails outright where eps is not
    finite, or where the net does not cover the trim's region (an extended blend past its
    net).
  - *Cost in the kernel.* The net bisection only, since eps is stored (JUDGED). In the
    prototype eps itself is costly: 3 to 171 s for the whole domain in 2D (MEASURED). That
    is the price of recertifying, not of bounding a trim.
- **Trim-local eps (a cheaper, tighter variant).** Bound `|B - S|` only along `c(t)`, a 1D
  bisection of the same kind as B, instead of over the whole domain. It removes most of the
  looseness, because the composition becomes `max B + max eps along c`, both on one
  parameter. Its cost is one certified evaluation of the definition per piece. For the kernel's
  blend that needs the certified section solve that recertify already performs per span
  (JUDGED; not built).
- **Direct** bound on the definition. Tight (1.0006, MEASURED) when the definition has
  closed-form jets. For the kernel's implicit section it needs an interval Newton enclosure
  of the section solve with its derivatives (JUDGED; not built).
- **Global Lipschitz on the definition.** Rejected for the same reason as for offsets.

## 2. The 24 refusals in solidcore

The 24 are report 35's OFFSET class (20 trims: vase1 x8, vase2 x4, vase3 x8, each an edge
B-spline on an `OffsetSurface`) and its NO-PROOF class (4 trims on variable_round's
`ProceduralBlend`). All are on stripped bodies, each counted once per mate pair (35).

Method: in a scratch solidcore worktree at the pin, never committed, two thread-local switches
were added (patch kept outside this repository):

- `exact`: the edge bound's bisection (csg.rs:2524) takes `NetSurface::of_exact` in place of
  `NetSurface::of`;
- `blend`: `NetSurface::of_exact` takes a procedural blend's net as it is, with no probe, and
  `net_to_surface_bound` (csg.rs:1630) returns its eps.

These are the two changes the verdict proposes. For each edge use the probe records L, the
fit, G certified by the pin and with both switches, T's budget from the certified G, a trim
built at that budget with the pin's builder (`create_uv_trim_curve_fitted`, p3.rs:518), the
pin's B and the switched B (tight, within L, and same-parameter), and the sampled gaps of
that trim (20 001 points with a local refinement).

48 edge uses were probed (the 24 and their mates). Each mate gave the same figures as its
partner, so the table counts canonical trims. All figures are MEASURED. Ratios are to the
sampled value. "T's trim" is the trim built at the budget `t = (L - G)/1.5` from the
certified G.

| group | trims | L | G pin | G certified (ratio; ms) | t / fit | B on T's trim (ratio; tight ms; within-L ms) | B / L |
|---|---|---|---|---|---|---|---|
| vase1 | 4 | 8.34e-4 | refused | 7.581e-4 (1.0004; 35-41) | 0.061 | 7.582e-4 (1.0005; 142-148; 14) | 0.910 |
| vase1 | 4 | 1.081e-3 | refused | 9.837e-4 (1.0010; 24) | 0.061 | 9.830e-4 (1.0004; 133-136; 13) | 0.910 |
| vase2 | 3 | 7.55e-4 | refused | 6.872e-4 (1.0009; 14) | 0.061 | 6.869e-4 (1.0004; 189-190; 20) | 0.910 |
| vase2 | 1 | 8.67e-4 | refused | 7.892e-4 (1.0007; 21) | 0.061 | 7.889e-4 (1.0003; 243; 20) | 0.909 |
| vase3 | 4 | 6.82e-4 | refused | 6.202e-4 (1.0005; 27-28) | 0.061 | 6.202e-4 (1.0005; 149-153; 13) | 0.910 |
| vase3 | 4 | 5.41e-4 | refused | 4.923e-4 (1.0007; 38-39) | 0.061 | 4.921e-4 (1.0003; 158-163; 15) | 0.909 |
| variable_round | 2 | 8.0e-5 | refused | 2.667e-7 (eps; 4) | 1 | 2.668e-7 (sampled 5.7e-14; 45-51; 36-37) | 0.003 |
| variable_round | 2 | 8.0e-5 | refused | 8.07e-7 to 1.31e-6 (1.28 to 1.58; 384-415) | 1 | 8.09e-7 to 1.31e-6 (1.28 to 1.58; 117-160; 50-54) | 0.016 |

**All 24 certify.** None remains unprovable once both changes are in.

- **Offsets (20).**
  - The pin already certifies B on these faces. `Bpin` equals the switched B to every
    printed digit: the trim bound goes through `of_exact`. G alone was refused.
  - With the `exact` switch, G certifies at 1.0004 to 1.0010 times the sampled gap, in 14 to
    41 ms.
  - The edges' tolerances were set at 1.1 times the sampled G (35). So the certified G sits at
    0.909 to 0.910 of L, and T's budget is about 6% of the fit tolerance.
  - The trim built at that budget has B at 1.0003 to 1.0005 times its sampled Hausdorff gap,
    and at 0.909 to 0.910 of L. That B/L is 1/1.1 by construction: B tracks G to within
    5e-4, and G was placed at L/1.1. It shows the bound is tight, not that there is room
    under L (JUDGED). The tight bound costs 133 to 243 ms. Asking only "within L"
    costs 13 to 20 ms. (These are kernel timings, from the probe.)
  - For context, main's trim (built at the full fit) certifies at B above L on 8 of the 20:
    vase2 x4 (1.015 to 1.033 L) and the smaller-L vase3 x4 (1.048 L). It is accepted on the
    other 12. T's budget, possible only with a certified G, is what brings those 8 within L.
  - On vase2, main's same-parameter gap is 1.05 to 1.16 times its Hausdorff gap, so the
    Hausdorff form matters there. On T's trims the two forms agree to 1e-4.
- **Blends (4).**
  - The pin refuses both B and G: `NetSurface::of` fails its probe on a procedural blend
    (csg.rs:104-118).
  - With the `blend` switch, B is the net bound plus eps = 2.667e-7. That is 0.3% to 1.6% of
    L, so every trim is accepted with room to spare.
  - On two trims the gap itself is at the rounding level (5.7e-14). There the bound is all
    eps: correct, but 4.7 million times the sample. On the other two the gap is 5e-7 to 1e-6,
    and the bound is 1.28 to 1.58 times it.
  - The cost is 45 to 160 ms tight and 36 to 54 ms within L. G on the two curved trims costs
    384 to 415 ms, the slowest figure here.
- **Every bound is at least its sample.** No B was below its sampled Hausdorff gap, no
  same-parameter B below its sampled same-parameter gap, and no G below its sampled G, on all
  48 (MEASURED).
- **The within-L stop.** It returns a bound at most L, or infinity when the gap is over L.
  Examples are main's vase2 and vase3 trims above, refused at 5 to 20 ms. So "over L" is
  answered faster than "how far over".

## 3. What solidcorex needs

1. **Interval Taylor jets over a parameter box.** A scalar type carrying the value and the
   first three derivatives along one parameter, generic over the scalar so jets nest for the
   two surface directions. `scx-num`'s `Ring` and `TayScalar` traits are enough to build it on
   (the prototype's `src/jet.rs` does, in about 200 lines) (MEASURED: it builds and certifies).
2. **One surface evaluator, generic over that scalar, per surface kind.** The bound is only
   as good as the agreement between what it encloses and what the kernel evaluates. An offset
   must evaluate by `S + d N` everywhere the bound accepts it. The evaluator must refuse where
   it would step off a pole. The refusals of `of_exact` (offset of an offset, extended base,
   extension) carry over as "no bound" (JUDGED, from READ csg.rs:139-177 and 371-411).
3. **One bisection for both G and B.** The pin's two paths took different net constructors,
   and that alone left G uncertified on offsets. In solidcorex, G and B should be two
   correspondences of one bound (same parameter, nearest point), over one prepared surface
   (JUDGED).
4. **A certificate on every approximated surface.** A same-parameter `eps` with the region it
   holds over and the parameter map to the definition. This is 54's point 1. A blend without one
   is "unproven" by construction. The certificate should be stored per span, not only as one
   worst case, so a trim crossing a few spans can take theirs (JUDGED).
5. **Caps and a distinct flag.** A piece cap (the pin's `MAX_PIECES`, 2^15, csg.rs:66) and
   a depth cap. Hitting either returns the bound reached, which is still proved, with a flag
   that the stop was not met (the prototype's `resolved = false`). The flag decides: under
   the within stop it means "not shown to be at most L", so T refuses; under the tight stop
   the value may be recorded only as a proved but loose bound, never as the tight figure. The
   prototype's capped rows show why the flag matters: the Lipschitz and Direct rows of
   section 1 stop at the cap with valid bounds 2.9 to 50 000 times the sample (MEASURED). Returning the finite value rather than infinity keeps
   what callers need: 54's point 4 asks for the accuracy reached, and a proved loose bound
   is a truthful record of it, where infinity would discard it (JUDGED).
6. **Within-tolerance stopping.** The caller needs "is it at most L", not the tight value.
   The saving is small near L (above), but it lets an easy trim stop after a few pieces
   (JUDGED).

## 4. Owner questions

1. **Is a procedural blend's stored bound same-parameter?** The composition needs
   `|B(w) - S(w)| <= eps` at every parameter of the face, the extended pieces past the
   section included (`section_growth`). The code says "the net's certified distance from the
   definition" (procedural_blend.rs:271), and recertify builds it from same-parameter terms
   (READ). The probe agrees on these 4 trims (MEASURED). But no proof in the code states it
   for every blend. If it is only a Hausdorff distance, the composed bound is not valid.
2. **Should the edge bound take `of_exact` in solidcore itself?** It is the one-line cause of
   20 of the 24 refusals. It changes no accepted result, because B already uses it. That makes
   it a candidate fix upstream of solidcorex, filed or not as the owner decides.
3. **Tight or within?** T needs only "B <= L". The within stop is 9 to 12 times cheaper on the
   offset trims here (MEASURED in the kernel probe, section 2; the prototype's 2 to 16% of
   section 1 is a different setting, tolerance at 1.1 times the gap). But it records a
   bound up to L, not the tight figure. Should the record carry the tight value (the honest accuracy, 54's point 4) or only the pass?
4. **Trim-local eps.** Whether to build the 1D eps bound along the trim, which needs the
   section solve in interval arithmetic. On this corpus the stored eps is already 300 times
   below L, so nothing here needs it (MEASURED); it would matter only for coarse blends.
