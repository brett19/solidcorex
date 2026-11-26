# 03 — Kernel ambient state and existing threading

Read-only investigation of a local worktree at `782ef9afc`. Nothing was built or run; every claim comes from reading source.

**Path shorthand.** `K/` = `solidcore/crates/solidcore/src/`, `OPS/` = `solidcore/crates/solidcore-ops/src/`, `TESS/` = `solidcore/crates/solidcore-tess/src/`. App-side files (`crates/…`, `.agents/…`) are outside the worktree's sparse checkout and were read with `git show HEAD:<path>`.

**How the counts were made.** "Library code" = `K/**/*.rs` minus `tests.rs`, `*_tests.rs`, `tests/` directories and everything after a column-0 `mod tests {`. Function counts come from a throwaway brace-matching script, so they are good to a few percent, not exact. Anything inferred and not confirmed in code is marked **UNVERIFIED**.

## Headline findings

1. **89% of `&mut Kernel` is pass-through.** Of about 1,232 library functions that take `&mut Kernel`, about 134 touch kernel-mutating state themselves. The real causes are few: the notify context and identities (≈90 functions), mark locks (≈32), `doing_boolean` (≈21), the two record-map scratch fields (6), the RNG (2 real draw functions), the context size scalars (3).
2. **`Kernel` is `!Sync` for two reasons only:** `Cell`/`RefCell` fields written through `&Kernel` (memos, counters, the fillet latch, the blend net scope), and `Context`'s two `Box<dyn NotifyCallback>` (the trait is `Send`, not `Sync`).
3. **Result-affecting history is small but real:** the RNG position, the fillet step latch, and — not on the brief's list — the `Context` size scalars (`model_size_estimate`, `large_small_size_ratio`, `zone_tol_3d`), which `new_brep`, heal and a scaling transform overwrite and later calls read. The ops pool's per-call reset does not reset them.
4. **`doing_boolean` has one result-relevant reader** (`K/surface/surface/p1.rs:1045`, eager or lazy trim-cache tree); the other ≈250 references are save/set/restore and tests. It is, however, part of two cache keys.
5. **`blend_net_tolerance` is read on every procedural blend and sweep evaluation** and is in no cache key I found.
6. **No kernel `Brep` is shared across threads anywhere today.** The mesher and hidden lines parallelise over plain record data with no `Kernel`; the two kernel pools hand one kernel to one call and lock each body with a `Mutex` (exclusive even for reads).
7. **The geometry and body value types look `Sync` already.** `Curve` is asserted `Send + Sync`, which forces `Surface: Sync`; `Brep`, the caches and `Tree` have no `Cell`/`Rc`/raw pointer fields. The blockers are the `Kernel`, the transient query wrappers (`OnceCell`/`RefCell`) and the `&dyn Fn` identity environments.
8. **A per-task kernel copy already exists:** `cache_kernel` (`K/boolean/merge/p4/kernel_adapter.rs:60-108`) decides field by field what a copy shares, copies or starts empty. Four other sites build ad hoc kernels with a different zone tolerance and silently drop the context, latch and memos.

---

## 1. Kernel field table

Classes: **CONFIG** (immutable for a call), **WORK** (per-thread work state), **MEMO**, **SCOPE** (dynamic scope), **HISTORY** (an earlier call changes a later answer).

Counts over the 35 leaf fields (24 of `Kernel`, 2 of `Caches`, 9 of `Context`): **CONFIG 4, WORK 10, MEMO 5, SCOPE 7, HISTORY 7, callbacks 2** (configuration that carries `&mut` state; apply-phase only).

### 1.1 `Kernel` (`K/kernel.rs:809-952`)

| Field (line) | Class | Mutated how, by whom | Multi-threaded design |
|---|---|---|---|
| `tolerances` (811) | CONFIG | Never assigned in library code on a live kernel. Four sites build a temporary kernel with another `zone_tol_3d` (§1.4). | Share. Pass the owner zone tolerance explicitly instead of minting kernels. |
| `rng` (813) | **HISTORY** | `&mut`: `K/topology/shell.rs:1714-1731`, `K/topology/face/p2.rs:1654-1659`. Saved and restored at `K/boolean/api_brep.rs:532,545`. Cloned into copies (`kernel_adapter.rs:63`, §1.4). | Remove, or derive per-task streams (§3). Never share. |
| `caches` (815) | see 1.2 | | |
| `context` (818) | see 1.3 | | |
| `fillet_point_step` (827) | **HISTORY** (latch) | `Cell` through `&Kernel`: `latch_fillet_point_step` (`kernel.rs:1119-1128`), called at `K/blend/fillet_solver/p4.rs:4040`. | Remove. Already bypassed when `fixes.fillet_point_step_per_point` is on (`p4.rs:4037`); SolidCraft runs all fixes on (`crates/smlib/src/core.rs:76-78`). Retiring the flag deletes the latch. |
| `process_fillet_point_step` (837) | **HISTORY** (process-wide latch) | `&'static OnceLock<f64>`, set by the ops pool (`OPS/session.rs:66,212`). First latch in the process wins. | Remove with the latch. It is `Sync` but makes results depend on which call ran first. |
| `upstream_ub_passed` (843) | WORK (diagnostic list) | `&mut`: `note_upstream_ub` at `K/boolean/topology_intersector/p2.rs:1475,1480,1499`; restored at `api_brep.rs:546`. "no operation reads it" (`kernel.rs:842`). | Per-thread list, merged in task order with dedup. |
| `memoize_curve_caches` (851) | CONFIG | Set at construction. | Share. |
| `fixes` (853) | CONFIG | Set by pools before a call (`OPS/session.rs:513`). No library assignment on a live kernel. | Share. Shrinks as flags retire. |
| `cancel` (861) | WORK (polling) | `Option<CancelToken>`; token is `Arc<AtomicBool>` and is asserted `Send + Sync` (`kernel.rs:1160`). Polled at ≈270 library sites. | Share the token. An `after_polls` token (`kernel.rs:738`) stops "at a point fixed by its own work"; with workers the poll order is not fixed, so that guarantee needs per-task counts or goes. |
| `notify_records` (866) | WORK (allocation reuse) | `&mut`: `mem::take` and put back in `kernel_named_edit` (`K/topology/brep/p3.rs:1490,1528`). Reset every edit. | Apply thread only; move into an edit/apply context. |
| `blend_net_tolerance` (874) | **SCOPE** | `Cell` through `&Kernel`: `NetScope` (`K/surface/procedural_blend.rs:155-239`), entered at ≈16 sites in 11 files (surface cache build, point solve, fillet session, …). Read by every procedural evaluation: `procedural_blend.rs:1165`, `procedural_sweep.rs:563`, `K/surface/surface/p2.rs:2233`. | Pass explicitly as an evaluation parameter. A per-thread copy must inherit the forker's value (as `cache_kernel` does, `kernel_adapter.rs:76`). |
| `blend_counts` (877) | WORK (counter) | `Cell` get/set, `procedural_blend.rs:387-425`. No result reads it. | Per-thread, summed. |
| `blend_station_memo` (883) | MEMO, key suspect | `RefCell` through `&Kernel`, `K/blend/fillet_section/definition.rs:498,608,659`. 4-slot ring keyed by definition `Arc` identity, `s`, `ts`, `te`, side (`definition.rs:503`). | Per-thread. Both pools clear it per call "so its result does not depend on the calls before it" (`OPS/session.rs:230-241`; `crates/smlib/src/core.rs:155-166`), so its owners do not trust it as a pure memo. Why a hit could differ from a fresh solve is **UNVERIFIED**; resolve before sharing it. |
| `arena_notify_records` (890) | WORK (allocation reuse) | `&mut`: `K/heal/stitch/p1.rs:2704,2721,2781`, `K/primitive/primitive_creation/model.rs:261`, `…/p1/primitives.rs:152`. | Apply thread only. |
| `blend_w_memo` (899) | MEMO, key complete | `RefCell`; key `(order, u.to_bits())` (`K/blend/fillet_section/section.rs:419-432`). Emptied when the outermost `OperationScope` opens (`section.rs:399-405`). | Per-thread, or one concurrent map; the solve is pure in the key. |
| `loop_area_memo` (904) | MEMO, key complete | `RefCell`, `K/mass/loop_area.rs:276-280`. Key = every bit of the loop's curves; "the kernel is read only for its cancel token" (`loop_area.rs:217-225`). `HashMap`, lookup only. | Share behind a lock or per-thread; best kept across calls. |
| `recertify_memo` (910) | MEMO, key complete | `RefCell`, `K/surface/procedural_blend/convert.rs:845-850`. Key = blend record text + tolerances + fixes (`convert.rs:781-789`). | As `loop_area_memo`. |
| `blend_operation_scopes` (918) | **SCOPE** (depth counter) | `Arc<AtomicU32>`; `OperationScope::new` at `K/blend/fillet_executive/p2.rs:1718`, `…/session.rs:1716`. Its only effect is clearing `blend_w_memo`. | Replace with an explicit per-operation context that owns the memo. |
| `work_counters` (922) | WORK | 22 `Cell<u64>` (`kernel.rs:182-205`), added at ≈31 sites in 16 files. Tests assert on them. | Per-thread, summed at join (sums are order-free). |
| `blend_face_log` (927) | WORK (diagnostic output) | `&mut`, `K/blend/fillet_executive/session/blend_face_log.rs:90-217`. Appended per finished session. | Apply thread; order = apply order. |
| `coons_tangency_log` (934) | WORK (test diagnostic) | `RefCell`, `K/nurbs/b_spline_surface_n_lib/p2.rs:3419,3461`. | Per-thread, concatenated in task order, or dropped. |
| `profiler` (941, feature) | WORK | `Arc` over `Mutex<State>` + atomics (`K/profile.rs:275-280`); copies share one tree. | Already shareable; scope nesting assumes one thread, so use per-thread trees merged at join. |
| `strip_probe` (945, test) | WORK (test seam) | `Arc<Mutex<_>>`. | Unchanged. |
| `replay` (951, feature) | CONFIG (test hook) | `Arc<Mutex<Box<dyn FnMut + Send>>>` (`K/surface/surface/replay.rs:64-69`). | Call order becomes thread-dependent; keep the goldens' replay serial. |

