# 36. Intermediate bodies and what "every body carries all its UV trims" would cost

Date: 2026-11-24. Base: origin/main `da240580a4`. Scratch probe only: nothing pushed upstream, no issue or PR touched.

Labels: MEASURED means counted by the probe on the corpus below. READ means read from the code. INFERRED means a conclusion drawn from the two.

## Plain answer

- **MEASURED.** Regenerating 63 real designs from scratch created **2,104 solidcore-ops bodies**:
  - **1,275 (61%) were intermediate**: consumed as an operand, or discarded without ever being recorded, measured or kept.
  - 473 (22%) were recorded and then dropped. These are the operation results the app turns into its record.
  - 344 (16%) were still alive at the end (kept).
  - 12 (<1%) were only measured.
  - The median design made one intermediate body for every kept or recorded one. The range was 0% to 77%.
- **MEASURED.** Only **336 of 2,104 bodies were born with all their trims**. Across all of them, 128,704 of 300,636 face edge uses had a trim at birth (43%).
- **MEASURED.** Today the app builds **38,654 trims** in total (about 1.2 s of 105 s wall time).
- **MEASURED.** If every body carried all its trims from creation:
  - The intermediate bodies alone would need **54,401 more trim builds** (about 1.8 s indicative), about 1.4 times everything built today. None of these trims is ever read.
  - Kept bodies would add about **12,400 builds** that are never needed (18,191 missing at birth, of which 5,823 were later built anyway).
  - In all, roughly **2.7 times today's trim builds**.
- **MEASURED.** **Zero** of the counterfactual builds (86,700 of them) triggered a certified deviation bound. The certified-bound split is therefore entirely "unbounded". Certified bounds (6,660 of them, 15.5 s, which is 12 times the build time) come from operations that check trims (booleans, `union_all`, shell, screw sweep, `FlatFromBrep`), not from filling missing trims.
- **INFERRED.** "Every operation returns a body with all its trims" would more than double trim building, for bodies most of which nobody reads. The trims actually needed are already built where they are consumed: recording, booleans on their operands, and measuring. An eager policy is not justified by this corpus. A lazy policy should stay, or trims should be made at the record and carried into the rebuilt body (see section 3).

## What was driven, and what was not

- **MEASURED.** The driver was `solidcraft-cli run` with `SOLIDCRAFT_NO_MODEL_CACHE=1`, one process per design, through the release CLI built on the build server. This is option (1) of the brief: app regeneration of real designs. The inputs were:
  - 10 action-matrix JSON recipes;
  - 37 regression recipes from `examples/regressions`;
  - 2 acceptance recipes;
  - 4 action-matrix `.sc` designs and 5 engine fixture `.sc` designs (fully re-evaluated because the cache is disabled);
  - 11 STEP regression parts, imported.
- **MEASURED.** Each run evaluated every feature, then displayed (`world_state().warm_meshes`), saved (`file.save_as`) and measured (`inspect.measure`), then dumped the log.
- **MEASURED.** The corpus ran twice: once without the counterfactual for clean timings, once with `ZZ_CF=1`. Both runs had identical counts (2,104 bodies, 38,654 builds, 6,660 bounds), so the counterfactual does not disturb what it measures.
- **MEASURED.** Three recipes failed in the app itself and produced no log: `fuzz_seed_158_shell` (shell refused), `fuzz_seed_185_pattern` (boolean not closed) and `fuzz_seed_73_boolean_join`. That leaves 63 of the 69 inputs with logs. The other three failures produced no records.
- **Not driven: (2) the app test suites.** Regeneration of real designs succeeded, and the suites mostly re-run the same recipes, so the extra roughly 40-minute build-and-run was not spent.
- **Not driven: (3) the ops test suite.** It does not show the app's lifecycle (record, rebuild, measure), and that lifecycle is what the question is about.
- **Not driven: interactive display after every command.** Display ran once at the end. **INFERRED:** this changes nothing for ops bodies, because display reads the app's record (its pcurves), not an ops body (READ: `crates/kernel/src/body.rs` keeps `RecordCell` and builds `smlib` and `ops` lazily from it).

## Instrumentation (scratch; the patch is kept privately at `count-kit/36-data/probe.patch`)

