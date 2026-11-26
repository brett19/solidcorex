# solidcore caches: plan, version 4

2026-11-22, on `origin/main` 782ef9afc. Replaces plan v3, which its adversarial review
(`plan-v3-review.md`) sent back as "rethink". Built from that review, the Boolean study
(`07-boolean-gather-apply.md`) and four measurement runs on server2 (logs `m4-*`, `m5-*`, `m1-*`).

Private note: the end goal is correctness first, then performance and maintainability, and a kernel
that work inside one operation can later be spread over threads. No threading is added here and none
is named in any issue, PR or commit.

## 1. What the measurements settled

| Question | Answer | Evidence |
|---|---|---|
| What is `Sync` today? | Everything shared is: `Brep` and its records, `Surface`, `Curve`, all three cache types, `Tree`. Only `Kernel` (and its `Caches`, `Context`, `WorkCounters`) and `BrepQuery` are not. | compiler, `m4-sync-probe.log` |
| Does an answer depend on kernel history? | Yes, four ways (below). | `m5-history-kernel.log` |
| Does the trimmed cache's history change a classification? | Not in 45 comparisons over 3 bodies and 14 histories. | same |
| Do reads change the body? | Yes: volume and area create 5 to 6 UV trims; a full check fills up to 153 gap fields. No tolerance moved and no later answer changed on these bodies. | same |
| Does the ops layer show any of it? | No: pool battery, check-then-measure, record-then-measure all identical. | `m5-history-ops.log` |
| Are caches rebuilt wastefully? | Planes and cylinders use no surface cache at all. Curved Booleans build one tree per face (fine) but make 8 whole-cache copies per face pair. One edge fillet makes 260 whole-cache copies for 2 trees built. A repeated full check rebuilds its trees each time. | `m1-cache-counts*.log` |

Reproduced history dependence:
1. **Random draws.** The validity check of one untouched body answered valid 13 times and invalid 3
   times in 16 calls; the inner-shell test `TRUE` 30 and `UNSURE` 10 in 40; an untouched body's first
   answer flips after two sampling calls on another body.
2. **Stale gap memos.** After `edge::set_interval`, two cached gaps answer 0.0 where a fresh computation
   gives 4.0.
3. **Context scalars.** After scaling one body, the model size estimate reported for another, earlier
   body changes from 50 to 17,320.
4. **Fillet step latch.** One history leaves another step latched, and the same fillet differs in one
   value of 433 by one unit in the last place.

