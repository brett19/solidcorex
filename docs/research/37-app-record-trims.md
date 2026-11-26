# 37. Why the app's record round trip loses UV trims, and what carrying them would take

Date: 2026-11-24. Base: `da240580a4` plus the scratch probe of report 36. Local investigation only: nothing pushed upstream or to the fork, no issue, PR or comment touched. Scratch branch `cache-rectrims-m` (local a local worktree, no build output; the server worktree, branch and side ref were removed).

Labels: MEASURED = counted by the probe on the 63-design corpus of report 36. READ = read from the code (file:line in the unprobed tree a local worktree). INFERRED = drawn from the two.

Units. "SMLib edge use" is what the probe counts: every face edge has two (one per side of the face) and the pair shares one trim, so trims BUILT = half the edge uses lacking one. A "record use" is one oriented edge of the stored record (one per pair). Counts are exact and the same on every run; times are indicative (shared server; this run's bound time was 12.1 s where report 36 saw 15.5 s).

## Plain answer

- **The trims are lost on planes, in the read side of the SMLib rebuild, and nowhere else that matters.** MEASURED: of 72,062 SMLib edge uses on bodies rebuilt from the app's record, 48,182 come back without a trim, and 48,086 of those (99.8%) are on plane faces. Every curved face (cylinder, cone, sphere, torus, B-spline, blend) keeps its trims (the other 96 are 20 cylinder, 8 torus and 68 B-spline-face uses).
- **The stored form already has the trims.** READ and MEASURED: every record use has a `pcurve` field (not optional), and on planes the export fills it with SMLib's own trim for 100% of uses (35,243 of 35,243; none dropped, none replaced by the closed-form routine). The loss is on read: `Body::smlib()` imports the record through flat arrays, and both layers skip a plane's trims ("SMLib rebuilds an analytic face's trims from its edges").
- **Carrying plane trims needs no new field and no design-format change.** Prototype (scratch, about 50 lines, env switch): builds on the corpus fall from 38,654 to 14,070 (-64%), bodies rebuilt from the record go from 33% to 99.9% of edge uses trimmed, certified bounds fall only 4% (6,660 to 6,384).
- **It will not make a reloaded body's mass properties match the pre-save value bit for bit.** MEASURED: that mismatch (247 of 345 bodies today) is not caused by the trims: the same rebuilt body gives identical bits whether its trims are built lazily or all at once (462 of 462), and carrying changes the count of matching bodies from 98 to 82 (noise).
- The 12,108 "first" builds at export are partly the same loss one step earlier (MEASURED): with plane trims carried they fall to 2,861.

## 1. The path from a kernel body to its stored form and back

**Out (READ).**
1. An operation's result is an SMLib body (`solidcraft_smlib::Brep`). The app records it through `Brep::to_native_ordered_report` / `to_native_report` (`crates/smlib/src/export.rs:123`, `:102`), which call `to_arrays` (`export.rs:93`, `core/convert.rs:293`) and `flat_from_brep` (`core/convert.rs:796`).
2. `flat_from_brep` first runs `create_missing_uv_trim_curves` (`convert.rs:797`): this is the 12,108 builds ("FlatFromBrep"). A trim exists for every use from here on.
3. The loop at `convert.rs:1060-1100` writes each use's trim into the arrays (`EDGEUSE_NURBS_*`) for every loop that exports UV (`loop_exports_uv`, `convert.rs:945`: B-spline kinds, NURBS forms and planes whose affine map exists). It drops a trim only for a line edge whose parameter is not an affine image of the edge's (`:1086`) or on an error (`:1097`).
4. `native_ordered` (`export.rs:888`) turns the arrays into the schema 4 record. For a use on a face whose parameters are a spline, rational spline or plane (`export.rs:984`) and an edge with a trim map (`:1006`) it keeps SMLib's own trim as the use's pcurve (`given`); otherwise the closed-form routine `solidcore_ops::pcurve::pcurves_within` computes one (`export.rs:1790`). Pcurves are mandatory in the record (`solidcore-record/src/brep.rs:137`; `docs/native-brep.md` "Pcurves").
5. The record becomes the app's body (`Body::from_smlib_record`, `crates/kernel/src/body.rs:790`; `surfaces.rs:1398`) and is what `encode_native_body` saves (`crates/kernel/src/native.rs`, `Saved { solid: body.record().clone(), .. }`).

**Back (READ): two lazy forms, built on first use.**
- `Body::smlib()` (`body.rs:807`, `:826`): `solidcraft_smlib::Brep::from_record(&record, true)` (`import.rs:793`), i.e. record, then flat arrays (`arrays` / `laid_out`, `import.rs:166`), then `from_arrays` (`convert.rs:2587` `brep_from_flat`, `make_uv_trim_curves: false` at `:2598`). This is `sm:BrepFromFlat` in the logs. It is the form measure, check, SMLib operations and booleans read.
  - Layer 1, `laid_out` writes a record pcurve as a trim only `if spline && !bare` (`import.rs:289`). `surface()` (`import.rs:420`) returns true only for Spline, RationalSpline, ball patch and NURBS-form faces; `plane()` (`import.rs:585`) returns `()`, so a plane's uses get zero-count entries. If the body has no spline-like face at all, `any_spline_face` (`import.rs:244`) is false and no trim arrays are laid out.
  - Layer 2, `bspline_curve_2d` (`convert.rs:1938`) returns `None` at `:1949` (`if !face_has_nurbs`) even when the arrays carry a trim: a plane is an analytic block with no NURBS.
- `Body::ops_body()` (`body.rs:1087`, `:1099`): `solidcore_ops::Body::from_record`, which uses solidcore-convert `to_kernel` (`solidcore-convert/src/brep.rs:213`). It carries every record pcurve (`brep.rs:656`), including a plane's, which `face_surface` scales into the kernel's plane parameters (`solidcore/src/surface/face_surface.rs:49-53`, plane at `:171`/`:197`). It skips only `bare` uses and faces with `trims: false` (an analytic label with no NURBS form, an exact revolution; `face_surface.rs:100`, `:138`). This is `brep::to_kernel` in the logs, and the earlier "8 lost in 150 round trips" is this path.

**Deliberate?** The omission is commented (`import.rs:9-11` "carried on B-spline faces only: on analytic faces SMLib keeps its trims in its own parameterisation and rebuilds them from the 3D edges"; `import.rs:581`; `convert.rs:1969`). `git log -S` for that text reaches only the squashed merge `ea772a0438` ("SolidCraft on the SMLib kernel", #11351): no earlier history and no recorded reason. INFERRED: the comment is true for cylinder, cone, sphere and torus on their own parameterisation, and those are carried anyway through the NURBS form. It is not true for a plane: the same file's export (`convert.rs:~930`, `plane_map`) maps SMLib's plane trims to the record's parameters by an affine map, so the inverse is available. The rule was applied to planes without the check.

## 2. Counts by cause (MEASURED, 63 designs, same as report 36)

The probe counts, per edge use, the surface kind, the edge curve kind and the stage. Kept-body birth counts reproduce report 36 exactly (262 SMLib-rebuilt kept bodies, 344 in all, 12,108 / 5,957 builds), so the probe does not disturb the run.

**Born with trims, by rebuild path (kept bodies).**

| rebuild path | kept bodies | born with all trims | missing some | edge uses | with a trim at birth |
|---|---|---|---|---|---|
| `sm:BrepFromFlat` (SMLib form) | 262 | 16 | 246 | 51,182 | 17,054 (33%) |
| `brep::to_kernel` (ops form) | 81 | 68 | 13 | 20,536 | 19,644 (96%) |
| other (a boolean result) | 1 | 1 | 0 | 52 | 52 |

So 246 of the 259 bodies born missing trims, and nearly all the missing uses, are in the SMLib form.

**The chain, by stage (record uses; total 35,243).**

| stage | result |
|---|---|
| live body before export | 46,270 of 70,486 SMLib edge uses hold a trim, own or mate's; 24,216 do not (= the 12,108 builds, 11,486 of them on planes) |
| written to the arrays | 35,243 of 35,243 (reasons: no drop, no non-affine line, no error) |
| stored as the record's pcurve | 35,243 SMLib's own (`given`); 0 closed-form |
| laid out for the SMLib rebuild | carried 11,934; plane not carried 19,339 (`not_spline_face`) + 4,714 (`no_spline_face_in_body`); `bare` 26; 10 more on cylinders with no NURBS form (the extruded-circle recognition) |
| read by `from_arrays` | built 11,940; `arrays_empty` 19,377; `uv_off` 4,714; of 24,043 plane uses read, 0 built |
| present on the rebuilt body | 23,880 of 72,062 SMLib edge uses (33%); missing 48,182 |

**Missing after the rebuild, by surface and edge kind (SMLib edge uses).**

| surface | edge | uses | missing |
|---|---|---|---|
| plane | line | 37,904 | 37,904 |
| plane | circle or ellipse | 5,724 | 5,724 |
| plane | B-spline | 4,458 | 4,458 |
| B-spline faces | all | 9,340 | 68 |
| cylinder | all | 13,996 | 20 |
| torus | ellipse | 192 | 8 |
| cone, sphere | all | 448 | 0 |

(the edge kind "ellipse" in the probe includes circles.)

**The 5,957 builds on `sm:mass_properties` (5,823 of them on kept bodies):** plane x line 4,571, plane x circle/ellipse 569, plane x B-spline 406 (93% planes); revolution 375 (SMLib's exact revolution surface, on bodies not rebuilt from a record), blend 20, B-spline 16.

**Builds by surface over the whole run:** plane 34,973 of 38,654 (90%). By call, plane share: FlatFromBrep 11,486 of 12,108; SmApiBoolean 7,796 of 8,076; mass_properties 5,546 of 5,957; blend 4,461 of 5,266; check 904 of 904.

**Do the rebuilt trims equal the originals?** Rebuild, build every trim, export again, and compare use by use with the export that made the record (345 of 462 rebuilt bodies; the other 117 have records not made by an export in the same process):

| | identical bits | within 1e-9 relative | beyond |
|---|---|---|---|
| SMLib trims today (31,881 uses) | 29,835 (93.6%) | 1,928 (6.0%) | 118 (0.4%; 108 on planes) |
| with plane trims carried | 31,178 (97.8%) | 681 | 22 |

No use had a trim on one side only. Tolerances after everything is built, rebuilt against the record: body tolerance equal in 462 of 462; edge tolerance equal in 17,678 of 17,680 (2 larger after the rebuild); vertex tolerance equal in 11,613 of 11,617 (4 larger). With the carry: all equal. So today's double build does change a few tolerances (6 of 29,297) and moves the trim data in the last bits for 6% of uses, in a few cases (0.4%) by more than 1e-9; the effect on answers is small but not nil. Structure differs in 1 of 462 rebuilds (not examined).

**Mass properties, reloaded against pre-save (345 bodies with a stored pre-save value, both measured on a copy at accuracy 1e-8):** all eight quantities (area, volume, centroid, three moments) identical bits in 98 (28%); volume identical in 192, within 1e-14 relative in 151, within 1e-12 in 1, worse than 1e-6 in 1. Control: the rebuilt body measured after all its trims were built gives the same bits as measured lazily in 462 of 462. So the trims do not explain the mismatch (INFERRED: record order and the arrays' rounding do).

## 3. What carrying the trims takes, and what it saves

**Smallest change: read side only, two places, no new field.** READ and prototyped:
- `import.rs`: `plane()` and `surface()` report that a plane carries trims (where the axes are unit and orthogonal, so the record's (u, v) are the arrays' (u, v)), and `any_spline_face` counts such a face.
- `convert.rs`: in `flat_to_brep_data`, for a plane face compute the affine map from the plane's NURBS (u, v) to the arrays' STEP (u, v) exactly as the export does (`convert_uv_from_nurbs_to_step` at three points, times `uv_scale`), invert it, and pass it to `bspline_curve_2d`, which then applies it to the control points instead of returning `None`.
The prototype is behind an environment switch (`ZZ_CARRY`) in scratch code only; `rectrims.patch` in the data directory has it.

**Result on the corpus (MEASURED, same 66 designs that run; the three that fail in the app fail in both).**

| | today | planes carried |
|---|---|---|
| UV trim builds | 38,654 | 14,070 (-24,584, -64%) |
| of which `sm:FlatFromBrep` | 12,108 | 2,861 |
| `SmApiBoolean` | 8,076 | 3,078 |
| `sm:mass_properties` | 5,957 | 411 |
| `blend` | 5,266 | 2,540 |
| `sm:check` | 904 | 0 |
| `sm:smlib_offset_faces` / `smlib_shell` | 2,082 / 1,562 | 1,752 / 1,352 |
| `RemoveTopologicalEdgesAndVertices` | 312 | 0 |
| `union_all` | 470 | 268 |
| edge uses missing a trim after a rebuild | 48,182 of 72,062 | 96 |
| certified bounds | 6,660 | 6,384 (-276) |
| bound time (this run) | 12.11 s | 12.06 s |
| edge / vertex tolerance differing after rebuild | 2 / 4 | 0 / 0 |
| designs with different version, face, use or alive-body counts | | 0 of 66 |

Build time was 1.2 s of 105 s in report 36, so the saving in time is under a second there (INFERRED, indicative). The certified bounds hardly move: they come from the faces operations make, not from the trims the rebuild lost. Mass properties on a reloaded body do not become bit-identical (98 to 82 of 345; see above). The 9,247 builds saved at export (12,108 to 2,861) are the same loss cascading: an operand rebuilt without its plane trims hands its result no trims, and the export builds them again (INFERRED from the counts).

**What could go wrong.**
- A stored trim that no longer matches the rebuilt geometry. For planes the map is taken from the rebuilt plane itself, so it matches by construction; the prototype's results show the carried trims equal the originals in 98% of uses to the bit and none beyond 1e-9 except 22. A record healed on import (`from_record(.., false)`) could change faces after the trims were written; the corpus only has trusted rebuilds, so this is untested.
- A record not written by SMLib (STEP import, truck) holds closed-form pcurves, not SMLib's own trim, and its edge tolerance was not raised by a trim build. Carrying such a trim skips the raise the build would have done. The ops form already does this for every kind, so the risk is not new, but the SMLib form would take it on. Untested.
- Planes with non-unit or non-orthogonal axes (truck's, and the bilinear-patch case) were left uncarried by the prototype.
- Old designs: no problem, since every record has pcurves. Nothing older lacks the field.
- The check that sizes edges and trims (`sm:check`, 904 builds today, 0 with the carry) now reads trims that were built elsewhere, which is the point; the 1 structure mismatch in 462 rebuilds is worth looking at before anything ships.

**Design-format note (plainly).** The change above needs none: the pcurve is already in the record and in the saved model. A new field would: any addition to `solidcore_record::brep::Brep` or `native::Saved` (both `deny_unknown_fields`, `VERSION = 4`) changes what a saved model holds, so it goes in `crates/doc/src/format/pending/<name>.rs` with the note the skill asks for (what is saved, what older designs get, and that the last release opens it with a warning and keeps what it cannot read), never touching `FORMAT` or any `v<N>/` folder (`.agents/skills/solidcraft-file-formats/SKILL.md`). Nothing in this finding calls for one.

**Alternative (INFERRED, untested).** Build the SMLib form from `solidcore_convert::to_kernel` as `ops_body()` does, which already carries 96% of trims, instead of going through flat arrays. That removes the two read-side layers but also the `analytic()` recognition and the C API port's exactness the arrays path keeps; a larger change.

## 4. Certified-bound computations (6,660 of them)

Per source (MEASURED, this run; report 36's 15.5 s was the same counts on a busier server): calls, bounds, time, and how many repeat identical inputs (hash of surface, edge curve, trim, interval, floor, thresholds and budget).

| source | bounds | time | distinct | repeats |
|---|---|---|---|---|
| `SmApiBoolean` | 722 | 4.05 s | 678 | 44 (later call) |
| `union_all` | 252 | 2.35 s | 252 | 0 |
| `sm:FlatFromBrep` | 8 | 1.74 s | 8 | 0 |
| `sm:smlib_screw_sweep` | 156 | 1.55 s | 156 | 0 |
| `blend` | 212 | 0.75 s | 182 | 30 (same call) |
| `sm:smlib_shell` | 743 | 0.66 s | 743 | 0 |
| `sm:smlib_revolve` | 25 | 0.49 s | 25 | 0 |
| `sm:mass_properties` | 164 | 0.37 s | 164 | 0 |
| `sm:smlib_extrude` | 3,877 | 0.08 s | 3,773 | 104 (later call) |
| the rest | 501 | 0.1 s | | |
| all | 6,660 | 11.75 s counted, 12.1 s total | 6,482 | 178 (2.7%) |

- **Repeats are rare here:** 178 of 6,660 (30 inside one call, 148 across calls), costing 0.37 s of 12.1 s. The earlier kernel-level finding (51 of 100 repeats inside one validity check) does not show: `sm:check` made 6 bounds in this corpus. The repeats cache would save 3%.
- **A few designs dominate:** `fins_joined_into_their_hub` 4.3 s, `revolve-360-cut` 2.3 s, `rod_doubled_edge` 1.6 s of the 12.1 s; the per-bound cost is large where a dense net or deep bisection is involved (FlatFromBrep: 8 bounds, 1.74 s, 1.64 s of it in one design).
- **Why an export computes bounds, and whether sampled mode applies.** READ: `create_missing_uv_trim_curves` (`convert.rs:797`) builds a trim with `create_uv_trim_curve`, which sets it through `trim_raise_distance` (`edgeuse/p3.rs:1854`). That computes a proved bound (`trim_deviation_bound`, `p3.rs:2683`) only when the build would raise the edge's tolerance (`tol < 2 * upstream`, `p3.rs:1867`) and the kernel is in the proved mode (`:1868`). `with_sampled_trim_raise` (`kernel.rs:1101`), which skips it, is entered only by `to_arrays_sampled_trims` (`convert.rs:304`), reached from `to_native_ordered_sampled_trims` (`export.rs:139`) and `pcurves::SmlibImportTrims` (`pcurves.rs:50`): the per-face sheets of a STEP import. The ordinary record export of an operation result (`to_arrays`) is in the proved mode, so the sampled mode does not apply to it. MEASURED: only 8 of its 12,108 builds raise an edge (4 designs, 2 each), which is why the bound count there is small and the cost is concentrated.
- With the plane carry the bounds from `mass_properties` fall 164 to 16 and from blend 212 to 120; the total falls 4%.

## Method and caveats

- Probe (scratch, `count-kit/37-data/rectrims.patch`): stage tallies by surface kind, edge kind and call frame at: build (`create_uv_trim_curve`), export (`flat_from_brep` before and after building, per-use write reason), record (`export.rs` given or not), import (`laid_out` carried or not, `from_arrays` read or not), rebuilt census, bound (inputs hash, first or repeat, time). Rebuild check (`crates/smlib/src/zz.rs`): at every `Body::smlib()`, a copy of the rebuilt body is measured, all its trims built, exported and compared with the export stored under the record's hash and with the live body's mass properties; its counters are suppressed and restored. Runner and tallies: `count-kit/37-data/` (`run.sh`, `tally-base.tsv`, `tally-carry.tsv`, `bodies-*.jsonl.gz`, `agg.py`, `cmp.py`).
- The shadow check makes each run slower than report 36's; counts are not affected (kept-body numbers reproduce exactly).
- 117 of 462 rebuilds had no stored original in the process (the record came from elsewhere); trim and mass comparisons use the other 345.
- "Call frame" is the top two probe frames; rebuilds happen outside any frame, so they are not attributed to a caller.
- Not examined: the structure mismatch in 1 of 462; healed (untrusted) records; non-unit plane axes; carried answers beyond topology counts (the three designs that fail in the app fail in both runs, errors alike); wall-clock gains.
- The corpus is mostly small regression parts (as in report 36), so absolute build counts will not carry to large assemblies; the plane share (90% of builds, 99.8% of lost trims) is the portable result.
