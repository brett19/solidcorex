# Review r2: tangency-labs/solidcraft#20945, commit 9ab662e355 (+ merge 488291069e)

**Verdict: PASS WITH FIXES.** The fixes are wording and documentation only, with no code or data changes:

1. The test doc (heal_edge_tolerance_covers_edge_face_gap.rs:487-494) and the PR body say the plain read is "the same body before its UV trims are rebuilt". That is exact for edge/face gaps and not for vertex/face gaps. Say which is by construction and which is measured (F1).
2. The `replay` doc (:496-499) says the digest hashes "edge/face gaps" of the read. On the rebuilding read it now hashes the plain read's (heal-time) gaps, not the rebuilt body's. Say so (F2).
3. PR body table: the vertex-14 "cached gap as read on main" cell (3.019034068882236e-5, marked "stale") is not the heal's 3.019034067154111e-5. It was a readout that mixed stale and refreshed values, 5.7e-10 relative off the heal's gap (the old 1e-9 allowance). Relabel it (F3).

The changed test does not hide a defect, and the stated mechanism is right.

Scope: the last commit 9ab662e355 (one test file) and the PR body section "A corpus-read test that passed through a stale cache", plus the merge 488291069e's ledger and `transform_in` resolution. I read code only: no build and no test run.

## 1. Is the mechanism right? Yes, each link holds (VERIFIED from code unless marked)

- **The heal runs before the trim rebuild.** `read_from_data` (topology/brep/p7.rs:801-821) calls `make_topology_from_data` first. That calls `heal_made` → `heal_brep` (p10.rs:524-528). Only after it does the read run `rebuild_uv_trim_curves` (rebuilding read) or `check_and_fix_uv_trim_curves` (plain read). The `rebuild` flag is not read before the heal, so both reads build and heal the same body: same entities, ids, curves, intervals and tolerances.
- **The heal's tolerance is a function of the gap it measured.**
  - `cache_edge_props` reads `eu1::max_edge_face_gap(k, b, eu, false)` (heal_data/p2.rs:472). At heal time that calculates and caches the gap.
  - `fix_tol_sizes` reads the cached length (`max_edge_face_gap_length` → `ef_gap_length`, p2.rs:429). It then sets `zone_tol3d(gap_gain * gap)` when the gap exceeds the formula's value (p2.rs:3128-3144).
  - Vertices follow the same pattern: `zone_tol3d(gain * 2 * max(ve, vf))` (p2.rs:3209-3226).
- **The interval change goes through `set_interval`.** `create_uv_trim_curve`'s interval recovery calls `edge::set_interval` (edgeuse/p3.rs:1019 and :1043, the OnePercentEdgeTrimRecovery paths). It is reached from `rebuild_uv_trim_curves` (brep/p2.rs:454).
- **The old test read the cache.** Before this commit, `edge_gap` and `vertex_gap` passed `force_calc = false` (diff at :446 and :466-469). A cache that is initialised is returned as is (edgeuse/p1.rs:917; vertexuse.rs:697 and :725).
- **On main the cache survives the interval change.** Main's `set_interval` is a bare assignment (the PR diff at edge.rs:388). Nothing in the recovery path resets `max_edge_face_gap` or `vertex_edge_gap`. So main's rebuilt body keeps e17's edge/face gap from the heal, over [0, 1].
- **The numbers agree with this mechanism (ARITHMETIC):**
  - tol = cover(3.019e-5) = 3.32e-5.
  - With the fix the cache is cleared, so the test reads 1.2547e-5. cover(1.2547e-5) = 1.38e-5, which is not equal to 3.32e-5, hence the old failure.
  - For v14: cover(gain·2·3.18e-5) = 7.0e-5, which is not equal to 6.64e-5.
- **One refinement to the explanation.** On main the v14 readout was not purely stale:
  - The edge/face gap is a function of curve, interval and surface only (edgeuse/p1.rs:1007-1027).
  - The vertex/face gap seeds its projection from the UV trim ends (`compute_uv_point` doc, vertexuse.rs:1159-1166), so rebuilt trims move it slightly.
  - Main's 3.019034068882236e-5 against the heal's 3.019034067154111e-5 is that 5.7e-10 relative shift. It is what the removed 1e-9 allowance absorbed.

## 2. Does the test still guarantee the invariant? Yes

