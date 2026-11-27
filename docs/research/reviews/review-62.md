# Review of research 62 - error taxonomy

An adversarial review of `docs/research/62-error-taxonomy.md` ("62" below), as of 2026-11-27.
Nothing in the repository was built or changed. One scratch program, outside the repository, was
compiled against the existing debug rlibs to measure type sizes (§2, H5). The only file written
in the repository is this one.

- **Labels:**
  - VERIFIED: checked here against code, docs or a command, with the place given.
  - JUDGED: a judgement.
- **Paths:**
  - scx paths are relative to `crates/`.
  - Pin paths are relative to the pin's `solidcore/crates/`.
  - "RFC" is SolidCraft issue #22563, read in full with `gh issue view`.
- **Verdicts:** agree, agree with changes, or disagree.

---

## 0. Summary

62's main shape holds: a kind is computed from the cause and never stored, lower errors sit
whole in typed fields, `Display` prints one frame, and the remedy is derived. Four things in it
are wrong or underspecified, and three of them change decisions.

1. **Rule 1 of §2.3 (the wrapper judges the kind) inverts the RFC's default and contradicts
   62's own mitigation.** It offers no answer when an argument is partly the caller's and partly
   built by us, which is the common case in geometry. Validating at the public boundary makes the
   rule mechanical (H1).
2. **T1 cannot be written as specified.** `SolveError<E>::kind()` needs a kind from `E`, and
   `EvalError` has none. D8 ("keep the predicates for now") blocks T1 (H2).
3. **T1's `source()` for `SolveError::Num` brings back the double printing 62 removes.** That
   variant's `Display` already delegates to the inner error (H3).
4. **The determinism rule needs one more fact to hold:** the work at which each event happened.
   Warnings also need a rule for cache hits and merge order (H4).

The five kinds hold up better than expected. The real cases that strain them are resource limits
and causes whose kind and remedy point different ways (M1, M2). Neither needs a sixth kind,
though both need a written rule.

---

## 1. High

### H1. "The wrapper judges the kind" pushes guesswork into every crate as written. JUDGED, with VERIFIED parts

- **The default is inverted relative to the source it cites.** VERIFIED: RFC §8.2 says "The check
  reports `InvalidInput`. A kernel caller that built the bad argument re-raises it as a defect …
  A site is converted only after its callers are read. Until then it stays `InvalidInput`." So
  the RFC's default is to inherit the kind. 62 §2.3 rule 1 makes re-judging the default ("made
  the default"): a lower `Refused` becomes `Unsupported` or `Defect`.
- **62 contradicts itself.** The "Weak points" list in §8 says to "start conservative (inherit)
  at each site until its callers are read". That is the RFC's default, not rule 1. A builder
  reading §2.3 and a builder reading §8 will write different code.
- **Mixed provenance has no answer.** A solver hands its evaluator the caller's surface and a
  parameter the solver chose. Take an evaluator `Refused` for a non-finite control point found
  lazily during evaluation. The fault is the caller's, but rule 1 says the wrapper "built the
  data it handed down", so it reports `Unsupported`. Most geometry calls are mixed in this way.
- **Generic wrappers cannot judge at all.** `SolveError<E>` knows nothing about `E`. Only the
  code that built the evaluator knows the provenance.
- **`?` inherits silently.** VERIFIED: `impl<E> From<NumError> for SolveError<E>`
  (`scx-algo/src/error.rs:181`) makes every `?` on a `NumError` pass the kind through unchanged.
  That is exactly the boundary where rule 1 asks for a judgement. Under rule 1, `From` impls
  between crates are a lint hazard.
- **What fixes it.** The RFC's own stated end state (§8.2, "Alternative"): validate at the public
  boundary and pass validated types inward ("parse, don't validate"). For example, a finished
  `Brep` is already checked, and the repo already has unit vectors and finite points (57 M1-M5).
  - After that, any `Refused` raised below a public entry point is our fault by construction. The
    rule becomes mechanical: below the boundary a lower `Refused` maps to `Defect`, and only
    boundary validators create `Refused`.
  - Until a routine is split this way, inherit, as the RFC does. A user told "report a bug" for
    their own bad input is the cheaper error to find, because it reaches us.
