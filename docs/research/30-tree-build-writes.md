# 30: What main's face tree build writes, by value

Labels: **MEASURED** (probe output), **READ** (from code), **INFERRED** (reasoning on the two).

Scratch only. Worktree `sc-wt/cache-tree-slot-probe`, branch `zz-tree-slot-probe`, side ref `push/cache-tree-slot-probe-3` (`ed0b8f5d3a`). Run on server2 in 176 s. Nothing is on the `cache-tree-slot` branch and nothing was pushed to the fork.

## Method

**Probe.** The probe has two parts:
- `solidcore/src/zz_value.rs`, a `doc(hidden) pub` module;
- `solidcore-convert/tests/zz_tree_value.rs`.

**Variants.** The body builders are report 29's, copied verbatim: kernel bodies, scenarios, corpus files and the round trip. Each runs as is and stripped. That gives 288 variant-states:

| Class | Variant-states |
|---|---|
| K | 10 |
| O | 116 |
| I | 56 |
| RK | 10 |
| RO | 106 |

10 more were skipped because 5 round-trip intersections fail to build. That is report 29's 295-variant set less those skips.

**Build path.** Main's own build is `query::brep::ray::face_surface_cache(k, b, f, get_tree = true)`: the factory, `get_tree_with_face`, `build_tree_body` and `implant_face`, with the face routine inside. It is called directly on every face of the body. That covers every face any query family would build. The query families themselves were not re-run: they act on working copies, and the body does not change.

**Snapshot.** The whole body is snapshotted by value before and after each build. Write stamps are ignored.
- Face: tolerance, uv_domain, rectangular_trim, loops.
- Face surface: geometry (Debug hash), owner, owner_chain_face, owner_zone_tol_3d, made-NURB kind (the lazy NURB slot), cache-slot state (Empty/Stored/CheckedOut, modified-while-out, cleared_for), definition present.
- Edge: tolerance, interval, curve geometry and owner.
- Vertex: tolerance, point.
- Edge use: topology links, trim geometry (Debug hash of the kind, i.e. control data, knots and interval), trim owner and owner tolerance, both gap caches, drop-fail memo.
- Vertex use: uv, owner, both gap caches.
- B-rep: editing flag, tolerance, size estimates, surface_owner_cleared, deleted surfaces, entity counts.

**Classes.**
- (i) no value changed;
- (ii) only derived caches changed: gap caches, drop-fail memo, made-NURB kind, cache slot;
- (iii) something of meaning changed.

**Runs per variant-state.**
- R1 raw: pass 1 and pass 2 over every face.
- R2 settled: a fresh body, then `create_missing_uv_trim_curves`, then pass 1 and pass 2.
- R3 copy: on a fresh, independent body (raw, and settled), each face is built on its own `clone_standing_for_self()` copy (definitions dropped). Its tree is compared with the R1 or R2 tree for that face. The body is snapshotted around all the copy builds.

**Tree identity.** A tree is identified by a Debug hash of `Tree`, which is handle-level data with no addresses.

A note on counting trims: a face's back face-use edge uses never hold a trim (`uv_trim_curve_pointer` is None on both sides of every settled face: exactly half, 2644 of 2644). So "every edge use has a trim" means every front-side edge use.

## 1. Value diff per build (MEASURED)

Each state has 1322 face builds per pass.

| State | Pass | All trims present? | (i) | (ii) | (iii) | What changed in (iii) |
|---|---|---|---|---|---|---|
| as_is raw | 1 | yes (1065) | 1059 | 1 (slot) | 5 | trims rewritten, VU gaps |
| as_is raw | 1 | no (257) | 9 | 0 | 248 | 248 trims created, 78 E.tol and 74 V.tol raised |
| as_is raw | 2 | | 1322 | 0 | 0 | |
| as_is settled | 1 | yes (1322) | 1307 | 1 (slot) | **14** | 14 trims rewritten, 3 E.tol raised, VU gaps |
| as_is settled | 2 | yes | 1322 | 0 | 0 | |
| stripped raw | 1 | no | 56 | 0 | 1266 | trims created, E.tol and V.tol raised, one E.int |
| stripped raw | 2 | | 1322 | 0 | 0 | |
| stripped settled | 1 | yes (1322) | 1263 | 1 (slot) | **58** | 58 trims rewritten, 5 E.tol and 1 V.tol raised, VU gaps |
| stripped settled | 2 | yes | 1322 | 0 | 0 | |

**First build after `create_missing_uv_trim_curves`, with every trim present:**
- The loop-2 gap fix (`fix_uv_trim_curves`) rewrites existing trims in 72 of 2644 builds, across 25 variant-states.
- In 8 of those builds it also raises edge or vertex tolerances. The largest is `vase1` stripped E28, 1.206e-3 → 6.608e-3 (5.5 times). Others are `vase2` E19 1.70e-4 → 2.67e-4 and `vase3` E39 2.52e-5 → 4.59e-5.
- As-is (unstripped) variants affected:
  - `shared_face_overlap1e-6_union` (O and RO)
  - `Fillet/open_shell`
  - `vase1`, `vase2`, `vase3`
- Stripped variants affected: those above, plus:
  - `slot_through_holes_diff`
  - `torus_box_inter`
  - `shell_cyl`
  - `blade_hub_2`
  - `bottle`
  - `cliff_2`
  - `cyl_cyl_2191`
  - `shell_fillet2`
  - `ShellBrep_983`
- The same rewrite occurs on raw as-is bodies where every trim is present (5 builds: `shared_face_overlap1e-6_union`, `open_shell`, `vase2`). Bodies straight out of operations or file read carry trims that the face pass changes on first use.

**Second build: 5288 of 5288 builds changed no value, in all four states.** Not even a derived cache changed. The pass converges in one round.

