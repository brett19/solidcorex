# Review: research 67, cross-platform bit identity

This is an adversarial review of `docs/research/67-cross-platform-bit-identity.md`. It was checked
against:
- 05 (the benchmark-gaps row, the golden-hash row, the CI row);
- 65 §2.1, §3.1 and §5.3;
- 01 §10.2;
- 07 §7;
- `crates/scx-num/src/fmath/mod.rs`, `crates/scx-num/src/eft.rs`, `clippy.toml` and
  `.github/workflows/ci.yml`;
- SolidCraft's `design-container.md`, `native-brep.md` and `perf.md`;
- tangency-labs/solidcore#1252, through `gh`;
- these pages, fetched again 2026-11-27 and read as data: RFC 3514, Box2D's determinism post,
  Rapier's determinism guide and its `rapier3d/Cargo.toml`;
- web searches for the OCCT, glibc, MinGW and kernel claims.

The labels follow 67:
- VERIFIED: checked here, from a file, a command or a fetched page.
- JUDGED: reasoning made here.
- UNVERIFIED: could not be checked.

Two small local experiments were run with the installed stable toolchains (Rust 1.99.1 and 1.100.0
on x86_64-unknown-linux-gnu). Nothing in the repository was changed apart from this file.

**Verdict.** The direction survives: keep D3 now, and stop paying for costs that D3 does not cause.
The case as written needs revising before the owner rules on it:
- One precedent is misread. Rapier's portable libm is not "faithful".
- Two Rust facts are wrong: fast-math is available on stable Rust, and the platform's `fma` is
  still linked.
- The step-4 gate cannot answer the question it is meant to settle.
- The contract the owner is asked to approve is not stated precisely enough to test. Its biggest
  gap is the toolchain.

Severities:
- **wrong**: the claim is false.
- **overstated**: the claim goes further than its evidence.
- **missing**: a consideration that changes the analysis.
- **minor**: wording or sourcing.

## Findings

### 1. Rapier's determinism is not "merely faithful" portable math (wrong)

**Where:** §1 item 2, §3 (the Rapier row and its reading), §4.2 rows 1 and 8, §5 (e) item 2.

**Problem:** 67 says a portable function that is "merely faithful" gives identical bits at close to
libm speed, and that "Rapier does exactly this". VERIFIED:
- Rapier's `enhanced-determinism` is `["simba/libm_force", "parry3d/enhanced-determinism"]`
  (rapier3d `Cargo.toml`).
- `libm_force` routes the functions through the `libm` crate.
- 04 §3 (11-27) measured that crate at 80-98% correctly rounded per function, with a worst case of
  496 ulp on `powf`.

That is neither faithful nor anywhere near it. 67 cites the 496 ulp figure itself in (b), so the
document contradicts itself. Rapier shows that *portable* math gives D3. It says nothing about
faithful math, and nothing about speed: neither Rapier nor this report measured the speed of the
`libm` crate against glibc.

**Fix:**
- Say that Rapier's D3 comes from the `libm` crate (a musl port, not faithful: 496 ulp on `powf`,
  04 §3).
- Drop "near libm speed" for faithful portable math, or label it INFERRED with no source.
- In (e) item 2, the faithful fallback is a new `fmath` variant that would have to be built and
  proved faithful. It is not an existing off-the-shelf option.

### 2. The step-4 gate cannot measure what it gates (overstated, and partly unmeasurable)

