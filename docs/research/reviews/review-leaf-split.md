# Review: `cache-leaf-split` (binormal split into ensure + read)

Branch `cache-leaf-split` at `6531c0bcda`, 4 commits on `dd955acde5`. I reviewed it by reading only: I ran no build or
goldens. Paths are relative to `solidcore/crates/solidcore/src` unless they say otherwise. Line numbers are
the branch's.

## Verdict: PASS WITH FIXES

The split does what it says at every one of the 80 call sites. Each site ensures the same edge use with the same
flag. Nothing runs between the ensure, the read and the cache keep. Loops keep the old order. No site reads a copy.
No `BinormalRead` is dropped without its cache step.

I found no BLOCKER. The three refusals the author lists cannot be reached from a valid body.

One divergence is missing from the report and is reachable on valid input (S1 below). The build can shorten the
edge's interval. The old read had already taken the parameter, the interval and the 3D point before that edit,
and the new read takes them after it. The new value is the one main's own second call returns, so it is defensible.
But it has to be stated and measured. The other gaps are in evidence and tests, not in the code.

## 1. Equivalence at every call site

The 80 reads are 79 `.keep_surface_cache` and 1 `.read_only()`, which is a test (`topology/edgeuse/p3.rs`
tests, `lone()`). Of the 79:

- **62 call the ensure first, not 63** (report §1 says 63).
- **17 pass a literal `use_3d_only = true` and skip the ensure.** Those sites are `blend/blend_corner/end.rs:504`,
  `blend/fillet_corner/topology/nx1_end.rs:139`, `blend/fillet_tangent_rollover.rs:890`,
  `blend/weld_bead.rs:468`, `heal/stitch/p2.rs:285`, `intersect/curve_class/p4.rs:536`,
  `sweep/section_blend.rs:1044`, `topology/brep/validation/assert_valid/edge.rs:396,479,480,898,899`,
  `.../tangent_sections.rs:655` plus 2 in its tests, `topology/edgeuse/p1.rs:1387` and
  `topology/edgeuse/p3.rs:1422` (`pick_seam_curve`).

For the 17, skipping the ensure is exact:

- The old lazy branch needs `!use_3d_only`.
- `use_3d_only` is only ever turned off when a trim exists (`p3.rs:3210`).
- The old throwaway build (`:3343`) returns first when `use_3d_only` is set.

Patterns, each checked against the old code:

| Pattern | Sites | Edge use / flag / order |
|---|---|---|
| A: `ensure(eu,false).and_then(read(eu,false).keep)` with a parameter bound before the call | all literal-`false` sites: band_room, join_gap, blend_refusal ×2, chamfer_side, fillet_corner/p1 ×2, fillet_corner/topology ×3, closed_turn, insertion/geometry, two_offsets ×2, fillet_geom/p2 ×3, p6 ×2, fillet_solver p1/p4/rails_fit ×2, standard_solver p1 ×2 / p2, hcr, arena_pair, crossing_loops, offset edgeuse, reach ×2, api_queries, edge_turn, tangent_edge, cutting, brep/p6, calc_sector_angle ×2, goldens and tests | Same `eu` in the ensure and the read every time. No site passes a mate, radial or next. Each parameter is a local evaluated before the ensure (`mid`, `t`, `param`, `cp.parameter`, `seq_param`, `dropped_parameter`), as the old argument was evaluated before the call. No parameter expression inside a closure reads the body. |
| B: the flag is a variable passed to both halves | `heal/stitch/p1.rs:2807`, `edgeuse/p2.rs:190`, `:424` (`!ok_to_make_uv_trim_curve`), `face/p4.rs:1388` (`!uv_space_okay`), `brep/p1.rs:1691`, `query/face/p3.rs:2613`, `sweep/topology_sweep/access.rs:642`, golden `topology_edgeuse_p3.rs:327` | The same binding goes to both halves. |
| C: status-ignoring wrappers (`ignored`, `match`, `binormal_status_ignored`, `check_nx1_degeneracy_status`, `binormal_ignoring_status`, standard_solver) | fillet_corner/p1 ×2, fillet_solver/p1, standard_solver p1 ×2 / p2, edgeuse/p2:190 | The ensure's error is exactly the old `fetch.status?` / `get_or_create(..)?` error (`p3.rs:2962` against the deleted `:3088` lines). It arrives through the same `Result`, so it is filtered the same way. |
| D: in loops | `blend_refusal.rs:408` (STATIONS), `fillet_corner/topology.rs:542` (uses), `assert_valid/edge.rs:472` (radial walk, 3D only) | The order is ensure(eu), read, keep, then ensure(other). The old order was build(eu), note(eu), build(other), note(other). It is the same per iteration, with nothing ensured up front. |