Kernel probe (`solidcore/crates/solidcore/src/zz_probe.rs`, new file):
- Thread-local counts of builds and certified bounds, with time measured on the outermost call only, split by phase.
- A per-edge-use build log used for the counterfactual.
- A trim read set keyed by edge-use arena address.
- A census of a body's trims: own, own-or-mate, and missing on face edge uses.

Kernel hooks:
- `topology/edgeuse/p3.rs:501` (`create_uv_trim_curve`, build timer).
- `topology/edgeuse/p3.rs:2703` (`trim_deviation_bound`, bound timer).
- `topology/edgeuse/p1.rs:631,639,676` (`uv_trim_curve` / `uv_trim_curve_pointer` reads).
- `topology/brep/p2.rs:508` (the `create_missing_uv_trim_curves` scan is not counted as reads).
- `topology.rs` and `stamped.rs`: arena address and faces stamp.

Ops log (`solidcore/crates/solidcore-ops/src/zz_bodylog.rs`, new file, with a global Mutex registry that is scratch only):
- `body.rs:54-59`: `Held` carries an id, and `Drop` records the end as "dropped".
- `body.rs:171-189`: `BodyGuard` takes a census at lock and at drop. Trims created between the two are counted, along with work done while locked. A change in the faces stamp counts as "edited".
- `body.rs:247-353`: birth with `#[track_caller]` in `held`, `primitive`, `copy`, `from_kernel`, `from_record`. `take_held` and `into_inner` record the end as "operand taken".
- `body.rs:647,700,728,828`: proof and `to_record` phases.
- `session.rs:525` (`run_with`): call frames.
- `session.rs:622,714,752,795-825`: door events (edit, read_else_edit, read, measure copy).

App hooks:
- `crates/smlib/src/core.rs:235-237`: an `sm:<call>` frame for each SMLib-bridge kernel call.
- `crates/doc/src/eval.rs:13747`: a feature-kind tag set in `eval_feature`.
- `apps/solidcraft-cli/src/main.rs`: display, save and measure tags, and the dump.

Counterfactual:
- At birth, and on any edit that newly loses trims, the probe makes a same-id scratch copy (`clone_standing_for_self`). It runs `create_missing_uv_trim_curves` on the copy with a fresh kernel and logs each edge use built (its time and its bounds).
- "Extra" counts the edge uses missing at birth that were still missing when the body ended.

## 1. Bodies by fate (MEASURED, CF run; counts identical without CF)

Classification of each body by what happened to it:
- **kept**: alive when the run ended.
- **operand**: its B-rep was taken by an operation.
- **recorded**: `sm:FlatFromBrep` or `brep::to_record` ran on it.
- **measured**: only `sm:bounds` / `mass_properties` / `classify_point` ran on it.
- **intermediate**: everything else that was dropped.

| fate | bodies | edge uses | trims at birth | trims built on it | counterfactual builds | extra (never built for real) | extra ms |
|---|---|---|---|---|---|---|---|
| intermediate: queried or bookkept, then dropped | 578 | 75,354 | 25,482 | 0 | 24,936 | 24,936 | 1,481 |
| intermediate: copied, then the original dropped | 461 | 58,232 | 18,562 | 0 | 19,883 | 19,883 | 236 |
| intermediate: operand taken | 236 | 27,056 | 7,892 | 0 | 9,582 | 9,582 | 89 |
| recorded (operation result, then dropped) | 473 | 68,016 | 39,866 | 15,400 | 14,082 | 0 | 0 |
| kept (alive at end) | 344 | 71,770 | 36,750 | 5,823 | 18,191 | n/a (alive) | n/a |
| measured only | 12 | 208 | 152 | 0 | 28 | 28 | 0.1 |

The "extra ms" column is indicative. The CF run is about 1.3 times slower overall. The intermediate-queried figure includes one outlier body made on a non-eval thread, at about 1.2 s.