**Where:** §5 (e) item 6, §6 reply 4 ("unanswerable without measurement, so (e) includes the step-4
gate").

**Problem:** the gate is offered as the answer to adversarial point 4, which says the heavy layers are
unmeasured. As written it cannot answer it. VERIFIED and JUDGED:

1. **Wrong corpus.** Step 4 (01 §10.2) is analytic only: `scx-mass`, classification, primitives and
   extrude, Booleans *on analytic bodies*, and tessellation. The layers point 4 names (B-spline SSI
   marching, fitting, fillets, mass over NURBS) arrive in steps 5 to 10. A gate at step 4 measures
   the transcendental-heavy closed forms, where Ziv helps. It does not measure the mul-add-heavy
   NURBS loops, where FMA would.
2. **No mechanism for "FMA contraction where LLVM can".** rustc emits `fmul`/`fadd` without
   `contract` flags (RFC 3514), so LLVM never fuses them. The only stable routes are explicit
   `mul_add` and the `algebraic_*` methods (finding 4). Both mean editing source at every site, not
   a build flag. The gate does not say which route it uses. A backend `-C llvm-args` contraction
   option is UNVERIFIED for rustc.
3. **No target named.** On baseline x86-64 and wasm32 no contraction is possible, so the gate
   measures nothing there. wasm is where SolidCraft evaluates most slowly, and so where a cost
   would hurt most.
4. **It mixes two prices.** "The price of D3 plus correct rounding" is one number for two decisions
   with different fixes. If the gap is large, the next step (revisit (b) or (c)) does not follow.
   Dropping correct rounding while keeping D3 (faithful `fmath`) might close it.
5. **No unit.** "10% of corpus time": wall time is noisy (35% between runs per the `fmath` docs),
   while the project measures instruction counts from side-channel instrumentation.

**Fix:** restate the gate as at least three builds of the same corpus:
- as built;
- D3 kept with a faithful or `libm`-crate `fmath`;
- platform-free: std libm, plus `algebraic_*` or explicit `mul_add` in the named hot loops,
  measured on `x86-64-v3` and aarch64.

Report instruction counts, and time on one pinned core. Run it at step 4 for the transcendental
question, then again at step 5 or 7, once B-spline SSI and fitting exist, for the FMA question.
State per target which result triggers what.

### 3. Toolchain and compiler versions are not considered (missing)

**Where:** §2's D0-D4 table, §5 (e) item 1.

**Problem:** D3 as defined is "the same source built for different targets". It is silent about the
compiler. Both of 67's precedents make the compiler version part of the condition (VERIFIED):
- Rapier's local determinism holds "using the same version of Rapier, and the same version of the
  Rust compiler".
- Box2D warns that "Compiler versions can break it."

For solidcorex this matters in three ways:
- **Downstream compilers.** `rust-toolchain.toml` pins 1.100.0 only for this workspace. A public
  library is compiled by its consumers with their own rustc. The xplat legs prove D3 for one
  toolchain.
- **The app's cache key.** SolidCraft's evaluation-source digest covers the source, the locked
  dependencies and `.cargo/config.toml`. Its documented list does not include the Rust toolchain
  (VERIFIED, `design-container.md`). So the app already assumes that a compiler upgrade does not
  move bits, and nothing checks that.
- **What can move with a release.** Basic IEEE operations will not change. What can:
  - which library supplies `fma` and how it dispatches (finding 5);
  - how `f64::min`/`max` are lowered, and therefore which zero they return (finding 14);
  - NaN bits, which RFC 3514 allows to differ between const evaluation and run time.

There is good evidence already that 67 does not cite. 05 records the golden hashes unchanged from
Rust 1.99.1 to 1.100 on all three targets. That is the right kind of check.

**Fix:**
- Add a D-row: same source, different compiler release.
- State the contract as "for a given toolchain, verified on each toolchain bump", or add a CI leg
  that runs `fmath_pin` and `xplat_pin` on current stable and beta besides the pin.
- Tell the app that its cache key leaves out the toolchain.

### 4. Fast-math *is* available on stable Rust, and the bans have holes (wrong)

**Where:** §4.2 row 4 ("fast-math is not available in stable Rust").

**Problem:** VERIFIED: `f64::algebraic_add` compiles and runs on stable 1.99.1 and 1.100.0 with no
feature gate. The `algebraic_*` methods let LLVM reassociate and contract, which is exactly
platform-dependent fast-math, per call site. `clippy.toml` bans none of them.

It also leaves these unbanned, all of which go to the platform libm:
- `f64::exp_m1`, `ln_1p`, `asinh`, `acosh`, `atanh`;
- every `f32` transcendental.

No library code uses any of them today (VERIFIED, grep), so the hole is open but no code falls
through it yet.

**Fix:**
- Correct the sentence.
- Add the `algebraic_*` methods (f32 and f64), the unbanned f64 libm methods and the f32 libm
  methods to `disallowed-methods`. Or note them as an open enforcement gap in 01 §8.3.

### 5. The kernel still links a platform `fma`, and the shipped targets are not the tested ones (wrong and missing)

**Where:** §2 ("unless the libm stays our own code"), the D2 row, §5 (e) item 1.

**Problem:**
- **The `fma` is not our code.** On baseline x86-64, `two_prod` is `mul_add`, which becomes a call
  to a library `fma`. VERIFIED with `nm` on a stable 1.99.1 linux-gnu binary: the symbol comes from
  `compiler_builtins` (`libm_math::arch::x86::fma`). It dispatches at run time between `fma_with_fma`,
  `fma_with_fma4` and `fma_fallback`. That is the real D2 dispatch, not `two_prod`'s compile-time
  `cfg`. Correct rounding makes all three agree, but D3 now rests on a third-party implementation.
- **Which library supplies it depends on the target.** On `*-pc-windows-gnu`, MinGW's `fma` is not
  fused, and `mul_add` gives wrong results (PUBLISHED: rust-lang/rust#140515; mingw-w64 bug 848).
  The `fmath` module docs say the call goes to "the C library's `fma`", while 05 says
  compiler_builtins. Even the internal sources disagree.
- **The CI legs are not the shipped targets.** They run `wasm32-wasip1` under wasmtime and
  `aarch64-unknown-linux-gnu` (VERIFIED, `ci.yml`). SolidCraft ships:
  - `wasm32-unknown-unknown` in V8, SpiderMonkey and JavaScriptCore;
  - Windows (MSVC or GNU);
  - macOS on aarch64 and x86-64.

  None of these is tested. Where wasm NaN bits are non-deterministic, they can differ by engine
  (RFC 3514: "even the underlying target behavior is non-deterministic, namely on wasm").

**Fix:**
- State in the contract which targets are verified: the triples, not the ISAs.
- Add at least a Node (V8) run of the hash tests on `wasm32-unknown-unknown`, and a Windows leg.
- Consider a portable `fma` inside `eft`, so that D3 no longer rests on whichever library resolves
  `fma` for the target. The software path already exists for wasm, so the cost is small.
- Fix the `fmath` doc comment.

### 6. The floating-point environment can be changed by other code in the app's process (missing)

**Where:** §5 (e) item 1, which names RFC 3514's target exceptions but not its environment
assumption.

**Problem:** RFC 3514's guarantees assume the default environment: no flush-to-zero, and
round-to-nearest. Rust "does not support" running with changed control bits (VERIFIED, RFC).

The desktop app is a process that also loads GPU drivers, windowing code and other native libraries.
A shared object built with GCC `-ffast-math` links `crtfastmath.o`, which sets FTZ and DAZ in MXCSR
when it loads (JUDGED from GCC's documented behaviour; not tested here). Browsers do not flush in
wasm, so the desktop would then diverge from the web on every subnormal. Subnormals do occur in
kernel hot paths: 05 found `solve1d` running on stale, often subnormal, values. D3 as written can
silently fail in the very configuration it exists for.

**Fix:**
- Add the default FP environment to the contract.
- Recommend a debug assertion of MXCSR and FPCR at the kernel's entry points, or one check at app
  start-up.
- Note it among what the app must guarantee.

### 7. Certified decisions do not make D3 unnecessary (missing; it strengthens the recommendation)

**Where:** §2.9 and §6 reply 2 ("Until the last uncertified threshold is gone, D3 is what keeps a
file consistent").

**Problem:** reply 2 concedes that certification would eventually remove the need for D3. That is
true only for *exact* predicates, like CGAL's filtered ones, which fall back to exact arithmetic
and so always give the same answer.

The certified section of `scx-num`, and Krawczyk tests in general, answer `Yes`, `No` or
`Undecided`. Where the band edges fall depends on the rounding of the enclosure. With different
rounding, one platform decides and the other returns `Undecided`, and the operation then takes a
different path. So certified-but-inexact decisions still need D3 for agreement between platforms.

**Fix:**
- In 2.9 and reply 2, separate exact predicates (no D3 needed) from certified three-way tests (D3
  still needed for agreement).
- This makes adversarial point 2 weaker than 67 allows, and the case for D3 stronger.

### 8. The evidence for one-ulp decision flips is overstated (overstated)

**Where:** §2.7 and §2.9.

**Problem:** of the three items, only #1252 is a cross-platform decision flip. VERIFIED, `gh`: the
tangent cylinders are unsound with GCC contraction on Linux aarch64, and pass on x86-64 and on both
macOS targets.

- **Research 39** shows face *order* moving *mass* "by the same last-bit amounts". That is D1 and a
  metric change, not a topology change.
- **Research 33's** cos 0.9957 is 4e-3 away from the 0.99985 limit. That is not near a threshold.
  Those reports came from kinked trims written by lazy writes. Its 3.6e-14 ray differences are
  always "the same face, class and region". Nothing flipped.

Saying this "shows how small the trigger can be" is wrong.

**Fix:**
- Keep #1252.
- Replace 33 and 39 with cross-platform evidence. The web searches found OCCT reports of topology
  that differs by platform: #30012 (10 intersection curves on Windows, 9 on Linux), #31849 and
  #29751 (UNVERIFIED beyond the search snippets, because the tracker did not resolve).
- Or label 33 and 39 as D1 evidence only.

### 9. The contract is not stated precisely, and removing it later is not cheap (overstated and missing)

**Where:** §5 (e) item 1, §6 reply 5 ("removing D3 later is cheap: delete a rule"), §7 question 1.

**Problem:**
- **Removal is cheap only inside the repository.** solidcorex is a public library. Once D3 is a
  documented "product property", consumers may build on it: shared caches, server-side renders
  that must match the client. Removing it is then a breaking change for them. The asymmetry in
  reply 5 holds only while no consumer relies on D3.
- **The contract cannot be tested as written.** "x86-64, aarch64, wasm32, NaN bits excepted" leaves
  open:
  - which target triples count;
  - which toolchains (finding 3);
  - which FP environment (finding 6);
  - the sign of a zero from `min`/`max` (finding 14);
  - versions of the kernel, which D4 does not promise.

"Product contract, not correctness" is coherent as a framing. As a ruling it needs that scope, or it
cannot be checked, and it cannot be retired cleanly later either.

**Fix:**
- Draft the contract in one paragraph: the verified triples, "per toolchain, rechecked on each
  bump", the default FP environment, NaN payloads and zero signs excepted unless stated, and no
  promise across kernel versions.
- Say whether it is a public promise of the library or an internal one owed to SolidCraft. Rewrite
  reply 5 to match.

### 10. Kernel upgrades fork histories more than platforms do, and the app has another route (missing)

**Where:** §2.3, §2.4, §2.5, and the owner's question about what the app actually needs.

**Problem:** the app's cache is keyed by the evaluation-source digest. Any kernel or dependency
change evaluates every design again (VERIFIED, `design-container.md`), and D4 lets results move
under truth over pin.

So the 2.4 story is not prevented by D3: a reference that finds nothing, then a re-pick, then a
forked history. It happens on every kernel release that moves a near-threshold result, on every
platform at once. D3 removes the *platform* source of forks, not the *version* source, and the
version source is the more frequent one. A shared desktop and web cache (2.3, 2.5) also needs both
clients on the same commit. A web app that auto-updates and a desktop app that lags will rarely be.

There is also an app-side route 67 does not list. Trust a stored per-feature body cache across
platforms (drop the platform from the key), and re-evaluate only the features downstream of an edit.
Opening and viewing then agree everywhere without D3. Divergence is limited to features re-evaluated
near a threshold, which is the same exposure the app already accepts across kernel versions.

**Fix:**
- Add this as an option, or at least as a weight against 2.3 and 2.5.
- In 2.4, say that D3 removes one source of forks and that kernel releases are the other.
- Ask the owner, in §7, whether the need is desktop and web agreement on *re-evaluation*, or only
  files that reproduce and open the same everywhere.

### 11. Option (b)'s loss of accuracy is argued from the wrong library (overstated)

**Where:** §5 (b), "the `libm` crate reaches 496 ulp on `powf`".

**Problem:** (b) means the *platform* libm. That is glibc, Apple's libm or the MSVC CRT, not the
`libm` crate. glibc's double-precision functions are generally within about 1 ulp (JUDGED from
glibc's published ulp tables; not re-measured here). The real losses under (b) are:
- interval code loses its *proved* 0.5 ulp bound, so it must widen by a stated ulp bound for each
  libm;
- results move with the libm version (D2 and the user's glibc).

**Fix:** use the platform libm's documented error bounds, or say "unmeasured", in place of the
496 ulp figure.

### 12. The uniqueness of correct rounding is asserted, not proved, and maintaining `fmath` is not costed (overstated and missing)

**Where:** §5 (e) item 2, §7 question 2.

**Problem:** the case for keeping correct rounding is that a faster rewrite "cannot move a single
result". That holds only if both the old and the new implementation are correctly rounded on
*every* input. `fmath`'s evidence is 5,000 inputs per function within 0.501 ulp (VERIFIED, module
docs: "on every input tried"). That is not a proof. A rare misrounding in either version moves a
result and fails a hash.

67 also does not cost `fmath` as a maintained component:
- 24 functions, each in double-double with its own reduction and rounding test;
- every new function a later layer needs (`exp_m1`, `ln_1p`, the inverse hyperbolics, perhaps
  `erf` for fitting) must be correctly rounded too;
- the Ziv fast path needs a proven error bound for each function.

**Fix:**
- State that uniqueness depends on worst-case coverage.
- Add the published hardest-case inputs (Lefèvre and Muller's tables, or CORE-MATH's worst-case
  lists, after checking that their licences meet the corpus bar) to `tests/fmath.rs`.
- Count the maintenance in §4 as a cost of correct rounding.

### 13. The "per-site `eft::fma`" trade makes wasm about seven times slower in those loops (overstated)

**Where:** §5 (e) item 4, bullet 3; §6 reply 4 ("at a cost only on wasm").

**Problem:**
- **On wasm.** 05 measured software `fma` on wasm32 at 14.7 ns, against about 2 ns for the
  equivalent plain arithmetic. In a Horner or de Boor loop that is roughly a sevenfold slowdown on
  the platform 67 itself calls the slowest, and where SolidCraft evaluates every desktop-saved
  design again.
- **On baseline x86-64.** It is a call through the runtime-dispatched `compiler_builtins` `fma`
  (finding 5), not an inline instruction. The 12% of `cubic` measured in 05 is that call.

"Decided per site, on accuracy and wasm speed" understates this.

**Fix:** state the cost plainly: about 7x per multiply-add on wasm, and a call on baseline x86-64.
Make the rule "only where accuracy needs the fused rounding", not where native speed would gain.

### 14. D3 is not fully held or tested today: zero signs from `min` and `max` (missing)

**Where:** §1 item 2 (it lists reading the sign of a zero from `min`/`max` as forbidden), §4.2's
last row.

**Problem:**
- **The targets differ today.** 05 records that x86 `f64::min` gives -0 on `[0, -0]` because of
  operand order. aarch64 `fminnm` and wasm `f64.min` give -0 whatever the order (JUDGED from the
  ISA and wasm specifications).
- **The code relies on a doc rule, not a test.** `Hull1d` and the extent folds keep `f64::min`/`max`.
  The ruling that "no result reads the sign of a zero end" is documented, but no hash or lint
  enforces it. 05 says the variant that was tried "changed results no test or hash covers".
- **The lowering can change.** It is an LLVM detail that can change between releases (finding 3).

So today's D3 depends on a convention no test enforces, and 67 presents it as a solved
"care with zero signs".

**Fix:**
- Add a `[0, -0]` and `[-0, 0]` case to `xplat_pin` for every fold, so the targets are compared on
  it.
- Or canonicalise zero ends in the extent constructors.
- List this as an open D3 gap, not a cost.

### 15. A future GPU path is ruled out, and 67 does not say so (missing)

**Where:** §4 and §5.

**Problem:**
- **WebGPU's WGSL has no `f64`.** GPU arithmetic is not IEEE-exact in general: division and
  transcendental functions carry ulp allowances, and flushing subnormals is permitted.
- **67's own argument closes the door.** Under (c) it argues that tessellation and evaluation feed
  decisions and stored values. Together with D3, that forbids any kernel-side GPU evaluation,
  tessellation or SSI seeding, permanently, not just "the app's GPU display path".
- **Wide SIMD fares better.** 67's allowance of IEEE lanes covers AVX2 and AVX-512 as long as each
  lane keeps the scalar order. A future runtime dispatch between lane widths is also fine under
  that rule. Stating this would help.

**Fix:** list "no GPU compute in result paths, ever, under D3" as a cost in §4.2. Note that the 65
acceleration work, and any later GPU research, has to live outside result paths.

### 16. Option (c) is dismissed on an unmeasured share, and (e) is already partly tiered (overstated)

**Where:** §5 (c).

**Problem:** the case against (c) is sound: tessellation seeds the solvers and is persisted, and the
app compares edge polylines bit for bit (VERIFIED, SolidCraft `perf.md`). But "a small share of
kernel time" is INFERRED with no basis. And (e) is itself a tiering by operation:
- per-site `eft::fma`;
- fixed-accumulator reductions;
- certified predicates as the "sound core".

**Fix:**
- Label the share unmeasured, and fold the question into the corpus gate (finding 2).
- Describe (e) honestly as "D3 everywhere, with chosen exact relaxations", so that (c) is rejected
  for what it is: a *free* tier with a leak audit.

### 17. "No kernel promises bit identity" is an absence of evidence (minor)

**Where:** §1 item 6, §3's reading ("The CAD kernels do not offer bit identity").

**Problem:** the searches here also found no public statement from Parasolid, ACIS, CGM or C3D.
Absence from public pages is not absence of an internal or contractual guarantee. A secondary blog
(Novedge) claims Parasolid emphasised determinism across platforms (UNVERIFIED). There are nearer
precedents in the same language:
- `ifc-lite-geometry`, a Rust mesh kernel, documents exact predicates whose "trip point is
  IDENTICAL on native x86_64 / aarch64 and on wasm32" (search snippet, UNVERIFIED);
- Blender's exact Boolean has a report of results that differ by platform (T90803, search result
  only).

**Fix:**
- Write "no public statement found" in §3's reading as well as in §1.
- Cite the Rust mesh-kernel precedent as weak support for (e).

### 18. The OCCT tracker items could not be re-checked (minor)

**Where:** §3's OCCT row, the sources list.

**Problem:** the OCCT tracker URL in 67 did not resolve on 2026-11-27, and the mantis archive gave
404 for #27710 and #25531. The quoted sentence ("often different on LINUX and WINDOWS platforms")
was not found by search either. UNVERIFIED, not disproved. Better-indexed items exist: #30012,
#31849, #29751 (finding 8).

**Fix:** record the date the quotes were read, and add one of the better-indexed items as a second
source.

### 19. Some of 67's readings of RFC 3514 go further than the text (minor)

**Where:** §1 item 2, §3's Rust row.

**Problem:** VERIFIED against the RFC.
- It lists `+ - * / % abs copysign mul_add sqrt`, casts and comparisons. `%` matches C's `fmod`,
  not an IEEE operation.
- It does not state a contraction rule. That rustc does not contract follows from the exact
  semantics, and the RFC mentions it only among rejected alternatives.
- On subnormals it says only that the baseline excludes flush-to-zero.
- It adds two points that 67 leaves out:
  - wasm NaN behaviour is non-deterministic in the target itself;
  - const evaluation and run time may choose different NaNs.

**Fix:** quote the operation list exactly, mark "never fuses" as following from the semantics, and
add the wasm and const-eval NaN notes to the contract (finding 9).

### 20. The `powi` bug is misdescribed (minor)

**Where:** §2.6.

**Problem:** 05 says `powi` multiplied by `pow2(-1023) = 0`. The 0 times infinity gave NaN, and
`round_scaled` returned an *infinity* carrying that NaN's sign: negative on x86, positive on
aarch64. The function did not "return NaN". The point stands: a per-platform golden would have
blessed a wrong infinity.

**Fix:** use "returned an infinity whose sign came from an intermediate NaN".

### 21. Box2D's version and its trig (minor)

**Where:** §3's Box2D row.

**Problem:** the post (2024-08) says "version 3", not v3.1. Its `sinf`, `cosf` and `atan2f` are the
author's own *approximations*. Its no-cost claim is for a float32 physics engine with a few
transcendental calls a step, so it says little about a double-precision kernel.

**Fix:** use "v3", and add that its functions are approximate and portable, not correctly rounded,
which supports finding 1.

### 22. The `x86-64-v3` gain was measured with `native` (minor)

**Where:** §5 (e) item 3.

**Problem:** the 30-40% is from `-C target-cpu=native` on the measuring machine (VERIFIED, `fmath`
docs). `native` is not `x86-64-v3`. "Same bits" was shown on the `fmath` hashes only.

**Fix:** label the v3 figure INFERRED from `native`. Add an `x86-64-v3` build to the hash legs before
recommending it to the app.

### 23. The conflict with 07 rule 5 is wider than "horizontal sums" (minor)

**Where:** §4.2 row 4, §5 (e) item 4, §7 question 4.

**Problem:** 07 rule 5 first requires every variable-length reduction to run "in id order, left to
right". A fixed-accumulator reduction breaks that sentence, not only the clause about horizontal
sums. Adopting it also moves every affected result once, on every target, which needs checking
against the truth.

**Fix:**
- Quote the whole rule.
- Note that the change moves results once, and is truth-checked under 04 §1.

### 24. The app already keys its cache by OS as well as by architecture (minor)

**Where:** §2.3 ("So the app today assumes D2, not D3").

**Problem:** `design-container.md` records the build's "CPU architecture and OS", and matches on
"the platform". Windows x86-64 and Linux x86-64 are therefore different keys. The app does not even
rely on agreement across operating systems on one ISA.

**Fix:** use "the app assumes at most D2 within one OS". This slightly lowers the present-day weight
in 2.3.

## Does the recommendation survive?

**Yes, in direction; no, as a ruling ready to sign.**

Keeping D3 holds up under this review. Finding 7 strengthens the case: certified three-way tests
still need D3 for agreement. The cost really is mostly correct rounding and the baseline x86-64
target.

Four changes are needed before the owner rules:
1. **State the contract precisely** (findings 3, 5, 6, 9, 14): verified target triples, per
   toolchain, the default FP environment, NaN and zero-sign exceptions, and public or internal.
2. **Fix the factual errors** (findings 1, 4, 5): Rapier's libm, `algebraic_*` on stable, the
   linked `fma`. Close the clippy holes.
3. **Rewrite the gate** (finding 2): three builds, named targets, instruction counts, and a second
   run once B-spline SSI and fitting exist.
4. **Weigh D3's benefit against the app's version churn** (finding 10). Ask the owner whether the
   app needs re-evaluation to agree, or only files that reproduce.

"Product contract, not correctness" is the right framing. It only means something once the contract
has edges.