Accessors re-borrowed inside the closure have no side effects:

- `InsertionGeometry::body_mut` (`blend/fillet_executive/session/insertion/geometry.rs:698`) is a plain match.
- `ArenaPair::brep_m`, which clears face caches, is called once, as before (`boolean/merge/p4/arena_pair.rs:1090`).

`intersect/curve_class/p4.rs:533` still reads on `clone_sharing_surface_caches()` and keeps the cache on that
scratch copy, as before.

So the site-level claim holds. The finding is about what the ensure does to the body before the read takes its
snapshot.

### S1 (SHOULD-FIX). The build can shorten the edge interval, which the read now sees

The old lazy build sat at `:3088`. By then the read had already done four things (branch lines):

1. clamped `parameter` to the edge interval (`:3091-3097`);
2. captured `edge_ivl` (`:3100`) and `curve_3d` (`:3101`);
3. evaluated `curve_pv`, the 3D point and tangent (`:3110-3177`);
4. taken the surface snapshot (`:3188`).

`create_uv_trim_curve` can edit the edge's interval. It calls `edge::set_interval` on its 1% edge-trim recovery
(`p3.rs:1036` and `:1060`, `OnePercentEdgeTrimRecovery1/2`) and trims the other uses (`trim_other_uv_curves`).
That happens when the dropped UV curve is shorter than the edge and its end lies within the intersection tolerance
of the vertex. That is ordinary valid input, not a degenerate case.

What changes:

- **On main**, the first lazy call evaluated the 3D point and tangent at the old end. It then clamped to the trim
  interval, used the old `edge_ivl` for `crv_step` and the step-offs (`:3391`, `:3440-3462`), and returned
  `point = curve_pv[0]` from before the edit.
- **On the branch**, the ensure edits the interval first. The read then clamps to the new interval and evaluates
  the curve there.
- **Where they differ:** for a parameter inside the trimmed sliver, the returned point and tangent differ by up to
  the vertex intersection tolerance. That is far above the goldens' `1e-9` relative tolerance. Callers that ask at
  `ivl.min()` / `ivl.max()` are exactly in that sliver: `fillet_corner/topology.rs:445`, `closed_turn.rs:128` and
  `join_gap.rs:at_vertex`.

The author's trim counts and (edge use, flags) sequences cannot see this, because the same builds happen in the
same order. The goldens see it only if the recovery fires on a binormal-made build at an end parameter.

The new value equals what main's own second call returns once the trim exists. So it removes a dependency on
whether this was the first call, and I would accept it as the better result.

Fix:

- State it as divergence (d) in the report and the commit message.
- Measure it: in the scratch counter, log `set_interval` calls made inside ensure-made builds over the 49 units and
  the ops binaries.
- If the count is not zero, list the cases. If the goldens did not move there, add a targeted test (§5).

## 2. The three listed divergences: reachable with valid input?

- **(a) An error before the old lazy point.** The ensure (`p3.rs:2919-2963`) copies the read's lookups but not its
  evaluations. Those are curve evaluation (`:3110/3130/3144/3172`), `NoStepInsideInterval` (`:3165`) and the
  surface's `clone_for_evaluation` depth error (`:3197`). Where one of them fails, the branch builds a trim, then
  returns the same error, or the build's error if the build failed.
  - The first three need the curve to fail to evaluate inside its own interval, or to have zero first and second
    derivatives over the ±10·√ε step window. The last needs a surface nested past its depth bound. A valid body
    has none of these.
  - It is not a refusal of valid input either: that input was already refused.
  - The only visible effect is an extra trim, plus its tolerance raise and possible interval edit, at callers that
    swallow the error (pattern C).
  - **NIT:** say this in the ensure's doc.
- **(b) The cache note makes this edge use's trim and the foot then fails.** **(c) The old throwaway build.** Both
  need all of the following:
  - `!use_3d_only`;
  - no trim on the edge use or its mate;
  - a drop-curve record whose `fail_status` is `SM_ERR_UNKNOWN`, the record's un-initialised value
    (`surface/surface/p2.rs:6730,6773`), because otherwise the ensure has built the trim;
  - no foot from the global point solve at 1000 × the edge tolerance (`:3325-3341`).

  Is that combination reachable?
  - **The record.** Records are attached only together with a trim (`topology/edgeuse/p1.rs:806`,
    `topology/brep/p9.rs:1726,1762`). `get_or_create_uv_trim_curve` attaches none when the edge use had none
    (`p3.rs:1779`). `create_uv_trim_curve` sets `SM_SUCCESS` on success (`:639`). A trim-less use with an
    un-initialised record therefore needs a trim removed with its record left behind, as `brep/p3.rs:4530` (squeeze)
    does. Even then the record attached with that trim must still be un-initialised, which the paths above never
    produce.
  - **No foot.** An edge more than 1000 × its tolerance from its own face is not a valid body.

  So neither (b) nor (c) is reachable from a public entry with valid input. No BLOCKER. This matches the
  measurements: R1 = R3 = 0 and main's B3 = 0.
