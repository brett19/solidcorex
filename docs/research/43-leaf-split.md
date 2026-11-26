# 43: splitting a lazily trim-making leaf (the binormal)

Branch `cache-leaf-split` (local only, from main `dd955acde5`), five commits:
`4aee8129ce` (the split), `1e1f9429fe` (unused imports), `c81dbcf933` (shared borrows at calls clippy flagged;
options by value), `6531c0bcda` (named bindings, fmt of three golden units) and, after the independent review
(`reviews/review-leaf-split.md`), `a2bbe62eed` (two-stage read in upstream's order, one mutable helper at every
site, contract tests). Server2 side refs `push/cache-leaf-split-1` .. `-5` (tip `-5`). Scratch, never on the
branch: `push/cache-leaf-split-m2` (instrumented main), `-mb2` (instrumented `4aee8129ce`) and `-mc1`
(instrumented `a2bbe62eed`, interval edits).

## Result in one line

**No result moved.** MEASURED at `a2bbe62eed`: every golden unit (283) forwards and `--reverse`, all
`PASS changed=0 added=0 removed=0 broken=0`. Kernel lib 6,404 passed, 0 failed. Ops tests the same pass/fail as
main. One divergence the first four commits had was found by the review and measured: 1 read in the whole
goldens run, 2.0e-6 in the edge point. `a2bbe62eed` removes it by restoring upstream's order (§0).

## 0. Review round: `a2bbe62eed`