- **Change asked:** restate rule 1 as "inherit by default; a frame re-judges only when it has
  proved it built the argument; boundary validation is the end state". Drop blanket cross-crate
  `From` impls, or keep them only where a pass-through is correct.

### H2. `SolveError<E>::kind()` (T1) has no kind to read from `E`. VERIFIED

- `pub trait EvalError: fmt::Debug + fmt::Display` (`scx-algo/src/error.rs:31`) has only
  `ends_solve`, `passes_through` and `never_ignored`. No `kind()`.
- So `SolveError::Eval(e).kind()` and `CallFailed { cause }.kind()` cannot be computed. T1 lists
  `kind()` and depends on nothing, while D8 defers giving `E` a kind.
- With the scx architecture the three predicates can be derived from the five kinds:
  - `ends_solve` is `Exhausted | Cancelled | Defect`.
  - `passes_through` is the same set. The pin's `PreparationRequired` has no scx counterpart,
    because cells are pure (01 §3.2).
  - `never_ignored` is the same set. `FaceTrimsUnavailable` is gone by the finished/editable split
    (01 §2.3). "A blend certificate ran out" is now an `Unproven` success (61 §7.5), not an
    error.
  - The pin's `Unported` maps to `Unsupported`, which no longer ends a solve. That is a behaviour
    change to record in the difftest, not to avoid (04 §1, 11-27).
- **Change asked:** decide D8 now. Add `fn kind(&self) -> ErrorKind` to `EvalError` and derive
  the three predicates from it as provided methods. These are not failing defaults: they are
  total and derived. The alternative is to drop `kind()` from T1.

### H3. T1's `source()` for `Num` double prints. VERIFIED

- `SolveError::Num(e) => e.fmt(f)` and `SolveError::Eval(e) => e.fmt(f)`
  (`scx-algo/src/error.rs:167`, `:174`) are transparent: their `Display` is the inner error's.
- T1 says "`source()` for `Num` and `CallFailed`". If `Num`'s `source()` returns the `NumError`,
  then `error::chain` prints that frame twice. This is the defect §2.4 sets out to remove.