- **SHOULD-FIX (API footgun).** The refusal is easy to reach from code: any new `&mut` caller that forgets the
  ensure gets `EdgeuseHasNoTrimCurve` on a valid body that lacks trims. Most bodies from primitives, Booleans and
  blends lack them (report 42: 1,302 of 4,101 bodies).
  - Add one `pub fn` in `p3.rs` that runs ensure, read and keep for a caller holding `&mut Brep`, and use it at the
    62 sites. Today the triple is written out 62 times, besides 8 local wrappers that already do this (hcr,
    fillet_solver/p4, stitch/p1, brep/p1, query/face/p3, fillet_corner/p1, standard_solver p1/p2).
  - The order (ensure, then read, then keep) then lives in one place.
  - It also removes the near-duplicate long closures, which go past rustfmt's width in the golden
    `blend_fillet_executive_p3/two_offset_chamfer.rs:18,251,254`. That file was already unformatted on main (it is
    reached through `#[path]`), so this is a NIT.

## 3. The deferred surface-cache step

What the build writes: `face_surface_cache` → `note_face_surface_cache`. That fills or refreshes the slot, and the
trimmed-cache build runs `create_uv_trim_curves` for every edge use of the face, with its gap fix. That can move
trim ends, raise tolerances and, through `create_uv_trim_curve`, edit edge intervals (S1).

Can the deferral change the read's value? After the old note point (`:3301`), the read touches the body once: the
trim re-read in the no-foot branch (`:3350`), which is case (b). Everything else uses locals captured earlier:

- `edge_ivl`, `curve_3d` and `edge_tol` (`:3228`, before the note);
- `surface`, the evaluation copy, which shares the slot;
- `solve_surface = surface.clone()`, which has a fresh `CacheSlot::default()` (`surface/surface/p2.rs:621`).

So the global point solve cannot see the slot that was just filled. Its point test is off and its owner is
cleared, and with `point_test_off` the fast-drop trim test is skipped (`surface/surface/p4.rs:1740`). The
deferral is exact apart from (b). I confirm the author's claim.

Can it change what the caller sees next, or be skipped?

- `keep_surface_cache` runs on both `Ok` and `Err`, with the cache error first, as in the old order
  (`p3.rs:2988-2993`).
- `BinormalRead` is `#[must_use]`.
- No library caller drops it, and there is no `let _ =` of a read anywhere.
- `read_only()` is used once, in a test, on a body with no face, where the old code reached no note.

So no path loses a build main did, and nothing is moved to a later caller.

## 4. API honesty

- **Writes through `&Brep`.** `evaluate_binormal(&Brep)` is not write-free. The surface copy it takes
  (`clone_for_evaluation`, `surface/surface/p2.rs:982-994`) shares the live surface's `cache_slot`, `made_kind`,
  `continuity_memo` and plane / extrusion NURB cells. Evaluations on it can fill those memos. Those fills existed
  on main too and do not affect the value.
  - **NIT:** the report calls the read "the pure half", and the module doc says "reads the B-rep shared". Say
    instead that the read may fill value-neutral memos on the face surface and makes no topology or trim edit.
  - `work_counters` (atomics) are touched only in `create_uv_trim_curve`, so not by the read.
- **The error.** `SurfaceFailure::EdgeuseHasNoTrimCurve` exists on main (`error/surface.rs:230`, used in
  `surface/trim_srf_cache.rs:1041,1372`). The new `uv_trim_missing` constructor only adds `.at(Edgeuse)`. Its
  class is `InvalidInput`.
- **Callers outside the kernel.** None of SolidCraft's app crates, `solidcore-ops` sources, `-tess` or `-convert`
  call the binormal (grep). Only the ops test `issue_2953_drafted_peg.rs` does. `xtask/solidcore-paths.txt` lists
  no edgeuse path.

## 5. Tests

**SHOULD-FIX: no test pins the new contract.** The diff only adapts existing calls (grep shows no new `#[test]`).
Add to the tests in `topology/edgeuse/p3.rs`, using the square-face fixture as in
`intersect/topology_solver/p1.rs:3010`:

