# 38. Why 1,275 of 2,104 bodies are intermediate

Base `da240580a4`. Probe: report 36's scratch probe (commits `2808b8d737`, `b264bd9d3a`, `0eb6d15fcf`) plus one scratch commit `e1e9a0dea3` (local branch `cache-whybodies-m`, never pushed to GitHub or the fork). Run: the build server, 69 processes (the report-36 corpus: 63 designs with bodies), `SOLIDCRAFT_NO_MODEL_CACHE=1`, no counterfactual builds. Data, runner and scripts: `count-kit/38-data/` (`bodies.jsonl.gz`, `run.log`, `run.sh`, `why.py`, `fate.py`).

Labels: **MEASURED** means from this run's logs. **READ** means from the code at the base commit. **INFERRED** means reasoned from both.

## 0. Plain answer

The run reproduces report 36's counts exactly: 2,104 bodies, of which 1,039 are intermediate and 236 are operand copies, so 1,275 in all (MEASURED).

| group | bodies | what they are | time to make them |
|---|---:|---|---:|
| **Necessary** with the current history design | **402** | the private tagged copy, one per operand (`smlib_history_tag`) | 169 ms |
| **API shape** | **663** | 405 copies taken only to count and tag (`smlib_body_ordered`); 236 second copies of a copy that is already private (`core/boolean.rs:43`); 22 copies taken only to check | 281 ms |
| **App storage** | **210** | live forms rebuilt from the record and then dropped (197 `BrepFromFlat`, 13 `brep::to_kernel`) | 65 ms |

All three groups are MEASURED for counts and time. 30 of these bodies belong to a single trial union (holes-pattern-chamfer `tools_clearly_apart`): 10 in the necessary group and 20 in the API-shape group.

- **Writes.** 1,256 of the 1,275 (98.5%) have no value written after they become a body. None of the 19 that do has an attribute written (MEASURED, write stamps). The only "value" of a tagged copy is the tags written while it is made. Operand copies are written by the kernel after they are taken (READ). Every other intermediate is a read-only scaffold.
- **Cost of the intermediates.** Making all 1,275 takes **515 ms**. Feature evaluation takes **39.24 s** over 628 evaluations, so the intermediates cost **1.3%** (MEASURED).
- **Cost of the surrounding work.** All whole-body copies, rollback snapshots, rebuilds from the record and record conversions inside features cost **1.76 s**, which is **4.5% of evaluation** (MEASURED):
  - 1,376 raw copies: 607 ms
  - 492 snapshots: 116 ms
  - rebuilds: 578 ms
  - record conversions: 463 ms

  This is a real cost but not where evaluation time goes. It is concentrated in RectangularPattern (27%), Chamfer (13%), Extrude (10%) and Fillet (8%).

## 1. The life of a body

These rules are READ from `crates/kernel/src/body.rs`.

- An app `Body` keeps the **record** (`RecordCell`, an `Arc` of the schema-4 `Brep`). That record is what is saved, cached and shared.
- Two lazy live forms hang off the body, both shared by clones (`Arc<FormCell>`):
  - the SMLib form (`smlib()`, built by `Brep::from_record`, frame `sm:BrepFromFlat`);
  - the ops form (`ops_body()`, built by `solidcore_ops::Body::from_record`, frame `brep::to_kernel`, body.rs:1099).
- The body is never mutated in place.
- `Body::from_smlib_record` (body.rs ~786) deliberately drops an operation's live result after converting it to a record (`FlatFromBrep`). The export is not bit-exact, so "every operation starts from the record's import": a fresh evaluation then equals a reopened file. The next operation therefore rebuilds the live form.

**Extrude that joins an existing body** (up-down-join; bodies listed in birth order; MEASURED):