- **The idiom** (thiserror's `#[error(transparent)]`, written by hand): a transparent variant
  forwards both, `Display` to `inner.fmt` and `source()` to `inner.source()`. A variant with its
  own frame (`CallFailed`) prints its own frame and returns the inner error as its source.
- §2.3 rule 4 ("context frames are transparent") uses "transparent" in a different sense: it
  means "adds no kind". `Within` still prints `in #id`. 62 should name the two cases apart:
  - **transparent**: no frame, `source` forwarded;
  - **context**: a frame, kind forwarded.
- **A fifth double printer.** VERIFIED: `Unwritable::Geometry(g)` writes
  `"{g} has no ISO 10303-42 form"` and also returns `g` from `source()`
  (`scx-p21/src/error.rs:388`, `:394-397`). `Refusal::Syntax`, `Unwritable` and `Entity` also
  print their cause and return it (`:217`, `:231-233`, `:238-246`). T3's count of "4 Display
  impls" is low.

### H4. "Lowest-index event" is deterministic only with two more rules. JUDGED, against VERIFIED code

`Scoped` (`scx-algo/src/executor.rs:61-81`) hands out indexes from an atomic counter and has no
early exit: every item runs to completion. Under 01 §6.3 one total budget is metered per item and
summed in index order, and item `i` cannot know the prefix sum `S(i-1)` while it runs. So:

1. **Each item must run under the full budget B, not the remaining B - S(i-1).** Its outcome must
   be recorded with the work at which each event happened. The merge then walks the items in
   order. At item `i`, a failure or defect at work `w` counts only if `S(i-1) + w <= B`; otherwise
   the outcome is `Exhausted { item: i }`. That is what serial running would give.
   - 62 §5.2's "ties go to the order the item's own code would meet them in serial" says this
     loosely. It needs to be the stated rule, and each per-item slot needs `(outcome, work)` on
     the error path too.
   - Without it, take an item that fails at work 900 under B = 1000 with S(i-1) = 300. That item
     reports its failure in parallel and `Exhausted` in serial.
2. **No algorithm may read its remaining budget except to stop.** If a routine chooses a cheaper
   method, or sizes a certificate budget, from what remains, then serial (given B - S) and
   parallel (given B) build different results.
   - 61's certificate budget must be a fixed setting, not a share of the caller's budget.
     Otherwise `Unproven` versus `Certified` would depend on executor and item order.
3. **§5.3 needs the same work test for defects.** A defect at a lower index "wins" only if it
   would have been reached within the budget. Otherwise serial reports `Exhausted` at that item
   and parallel reports the defect.
4. **Unspecified cases:**
   - **Collect-all and the budget.** When the budget runs out partway, does the call return
     `Exhausted` and drop the `Items` gathered so far?
   - **`Skip` rounds and the budget.** Is the budget per round or total? 01 §5.2 does not say.
     Both choices are deterministic, but they differ.
   - **`Cancelled` racing a failure.** It can return either. 62 §5.4 allows that, which is fine,
     but it should say so explicitly.
5. **Wasted work.** With no early exit, a failure at item 0 still runs every other item. That
   costs up to n·B work units, because each item may use the full budget. A shared "lowest index
   failed so far" atomic lets items above it stop. That is still deterministic, since only
   higher-index items are skipped. This is a performance point for 01 §8.2, not a correctness
   one.

---

## 2. Medium

### M1. Resource limits fit two kinds, or none. VERIFIED

- **Allocation failure is filed as invalid input.** `ObjsInVoxels` maps a failed allocation of
  `bucket_count` buckets to `NumError::InvalidInput` (`scx-algo/src/hash_table.rs:205-211`).
  62 §3.1 sends every `InvalidInput` to `Refused`. The input may be valid: the machine is out of
  memory. "Change the input" is the wrong action.
- **`Refusal::NeedsMemory { needed, available }`** (`scx-p21/src/error.rs:187-192`,
  `parse.rs:160-166`) fits two kinds. `available` is a limit the caller sets, so this is
  `Exhausted` in all but name. It is filed as `Refused`.
- **The fixed caps disagree with A10.** `EntityCause::TooDeep { limit }`,
  `TooManyInstances { limit: MAX_VISITS }` and `Refusal::TooMany` are fixed caps on valid files
  (`scx-p21/src/product.rs:1257-1263`). By A10's own reasoning ("a fixed internal cap … the input
  is valid and the method gave up"), they are `Unsupported`, not `Refused`.
- **No sixth kind is needed.** Widen `Exhausted` to "a limit the caller or the machine sets ran
  out", with a typed cause (`Work`, `Memory`, `Limit { what, limit }`). Fixed internal caps stay
  `Unsupported`.

### M2. A kind and its remedy can point different ways. VERIFIED (pin), JUDGED (consequence)

- 62 §1.1 says `Unsupported` means "never 'fix your input'".
- But the pin has causes where the kernel does not build the case and a smaller size is the way
  out. `RailLeavesFace` is one: "the kernel does not build over the cliff beyond" (pin
  `solidcore/src/error.rs:1429`). `CliffOverflowShape` is another (`:1458`).
- Under 62 these are `Unsupported` with `Remedy::Resize`, or `Refused` for a valid input. Either
  way the "one kind, one caller action" claim of A1 is not exact.
- **Change asked:** say that the kind is the coarse route and the remedy refines it. Define
  `Unsupported` as "valid input this method does not build", and drop "never fix your input".
- `Remedy::Unsupported`, `MoreBudget` and `Report` only restate the kind, so they can go:
  `Option<Remedy>` is `None` where the kind says it all. Otherwise the derived remedy and the
  derived kind are a second pair that can disagree.

### M3. `JacobiDidNotConverge` should not be `Unsupported`. JUDGED (mathematics), VERIFIED (pin)

- Cyclic Jacobi on a finite real symmetric matrix always converges, quadratically once it is
  close. So failing after the sweep cap means one of two things:
  - a non-finite input, which is `Refused`;
  - our bug or a cap set too low, which is `Defect`.
- The pin's experience points the same way. It files a station solve that does not converge as
  a defect (`StationDidNotConverge`, pin `solidcore/src/error.rs:1480`).
- Brent's iteration limit and `ZeroDifferenceStep` depend on the evaluator, so `Unsupported` is
  fair for them.
- **Change asked:** in §3.1, re-kind Jacobi as `Defect`, after a finiteness check that refuses.
  Do not re-kind `Disjoint` and `PointOutsideExtent` until T5 has read their callers. As written,
  §3.1 sets them to `Refused` first and fixes them later.

### M4. `Remedy::Resize` loses the direction when no size is proved. VERIFIED

- RFC §5.4: `Resize { measure, bound, fit: Option<Fit> }`.
- 62 §4: `Resize { at: Site, fit: Option<Fit> }`. Here `measure` and `bound` live only inside
  `Fit`.
- With `fit: None`, 62 cannot say "smaller radius", which is the most common remedy when the
  check did not compute a size.
- **Change asked:** keep `measure` and `bound` on `Resize`.

### M5. Error sizes grow by one inline lower error per layer. VERIFIED (MEASURED)

Measured with `size_of` against the current debug rlibs on x86-64 (scratch program, rustc 1.99.1):

| Type | Bytes |
|---|---|
| `NumFailure` | 40 |
| `NumError` | 64 |
| `Result<f64, NumError>` | 64 (an `Option<f64>` is 16) |
| `SolveError<small E>` | 64 |
| `SolveError<NumError>` | 80 |
| `SolveError<SolveError<NumError>>` | 96 |
| `P21Error` | 56 |

- **How it grows.** Each generic layer adds about 16 bytes. A `BlendCause` holding an `SsiError`
  inline, which holds `SolveError<_>`, which holds `NumError`, will pass clippy's
  `result_large_err` threshold of 128 bytes within two or three layers.
- **Where it costs.** Every `Result` return on a hot path is moved through memory. That includes
  `solve_linear_system` inside the Newton loop (`local_solve_nd.rs:506`).
- **The hottest primitives are not affected.** VERIFIED: `normalized` and `unit` return `Option`
  (`scx-num/src/vector3d.rs:577`, `:690`).
- **Changes asked:**
  - Box lower errors in cause fields (`by: Box<SsiError>`). The cost is one allocation on a cold
    path.
  - Enable `clippy::result_large_err` for the workspace.
  - When T4 types the reasons, drop `operation: &'static str` from `NumError` (16 bytes a frame).
    The difftest facade can map each variant to the pin's operation path.
  - 62 has no cost section. Per the owner's performance rule it needs one, with before and after
    sizes for T2 and T4.

### M6. `chain()` does not reproduce p21's one-line form. VERIFIED

- `P21Error`'s `Display` prints the innermost error's class word first, then the `in #id:` path,
  then the cause (`scx-p21/src/error.rs:95-116`).
- A frame-by-frame chain prints the outer `Within` frame first: "in #12: in #11: STEP input
  refused: #10: …".
- 62 §2.4's claim that 58 §7's form "is what `chain` prints" is false.
- **Choose one:**
  - accept the new order and update 58 §7 and the tests that read the text;
  - or give `P21Error` a reporter of its own that puts the class first.

### M7. Typed warnings and `Unproven`: the claim is true only per call. JUDGED

- **Within one call, the claim holds.** Emitting a warning changes nothing, and `Unproven` is a
  success.
- **Across calls, `Unproven` changes results.** A `Basis::Unproven` record and its bound are what
  later readers use (04 §1, 11-26: "every reader uses the recorded value"). So it changes later
  operations' tolerances. That is acceptable, but 62 should not say "never changes a result"
  without that qualifier.
- **Warnings and cache warmth.** 01 §6.3 rules that a cache hit charges the work recorded at
  fill, so the stop point does not depend on warmth. 62 has no matching rule for warnings.
  - A cell that falls back and warns must store the warning and replay it on a hit. Otherwise the
    warning list depends on what is cached.
  - The same applies to a lower crate's warnings returned beside a memoized value.
- **Merge order.** Warnings from parallel items must be merged in item order. Warnings from items
  above a reported failure or the budget cut must be dropped. Neither rule is stated.
- **Stored twice.** `Basis::Unproven` on the entity and a warning in `Report` say the same thing
  in two places. By 62's own "derived, never stored" rule (§4), the warning should be derivable
  from the changed records, or `Report` should name the entity and leave the bound in the record.

---

## 3. Low

### L1. Migration counts, spot-checked. VERIFIED

Counted with `grep` over non-comment lines, in each crate's whole tree unless a directory is
named:

| 62 says | Found | Command or place |
|---|---|---|
| 19 `SolveError::` sites | 19 | `grep -rn 'SolveError::' scx-algo` |
| 20 `invalid(` calls (p21) | 20 | `grep -rn '\binvalid(' scx-p21/src` |
| 16 `.within(` sites | 16 | `grep -rn '\.within(' scx-p21` |
| 3 `EvalError` impls | 3 | `solvers.rs:103`, `local_solve_nd.rs:797`, `local_solve1d.rs:1236` |
| ~11 p21 text tests | ~11 | `grep -B1 -A2 'e.to_string()'` in `scx-p21/tests` |
| 42 `ErrorKind::` lines | 47 in num/algo/p21, 48 with difftest | `grep -rn 'ErrorKind::'` |
| 44 `NumFailure::` | 41 in num/algo/p21, 49 with difftest | `grep -rn 'NumFailure::'` |
| 2 test asserts on `Degenerate` | 3 | `matrix.rs:1207`, `error.rs:373`, `series.rs:822` |
| 7 `NumError::InvalidInput {..}` in `hash_table.rs` | 10 (6 in code, 4 in tests at `:994`, `:1014`, `:1029`, `:1267`) | `grep 'NumError::InvalidInput' scx-algo/src/hash_table.rs` |
| 39 reason literals in 11 scx-num files | about 49 lines in 10 files, error.rs's own tests included | `grep -E 'invalid_input\(|reason: *"' scx-num/src` |
| 4 Display impls double print | 5 or more (H3) | |

- None of these changes a task's size. 62 should still give the exact command it ran, so the
  counts can be repeated.
- **T4 is missing tests that read reason text.** The hash_table tests assert on reason text
  (`reason: "voxel dimensions overflow"`), which 62 §6's "no other test reads text" misses. T4
  breaks them.
- **T6 must match through wrappers.** It must match `Singular` through `CallFailed` wrappers, or
  the existing test `bad_jacobian_reads_the_status_of_a_typed_failure`
  (`local_solve_nd.rs:1151-1155`) fails. It is a behaviour change if any other `Failed` cause can
  reach `is_bad_jacobian` (`:95-96`). Check which causes `solve_linear_system` can return before
  calling the change "no result changes".

### L2. D5's premise is partly obsolete. VERIFIED

- The facade already compares an `InvalidInput` by class alone, without its operation or reason
  (`scx-difftest/tests/diff/facade.rs:980-991`). Only `Failed` compares the operation and the
  `to_string()` text.
- So the reason texts are already free to change. T4 needs no facade mapping to pin reason text.

### L3. Idiom points. JUDGED, owner rules VERIFIED in the builder preamble

- **`Fit.inclusive: bool` is a bool flag**, one of the shapes the owner's rules hunt. A
  four-variant `Bound { Below, AtMost, AtLeast, Above }` says the same with no flag.
- **`Fit.size: f64` with a run-time `Measure` drops the unit types** that 57 M1-M8 just merged.
  Every listed measure is a length. A chamfer or draft angle would be an angle. Carry a `Length`,
  or an enum of typed sizes.
- **`Defect { at: &'static str }`** can be a `&'static core::panic::Location<'static>` taken with
  `#[track_caller]`. That gives file, line and column with no hand-written strings, and is the std
  idiom.
- **`Items` "non-empty" is a comment, not a type.** Use `{ first, rest }` or a small non-empty
  wrapper.
- **`Exhausted { item, work }`** has no meaningful `item` for an operation with no items
  (booleans, 01 §5.2). Use `Option<usize>`, or a separate variant.
- **`Skipped` has a `round` field** that means nothing in collect-all mode.
- **`P21Error::kind()` walks the `Within` chain** (`scx-p21/src/error.rs:38-54`). That contradicts
  the claim in §2.3 that `kind()` "does no search". The walk is harmless at O(depth), but the
  claim should be dropped.
- **A9's argument against codes applies to `name()` too.** A hand-kept `name()` per cause is also
  a table to keep in step. A small `macro_rules!` in `scx-num` that declares each cause with its
  kind, name and text in one row keeps all three together, std-only. That is the thiserror
  pattern without the dependency. It is optional, and it must not hide the exhaustive `match`.
- **`ErrorKind` as `#[non_exhaustive]`** forces every app's `match` to carry a wildcard arm. A
  kind added later then routes silently to that arm. Before publication breaking changes are
  free, so leave the attribute off until 01 §7.4's publication step, or accept the trade-off
  explicitly.

### L4. 62 and the RFC differ on fit caps. VERIFIED

- RFC §8.3 leaves "fit caps and certificate budgets" unclassed, with remedy `MoreFuel` or a
  resize of the tolerance asked.
- 62 sends fixed caps to `Unsupported`, and certificate budgets to `Unproven`.
- 62's choice is defensible: it has no `Unclassed`, and valid input is never refused. But §8 says
  it "differs" from the RFC only where it records why, and this difference is not recorded.

---

## 4. Verdicts on D1 to D10

| # | Verdict | Why |
|---|---|---|
| D1 `Refused` | Agree | It names the caller's action, covers false facts and sizes that do not fit, and matches p21's variant |
| D2 re-kind per §3.1 | Agree with changes | Jacobi goes to `Defect` after a finiteness refusal (M3). Allocation failure and `NeedsMemory` go to a widened `Exhausted` (M1). `Disjoint` and `PointOutsideExtent` are not re-kinded until T5 reads their callers. T6 is a behaviour change to check (L1) |
| D3 lowest-index default, collect-all explicit | Agree with changes | Add the work-at-event rule and "no algorithm reads its remaining budget" (H4). State the budget semantics for collect-all and `Skip`, and the merge order for warnings (M7) |
| D4 drop `Lower(LowerError)` and shared cause types | Agree with changes | Box lower errors in cause fields (M5). Drop or limit cross-crate `From` impls, which inherit kinds silently (H1) |
| D5 facade maps causes to pin text | Agree | Its premise is partly obsolete: `InvalidInput` text is already uncompared (L2). Only `Failed` needs the map |
| D6 cause names, no numeric codes | Agree | Names fit RFC 0016's signature. 62's argument against codes also applies to names (L3), so the reason to prefer names is readability, not one fewer table |
| D7 take the RFC's remedy | Agree with changes | Keep `measure` and `bound` on `Resize` (M4). Replace `inclusive: bool` with a four-way `Bound`, and give `size` a unit type (L3). Drop the remedy variants that restate the kind (M2) |
| D8 keep `EvalError`'s predicates for now | Disagree | T1's `SolveError::kind()` cannot be written without a kind from `E` (H2). Add `kind()` to `EvalError` now and derive the predicates from it |
| D9 inherent `kind()` and `name()`, no trait | Agree | No caller needs `&dyn Cause` yet. If one does, raising `rust-version` is cheap (the owner allows newer toolchains, and 1.99 is installed) |
| D10 wait for the RFC's ops `Error` | Agree | Keep §2.1 mappable into it. The RFC's open points (`Unclassed`, the code registry) need not shape scx |

**Not in D1 to D10, to add:**

- **D11.** Rule 1's default is inherit until boundary validation exists (H1).
- **D12.** Define "transparent" versus "context" frames for `Display` and `source()` (H3).
- **D13.** Warnings replay on cache hits and merge in item order (M7).
- **D14.** A size budget for errors, with `result_large_err` enabled (M5).