- With the trims removed, `evaluate_binormal(&b, eu, t, false)` returns `EdgeuseHasNoTrimCurve` at `eu`, and the
  body is unchanged (trim count, edge interval and tolerance equal to before).
- Then `ensure` followed by the read returns the closed-form binormal of the plane: in-plane, square to the edge,
  pointing into the face.
- With `use_3d_only = true` and no trims, the read succeeds and names the face. The body is still unchanged.
  `keep_surface_cache` then makes the face's trims.
- For S1, if the measurement finds the edge-trim recovery: a case where the ensure's build shortens the interval,
  asserting that the read evaluates on the new interval.

**SHOULD-FIX: the golden set misses units whose own code calls the binormal.** These were not run:

- `topology.edgeuse_p2`: `evaluate_binormal_step_off_into` (golden `topology_edgeuse_p2.rs:465`) reaches the
  ensure site `edgeuse/p2.rs:424`.
- `topology.face_p4`: `find_vertex_sector` (golden `topology_face_p4.rs:705`) reaches the site `face/p4.rs:1388`.
- `query.face_p3`, `intersect.curve_class_p4`, `topology.assert_valid`, `topology.brep_cutting`,
  `query.api_queries`, `sweep.topology_sweep` and `topology.edgeuse_p1`: their golden runners call the functions
  that hold sites.

Run `--all --jobs 4` forwards and `--reverse`, or at least these units, before claiming "no result moved". The 49
units the author ran do reach the binormal (blend, boolean, offset, heal), and the trim-count evidence for them is
strong.

## 6. Performance

- **Per site:** one more call and about 9 id or field lookups (`p3.rs:2928-2955`), which the read then repeats. No
  evaluation is added.
- **Builds:** the ensure builds exactly where main's lazy branch did, except in case (a), which is rare and needs
  an invalid curve. The author's identical C/G counts back this.
- **Not added work:** the cache note is moved, not repeated.

## 7. Rules and text

- No `unsafe`, `panic`, `expect` or indexing added in library code.
- The added `.unwrap()` calls are all in tests (`tangent_sections.rs` `mod tests`, `edgeuse/p2.rs` `mod tests`,
  `tests/`, the ops test).
- `.unwrap_or(false)` at `p3.rs:2956` is a total function. It swallows lookup errors on purpose (documented), and
  the read reports them.
- No comment or commit message mentions threads, threading, parallelism or concurrency. Nothing committed has a
  local path or a task name.
- Names are in the kernel's vocabulary (`ensure_binormal_uv_trim_curve`, `BinormalRead`, `keep_surface_cache`).
- The upstream tag `[B438]` is kept.
- **NIT:** the commit message of `4aee8129ce` says "every caller that holds the B-rep mutably calls it
  immediately before the read". Add "(except with `use_3d_only` set, where it is a no-op)".
- **NIT:** the report says 63 ensures; there are 62.

## Summary of findings

| # | Class | Finding | Suggestion |
|---|---|---|---|
| S1 | SHOULD-FIX | The ensure's build can shorten the edge interval (`p3.rs:1036,1060`) before the read clamps and evaluates (`:3091-3177`). The read now evaluates on the new interval, and main evaluated on the old one on its first call. This is reachable on valid input, and the counts the author measured cannot see it. | State it as divergence (d), measure `set_interval` calls inside ensure builds, and add a test if they occur. Accept the result as main's second-call value. |
| S2 | SHOULD-FIX | No test pins the new refusal or that the body is left unchanged, nor ensure-then-read against a closed form. | Add the tests in §5. |
| S3 | SHOULD-FIX | Golden units that reach sites were not run: `topology.edgeuse_p2`, `topology.face_p4`, `query.face_p3`, `intersect.curve_class_p4`, `topology.assert_valid` and others. | Run `--all` forwards and `--reverse`. |
| S4 | SHOULD-FIX | The ensure, read and keep triple is inlined 62 times. A future `&mut` caller that forgets the ensure turns a valid trim-less body into a refusal. | Add one `&mut` helper in `p3.rs` and use it at the sites. |
| N1 | NIT | (a) builds an extra trim before an error the read already returned. Unreachable on valid curves. | Document it in the ensure's doc. |
| N2 | NIT | "Pure" or "shared" read: the read still fills value-neutral memos on the face surface through the shared evaluation copy. | Reword the docs and the report. |
| N3 | NIT | Count 62, not 63. The commit message omits the 3D-only exception. The golden `two_offset_chamfer.rs` has 100+-column lines (that file was already unformatted on main). | Fix the text, and run rustfmt on that file if it is touched again. |

(b) and (c) are not reachable with valid input (§2), so they are no BLOCKER.