### 1.2 `Caches` (`K/kernel.rs:137-150`)

| Field | Class | Mutated how | Design |
|---|---|---|---|
| `curve_analysis` (142) | MEMO, key complete | `RefCell` through `&Kernel`, `K/nurbs/b_spline_curve/p3.rs:340-349`. Key = curve content bit for bit + range + property + caller-encoded tolerances and fixes (`K/nurbs/b_spline_curve/analysis.rs:69-103`). FIFO, 4,096 entries. | Share behind a lock or per-thread. FIFO eviction is order-dependent but only affects speed. |
| `prove_lazy_trim_raise` (149) | **SCOPE** | `&mut`: `with_sampled_trim_raise` (`kernel.rs:1046-1052`), file import only. Read at `K/topology/edgeuse/p3.rs:1835`. Result-affecting: chooses proved or sampled trim raise. | Pass explicitly as an import option. The doc says the path is to be removed. |

`Caches::clear` (`kernel.rs:164`) keeps `prove_lazy_trim_raise`, in line with "Clearing must never change results, only speed" (`kernel.rs:134`). One library caller: `K/boolean/merge/p4/arena_pair.rs:311`.

### 1.3 `Context` (`K/geom/context.rs:208-233`)

| Field (line) | Class | Mutated how, by whom | Design |
|---|---|---|---|
| `current_marks: [AtomicU64; 5]` (214) | **SCOPE** (traversal epoch) | `fetch_add(2)` through `&Context` (`context.rs:777-784`). Via `MarkLock::new(&mut [Context])` at ≈34 library sites in 23 files, and `claim_mark(&self)` at `K/query/face/p2.rs:2567`, `K/intersect/curve_class/p9.rs:579`. | One mark epoch per traversal, owned by the traversal with its side table. Never shared. |
| `mark_locks: [AtomicBool; 5]` (217) | **SCOPE** | load-then-store, not compare-and-swap (`context.rs:797-815`, `unlocked_mark` 861-880). Two threads on one `Context` can claim the same mark. | Goes with the marks. |
| `doing_boolean: bool` (219) | **SCOPE** | `&mut`: ≈59 library lines in 22 files, all set-and-restore (e.g. `K/topology/edge.rs:2614-2637`, `K/boolean/topology_intersector/p1.rs:2446-2451`, `K/boolean/merge/p4.rs:165-167`, `K/query/brep/ray.rs:558`). | Pass explicitly to the one reader, or delete (§4.1). |
| `model_size_estimate` (221) | **HISTORY** | `&mut`: `Brep::set_this_model_size_estimate` writes the kernel's context (`K/topology/brep/p1.rs:231-240`). Callers: `new_brep_in` (`K/topology/brep/p3.rs:664`), heal (`K/heal/heal_data/p2.rs:3065-3067`), scaling transform (`K/topology/brep/p5.rs:3707-3708`, sets it to the scaled body's box size). | Make per-body data. The value a body should use is the body's, not the last one any body wrote. |
| `large_small_size_ratio` (223) | **HISTORY** | `&mut`: `K/topology/brep/p1.rs:244-248`, from `brep/p3.rs:665`. | As above. |
| `zone_tol_3d` (225) | **HISTORY** | Derived from `model_size_estimate` in the same setter (`context.rs:723-727`). Read by `context_zone_tol_3d` (`K/geom/tol.rs:388-395`), which sets each new B-rep's tolerance (`brep/p3.rs:666`). | As above. |
| `user_notify_callback` (227) | callback | `Box<dyn NotifyCallback>`, `NotifyCallback: Debug + Send` (`context.rs:166`); `execute(&mut self)`. Moved by lease (`context.rs:638-691`). | Apply thread only. This is the `!Sync` field of `Context`. |
| `sys_notify_callback` (229) | callback | Same. | Same. |
| `identities` (232) | **HISTORY** (issue order) + SCOPE (`lent`) | `token: Arc<DomainToken>`, `geometry_issued: Arc<AtomicU64>`, `breps_issued: u64`, `lent: bool` (`context.rs:267-275`). Issued by `geometry_key(&mut self)` (405) and `brep_serial(&mut self)` (456). | Issue keys on the apply thread in deterministic order. Keys index `NotifyRecords`' `BTreeMap` (`K/topology/topology.rs:322-323`); whether any result depends on that order is **UNVERIFIED**. |

