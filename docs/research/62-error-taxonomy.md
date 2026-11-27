# 62. Error taxonomy: kinds, causes, chains, remedies

As of 2026-11-27. Trust: READ (today's scx error types, the pin's error model, the RFC), MEASURED
(the site counts below, by `grep` over non-comment lines), with the design JUDGED. Nothing was
built or run. Section 9 lists the decisions that needed the owner; the rulings of 2026-11-27 are at the end, and
bracketed notes in the body mark each place a ruling changes the text.

This is the "Error taxonomy" item of [05-status](../05-status.md) §6. It extends
[01-architecture](../01-architecture.md) §5 and rules on the points 01 leaves open. Where it
changes 01's text, it says so.

## 0. Sources

| Source | What it gives | Mark |
|---|---|---|
| 01 §5.1–§5.3, §2.3, §4, §6.3, §8 | One error type per crate, `kind()`, `Defect`, `Unfinished`, `Done`/`Report`, `Cx`, lowest-index rule | READ |
| [04-decisions](../04-decisions.md) (11-26) | `EvalError`'s three predicates; `NumError` matches the pin one for one while the difftest compares it | READ |
| [58](58-p21.md) §7 (owner, 11-27) | `ErrorKind` gains `Unsupported` and `Cancelled`; `P21Error::Within`; `Cycle` and `TooManyInstances` stay two causes | READ |
| [60](60-other-kernels.md), [61](61-g2-conic-fillets.md) | Causes already ruled for merge, coincidence, transforms (60) and blend corners, G2 (61) | READ |
| Pin `solidcore/src/error.rs` | `SolidError`, `Failure { operation, reason, site, fit, cause }`, 54-kind `ErrorKind` with four response predicates | READ |
| Pin `solidcore-ops/src/error.rs` | Flat ops `Error`, about 60 variants, with `Status(i32)`, `Record(String)`, `Internal(String)` | READ |
| Pin `docs/failure-codes-and-reproduction.md` §3.1, §6 | `FailureCode { call, kind, operation }`; the guide for converting failure sites | READ |
| SolidCraft `docs/rfcs/0016-failure-signatures.md` | Signature = command · call · kind · operation; the kind is the bucket and never changes meaning | READ |
| SolidCraft issue #22563, "RFC: solidcore's error model — typed causes, stable kinds, a remedy, one public ops error" (open draft, 2026-11-26) | Kind from cause; `Response`; derived `Remedy`; one ops error struct; kind-code registry | READ |

The RFC is not in the pin's docs. It is an open issue, so this doc adapts to it and does not depend
on it. Where the two agree, this doc says so. Where they differ, §8 records why.

## 1. Vocabulary

Terms agreed with the owner: a **cause** is a typed variant. A **kind** is an `ErrorKind`. A
**response group** is the pin's defect, refusal, sized or port-failure class. No new general kinds.

### 1.1 The kinds

`ErrorKind` lives in `scx-num`. It is `#[non_exhaustive]`, `Copy`, `Eq` and `Hash`. Each kind is one
thing the caller does next.

| Kind | Meaning | The caller | Pin response group |
|---|---|---|---|
| `Refused` | The input is invalid, or cannot hold as asked: a bad argument, a false stated fact, a size that does not fit | Changes the input. The remedy (§4) may say how. [ruled: fixed caps on input size are `Exhausted`, not `Refused`, M1] | refusal, sized |
| `Unsupported` | The input is valid, and this method cannot build it | Tries another method or reports the gap. [ruled: "never fix your input" is dropped; the kind is the coarse route and a remedy may differ from it, as with `RailLeavesFace`, M2] | refusal (a separate kind here) |
| `Exhausted` | The caller's work budget ran out [ruled: widened to a limit the caller or the machine sets, with typed limit causes: work, allocation, depth, count, memory; p21's `TooDeep`, `TooMany`, `TooManyInstances` and `NeedsMemory` are such causes, M1] | Gives more budget, or stops | (`OutOfFuel`) |
| `Cancelled` | The caller's `Stop` fired | Nothing | (`Cancelled`) |
| `Defect` | Our fault: an invariant we proved broke | Reports a bug with the input. Never handles it | defect |

JUDGED: five kinds is the floor. Merging any two would merge two different caller actions. Adding
one needs a sixth action. This is the RFC's own rule (§5.2: "a cause gets a kind of its own only when
its response class or its remedy rule differs").

**Name.** 01 §5.1 calls the first kind `InvalidInput`. This doc names it `Refused` [ruled D1: `Refused`]. It covers more than malformed arguments: a radius too large, two vertices within coincidence,
a false stated incidence (60). It also matches the variant name the operation errors use.

**What leaves the enum.** Today's `ErrorKind` (READ, `scx-num/src/error.rs`) has six kinds:
`InvalidInput`, `Degenerate`, `StageFailed`, `Failed`, `Unsupported` and `Cancelled`. Three of them
are pin causes, not caller actions. They become causes, and their kind is judged per cause (§3.1).

