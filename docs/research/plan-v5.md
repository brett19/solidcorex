# solidcore caches: plan, version 5 (2026-11-23)

Supersedes plan-v4. Same three stages; this version records what has landed, what the measurements since
v4 settled, and the order for stage C. Private: the end goal (work inside one operation spread over
threads) is never named in an issue, PR or commit.

## Stage A: an answer does not depend on which kernel computed it

| Step | Issue | PR | State |
|---|---|---|---|
| A body's size estimate is its own | #20938 | #20944 | merged |
| One kernel routine for the duplicated trim loop | #20940 | #20943 | merged |
| Random draws are a function of the call; the kernel's generator is gone | #20936 | #20942 | merged |
| Fillet step latch and its flag retired | #18916 | #20941 | ready, CI green |
| Cached gaps cleared on a scale and on an interval change | #20939 | #20945 | draft: a corpus read's tolerances grow with the fix; mechanism under investigation |
| One reset rule for the two kernel pools | not filed | n/a | unblocked once #20941 merges: after the three merged fixes little is left to reset |
| Compile-time Send + Sync assertions | n/a | n/a | held: hard to justify publicly without naming threading |

Lessons written into `briefs/local-checks.md`: run `cargo xtask fast --no-tests` (layers) before every
push; a blessed golden needs its ledger digest; other goldens-crate modules may pin the same rows.

## Stage B: a kept cache is read in place

Issue #20976. One combined PR, #21019 (draft), from three steps:
1. state bits read by borrowing (done; one edge fillet 260 -> 130 whole-cache copies);
2. solve modes carried by the solve (done; no result or count moved, removes the reason to write a cache);
3. lend the kept cache to the solve (in progress; copy only where a consumer really writes).
Not needed for these counts: a handle-view redesign. Measured with temporary counters
(`count-kit/`), never committed.
After step 3 a solve still needs a writable cache for: the iso boundary curve memo, lazy tree builds,
lazy bounding planes, and the drop and trace signatures. Those are stage C's "a lent cache is complete".

## Stage C: prepare, read, apply, one operation at a time

### What the studies and probes established

| Operation | Coupled writes | PRs (est.) | Result-changing | Report |
|---|---|---|---|---|
| Reads between operations | 1, removed by preparing trims in edge order | 10-14 | 1, maybe 2 | 11 |
| Validity check | 3 (+2 in update mode) | 9-11 | 2, maybe 3 | 12 |
| Boolean | 3 (+1 conditional), none fired in 47 Booleans | 18-29, likely fewer | at most 3, on rare paths | 07, 14, m9 log |
| Fillet | 4 (1 removed by stage A) | 14-22 + the Boolean's | 2 or 3 | 13 |

Measured on 47 Booleans (tangent, coincident to 0 / 1e-6 / 1e-4, edge in face, vertex on edge, scale
ratios to 1e5), `m9-boolean-coupling.log`:
- the edge raise, the vertex raise, the edge-curve repair and the conditional vertex block were applied
  0 times (the vertex raise was considered 274 times, never needed);
- no UV trim is created inside the pair loop; the loop's only writes were 12 trimmed cache tree builds;
- a repeated face-level trim pass changed nothing on 52 bodies; a first pass only creates missing trims;
- preparing trims before the pair loop (every face, paired faces, paired faces in first-use order) gave
  identical results in all 141 comparisons; prepare paired faces only (preparing every face leaves trims
  on faces the Boolean never touched);
- the pair loop's "edited" marker (224 times) records that a mutable entity handle was REQUESTED
  (`ClassGeometryMut::topology_entity_mut`, topology/face/p3.rs:2310), not that anything was written;
  the extract step's cache clearing never fired.
In every operation studied a phase-scoped cache table needs no key and no version: while the body is
borrowed shared, "built in this phase" is the validity rule.

### Order

1. **App reads off the edit door** (ops layer only). About 30 reads go through `with_body`, which snapshots
   the whole body and drops its proof; point classification copies the body twice. No kernel change.
2. **Reads between operations**, closest first: mass / volume / area (need trims, and the B-spline
   `Unclamped` flag passed as an argument), then tight box and face queries (need the slot lent, stage B),
   then classification and rays (a face-cache request loop; `BrepQuery` split into shared prepared data
   and a per-reader cursor).
3. **Validity check**: restores the tolerance document's invariant that a check changes nothing and is a
   function of the body; report separated from repair; gaps as values; the ops layer's copy before every
   check goes.