Evidence that the size scalars are history, not configuration:
- the setter writes the shared context: `k.context.set_this_model_size_estimate(model_size_estimate);` (`K/topology/brep/p1.rs:237`);
- a body that stores `USE_DEFAULT` reads the context's value back: `K/topology/brep/p1.rs:198-205`, `216-222`;
- the three scalars are in `CurveMemoKey` (`K/curve/curve/p5.rs:414-418`), `FaceCacheKey` (`K/query/brep/ray/live.rs:130-134`) and ops' proof `Settings` (`OPS/body.rs:84-94`), so their authors knew builds read them;
- neither pool resets them: `start_clean` resets the RNG and the station memo only (`OPS/session.rs:238-241`); the app pool also resets the latch and the UB list (`crates/smlib/src/core.rs:161-166`) and states "Tolerances and fixes are the same on every kernel" without mentioning the context.

Whether a real ops call sequence reaches a non-default context size (a scaled transform with tolerance scaling, then another call on the same pooled kernel) is **UNVERIFIED**.

### 1.4 Kernel copies that already exist

| Site | What is copied | What is dropped |
|---|---|---|
| `cache_kernel` (`K/boolean/merge/p4/kernel_adapter.rs:60-108`) | tolerances, `rng.clone()`, trim-raise mode, `context.metadata_copy()` (marks, locks, `doing_boolean`, size scalars; no callbacks, fresh identity domain; `context.rs:531-549`), latch value and process latch, fixes, cancel, net tolerance, blend counts, scope depth (fresh `Arc`), profiler, test seams | all memos start empty; counters zero; logs off; UB list empty |
| `trim_other_uv_curves` (`K/topology/edgeuse/p3.rs:1191-1200`) | tolerances with another `zone_tol_3d`, `rng`, cancel, one fix flag | context, latch, memos, net tolerance, all other fixes |
| `edge_zone_kernel` (`K/blend/fillet_solver/p2.rs:3151-3163`) | same | same |
| `K/blend/fillet_solver/p1.rs:3607-3619` | same | same |
| `K/blend/fillet_executive/p1.rs:1510-1518` | tolerances, `rng`, cancel | everything else, including all fixes |

The four trim kernels exist only because `Curve::trim` reads its zone tolerance from the kernel. They are a second, weaker copy rule beside `cache_kernel`.

A cloned RNG means a copy's draws repeat the parent's next draws and do not advance the parent.

---

## 2. Why `&mut Kernel`

Counts: 1,338 `: &mut Kernel` parameter lines in 135 library files and 5,024 `: &Kernel` lines in 432 files. Parsed as functions: ≈1,232 `&mut`, ≈4,909 `&`.

### 2.1 Root causes

| Cause | Functions that touch it directly | Why it needs `&mut` | Representative sites |
|---|---|---|---|
| Notify context and identities | ≈90 | `NotifyEnv.context: Option<&mut Context>` (`K/topology/edge.rs:2487-2503`); `execute(&mut self)`; `geometry_key`/`brep_serial(&mut self)`; lease swapping | `Context::with_notification_context` (`context.rs:498-509`, 36 call lines in 16 files); `K/sweep/topology_sweep/store.rs:950,981`; `K/offset/offset_executive/p1.rs:1938,2251,2521` |
| Mark locks | ≈32 | `MarkLock::new(&mut [Context], …)` (`context.rs:944`), `with_mark_lock(k: &mut Kernel, …)` (`context.rs:1120-1129`). The marks themselves are atomics; `claim_mark(&self)` already works through `&Kernel`. | `K/offset/offset_executive/p2.rs:803,965`; `K/boolean/merge/p3.rs:1115`; `K/heal/heal_data/p1.rs:463`; `K/heal/stitch/p1.rs:1356` |
| `doing_boolean` | ≈21 | plain `bool` | `K/topology/edge.rs:2614`; `K/offset/offset_executive/p2.rs:583,803,2902`; `K/heal/stitch/p2.rs:440` |
| Record-map scratch | 6 | `mem::take` of an owned field | `K/topology/brep/p3.rs:1480`; `K/heal/stitch/p1.rs:2671,2713,2759` |
| RNG | 2 draw functions, 1 save/restore | `next_u64(&mut self)` | `K/topology/shell.rs:1584`; `K/topology/face/p2.rs:1654`; `K/boolean/api_brep.rs:513` |
| Context size scalars | 3 | plain `f64` setters | `K/topology/brep/p1.rs:231,244`; `K/topology/brep/p5.rs:3568` |
| UB notes | 2 | `Vec::push` | `K/boolean/topology_intersector/p2.rs:1380` |
| Scopes and logs | ≈5 | `NetScope::within_mut` (`K/blend/fillet_executive/session/finish.rs:883`), `with_sampled_trim_raise`, `blend_face_log` | |

Cause counts overlap (a function can hit several) and the notify pattern also matches `&mut k.context` passed to `MarkLock`, so read the split between the first two rows as approximate.

**Pass-through share: ≈1,098 of ≈1,232 (89%).** Those functions take `&mut Kernel` only to hand it to a callee.

`&mut` is **not** needed for: tolerances, fixes, cancel polling, all five memos, counters, the fillet latch, the net scope, the profiler. All are read or written through `&Kernel` today.

### 2.2 By area

| Area | Direct | Pass-through | What it really mutates |
|---|---|---|---|
| topology | 66 | 355 | Euler operators and edits: notify context, identities, marks, `notify_records`; `new_brep` writes the context size; shell classification draws random numbers |
| blend | 24 | 319 | fillet session: notification lease, marks, `doing_boolean`, `blend_face_log`, `within_mut`. The latch, station memo and thumbweight memo go through `&Kernel`. |
| boolean | 5 | 99 | `doing_boolean` scope, mark pair, notify lease, UB notes, RNG save/restore in the glue. The public entry points (`K/boolean/api_brep.rs:669-699`) only forward. |
| primitive | 8 | 85 | `arena_notify_records`, `doing_boolean`, notify; `new_brep` |
| offset | 10 | 80 | `doing_boolean`, marks, notify |
| heal | 12 | 78 | marks, `doing_boolean`, `arena_notify_records`, model size |
| sweep | 3 | 57 | notify context in the sweep store |
| intersect | 0 | 11 | forwards only |
| query | 0 | 8 | forwards only (e.g. `face_closest_point`, `K/query/api_queries.rs:960`); queries otherwise use `&Kernel` plus `claim_mark` |
| mass | 0 | 0 | `&Kernel` only; writes `loop_area_memo` through a `RefCell` |

So intersection, classification, queries, mass properties and all geometry evaluation need no `&mut Kernel` of their own. Every direct cause except the two RNG draw functions belongs to topology editing (the "apply" side).

---

## 3. RNG

`Rng` is SplitMix64 (`K/kernel.rs:97-130`), seeded from `KernelOptions::seed`. Both pools reseed it to the default at the start of every call (`OPS/session.rs:239`; `crates/smlib/src/core.rs:162`), so within a call the stream position depends only on the draws made earlier in that call.

### 3.1 Draw sites (complete; grep of `.rng`, `next_f64`, `next_u64`, `next_signed_unit`)

| Site | Purpose | Reached from | History dependence |
|---|---|---|---|
| `K/topology/shell.rs:1714-1718` | 3 draws per sample: random ray direction from a wire shell's edge, in `is_inner_shell_of` | `K/topology/brep/enclosing.rs:88`, `brep/p3.rs:3966`, `brep/validation/assert_valid/brep.rs:154`, `…/seams.rs:86` | The directions used depend on how many draws came before. |
| `K/topology/shell.rs:1728-1732` | Same, for a vertex shell | same | same |
| `K/topology/face/p2.rs:1654-1659` (`rand_dither`), used at `p2.rs:1742,1834` | V dither of each sample point in `points_in_face` when more than one point is asked for | `K/topology/shell.rs:1683` (face-use shells in `is_inner_shell_of`), `K/topology/topology_traverser.rs:748` (`is_inner_shell`, via `shell.rs:312`, `brep/p6.rs:1766`, `brep/p7.rs:709`, `brep/p3.rs:3949`), `K/blend/feature_executive.rs:2002,2027` (one point: no draw) | The sample points of the n-th shell classified depend on how many shells were classified before it. |

