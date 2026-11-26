# Review of plan-v3 ("caches as values, bodies stable under reads")

**Verdict: rethink.** The direction (keyed immutable caches, reads that do not write) is right and Tier 1 survives. The crux does not: "an operation returns a completed body" (rule c, step 3.1) does not make the read stack pure, because most of that stack runs inside operations on bodies that are mid-edit, and "completed" is not definable today for every body an operation returns. Tiers 3 to 5 as written are 150+ PRs resting on that rule, on keys that omit inputs the builds read, and on a reporting scheme the rule set does not accept.

Read-only review on 782ef9afc. Nothing was built or run. `K/` = `solidcore/crates/solidcore/src/`. **[V]** = I read the code at the cited lines. **[I]** = inference from what I read. I did not open GitHub, so #20694 and #20785 are judged only from the tree.

---

## Findings, most serious first

### F1. Rule (c) covers reads between operations; the `&mut Brep` read stack is mostly called inside operations

- **[V]** The restart protocol that 5.2 deletes fires mid-Boolean: `keep_face_cache_tree` / `face_cache_tree` at `K/boolean/topology_intersector/p1.rs:4596,4613`; `QueryTopology::Mutable` builds a tree during classification at `K/query/face/p2.rs:1046-1056`.
- **[V]** The flag doc `K/kernel/fixes/brep_ray_classifies_current_trims.md` describes classification "while the body is being edited" with open loops and trims that fail to implant, answered by `FaceTrimsUnavailable`. `K/query/brep/ray.rs:211` reads `b.editing_enabled()` for it.
- **[V]** `Brep::editing_enabled()` has 132 non-definition read sites in the kernel and 223 set sites; the trimmed build branches on it (`K/surface/trim_srf_cache.rs:795-801`) and sets it during the implant (`:820-826`).
- **[I]** End-of-operation completion (3.1) does nothing for these callers: new edges inside a Boolean, fillet or offset have no trims when the operation's own classification reads them. So 5.1 cannot flip those functions to `&Brep`, and 5.2 cannot remove the restart protocol, unless each operation is restructured into edit, complete, read phases. That is the algorithm change the plan puts out of scope (Tier 6, decision 5).
- **Meaning for the plan:** split the goal. "Between operations" is reachable cheaply (see the alternative plan). "Inside operations" is a per-operation redesign and must be its own plan with its own evidence. The dependency arrow `4.4 -> 5.1 -> 5.2` is wrong as drawn.

### F2. "Completed body" is not well defined today, and completing changes results in ways that are hard to prove right

