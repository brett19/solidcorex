# Review: report 55, the certified trim gap

Adversarial read-only review of `docs/research/55-certified-trim-gap.md` and its prototype
`prototypes/trim-gap-bound/`. The rule in question: a fallback trim that records the accuracy it
reached must record a certified bound (the 11-26 rows of `docs/04-decisions.md`, report 54).

I read the report, the prototype (all of `src/`, `tests/bounds.rs`, `src/bin/gap_bench.rs`), the
pinned solidcore's blend, recertify, extension, removal and gap code, and the private run data
(probe TSV and log, both prototype bench TSVs, the probe patch). I built nothing and ran nothing.
The solidcore paths below are relative to the pinned crate's `src/`. Evidence tags:
READ (code or data I read), MEASURED (a number from the data files), JUDGED (my reasoning).

## Verdict: sound in direction, two blocking gaps

The offset method and the measured kernel claims hold up. Two things block the recommendation as
written:

1. **The recommended centred form is unsound across a C^1 net's patch lines.** The blend rows
   use it on exactly such nets (finding P1). The plain second-order form is sound there.
2. **The blend's stored bound can go stale.** Fillet removal grows a blend face's net past its
   certified domain and keeps the old bound (finding B2). Until that is fixed, "B on the net
   plus the stored eps" is not a certified bound for every blend face.

## 1. The blend's stored bound (task 1, owner question 1)

**Answer: same-parameter, yes. It covers the t-extension. It is valid only over the domain it
was certified on, and one kernel edit leaves it stale.**

### B1. Same-parameter: yes (no change needed to the claim, but state the proof)

- `def.bound` (`surface/procedural_blend.rs:270-271`) is `|B - H| + |H - S|` plus a floor
  (`blend/fillet_section/recertify.rs:617-918`) (READ).
  - `|B - H|` comes from the control nets of matched Bezier pieces over the same (s, t) box, so
    it holds at every common parameter.
  - `|H - S|` is the jet-row certificate `gn_certify_span`
    (`blend/fillet_section/certificate.rs:4217`): per-row control differences plus the Hermite
    remainder, at the same s. Both terms are pointwise at one parameter (READ).
- The net's (p, q) map to (s, t) affinely (`a + b(p, q)`), so a bound at one (s, t) is a bound at
  one (u, v). The kernel never uses the bound as a Hausdorff distance (READ).
- Past t in [0, 1], `recertify_in_frame` bounds the extended rows as the certificate times
  `section_growth` (recertify.rs:933) and takes the max of the inner and outer totals.
  Extension along s (`surface/extend/blend.rs`, lines 181 and 389) certifies new spans and
  writes a new bound. The #17015 end-row join (`blend/fillet_geom/p1.rs:3896-3935`) calls
  `rebound` and refuses past `dist_tol`. A transform scales the bound and adds rounding
  (procedural_blend.rs:1531-1610). A read from a record recertifies
  (`topology/brep_data.rs:1984-1995`). Trim and domain changes do not change the geometry
  (`surface/surface/p2.rs:3330-3345, 3795-3810, 3834-3843`). All of these keep it valid (READ).

**Severity: should change.** The report leaves this as an open question. It can be answered:
put the paragraph above in §1 and close owner question 1 as "yes, within the certified domain",
with B2 and B3 as the conditions.

### B2. The bound goes stale under fillet removal. **Blocking**

- `blend/fillet_removal.rs:1782-1799` (`grow_face`) calls `expand_boundary_span` on every
  B-spline-kind face. For a procedural blend, line 1829 picks `pb.net_mut()`, with the comment
  "its net's span, the definition and its map untouched" (READ).
- `expand_boundary_span` moves the net's boundary out by polynomial extension
  (`nurbs/b_spline_surface_n_lib/p1.rs:1736-1800`) (READ). Nothing recertifies or raises
  `def.bound`.
- From then on, `net_answers` (procedural_blend.rs:1246-1250) still lets the net answer any
  scope at or above the old bound. At the grown parameters the net is extrapolated, and the
  definition clamps uv to the net's (new) natural domain (procedural_blend.rs:1352-1410). So
  `|B - S| <= bound` there is unproven (JUDGED). That is already a kernel issue, not only one
  for report 55.