1. The base extrude result is converted to a record and dropped.
2. The tool extrude result is converted to a record and dropped.
3. The target's SMLib form is rebuilt from its record (`BrepFromFlat`). The timeline keeps it alive.
4. The tool's SMLib form is rebuilt from its record and is intermediate.
5. For each operand, `smlib_body_ordered` takes a `try_clone` (`SmApiBrepCopy`). The clone is read by `counts`, copied by `tag`, then dropped. This is intermediate ×2.
6. For each operand, `smlib_history_tag` makes the tagged copy. This is intermediate ×2.
7. `SmApiBoolean` → `union_copies` borrows the tagged copies and copies them again internally. Those internal copies are hidden raw copies, not bodies.
8. The result is checked, its history read (`smlib_history_read`), converted to a record (`FlatFromBrep`) and dropped.
9. A later query or measure rebuilds the result from its record.

**Fillet** (filleted-shelled-box; MEASURED):

1. The input's SMLib form is rebuilt from its record and kept.
2. `try_clone` (counts, tag) is made and dropped.
3. The tagged copy is made and dropped.
4. The blend copy (`blend.rs:305`) is edited in place by the blend and becomes the result. It is used for edge picks, check, `history_read` and `FlatFromBrep`, then dropped.
5. An ops form is built from the record (`brep::to_kernel`) for face queries (`face_distances_within`, `closest_to_point`). These queries **write** to it (edge uses).

Per fillet evaluation (MEASURED): 4.4 raw whole copies, **11.4 rollback snapshots** (mostly `face_plane_normal`), 1.5 rebuilds and 1.0 record conversion.

**One RectangularPattern instance** (holes-pattern-chamfer; MEASURED):

1. Once per pattern, a **trial** `union_all` of the 10 tools (`tools_clearly_apart`) runs: 10 `try_clone`s, 10 tagged copies and 10 operand copies, giving a 25-face result that is recorded and dropped. The answer is "not apart", so the pattern falls back to cutting one tool at a time.
2. Each instance:
   - The accumulating target is rebuilt from the previous cut's record (`BrepFromFlat`).
   - The target gets `try_clone` → tagged copy → operand copy.
   - The tool gets `try_clone` → tagged copy → operand copy.
   - The cut result is recorded and dropped.

That is about 7 intermediate bodies per instance. Per pattern evaluation (MEASURED): 25.8 raw copies, 7.2 rebuilds, 4.7 record conversions.

## 2. Each source, and why it exists

| source | bodies (MEASURED) | class | reason (READ unless marked) |
|---|---:|---|---|
| `smlib_body_ordered` copy (`native.rs:60,102-111`: `body.smlib()?.try_clone()`, then `counts()`, then `.tagged(i)`) | 405 | **(a) API shape, no real change** | `counts` and `tag` both take `&Brep`, and `tag` copies anyway. The owned clone exists only because the function returns an owned `solidcraft_smlib::Brep`. A second `try_clone` is taken if the counts differ (line 110). 4 of the 405 show writes after birth (MEASURED), all from `counts` running under a `&mut` lock (`with_body`), which are lazy writes. |
| tagged copy (`core/history.rs:205-232`) | 402 | **(a) required by ownership, real writes** | Tagging writes one attribute per face and per edge (median 53 attribute writes at birth, MEASURED). The shared form is shared by `Body` clones and by threads (`Arc<FormCell>`), so the tags must go on a private body. |
| operand copy (`core/boolean.rs:41-44` `copies()`; also `union_all`) | 236 (194 taken by `SmApiBoolean`, 42 by `union_all`) | **(a) API shape** | `ops::boolean(a, b)` consumes both operands and the kernel really edits them: the first is edited and the second moved in (READ, ops boolean.rs:151-178). One private copy per operand is necessary. **The tagged copy already is that private copy.** The extra copy exists because the public `solidcraft_smlib::Brep` boolean takes `&self`/`&other`, so the core layer cannot consume what the app owns. |
| rebuilt SMLib form (`BrepFromFlat`) | 197 | **(d) rebuild** | This is the record-as-truth rule (body.rs ~786). 85 are used only for `bounds`, 99 only to be copied, 7 for `edge_face_distance`, the rest for check, classify and counts (MEASURED). |
| rebuilt ops form (`brep::to_kernel`) | 13 | **(d) rebuild**, then **hidden writes** | `solidcore_ops::Body` is `Mutex<Held>` because "the kernel's operations take `&mut Brep`, even to measure" (ops body.rs:39). 10 of the 13 have edge uses written by `face_distances_within`, `closest_to_point` and similar (MEASURED). These are lazy writes (trims), not value changes. |
| check copy (`kernel/validation.rs:237,353` `try_clone` then `check`) | 22 | **(b) query on a copy / (e) defensive** | `check` takes `&`. The copy protects the shared form. ops already puts every `with_body` call inside `with_rollback` (READ, session.rs:704-721). |
| trial (`tools_clearly_apart` → `union_all`) | 30 (overlapping the rows above) | **(c) trial** | The union is run only to count material regions, and the result is discarded. Only one design hits it in this corpus (MEASURED). Other boolean-as-test sites were instrumented and produced no bodies here: `copy_misses`, `one_body_of`, the intersect checks at eval.rs:11680/11706, `sheet_boolean_unchanged`, `eval_frame.rs:379`, `eval_thread.rs:1019`, `frame_multi_trim.rs:169`, eval.rs:8514. |