Where intermediates come from:
- **MEASURED.** 427 are `sm:SmApiBrepCopy` copies that go through `sm:smlib_history_tag` and `sm:counts` and are then dropped (history bookkeeping).
- **MEASURED.** 273 `SmApiBrepCopy` operand copies are born under `smlib_history_tag`. 236 of them are then taken by `SmApiBoolean`/`union_all`.
- **MEASURED.** 188 bodies are rebuilt from records (`sm:BrepFromFlat`), copied, and then dropped.
- **MEASURED.** 104 `SmApiBoolean` results are only queried.

Per feature kind (MEASURED; the tag is the feature being evaluated on that thread):

| feature | bodies | intermediate | recorded | kept | extra trims on intermediates | extra ms |
|---|---|---|---|---|---|---|
| Extrude | 660 | 392 | 193 | 74 | 18,279 | 142 |
| RectangularPattern | 317 | 261 | 42 | 9 | 12,510 | 141 |
| Combine | 176 | 126 | 21 | 29 | 916 | 5 |
| Fillet | 134 | 60 | 30 | 44 | 7,146 | 230 |
| Box | 130 | 79 | 37 | 14 | 4,588 | 15 |
| Shell | 111 | 72 | 21 | 18 | 3,228 | 23 |
| (measure command) | 81 | 0 | 0 | 81 | 0 | 0 |
| Cylinder | 77 | 46 | 23 | 8 | 1,144 | 2 |
| Mirror | 66 | 48 | 15 | 3 | 3,324 | 15 |
| (other thread) | 60 | 18 | 17 | 25 | 99 | 1,224 |
| Loft | 57 | 34 | 18 | 5 | 1,512 | 5 |
| Hole | 47 | 29 | 11 | 4 | 358 | 0.5 |
| Thread | 42 | 31 | 8 | 1 | 124 | 0.3 |
| Sweep | 36 | 23 | 8 | 4 | 280 | 0.3 |
| Chamfer | 27 | 14 | 7 | 6 | 276 | 0.7 |
| Revolve | 23 | 8 | 8 | 7 | 137 | 0.7 |
| CircularPattern | 18 | 15 | 2 | 1 | 120 | 0.2 |
| others (Sphere, SnapFit, Trim, Thicken, Sketch, Move, flanges, SurfaceLoft) | 42 | 19 | 12 | 11 | 360 | 3 |

- **MEASURED.** Patterns are the worst case. 82% of their bodies are intermediate: per-instance copies, history-tag copies and operands.
- **MEASURED.** No ops proof ran on this corpus: `proved_at_birth` is 0 for every body. The proof (`finish_proved`) is used only by `trim.rs` (READ), and no design here used that path.

## 2. Trims on intermediate bodies, and the counterfactual

- **MEASURED.** Intermediate bodies held 51,936 trims at birth over 160,642 face edge uses.
  - No trim was built on any of them while they lived: "trims built on it" is 0 for all three intermediate kinds.
  - Their counterfactual is **54,401 builds**, all of them extra.
- **MEASURED, certified-bound split.**
  - Counterfactual builds that triggered a certified bound: **0 of 86,700**, over all bodies. Building a missing trim through `create_missing_uv_trim_curves` never ran `trim_deviation_bound` on this corpus.
  - The bounds that do run: **6,660** in total, taking 15.5 s in the no-CF run against 1.2 s for all trim builds.
    - `SmApiBoolean`: 722 bounds, 4.3 s
    - `sm:FlatFromBrep`: 8 bounds, 3.2 s
    - `sm:smlib_screw_sweep`: 156 bounds, 2.9 s
    - `union_all`: 252 bounds, 2.3 s
    - `sm:smlib_shell`: 743 bounds, 1.0 s
    - `blend`: 212 bounds, 0.8 s
    - `sm:smlib_extrude`: 3,877 bounds, which are cheap
  - **INFERRED.** An eager-trim policy would not add certified-bound work. It adds plain builds only. But certified bounds, not builds, dominate today's trim-related time.
- **MEASURED.** Today's builds by where they happen (calls; time from the no-CF run):

  | call | builds |
  |---|---|
  | `sm:FlatFromBrep` (record) | 12,108 |
  | `SmApiBoolean` | 8,076 |
  | `sm:mass_properties` | 5,957 |
  | `blend` | 5,266 |
  | `sm:smlib_offset_faces` | 2,082 |
  | `sm:smlib_shell` | 1,562 |
  | `sm:check` | 904 |
  | `planar_regions_boolean` | 708 |
  | `sm:SmBrep::Transform` | 564 |
  | `union_all` | 470 |
  | rest | under 320 each |