Not draws, but they copy or restore the position: `kernel_adapter.rs:63`, `edgeuse/p3.rs:1198`, `fillet_solver/p2.rs:3159`, `fillet_solver/p1.rs:3617`, `fillet_executive/p1.rs:1517` (clones) and `api_brep.rs:532,545` (the all-or-nothing glue puts the position back on failure).

All draws serve one job: picking sample points and ray directions for inside/outside shell classification. The answer is a geometric fact, so it should be the same for any draw; the outcome can still differ where a ray is rejected and the function returns `SM_UNSURE` after its samples run out (`shell.rs:1567-1569`). How often that happens is **UNVERIFIED**.

### 3.2 What per-task deterministic streams would require

- A **stable task key** that is not a thread index: the shell or face id, or the operation-local ordinal of the classification. Child seed = a mix of (operation seed, task key); SplitMix64 was chosen as a splittable generator.
- The **draw functions take the stream explicitly** (`&mut Rng`), not `&mut Kernel`. Only two functions and their callers change.
- The **serial results change**, because today's stream is one sequence in call order. Goldens that include these sample points must be blessed with a justification (§7).
- The save/restore in `glue_all_or_none` disappears: with keyed streams a failed attempt consumes nothing shared.
- The rule "randomness only from `kernel.rng`" (`solidcore/AGENTS.md:102`) needs rewording.

The simpler option is to **remove the randomness**: both uses want "varying" directions and dither, which a fixed low-discrepancy sequence indexed by sample number gives with no state. That also removes the only `&mut Kernel` cause outside topology editing.

---

## 4. Context flags, marks and notifications

### 4.1 `doing_boolean`

- **Writers:** ≈59 library lines in 22 files, always "read previous, set true, run, restore": `K/topology/edge.rs:2614-2637` and `2786-2797`, `K/boolean/topology_intersector/p1.rs:2446-2451`, `K/boolean/merge/p4.rs:165-167`, `K/offset/offset_executive/p2.rs` (9 lines), `K/sweep/topology_sweep/engine.rs` and `store.rs`, `K/primitive/primitive_creation/p1/primitives.rs`, `p3/sweeps.rs`, `K/heal/stitch/p2.rs`, `K/blend/fillet_executive/p2.rs`, `p3.rs`, `session/surface.rs`, `session/insertion/live.rs`, `K/query/brep/ray.rs:558`.
- **Readers that change behaviour:** one. `trim_srf_cache_tree` (`K/surface/surface/p1.rs:1038-1052`): while the flag is set, a new trimmed-surface cache does not build its tree; the first `get_tree` builds it.
- **Readers that capture it in a key:** `CurveMemoKey::of_kernel` (`K/curve/curve/p5.rs:419`) and `FaceCacheKey::of` (`K/query/brep/ray/live.rs:135`).
- **What answer changes:** by the code's own statement, only when the tree is built ("Nothing else decides it", `p1.rs:1034-1035`). Whether an eagerly built and a lazily built tree are always identical is **UNVERIFIED**. Because the flag is in two cache keys, a cache built inside a boolean does not serve the same build outside one.

A `bool` on shared state that workers would read while one thread sets and restores it cannot survive fork-join. It should become an explicit "defer the tree" argument at the one reader, or go, since a lazy tree is needed anyway.

### 4.2 Marks

- **Where the mark values live:** not in `Brep` entities. `Topology.marks: [u64; 5]` (`K/topology/topology.rs:144-152`) lives in side tables owned by the caller: `topology_traverser::Marks` (one table per traversal, `K/topology/topology_traverser.rs:76-150`) or a `NotifyRecords` map keyed by notification identity (`K/topology/brep/p3.rs:457-482`).
- **Where the epoch lives:** `Context.current_marks`. An entity is marked when its stored value equals the context's current value (`topology.rs:303-305`).
- **Per-operation scratch:** yes. A traversal claims one of five marks, bumps its epoch, marks as it visits, and releases the lock.
- **Would concurrent readers collide:** yes, if they share one `Context`. Two traversals can pick the same mark (the lock is load-then-store), and a second `new_mark` on the same slot changes the epoch under the first traversal, so its earlier marks stop comparing equal. With one `Context` per worker and one side table per traversal there is no collision.
- **Result-affecting:** no evidence that the epoch's absolute value matters (only equality is tested); the app pool's comment agrees ("marks, identities … change no answer", `crates/smlib/src/core.rs:157`).

### 4.3 Notifications

- **Mechanism:** `topology::notify` (`K/topology/topology.rs:764-777`) takes `&mut` records, `&mut AttributeStore` and `Option<&mut Context>`; it propagates marks and flags between records, then runs attribute handlers and the context's user and system callbacks (`K/geom/object.rs:216-236`). Callback status is ignored.
- **What triggers it:** topology edits only (`PreEdit`, `PostEdit`, `Split`, `Merge`, `Copy`, `SplitInBrep`, `MergeInBrep`, `Coincident`, `Construction`), through a `NotifyEnv` built by the editing operation.
- **Can a read trigger a callback:** no. Calling one needs `&mut Context`, which exists only under `&mut Kernel` or as an explicit parameter, and `Context` has no interior mutability for callbacks. The lazy work reads do (`get_or_create_uv_trim_curve(k: &Kernel, b: &mut Brep, …)`, `K/topology/edgeuse/p3.rs:1690-1697`) has no notify environment.
- **Reads that mutate the body:** that same function takes `&mut Brep`, and `points_in_face` documents that its classifications leave UV trim curves, tolerance growth and gap caches on the B-rep (`K/topology/face/p2.rs:136-139`). This is the body-side blocker for "workers share one immutable body"; it is outside this report's scope but it is what makes ops copy a body to measure it (§5a).
- **Callbacks are `Send`, not `Sync`,** and hold `&mut self` state (trackers: `TopologyChangeCallback`, `Arc<Mutex<ChangeState>>`, `topology.rs:960-966`). They belong on the apply thread.
- **Leases:** an edit moves the callbacks and identity domain to a stand-in context and marks the kernel's as `lent` (`context.rs:498-528, 686-691`); a nested edit is refused. This is dynamic scope on the kernel that a worker must never see half-moved.

### 4.4 An existing gather-then-apply seam

The ray and face-cache paths already split "ask" from "mutate": a read-only solve records what it needs in a `Cell` (`RayPreparation.pending`, `K/query/brep/ray.rs:53-58`; `FaceCaches.pending`, `K/boolean/topology_intersector/p1.rs:4196-4211`) and the mutable caller builds it "between attempts", then re-runs the solve. It is a retry loop on one thread, but the ownership split is the one fork-join needs.

---

## 5. Existing threading

### 5a. `solidcore-ops` session pool (`OPS/session.rs`)