Not established: any wrong answer from the trimmed cache's four homes; any quadratic from cache
rebuilds (#14453 is planar geometry, which uses no surface cache).

## 2. What changes from v3

Dropped, with the reason:
- **Kernel split into config / work / apply.** One kernel per thread is already the kernel's stated
  unit; the kernel never needs to be `Sync`. What must hold is that an answer does not depend on which
  kernel computed it, which is section 3.
- **Face versions with private fields.** Unsound as specified (what a face cache reads lives on edges
  and vertices; rollback restores counters). Not needed inside a phase (section 5).
- **Every kernel operation returns a completed body.** Not definable today (trims fail by design, heal
  forbids them, stitch returns trimless faces), and it does nothing for reads inside operations.
- **One home for every cache, kernel-wide, as a refactor.** No wrong answer reproduced; goes ahead only
  where a count or a defect supports it.
- **The read-purity guard and the classification doc** as standalone steps.

Kept: the reproduced defects, `Sync` assertions, dead-code removal, and the cache work that has a count.

New: an explicit second stage for work inside an operation, starting with the Boolean, using a
phase-scoped cache table that needs no key and no version.

## 3. Stage A: answers that do not depend on history (each is a reportable defect)

| # | Step | Evidence in hand | Result change | Size |
|---|---|---|---|---|
| A1 | Random draws become a function of the call: each call makes its own generator from the kernel's seed. Then remove the kernel's generator, its save and restore in the Boolean's rollback, and the per-call resets in both pools. | validity flips 3 of 16 | yes: goldens move where a draw was not first on its kernel | S to M |
| A2 | Gap memos are reset by `set_interval` and by every edit that changes what they measure (one memo is reset by nothing today). | 0.0 against 4.0 | yes | S |
| A3 | The model size estimate and its two companions belong to the body; a body's answer never reads another body's. | 50 against 17,320 | yes, where a scale preceded | M |
| A4 | Retire the `fillet_point_step_per_point` flag to its on state and delete the latch and the process-wide cell. The app already runs flag-on. | 1 unit in the last place | flag-off goldens | S to M |
| A5 | Unify the two pools' per-call resets (`solidcore-ops` resets two things, `crates/smlib` four); after A1, A3, A4 there is little left to reset. | code reading | no | S |
| A6 | Compile-time `Send + Sync` assertions for the 19 types that pass today. | compiler | no | S |
| A7 | Remove `BrepCacheSlot` (never constructed) and the Boolean's no-op `delete_stale_caches`. | code reading | no | S |
| A8 | One kernel routine for "create every missing UV trim", replacing the identical loops in `solidcore-convert` (`to_record`) and `crates/smlib` (`flat_from_brep`). | read side by side | no | S |

A2 needs one more thing before filing: a real operation that reaches the stale memo (the probe called
the setter directly). A1, A3, A4 are ready to file with the probe output as reproduction.

Basis for reporting, with no mention of threading: the app's own stated requirement that the same
operation on the same bodies answers the same on any kernel, however much that kernel did before
(`crates/smlib/src/core.rs:155-166`), and replay of a failed call from its inputs alone.

## 4. Stage B: cache work with a count behind it

| # | Step | Count today | Target | Result change |
|---|---|---|---|---|
| B1 | Add two kernel work counters: surface cache trees built, whole-cache copies. | n/a | n/a | no |
| B2 | Solve modes (`point_test_enabled`, `process_boundary_curves`) become solve arguments, removing the clone made to flip one flag. | 2 clones per face pair | 0 | no |
| B3 | Consumers borrow a built cache through a handle view; no `map_handles` deep copy per fetch. | 6 copies per face pair; 260 per edge fillet | 0 | no |
| B4 | The validity check keeps the caches it builds for the length of the check. | trees rebuilt on each repeat | built once | no |

B3 is the large one (the tree's nodes carry the consumer's handle type; about 65 readers) and should
start with the fillet, where the count is worst. Each step is shown by the counter, not by a timing.
Unknown until measured on real designs: how much time these copies cost. If a profile of a slow fillet
shows the copies are small, B3 drops to the Boolean pair phase only (stage C needs it there).

## 5. Stage C: one operation at a time, prepare then read then apply

Start with the Boolean (study: 18 to 29 PRs, three with result changes). The mechanism, each part with
existing code close to it:

1. **A prepared view over `&Brep`** (`QueryTopology::Prepared`, `point_classify_prepared`).
2. **A phase table, face to built cache or build failure, with no key and no version.** Every build
   input is constant for the phase; "built in this phase" is the validity rule, enforced by holding
   `&Brep`. The apply step drops the entries around what it wrote.
3. **Typed requests returned in the item's value** (`NeedFace`, `NeedTrim`, `RaiseEdge`, `RaiseVertex`,
   `RepairEdge`), generalising `PreparationRequired` and the existing `EdgeToleranceRaise` value.
4. **The loop:** prepare candidate faces, read every item, fulfil requests in item order taking the
   largest per entity, re-run the affected items. `with_face_caches` already is this loop for the
   Boolean's topology solves; the pair phase does not use it.
5. **A kernel per worker** for counters and memos (`cache_kernel`).

Order inside the Boolean:
- C1 the pair phase reads through the phase table (uses B2, B3);
- C2 the seven preparable writes move to a prepare step over the broad-phase candidates only;
- C3 the three order-coupled writes become requests (result change where they fire: not equal to
  today's loop-order result, so each moved case needs its proof);
- C4 the two cache homes that give one face different caches within a pair become one (result change);
- C5 classification phases the same way.

Stays sequential regardless: trim creation, all raises, proto topology, imprint, marks, merge.

Then carry parts 1 to 4 to the app-facing reads (the `measure_body` callers stop copying the body), to
mass properties per face, point classification batches and the validity check per entity. About a third
of the Boolean's work is shared machinery.

## 6. Order

```
A1 A2 A3 A4 A6 A7 A8        independent, start now
A5                          after A1, A3, A4
B1 -> B2 -> B3 -> B4        B3 fillet first
B2, B3 -> C1 -> C2 -> C3 -> C4 -> C5
C (Boolean) -> the same mechanism on reads, mass, classification, validity check
```

## 7. Still unknown

- A real operation that reaches the stale gap memo (A2).
- Time cost of the cache copies on real designs (B3's priority).
- Whether preparing the candidate faces before the pair loop gives today's body state: it does only in
  first-intersection order and only if the face-level trim routine's second pass is idempotent (C2).
- How many goldens move at C3 and C4.
- The probes used three or four simple bodies; "did not reproduce" is not "cannot happen". Two suspected
  cache sequences have no public entry point and were not probed.
- Order relative to the open PRs that touch the same code: ours #20694, and another contributor's draft
  #20785.