- **S1, a fourth divergence (MEASURED, then fixed).** Making the UV trim curve can shorten the edge's
  interval (`CreateUVTrimCurve`'s one-percent edge trim recovery, `edge::set_interval` at p3.rs ~1042/1066).
  Main's read had already snapped the parameter, taken the interval and curve and evaluated the edge point and
  tangent before that creation; commits 1-4 did all of that after it. A scratch log at every binormal-made
  creation (before/after interval of the edge, and the commits-1-4 value computed beside the main-order one):
  - goldens `--all`: 144,801 creations, 3 interval edits, 1 with the parameter in the trimmed-off part:
    `blend/fillet_corner/p1.rs:1293` (the status-ignoring wrapper), interval max 0.750000022610394 -> 0.75, read
    at the old end. Commits 1-4 gave a point 2.0e-6, a tangent 6.7e-6 and a binormal 3.5e-8 away from main.
    The other 2 edits (edgeuse/p2.rs:188) read inside the kept part: 0 difference. (The log has no unit name;
    the case was in none of the 49 units run before, or not in a recorded value.)
  - ops: solidcore_ops lib 1,633 creations, boolean 2,096, ops 33,866: 0 interval edits.
  - The fix: the read is two stages. `binormal_curve` (snap, interval, curve, edge point and tangent, face
    surface, whether a trim is needed: no trim needed) and `binormal_uv` (the UV part, with stage 1's values).
    `evaluate_binormal_mut` / `evaluate_binormal_uv_mut(k, &mut Brep, ...) -> Result<T>` run stage 1, the
    creation where needed (`get_or_create_uv_trim_curve(.., false, false, true)?.status?`, as main), stage 2,
    then `keep_surface_cache`: main's order exactly. `evaluate_binormal(&Brep)` runs both stages on the body as
    it stands and refuses a missing trim.
  - Test `a_uv_trim_that_shortens_the_edge_leaves_the_binormal_read_before_it` constructs the case: a
    `1 x 2 x 3` box edge with its trims removed and its interval stretched 2e-5 past a vertex (start, then end).
    The creation shortens it back (MEASURED: 1e-5 at the end and 2e-5 at either end trigger it; 1e-7..5e-6
    do not). The mutable read gives the old end's point; the shared read afterwards gives the new end's,
    2e-5 apart, same binormal.
- **S4, one helper (READ).** All 79 mutable reads now call `evaluate_binormal_mut`/`_uv_mut` (62 that
  created before, 17 with literal `use_3d_only = true`, which need no trim and get none; the count 63 in
  earlier drafts was wrong, it is 62). `ensure_binormal_uv_trim_curve` is deleted. The 1 shared read is the test
  `no_edge_is_an_error`. Re-exported from `topology::edgeuse` beside `evaluate_binormal`.
- **Divergence (a) is gone.** Stage 1 errors (curve evaluation, `NoStepInsideInterval`, the surface copy's depth)
  now return before any creation, as on main. Commits 1-4 created a trim first there (an input already refused;
  visible only as an extra trim at callers that swallow the error). Measured 0 times either way.
- **(b), (c) unreachable from a valid body (READ, review §2).** Both need `use_3d_only` false, no trim on the
  use or its mate, a drop-curve record still `SM_ERR_UNKNOWN` (records are attached only with a trim and set to
  `SM_SUCCESS` on creation; only a trim removed with its record left, as squeeze does, leaves one), and an edge
  point more than 1000 x the edge tolerance from its face. MEASURED R1 = R3 = 0, main B3 = 0.
- **S2, contract tests** (topology/edgeuse/p3.rs tests, box fixture): the shared form on a trimless box refuses
  with `EdgeuseHasNoTrimCurve` at the edge use, names no cache face, and leaves `Brep::write_stamps` equal;
  the mutable form creates the trim and gives the closed form (edge point, axis tangent, axis face normal,
  binormal = normal x tangent pointing into the box), equal to the shared form on the box that kept its trims
  (whose stamps stay equal); with `use_3d_only` the shared form succeeds without trims, names the face and
  writes nothing, and the mutable form gives the same value and its cache step makes the trim.
- **S3, every golden unit (MEASURED).** `goldens --all --jobs 4` forwards (wall 818 s) and `--reverse` (614 s):
  283 of 283 pass, every line `changed=0 added=0 removed=0 broken=0`, including the units the review named
  (topology.edgeuse_p1/p2, topology.face_p4, query.face_p3, intersect.curve_class_p4, topology.assert_valid,
  topology.brep_cutting, query.api_queries, sweep.topology_sweep).
- **Other checks at `a2bbe62eed` (MEASURED).** core and root `fmt --check`, `clippy --workspace --all-targets
  -D warnings`, `cargo xtask fast --no-tests`: rc 0. Kernel lib (all tests) 6,404 / 0 / 5. Ops: ops 604/0/11,
  boolean 81/0/1, solidcore_ops lib 369/1/2 (the hlr ball-corner test that fails on main). The five kernel
  integration tests that call the binormal pass.
- **Nits.** "Pure" is reworded: the shared form makes no topology, trim or cache edit, but the surface's
  value-neutral evaluation memos may fill (module docs). The commit message says 3D-only calls need no trim.
- **Work.** Unchanged from main: the creation condition is now the read's own (`needs_uv_trim`), so the ensure
  step's repeated id lookups are gone too.

## 1. The split as first built, commits 1-4 (READ; §0 has what changed)

`topology/edgeuse/p3.rs`:

- **`ensure_binormal_uv_trim_curve(k, &mut Brep, eu, use_3d_only) -> Result<()>`.** This is the mutable
  half. It calls `get_or_create_uv_trim_curve(k, b, eu, false, false, true)` and returns its status, with
  the same flags the binormal used at its old :3088. It builds only under the binormal's own condition:
  `!use_3d_only`, a loop edge use, an edge with a curve, a face with a surface, a face use, no trim of its
  own or its mate's, and a drop-curve status other than `SM_ERR_UNKNOWN`. Otherwise it does nothing. A
  lookup error makes it do nothing, and the read then reports that error.
- **`evaluate_binormal` / `evaluate_binormal_uv(k, &Brep, ...) -> BinormalRead<T>`.** This is the shared half (no topology, trim or cache edit; value-neutral surface memos may fill).
  - Where the trim used to be built (:3088), it refuses with
    `SurfaceFailure::EdgeuseHasNoTrimCurve` at the edge use. This is the existing error, through a new
    `SolidError::uv_trim_missing(op, eu)`.
  - Where a throwaway trim used to be built (:3223, no foot on the surface and no trim), it refuses with
    the same error.
- **Surface cache (:3172).** It calls `face_surface_cache` → `note_face_surface_cache`, only when the edge
  point matches no trim point.
  - What it writes (READ): when the face surface's slot is empty, or stale (stored, face modified, editing
    off), it builds the trimmed-surface cache. That build calls `create_uv_trim_curves` for every edge use
    of the face, toggles editing and stores the cache in the slot. When the slot is checked out, it builds a
    cache and drops it. When the slot holds a fresh cache, it re-adopts the owner.
  - So it cannot stay in the shared half: it writes trims of other edge uses and toggles flags, and the slot
    being `Arc<Mutex>` does not cover those.
  - It also cannot move ahead of the read, because it depends on a value the read computes (`got_uv`).
  - So the read records the face (`BinormalRead::surface_cache_face`), on both Ok and Err.
    `BinormalRead::keep_surface_cache(k, &mut b)` gets the cache immediately after the read. A cache error
    takes priority over the read's own error, as in the old order.
  - Nothing after :3172 in the read uses the B-rep, except the :3219 trim re-read. The surface evaluations
    after it run on `surface.clone()` (a fresh slot) or on cache-independent geometry (READ). So moving
    the note after the read changes no value, unless the note creates *this* edge use's trim and the foot
    then fails. MEASURED: that never happened (R3 = 0 below).
  - `BinormalRead::read_only()` is for a shared-B-rep reader. It keeps no cache.
- **Call sites (commits 1-4; superseded by §0).** All 80 calls went through `.keep_surface_cache`. 62 called ensure first, in the shape
  `ensure_binormal_uv_trim_curve(k, b, eu, f).and_then(|()| evaluate_binormal(k, b, eu, t, f, &o).keep_surface_cache(k, b))`.
  - The 17 calls with a literal `use_3d_only = true` skip ensure, which is a no-op for them.
  - One test uses `read_only`.
  - Across solidcore, goldens and ops tests: 54 files, +685 / −297. No caller was `&Brep`-only before,
    because the function took `&mut`. No app crate or solidcore-tess/-convert calls the binormal.
  - The lazy form is deleted.

## 2. No result moved (MEASURED, server2, `--profile goldens`)

- **Goldens.** 49 units, forwards and `--reverse`: 98 of 98 `PASS changed=0 added=0 removed=0 broken=0`. The
  units: all 25 blend, all 8 boolean, corpus.parity, heal.stitch_p1/p2, mass.integrator, all 3 offset,
  primitive.api_primitives, primitive.primitive_creation_p1..p3, sweep.api_brep,
  sweep.rotational_sweep_geometry, topology.edgeuse_p3 and topology.proto_topology_p1.
- **Kernel lib tests, by filter.** All pass, 0 failed:

  | Filter | Passed |
  |---|---|
  | edgeuse | 37 |
  | binormal | 75 |
  | tangent_edge | 4 |
  | edge_turn | 4 |
  | chamfer_side | 7 |
  | carried_face | 64 |
  | weld_bead | 100 |
  | proto_topology | 35 |

- **Kernel integration tests that call the binormal.** All pass: issue_2307_peg_chamfers,
  issue_2175_chamfer_rail_past_tangent_rim_end, fork_ea59e9c_section_orders, fork_c3f8c80_ball_sphere_turned_round
  and issue_2360_mixed_corner. The test binaries are `blend` and `geometry`.
- **solidcore-ops test binaries.** Each one has the same pass/fail count on main and on the branch:

  | Binary | Passed | Failed | Ignored |
  |---|---|---|---|
  | ops | 604 | 0 | 11 |
  | boolean | 81 | 0 | 1 |
  | fixes | 1 | 0 | 0 |
  | issue_3012 | 2 | 0 | 0 |
  | issue_3045 | 1 | 0 | 0 |
  | round_between | 1 | 0 | 0 |
  | lib | 369 | 1 | 2 |

  The one lib failure is `hlr::tests::a_box_with_rounded_corners_is_drawn_with_its_ball_corners_as_spheres`.
  It fails on main too, with the same numbers (`lines 4803.159770066132, hexagon 4803.15977142607`), so it
  is not this change.

## 3. Work and order (MEASURED, scratch counter, never on the branch)

The scratch log writes one line per `create_uv_trim_curve` entry (`C eu dev`) and per
`get_or_create_uv_trim_curve` build (`G eu dist dev`). It also logs:

- the binormal's builds: B1 on main, inside ensure on the branch;
- its cache requests (BF), with the slot state;
- the branch's deferred note (BK), with the trims it built;
- the branch's refusals: R1 at the old :3088, R3 at the old :3223.

- **Golden units (jobs 1).** Trim builds C are identical per unit, and so are G. The full C/G sequence with
  (edge use, flags) is identical, line for line, in all 43 units that log anything. Totals over those
  units: C = 197,388. The binormal's own builds are B1 = 79,255: boolean.api_brep 35,175,
  blend.fillet_executive_p3 13,875, offset.offset_executive_p2 7,550, primitive.api_primitives 4,532,
  heal.stitch_p2 3,711, heal.stitch_p1 3,484, blend.feature_executive 3,069. Cache requests: 20,725, with
  4,087 on an empty slot.
- **Trims made by the cache note.** On main, 1,004 (all in topology.edgeuse_p3). On the branch's deferred
  note, the same 1,004, at the same place in the sequence.
- **Ops binaries (8 test threads).** The multiset of C and G lines is identical:

  | Binary | C | G |
  |---|---|---|
  | ops | 271,223 | 271,221 |
  | boolean | 35,729 | 35,702 |
  | lib | 14,677 | 14,675 |
  | issue_3012 | 1,028 | 1,028 |

  fixes, 3045 and round_between were identical in the first run. The first ops run wrote lines in
  several pieces and they interleaved across threads. It was redone with one write per line, giving 0
  garbled lines.
- **Ordered sequences (test-threads 1).** All 5 cases are identical line for line:

  | Case | C |
  |---|---|
  | prism::an_arc_tangent_to_the_edges_it_ends_on_leaves_no_hairline_edges | 15,162 |
  | prism::a_bent_edge_crossing_its_neighbour_by_their_corner_leaves_no_sliver | 21,208 |
  | prism::far_faces_of_recorded_sheet_cuts_build_and_their_areas_agree | 164,555 |
  | tangent_rollover::a_lone_round_between_two_cylinders_rolls_over_the_seam_of_one (blend) | 3,421 |
  | boolean::tests::a_five_box_chain_union_all_measures_the_closed_form_with_the_update_per_pair (box) | 209 |

- **Refusals.** R1 = 0 and R3 = 0 everywhere on the branch. On main, B3 (the throwaway build at :3223) = 0
  everywhere.
- **Added work: none.** The ensure step's checks are id and field lookups the read repeats. They do no
  evaluation.

## 4. What this proves and what it does not

**Proves (MEASURED):**
- On every golden case and ops test that reaches the binormal, building first and reading then gives
  the same results.
- It does so with the same builds, in the same order.
- Deferring the cache note after the read is exact in practice.

**Does not prove (INFERRED):**

- **(a) A curve-evaluation error before :3088 on an edge use without a trim.** The lazy form returned that
  error without building. The ensure step now builds first, so it does one extra build and may return a
  different error if the build also fails. This happened 0 times (the counts are identical). It is
  reachable only if the edge curve fails to evaluate at that parameter.
- **(b) The note makes this edge use's trim and the foot then fails.** Lazy used the new trim; the branch
  refuses (R3). Seen 0 times. It needs `use_3d_only` false, no trim and a drop-curve status of
  `SM_ERR_UNKNOWN`, together with no surface foot.
- **(c) :3223's throwaway build.** It is now a refusal. It was never reached on main (B3 = 0).
- **(d) The creation shortens the edge's interval (found by the review).** Measured once in goldens `--all`
  (§0); fixed in `a2bbe62eed`. (a) is gone there too; (b) and (c) are unreachable from a valid body (§0).
- **The run covers what was run.** All golden units and the ops tests were run; the app's bodies were not.

### Other trim-making leaves that reads reach (READ; counts by grep, include tests)

| Leaf | Mechanical? | Call sites | What differs |
|---|---|---|---|
| `QueryTopology::ensure_trim_curve` (`topology/loopuse.rs:159`), used in the containment walk `query/loop.rs:463,513` and `uv_corner_boundaries` `loopuse.rs:1418` | **No** | Leaf: 3. Mutable entry points: `contains_uv_point` 14, `uv_corner_boundaries` 4, `point_classify` (`query/face/p2.rs:841`) 44 | The build sits inside a walk with data-dependent exits: an on-edge hit returns before the next edge use's build, and a failed build returns Unknown. A "build all, then read" ensure would build trims lazy never builds, which is more work. An exact ensure must replay the walk. The `Prepared`/`Requested` arms already exist, and they are the refusal+fallback pattern that is on hold. |
| Mass trims (`mass/integrator/properties.rs:1949`, gap closure; `mass/enclosure.rs:2568` `pcurve`) | **Mostly** | Mutable entry points: `face_area` 55, `face_centroid` 20, `brep_area` 35, `brep_properties` 11, `face_properties` 14 (mostly tests) | Gap closure visits every edge use of every non-vertex loop of the face, with no early exit except errors. So "ensure every edge use of the face" is exact, if it runs only where gap closure runs (a computed condition). The ensure must then sit after the decision, as the cache note does here. |
| Record conversion `create_missing_uv_trim_curves` (`solidcore-convert/src/brep/record.rs:131`) | **Already split** | 7 (5 src, 2 tests) | It is a whole-body ensure called before the record is built. Only the read signature (`&Brep`) is left to do. |
| Tree build's face routine `create_uv_trim_curves` (`topology/face/p3.rs:1231`, from the surface-cache build `query/face/p2.rs:554` and `face_cache_tree`) | **No** | Through every surface cache or tree build (report 42 L2) | It makes trims for the whole face. Its gap fix (`fix_uv_trim_curves`) moves trim end points and can raise vertex and face tolerances. So the ensure is a face edit that changes other edge uses' trims, not a single edge use fetch. Split it as "ensure face prepared" (trims + gap fix) before any tree read. Its result depends on face state only, so it can move earlier, but tolerance raises then happen earlier too. Needs its own measurement. |
| `normalized_evaluate` (`edgeuse/p3.rs` :3465, `get_or_create(..., true, true, true)`) | **Yes, as here** | 56 (55 src) | A single edge use with fixed flags (dist, dev). Same shape as the binormal, with different flags. |

**Size (INFERRED):**
- This split was about half a day of edits, with 54 files touched by script. Measuring it took about 2
  hours of server time.
- `normalized_evaluate`: about the same size (56 sites).
- The mass trims: small (a few internal sites), plus the decision point.
- Record conversion: a signature change.
- The containment walk and the face routine are design work, not mechanical: about 2–4 days with
  measurement.
- All of them: about 1–1.5 weeks.

## 5. Read entry points the binormal split now lets be `&Brep` end to end (READ, from report 42; not flipped)

Each one still needs two things:
- its kernel path changed from `&mut Brep` to `&Brep`, with the read halves and `read_only()`;
- its ops callers to guarantee the trims first (1,302 of 4,101 bodies lacked them; primitives, Booleans
  and blends never do).

Dropping the note (`read_only`) changes no value of the read (§1). It only skips the cache side effect.

| Entry point | Binormal-only per report 42? | Kernel path to make `&Brep` | Still needs |
|---|---|---|---|
| tangent_chain / tangent_face_chain_ordered | yes | `query/tangent_edge.rs` `tangent_all_along`→`decide` (:957); `query/edge_turn.rs` `convex_at` (:223) | path signatures; trims guaranteed |
| carried_faces | yes | `blend/carried_face.rs:254` → `fillet_solver::p5::is_concave_edge`, `chamfer_side::outward` (:53) | path signatures; trims guaranteed |
| face_plane_normal, face_normal_at_edge | yes (C was the binormal's note) | `query/api_queries.rs:2275` `face_normal_at_edge` (:2080) | signatures; trims guaranteed |
| edge_faces_along, edges_share_wedge | yes (C was the note) | `blend/chamfer_side.rs:77` `faces_along`, `:99` `wedge_at_middle` | signatures; trims guaranteed |
| beads | the binormal, at `use_3d_only = true` (weld_bead.rs:470) | `blend/weld_bead.rs` `beads_apart` | It also builds new bodies, so it is not a pure read. Only its measuring part can be `&Brep`. |
| continuity_report | no | it also reaches LiveBrepObjects | report 42 blocker 2 |

None of these was flipped. Their ops callers still go through `measure_body`'s copy.

## 6. The second leaf: `normalized_evaluate` (branch `cache-leaf-split-2`, head `53a7f14856`)

**Result in one line: no result moved.** All 283 golden units, forwards and reversed, changed=0.
The trim builds are identical to main per unit, in count and in ordered (edge use, flags, distance,
deviation) sequence.

### 6.1 The split (READ)

Commits on local `cache-leaf-split-2`, based on `cache-leaf-split` 6531c0bcda:
- `1da7d0d69a` split;
- `2003038882` one helper;
- `a8fd01bdfa` merge of `cache-leaf-split` a2bbe62eed (no rebase), which takes the binormal's shape;
- `53a7f14856` rustfmt.

Server2 ref: `push/cache-leaf-split-2-4`.

- `normalized_evaluate(k, &Brep, eu, t, parameter_space, want_first_deriv, uv_trim_required)` is the
  shared form.
  - In UV space with `uv_trim_required`, a missing trim refuses with `EdgeuseHasNoTrimCurve` at the edge
    use (`SolidError::uv_trim_missing`).
  - Otherwise it is main's read-only arm (the trim if present, else the 3D point dropped to the surface).
- `normalized_evaluate_mut(k, &mut Brep, ..., ok_to_make_uv_trim_curve)` is the one entry for mutable
  callers, with main's arguments.
  - Stage 1 is orientation only; that is all main computes before its build.
  - Where main's lazy branch fires (`parameter_space && ok`), it builds the trim with
    `get_or_create_uv_trim_curve(.., true, true, true)`, status ignored as on main.
  - Stage 2 then reads, requiring the trim only where the edge use has it afterwards.
  - A failed build therefore reads the 3D drop, as main does, and is not a refusal.
  - There is no public ensure: the review's "one helper" rule.
- Every mutable site is a rename to `normalized_evaluate_mut` with identical arguments:
  - edge.rs; face p1 (Mutable arm), p2, p3, and p4 ×3; brep p1, p3, p4; validation/pointers;
  - stitch p1; vertexuse (Changing impl); offset_executive edgeuse and p1;
  - goldens edgeuse_p3, offset_geometry_creation, brep_constructor.
- 3D-only sites (literal `parameter_space = false`) call the shared form. Their last flag changes from
  `true` to `false`, which has no effect.
- No callers in ops, convert, tess or the app crates. The lazy form is gone.

### 6.2 No result moved (MEASURED, server2, `--profile goldens`, at a8fd01bdfa; 53a7f14856 is fmt only)

- **Goldens:** all 283 units, forwards and `--reverse`: 283 passed, every line changed=0 added=0
  removed=0 broken=0. The same held at 2003038882, before the merge.
- **Kernel lib tests:** 14 area filters pass (edgeuse 69, binormal 8, fillet_corner 56, vertexuse 11,
  face::p1/p3/p4, brep::p1/p3/p4, validation 59, stitch 60, offset 478).
- **solidcore-ops tests:** 369 passed and 1 failed. The failure is
  `hlr::tests::a_box_with_rounded_corners_is_drawn_with_its_ball_corners_as_spheres`, the known failure
  on main. All other ops binaries are ok.
- **Static checks:** `cargo fmt --check` 0; `clippy --workspace --all-targets -D warnings` 0;
  `cargo xtask fast --no-tests` 0; `cargo xtask layers` 0.
- **Contract test** `the_uv_evaluation_refuses_a_missing_trim_and_the_mutable_form_makes_it` (p3.rs
  tests), on a sphere (r 23.567) edge use with its trim deleted:
  - The shared form refuses at the edge use, with the body's Debug and `write_stamps()` unchanged.
  - The mutable form then equals main's lazy steps on a fresh copy (get_or_create(true, true, true),
    then the read), and its trim equals that copy's.
  - The shared form then reads the same answer.
  - Closed form: the 3D point is at the radius within 1e-9, and the surface at the UV point is within the
    edge tolerance of it.
- **The test fails before the change.** On main there is no `&Brep` form, so it does not compile. A
  scratch copy of its refusal clause run against main's lazy form fails: main answers `Ok((0, 0.3, 0))`,
  because it builds the trim instead of refusing.

### 6.3 Work and order (MEASURED, scratch counter, never on the branch)

Each of the 283 units was run on its own with `ZZ_LEAF_LOG`, on main (`dd955acde5` + logs) and on the
branch at the merge (+ logs). `at=` was stripped before comparing. 106 units make trims; 177 make none on
either side.

| Count over all units | main | branch |
|---|---|---|
| C (`create_uv_trim_curve`) | 475,734 | 475,734 |
| G (`get_or_create` builds) | 268,144 | 268,144 |
| N (lazy branch fired / helper builds) | 2,200,150 | 2,200,150 |
| units whose ordered C/G/N sequence differs | – | 0 |

- **Builds made because a trim was missing (NI): 24,122.**
  - 10 shortened the edge interval, all in `blend.fillet_executive_p3` on edge uses 42 and 75. The
    upper end moved from 0.750000022610396 to 0.75, a shrink of 2.3e-8.
  - The read's parameter was t = 0.37, normalised to the interval as it stands after the build, on main
    and on the branch alike. That is about 0.435, nowhere near the removed part.
  - Main also normalises after its build, so this is equal by construction.
- **Refusals at the read (NR): 0.**
- **Named ops sequences** (solidcore-ops, `--test-threads 1`, main against the branch): the ordered C/G/N
  sequence is identical, compared by md5, with NR 0 throughout.

  | Sequence | Lines |
  |---|---|
  | `issue_3045_round_at_plate_arc_end` | 30,152 |
  | `issue_3012_filleted_boxes_apart` | 20,944 |
  | `--test boolean` | 879,387 |
  | `--lib hlr` | 4,766 |
  | `--lib blend` | 1,395 |

  Single-threaded, the two hlr tests passed on both sides, including the one that fails in the parallel
  ops run on main and on the branch alike.

### 6.4 Divergences possible in principle (INFERRED from the code), and whether valid input reaches them

1. **The shared form called with `uv_trim_required = true` on an edge use with no trim refuses.** Main
   would have built the trim.
   - Valid input does not reach this through a public entry: every former lazy caller goes through
     `normalized_evaluate_mut`, and the only direct `true` callers are the contract test and the 3D-only
     sites, where the flag is unused.
   - A future caller that passes `true` without the helper gets a typed refusal, not a wrong value.
2. **A failed build.** Main ignored the status and read the 3D drop. The helper reads the same, because
   `uv_trim_required` is whether the trim exists afterwards. Exact.
3. **Order.** Main reads only `orientation` before building, and the build does not change it. The
   interval, clamp and points are all read after the build on both sides. Exact (NI interval edits
   above).
4. **Repeated lookups.** The helper calls `orientation` and `uv_trim_curve` once more, then
   `existing_uv_trim_curve`. These are id and field lookups with no evaluation and no write, and the
   same errors as main for dangling ids.
5. **`vertexuse` `Reading` impl.** Unchanged: it still gives up when `ok_to_make` is set, as before.

### 6.5 Report 42's read entry points after both splits (READ)

- In report 42, `normalized_evaluate` was reached only by **face_section**. That route also goes through
  BrepFaceServices (L4) and the surface cache and tree (L2). So face_section now has every per-edge-use
  trim leaf lifted out, but it still needs:
  - L2's tree and cache fill: `face_cache_tree` may run `create_uv_trim_curves` and its gap fix, a face
    edit;
  - L4's `RefCell<&mut Brep>` frontier;
  - its ops caller to guarantee trims first.
- No entry point moves to READY on this split alone. The entry points with every trim-making leaf lifted
  are still §5's binormal-only list: tangent_chain, tangent_face_chain_ordered, carried_faces,
  face_plane_normal, face_normal_at_edge, edge_faces_along / edges_share_wedge, and beads (its
  measuring part). Each still needs its kernel path signatures changed to `&Brep` and its trims
  guaranteed.
- What remains, by leaf:
  - the containment walk (`QueryTopology::ensure_trim_curve`, L3: point classify, face_line_*,
    face_normal_at_point, classify_points_with_normals), which is design work;
  - the face routine in the tree build (L2);
  - the mass trims (volume, check_solid);
  - `to_record`'s signature;
  - check's flags and gap caches (L5).