| Question | Answer |
|---|---|
| Process-shared state | `IDLE: Mutex<Vec<Kernel>>` (61), `STEP: OnceLock<f64>` (66, the fillet latch), `FIXES: Mutex<Option<UpstreamFixes>>` (67), `HOOK: Mutex<Option<Hook>>` (88), `MOST: OnceLock<usize>` (219), `TAKEN` in `OPS/provenance.rs:85` |
| Per-thread state | `THREAD_FIXES`, `IN_CALL`, `IN_HOOK`, `STOPPED`, `DETAIL`, `CURRENT`, `CODE`, `NOTED` (`session.rs:259-279`); the cancel token `CURRENT` (`OPS/cancel.rs:25-27`) |
| Per kernel | Everything in the `Kernel`. One call takes one kernel (`take`, 223-226) and gives it back (`give_back`, 245-257). The pool lock is held only for the pop and push. |
| Per-call reset | `start_clean` (238-241): RNG to the default seed, station memo emptied. Then `k.fixes` and `k.cancel` are set (513-514). Memos, counters, the latch and the context scalars carry over. |
| After a panic or cancel | The kernel is dropped and a fresh one goes back to the pool (560-562). |
| How bodies are stored | `Body(Mutex<Held>)`, `Held { brep, outward, primitive }` (`OPS/body.rs:37-51`). The application owns each `Body`. Ops keeps no body cache and no "cached body per design object"; the proof of validity is cached beside the B-rep and dropped on any mutable access. |
| `with_body` (711-738) | Locks the body, edits it in place under `Brep::with_rollback`, restores on refusal, failure, cancel or panic. |
| `read_body` / `read_held` (745-773) | Locks the body for the whole read. |
| `measure_body` (780-796) | Locks, makes a full `brep_copy`, unlocks, measures the copy: "a measure that takes the kernel's `&mut Brep` (for its caches)". `Body::volume` uses it (`OPS/body.rs:411-421`). |
| Can two calls touch one body at once | No. The lock is a `Mutex`, not a reader-writer lock, so even two reads serialise. Two measures can overlap after each has made its own copy. |
| Two bodies in one call | `Body::lock_pair` locks by address order (`OPS/body.rs:215-224`); used at `OPS/prism.rs:306`. Booleans take their operands out (`take_held`, `OPS/boolean.rs:119`). |
| Reentrancy | An ops call from inside a call or the failure hook returns `Error::Reentered` (500-502). |
| Does ops start threads | No. `available_parallelism` only sizes the idle list. |

The application keeps a **second pool** for the families that still call the kernel crate directly: `POOL: Mutex<Pool<Kernel>>` with `most_idle: 8`, `most_running: 256` (`crates/smlib/src/core.rs:96-129`). Its `start_clean` (161-166) resets four things, two more than ops': RNG, station memo, `fillet_point_step`, `upstream_ub_passed`. Its kernels do not share the process latch (`fresh`, 81-85). The two pools therefore give different history guarantees.

### 5b. `solidcore-tess` rayon use (`TESS/par.rs`, `TESS/mesher/mod.rs`)

- rayon is a non-wasm dependency of this crate only (`solidcore/crates/solidcore-tess/Cargo.toml:24-25`). `par::map` (`par.rs:68-93`) is an ordered `par_iter().map().collect()` on a private pool with 8 MiB stacks, or on the caller's pool when already on a rayon worker; serial on wasm and for one item. Each item runs under `catch_unwind`.
- **No `Kernel` is involved at all.** The mesher uses `solidcore::eval`, the kernel's evaluator "for geometry given by value … with no `Kernel` and no `Brep`" (`K/eval.rs:5-8`).
- **What the closures capture:** shared references to plain data: the record B-rep `b` (`solidcore_record::brep::Brep`), `&[poles::Face]`, `rules`, `boundaries`, `nodes`, `kept`. Call sites: `mesher/mod.rs:328` (face tessellation), `600` (face sizes), `621` (edge nodes), `660` (face boundaries), `959` (boundary crossings).
- **Shared mutable state inside items:** `poles::Face` holds `collapsed: Mutex<BitsMap<…, bool>>`, a memo keyed by bits, and `refused: Mutex<Option<String>>`, "the first point where the kernel could not tell" (`mesher/poles.rs:22-32`). Which point is "first" under parallel faces is per `Face`, and one face is one item, so it looks deterministic; **UNVERIFIED**.
- Results come back in item order.

### 5c. Hidden lines `Workers` (`K/hlr.rs`)

- `trait Workers: Sync { fn run(&self, count, work: &(dyn Fn() + Sync)) -> bool }` (`hlr.rs:660-665`); `Serial` runs nothing (668-676). The kernel asks the caller for threads because "library code starts none".
- `map_on_threads` (873-943): workers pull item indices from an `AtomicUsize`, push `(index, result)` to a per-worker `Vec`, and results are placed by index. First error wins through a `Mutex<Option<_>>`; a panicking worker sets `stop`.
- **No `Kernel`.** Workers share `&Ctx` (`hlr.rs:815-839`): `defs: &[Prepared]` and `scene: &[Placement]` (plain data from `K/hlr/body.rs`), `AtomicUsize` totals for budgets, an `AtomicBool` stop, and `cancel: &(dyn Fn() -> bool + Sync)`.
- The ray budget is a shared counter (`hlr.rs:2914-2916`). The total is thread-independent, so whether the budget is exceeded is too; which item reports it is not.
- The library does call `std::thread::available_parallelism()` (`hlr.rs:1015`, non-wasm) and `std::thread::panicking()` (`hlr.rs:897`), and holds an `Instant` for phase timing off wasm (`hlr.rs:3025-3028`). These sit beside the "no clock" and "no threads" rules (§7).
- The only implementation that starts threads is the application's: `crates/geom/src/hlr.rs:13-28` (`std::thread::scope`, threads named `solidcraft-hlr`). Ops' own `hidden_lines` uses `Serial` (`OPS/hlr.rs:289`).

### 5d. Other thread use

In the solidcore crates:
- `K/` library code: none beyond 5c. Every `thread_local!` and `static` in `K/` is test-only: `K/topology/face/p3.rs:3067` (inside `mod tests`, line 3051), `K/query/api_queries.rs:1184` (`#[cfg(test)] mod forced`, 1179), `K/blend/blend_setback/certify.rs:712` (`mod tests`, 708), `K/topology/brep/p2.rs:734-735` (`#[cfg(test)] mod seeded`), `K/error/site_scan.rs:896` (module is `#[cfg(test)]`, `K/error.rs:106-107`), `kernel.rs:1290` (a test). The `std::thread::Builder` uses in `K/blend/fillet_section/section.rs:2755` and `certificate.rs:6172` are tests wanting a larger stack.
- Interior synchronisation that already exists in value types: `CurveMemo(OnceLock<Arc<Mutex<…>>>)` (`K/curve/curve/p5.rs:479`), `CacheSlot(Arc<Mutex<SlotState>>)` and `MadeKindSlot(Arc<OnceLock<…>>)` (`K/surface/surface/p2.rs:509,549`), `SingularitiesMemo(Mutex<…>)` (`p3.rs:648`), `ContinuityMemo` (`p6.rs:161`), `BoundaryMemo(OnceLock<…>)` (`p8.rs:394`), `CrvOnSurf`'s three `Mutex` caches (`K/curve/crv_on_surf.rs:327-343`), `NurbSlot(Arc<OnceLock<Surface>>)` (`K/nurbs/b_spline_surface/p1.rs:461`), the offset surface's atomics and `Mutex` (`K/surface/offset_surface.rs:242-269`). These are the other agents' subject; they matter here because they are lazily filled and several are keyed by `CurveMemoKey`, i.e. by ambient kernel state.