**Nothing else moved.**
- Owner, editing flag, surface_owner_cleared, face tolerance and NURB slot never changed by value in any build. The bracket restores all of them. (Earlier probe: the `faces`, `editing_enabled` and `surface_owner_cleared` stamps still move on every build, including the no-op second builds.)
- The made-NURB kind never changed in any build. INFERRED: these bodies already had their NURBs made before the first build, or the kinds involved need none. This is not proof that the build never makes one.
- The only derived-only change was the cache slot: 1 face in `sphere_tangent_box_union` on first build, and 104 on stripped raw alongside trim creation.

**Faces whose trims are missing but where the pass changed nothing:** 65 face builds on `offset_*` and `shell_*`. No trim build was even attempted (`trims=0`). So "a trim is missing" does not imply the pass writes, either.

**Trees (MEASURED):**
- For every face in every state, the pass-1 tree equals the pass-2 tree (5288 of 5288), including the 72 builds whose pass rewrote trims. The tree is built after the fix, on the fixed trims.
- The raw-route tree equals the settled-route tree after convergence (2644 of 2644). Settling first or not makes no difference to the final tree.

**Cost (MEASURED, sum of microseconds over 1322 builds):**

| State | Pass 1 | Pass 2 |
|---|---|---|
| as_is raw | 6623 ms | 239 ms |
| as_is settled | 723 ms | 244 ms |
| stripped raw | 24030 ms | 241 ms |
| stripped settled | 4373 ms | 238 ms |

A converged build, pass included, averages about 0.18 ms per face.

## 2. The no-op condition

**"Every edge use has a trim" is not it (MEASURED).** It is false-positive: 77 first builds have every front-side trim yet rewrite trims, and 8 of them raise tolerances. It is also false-negative: 65 builds have trims missing and change nothing.

**"The face pass has run since the last write to the face's inputs" holds everywhere measured.** Every second build is a value no-op. This is the condition.

**Inputs the pass reads (READ, `create_uv_trim_curves_notified` and its loops):**
- the face's edge uses: trims, topology, gaps;
- their edges: curve, interval, tolerance;
- their vertices: point, tolerance;
- vertex uses: uv;
- the face: surface, uv domain, tolerance;
- the kernel tolerances and fixes.

**Where a per-face mark held in the body must be cleared (READ).** Every write to any of those inputs:
- for an edge or vertex, the mark of every face using it. Face A's pass raised shared E.tol in the vase cases, so A's pass must clear B's mark. The measured second round shows B's re-run is then a no-op.
- The raw-field writes go through `face_mut` / `edge_mut` / `vertex_mut` / `edgeuse_mut` / `vertexuse_mut` arena access. A crude grep finds:

| Write | Sites | Files |
|---|---|---|
| `.uv_trim_curve =` | 71 | 28 |
| trim `take` / `replace` / `as_mut` | 16 | 13 |
| edge tolerance | 13 | 10 |
| vertex tolerance | 13 | 6 |
| `.surface` writes | ~141 | 68 |
| `.interval =` | ~77 | 36 |
| `.curve` writes | ~227 | 92 |
| `.point =` | ~91 | 36 |

  The last four rows include unrelated structs. The edge-use and vertex-use topology writes come on top.
- INFERRED: a hand-traced per-face mark is a large, fragile trace. It is the same set the write stamps already cover, by construction, through DerefMut.