## 3. When kept bodies get their trims (MEASURED)

- 344 kept bodies:
  - 262 were born from a record (`sm:BrepFromFlat`) and 81 from `brep::to_kernel`, which is also from a record.
  - 85 were **born full**. 259 were **born missing** trims.
  - Of the 259 born missing, 73 got trims later, all at `sm:mass_properties` (5,823 builds: the measure and volume checks). **186 never got them**, and nothing needed them.
- **READ.** The app's body is its record (`crates/kernel/src/body.rs`, `RecordCell`). The ops and SMLib bodies are rebuilt from it lazily. `from_record` carries trims only for faces whose parameters are the record's (spline, NURBS, ball net). Everywhere else the trims are dropped and built again on demand.
- **INFERRED.** Trims are built twice:
  - First, on the operation's result when it is recorded (`FlatFromBrep`: 12,108 builds on 473 recorded bodies).
  - Again, on the body rebuilt from that record when it is measured (5,823).
  - Carrying the record's pcurves into the rebuilt body would remove the second set. That is a narrower and cheaper target than eager trims on every operation's result.

## 4. Boolean operands: trims read against trims held (MEASURED, with a caveat)

| taken by | operands | face edge uses | trims held at take | distinct edge uses whose trim was read | trims built while held |
|---|---|---|---|---|---|
| `SmApiBoolean` | 194 | 24,724 | 6,580 (27%) | 10,248 (41%) | 0 |
| `union_all` | 42 | 2,332 | 1,312 (56%) | 1,555 (67%) | 0 |

- **MEASURED.** Booleans read trims on about 41-67% of an operand's edge uses. That is more than the operands held when they were taken. The difference is built by the boolean itself after the take: `SmApiBoolean` makes 8,076 builds in total.
- **INFERRED.** About half of an operand's edge uses are never read, so carrying all trims on every operand would waste about half of that work.
- Caveat: read attribution is a lower bound (see caveats).

## Caveats

- **Read coverage.**
  - Reads are counted only through the `uv_trim_curve` / `uv_trim_curve_pointer` accessors. Direct `.uv_trim_curve` field reads are not counted (READ: about 200 sites, many in tests).
  - Reads are keyed by edge-use arena address. Copy-on-write snapshots (`with_rollback`) and scratch B-reps inside operations move the arena, so **76% of reads (36,538 of 48,341) were not attributed** to a body.
  - Read counts per body are therefore lower bounds. The build counts, the fates and the counterfactual are not affected.
- **Kept versus alive.** "Kept" means alive when the CLI process dumped the log. That includes bodies cached in timeline snapshots and lazy `ops`/`smlib` cells. A body "kept" only by a cache is counted as kept, so the intermediate share is if anything understated.
- **Feature tags** are per thread. Bodies made on validation or worker threads show as "(other thread)": 60 bodies.
- **Bodies were not split at edits.** A body edited in place stays one body. Edge-use indices reused after an edit could blur "still missing at end" slightly.
- **Timings are indicative.** They come from one release build on a shared server, measured with `Instant` on the outermost build only. The CF run is 1.3 times slower because of the counterfactual and census work. The counts are exact and deterministic: identical across the two runs.
- **Three designs failed** in the app and are missing (listed above).
- **Corpus bias.** The recipes are mostly small regression parts. Large STEP assemblies were not run. For STEP imports, each part produced only 3-4 bodies.

## Data

- `count-kit/36-data/`:
  - `bodies-cf.jsonl.gz` and `bodies-nocf.jsonl.gz`: one line per body plus one per call.
  - `run.sh`: the runner.
  - `probe.patch`: the scratch instrumentation; never commit it.
- `count-kit/zz_bodies_agg.py`: the aggregator. The fate split in section 1 was refined beyond that script by a one-off pass that sorts dropped bodies by which calls touched them.


The server worktree, its branch and its push ref were removed, and the server `/tmp` logs deleted. The local scratch a local worktree was kept; it has no build output.