1. **Two creation routines give different bodies. [V]** `to_record` uses the per-edge-use routine (`solidcore-convert/src/brep/record.rs:131-136`). A cache build uses the face-level routine (`K/surface/trim_srf_cache.rs:830-832`), which also moves end points of a trim and its neighbour and raises vertex tolerances (`K/topology/face/p3.rs:1390-1470`, second loop over all edge uses of the loop, run even when every trim already existed, `:1327-1329` skips only the creation). The plan does not say which one "completion" is. Either choice differs from today on some faces.
2. **A trim can fail, by design, with no record of it. [V]** `get_or_create_uv_trim_curve` returns `status: Err` and leaves the edge use bare (`K/topology/edgeuse/p3.rs:1717-1732`); nothing stops the next reader retrying. Healing forbids creation outright (`uv_space_ok == false`, `:1711-1715`). The stitch deliberately returns edges on faces "with no trim there" and covers them with a proved tolerance (`cover_joined_faces`, `covered_trimless_faces`, `keep_trimless_faces_covered`, header `K/topology/edgeuse/p3.rs:107-113`, body `:2026`). So bodies an operation returns today can have trims that are absent on purpose or cannot be made.
3. **`to_record` and the face routine fail on the first failed trim. [V]** `?.status?` at `record.rs:135`; `status?` at `K/topology/face/p3.rs:1383`. A kernel-level completion built from either turns a kernel success into a failure for every such body. The plan's "or a recorded failure" needs a new field on the edge use, a rule for when a recorded failure is retried (after any raise? then termination needs a round cap), and a decision for each reader that today retries, ignores or errors differently. None is in the plan.
4. **"Fixpoint" is a round cap in practice. [V]** Existing settle code is capped and heuristic: `SETTLE_ROUNDS = 8`, `SETTLE_RISE = 2e-3`, "a heuristic cap" (`K/topology/edgeuse/p3.rs:2414-2416,2550,2564`). **[I]** Creation is once per edge use and tolerances only grow, so "every trim exists" terminates; "raises settled" does not without a cap, because a later raise changes the fit tolerance earlier trims were fitted at and nothing refits them. The result stays order-defined, not order-free. A fixed order makes it reproducible; it does not make it the same as today's lazy order.
5. **Proof burden. [I]** AGENTS.md 144-148 needs, per moved case, old and new values and why the new one is right. "The order is now fixed" is not a correctness proof of an edge tolerance. Each moved tolerance needs its enclosure argument (`trim_deviation_bound` exists for that, report 02 §3.5). M2 is therefore not a measurement step; it decides whether 3.1 is landable at all, and belongs before any Tier 2 work that exists only to serve 3.1/4.2.
6. **Cost is known to be large. [V]** `K/kernel.rs:1036-1039`: proving every trim on import "cost many times the translation (a 561-part assembly opened in 126 s instead of 20 s)", which is why `with_sampled_trim_raise` exists (caller `crates/smlib/src/core/convert.rs:303`). Kernel-level completion pays the proved raise on every intermediate body of a multi-call recipe, including bodies that are discarded, and leaves the raises on edges that survive. **[I]** That is a result change and a slowdown for flows that today never build those trims.
7. **Raise mode is ambient. [V]** `k.caches.prove_lazy_trim_raise` chooses proved or sampled at creation time (`K/topology/edgeuse/p3.rs:1835`). **[I]** Moving creation from "first read after import" to "end of import" changes which mode a trim is made under. Not mentioned in the plan.
8. **Read-only operations. [I]** An operation that reads and returns must either complete its input (a read that writes, criterion 1 broken one level up) or require a completed input. The second needs a way to know: every `*_mut` accessor un-completes a body and there are about 600 non-test accessor call sites on public fields (F5). This wants a type (`Prepared<'_>` over `&Brep`), not a convention.
9. **Rollback. [V]** ops runs every edit, `to_record` included, under `Brep::with_rollback` (`solidcore-ops/src/session.rs:699-722`, `body.rs:501-517`). A refused or failed call puts back the snapshot, so completion writes vanish with it. Fine if bodies are always complete on entry; a trap during the transition.
10. **The proof beside the body. [V]** `Outward` "stands for that B-rep only while it is unchanged" (`solidcore-ops/src/body.rs:60-66`). **[I]** Completion after `prove` drops the proof; `prove` itself takes `&mut Brep`. Order of prove and complete must be fixed.

**What already holds, and the plan undersells: [V]** in the app every body that is displayed or saved has already gone through a completion loop: `to_record` (`record.rs:131`), `flat_from_brep` (`crates/smlib/src/core/convert.rs:801`) and `crates/smlib/src/core/trim.rs:140`. Three copies of one job in two layers, which `docs/layers.md` calls a bug. Folding them into one kernel function called from the ops waists reproduces today's app results by construction and is reportable on its own.

### F3. Keys in the target table omit inputs the builds read

The code's own key is wider than the plan's. **[V]** `CurveMemoKey::of_kernel` (`K/curve/curve/p5.rs:407-423`): tolerances, three context scalars, `doing_boolean`, all `fixes`, owner zone tolerance. `FaceCacheKey` (`K/query/brep/ray/live.rs:117-140`) adds `b.editing_enabled()`. The plan's table says "kernel tolerances + fixes".

| Cache (plan §4) | Missing from the stated key | Evidence |
|---|---|---|
| Curve value | context scalars; `doing_boolean` until 2.6 lands; the B-spline curve's own out-of-bounds flag | [V] p5.rs:414-419; flag hits in `K/nurbs/b_spline_curve/p1.rs` |
| Surface value (4.1) | owner/face tolerance (plan notes it); context scalars; blend net scope; the surface's out-of-bounds flag, which `Clone` copies at its current value; offset distance; whether the NURB is made; request tolerance of iso boundary curves | [V] `K/nurbs/b_spline_surface/p1.rs:864-866,900-902`; `K/surface/offset_surface.rs:505`; `K/surface/surface_cache/p1.rs:3146-3161` |
| Face of a body (4.2) | `editing_enabled` and `implants_while_editing` (standalone or trimmed); `doing_boolean` (built or deferred); owner-cleared scope; edge and vertex tolerances and the trims themselves (not face fields); `prove_lazy_trim_raise` if the build may create trims; which creation routine ran | [V] `K/surface/trim_srf_cache.rs:795-801,838-872`; `K/surface/surface/p1.rs:1045` |
| Body | "body version" is not unique across copies or rollback (F5) | [V] `K/topology.rs:1400-1402` |