- Fix:
  - Route a blend in `grow_face` through `extend::blend`, which certifies new spans.
  - Or call `rebound` after the expansion and refuse past the scope tolerance.
  - Or mark the blend unproven, so the composition refuses.
  - Until then, the 55 composition must check that the trim's uv box lies inside the
    certified domain (B3).

### B3. The domain is part of the certificate. **Should change**

- The report's failure mode "the net does not cover the trim's region" (55:182-184) is right but
  is not enforced anywhere.
  - The probe patch's `blend` switch hands the raw net to the bisection with
    `EndContinuation::default()`, with no domain or `NetScope` check (READ, probe patch).
  - Its `net_to_surface_bound` returns `pb.def.bound` unconditionally (READ).
- Fix: in the production composition, store the region eps was certified on (per span, which
  recertify already computes) and refuse when the trim's uv box leaves it. Also refuse when the
  blend was edited since (a generation stamp on `net_mut`, or recertify there).

### B4. Evaluated definition versus exact definition. **Consider**

- The certificate bounds the net against the true jets (certificate.rs:1-120). The kernel's
  evaluated definition solves its stations by Newton and differs by the precision floor,
  about 1e-10 (recertify.rs:285). The floor is already inside `bound` (READ).
- The probe's sampled "gap on the definition" therefore carries up to that much error. That is
  harmless at L of about 1e-4, but the report should say which S the composed bound is about:
  the exact one (JUDGED).

## 2. The prototype's soundness (task 2)

What holds (READ):

- Every enclosure is interval-valued, including the end values `|g(a)|`, `|g(b)|`
  (`norm_at`, gap.rs, uses `Ci::point` jets). The constants are lifted exactly. `Ivl`
  multiplication returns the whole line on NaN, division by an interval holding 0 returns the
  whole line, and `sqrt` of a NaN end returns the whole line (`crates/scx-num/src/interval.rs`,
  lines 209-245 and 404-433).
- The NaN guard `finite_or_inf` covers each piece's upper value. The `f64::max` uses on
  `na.hi`/`nb.hi` cannot drop a NaN, because `norm_v` gets its `hi` from `Ivl::sqrt`, which
  never returns a NaN end.
- Bisection: the children `[a, m]` and `[m, b]` share `m` exactly, so there are no gaps. The
  first pieces are cut at the merged knots of both curves. The returned bound is the heap
  maximum, so a wrong `lower` changes only when the bisection stops, never the bound's
  validity.
- The plain second-order form, `max(|g(a)|,|g(b)|) + h^2/8 sup|g''|` with per-coordinate
  sups, is correct for `g` in W^{2,inf}, that is C^1 with a bounded, jumping g''. So it is
  sound across a C^1 net's lines.
- The pole: where `|S_u x S_v|` holds 0, the division returns the whole line, the piece's
  value is infinite, and the piece is bisected.
- The third-derivative switch is set only around the one evaluation whose `ddd` is read.
  Every jet is rebuilt inside `g_ci`, so no stale `ddd` leaks.
- `eps.rs` keeps each box inside one net patch (`net_gap_bound` clips to the patch), so its
  centred tensor form is applied only where `B - S` is smooth. The bilinear-plus-two-terms
  error formula is correct.

### P1. The centred form misses g'' jumps inside a piece. **Blocking** (for the recommendation)

- `Method::Centred` (gap.rs, `enclose`) takes `sup|g''| <= |g''(m)| + h/2 sup|g'''|`, or the
  plain enclosure where that is smaller. That inequality needs g'' continuous on [a, b].
  - Pieces are cut only at the trim's and edge's knots (`cuts`), not at the surface's patch
    lines.
  - `frame_ci` hulls the patches' jets. The hull of `ddd` is a bound on each side, and `gm`
    is the one side's value at the middle point. A jump J in g'' between `m` and a patch line
    is not in the sum (READ).
- The report's assumption (55:119-120) says the jump "stays bounded, and the enclosure over the
  piece takes both sides". That is true of the plain enclosure but not of `|g''(m)|`.
- The blend rows use exactly this case: `gap_bench.rs:386` and `tests/bounds.rs:313` call
  `Method::Centred` on a C^1 `HermiteNet`. The kernel's blend net is C^1 too: a cubic Hermite
  interpolant across stations (recertify.rs:11-15) (READ).