4. **Boolean**: prepare paired faces and their caches before the pair loop (evidence above says no result
   moves), read phase over the pairs through a phase table, the coupled writes as requests on a rare
   path, then the classification phases.
5. **Fillet**: heavy phase behind the existing compute / insert split; per-solver records returned and
   applied in order; the certificate's span hint; insertion rides on the Boolean.

Steps 1 to 3 build the shared pieces (prepared view over `&Brep`, phase table, a kernel per unit of work)
on the easy cases and each pays for itself (fewer body copies, a check that does not write).

### How it is enforced
- A read's signature becomes `&Brep` as its last step; flipping a signature in a scratch copy and running
  a check build lists the writes underneath (use this to size each step).
- `&Brep` does not stop writes through locks and atomics: the surface cache slot's take and put, the
  B-spline evaluation flags, the offset distance. Stage B and step 2 remove those. Pure value-keyed memos
  stay.
- Later, if wanted: a shrinking checked-in list of interior mutability in geometry types, and a
  `Prepared` wrapper type obtainable only from the prepare step.

## Open
- #20945: which tolerance is right on the corpus read.
- Stage B step 3's design (lend versus copy-on-write) and its review.
- A near-tangent union (cylinder sunk 1e-6 into a plane face) returns a body that fails its validity
  check ("bad sector angle sequence"), with or without preparation: an existing defect, to file.
- Still unstudied: offset and shell, sweep, heal and stitch, import.

## Addendum, 2026-11-23 (later): every major operation now studied

Merged since: #20941 (fillet latch). Stage A is complete once #20945 (ready, CI running) lands; the pool-reset
item was dropped (the two pools' resets already agree on everything that affects an answer). Stage B's
combined PR #21019 is ready with CI green (one edge fillet: 260 -> 2 whole-cache copies). Stage C step 1 is in
progress (#21071, draft #21072).

Reports 16 (offset, shell, sweep) and 17 (import, heal, stitch), code readings:

| Operation | Coupled writes | PRs (est.) | Note |
|---|---|---|---|
| Read from data (record to kernel, file read) | 0 | 3-5 | geometry pass already takes `&Kernel`, one item per edge and face; needs no caches |
| Offset face loop | 0 | 5-8 | `face::build` already returns a plain value from `&Brep`; no result change expected |
| Shell of a solid | n/a | 0 | ends in a Boolean difference: rides on the Boolean |
| Heal | 1 (trim raise seen by the neighbouring face) | 2-4 | runs only on kernel file reads and untrusted arrays; low payoff in the app |
| Stitch | 3 (the greedy joins themselves) | 2-3 | only covers and measurements can become read phases |
| Offset edge loop | 3, firing on every offset | 4-6 | optional, later |
| Sweep, prism, primitives | n/a | 0-2 | construction code; no read phase of their own |

Facts that change earlier assumptions:
- A body converted from a record does NOT always carry every UV trim: records with analytic labels and no
  NURBS form, and revolutions, get none; bare uses have none on purpose. Records the kernel wrote itself do.
  So "bodies in the app are already prepared" is true for kernel-written records only.
- Stitched faces left trimless do reach a record: `to_record` builds their trims on the live body with the
  proved raise, so the export changes that body and the stitch's no-trim cover does not survive it. Stitch
  results should be prepared on request, not eagerly.
- No writer touches an offset surface owned by a face; the stage B guard (never lend a cache holding an
  offset surface) could be narrowed to surfaces that allow extension. One small PR, no result change.
- Several construction paths return a body with `editing_enabled` still set (UNVERIFIED whether read later).
- The sampled-raise mode is read in one place and set around one export; making it an argument is 1 PR.

Revised order for stage C:
1. App reads off the edit door (in progress).
2. Reads between operations (mass first), with read-from-data alongside.
3. Validity check.
4. Boolean (shell of a solid comes with it).
5. Offset face loop.
6. Fillet (stitch covers inside it).
Later or optional: heal, offset edge loop, stitch measurements.

Probe m10 (per-type cached gap against a fresh computation, 28 corpus files, both reads, on #20945's head):
edge-to-face 0 stale; vertex-to-face 0 stale (a trim rebuild already clears them: 24 filled after a
rebuilding read against 1,361 after a plain one); vertex-to-edge 8 mismatches at rounding level on closed
edges; edge-to-next-edge 4 mismatches in one file (cached 0, fresh 9.48e-6, also on the plain read):
unexplained, read only by the legacy healer.