**The cheap, sound form (INFERRED from the measurements).**
1. Make the pass bracket stamp-neutral when it changes nothing. The bracket is the owner clear and restore on `faces`, the `sync_face_surface_tolerance` rewrite of the same value, the editing toggle in `implant_face`, and the surface_owner_cleared push and pop. Two ways: write-if-changed, or a scoped restore of the stamp.
2. The mark is then "the WriteStamps seen right after this face's pass" (the coordinator's per-face mark, held by the caller, not the body). Any later write to any input moves a stamp and voids it, with no site trace.
3. A pass that does write (the 72 cases) moves stamps honestly, and voids earlier marks. The next round is a no-op (measured), so marks become stable after at most two rounds.

## 3. Design options

### (A) Read-only build when the condition holds, main's path otherwise

**What holds (MEASURED and READ):**
- When the condition holds, main's build writes nothing (5288 of 5288).
- So a read-only build — implant only, no owner clear, no sync, no gap fix, with the editing toggle kept adapter-local as in report 29's `LazyCacheFace` — has the same inputs as main's.
- INFERRED: it gives the same tree. This is not measured here. The test is to add report 29's read-only build to this probe and compare its tree hash with the pass-2 hash per face. That is one more probe run.

**Where (A) is not neutral.** When the condition does not hold, the read-only tree differs from main's (INFERRED): main implants the fixed trims and the read-only build would implant the unfixed ones. That is the 72 builds over 25 variant-states. Report 29's 70,337 matching answers were on exactly such settled-but-not-passed bodies. So answer equality hid tree differences: classification, rays and mass at 1e-7 were not sensitive to these trim refits or tolerance raises. Answer-level equality is not evidence for (A) without the mark.

**Key.** The key is the mark (post-pass stamps), plus kernel tolerances, fixes and context, plus:
- `blend_net_tolerance`. READ: it is a kernel `Cell`, not in `FaceCacheKey`. It must go into the key; nothing in this corpus varied it.
- The lazy NURB state. MEASURED: it never changed during a build. READ: `natural_uv_domain_with_nurb` in PreRescue fills it; the implant reads `natural_uv_domain_if_built`. INFERRED: the build is independent of it once the PreRescue fill has happened. A read-only build should call `natural_uv_domain_with_nurb` itself, which fills a shared cell and is stamp-free. Then it is not a key input.

**Do read entries need a standing copy?** Only because main's pass writes.
- MEASURED: a tree built on a standing copy (definitions dropped) equals the body's tree in 2644 of 2644 settled builds, and in 2642 of 2644 raw builds.
- The 2 raw differences (`test8` and `torii_unioned` stripped, face 1) are history, not copying: the body's face 1 was built after face 0's pass had raised a shared tolerance, and the fresh copy's had not. Main itself is history dependent on unsettled bodies.
- MEASURED: copy builds changed the source body's values in no case. In 62 variant-states they changed only its shared cache slot (derived).
- INFERRED: with the mark holding, a read-only build needs no copy at all. Without the mark, the copy is still needed for main's path.

### (B) Scoped toggles that do not move stamps

**Evidence.** MEASURED: on a converged body, the only stamps a build moves are `faces`, `editing_enabled` and `surface_owner_cleared` (earlier probe). Every value under them is restored (this probe). So (B) changes no value. It is results-neutral by construction, and testable: a second pass moves no stamp.

**Risk (READ).** A read inside the scope that keys on stamps would see the inner state under the outer stamp. Write-if-changed for the tolerance sync avoids that for `faces`. The owner clear and the editing toggle do change value inside the scope, so those need the scoped-restore form. Their inner readers are the trim creation and the implant, which take no stamp-keyed cache today (READ). Nothing has been built for (B) yet.

### (C) Leave the tree per call

The cost of (C) is a converged rebuild of about 0.18 ms per face per call (MEASURED), plus the standing copy per read entry. It is always correct.

## 4. Recommendation

**Do (B) first, then (A) keyed on post-pass stamps.**
1. **(B) alone is the results-neutral first step on main.** It changes stamps only, never values. Its golden: a second tree build on a settled body moves no stamp, and the existing `write_stamps_move_on_every_write_and_only_then` stays green.
2. **(A) on top**, with one entry point:
   - if the face's post-pass stamp mark, tolerances, fixes, context and `blend_net_tolerance` all match, use the kept tree or a read-only build;
   - otherwise use main's path, which runs the pass and records the mark.

   This is results-neutral iff the read-only tree equals main's when the pass is a no-op. That needs the one probe run described above before the step is filed.

**(A) cannot be a results-neutral step by itself without (B).** Its only cheap condition, "all trims present", is wrong in 77 of 2644 first builds and would change trims and tolerances on 25 variant-states. A body-held per-face mark would need the ~600-site write trace instead.

If (B) cannot be made sound (a stamp-keyed reader inside the bracket), fall back to (C).

## Evidence files

- probe log: `/tmp/claude-zzv-full.log` (main box)
- analysis scripts: `sc-trash/zzv/an.py`, `sc-trash/zzv/an2.py`

## Tolerance creep (scratch measurement)

Scratch branch `zz-tree-slot-s`, side ref `push/cache-tree-slot-probe-7` (`d3233d30f7`). Run on server2: 6 tests, all ok, 436 s. Nothing was pushed to the fork.

### Method

**Write log (scratch only).** `solidcore/src/zz_tol.rs` keeps a per-run log of every tolerance write whose value changes (bitwise). Each entry holds the entity kind and id, the old and new value, the write site, and the stack of named scopes it ran under.

Write sites logged:
- `edge::set_tolerance`, `vertex::set_tolerance` and the face tolerance setter;
- the record read in `p10` (from the stored value);
- direct field writes that bypass the setters: the reach restore in edge use, wide-vertex centring and restore, the trim copy write-back in `apply_uv_trim_updates`, the edge merge in `p3`, and the fillet rail.

Scopes (the cause):

| Scope | Covers |
|---|---|
| `facepass` | `implant_face` |
| `set_trim` | `set_uv_trim_curve` |
| `create_missing` | `create_missing_uv_trim_curves` |
| `face_trims` | the face routine `create_uv_trim_curves` |
| `fit1.01` | the loosened-fit `error * 1.01` write |
| `after_boolean` | `update_tolerances_after_boolean` |
| `validate_update` | `validate_and_update` |
| `check_brep` | `check_brep_scoped` |
| `operand_match` | `matched_tolerances` |
| `stitch` | the stitch entry points |
| `heal` | heal entry points |
| `transform` | `transform_matrix` |
| `fillet` | the API fillet |

Writes to new entities (old value 0) count as initialisation and are not counted as raises.

Not logged:
- constructors and entity copies;
- the B-rep-level tolerance;
- poly;
- two local structs, in the topology intersector and in p9's face-point input.

**Chains.** Each runs on the ops layer's own path (`solidcore-ops/tests/zz_tol_creep.rs`): `Body::boolean` and fillet, then the proof (`check_solid`), then reads (`volume(1e-4)` and `check(All)`), then optionally a round trip (`to_record`, then `from_record`).

There are four variants per chain:

| Variant | Settle | Round trip |
|---|---|---|
| main | no | no |
| main+rt | no | yes |
| S | yes | no |
| S+rt | yes | yes |

Under S, `settle` runs on the result of each operation, and again after each `from_record`.

The tools are placed near a random vertex of the body, jittered by 0.13 of its extent:
- even steps: a small cylinder, subtracted;
- odd steps: a small box, united when i%4 = 1, otherwise subtracted.

Fillets are Round, with r = 1% of the smallest extent. All fixes are on.

Statistics:
- per step: n, max, median and the count above the default 1e-5, for edges, vertices and faces;
- per step: raises and lowerings by cause, with count and max ratio.

### Results (MEASURED)

End state of each chain, comparing main with S. "Start" and "end" are E max / count above default of n.

| Chain | Variants | Start E | End E, main | End E, S | Start V | End V, main | End V, S | Real raises, main / S | Same state every step |
|---|---|---|---|---|---|---|---|---|---|
| (a) box + 20 Booleans | no rt | 1e-5 / 0 of 12 | 1e-5 / 0 of 225 | 1e-5 / 0 of 225 | 1e-5 / 0 | 1e-5 / 0 | 1e-5 / 0 | 0 / 0 | yes |
| (a) | rt | 1e-5 / 0 of 12 | 1e-5 / 0 of 237 | 1e-5 / 0 of 237 | 1e-5 / 0 | 1e-5 / 0 | 1e-5 / 0 | 0 / 0 | yes |
| (b) + fillet every 3rd | no rt | 1e-5 / 0 of 12 | 4e-5 / 14 of 167 | 4e-5 / 14 of 167 | 1e-5 / 0 | 4e-5 / 15 | 4e-5 / 15 | 9 / 9 | yes |
| (b) | rt | 1e-5 / 0 of 12 | 4e-5 / 20 of 171 | 4e-5 / 20 of 171 | 1e-5 / 0 | 4e-5 / 21 | 4e-5 / 21 | 2 / 2 | yes |
| (c) blade_hub_1 | both | 4.619e-4 / 42 of 117 | 4.619e-4 / 57 of 177 | 4.619e-4 / 59 of 177 | 4.619e-4 / 62 | 4.619e-4 / 78 | 4.619e-4 / 78 | 53 / 56 | no |
| (c) vase1 | both | 1.958e-3 / 36 of 48 | 3.901e-3 / 48 of 86 | 3.901e-3 / 50 of 86 | 1.958e-3 / 32 | 3.901e-3 / 44 | 3.901e-3 / 45 | 55 / 55 | no |
| (d) box x2 / x0.5, ten times | both | 1e-5 / 0 of 12 | 1e-5 / 0 of 12 | same | 1e-5 / 0 | 1e-5 / 0 | same | 260 / 260 (all undone) | yes |
| (d) vase1 | both | 1.958e-3 / 36 of 48 | 1.958e-3 / 48 of 48 | same | 1.958e-3 / 32 | 1.958e-3 / 32 | same | 990 / 990 | yes |
| (e) 20 round trips, box / blade_hub_1 / vase1 | both | as start | as start | as start | as start | as start | as start | 0 / 0 | yes |
| (f) check_brep in place, validate_and_update (Level 2), ops check; 3 bodies | both | as start | as start | as start | as start | as start | as start | 0 / 0 | yes |

#### Raises by cause, summed over the steps (main; S where it differs)

| Chain | Cause (step\|scope\|site) | Count | Max ratio |
|---|---|---|---|
| b | fillet\|stitch\|vertex_set | 8 | 1.000 (sub-ulp-scale) |
| b | fillet\|stitch\|edge_set | 1 | 4.0 (1e-5 to 4e-5; the plateau) |
| c | boolean\|set_trim (E 22, V 37) | 59 | 2.385 |
| c | boolean\|face_trims (E 19, V 16) | 35 | 120.4 |
| c | fillet\|face_trims and set_trim | 5 | 58.8 |
| c | fillet\|stitch\|vertex | 3 | 1.0 |
| c, main only | fillet\|facepass>face_trims (E 4, V 2) | 6 | 27.4 |
| c, S only | settle\|face_trims (E 6, V 3) | 9 | 36.4 |
| d | scale\|transform (E 600, F 250, V 400) | 1250 | 10.0 once, then 2.0 |

**Causes with zero writes in every chain and variant:**
- `fit1.01`;
- `after_boolean`;
- `validate_update`;
- `check_brep`;
- `operand_match`;
- heal (after import);
- edge merge;
- the trim copy write-back;
- wide-vertex centring;
- the record read.

Nothing was raised by a proof, a read or a round trip, in any variant.

At import, heal lowers tolerances before step 0. On vase1 it lowers 48 edges (by up to x651), 19 faces and 32 vertices. On every body, the first raises are trim creation during the Boolean itself.

### Answers

**Bound.** Tolerances plateau; none grows without bound.
- (a) stays exactly at the default.
- (b) plateaus at 4e-5 from the first fillet, which stitches one edge at x4. Later steps add new entities at that value but raise none further.
- (c) max is flat on blade_hub_1. On vase1 it doubles once, at the first Union (1.958e-3 to 3.901e-3, from Boolean trim creation), then stays flat. The count above default grows with the new entities each Boolean adds. These are fresh trims on fresh intersection edges, not existing entities raised again.

**Dominant cause.** Trim creation in the operation:
- `set_trim` (`set_uv_trim_curve`) has the most raises, at ratios up to 2.4;
- the face routine `face_trims` has the largest ratios, up to x120, on vase1's Boolean.

The fillet's stitch causes the (b) plateau. The 1.1-gain updates, the 1.01x fit and operand matching never fire on these chains.

**Fixed points.**
- (e) and (f) are exact fixed points: zero writes on all three bodies.
- (d) is a fixed point on the box: exact x2 and x0.5, back to the bit.
- (d) on vase1 is not, once. The first x2 raises every face (1e-5 to 1e-4, x10) and the 12 edges that were below the new floor, and they never come back. After that, each cycle is exact.

  READ, `topology/brep/p5.rs` around line 3743. With scale_tolerances, the transform sets the B-rep's size estimate from the scaled box, `zone = size_estimate_to_zone_tol_3d(size)`, and writes each tolerance as `max(zone, tol * s)`. The x0.5 step writes `max(zone/2, tol/2)`. A tolerance below the floor at the first scale (heal had lowered it, or it was the 1e-5 import default on a body whose size-based zone is larger) is lifted to the floor and then tracks it.

  So this is a one-time move from the import default to the size-based zone, not creep. It is a raise with no change in shape: the x2 then x0.5 pair is geometrically the identity, but it leaves 12 edges and 19 faces higher.

**Design S against main.** End tolerances are identical on (a), (b), (d), (e) and (f), at every step.
- On (c) both reach the same max.
- S has 2 more edges above default on each body, and 1 more vertex on vase1.

Why (INFERRED from the per-step log):
- S settles the Boolean's result at once. Its face routine raises faces that main raises only when a later tree build passes them.
- Main's vase1 fillet step does this in its own internal face pass (`fillet|facepass>face_trims`, E 4 and V 2, x27.4). Under S, the step 4 settle had already made the same raises (same ratio, x27.424).
- The extra edges under S are the faces main has not yet passed at the end of the chain: S's settle at step 7 raised 2 edges at x36.4.

So S makes no new kind of raise. It makes main's lazy face-pass raises earlier and on every face, where main makes them only on the faces it happens to build. Reads under S never raise.

On these chains settle converged in at most two rounds every time:
- 1062 settles made no change;
- 36 settles changed 1 to 10 entities in round 1, then nothing.

### Open

- (b) with round trip reports 115 "unlogged" changes. These are value changes seen by the snapshot but not by the log. They are likely ids renumbered by `from_record`, not writes. Unverified.
- Proofs: 4 steps on blade_hub_1 do not prove, and vase1 never proves (`Unsupported`). This is the same in main and S.

Evidence: run log `/tmp/claude-zzt-run.log` (main box); scripts `sc-trash/zzv/patch_tol.py` (the instrumentation) and `sc-trash/zzv/an_tol.py`.

## 5. Design S (scratch measurement)

The run is complete: 24 shards, all ok, in 4024 s on server2. 298 variant-states, plus 10 skipped as unbuildable. Branch `zz-tree-slot-s`, side ref probe-7 (`d3233d30f7`). Log: `/tmp/claude-zzs-full.log`. Comparisons: M0 = main on the raw body in its own lazy order; MS = main on the settled body; S = design S.

**1. Read-only tree vs main's second-pass tree (MEASURED).**
- 2640 of 2644 faces have the same hash, and 0 differ.
- 4 faces give the typed refusal `PreparationRequired { "lazy trim: classification tree" }`. All 4 are on `sphere_tangent_box_union` (O and RO, as_is and stripped), one face each.
- CORRECTED (6.1): this is not a missing trim. After settle no edge use lacks a trim. The refusal is the read-only path giving up on a vertex use's global UV solve, which main runs as a plain surface solve inside its build. With that solve behind `&Brep`, 2644 of 2644 trees match.
- Every body was still settled after its reads, and twin bodies were equal.

NOTE: the first run's answers and counters below may be contaminated. The prototype's switches and counters were process-wide, shared by the shard tests running side by side in one process. They are now per test, and the clean rerun is in 6.0, which supersedes the numbers here.

**2. Answers (MEASURED).**

*MS vs S:* 0 differ in every family: box, cls, ray, closest, mass, check. Refusals are on the S side only:
- 279 cls and 25 ray answers, all ERR `unported` in the scratch prototype (the radial and other paths not routed to `lazy_face_tree`);
- not refusals of the design.

*M0 vs S,* per family:

| Family | Result |
|---|---|
| box | same |
| closest | same |
| check | 4 differ (stripped box_extrusion2, nurb_box1, vase1, vase3) |
| mass | 19 differ over 8 variant-states |
| ray | 40 differ over 14 variant-states |
| cls | 0 answer values differ; the 29 "differ" (12 variant-states) are cases where both sides refuse with different errors |

- mass sizes:
  - about 2e-16 on blade_hub_1, 2 and 3 and on cliff_1 and 2;
  - 1e-7 on open_shell as_is;
  - 6.5e-9 on RO shared_face_overlap1e-6_union;
  - 3.1e-5 on vase2 as_is (f8 area and centroid).
- ray: the differences are in hit count or structure (no numeric field to compare).
- M0 also refuses, where S answers: 3713 mass answers and about 3286 cls answers, on stripped bodies (no trims, and the raw read path does not create them).

*Which side used the fixed trims.* Every M0-vs-S value difference in box, ray, mass and check also appears in M0 vs MS, and MS = S. So:
- S and main agree whenever main reads a settled body;
- the differences come from settling (trims created or refitted, tolerances raised), and both MS and S used the settled trims;
- M0 used the unsettled ones.

The set is wider than part 1's 25 variant-states because it includes stripped bodies, where M0 has no trims at all.

*Main mutually inconsistent there.* Not measured further. Part 1 showed main is history dependent on unsettled bodies (2 raw builds differed by which face was passed first). This run did not compare main's own families against each other.

**3. Idempotence and order (MEASURED).**
- Settle twice: 298 of 298 give 0 changes and 0 value diffs, and the mark holds.
- Reverse and shuffled face order: 596 of 596 give 0 value diffs.
- Rounds: 274 settles took 1 round and 24 took 2 (the second round changed nothing). None needed 3, none failed to converge, 0 errors.

**4. Cost (MEASURED).**

Fresh settle:
- trim creation: total 52.6 s, median 0.05 ms, max about 4 s (stripped cliff_1, revolution_mold, blade_hub_1);
- face pass: total 6.0 s, median 0.08 ms, max 1.7 s (stripped vase3).

Settle after an edit:

| Edit | Bodies | Settle total |
|---|---|---|
| union | 298 (10 failed) | 0.29 s |
| fillet | 298 (122 failed) | 0.06 s |
| transform | 297 | 0.05 s |
| touch (same-value toggle) | 297 | 0.04 s |

"Settled before" is false after every edit, the no-change touch included. Every write path moves stamps, so the mark is voided and a settle round (no changes) runs.

Reads, full families:

| Read | Time | Trees | Copies |
|---|---|---|---|
| main | 689 s | 7474 | 1212 body, 2614 cache |
| main again | 595 s | 7423 | — |
| S | 779 s | 2951 | 1524 body |
| S again | 792 s | 3525 | — |

Edit reads (box and mass), main vs S:

| Edit | Main | S |
|---|---|---|
| union | 409 s | 721 s |
| fillet | 659 s | 695 s |
| transform | 598 s | 566 s |
| touch | 586 s | 571 s |

S still builds trees and copies in the scratch prototype, through families not yet routed to the read-only tree. lhits (read-only tree reuse) is 2.18M. So these timings say the prototype is no faster yet. They do not measure the design's read cost: that needs every family routed.

**5. Where settle sits in the ops flow (READ / INFERRED).**
- Under the default `Contract::Closed`, every operation result and every `Body::from_record` body reaches its reads unsettled (INFERRED: nothing on those paths creates the missing trims or runs the face pass).
- The candidate points are `finish_proved` (operation results) and `Body::from_record` (imports and round trips).
- `to_record` reads the body, and the proof (`check_solid`) runs inside the operation's finish. So settling at `finish_proved` before the proof puts both the proof and the record on a settled body.
- In the creep chains with S, the settle after each operation and each `from_record` changed entities 36 times out of 1098, and never needed a third round.
- The exact count of ops-flow bodies that reach a read unsettled is not measured: by the inference above it is all of them.

## 6. Follow-up on design S (scratch measurement)

### 6.0 Clean rerun of section 5 (MEASURED)

The rerun was at probe-9: per-test switches and counters, the edge and vertex radial sectors routed (6.2), and the read-only global solve off on the read path. 298 variant-states. Log: `/tmp/claude-zzs3-full.log`.

- **Tree:** 2640 of 2644 faces are the same and 0 differ. The 4 refusals are the `sphere_tangent_box_union` face, because this read path had the read-only solve off. With it on (6.1), 2644 of 2644 are the same.
- **Bodies:** every body was still settled after its reads, and the twins were equal.
- **Answers, MS vs S:** 0 differ in every family.

| Family | Answers |
|---|---|
| cls | 59072 |
| ray | 5926 |
| closest | 8940 |
| mass | 6182 |
| box | 298 |
| check | 298 |

- **S refusals:** 28 cls and 34 ray, all `REFUSE:Tree` on `sphere_tangent_box_union`, which is the same face. The 279 cls and 25 ray `unported` refusals of the first run are gone. The newly routed edge and vertex sector answers equal main's.
- **Answers, M0 vs S:** the same as M0 vs MS in every family (box 0, closest 0, cls 0 differ; check 4; mass 19 over 8 variant-states; ray 50 over 18). So every M0-vs-S difference is settling, not S.
- **Idempotence and order:** idempotence 298 of 298; order 596 of 596 with 0 value diffs.
- **Settle cost:** rounds were 1 for 274 and 2 for 24, all converged. Fresh settle cost 32.4 s of trim creation plus 5.3 s of face pass.
- **Settle after edits:** no edit left the body settled before its settle.

| Edit | Settle |
|---|---|
| union | 0.17 s |
| fillet | 0.04 s |
| transform | 0.03 s |
| touch | 0.03 s |

- **Read phase, full families:**

| Read | Time | Trees | Cache copies | Body copies | Trim builds | Read-only tree hits |
|---|---|---|---|---|---|---|
| main | 459 s | 6120 | 4827 | 884 | 12301 | — |
| S | 661 s | 0 | 934 | 0 | 0 | 6.84M |

- S built no trees in the read phase because the tree check had already built every face's kept tree. S is still slower in total, and 6.2 shows where the time goes.

### 6.1 `sphere_tangent_box_union` and settle as the face pass alone (MEASURED)

**The cause.**
- Today's settle leaves no edge use without a trim. `untrimmed_after_today` is 0 on all 298 variant-states, so the question "which edge use does create_missing leave" has no case.
- The refusal is a vertex use's UV point. `uv_point_in` falls back to the global point solve, and `LazyReading::global_point_solve` gave up behind `&Brep`.
- Main's `global_point_solve` skips the cache lease when the face's trimmed cache is checked out, which is the case inside its own tree build. So main runs a plain surface solve on an evaluation copy, with no point test.
- The read-only build now runs that same solve. Then 2644 of 2644 trees equal main's second-pass tree, with 0 refusals. The solve ran once per affected face (f1 on O, f4 on RO), with 0 give-ups.

**The two routines (READ).**

| | `create_missing_uv_trim_curves` | The face routine (`create_uv_trim_curves_body`) |
|---|---|---|
| Skips an edge use | with no face use, or with its own trim | with no edge, or with its own or its mate's trim |
| Calls | `get_or_create_uv_trim_curve(.., false, false, true)` | `get_or_create_uv_trim_curve(.., true)` |
| Gap fixing | none | yes, raising edge and vertex tolerances |

No seam, pole or zero-length case is involved here.

**Settle as the face pass alone (no create step), against today's settle (cap 6).**
- It creates everything: `untrimmed_after` is 0 in all 894 runs (298 × face order, reverse, shuffle).
- It converges in every run with 0 errors. In face order, 188 needed 2 rounds (one change round, one confirming) and 110 needed 1. None needed 3.
- Cost in face order: 34.2 s, against today's 35.1 s.
- It does not give the same bodies. 20 of the 894 runs differ from today, on 13 stripped corpus bodies:
  - by order: 4 face order, 10 reverse, 6 shuffle;
  - the bodies: shell_fillet2, bottle, vase1, vase2, vase3, torii_unioned, blade_hub_1, blade_hub_2, cyl_cyl_shell, revolution_mold, cliff_1, cliff_2, test8;
  - what differs: edge and vertex tolerances (up to 0.82 relative, vase1) and trims.
- Today's settle is order independent (596 of 596). So the face-pass-only settle is order dependent on those bodies.
- The mechanism (INFERRED from the routines): the face pass creates a missing trim with gap fixing. That raises shared edge and vertex tolerances, which the next face's trims then see, so the face visited first decides.

**Conclusion.**
- Keep the create step. It makes every trim by the one non-adjusting path before any face pass, and that is what makes settle order independent.
- "After settle the face pass creates nothing and changes nothing" already holds for today's settle: its last round changes 0 in all 298, and the tree check above finds nothing left to create.

### 6.4 The 115 unlogged changes in creep chain (b) with a round trip (CONFIRMED by elimination)

The renumbering explanation holds. The evidence:
- Unlogged changes appear only in the round-trip variants (main+rt and S+rt), and only on the fillet steps (steps 5, 18, 16, 30, 4 and 42 of chain (b)).
- The same fillet steps without a round trip have 0 unlogged changes.
- With a round trip, chains (d) and (e) have 0 unlogged changes. Their steps do not change topology.
- The `unlogged` count compares `before`, keyed by the ids before the step, with `after`, keyed by the ids after `round_trip`. `to_kernel` renumbers entities whenever the step changed topology. So an entity that kept its values under a new id counts as "changed" and is not in the step's log.
- Chain (e) round-trips every step and keeps every value exactly. So the round trip itself changes no values.

Nothing in the 115 is a value change that the operation's log misses. The count is an artefact of keying by id across a renumbering. A keyed-by-geometry match was not written, so a per-entity proof was not made. The test is elimination over the variants above.

### 6.3 No-change writes void the settled mark (READ; the empty-round cost is measured in 6.2's run)

Every write moves the write stamps, so the mark is voided even when no value changes. The settle after an edit then runs one face-pass round that changes nothing.

Ops entry points take `&mut Brep`, through `with_body`, which `deref_mut`s and clears proof and template, though they change no body value. Read at origin/main `b280612602`. Each one voids the mark needlessly until its signature takes `&Brep`:

| File | Entry points (line) |
|---|---|
| blend.rs | `edge_faces_along` 2493, `carried_faces` 2621, `tangent_chain` 2697 (each also has a `measure_body` path) |
| blend_surface.rs | `fillet_sheets::inside_points` 468 |
| body.rs | `with_record` 536 (`to_record`) |
| cap.rs | `edges_lie_on_face` 146 |
| continuity.rs | `continuity_report` 308, `principal_curvatures` 643 |
| measure.rs | `face_hit_by_ray` 868, `face_hits_by_ray` 904, `closest_to_members` 1085, `fire_ray` 1129, `face_distances` 1182, `face_distances_within` 1262, `face_normal_at_edge` 1423, `face_normal_at_point` 1453, `classify_points_with_normals` 1491, `point_on_face` 1539, `face_plane_normal` 1590 |
| surface_cvs.rs | `first_side` 112 (the edit `move_face_cvs` 991 is legitimately mutable) |
| sweep.rs | `project_curves` 1678, `face_section` 1741 |
| tangent.rs | `tangent_face_chain_ordered` 179 |

These are reads in today's code, but their kernel callees are on #21072's blocked-by table: the kernel read creates trims, so the `&mut` is real today. Each becomes `&Brep` once its kernel callee reads the settled body behind `&Brep` (the 6.2 routing).

The `measure_body` callers read a copy, so they do not void the mark but they pay for a body copy per call. They are:
- `edges_share_wedge`, `edge_faces_along`, `carried_faces`, `tangent_chain`, `band_room`;
- `brep_volume`, `check`, `check_solid`.

The real edits (`blend` 1734, `chamfer_sides` 2882, `fillet_variable` 3207, `SmBrep::Transform` 374, `turn_over_sheet` 396, the offset entry points, `Draft`) should void the mark.

### 6.2 Read routing behind `&Brep`: per-family call bench, memory, empty round (MEASURED)

Every family now reads the settled body behind `&Brep`:
- **box:** `LazyBrep::Lazy`.
- **cls:** `BrepClassifier::point_3d_classify`, using the lazy edge and vertex radial sectors.
- **ray:** `ray_fire_lazy`.
- **cp:** `closest_to_members_lazy`.
- **mass:** `face_centroid_prepared`, which was already a read.

Bench setup:
- Two settled twins per body.
- 100 calls per family, with the 1st call and calls 2 to 100 timed separately.
- Main keeps its classifier and ray copy across calls. S makes a fresh handle every call.
- 182 bodies, 172 for mass. The 10 skipped are five zero-face `_inter` bodies, as_is and stripped. The test harness divides by the face count there; this is not a kernel issue.

Column key for the bench table:
- **trees / cache copies / body copies:** summed over all bodies.
- **lhits:** kept-tree lookups (hits).

| Family | Mode | 1st call (s) | Calls 2–100 (s) | trees | lhits | cache copies | body copies |
|---|---|---|---|---|---|---|---|
| box | main | 325.7 | 35604.6 | 0 | 0 | 0 | 18018 |
| box | S | 338.5 | 35032.9 | 0 | 329.9M | 0 | 0 |
| cls | main | 0.08 | 3.67 | 504 | 0 | 834 | 0 |
| cls | S | 0.01 | 3.57 | 368 (350 lazy) | 19474 | 376 | 0 |
| ray | main | 0.06 | 13.95 | 11760 | 0 | 11244 | 0 |
| ray | S | 0.01 | 3.04 | 714 (674 lazy) | 30405 | 682 | 0 |
| cp | main | 0.09 | 6.53 | 24968 | 0 | 0 | 18018 |
| cp | S | 0.03 | 2.42 | 64 (64 lazy) | 87450 | 0 | 0 |
| mass | main | 0.59 | 47.29 | 0 | 0 | 0 | 0 |
| mass | S | 0.58 | 47.32 | 0 | 0 | 0 | 0 |

A fresh main handle per call (main_fresh5, calls 2 to 6) costs cls 0.61 s and ray 0.83 s, with 860 and 910 body copies.

Totals over the read phase: main 36002.6 s, S 35428.4 s. The tight box is almost all of it: 100 calls on the vase bodies take hours each way. The vase bodies dominate both columns.

**Answers.** box, cls and mass: 0 differ. The only differences are on `sphere_tangent_box_union`, where S refuses with `REFUSE:Tree`:
- ray: 40 differ;
- cp: 114 differ.

This run predates the read-only solve in variant P (see 6.5).

**Memory** (kept trees, `Tree` and arena capacity bytes, without control nets):
- Largest face: test8 f1, 9.43 MB.
- Largest bodies:
  - test8 stripped: 18.9 MB (2 faces).
  - test8 as_is: 18.9 MB.
  - blade_hub_3 stripped: 15.3 MB (46 faces, median 259 kB).
- Refused trees: 0.

**Empty round after a no-change write.** On 172 bodies, the round changed nothing on every body.
- Largest: blade_hub_3 (504 edge uses) takes 0.84 to 0.86 ms; blade_hub_1 and cliff_1 take about 0.8 ms.
- The mark check takes 0.012 to 0.017 ms.

### 6.5 Where S's read time goes (MEASURED, temporary time spans)

The 661 s against 459 s in 6.0 compares main on the raw starting body (M0) with S on the settled body. It is not a cost of S.

Setup:
- 16 bodies: the vase bodies, variable_round, torus_ball_diff, slot_through_holes, plate_drilled_diff and round_drilled_union, each as_is and stripped.
- Per family: M0, MS (main on the settled twin) and S, each run twice (first pass, then again).

| Family | M0 first / again (s) | MS first / again (s) | S first / again (s) | Where S's time goes |
|---|---|---|---|---|
| box | 434.9 / 394.2 | 395.7 / 371.4 | 379.1 / 375.0 | tight box 375 s; 3.32M tree lookups 1.15 s |
| mass | 10.6 / 8.2 | 388.4 / 369.2 | 374.2 / 369.7 | tight box 366–371 s; lookups 1.15 s |
| cls | 12.5 / 9.7 | 0.9 / 0.9 | 0.9 / 0.9 | solve 0.27 s, body box 0.07 s, cache copies 0.02 s |
| ray | 8.5 / 8.5 | 0.5 / 0.5 | 0.03 / 0.03 | — |

Findings:
- **Lookup cost.** One kept-tree lookup costs about 0.35 µs: the 32-stamp key, the lock, the rekey comparison and an Arc clone. Kernel tolerances and fixes are compared by value, and nothing is hashed. 6.84M lookups cost about 2.4 s.
- **Mass on M0.** On the raw vase bodies, main refuses 9 mass reads, and with them the body's tight box. That box is 70–90 s per call on a vase body, the same work in main and in S.
- **Spread.** Within one mode, the first-to-repeat spread on these bodies is ±10–15%. Read-only solve, BrepCache, radial sector and Shape spans are all at noise level.
- **The one systematic S loss.** cls on slot_through_holes takes 20 ms against 16.5 ms. classify_lazy and ray_fire_lazy recomputed the loose body box each call, which main's classifier keeps.

Fix (scratch): the loose and the tight body box are kept in the body's lazy slot under the trees' key and dropped by any write. The rerun is in 6.6.

### 6.6 Rerun with the kept boxes and the read-only solve in variant P; zero-refusal confirmation (MEASURED)

This is the full zz_s and zz_s4 run at scratch probe-11, which keeps the body boxes in the lazy slot and turns the read-only global solve on in P.

**Zero refusals**
- Tree: 2644 of 2644 faces the same, 0 differ, 0 read-only errors.
- MS vs S answers: 0 differ and 0 refused in every family. That covers box 298, cls 59100, ray 5960, closest 8940, mass 6182 and check 298.
- The 28 cls and 34 ray `REFUSE:Tree` on `sphere_tangent_box_union` are gone.
- Idempotence 298 of 298; order 596 of 596; every body converged (274 in one round, 24 in two).

**Read phase as in 6.0 (M0 against S).** Server2 was more loaded than in 6.0, so compare within this run only.
- main 694 s, main again 719 s.
- S 525 s, S again 47.5 s.

**Per family, 182 bodies (zz_s4):** totals in seconds, first pass / again.

| Family | M0 | MS | S |
|---|---|---|---|
| box | 551.3 / 574.5 | 572.4 / 523.4 | 428.7 / 0.0 |
| cls | 63.9 / 65.2 | 9.3 / 9.7 | 8.9 / 8.6 |
| ray | 40.4 / 42.4 | 4.6 / 4.7 | 0.9 / 0.9 |
| mass | 19.0 / 19.7 | 498.6 / 483.6 | 24.6 / 24.2 |

S is at or below MS in every family on both passes.
- **mass caveat:** S's mass time is low because the tight box was kept from the box family that ran before it. A mass read alone on a fresh settled body pays the tight box once, as main does.
- **M0 mass:** M0's mass is low because the raw body refuses most mass reads (3716 refused).

**Where S is still slower than MS on single bodies**
- **The first box call: S is slower on 108 of 182 bodies.** Examples:
  - test8 as_is: 115 ms against 66 ms;
  - revolution_mold: 36 ms against 28 ms.

  The cost is the tree build (`tree_build` span: 52 ms and 8 ms). In S, the face point classification inside the tight box reads the face's kept tree, building it on first use. Main's `point_classify_live` walks the loops with no tree. The trees are kept and reused by every later cls, ray and closest read, so the build moves earlier rather than adding work. Every repeat box call is then 0.001–0.005 ms against main's full recompute.

  Matching main on that first call would mean classifying by loop walk until the face's tree exists. That makes the classification path depend on read order, so it was not done.
- **The other 30 slower rows are noise.** Each is one pass only, and the other pass reverses it:
  - cliff_2 cls: MS 423/715, S 615/424;
  - test8 mass: MS 1771/1946, S 2022/1599;
  - revolution2 mass: MS 227/225, S 219/277;
  - cylinder cls: MS 38.6/38.6, S 38.1/43.0.