- **Counter-example (JUDGED, constructed by hand; not run).** Take one scalar coordinate of g on
  the piece [0, 2] (m = 1), with a patch line at p = 1.1, and set
  `g(t) = l t - (t - p)_+^2` with `l = (2 - p)^2 / 2 = 0.405`.
  - g is C^1. Its g'' is 0 left of p and -2 right of it. g''' is 0 on both sides, so its hull
    is {0}.
  - g(0) = g(2) = 0.
  - The centred value is `|g''(1)| + 1 * 0 = 0`, smaller than the plain hull's 2, so it is
    taken. The piece's bound is `0 + 4/8 * 0 = 0`.
  - The true maximum is at t = p + l/2 = 1.3025, where g = l p + l^2/4 = 0.4865.
  - The middle value g(1) = 0.405 is proved attained, so the Tight stop also fires at once.
    The run returns a bound below its own proved lower value.
- Realising this case: a two-patch `HermiteNet` whose z data are 0 on the left patch and
  `-(u - p)^2` on the right (cubic Hermite reproduces the quadratic), a straight trim along u,
  and the straight edge `l t`. The interval overestimation of the curve's jets adds an O(h^2)
  term, so in the prototype the shortfall appears on the small pieces that straddle a line
  with their middle on the flat side, and the bisection then hides it within the stop's 0.1%.
  The measured blend rows stay above their samples (MEASURED), but that is not a proof.
- Fix (any one):
  - When `surf.patches` returns more than one patch for a piece, use the plain `sup|g''|`.
  - Or cut the first pieces where the trim crosses a patch line where the surface is not C^2.
    This is what csg.rs already does for C^0 lines (`query/curve_surface_gap.rs:915-950`).
  - Then add the two-patch case above as a test.
  - The report's verdict should name the condition: centred form only where the surface is C^2
    on the piece.

### P2. An offset of a C^1 net breaks the plain form too. **Should change** (latent)

- With `d != 0`, `Q = S + d N`, and N uses `S_u x S_v`, which is only C^0 across a C^1 line. So
  g' jumps there, and the h^2/8 form is unsound across it (JUDGED).
- The prototype accepts `Problem { surf: HermiteNet, d != 0 }` without a guard. The gap.rs
  module doc says "across a C^1 net's patch lines the second derivative jumps but stays
  bounded", which is false for an offset.
- No current case exercises it (offsets are only on the single-patch Bezier and torus). The
  kernel handles it: an offset is "one order less" (csg.rs:229-230, 496-514).
- Fix: refuse or cut at net lines when `d != 0`, and correct the doc.

### P3. The cap behaviour does not match the doc. **Consider**

- The gap.rs module doc says an unresolved piece "leaves the bound infinite (refused)".
  `certify` actually returns the finite heap maximum with `resolved = false`. That value is
  still a valid bound, but report §3.5 makes a cap "unproven, never a large number taken as
  proved".
- Fix: make the prototype return infinity on a cap, or reword the doc to match, so the
  prototype matches the proposal.

### P4. Trims that touch the domain boundary off an iso line refuse. **Consider**

- `frame_ci` returns `None` when the uv enclosure leaves the domain at all. The interval Hermite
  evaluation overshoots by O(h^2) near an end that lies on the boundary, at every depth, so
  such a trim never resolves (JUDGED from the code).
- Iso trims evaluate exactly, and every case starts inside the domain, so the bench never meets
  this. Real face trims often meet the boundary.
- csg.rs clips and handles boxes past the net instead.
- Fix: clip the uv box to the domain before evaluating (the patch is polynomial past its
  edge), or note the limitation beside the cost figures.

## 3. "Gap" terminology (task 3)

- The definitions (55:22-31) are clear. Same-parameter implies Hausdorff (55:26, correct with
  psi the identity). G is a one-sided distance from C to S. eps is same-parameter (READ).
- One gap: the composition needs a same-parameter eps for B, while G needs only a one-sided
  net-to-definition distance. The stored bound serves both, because it is pointwise. Say this
  once in §1. **Consider.**
- On what consumers need:
  - T's acceptance (B <= L) and a B-rep's tolerance checks need the Hausdorff form at minimum.
  - Anything that evaluates the pcurve and the 3D curve at one t (tessellation stitching,
    seeding intersections) needs same-parameter. On vase2 the two differ by 1.05 to 1.16
    (MEASURED), so the difference is real.
  - Recommendation: record the same-parameter bound when it passes and fall back to the
    Hausdorff one, and name which form the record holds. **Should change** (54's point 4
    asks for the honest figure; the honest figure needs its form).

## 4. The measured claims on the 24 cases (task 4)

Checked against the probe TSV (48 rows) and the two bench TSVs (MEASURED). The claims follow:

| claim | data |
|---|---|
| G with `of_exact` at 1.0004 to 1.0010 times sampled | yes, 1.0004 to 1.0010 |
| T's B at 1.0003 to 1.0005 times sampled Hausdorff | yes |
| main's trims over L on vase2 and vase3 | vase2 1.015 to 1.033 L, vase3 1.048 L |
| vase2 same-parameter / Hausdorff 1.05 to 1.16 | yes |
| blend eps 0.3% to 1.6% of L | yes; composed / sampled 1.28 to 1.58 |
| within-L 9 to 12 times cheaper | yes, on the offset trims |
| every bound at least its sample | yes, all rows |

Three caveats on the data:

- **D1. B/L of 0.91 measures the setup, not the method. Should change.** The edges' tolerances
  were set at 1.1 times the sampled G (55:241), and T's trims have B about equal to G, so B/L
  of about 0.909 to 0.910 is 1/1.1 by construction. Reword: what it shows is that B tracks G
  within 5e-4. It does not show headroom under L.
- **D2. The blend rows do not test a different trim. Consider.** For the 4 blend trims T's trim
  is main's trim (ratio t/fit = 1). They test the composition, not the fit.
- **D3. The probe bypasses the domain and scope check (B3). Consider.** State it beside the
  blend rows.
- Also, §1's "within is barely cheaper (2 to 16%)" (prototype, at 1.1 times) and owner
  question 3's "9 to 12 times cheaper" (kernel, offsets) read as contradicting each other. Say
  in both places which setting each comes from. **Consider.**

## 5. The open questions (task 5)

**Q3, tight or within: record the tight value; use within only as an early exit.**

- On the blend trims the within-L bound lands near L (7.3e-5 to 7.8e-5), while the tight one is
  2.7e-7 to 1.3e-6 (MEASURED). A pass-only record would overstate the blend's error 60 to 300
  times. That error then stacks into every later tolerance (54's point 4).
- Scheme:
  - Run the within stop first, as the accept/refuse decision (cheap, and it covers the
    refusals).
  - Continue to tight when the extra pieces are within a fixed budget (say 2 times the within
    cost). Record whichever was reached, with a flag for "tight" or "pass at L".
  - A consumer that needs the real figure can tighten later from the same record.

**Q4, trim-local eps: do not build it now.**

- The stored eps is 300 times below L on this corpus (MEASURED). The interval section solve it
  needs is the costly part.
- The cheap step that gets most of the benefit: store eps per span with its (s, t) region.
  recertify already computes per-span totals. Take the max over the spans the trim's uv box
  meets.
  - This is also what B3 needs to check the domain, so it does double duty.
- Revisit trim-local eps only if a coarse blend's eps reaches about L/10.

**Q1: yes (see B1), conditional on B2 and B3.**

**Q2 (of_exact for G upstream):** I agree it is a candidate fix. The data show it changes no
accepted B. Not reviewed further.

## Summary of findings

| id | severity | one line |
|---|---|---|
| P1 | blocking | centred form unsound across C^1 patch lines; blend rows use it there |
| B2 | blocking | fillet removal grows a blend's net and keeps the stale bound |
| B1 | should change | answer owner question 1: same-parameter, with the proof |
| B3 | should change | enforce the certified domain and edits in the composition |
| P2 | should change | offset of a C^1 net breaks even the plain form; guard it |
| D1 | should change | B/L 0.91 is 1/1.1 by construction |
| §3 | should change | record which gap form the bound is |
| B4, P3, P4, D2, D3 | consider | floor wording, cap doc, boundary trims, data notes |