In the application (brief):
- `crates/kernel/src/par.rs`: `map_on_threads` (71-111) spreads independent jobs over scoped plain threads with 8 MiB stacks, carrying the caller's validation scope, stop token and trace into each. `map_with_local_pools` (33-63) gives each such thread a one-thread rayon pool so nested tessellation neither multiplies nor deadlocks. Each job makes its own ops calls, so each gets its own pooled kernel.
- `crates/smlib/src/core/call.rs:45-61`: every ops call runs under `solidcore_ops::cancel::within(token, f)`; fixes are named to the ops session once.
- Long work runs beside the window on named threads (`crates/ui-egui/src/dialogs.rs:786` "apply", `control_job.rs:280`, `deferred.rs:346,458`) and is applied when done, per `.agents/skills/solidcraft-performance/SKILL.md:9-10`.
- `crates/render` uses rayon for CPU ray tracing only (`crates/render/src/raytrace/image_parallel.rs:315`).

Today's model is therefore strictly **one kernel per call, one call per body, parallelism across bodies or over plain data**.

---

## 6. Auto traits

Determined by reading field types; nothing was compiled. "Inferred" means every field I could reach is composed of `Sync` parts and a crate-wide grep of struct fields for `Cell<`, `RefCell<`, `Rc<`, `OnceCell<`, raw pointers and unbounded `dyn` found none in that type.

| Type | `Send` | `Sync` | Evidence / blocker |
|---|---|---|---|
| `Kernel` | yes, asserted (`K/kernel.rs:1157-1159`; also `solidcore/crates/solidcore/tests/issue_2136_cooperative_cancellation.rs:184-187`) | **no** | `Caches.curve_analysis: RefCell` (142); 22 `Cell<u64>` in `WorkCounters` (183-204); `fillet_point_step: Cell` (827); `blend_net_tolerance: Cell` (874); `blend_counts: Cell` (877); `blend_station_memo`, `blend_w_memo`, `loop_area_memo`, `recertify_memo: RefCell` (883, 899, 904, 910); `coons_tangency_log: RefCell` (934); and `Context` |
| `Context` | yes | **no** | `user_notify_callback`, `sys_notify_callback: Option<Box<dyn NotifyCallback>>` (`context.rs:227,229`); the trait is `Debug + Send` (166). Everything else is atomics, `Arc` and plain data. |
| `CancelToken` | yes | yes, asserted (`kernel.rs:1160`) | |
| `Curve` | yes | yes, **asserted** (`solidcore/crates/solidcore/tests/curve_cache_memo.rs:183-193`) | The doc at `K/curve/curve/p5.rs:476` ("`Send` and not `Sync`") is stale. |
| `Surface` | yes | yes, **implied by the `Curve` assertion** | `CrvOnSurf` holds `Box<Surface>` (`K/curve/crv_on_surf.rs:319-321`), so `Curve: Sync` cannot hold unless `Surface: Sync`. Fields: `Arc<Mutex<SlotState>>`, `Arc<OnceLock<…>>`, `Mutex`, `OnceLock` (`K/surface/surface/p2.rs:428-440`). |
| `Brep` | yes, asserted (`curve_cache_memo.rs:193`) | yes, inferred; **UNVERIFIED by compilation** | Arenas of plain records; `Shared<Arena<…>>` = `Arc` (`K/topology.rs:182`); `AttributeStore` is asserted `Send + Sync` (`tests/issue_1730_application_attribute_handlers.rs:264-268`; handlers are `Send + Sync`, `K/geom/attribute.rs:718`); `IdentitySlot` holds `Arc<DomainToken>`; `ProgressTag(Option<Arc<()>>)`. |
| `Face`, `Edge` | yes | yes, inferred | Plain fields plus `Option<Surface>` / `Option<Curve>` (`K/topology.rs:411-417, 456-464`). `Edgeuse` likewise (587-598). |
| `SurfaceCache<O>` | yes (required: it sits in `Arc<Mutex<SlotState>>` inside a `Sync` `Surface`) | yes for `O: Sync`, inferred | `K/surface/surface_cache/p1.rs:1630-…`; holds `Surface`, `Tree<O>`, `Option<fn(u64) -> O>` (a fn pointer). |
| `CurveCache<O>` | yes (required by `Mutex<Option<(CurveMemoKey, CurveCache<…>)>>` in `CrvOnSurf`) | yes for `O: Sync`, inferred | `K/curve/curve_cache.rs:302-314`. |
| `BrepCache<O>` | yes | yes for `O: Sync`, inferred | Three `Option<Tree<O>>` (`K/topology/brep_cache.rs:101-106`). |
| `Tree<O>` | yes | yes for `O: Sync`, inferred | `Vec`s plus `Option<Arc<CurveTreeData>>` (`K/geom/tree.rs:424-435`). |

Types on the read path that are **not** `Sync` (transient wrappers, not stored in bodies):

| Type | Field | Site |
|---|---|---|
| `query::brep::BrepQuery` | `cache: OnceCell<BrepCache>`, `bbox: OnceCell<Extent3d>`, `face_caches: KeptFaceCaches` (`Cell`s) | `K/query/brep.rs:588-595`; `K/query/brep/ray/live.rs:149-153` |
| `BrepFaceServices` | `brep: RefCell<&mut Brep>`, `caches: RefCell<FaceCaches>` | `K/query/brep/ray/live.rs:614-617` |
| `RayPreparation` | `pending: Cell<…>`, `trim_brep: Cell<usize>` | `K/query/brep/ray.rs:53-58` |
| topology intersector `FaceCaches<O>` | `pending: Cell`, `tree_requested: RefCell`, `classified_faces: RefCell` | `K/boolean/topology_intersector/p1.rs:4196-4211` |
| `FaceCacheTarget` | `entities: Box<dyn Fn(O) -> Result<EntityRef>>` | `K/boolean/topology_intersector/p1.rs:4179` |
| `ArenaPair` and `Identities` | `caches: [OnceCell<BrepCache<H>>; 2]`, `cache_kernels: [Kernel; 2]`, `failure: Rc<Cell<…>>`, boxed `dyn Fn` | `K/boolean/merge/p4/arena_pair.rs:198-216` |
| `eval::query` | `trims`, `forms: Vec<OnceCell<…>>` | `K/eval/query.rs:132-134` |
| offset reach | `Net(Rc<net::Net>)` | `K/offset/offset_executive/reach.rs:413` |
| identity environments | `key_of: &dyn Fn(EntityRef) -> ObjectRef` and similar, with no `Sync` bound (≈100 struct fields) | e.g. `K/topology/brep_cache.rs:523-525`, `K/query/brep.rs:340-342`, `K/intersect/point_class.rs:195-199`, `K/mass/integrator/properties.rs:140`, `K/topology/edge.rs:2502` |

The last row matters most for readers: classification and mass-property calls take such an environment, so a `&`-reference to one cannot cross threads until the closures are `+ Sync` or each worker builds its own.

Existing compile-time assertions, all in tests: `K/kernel.rs:1155-1161` (`Kernel: Send`, `CancelToken: Send + Sync`); `tests/curve_cache_memo.rs:183-193` (`Curve: Send + Sync`, `Brep: Send`); `tests/issue_2136_cooperative_cancellation.rs:184-187`; `tests/issue_1730_application_attribute_handlers.rs:264-268`. Nothing asserts `Sync` for `Brep`, `Surface`, any cache or `Tree`.

---

## 7. Constraints in the repo's own rules

### Threads