Two contradictions inside the plan:
- **2.5 against 4.1. [I]** 2.5 moves the context size scalars onto the body. They are build inputs of every value-keyed geometry memo ([V] p5.rs:414-418). A "face-free" memo on the surface value then needs a body property in its key, or the scalars must stop being build inputs. The plan decides neither.
- **Iso boundary curves. [V]** A request at a looser tolerance is answered from curves built at a tighter one, and the stored tolerance is overwritten on every miss (`K/surface/surface_cache/p1.rs:3146-3161`). An exact-key memo returns something different from today. Any step that absorbs this (2.3, 4.1) moves results.

### F4. "No result change" labels that do not hold

| Step | Label | How it changes a result |
|---|---|---|
| 2.1 | no | [V] The out-of-bounds flag is not only a scope. It is set for good through `&mut` (`K/blend/fillet_removal.rs:1576`, `K/blend/fillet_geom/p2.rs:3661`, `K/surface/surface/p3.rs:256,2480,2521,2716`) and `Clone` copies its current value (`b_spline_surface/p1.rs:864`). [I] A copy taken inside a scope keeps the flag after the scope ends, including the surface copy a cache holds. An explicit argument either reproduces that leak or changes it. Offset distance is the same: the intersector "shares the surface with its owner" and writes its distance (`K/intersect/surface_intersector/p1.rs:2749-2757`), so any other holder of that value sees the change today. |
| 2.3 | no | [V] `BoundaryMemo` stores one entry ever, `ContinuityMemo` last writer wins, `SingularitiesMemo` first four (report 01 §4, mechanisms differ). [I] Multi-entry is safe only if keys are complete; F3 shows they are not for non-whitelisted kinds. Extending past the whitelist needs 2.1 and 2.6 first. "Arc hits with no deep copy" is a speed-up and needs a count (AGENTS 42-44). |
| 2.4 | no | [I] Replacing the address comparison in `K/query/brep/ray.rs:70-76` changes when preparation is reused, and so which body state a restarted solve sees. |
| 3.2 | no | [V] 223 setters, 132 readers, plus 19 uses in sibling crates and 12 in `crates/smlib`; `Brep::copy` carries the flag (`K/topology.rs:904`), snapshot and restore carry it. [I] An operation context must reproduce per-body, per-copy, per-rollback values exactly; one missed reader changes standalone against trimmed builds. Sized L; this is XL and crosses into the app crate. |
| 3.4 | no | [V] Tolerance setters and the direct writes send no notification (report 04 §4.2 items 1-3; `K/query/face/p2.rs:1570-1581`). Named edits notify and end in `rebind_geometry_owners`. [I] Routing raises through them marks caches modified where today they stay in use: results move. |
| 4.4 | no | [V] The slot is used as object identity: `K/blend/fillet_solver/trace_reach.rs:154`, `K/query/face/p2.rs:461`, `K/surface/surface/p2.rs:1504`. Deleting it needs the "same surface" identity the plan defers to Tier 6. |
| 2.6 | yes, size M | Understated. [V] `doing_boolean` lives on `Context`, which also exists outside the kernel's own (`n.context`, `K/topology/edge.rs:2783`) and per body kernel in the sweep store (`K/sweep/topology_sweep/engine.rs:842-860`). Deleting it means choosing eager or deferred for all of about 59 scopes. Eager builds run `create_uv_trim_curves` on mid-edit topology; deferred changes the state bits readers see (report 04 §3.6 item 3). |

### F5. Versions (2.4) are not sound as specified