**Which gap each tolerance is compared with (VERIFIED, :523-590):**

| read | raised edge: tol == cover(g) | every edge: tol ≥ now | moved vertex: tol == step, with step from gap | moved vertex: 2·now ≤ tol |
|---|---|---|---|---|
| plain | g = forced gap on the final plain body | forced | gap = forced | forced (implied by the step) |
| rebuilding | g = forced gap on the plain read, the heal-time body | forced on the rebuilt body | gap = forced on the plain read | forced on the rebuilt body (**new**) |

**Is the plain read a faithful stand-in for the gap the heal saw?**
- *Edges: yes, by construction (ARGUED from code).*
  - Both reads run the same deterministic topology build and heal.
  - After the heal, the plain read only runs `check_and_fix_uv_trim_curves`, which deletes failing trims. It also runs `cover_kept_uv_trim_curves`, which raises tolerances, and the test undoes those raises (`undo_trim_cover`).
  - Neither touches a curve, interval or surface. The forced recalculation is the same `calc_max_edge_face_gap` the heal ran.
  - The one assumption is that no heal step after CacheEdgeprops changes a raised edge's geometry. The bitwise `tol == cover(g)` check would catch a violation, and it passes on all 28 files (MEASURED by the builder, not re-run).
- *Vertices: only measured.* The vertex/face gap depends on the trims present (the seed). The plain body's trims are the post-check set. The heal-time body's trims are those of the file or the healer. Bitwise equality for every moved vertex (including v14, now held to the bit) is MEASURED by the builder, not guaranteed. This is the substance of fix 1.

**Sensitivity (VERIFIED by reading the conditions):**
- A heal tolerance below its gap's cover, or above it, fails bitwise at :541 (edges) and :585 (vertices) on both reads.
- A tolerance that changes on the rebuilding read when it should not:
  - non-raised edges and vertices still hash `tol` into the digest (:547 and :592);
  - raised ones are compared bitwise with cover or step, which does not depend on the rebuild.
- The rebuilt body's real gaps are now checked forced, for edges (:527) and for moved vertices (2·now ≤ tol). On main that check silently used stale values.

**Removed or loosened assertions:** none.
- The 1e-9 relative allowance on the rebuilding read when step == cover is removed, so v14 is now held bitwise. This is a tightening, confirmed.
- `2.0 * now <= tol` is added. It is new on the rebuilding read and implied on the plain read.
- `tol < now` keeps its form, but its argument is now fresh rather than cached, which is a tightening.

**Would "force only" have been a simpler honest fix? No.** With only the force added:
- The rebuilding read would compare e17's tol 3.32e-5 with cover(1.2547e-5) and v14's with cover(3.18e-5), and both fail.
- Its digest would hash 1.2547e-5, so `MAIN`'s rebuilding-read digest for open_shell would have to be re-recorded (0xb278077fa8594f34).
- The invariant ("each changed tolerance is the cover of its gap") would need an exception for edges whose interval moves after the heal.

The builder's approach, judging against the heal-time body, is the more faithful one.

**Minor (non-blocking).** Nothing asserts that the plain body and the rebuilt body have the same edge and vertex ids. `edge_gap(&healed_k, h, e)` assumes this. It holds today by construction (see above). An `assert_eq!` on the counts would make a future topology change in the rebuild fail loudly instead of unwrapping a missing id.

## 3. "No recorded value changed" (VERIFIED from the diff)

The diff of 9ab662e355 touches `edge_gap`, `vertex_gap`, `replay`'s body and doc, and adds one new test. No change to `MAIN`, `RAISED_EDGES`, `MOVED_VERTICES`, `REBUILD_*`, the floored list (:1166-1173, unchanged and still listing "open_shell rebuilding read v14"), the `(206, 296)` counts or any digest constant.

**Why the digest returns to main's value (ARGUED, by construction plus one measured premise):**
- The digest feeds `d.f64(g)` for every edge (:550).
- On main's rebuilding read, g was the cached gap. Nothing clears it during the read, so it is the heal-time value everywhere, including e17's stale 3.019e-5.
- On the previous head the cache was cleared, so g = 1.2547e-5, giving digest 0xb278….
- Now g is the forced gap on the plain body, which for edges equals the heal-time value by construction (section 2). So the hashed values are the same values main hashed, by construction.
- The premise is that main's caches held heal-time values on every edge (no other `set_interval` or reset during the reads). The builder MEASURED that this is the only `set_interval` call across the 28 reads.
- Vertex gaps are not hashed, so the vertex-only measured equality does not bear on the digest.