**Hidden copies that are not bodies** (MEASURED; class READ):

- **492 rollback snapshots, 116 ms.** `with_body` wraps every call, read-only queries included, in `Brep::with_rollback`, which takes a snapshot (a full copy). This is defensive (e) and a rollback **already exists** for it. The biggest user is `face_plane_normal` (265 snapshots, 88 ms, a pure query), then `blend` (102), `face_distances_within` (60) and `closest_to_point` (18).
- **274 raw copies inside operations, about 152 ms.** These are 205 inside `SmApiBoolean` (`union_copies` re-copying borrowed operands, `Witnesses` copying the other operand, `measure_body`) and 41 inside blend.

**`smlib_history_tag` in detail** (READ, `crates/smlib/src/core/history.rs`, ported from the fork's `capi/smlib_history.cpp`):

- `tag(body, operand)` locks the body (`with_body("smlib_history_tag")`) and makes `brep_copy` of it. On the **copy**, for each face `i` it adds one application attribute `Tag` (id `USER_1+7`, behaviour Copy) holding:
  - its origin `(operand, i)`;
  - the set of its boundary edge indices at tag time, used to tell later whether the face is still whole.

  For each edge `i` it adds a `Tag` with the origin and 5 sample points (ends, ¼, ½, ¾), used later to tell whether the edge moved.
- The input is never written. The copy is returned as a new Body.
- The attributes ride through the kernel's notify handlers: split (children join the origins), merge (the union of origins), copy (when a boolean copies the second operand into the first), and coincident (an entity landing on another's joins its origins).
- After the operation, `read(result)` (`smlib_history_read`) reads every face's and edge's origins. It classifies each face as Unchanged, Modified, Split or Merged from the number of origins, the number of result faces sharing them, and whether the boundary is still the original edges. `moved()` checks the edge samples with `edge_closest_point`.
- New faces (for example a boolean's intersection edges) carry no tag. That is how Generated faces are told apart.
- Persistent face names are built from these origins (docs/kernel-smlib-history.md, "names are built from origins").
- So the tag copy is the history mechanism's input. It needs a private copy only because the tags live **in the body's attribute store**. A side table keyed by entity, carried through the same notify handlers, would need no copy (INFERRED, class (f): a design change in the kernel's history path, not in the app).

**Correction to report 36.** Report 36 described "104 `SmApiBoolean` results only queried". These are not boolean results. They are **tagged union operands** (born `sm:smlib_history_tag`) that `union_copies` borrows and copies internally (MEASURED).

## 3. Costs

Times are per call frame, nested frames excluded. Raw copies are every `brep_copy_into` (outermost) and are charged to the frame running at the time. Snapshots are `Brep::snapshot`. Wall time for the 69 processes was 97.9 s, of which feature evaluation was 39.24 s (MEASURED).

Per feature kind (ms; the right-hand columns are counts per evaluation; MEASURED):

| kind | evals | eval | copies | snapshots | rebuild | record | share | copies/ev | snaps/ev | rebuilds/ev | records/ev |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Fillet | 30 | 9194 | 181 | 107 | 358 | 102 | 8.1% | 4.4 | 11.4 | 1.5 | 1.0 |
| Combine | 13 | 8169 | 32 | 0 | 10 | 10 | 0.6% | 11.7 | 0 | 2.8 | 1.6 |
| Shell | 9 | 4601 | 25 | 2 | 14 | 14 | 1.2% | 8.3 | 4.7 | 4.0 | 2.3 |
| Thread | 1 | 4240 | 7 | 0 | 6 | 3 | 0.4% | 30 | 2 | 10 | 8 |
| Extrude | 100 | 3694 | 153 | 2 | 49 | 161 | 9.9% | 4.1 | 0.3 | 1.3 | 1.9 |
| Sweep | 5 | 1801 | 4 | 0 | 66 | 4 | 4.2% | 4.0 | 1.2 | 2.4 | 1.6 |
| Revolve | 7 | 1235 | 4 | 0 | 6 | 5 | 1.3% | 1.0 | 1.3 | 1.3 | 1.1 |
| Chamfer | 7 | 775 | 8 | 1 | 3 | 88 | 12.9% | 3.0 | 2.1 | 0.9 | 1.0 |
| Loft | 12 | 641 | 15 | 1 | 4 | 5 | 3.8% | 3.2 | 1.0 | 0.9 | 1.5 |
| RectangularPattern | 9 | 603 | 93 | 0 | 30 | 39 | **27.0%** | 25.8 | 0.7 | 7.2 | 4.7 |
| Mirror | 3 | 517 | 26 | 1 | 7 | 7 | 7.9% | 16.0 | 4.0 | 5.0 | 5.0 |
| **all features** | 628 | 39237 | 607 | 116 | 578 | 463 | **4.5%** | | | | |

The Fillet rebuild figure is mostly ops forms (`brep::to_kernel`, 328 ms) built for face queries.

Worst designs, absolute (eval ms / copy+snapshot+rebuild+record ms; MEASURED):

| design | eval | overhead |
|---|---:|---:|
| round_pieces_swept_under_a_degree | 2687 | 381 (14.2%, mostly rebuild) |
| battery-tpu-case-v2 | 3470 | 243 (7.0%; 162 snapshots) |
| battery_case_pleats | 3500 | 219 (6.3%) |
| pi-case-0.8 | 1410 | 117 (8.3%) |
| oblique_cuts_through_chamfered_ring | 2348 | 95 (4.1%) |
| pi-case | 636 | 75 (11.8%) |
| sensor_enclosure | 752 | 75 (9.9%) |

Worst by share, for designs with evaluation over 100 ms:

| design | share |
|---|---:|
| lid_ribs_flush_with_window | 22.3% |
| step_export_screw_hole_enclosure | 17.4% |
| step_export_flanged_enclosure | 15.6% |
| holes-pattern-chamfer | 12.9% |

Outside features, one STEP import (rod_doubled_edge) spends 2.70 s in a single `FlatFromBrep` on a worker thread. That is a record conversion outlier worth its own look; it is not an intermediate (MEASURED).

**Write stamps** (every `Stamped` field's mutable-access count at birth and at end; MEASURED):

- 19 of 1,275 intermediates (1.5%) had any field written after birth:
  - 10 ops forms written by face queries;
  - 4 rebuilt SMLib forms and 4 ordered copies written under `counts`/`check` locks;
  - 1 check copy.
- 8 of the 19 were edge-use (trim) writes. 0 were attribute writes. 0 were tolerance writes.
- Values that really change are only:
  - the tagged copies (attributes, written before birth);
  - the operand copies (edited by the kernel after they are taken; READ).

## 4. What each change removes

Counts are MEASURED from the logs. The make times are the bodies' own birth frames.

| change | bodies removed | time (making them) | also removes (not bodies) | architecture constraint |
|---|---:|---:|---|---|
| **(i)** shared-reference reads with no hidden writes (`counts`, `check`, `tag` take a shared form under a read lock; ops queries do not take `&mut`) | **427** (405 ordered copies, 22 check copies) | 177 ms | 492 query snapshots (116 ms); the lazy-write locks on 10 ops forms | The live forms are shared by `Body` clones and worker threads (`Arc<FormCell>`, the `Send` model eval). Reads must be truly read-only (lazy trims computed and cached under interior mutability, or precomputed) before a shared lock is safe. This is wasm-compatible: there is no threading assumption beyond what exists. |
| **(ii)** operations borrowing operands they do not change, and consuming the private tagged copy for those they do | **236** operand copies | 104 ms | about 205 hidden copies in `SmApiBoolean` (66 ms) | The layer seam: the app talks to solidcore only through `solidcore-ops` (docs/layers.md, "one seam"). Consuming owned operands needs a by-value `boolean(self, other)` on the public `solidcraft_smlib::Brep`. Ops already has one (`ops::boolean(Body, Body)`). |
| **(iii)** the app keeping the live body after an operation instead of rebuilding it from the record | **210** rebuilt forms | 65 ms | all in-feature rebuilds of kept bodies too (578 ms in all) | **The record is the source of truth.** Every operation starts from the record's import so that a fresh evaluation is bit-identical to a reopened file (body.rs ~786; docs/kernel-smlib-history.md "reopens … with bit-identical volumes, areas and face names"). Keeping the live body breaks that unless the SMLib↔record round trip becomes exact. Undo and history replay from records, and the model cache stores records, so they are unaffected only if the record stays canonical. Records would still be needed (463 ms) but could be made lazily. |
| **(iv)** a non-destructive query (an "are these apart / does this meet" test) replacing the trial boolean | **30** (overlapping (i), (ii) and the tagged copies) | 2 ms, plus about 27 ms of trial `union_all` | | None architectural. It needs a kernel query (an interference or region count) exposed through ops. "Model decisions use exact geometry" (AGENTS.md) still holds. |

(i), (ii) and (iii) do not overlap. (iv) overlaps them by 10 + 10 bodies plus 10 tagged copies. With all four changes, the bodies left are the **392 tagged copies**, which are the history input. A side-table history (class (f)) would remove even those.

## 5. Bottom line

- **402 are necessary** under today's attribute-based history: 169 ms.
- **663 are due to API shape**: owned returns, `&self` booleans and `&mut` locks used for reads, 281 ms.
- **210 are due to app storage** (record-as-truth rebuilds): 65 ms for the intermediates themselves, 578 ms counting every rebuild inside features.

Together the intermediates cost 1.3% of feature evaluation. The wider copy, snapshot, rebuild and record traffic costs 4.5%. RectangularPattern (27%) and Chamfer and Extrude (about 10–13%) are where removing them would show (MEASURED). The largest single removable item that is not a body is the query snapshot: 492 snapshots, 116 ms, behind changes (i) and (e).

Probe limitations:

- A frame's time charged to a touched body is the whole frame's time, so the `ns_by_call` sums over-count shared frames. The tables above use frame totals and birth frames only.
- The trial context is thread-local, so trials run on worker threads would be missed. None was seen.
- Raw copies charged to "(other thread)" (13) are omitted from the per-kind tables.