- **[V]** Record fields are `pub` (`K/topology.rs:456-478`). Non-test call sites of `face_mut` 69, `edge_mut` 107, `edgeuse_mut` 138, `vertex_mut` 55, `vertexuse_mut` 56, `loopuse_mut` 47, `faceuse_mut` 48 (awk count up to each file's first `#[cfg(test)]`). Direct writes outside the setters that touch what a face cache reads: `K/query/face/p2.rs:1570,1576,1581`; `K/topology/edgeuse/p3.rs:2450,2452`; `K/topology/brep/validation/wide_vertices.rs:106,121,228,231`. `face_mut(f)?.surface.as_mut()` hands out `&mut Surface` (`K/topology/face/p3.rs:1264`).
- **[V]** What a face's trimmed cache reads is mostly not on the face: edge-use trims, edge tolerance, vertex tolerance and position (`K/surface/trim_srf_cache.rs:846-872`). **[I]** A face version is correct only if every setter on an edge, edge use, vertex and loop use bumps every face around it. Nothing does that today; the tolerance cascade goes edge to vertices and stops (`K/topology/edge.rs:2302-2325` per report 02). The plan's "setters bump the face version" hides this fan-out. Decision 2 rejects per-entity counters because edges are shared copy-on-write, but face versions need the same edge writes observed.
- **[V]** `restore` is `*self = saved.0` (`K/topology.rs:1400-1402`); `with_rollback_untouched` swaps bodies in place (`:1453`); copies carry all fields. **[I]** A counter stored on the body or face is restored by rollback and duplicated by copies, so (face id, version) recurs with different content. Safe only while every cache is physically inside the record it describes. Anything held outside (ray preparation, an ops-level table, a worker) needs a never-reused stamp, not a counter.
- **[V]** Ids are not reused within an arena (`K/topology.rs:226-229`). **[I]** After a rollback, ids issued by the discarded edit are issued again.
- **[V]** `Shared<Arena>` is used for `edges` and `edgeuses` only and clones the whole arena on first write (report 02 §6). **[I]** A cache stored on a face that describes shared edge records is exactly the "cache built on a copy answers for the original" defect in a new place unless versions see edge writes.

### F6. Decision 3 does not work under the rule set

- **[V]** AGENTS 301-305 is inside "## 4c. Concurrency and parallelism". Citing it names threading by reference. The owner's constraint and decision 3 contradict each other.
- **[V]** The same paragraph says latching state "goes when its construction is replaced". That is a statement of when, not a licence for standalone refactors.
- **[V]** "Anything not reported waits ... reviewed after all the ones that do" (45-46); "Show the problem first ... a fix nobody has seen the problem of, or built without that research, is not ready for review" (75-90); "No new document without the maintainers" (211-216, refuses 0.4 as written); layers §5 "No period with two copies, no flag choosing between them" (refuses a staged 4.2 with slot and face cache side by side, and M2's switch may not land).
- **[I]** What a maintainer says cold to 2.1 or 5.3: no issue, no failing case, hundreds of lines across ported fillet solvers or 1,232 signatures, conflicts with every open claim, goldens at risk; reviewed last, most likely closed.
- **An honest justification exists and needs no threading.** [V] The app's pool states the requirement: "the same operation on the same bodies answers the same on any kernel, however much that kernel did first" (`crates/smlib/src/core.rs:155-166`), and both pools reset state for it. [V] `docs/failure-codes-and-reproduction.md` is listed as the design for single-entry replay of a failed call. [I] A failure that replays only with its history is unreproducible. Each history-dependent answer with a repro is a real, reportable defect under that requirement. Steps with no such repro have no justification under the rules and should not be in the plan.
- **Consequence:** 1.5 and M5 are gates, not parallel work. If they find no reachable wrong or differing answer, Tiers 3 and 4 have no standing.

### F7. Claims about caches and solvers are only partly true

- **[V]** No query subdivides a built tree; marks and classes are written only by build code (grep of every subdivide, implant and mark entry point; callers are `surface_cache/p2,p3`, `trim_srf_cache.rs`, `query/tess.rs`, tests).
- **[V]** Solver paths do build lazily. `SurfaceCache::get_tree(&mut self)` builds when there is no tree, and rebuilds the whole tree on every call for a face-owned untrimmed cache (`K/surface/surface_cache/p3.rs:1226-1256`). The same rule is duplicated at `K/surface/trim_srf_cache.rs:744-751`, `K/query/face/p2.rs:668-674`, `K/surface/surface_cache/face_context.rs:388-396`. Under `doing_boolean` the first `get_tree` inside a solver is the implant, with `face: &mut dyn CacheFace` (`K/blend/hcr.rs:627,689`, `K/query/brep/ray.rs:244`), so it creates trims and raises tolerances during a solve.
- **[V]** Lazy extras written during solves are not all pure: iso boundary curves (F3).
- **Meaning:** "immutable cache plus scratch" needs the deferred build removed first (2.6 precedes 4.2; the graph does not say so), and the lazy extras need their own keys.

### F8. 4.1 gives 4.2 less than the table suggests

- **[V]** For non-planar faces the implant subdivides every leaf a trim crosses down to a minimum node size, and re-implants earlier edge uses into the children (`K/surface/trim_srf_cache.rs:1249-1275,1300-1320`). Leaf boundaries decide which roots solvers find (report 01 §A4(b), not re-verified).
- **[I]** The face's tree is the base tree plus deep edits along every boundary, on a `Vec`-backed tree. Sharing the base needs a per-face copy of the node and aux arrays (today's cost) or a persistent tree (a new data structure in a ported unit). 4.1 saves the base subdivision and the continuity and singularity work, no more. Planar faces skip subdivision. 4.1 is not useless; it is not "overlay", and 4.2's XL does not shrink because of it.
- **[I]** The held cache is generic over the handle type and consumers map it (`map_handles`). A cache "held with the face" must fix its handle space first, so 4.3 precedes 4.2; the graph has them parallel.

### F9. Order, size, and steps not worth doing alone

Missing edges: 2.1 and 2.6 -> 2.3 (extension) and 4.1; 2.5 -> 4.1 (scalars); 2.6 -> 4.2; 4.3 -> 4.2; identity for "same surface" -> 4.4; 1.3, 3.3, 3.4, 4.5 -> 5.1; M2 -> everything that exists for 3.1; 1.5/M5 -> Tiers 3 and 4.

My PR estimates (many agents, goldens per unit, a few sites per PR as AGENTS asks):

| Tier | Plan implies | My estimate | Most underestimated |
|---|---|---|---|
| 0 | 4 | 6-9 | 0.2: goldens digests re-bless every unit and collide with every open PR |
| 1 | 4 | 5-8 | 1.3 |
| 2 | 6 | 35-60 | 2.4 (about 600 accessor sites, fan-out bumps), 2.6 (marks at 34 sites, net scope at 16, `doing_boolean` at 59), 2.1 |
| 3 | 4 | 30-50 | 3.2 (355 sites plus two other crates), 3.1 (82 `refresh_cached_topology` sites in 28 files, each family's goldens) |
| 4 | 5 | 35-55 | 4.2, 4.3 |
| 5 | 4 | 60-110 | 5.3 (1,232 functions), 5.1 |

Not worth doing on their own, against the plan's claim: 0.2 (instrumentation with no consumer unless Tier 3 happens; it also misses writes made on scribble copies and through the surface slot, which are the hidden ones); 0.4 (a doc the rules refuse without maintainers, stale at the next field); 2.2, 2.3, 2.6-marks (pure refactors, no reported issue); 2.4 (no consumer before 4.2 except ray preparation); 2.1 (large diff in ported solvers for no result). Worth doing alone: 0.1; 0.3 (`BrepCacheSlot` has no constructor, [V]); 1.2 ([V] `K/topology/brep/mirror.rs:349`); 1.3; 1.4 if reproduced; 2.5-latch (flag retirement is welcome work); 2.5-scalars if a repro exists; the address comparison in `ray.rs`.

5.3 is unnecessary for the stated goal. **[V]** AGENTS 291-296: "The concurrency unit is the `Kernel` ... one `Kernel` per thread". **[I]** Readers on N threads each bring a kernel; what must hold is that the answer does not depend on which kernel (2.5 and the memo keys), not that `Kernel` be `Sync`. A per-task copy rule already exists (`cache_kernel`, report 03 §1.4).

### F10. Missing from the plan

- **ops layer.** [V] `Body` is a `Mutex<Held>` exclusive even for reads (`solidcore-ops/src/body.rs:186-207`); `read_body` gives `&Brep` but locks; `measure_body` copies. Parallel reads need an `RwLock` or `Arc` snapshot design here. No step covers it.
- **Second pool.** [V] `crates/smlib/src/core.rs:129` and `solidcore-ops/src/session.rs:66-68` each hold a pool and reset different fields. 5.3 mentions it last; it is the first place history shows.
- **Other completion loops** in `crates/smlib` (F2). 3.1 names only `to_record`.
- **Records and round trips.** [V] `to_kernel` runs with `make_uv_trim_curves: false, check_uv_trim_curves: false` (`solidcore-convert/src/brep.rs:280-283`). [I] A body from a record is "complete" only in that trims exist; gap table, versions and caches start empty, and a recorded trim failure has nowhere to go in the record.
- **Rollback and snapshots.** [V] Snapshots start with empty cache slots (`K/topology.rs:1328-1329`), and every ops edit takes one. [I] Caches "held for a body's life" are dropped or shared at every edit; which, and what it costs, is undecided. M6 measures memory but not this.
- **Error paths between take and put** during the transition: the lease has no RAII (report 01 §C4, not re-verified).
- **Staging.** No statement of how main avoids a state where slot and new face cache both answer. Layers §5 forbids it.
- **Test strategy for criterion 5.** Nothing proves order independence: no permutation runner, no "same call after N unrelated calls" check in the rig. Without it the criterion is asserted, not tested.
- **wasm.** [I] `OnceLock`, `Mutex`, `Arc` build there; larger keys and held caches matter more on a 4 GB target. Not mentioned.
- **In-flight PRs.** [V] the flag #20785 is said to retire is read at `K/query/brep/ray.rs:211` and is part of `FaceCacheKey` through `fixes`; it is the path that implants while editing. 3.2, 4.2 and 2.6 all edit that code. The plan says "do not collide" and gives no order.
- **Stale comment.** [V] `K/curve/curve/p5.rs:476` says `Curve` is "not `Sync`"; `solidcore/crates/solidcore/tests/curve_cache_memo.rs:183-193` asserts it is. The plan's "Curve is asserted" is true only in that integration test.

### F11. The acceptance criteria

- **1** is right but must say "between operations, on a prepared body". As written it is false inside operations (F1).
- **2** applies to what is shared. Requiring `Sync` of per-call caches is unnecessary.
- **3** bundles a requirement (immutable, complete key, no answer from history) with hygiene ("one home"). Several homes are harmless if each is keyed and pure.
- **4** is unnecessary under one kernel per thread; keep only "no answer depends on which kernel ran it".
- **5** is not met by the plan itself: the Boolean's order coupling stays (Tier 6). Restate as "a read's answer does not depend on other reads or on which thread or kernel made it; apply order is fixed".
- **Missing:** deterministic error selection when several gathers fail; cancellation ([V] report 03: `after_polls` stops "at a point fixed by its own work", not re-verified); panics with shared bodies (poisoning, `with_rollback` and `catch_unwind` assume one owner, [V] `session.rs:709-722`); lazily issued identity on first use from a read; id and handle numbering in apply order only; order-free reductions (sums over faces); a bound on cache memory; the serial wasm run giving the same bits.

---

## Claim-check table (item 1)

| # | Claim | Result | Evidence |
|---|---|---|---|
| a1 | No query refines a surface cache tree after build | **Confirmed** | [V] all callers of `subdivide_*`, `implant_*`, `build_tree*` are in `K/surface/surface_cache/p2.rs,p3.rs`, `K/surface/trim_srf_cache.rs`, `K/query/tess.rs:490-707`, `K/query/face/p2.rs:673,1055`, `K/surface/surface_cache/face_context.rs:395`, tests |
| a2 | Solvers do not write marks into tree nodes | **Confirmed** for marks and classes | [V] `set_marked`, `propagate_to_parents`, `mark_face_containment*` only at `surface_cache/p1.rs:2843-2977`, `trim_srf_cache.rs:912,966`, `query/tess.rs:707` |
| a3 | (implied) solvers never build or implant lazily | **Wrong** | [V] `surface_cache/p3.rs:1226-1256`; `trim_srf_cache.rs:736-755`; deferral at `K/surface/surface/p1.rs:1045`; callers `K/blend/hcr.rs:627,689`, `K/query/brep/ray.rs:244`, `K/boolean/topology_intersector/p1.rs:4613` |
| b | All real body writes from reads go through eight leaves | **Partly** | [V] bypasses: `K/query/face/p2.rs:1570,1576,1581` (the report's "ninth"); `K/topology/edgeuse/p3.rs:2450,2452` (tolerances put back by direct write, in the stitch's cover, an edit). Direct writes in edit code that no leaf sees, relevant to versions: `K/topology/brep/validation/wide_vertices.rs:106,121,228,231` (`centre_vertex` and its restore). Through `&Brep`: surface slot `Arc<Mutex>`, `MadeKindSlot`, three surface memos (`K/surface/surface/p2.rs:427-468`), `CrvOnSurf` mutexes (`K/curve/crv_on_surf.rs:327-343`), curve memo (`K/curve/curve/p5.rs:478`), atomics `K/nurbs/b_spline_surface/p1.rs:430,437`, `K/surface/offset_surface.rs:199,211` |
| c | `doing_boolean` has one behavioural reader | **Partly** | [V] one branch on it: `K/surface/surface/p1.rs:1045`. Two key readers that decide reuse or rebuild: `K/curve/curve/p5.rs:419`, `K/query/brep/ray/live.rs:135`. Copied into task kernels (`K/geom/context.rs:541`). Pure save, set, restore: `offset_executive/p2.rs:595` (restores on error only, `:775`), `offset_profile.rs:58-70`, `p2/skin.rs:278-302`, `face/p1.rs:2681-2698`, `brep/cutting.rs:594-607`, `brep/p7.rs:435-451`, `session/surface.rs:482-485`, `fillet_executive/p2.rs:1093-1096`, `p3.rs:4559-4589`; also `edge.rs:2783` (on an external context) and `sweep/topology_sweep/engine.rs:842-860` (two kernels) |
| d | Latch bypassed when its fix is on; SolidCraft runs all fixes | **Confirmed** | [V] `K/blend/fillet_solver/p4.rs:4037-4041`; `K/kernel.rs:1119-1128`; `crates/smlib/src/core.rs` `fixes()` returns `UpstreamFixes::all()`. The app pool resets the latch per call; the ops pool does not (report 03, not re-verified) |
| e | `to_record` walks edge uses in fixed order and creates each missing trim | **Confirmed, with limits** | [V] `solidcore-convert/src/brep/record.rs:130-136`: one pass, per-edge-use routine, aborts on first failure, no fixpoint, no corner fix. Two more loops in `crates/smlib` (`core/convert.rs:801`, `core/trim.rs:140`) |
| f | Implanting trims subdivides the base tree | **Confirmed, and deep** | [V] `K/surface/trim_srf_cache.rs:838-839,1249-1275,1300-1320`; planes exempt. See F8 |

Also checked: `BrepCacheSlot` never constructed ([V], no `BrepCacheSlot {` in any crate); `mirror.rs:349` standing copy ([V]); ids never reused ([V] `K/topology.rs:226-229`); body address compared ([V] `K/query/brep/ray.rs:70-76`).

---

## The plan I would write instead

Goal: a body held between operations can be read through `&Brep` by any number of readers, each with its own kernel, with the same answer. Inside operations nothing changes yet.

1. **Gate.** Rig sequences on the ops layer (v2 step 0, M5): same read after different histories; same call on a fresh and a used pooled kernel. File what differs. No repro, no further step.
2. **One completion job.** Move the three trim loops (`to_record`, `flat_from_brep`, `smlib trim.rs`) into one kernel function; callers switch in the same PR. Report as a layers duplicate. Results unchanged by construction; show it.
3. **Complete at the ops waists** (`finish*`, `Body::new`, tail of `with_body`), with that function, not in kernel operations. Run M2 here first: which rig cases move, each with its enclosure argument. If bodies fail to complete, they already fail `to_record`; say so per case.
4. **Prepared reads.** Extend what exists (`QueryTopology::Prepared`, `PreparedFaceQueries`, `point_classify_prepared`, the read-only evaluators) to the 11 `measure_body` callers and the `read_body` callers, one family per PR. A missing trim is `PreparationRequired`, never a write. These paths build caches per call or take them from step 5; they never touch the surface slot.
5. **One keyed home for prepared face caches, at the ops body**, using the existing `FaceCacheKey` widened to every input in F3, dropped by the same guard that drops the `Outward` proof on any edit. No kernel versions, no private fields.
6. **Kernel-independence only:** retire the fillet latch flag; fix the context scalars and the two RNG draws if step 1 reproduces them; unify the two pools' resets.
7. **Stand-alone defects:** 1.2, 1.3, 1.4, the address comparison, dead `BrepCacheSlot`.
8. **Assertions:** `Sync` for `Brep`, `Surface`, and the prepared cache; a permutation test in the rig for prepared reads.

Size: 25 to 40 PRs. Reaches criteria 1 to 3 for app-facing reads and the history half of 4 and 5.

Gives up: parallel work inside one operation (Boolean face pairs, fillet stations), the `&Brep` flip of the 207 internal functions, one home for caches inside the kernel, the `Kernel` split. Each of those needs the operation restructured into gather and apply, which is an algorithm plan per operation with its own issues, and can start from the prepared-read path this plan leaves behind.