- "Must build for `wasm32-unknown-unknown`: no filesystem or `std::time`, and no threads in library code for now (§4c)." — `solidcore/AGENTS.md:104-105`
- "**The concurrency unit is the `Kernel`.** A `Kernel` is `Send` and holds no global state … so independent kernels run on separate threads or workers without interfering: one `Kernel` per thread." — `solidcore/AGENTS.md:291-293`
- "**Inside an operation: parallel structure, serial for now.** Chains, spines, stations, spans and corners are pure functions of immutable inputs (`docs/generic-blend.md` §5), so they can be spread over threads later with identical results. Library code starts no threads today; wasm32 always runs the same code serially." — `solidcore/AGENTS.md:297-300`
- "The kernel starts no threads itself (library code starts none, `AGENTS.md`), so a caller that may start them hands one to [`hidden_lines_on`]." — `K/hlr.rs:657-659`. This is the precedent for how the kernel gets threads: an injected runner.
- "Work that can take long … runs beside the window and is applied when done." — `.agents/skills/solidcraft-performance/SKILL.md:9`

### Determinism

- "Deterministic: no clock, no hashing of addresses, no `HashMap` iteration order in results (use `Vec`, `BTreeMap`, or sort), randomness only from `kernel.rng`." — `solidcore/AGENTS.md:101-102`
- "**Same result, any thread count.** A parallel run must equal the serial one bit for bit: no shared mutable state, no order-dependent reductions, no lazily filled caches without synchronisation. State that latches on first use (ports of C++ function statics kept in `Kernel`) makes a result depend on earlier calls; it goes when its construction is replaced, and nothing new may be added like it." — `solidcore/AGENTS.md:301-305`
- "**Parallel structure, serial for now**: chains, spines, stations, spans and corners are pure functions over immutable inputs with results in a fixed order, so the output is bit-identical whatever the thread count once threading is switched on." — `solidcore/docs/generic-blend.md:540-542`
- "Evaluating S is a pure function of the definition: the station memo keys a definition by its own identity, never by a kernel's count, so kernels on other threads evaluating the same rounds agree with one kernel bit for bit." — `solidcore/docs/generic-blend.md:172-174`
- "**Crash, UB, hang and nondeterminism fixes are unconditional**" — `solidcore/AGENTS.md:256`
- "no `unwrap` or panic, deterministic results; no fallback, retry, per-case" — `solidcore/docs/coordinating.md:100`

### Global state

- "No global or `static mut`/`thread_local` state. Everything lives in `Kernel`." — `solidcore/AGENTS.md:103`
- "keeping all state in `Kernel` is what removes that limit, so never add global, `static` or `thread_local` state." — `solidcore/AGENTS.md:295-296`

A redesign that moves state out of `Kernel` into explicit contexts is consistent with the second sentence and against the letter of "Everything lives in `Kernel`"; the rule text will need an edit.

### wasm

- "Must build for `wasm32-unknown-unknown`" — `solidcore/AGENTS.md:104`; `crates/solidcore       the kernel (pure Rust, no unsafe, wasm-ok)` — `solidcore/AGENTS.md:13`
- "**Recursion.** wasm32 has no stack guard page: a stack overflow there silently corrupts memory instead of aborting." — `solidcore/AGENTS.md:121-122`. Worker threads need the 8 MiB stack both existing pools use (`TESS/par.rs:10`; `crates/kernel/src/par.rs:10`).
- "Everything below the apps must build for `wasm32-unknown-unknown`: `cargo xtask wasm` checks every library crate and the web app … Do not add a dependency that cannot." — `.agents/skills/solidcraft-performance/SKILL.md:22`
- "Application code is Rust only and builds for wasm" — `AGENTS.md:18`

### Panics and `unsafe`

- "No `unsafe` (`#![forbid(unsafe_code)]`, workspace-wide)." — `solidcore/AGENTS.md:98`. No hand-written `unsafe impl Sync`.
- "No `unwrap`/`expect`/`panic!`/`todo!`/unchecked `[i]` indexing (denied by lints; tests are exempt)." — `solidcore/AGENTS.md:99-100`. Lock poisoning must be handled with `unwrap_or_else(PoisonError::into_inner)`, as the existing mutexes do.
- "Never crash: no unwrap/expect/panic/unsafe in non-test code, errors are `Result`, input is hostile" — `AGENTS.md:13`
- "No new panic-prone calls the lints miss … or mark the site `proven` with its reason in `plan/panic-sites.tsv`" — `solidcore/AGENTS.md:113-115`

### Caches

- "Clearing must never change results, only speed." — `K/kernel.rs:134`
- "The memos and shortcuts never change a result, only the time; `false` builds and runs everything in full, to check that." — `K/kernel.rs:849-850`; the `curve-memo-off` feature "to run difftest and the tests in both positions. Results never depend on it." — `solidcore/crates/solidcore/Cargo.toml:49-51`
- "no lazily filled caches without synchronisation" — `solidcore/AGENTS.md:302-303`

### Dependencies

- The kernel crate depends on `serde` and `thiserror` only (`solidcore/crates/solidcore/Cargo.toml:59-61`). **rayon is not a dependency of the kernel**; it is in `solidcore-tess` for non-wasm targets only (`solidcore/crates/solidcore-tess/Cargo.toml:24-25`).
- `solidcore/deny.toml` bans licences, wildcards, unknown registries and git sources; it has no crate ban list, so rayon is not forbidden by it. What forbids it today is the "no threads in library code" rule and the injected-runner precedent.
- "no manifest under it may use a `path` that leaves solidcore/ or depend on a `solidcraft-*` crate" — `.agents/skills/solidcraft-layers/SKILL.md:16`

### Layering

- "**The kernel computes; ops decides and sequences.**" — `solidcore/docs/layers.md:26`; "Two implementations of one job are a bug." — `solidcore/AGENTS.md:309`. Two kernel pools with different reset rules (§5a) sit awkwardly with this.
- "Every geometric job … has exactly one implementation, in solidcore." — `.agents/skills/solidcraft-kernel-rules/SKILL.md:11`. A parallel path must be the same code as the serial path, not a second version.

### Gating: what may be worked on

- "**Speed tricks.** A speed-up is taken only with its root cause, shown as a count (evaluations, subdivisions, enclosures), never a timing, on a case that is slow for users or for CI. Making a slow path a constant factor faster, or making a slow test cheaper, is not taken." — `solidcore/AGENTS.md:42-44`
- "**Anything not reported** waits. A pull request names, in its first lines, the issue it fixes; one that fixes no reported issue is reviewed after all the ones that do." — `solidcore/AGENTS.md:45-46`
- "**Show the problem first, then research it.** … A fix nobody has seen the problem of, or built without that research, is not ready for review." — `solidcore/AGENTS.md:75-88`
- "**No new document without the maintainers.**" (never add a file to `docs/` not discussed on its issue) — `solidcore/AGENTS.md`, §4
- "A speed-up of kernel code goes in `solidcore/` with its root cause, per `solidcore/AGENTS.md`" — `.agents/skills/solidcraft-performance/SKILL.md:18`

Multi-threading is a constant-factor speed-up by this definition. As written, the rules do not admit it without a reported slow case and a maintainer decision.

### Flags for result changes

- "**No compatibility flags.** A fix changes the default result, with a regression test that fails without it. No new `Kernel::fixes` (`UpstreamFixes`) field, no old path kept behind a switch, no preference for the old behaviour … Each PR says what result changes and shows the evidence (for example: every case that built before is unchanged bit for bit, or the cases that change and why the new result is right)." — `solidcore/AGENTS.md:234-238`
- "A fix runs always. Don't add an off-by-default switch, a "legacy" mode or a flag that keeps the old wrong path alive." — `.agents/skills/solidcraft-kernel-rules/SKILL.md:16`
- "**Retire the existing flags.**" — `solidcore/AGENTS.md:239-…`