**What the pin has that this does not.** There is no "port failure" group: a port gap is
`Unsupported`, or a test failure. There is no `Unclassed`: every cause names its kind in an
exhaustive match, so nothing is "neither" by omission (the gap the RFC's §5.3 closes upstream).

### 1.2 Warnings

A warning is a typed value returned beside a success. It is never printed or logged, and it never
changes a result. Each one names the entity it concerns. [ruled D13: warnings are part of a
result. They replay on a cache hit and merge in item order, and warnings from items above a
reported failure or the budget cut are dropped. A warning that `Unproven` implies is derived from
the record, not stored twice. `Unproven` changes no value, but its recorded accuracy legitimately
affects later operations, so "never changes a result" holds per call only.]

- Operation warnings (`Warning::AccuracyPastResolution`, `ContinuityLowered { face, reached }` from
  61) live in one `#[non_exhaustive]` enum with `Report`, in `scx-brep`. A warning names entities,
  so it sits where the ids are.
- A crate below `scx-brep` with warnings of its own keeps its own enum and returns it. `scx-p21`
  does this today (`Parsed::warnings`: `DuplicateId`, `SkippedHeaderRecord`,
  `UnsupportedCodePage`, `TextAfterEnd`). An operation that reads STEP wraps them in one operation
  warning variant that holds the p21 value.
- A warning is not a downgraded error. If the caller asked for G2 as a hard requirement, missing it
  is `Unsupported(ContinuityNotReached)` (61). If the caller did not, it is a warning.

### 1.3 What is not an error

These outcomes are successes, and they live in `Done` and `Report` (01 §5.2):

| Outcome | Where it goes | Why it is not an error |
|---|---|---|
| `Basis::Unproven` (a certificate budget ran out, or a user's f64-only surface) | The result's `Basis` and a warning | The result exists and is correct as far as it is known. 01 §4: valid input is never refused for accuracy |
| Lost accuracy past the resolution | `Warning::AccuracyPastResolution` and a `Raise` | Same rule |
| A G2 corner built G1 | `Warning::ContinuityLowered` | 61 |
| A skipped item under `Skip` | `Report::skipped` | The caller chose to go on |
| An empty search ("no intersection", "the boxes are disjoint") | `Ok(None)` or an outcome enum | The pin's conversion guide (§6): an empty search is not a failure, and control flow is not a failure |

`Exhausted` stays an error for the caller's budget (01 §6.3). A certificate budget is internal, so
its running out gives `Unproven`, not `Exhausted` (61). [ruled H4: the certificate budget is a fixed
per-operation amount, never a share of the caller's remaining budget.]

## 2. Causes, chains and Display

### 2.1 A crate's error

Each crate has one public error type (01 §5.1). The kind comes from the cause's type and variant,
never from a field a site sets. No cause type sits under two kinds.

01's sketch breaks this. `Refused(BlendCause)` and `Unsupported(BlendCause)` let one cause appear
under both kinds, so a site decides the kind. `Items` holds causes of mixed kinds. The template for
an operation crate is:

```rust
#[non_exhaustive]
pub enum BlendError {
    /// One failure. `BlendCause::kind()` is `Refused` or `Unsupported`, per variant.
    Cause(BlendCause),
    /// Several items failed under the collect-all mode (§5). Non-empty, in item order.
    /// [ruled D3: collect-all is opt-in; the default reports only the lowest-index failure]
    Items(Vec<Skipped<BlendCause>>),
    /// The caller's budget ran out at `item`, after `work` units.
    /// [ruled M1: widened to typed limit causes (work, allocation, depth, count, memory)]
    Exhausted { item: usize, work: u64 },
    Cancelled,
    Defect(Defect),
}

impl BlendCause {
    pub const fn kind(&self) -> ErrorKind { /* one arm per variant, no wildcard */ }
}
```

`BlendError::kind()` is the cause's for `Cause`, the lowest item's for `Items`, and the variant's for
the rest. A crate whose outcomes have no items, and whose causes split cleanly by kind, may keep
kind-named variants with one cause type per kind. `scx-p21` does this (`Refused(Refusal)`,
`Unsupported(Unsupported)`). Both layouts keep the rule.

`Defect { at: &'static str, what: DefectKind }` stays as 01 has it. `at` is a code location, so the
`&'static str` here is a location and not a reason.

### 2.2 No phrase causes, no status codes

- A reason is a variant. A `&'static str` reason (today `NumError::InvalidInput { reason }`,
  `SolveError::InvalidInput { reason }`, `EntityCause::Invalid { what }`) becomes a variant with its
  data. MEASURED: 39 reason literals in 11 `scx-num` files, 12 in 3 `scx-algo` files, and 20 calls
  of the `invalid(id, what)` helper in `scx-p21`.
- A `String` field is allowed only for data from the input, such as the token text in
  `scx-p21::Syntax` or a found entity name. It is never prose written by us.
- No field holds an SMLib status number, and no API takes or returns one. One reason text names an
  SMLib constant ("a series' orders exceed SM_GN_MAXS or SM_GN_MAXE"). Its variant names the limit
  by value.
- `operation: &'static str` paths stay only where the difftest needs the pin's text (§6). They name
  the pin routine. Nothing branches on them.

### 2.3 Chaining

A lower error lives in a typed field of the upper cause. `source()` returns it. [ruled D14: a lower
error in a cause field is `Box`ed where it would grow the enum, e.g. `by: Box<SsiError>`.] There is no
`LowerError` enum naming every lower crate (01 §5.1's `Lower(LowerError)` is dropped, D4). A blend
cause that failed in a surface intersection holds the `SsiError` in that variant:

```rust
BlendCause::RailNotTraced { edge: EdgeId, by: SsiError }
```

The rules at each boundary:

1. **The wrapper judges the kind.** [ruled: inherit by default, D11. A wrapper re-judges only when it
   built the data it passed down: `Defect` if it proved that data valid, otherwise `Unsupported`.
   Long term, public entry points validate the caller's input, so a lower `Refused` from our own
   data is a bug by construction. Blanket cross-crate `From` impls that inherit silently are dropped
   or limited to where a pass-through is correct.] The kind is inherited only when the wrapper passed the
   caller's data through unchanged. If the wrapper built the data it handed down, a lower `Refused`
   is the wrapper's fault. It becomes `Defect` when the wrapper proved the data valid. Otherwise it
   becomes `Unsupported`: the method made a case it cannot finish. This is the RFC's §8.2 re-raise
   rule ("fault belongs to whoever built the argument, and only that frame knows"), made the default.
2. **Budget and stop pass through as the upper variant.** A lower `Exhausted` becomes the upper
   `Exhausted` at the current item, and a lower `Cancelled` becomes the upper `Cancelled`. Neither
   is kept as a source: they carry nothing more.
3. **A lower `Defect` stays a `Defect`**, with the lower error as source. A defect is never
   downgraded.
4. **Context frames.** [ruled D12: two frame types. A transparent frame forwards `source()` and
   Display to the inner error and adds nothing; `Num` and `Eval` in `SolveError` and p21's
   `Unwritable::Geometry` are transparent. A context frame prints only its own text, and `source()`
   returns the inner error. `Within` is a context frame that adds no kind.] A frame that only says where (`P21Error::Within { id }`, an
   item index, a face) adds no kind. `kind()` and the cause pass through it. 58 §7 ruled this for
   `Within`.
5. **Wrap, don't flatten.** A wrapper never copies a lower cause's text or fields into its own. The
   lower value is kept whole, so a test or a caller can match it.

JUDGED: rule 1 as first written reverses the pin's default [ruled D11: the default is inherit, as the RFC has it; the reversal applies only to a frame that built its data]. The pin takes the deepest classed error in the chain
(`deciding_class`). That needs a search, and gives no answer when nothing is classed. Here every
frame states its kind, so `kind()` reads the outer frame and does no search [review L3: `P21Error::kind()` still walks the `Within` chain, O(depth); the claim of no search is dropped]. A test can check each
frame.

### 2.4 What Display shows

- `Display` prints the error's own frame only, in English, for a developer. It never prints its
  source. A chain reporter prints the frames joined by ": ". This is the std convention, and it
  avoids double printing.
- Today all three crates double print (READ). `NumError` writes "op: failure; caused by {c}" and also
  returns `c` from `source()`. `SolveError::CallFailed` writes "caused by {cause}". `P21Error` walks
  the `Within` chain itself and also returns the inner error. `Refusal` prints its cause and returns
  it too.
- `scx-num` gets one std-only helper, `error::chain(&dyn Error) -> impl Display`, that joins the
  frames. It needs no trait upcasting.
- Display text is not an interface (01 §5.1). No code branches on it. User text belongs to the app
  (§4).
- p21's one-line form ("STEP input refused: in #12: in #11: #10: ...", 58 §7) is what `chain`
  prints. It is no longer built by `P21Error`'s Display. [ruled M6: `chain()` must reproduce p21's
  current one-line form, with the class word first, or p21 keeps its own formatter over the chain.
  The claim above was false for a plain frame-by-frame join.]

## 3. Rules per layer

| Layer (01 §1.1) | Error | Kinds it may return | Notes |
|---|---|---|---|
| 0: `scx-num` | `NumError` | `Refused`, `Unsupported`, `Defect` | Owns `ErrorKind` and `chain`. Its argument is all it has, so its checks refuse. No budget, no stop |
| 1: `scx-algo` | `SolveError<E>` | Its own, plus the evaluator's through `Eval` | Generic over the evaluator. `source()` on `CallFailed` only; `Num` and `Eval` are transparent [ruled D12, H3] |
| 1: `scx-p21` | `P21Error` | `Refused`, `Unsupported`, `Exhausted` (limit causes), `Cancelled` | Takes `Progress`, so it may cancel. No work budget yet; fixed caps are `Exhausted` limit causes [ruled M1] |
| 2–6: geometry and body crates | One per crate (`SurfaceError`, `SsiError`, ...) | Any of the five. `Exhausted` and `Cancelled` only in a crate that takes a budget and `Stop` (01 §6.3) | Wrap lower errors in cause fields (§2.3) |
| 4: `scx-brep` | `Unfinished` for `finish`; `Defect` | `Refused` via `FinishCause`, or `Defect` | Also holds `Warning`, `Report`, `Fit`, `Remedy` (§4) |
| 7–11: operations | `BoolError`, `BlendError`, ... | All five | The §2.1 template. Holds `Items` |

**A wrapper names only its direct callees.** A blend cause holds `SsiError`, not `NumError`. The
`NumError` is in the `SsiError`'s chain. This keeps each enum to the crates one layer down that it
calls.

**`Unfinished` from our own operation is a `Defect`** (01 §2.3). An operation that gets an
`Unfinished` from `finish` builds `Defect` with the `Unfinished` as source, unless the draft's
failure is a stated fact from the caller (60: a false stated incidence is `Refused(FalseFact)` at
the public API).

### 3.1 Today's causes, re-kinded

| Cause (today) | Kind today | Kind here | Why |
|---|---|---|---|
| `NumFailure::IndexOutOfRange`, `NotSquare`, `NotSymmetric`, `SpanOutsidePeriod`, `TooFewRows`; every `NumError::InvalidInput` | `InvalidInput` | `Refused` | A bad argument |
| `Singular`, `VectorVanishes`, `NoPerpendicular` | `Degenerate` | `Refused` | At layer 0 the degenerate value is the caller's argument. A caller that built it re-judges (§2.3 rule 1) |
| `Disjoint`, `PointOutsideExtent` | `Failed` | `Refused`, and a task (§7, T5) to check each caller [ruled: not re-kinded until T5 has read the callers, review M3] | Often a search result. Where a caller branches on it, it becomes an outcome, not an error |
| `JacobiDidNotConverge`, `SolveFailure::BrentDidNotConverge` | `StageFailed` | `Unsupported` [ruled D2/M3: Jacobi is `Defect`, after a finiteness check that refuses non-finite input; Brent stays `Unsupported`] | A fixed internal cap, not the caller's budget. The input is valid and the method gave up |
| `SolveFailure::ZeroDifferenceStep` | `Failed` | `Unsupported` | Same |
| `NumFailure::Defect { why }` | `StageFailed` | `Defect` | Reviewed as our fault |
| `CallFailed { class }` | the callee's | inherited, re-judged only by a frame that built the data (§2.3 rule 1, D11) | The `class` copy goes. The kind is read, not stored |
| `P21Error` | `kind()` of the innermost | unchanged | Context frames are transparent (58 §7) |

The pin's kinds are not lost. The difftest facade maps each cause to the pin's kind and text (§6).
The re-kinding changes no result, only `kind()`.

## 4. Remedy

The RFC (§5.4) adds a remedy: "what to do about it", as data, from a closed list. This doc takes it,
with three rules from the RFC that fit the owner's rules:

1. **Derived, never stored.** `fn remedy(&self) -> Option<Remedy>` is an exhaustive match over the
   cause. A stored field could disagree with the cause, as a stored kind could.
2. **Data, never text.** The kernel holds no UI text. An app turns `Remedy` into words, in its own
   language. `Display` stays an English diagnostic.
3. **No remedy is invented.** `None` for `Cancelled`, and for any cause with no action to offer. A
   remedy may differ from the kind, as with `RailLeavesFace` [ruled M2].

```rust
// scx-brep
pub enum Bound { AtMost, AtLeast }

// [ruled D7: `Fit.inclusive: bool` is the pin's flag shape; a four-way `Bound` (review L3) and a
// unit-typed `size` are open points for the builder, not rulings]
#[non_exhaustive]
pub struct Fit {
    pub size: f64,          // finite and positive; "no size builds" is no Fit
    pub measure: Measure,   // Radius, Setback, Distance, Thickness, ...
    pub bound: Bound,
    pub inclusive: bool,    // true when exactly `size` builds
}

#[non_exhaustive]
pub enum Remedy {
    // [ruled M4: `Resize` keeps `measure` and `bound` when no size is proved]
    Resize { at: Site, measure: Measure, bound: Bound, fit: Option<Fit> },
    ChangeInput { at: Site, what: InputChange },
    // [ruled M2: the variants `Unsupported`, `MoreBudget` and `Report` only restate the kind and
    // are dropped; `None` where the kind says it all]
}
```

`Site` names entities by id (edge, face, vertex) and may carry a point. It sits in `scx-brep` because
the ids do. Layers below have no remedies. Their causes keep the data (for example the size that
fits), and the operation cause that wraps them derives the remedy.

`Measure` and `InputChange` are closed lists in `scx-brep`, grown by variants. A per-operation
parameter type (`Fit<P>`) was considered and rejected (§8, A7).

The pin has no `Remedy` (the RFC says so, §4). Its `Fit { size, measure, edge }` has no direction and
uses size 0 for "no size". The pin's `docs/failure-codes-and-reproduction.md` §6 already asks that
"where a check measures a size, the refusal carries the size that would pass". The RFC adds `bound`
and `inclusive`, and drops the zero. This doc takes all three.

## 5. Determinism

1. **Same input, same cause.** A cause holds only values computed from the input: ids, indices,
   sizes, points. Never an address, a time, a thread id, or a value read in hash order.
2. **Lowest item index wins** (01 §8). [ruled H4: each item runs under the full budget and records the
   work at which each event happened. The merge walks the items in order and counts an event only if
   the prefix work plus that event's work is within the budget; otherwise the outcome is
   `Exhausted` at that item. `Scoped` may stop items above the lowest failing index early, as an
   optimisation that does not change results.] In a parallel pass, the reported outcome is the event at the
   lowest item index, among item failures, the budget passing, and defects. Ties at one item go to
   the order the item's own code would meet them in serial.
3. **One rule for defects.** [ruled H4: the same work test applies to defects.] A defect at a lower index than the reported failure wins. Defects at
   higher indexes go to `Trace`, which cannot affect results.
4. **`Cancelled` is the one timing-dependent outcome.** [ruled: it may race a failure and return either.] It carries nothing, so two runs that cancel
   differ only in whether they cancelled.
5. **Fix 01 §5.2.** It reports the first failure "plus any other failures found without extra work".
   Which extra failures a parallel run finds depends on scheduling. The default is to report only the
   lowest-index failure, as `Cause`. An explicit collect-all mode runs every item and returns `Items`
   in item order (D3). [ruled D3/H4: collect-all and `Skip` meter the same way as the default, and
   no algorithm sizes anything from its remaining budget.]
6. **Budget runs out at a deterministic item.** Work is summed per item in index order (01 §6.3), so
   `Exhausted { item, work }` is the same on every run and thread count.

The pin's HLR reports the first failure in time (07 #7). Where the difftest compares an HLR failure,
it compares the outcome only, until the pin is fixed or the case is excluded.

## 6. Tests and the difftest

**Unit tests assert on the value.** `assert_eq!(err, BlendError::Cause(BlendCause::TooLarge { .. }))`
or `matches!`. A test of `kind()` is a table over the variants. Display gets at most one test per
variant, and no other test reads text. MEASURED: about 11 `scx-p21` tests assert on Display text
today. `part42_round_trip.rs` walks `source()` to check that the last frame's text equals the
cause's. That check moves to a value match.

**Names.** [ruled D6/D9: signatures use cause `name()`, not numeric codes; `kind()` and `name()` are plain methods per type, with no shared trait, and rust-version stays 1.85.] Each cause has `fn name(&self) -> &'static str`, a stable snake_case word, with a test
that names are unique within the crate. A signature (RFC 0016) is call · kind · cause name, with the
pin's operation path where the facade has one. Names are the stable bucket, so no numeric code table
is needed here (D6).

**Golden and outcome rows record the kind only** (agreed). The exact cause belongs in unit tests.

**The difftest compares through the facade.** Today `impl Bits for NumError`
(`scx-difftest/tests/diff/facade.rs`) tags the kind's Debug name, the operation, the reason or
failure fields, and the `to_string()` text, and recurses into the cause. The pin side does the same.
Once kinds and Display change:

- The facade maps each scx cause to the pin's `(ErrorKind, reason text)`. One `match` per crate,
  exhaustive, kept in the difftest. Nothing in the scx crates names the pin's kinds.
- The facade walks the scx chain and the pin chain, frame by frame, and compares the mapped values.
- A cause whose pin text differs on purpose (a fixed message, a typed reason) gets its mapping
  written once. That is the "facade maps variants" rule of 01 §5.3, now applied to `scx-num` too.
- `solvers.rs` (`solve_pin`) keeps its one-for-one map. `t_p21.rs` keeps comparing outcomes only.

04 ruled that `NumError` strings match the pin while the difftest compares them. The facade mapping
keeps that comparison and lets scx's Display change. That replaces the 04 rule (D5). [ruled D5: the
rule is replaced. The facade already compares `InvalidInput` by class alone (review L2), so only
`Failed` needs a mapped text.]

## 7. Migration

Sized as builder tasks. Each is a small, separate change. Sizes are MEASURED sites from the counts
in §2.2 and below. T1 is M9.

| # | Task | Crates | Sites (MEASURED) | Size | Depends on |
|---|---|---|---|---|---|
| T1 (M9) | `EvalError` gains a required `fn kind(&self) -> ErrorKind`, and its three predicates (`ends_solve`, `passes_through`, `never_ignored`) are derived from it [ruled D8]. `SolveError`: `kind()`; `CallFailed` is a context frame with `source()`; `Num` and `Eval` are transparent frames forwarding `source()` and Display to the inner error [ruled D12, H3]. Folds in the old T11 | algo, difftest | 1 impl, 19 `SolveError::` sites; 3 `EvalError` impls (2 tests, `XErr`) | S | none |
| T2 | `ErrorKind` to the five kinds [ruled D1: `Refused`]; `#[non_exhaustive]` on `NumFailure`, `SolveFailure` (57 F18) [review L3: on `ErrorKind` itself only at 01 §7.4's publication step, or accepted explicitly]; re-kind per §3.1 as ruled (Jacobi `Defect` after a finiteness check, `Disjoint` and `PointOutsideExtent` left until T5); facade maps causes to pin kinds | num, algo, p21, difftest | 42 `ErrorKind::` lines, 44 `NumFailure::`, 2 test asserts on `Degenerate` | M | T1 |
| T3 | Two frame types (transparent, context) [ruled D12]; one frame per Display; `error::chain`, reproducing p21's one-line form or leaving p21 its own formatter [ruled M6]; facade compares chains frame by frame | num, algo, p21, difftest | 5 or more Display impls (`Unwritable::Geometry` included), ~11 p21 text tests | M | T2 |
| T4 | Typed reasons for `NumError::InvalidInput` and `SolveError::InvalidInput`; the facade needs no mapped reason text for `InvalidInput` (compared by class only) [ruled D5]; fixes the hash_table tests that read reason text | num, algo, difftest | 51 literals in 14 files; 7 direct `NumError::InvalidInput {..}` in `hash_table.rs` | M (split by file if needed) | T3 |
| T5 | `Disjoint` and `PointOutsideExtent`: read each caller. Where it is a search result, return an outcome; re-kind only after | num | 2 causes, callers in `extent2d.rs` | S | T2 |
| T6 | `is_bad_jacobian` matches the typed cause (`Singular`), through `CallFailed` wrappers, not "any `Failed`"; check which causes can reach it first | algo | 1 fn, 1 call | S | T2 |
| T7 | `EntityCause::Invalid { what }` to typed variants; `WrongType.expected` and `BadParam.expected` to a typed entity name if one exists | p21 | 20 `invalid(` calls | M | T3 |
| T8 | `fn name()` on every cause, plus a uniqueness test per crate [ruled D6, D9] | num, algo, p21 | every cause enum | S | T4, T7 |
| T9 | p21 rules: `Within` is a context frame; `within()` keeps not wrapping `Cancelled`; Display one frame | p21 | 16 `.within(` sites | S | T3 |
| T10 | `Warning`, `Report`, `Fit`, `Site`, `Remedy` types in `scx-brep` when that crate starts, with the rulings D7 (M2, M4) and D13 | brep | new | S | none (ruled) |
| T11 | Folded into T1 [ruled D8] | | | | |
| T12 | Box lower errors in cause fields and enable clippy's `result_large_err` at its default threshold for the workspace; errors stay off the success path's size [ruled D14]. Report before and after `size_of` for T2 and T4 | all | every cause field holding a lower error | S | T2 |
| T13 | Typed limit causes under `Exhausted` (allocation, depth, count, memory) [ruled M1]: `ObjsInVoxels` allocation failure, p21 `TooDeep`, `TooMany`, `TooManyInstances`, `NeedsMemory` | num, algo, p21 | 1 allocation site (`hash_table.rs:205-211`), 4 p21 causes | S | T2 |

Each task keeps the difftest green. [Ruled H4: a task that reads remaining budget to size work is not allowed.] No golden moves, since none records an scx kind yet.

## 8. Adversarial check and decisions

Each decision with its rejected alternative.

| # | Decision | Rejected alternative | Why |
|---|---|---|---|
| A1 | Five kinds, each one caller action | Keep the pin's 54 kinds or today's six | A kind that is not an action is a cause. The pin's list mixes the two (RFC §4: "a kind in neither list answers false to both") |
| A2 | Kind from cause type and variant; no cause under two kinds | 01's `Refused(BlendCause)` / `Unsupported(BlendCause)` | A site-chosen kind can disagree with the cause. It is the stored-field problem the RFC removes upstream |
| A3 | The wrapper judges the kind at each boundary [ruled D11: inherit by default; re-judge only a frame that built its data] | The pin's "deepest classed error decides" | Explicit per frame, no search, testable per frame. Same answer as the RFC's §8.2 re-raise rule, stated as the default |
| A4 | Lower errors in typed cause fields | 01's `Lower(LowerError)` naming every lower crate | A `LowerError` grows with every crate added below. Typed fields name only direct callees and say where in the operation the failure happened |
| A5 | Display one frame; `chain` joins [ruled D12: two frame types] | Each Display prints its chain | Today's double printing in all three crates. The std convention is one frame |
| A6 | Default reports the lowest-index failure only; collect-all is explicit [ruled D3 with the H4 rules] | 01 §5.2's "plus failures found without extra work" | The extra set depends on scheduling, which breaks the determinism rule |
| A7 | One `Fit` with a closed `Measure` list in `scx-brep` | `Fit<P>` with each operation's parameter enum | An app reads remedies across operations with one match. The RFC's shape is shared across kernels |
| A8 | Remedy derived by a function | A stored field, or text | A stored field can disagree; text puts UI in the kernel |
| A9 | Cause `name()` strings as the stable bucket | A numeric kind-code table like the pin's (and the RFC's registry) | A second table to keep in step, shaped like the status codes being removed. The facade can still emit pin codes for the pin's cases |
| A10 | `Unsupported` for fixed internal caps (Brent) [ruled: Jacobi is `Defect`, M3; fixed caps on input size are `Exhausted` limit causes, M1] | `Exhausted` | `Exhausted` means the caller's budget. More budget would not help a fixed cap |
| A11 | p21 keeps kind-named variants | One layout (`Cause`, `Items`, ...) everywhere | p21 has no items, and its causes split by kind already. A change would churn tested code for nothing |

**Against the owner rules.**

| Rule | Check | Result |
|---|---|---|
| Idiomatic Rust | `std::error::Error` with `source()`, one frame per Display, `#[non_exhaustive]`, `matches!` on values | Meets. M9 and the double printing are the current gaps |
| Determinism | §5. Causes hold input-derived values only; lowest index wins; `Cancelled` carries nothing | Meets once A6 lands. HLR in the pin does not, and the difftest compares outcomes only there |
| Honest signatures | One error type per crate; the kind is visible in the type; no `Result<_, String>` | Meets. `Unfinished` from `finish` stays the honest type for the draft API |
| No ambient state | No thread-local failure code. The pin's ops keeps its code in `NOTED`/`CODE` thread-locals; the RFC §6.3 moves it into the error. Here it is in the error from the start | Meets |
| std only | `chain` and the traits use `core::fmt` and `std::error` only | Meets. A shared `trait Cause: Error { fn kind(); fn name(); }` returned as `&dyn Error` needs trait upcasting, which is stable since Rust 1.86. `rust-version` is 1.85 (READ, `Cargo.toml`). Either raise it (D9) or keep `kind()` and `name()` inherent per type, which needs no trait [ruled D9: inherent] |

**Against what callers need.**

| Caller | Needs | Gets |
|---|---|---|
| An app | What to do, where, on which entity | `kind()`, `remedy()` with `Site`, and the cause |
| An agent retrying a size | Which way, and whether the named size itself builds | `Fit.bound`, `Fit.inclusive` |
| A higher operation | Whether to retry, skip or stop | `kind()`. Within the solver, `EvalError`'s predicates |
| A bug report | A stable bucket and the chain | call · kind · cause `name()`, and `chain()` text |
| The difftest | The pin's kind and text | The facade map (§6) |

**Weak points found.**

- Rule 1 of §2.3 asks each wrapper to know whether it built its callee's data. A wrapper that gets
  it wrong reports `Unsupported` where `Defect` was right, or the reverse. Mitigation: start
  conservative (inherit) at each site until its callers are read, as the RFC §8.2 says for the pin
  [ruled D11: this is now the default, not a mitigation].
- `Refused` at layer 0 for `Singular` means a solver that builds a singular matrix must re-judge
  it. Most do already: `is_bad_jacobian` branches on it and retries (T6).
- The RFC's one-struct ops `Error` (§6.1) is a proposal for a shared public shape. If the owner wants
  scx to expose that shape at its public top, the operation errors of §2.1 map into it. That needs a
  mapping layer, not a change to §2.1.

## 9. Decisions that need the owner

| # | Question | This doc's lean |
|---|---|---|
| D1 | Name the first kind `Refused`, not 01's `InvalidInput` | `Refused` [ruled: `Refused`] |
| D2 | Re-kind today's causes per §3.1 (`Degenerate` to `Refused`; `StageFailed` and `Failed` to `Unsupported` or `Defect`), with the facade mapping pin kinds | Yes [ruled: yes, with Jacobi `Defect` after a finiteness check (M3) and limit causes to `Exhausted` (M1)] |
| D3 | Change 01 §5.2: report the lowest-index failure only by default, collect-all explicit | Yes [ruled: yes, with the H4 rules] |
| D4 | Change 01 §5.1: drop `Lower(LowerError)` and `Refused(Cause)`/`Unsupported(Cause)` sharing one cause type, for the §2.1 template | Yes [ruled: drop `Lower(LowerError)`; no cause type under two kinds; Box lower errors (D14)] |
| D5 | Replace 04's rule that `NumError` strings match the pin, with the facade mapping causes to pin text | Yes, in T3 and T4 [ruled: replaced] |
| D6 | Signatures use cause names, not numeric kind codes; no scx entry in the RFC's code registry | Names. Revisit if SolidCraft needs scx codes in its registry [ruled: names] |
| D7 | Take the RFC's remedy (`Remedy`, `Fit.bound`, `Fit.inclusive`) in `scx-brep` | Yes [ruled: yes, with M2 and M4 changes] |
| D8 | `EvalError` (owner-ruled 11-26): keep the three predicates, or derive them from `kind()` | Keep for now. Derive once operation kinds exist [ruled: derive from `kind()` now; T1 unblocked] |
| D9 | Raise `rust-version` to 1.86 for a shared cause trait, or keep `kind()`/`name()` inherent | Inherent. No trait is needed yet [ruled: inherent; rust-version stays 1.85] |
| D10 | Expose the RFC's one-struct ops `Error` at scx's public top, if the RFC is accepted | Wait for the RFC [ruled: wait for the RFC] |

## 10. Rulings (2026-11-27)

The owner ruled on D1 to D10 and on the review's D11 to D14, M1 and M6, adopting the review's
corrections. Each place in the body that a ruling overturns carries a bracketed note.

- **D1.** The first kind is `Refused`, not `InvalidInput`.
- **D2.** Re-kind as §3.1 says, with the review's changes. Jacobi non-convergence on a finite
  symmetric matrix is `Defect`, after a finiteness check that refuses non-finite input (M3).
  `Disjoint` and `PointOutsideExtent` wait for T5.
- **D3.** By default only the lowest-index failure is reported; collect-all is opt-in. H4's rules
  are adopted:
  - each item runs under the full budget and records the work at which each event happened;
  - the merge counts an event only if the prefix work plus that event's work is within the budget;
  - no algorithm sizes anything from its remaining budget, including 61's certificate budget, which
    becomes a fixed per-operation amount;
  - collect-all and `Skip` meter the same way;
  - `Scoped` may stop items above the lowest failing index early, as an optimisation that does not
    change results.
- **D4.** Drop `Lower(LowerError)`. No cause type sits under two kinds.
- **D5.** The facade maps scx causes to the pin's kind and text, and the 04 rule that `NumError`
  strings must match the pin is replaced. The facade already ignores `InvalidInput` text.
- **D6.** Signatures use cause `name()`, not numeric codes.
- **D7.** Adopt the RFC's remedy in `scx-brep`, with two changes: `Resize` keeps measure and bound
  when no size is proved (M4), and remedy variants that only restate the kind are dropped (M2). A
  remedy may differ from the kind, as with `RailLeavesFace`.
- **D8.** `EvalError` gains a required `fn kind(&self) -> ErrorKind`, and its three predicates are
  derived from `kind()`. This unblocks T1; T11 merges into T1.
- **D9.** `kind()` and `name()` are plain methods per type; rust-version stays 1.85.
- **D10.** Wait for the RFC.
- **D11.** By default a wrapper inherits the lower kind. It re-judges only when it built the data it
  passed down: `Defect` if it proved that data valid, otherwise `Unsupported`. Long term, public
  entry points validate the caller's input, so a lower `Refused` from our own data is a bug by
  construction.
- **D12.** Two frame types. A transparent frame forwards `source()` and Display to the inner error
  and adds nothing. A context frame prints only its own text, and `source()` returns the inner
  error. `Num` and `Eval` in `SolveError` and p21's `Unwritable::Geometry` are transparent.
- **D13.** Warnings are part of a result. They replay on a cache hit and merge in item order.
  `Unproven` changes no value, but its recorded accuracy legitimately affects later operations.
- **D14.** Lower errors in cause fields are `Box`ed where they would grow the enum. Clippy's
  `result_large_err` is on at its default threshold. Errors stay off the success path's size.
- **M1.** `Exhausted` is widened with typed limit causes (allocation, depth, count, memory). Fixed
  caps on input size, such as p21's `TooDeep`, `TooMany` and `NeedsMemory`, are `Exhausted` limit
  causes, not `Refused`.
- **M6.** `chain()` must reproduce p21's current one-line form, or p21 keeps its own formatter over
  the chain.

Migration: T2, T3, T4 (scx-num), T7, T8 (scx-num, scx-p21, scx-nurbs), T9, T12 and T13 done on
branch `errors-h` (§11); T1, T5, T6 and scx-algo's parts of T4, T8 and T12 wait for the algo port.

## 11. Implementation notes (2026-11-27, branch `errors-h`)

Decisions made while implementing, for review. None changes a ruling.

- **Pin defect found (T2).** Jacobi at the pin runs on non-finite input and reports
  non-convergence. The truth is that the eigenvalues are undefined. scx refuses at entry (M3).
  4 inputs move from exact to `pin_defects.rs`.
- **The facade maps to the pin's type (T2).** `num_as_pin` builds the pin's `SolidError` from an
  scx error, with the pin's kind and text (D5). The difftest compares the two pin-shaped values.
- **Which limits are `Exhausted` (T13, M1).**
  - These fixed caps are `Exhausted` with a typed `Limit`:
    - scx-num: `MAX_MATRIX_SIZE`, `GW_MAX_DERIV`, `SM_GN_MAXS`/`MAXE`, and an allocation that
      fails softly.
    - scx-p21: the byte, count and memory limits, plus `TooDeep` and `TooManyInstances`. These
      two carry the instance id.
  - An element count or voxel dimensions that overflow `usize` stay `Refused`. They are not a
    limit anyone sets; the arithmetic overflows.
  - p21's `NestedTooDeeply` and `TooManyParts` stay `Syntax` causes, because the header skip
    catches `Refused(Syntax)`.
  - **Needs a ruling:** p21's "more than 1 000 000 control points" caps are still
    `EntityCause::Invalid`, kind `Refused`. By M1 they would be `Exhausted`.
- **Frames (T3, T9; D12).**
  - Context frames: `Refusal::Syntax`, `Refusal::Entity`, `Refusal::Unwritable { id: Some }`,
    `NumError::Call` and `P21Error::Within`.
  - Transparent frames: `P21Error::Refused`, `P21Error::Unsupported`, `P21Error::Exhausted`,
    `Refusal::Unwritable { id: None }` and `Unwritable::Geometry`.
  - p21 keeps its own formatter, `P21Error::one_line` (M6). It writes the class word, then
    `chain`. Its output is the text that `Display` gave before.
  - `SolveError::CallFailed` still prints its cause twice. That is left to T1.
- **Typed reasons (T4, T7).**
  - `NumError::InvalidInput` carries `Invalid`, which has 22 variants.
  - `EntityCause::Invalid` carries `InvalidValue`, which has 25 variants. Each prints the old text.
  - `WrongType.expected` and `BadParam.expected` stay text, because the crate has no typed entity
    name.
  - The facade compares an invalid input by class only: `pin_text` prints that frame as
    "op: invalid input". It compares texts frame by frame, with the pin's "; caused by" removed.
- **Names (T8, D6).**
  - `name()` is defined on every cause enum.
  - `NumError::name` and `P21Error::name` give the name of the last cause in the chain.
  - A wrapping variant delegates to its cause, so the names are unique over the leaf causes.
- **scx-nurbs (merged during the work), re-kinded under T2 and T8.**
  - `Unstable` is `Unsupported`: the input is valid and the elevation cannot finish it.
    `Degenerate` (a knot difference rounded to zero, or an overflow) was the old `Degenerate`
    kind and is `Refused` per D2 and §3.1: the short span or far parameter is the caller's.
  - A degree above `MAX_DEGREE` and a derivative order above `MAX_ORDER` are `Exhausted` (M1). A
    degree of zero is `Refused`.
  - A missing index that the built values' counts rule out is a new `NurbsError::Defect`. Before,
    it was `Degenerate`.
  - All other errors are `Refused`.
- **Sizes (T12, D14).** No error type grew. `NumError` is 64 bytes and `P21Error` 56; `Result`
  sizes are unchanged. `result_large_err` is set explicitly at clippy's default. The scx-algo
  sizes are unchanged too: `SolveError<NumError>` is 80 bytes. Boxing them is left for after the
  algo port.