**Consequence (fix 2).** The rebuilding-read digest no longer pins the rebuilt body's gaps at all. They are guarded only by the coverage checks. This is the honest like-for-like comparison with main's (stale) hash, but the doc should say it.

## 4. The new regression test `open_shell_rebuilt_trim_leaves_no_stale_gap` (:1206): sound

- **Real corpus file, real path.** It reads `Fillet/open_shell.smb` with rebuild and asserts that e17's interval moved (`0 < min < 1e-5`) while the plain read keeps [0, 1]. So `set_interval` was provably reached through the recovery.
- **It asserts values.** For every use of e17 it checks cached edge/face gap == forced, bitwise. For every v14 use it checks vertex/edge and vertex/face, both as `Option` bits. It also requires `checked > 2`.
- **It fails on main for the stated reason (INFERRED from code; the builder says MEASURED).** The heal leaves e17's uses cached at 3.019e-5 over [0, 1], and main's `set_interval` keeps it, so the first use's read (stale) is not equal to the forced 1.2547e-5. With the fix the cache is cleared, so the read recalculates.
- **Limitation.** After the first use's forced calculation, its mate is synchronised (p1.rs:1037-1043), so the second use's comparison is trivially equal. That is harmless: the first comparison is the one that discriminates.

## 5. Coverage lost by forcing everywhere (non-blocking; follow-up recommended)

- The old test's cache reads were not a stale-cache detector: a stale cache made it pass, not fail. Forcing removes an accidental, wrong-direction sensitivity, not real coverage.
- What is absent is a corpus-wide check that cached == forced after each kind of read. It is cheap to add: in `edge_gap` and `vertex_gap`, read once with `false` before the forced read and push a failure on a bit mismatch. That costs one cache read per use, on 28 files × 2 reads.
- It should not block: the PR's fix is proven by its targeted tests 1-5 plus this file's new test.
- Without a build I cannot say it would pass. Trim rebuilds re-seed vertex/face gaps (section 1), and I did not verify that every vertex/face cache is cleared or refreshed on the rebuilding read. It could surface a further stale-cache class outside #20939, which is worth a follow-up issue rather than this PR.

## 6. PR body section

- Mechanism, numbers and the "Test change:" line describe this commit and its reason accurately.
- MEASURED is used for the probe, the table and the 4000-sample check, which is correct. Nothing is mislabelled PROVEN.
- No local paths, no session links, no mention of threads, threading, parallelism or concurrency.
- Wording fixes:
  - (F1) "the plain read of the same file before its trims are rebuilt": add that it is after the plain read's trim check. Its edge/face gaps equal the heal's by construction. Its vertex gaps equal them as measured.
  - (F3) The v14 "cached gap as read on main" cell is the mixed readout, not purely the stale heal value (it differs from the "gap the heal measured" cell by 1.7e-14).

## Merge 488291069e (VERIFIED by diffing against main 1b31b754ae)

- **`trim_tolerance_main.tsv`.**
  - Both comment lines are kept: this branch's #20939 line and main's #20938 brep_p1 line.
  - `topology.brep_p1`'s digest is main's (no diff against main on that line).
  - Only the three units this PR moves differ: blend.fillet_executive_p1, topology.edge and topology.face_p3.
- **`transform_in` (brep/p5.rs:3603-3778).**
  - Main's #20944 body is intact: `set_this_model_size_estimate` with no context write, and no diff against main outside the PR's own hunks.
  - The #20939 cache-clear block sits after the reflection swap and before the tolerance scaling, as reviewed earlier.
  - Nothing was lost or duplicated.

## Could not verify without a build

- That the reworked test and the new test pass on head, and that the new test fails on main.
- The builder's 28-file probe: bit-identical tolerances, intervals and gaps between main and head, with open_shell e17 as the only `set_interval` call.
- That every moved vertex's plain-read gap equals its heal-time gap bitwise. The passing bitwise check would show this, but I did not run it.
- Whether a corpus-wide cached == forced sweep would pass (section 5).