So a "threads on/off" switch may exist only if both positions give bit-identical results; it cannot gate a result change (for example a new RNG stream).

### Goldens, known-failing lists and test changes

- "**The gate.** CI gates on fmt, clippy, the tests, the wasm build and the goldens." — `solidcore/AGENTS.md:133-134`
- "A case fails when its output changed (integers, topology, status and deferrals exactly; floats within `1e-9` relative …)" — `solidcore/AGENTS.md:139-141`
- "When your change moves a result on purpose, bless the unit (`--unit <id> --bless`) and commit the golden with its justification: in the commit message and the pull request, the cases that changed with their old and new values, why the new ones are right, and pictures before and after for geometry. A golden change without that is not reviewed. Never loosen a tolerance or a floor to keep a golden." — `solidcore/AGENTS.md:144-148`
- "Code ported unchanged from the pinned C++ or the fork keeps their results bit for bit." — `solidcore/AGENTS.md:230-231`
- "Every fix comes with a regression test that fails without it." — `solidcore/AGENTS.md:130-131`
- "Never take an expected value from your own run, never loosen a bound to fit what the new code produces, never mark a test `#[ignore]`, and never move a known-failing test's inputs until it passes." — `.agents/skills/solidcraft-kernel-rules/SKILL.md:25`
- "Changing or deleting a line of an existing test or golden needs a `Test change: <reason>` line in the PR body. The reason derives why the old value was wrong … "Matches the new output" is not a reason." — `.agents/skills/solidcraft-kernel-rules/SKILL.md:26`
- "A test fixed by your change: drop its line from `tests/KNOWN_FAILING_TESTS` in the same PR." — `.agents/skills/solidcraft-kernel-rules/SKILL.md:29`; kernel cases keep `solidcore/crates/solidcore/tests/kernel_cases/known_failing.json` and the goldens `solidcore/crates/goldens/expected/known_failing.json`, updated with a new render in the same change (`solidcore/AGENTS.md:248-255`).
- "If the old result was a valid solid, the change must still produce a valid result. A new refusal there is a regression." — `.agents/skills/solidcraft-kernel-rules/SKILL.md:20`
- Verify list: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test -p solidcore`; `cargo check -p solidcore --target wasm32-unknown-unknown`; the goldens of every unit touched — `solidcore/AGENTS.md:198-202`

Consequence for this redesign: a pure refactor (pass state explicitly, split `Kernel`) must leave every golden bit-identical and say so. The two changes that would move results — replacing the RNG stream and removing the latch — each need blessed goldens with a derivation, and "the draw changed" is exactly the "matches the new output" reason the rules reject, so each needs an argument that the classified result is invariant to the draw.

### Existing code that sits beside these rules

- `K/hlr.rs:1015` (`available_parallelism`), `897` (`thread::panicking`), `3025-3028` (`Instant`, non-wasm) in library code.
- `HashMap` in `LoopAreaMemo` (`K/mass/loop_area.rs:228`) and `RecertifyMemo` (`convert.rs:792`): lookup only, never iterated into a result, so within the rule.
- `OPS/session.rs` and `OPS/cancel.rs` use `static` and `thread_local!`; the "no global state" rule is scoped to `crates/solidcore`.

---

## 8. What must change for fork-join inside one operation

Ordered by how much code each touches, largest first.

1. **Split `Kernel` into a shared read-only part and per-thread parts** (≈6,100 signatures see the type; ≈4,900 can keep taking a shared reference). Shared and `Sync`: tolerances, fixes, `memoize_curve_caches`, the cancel token. Per-worker: counters, profiler, logs, memos. Apply-only: notify context, identities, record-map scratch. The compiler then enforces what workers may touch.
2. **Drop `&mut Kernel` from the ≈1,100 pass-through functions.** Once the apply-only state has its own type, only the ≈134 direct functions and their callers on the edit path need a mutable handle; everything on the compute path takes the shared part.
3. **Give topology edits an explicit apply context** (≈90 direct functions, 16 files using `with_notification_context`): callbacks, identity domain, `NotifyRecords` scratch (`notify_records`, `arena_notify_records`), `blend_face_log`, UB notes. One thread owns it. Notification keys and B-rep serials are issued only there, in a fixed order.
4. **Make the identity environments shareable** (≈100 struct fields of `&dyn Fn(EntityRef) -> ObjectRef` and similar): add `+ Sync`, or replace closures with a plain key-scheme value. Without this no classification, cache or mass-property call can run on a worker.
5. **Replace `Cell`/`RefCell` on the kernel with per-worker state merged deterministically** (≈31 counter sites, 5 memos, 2 logs): counters summed; logs concatenated in task order; memos per worker or behind a lock, legal only because a hit equals a fresh computation.
6. **Turn `blend_net_tolerance` into an explicit evaluation argument** (≈16 scope sites, 3 readers, every procedural surface evaluation). It is result-affecting, set far above its readers, and missing from the cache keys; a worker must inherit the forking thread's value exactly.
7. **Remove marks from the `Context`** (≈34 `MarkLock` sites, 2 `claim_mark` sites): a traversal owns its epoch and side table. This also removes the largest non-notify reason for `&mut Kernel`.
8. **Delete `doing_boolean`** (≈59 writer lines, 1 reader, 2 cache keys): pass "defer the tree" to `trim_srf_cache_tree`, or always build lazily, and drop it from `CurveMemoKey` and `FaceCacheKey` so caches built inside and outside a boolean are the same cache.
9. **Move the context size scalars onto the body** (3 setters, ≈5 callers, 3 key structs): `model_size_estimate`, `large_small_size_ratio`, `zone_tol_3d` are per-body facts stored in shared state. Until then, both pools should reset them per call.
10. **Replace the ad hoc kernel copies with one fork rule** (5 sites): pass the owner zone tolerance to `Curve::trim` explicitly, and make `cache_kernel`'s field-by-field decisions the single definition of "a worker's kernel".
11. **Remove or key the RNG** (2 draw functions, 5 clone sites, 1 save/restore): a deterministic direction and dither sequence indexed by sample number, or a stream seeded from (operation seed, shell or face id). Either moves goldens and needs the invariance argument of §7.
12. **Delete the fillet step latch and its process-wide cell** (1 reader, `OPS/session.rs:66,212`): turn `fillet_point_step_per_point` on for good. The rules already say latches "go" and nothing like them may be added.
13. **Settle the station memo** (3 borrow sites, both pools' `start_clean`): either show a hit is bit-identical to a fresh solve and treat it as a memo, or key it completely. Today its owners clear it per call because they do not rely on that.
14. **Unify the two kernel pools' reset rules** (`OPS/session.rs:238-241` vs `crates/smlib/src/core.rs:161-166`): once items 9, 11, 12 and 13 land there is nothing left to reset, and a kernel's history stops mattering.
15. **Decide how the kernel gets threads and amend the rule text** (`solidcore/AGENTS.md:103-105, 297-305`; kernel `Cargo.toml`): the precedent is an injected runner (`hlr::Workers`) with a serial default for wasm, not a rayon dependency in the kernel. Add compile-time `Sync` assertions for `Brep`, `Surface`, the caches and the new shared kernel part beside the existing ones, and decide what a cancel token that counts polls means with workers.
