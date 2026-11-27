# 67 - Cross-platform bit identity: what it is worth to a CAD kernel

**Status: proposed for review, revised after `reviews/review-67.md`.** Nothing in this report is
decided. It changes no rule. The one code change it led to, closing holes in `clippy.toml`, is a
separate commit (section 8, finding 4). Section 8 answers each of the review's 24 findings.

The owner's question (2026-11-27): "Can you do an analysis of how important it really is to a CAD
kernel that it has bit-precision across multiple platforms."

What the owner has said so far is a goal, not a requirement (04 §1, 11-26): "strive to provide
consistent results across all platforms and across builds of the same objects", and "All
platforms, but we can keep this in mind, and only test it at the end stages". The docs then turned
the goal into rules (01 §8.1, §8.3; 07 §7 rules 1, 4, 5, 9). In practice the rules mean four
things:

- no fast-math;
- no platform intrinsics in result paths;
- its own correctly rounded `fmath` in place of the platform libm;
- the golden hashes `fmath_pin` and `xplat_pin`, checked on the `xplat` CI legs (05, the
  golden-hash row).

The goal is "the fastest *correct* kernel" (04 §1, 11-27). This report asks two things. Is
cross-platform bit identity part of "correct"? What does it buy, and what does it cost? The first
draft also lacked a precise statement of what is being promised. Section 5.1 now gives one.

## Labels

- **READ**: read from solidcorex's code or docs, from the pin, from the SolidCraft app's docs, or
  from the source of a project with a permissive licence (Rapier, Apache-2.0). No LGPL or
  proprietary source was read: not OCCT, not glibc.
- **REVIEWED**: checked in `reviews/review-67.md` (commit 73e6855) by a file, a command or a fetched
  page, and not checked again here.
- **PUBLISHED**: stated in another project's documentation, papers, issue tracker or blog. These
  are claims by those projects, not checked here.
- **MEASURED**: figures already in 05, in `fmath`'s module docs, or in the research those cite.
  For this revision two checks were run: `nm` on a stable 1.100.0 `x86_64-unknown-linux-gnu`
  binary that calls `f64::mul_add`, and a throwaway clippy probe of the new bans. Both are marked
  MEASURED here.
- **INFERRED**: reasoning or estimates made here. Every speed figure not labelled MEASURED is
  INFERRED.
- **UNVERIFIED**: could not be checked: a search snippet, or a page that did not resolve.

## 1. Summary

1. **Bit identity is not part of "correct".** Correct means within a certified bound of the truth
   (04 §1, 11-27; 01 §9.2). Two platforms can both be correct and still differ in the last bit.
   Bit identity is a separate property: reproducibility. Its value to this kernel comes from how
   SolidCraft uses a kernel. The desktop app, the browser app (wasm) and the CLI each evaluate the
   feature history locally, and none of them is the authority (section 2.3). But the app
   re-evaluates every design on every kernel release anyway, so bit identity removes only one of
   two sources of divergence, and the less frequent one (section 2.10).
2. **Most of the measured cost is not the cost of bit identity.** It comes from two choices that
   sit beside it:
   - **Correct rounding.** `cos` costs about 289 instructions against glibc's 70, `acos` 304
     against 71, and `cbrt` 187 against 110 (MEASURED, 05). That is the price of correct
     rounding. Correct rounding also costs upkeep: every function a later layer needs must be
     written and proved to round correctly (section 4.3).
   - **The baseline x86-64 target, which has no FMA instruction.** The `fma` call costs about 12%
     of `cubic`'s instructions (MEASURED, 05). With `-C target-cpu=native`, `fmath` is 30 to 40%
     faster and gives the same `fmath` hashes (MEASURED, `fmath` docs). That is `native` on one
     machine, not `x86-64-v3`.

   The first draft claimed that a *faithful* portable libm gives identical bits "at close to libm
   speed", and that Rapier does this. That was wrong. Rapier's cross-platform mode routes its maths
   through the `libm` crate, which is portable but not faithful: 80 to 98% correctly rounded per
   function, 496 ulp at worst on `powf` (MEASURED, 04 §3, 11-27). Its speed against glibc has not
   been measured by anyone cited here. A faithful portable `fmath` does not exist off the shelf; it
   would have to be built and proved (section 5, (e) item 2).
3. **What bit identity itself forbids.** Rust gives `+ - * / sqrt mul_add`, `abs`, `copysign`,
   casts and comparisons IEEE 754 semantics on these targets, apart from NaN bits (PUBLISHED, RFC
   3514; `%` matches C's `fmod`). Because the semantics are exact, rustc does not contract
   `a * b + c` into an FMA on its own; the RFC states the semantics, not a contraction rule.
   Fast-math *is* reachable on stable Rust, through the `f64::algebraic_*` and `f32::algebraic_*`
   methods, which let LLVM reassociate and contract at that call site (REVIEWED, on 1.99.1 and
   1.100.0). Bit identity therefore forbids:
   - the platform libm, in `f64` and in `f32`;
   - the `algebraic_*` methods;
   - picking FMA or other instructions per CPU or per target;
   - approximate instructions such as `rcp` and `rsqrt`;
   - reductions whose shape follows the hardware's lane width;
   - reading the sign of a zero from `min` or `max`, or the bits of a NaN;
   - GPU computation in any result path (section 4.2).
4. **It holds only under conditions the first draft did not state.** It holds for one toolchain
   (a public library is built by its consumers with their own rustc), in the default
   floating-point environment (a native library loaded into the app's process can turn on
   flush-to-zero), and on the target triples actually tested. The CI legs today are
   `aarch64-unknown-linux-gnu` and `wasm32-wasip1` under wasmtime; SolidCraft ships
   `wasm32-unknown-unknown` in browsers, Windows and macOS, none of them tested (REVIEWED,
   `ci.yml`). The kernel also still links a third-party `fma` on baseline x86-64 (MEASURED, `nm`),
   and one platform's `fma` is known to be wrong (PUBLISHED, rust-lang/rust#140515, MinGW).
   Section 5.1 states the contract with these edges.
5. **What it buys, concretely, for SolidCraft:**
   - The same design re-evaluated on the desktop and in the browser gives the same faces, the same
     face names and the same stored references, *at the same kernel version*.
   - A user's file reproduces a failure on the developer's machine, whatever the user's platform.
   - The model cache and content-addressed bodies could be shared across platforms. Today the app
     keys its cache by architecture and OS, and evaluates the design again anywhere else (READ,
     SolidCraft `docs/design-container.md`).
   - There is one set of golden hashes instead of three.
6. **It does not buy robustness, and it can hide fragility.** One rounding difference made a
   tangent-cylinder Boolean unsound on aarch64 Linux with GCC contraction. The fix built every
   platform with `-ffp-contract=off`, which "hides this case but does not fix it" (READ,
   tangency-labs/solidcore#1252). A decision that flips on one ulp is a correctness defect on every
   platform. Bit identity only makes all platforms take the same wrong branch.
7. **Recommendation (option e, section 5): keep cross-platform bit identity as an internal goal
   owed to SolidCraft, stated precisely, and stop paying for things it does not require.**
   - Land the Ziv fast path, so correctly rounded functions run nearer libm speed.
   - Let the app build its desktop x86-64 binaries for `x86-64-v3`, once a v3 build passes the
     golden hashes.
   - Allow IEEE-exact SIMD, and reductions with a fixed number of accumulators (a change to 07
     rule 5 that moves results once).
   - Add a fragility probe: build with a perturbed arithmetic and look for topology that flips.
   - Close the enforcement and test gaps: the clippy bans (done, section 8), zero signs from
     `min`/`max`, the shipped targets, the toolchain, the FP environment.
   - Re-decide on a three-build measurement in instruction counts, run at step 4 and again once
     NURBS intersection and fitting exist (section 5, (e) item 6).
   - Do not make it a public promise of the library before step 12 (01 §10.2).
8. **Strongest argument against it (section 6):** no commercial kernel is publicly known to promise
   bit identity; the cost has been measured only on layers 0 and 1; the property breeds false
   confidence; and kernel releases already move results for every user at once. These survive as
   the gate, the fragility probe, the "internal, not public" scope, and a new question to the owner
   on what the app actually needs.

## 2. Who depends on it, and how

Six strengths of determinism need to be kept apart:

| | Property | Who guarantees it today |
|---|---|---|
| D0 | The same process, run twice, gives the same bits | the difftest runs every case twice (05; 02 §6) |
| D1 | The same bits for any thread count, cache state, call order, or save and load | 01 §8.1, 8.3, tested |
| D2 | The same binary on different machines of one target (one with AVX2 and FMA, one without) | holds if every runtime dispatch is between correctly rounded paths. The one known dispatch is the library `fma` on baseline x86-64: `compiler_builtins`' `fma` picks `fma_with_fma`, `fma_with_fma4` or `fma_fallback` at run time (MEASURED, `nm`, 1.100.0). `fma` is fully specified by IEEE 754, so the three agree if each is correct. `two_prod`'s own choice is at compile time (READ, `eft.rs`) |
| D3 | The same source and toolchain, built for different targets | `fmath`, no contraction, the golden hashes (05); on the tested triples only (5.1) |
| D3t | The same source and target, built with a different rustc release | not promised. Evidence: the golden hashes were unchanged across the move from Rust 1.99.1 to 1.100.0 on all three tested targets (READ, 05, the Rust 1.100 row) |
| D4 | Different versions of the kernel | not promised; truth over pin allows results to move (04 §1, 11-27) |

D3 is the subject here. D0 and D1 are needed whatever is decided about D3 (owner, 11-26: "the same
operation executed twice always yields the same answer").

D3t matters because solidcorex is a public library. `rust-toolchain.toml` pins 1.100.0 for this
workspace only; a consumer builds it with their own rustc. Both precedents make the compiler part
of the condition: Rapier's local determinism holds "using the same version of Rapier, and the same
version of the Rust compiler", and Box2D warns that "Compiler versions can break it" (REVIEWED).
What a release can move, even though `+ - * /` will not change (INFERRED):

- which library supplies `fma` on a target, and how it dispatches;
- how `f64::min`/`max` are lowered, and so which zero they return (section 2.11);
- NaN bits, which RFC 3514 allows to differ between const evaluation and run time.

The app's cache key does not include the toolchain: its evaluation-source digest covers the source,
the locked dependencies and `.cargo/config.toml` (REVIEWED, `design-container.md`). So the app
already assumes D3t within one target, and nothing checks it. That is worth telling the app (section
7, question 8).

D2 deserves a warning. It is not free once the platform libm is allowed, as option (b) would allow:

- glibc picks FMA variants of some functions at load time, by CPU. A binary run on two x86-64
  machines can then give different bits (INFERRED from glibc's multiarch layout; not read, since
  glibc is LGPL).
- Rust links glibc dynamically on `*-linux-gnu`, so the result also depends on the user's glibc
  version.
- `pxfm` chooses FMA or non-FMA arithmetic by target and CPU (MEASURED, 04 §3, 11-27).
- A game developer reports that replays which stay in step on one machine type fall out of step
  between AMD and Intel machines (PUBLISHED, gamedev.net thread; anecdotal).

So "same-platform determinism only" does not keep D2 unless the libm stays our own code.

Every one of D2, D3 and D3t also assumes the default floating-point environment: round to nearest,
no flush-to-zero (FTZ) and no denormals-are-zero (DAZ). Rust "does not support" running with other
control bits (REVIEWED, RFC 3514). The desktop app is a process that loads GPU drivers, windowing
code and other native libraries. A shared object built with GCC's `-ffast-math` links
`crtfastmath.o`, which sets FTZ and DAZ in MXCSR when it loads (INFERRED from GCC's documented
behaviour; not tested). Browsers do not flush in wasm. The desktop would then diverge from the web,
and from another desktop, on every subnormal intermediate. Subnormals do reach kernel hot paths: 05
found `solve1d` running on stale, often subnormal, values (READ, 05, the benchmark-gaps row).

### 2.1 Same-machine determinism (D0, D1)

This is required by the owner's words and by the design. Memos and cells must not change results
(07 rule 8). Budgets are summed in index order (04 §3, 11-27), and the warnings a cache replays
are part of the result (04 §3, 11-27). Without D1 a warm cache would answer differently from a
cold one, and parallel runs would answer differently from serial ones. None of this depends on D3.
**Weight: essential, and independent of this question.**

### 2.2 The same build on different machines (D2)

The user's laptop, a CI runner, and a support engineer's machine may run the same target on
different CPUs. A bug report replayed there must reproduce. With our own `fmath`, a correct library
`fma`, and the default FP environment, D2 holds today. **Weight: high. Option (b) puts it at risk
(above), and so does a native library that sets FTZ.**

### 2.3 Different platforms: desktop, web and server

These facts are from SolidCraft's docs (READ):

- **The design file is the timeline.** It carries `document.json` plus an optional model cache
  (`design-container.md`, "Model cache"). A design is evaluated locally on the desktop, "as design
  reading in the CLI and browser does". The browser build is wasm, and "everything below the apps
  must build for `wasm32-unknown-unknown`" (READ, quoted in research 03).
- **The model cache is keyed by platform:** "A cache is read back only when the format, the
  producer, the evaluation source, the platform, the kernel and both digests match: the same source
  compiled for the same platform computes the same numbers on the same kernel."
  - The platform recorded is the build's "CPU architecture and OS" (REVIEWED). So Windows x86-64
    and Linux x86-64 are different keys. The app today assumes at most D2 within one OS.
  - A design saved on the desktop and opened in the browser is evaluated again in wasm. That is
    the slowest place to do it. OCCT users report STEP reading is much slower in wasm than native
    (PUBLISHED, OCCT forum).
- **The app's own native-B-rep golden allows "1e-10 relative/absolute platform rounding"** for
  floating coefficients (`native-brep.md`). The app does not assume D3 today either.

What D3 would add for the app (INFERRED; each needs an app change):

- The platform could be dropped from the cache key, so a browser opens a desktop-saved model
  without evaluating it again.
- A server-side thumbnail, render or export would match what the user's client shows.

Both also need the two sides on the same kernel commit (2.10). And the first has a route that needs
no D3 at all (2.10). **Weight: low to medium today, because the app already copes. It becomes high
only once the app evaluates on one platform for a client on another, at the same version.**

### 2.4 Collaborative editing and replay of a feature history

SolidCraft has no real-time collaborative editing documented (no hits for collaboration in its
`docs/`; READ). It does replay histories:

- Opening a design without a valid cache replays the timeline.
- Undo replays from records (research 38, row (iii)).
- Support replays a failing command against the user's `.sc` with `solidcraft-cli` (the tickets
  service's reproduction procedure).

The failure mode a 1-ulp difference causes is concrete. Take a design authored on desktop x86-64
and opened in a browser, as in 2.3:

1. Feature 7 is a Boolean of two nearly tangent cylinders. On one platform the tangency test falls
   on one side of its threshold and the result has one face more. On the other it does not.
   solidcore#1252 is exactly this case, on aarch64 Linux with GCC contraction.
2. Faces are named from the history (`Changes`, `carry`, `scx-fates`; 01 §2.6). The extra face
   gets a name the other platform never made.
3. Feature 9 is a fillet that stores a reference to "the edge between face 7.3 and face 7.5". It
   finds no edge on one platform. The app's documented response to a name that finds nothing is
   that "the features holding them ask for their references to be picked again"
   (`design-container.md`).
4. If two people exchange the file and re-pick, the history now forks by platform.

With D3, both platforms take the same branch, at the same kernel version. The design is then
consistent: possibly consistently fragile, but the same everywhere. D3 does not stop the same story
across kernel versions (2.10).

Onshape avoids the problem by design. Its feature list is regenerated by its modelling servers, and
its "Topology ID is sometimes referred to as a Deterministic ID" (PUBLISHED, Onshape developer
docs; that the servers do the regeneration is INFERRED from its changelog's "modeling server").
There is one evaluator, so cross-platform identity never comes up.

**Weight: medium to high for a product that re-evaluates on more than one platform, as SolidCraft
does; lowered by 2.10.**

### 2.5 Caching keyed by result hashes

`cache/bodies.bin` stores each distinct body once, keyed by the SHA-256 of its encoding (READ,
`design-container.md`). Within one platform that needs D1. Across platforms, content addressing
(a shared or server cache of bodies, or deduplicated uploads) needs D3, and both producers on the
same kernel commit. Without D3 the same design yields different hashes on desktop and web, so it
never hits a shared cache. **Weight: low to medium; it matters only once caches are shared, and
only between clients on one version.**

### 2.6 Regression testing with golden files

- solidcorex has no bit dumps. The difftest compares with the pin within tolerance (01 §9.1).
- The bit-exact checks are the golden hashes: 24 for `fmath`, 6 for `scx-algo`, in 0.1 s. They
  are identical on the three tested targets (MEASURED, 05).
- Without D3 each hash needs a value per target. 07 §7 rejected that because "it doubles the
  oracle". In practice that means three blessings per change, and a mismatch on one target cannot
  be told apart from a bug there.
- The first aarch64 run of the hashes found a real bug. For `|x| >= 2^1023`, `powi` multiplied by
  `pow2(-1023) = 0`; the squaring made `0 * inf = NaN`, and `round_scaled` returned an infinity
  whose sign came from that intermediate NaN: negative on x86, positive on aarch64. The right
  answer was 0, or infinity for positive `n` (MEASURED, 05; 04 §3, 11-27). A per-platform golden
  would have blessed a wrong infinity.

**Weight: high, and cheap to keep.**

### 2.7 Bug reproduction from a user's file

Users run on Windows and macOS on x86-64 or Apple silicon, and in browsers. Developers and CI run
on Linux x86-64 and aarch64. With D3, any user's file reproduces bit for bit on any developer
machine, at the user's kernel version. Without it, a failure that depends on a near-threshold
decision may not reproduce at all. That is exactly the class of failure that is hardest to find by
other means.

There is a partial substitute that needs no D3: run the user's own target. A browser failure can be
replayed by a CLI built for wasm under a JavaScript engine; a Windows or macOS failure needs that
OS. That covers the browser, which is the case SolidCraft hits most (2.3), but not every desktop
(INFERRED).

The first draft cited research 33 and 39 here as evidence of how small the trigger can be. That was
wrong (section 8, finding 8): both are same-platform (D1) effects, and neither shows a topology
flip. **Weight: high.**

### 2.8 Topology naming and persistent ids

Ids are dense per body, and names come from `Changes` (01 §2.2, §2.6). Names survive geometric
change that keeps the topology. They break only when the topology differs, so D3 matters here only
through 2.9. **Weight: indirect.**

### 2.9 Tolerance decisions that flip on one ulp

This is where every other dependency gets its force. Evidence that such flips happen across
platforms:

- solidcore#1252: a tangent-cylinder Boolean is unsound with GCC contraction on Linux aarch64, and
  passes on x86-64 and on both macOS targets (REVIEWED, through `gh`). The opposite case, which
  fails without contraction, was filed alongside (READ).
- OCCT reports of topology that differs by platform: #30012 (10 intersection curves on Windows, 9
  on Linux), #31849 and #29751 (UNVERIFIED: search snippets only; the tracker did not resolve).
- Same-platform evidence of sensitivity, which is D1 and not D3: research 39 (face order moves
  mass by last-bit amounts in 173 of 474 bodies) and research 33 (read order moved ray answers by
  up to 3.6e-14, always on the same face, class and region). Neither is a topology flip.

The remedies are different, and the first draft merged two of them:

- **Same bits everywhere** gives the same branch everywhere. It does not give the right branch.
- **Exact predicates** give the same and the right answer whatever the rounding, because they fall
  back to exact arithmetic. CGAL's kernels "provide exact geometric predicates", while
  constructions may be inexact (PUBLISHED, CGAL Kernel manual). Where a decision is an exact
  predicate, D3 is not needed for agreement.
- **Certified three-way tests** (the certified section of `scx-num`, Krawczyk uniqueness tests; 65
  §2.1) answer `Yes`, `No` or `Undecided`, and are never wrong. But where the band edges fall
  depends on the rounding of the enclosure. With different rounding, one platform decides and the
  other returns `Undecided`, and the operation then takes a different path (INFERRED). So
  certified-but-inexact decisions still need D3 for agreement between platforms.

The kernel needs certified decisions anyway: 01 §9.2 makes truth the oracle. Most of them will be
three-way tests, not exact predicates. **Weight: high. Certification fixes correctness, and D3 is
still what makes certified-but-inexact decisions agree.**

### 2.10 Kernel versions fork histories more often than platforms do

Added after review (finding 10). The app's cache is keyed by the evaluation-source digest, so any
kernel or dependency change evaluates every design again (REVIEWED, `design-container.md`), and D4
lets results move under truth over pin.

- The 2.4 story (a reference that finds nothing, a re-pick, a forked history) is not prevented by
  D3. It happens on every kernel release that moves a near-threshold result, on every platform at
  once. D3 removes the platform source of forks, not the version source, and the version source is
  the more frequent one (INFERRED: the kernel releases often; the platform mix is fixed).
- Sharing a cache between desktop and web (2.3, 2.5) needs both clients on the same commit. A web
  app that updates itself and a desktop app that lags will often not be (INFERRED).
- **An app-side route that needs no D3:** trust a stored per-feature body cache across platforms
  (drop the platform from the key for reading), and re-evaluate only the features downstream of an
  edit. Opening and viewing then agree everywhere. Divergence is limited to features re-evaluated
  near a threshold, which is the exposure the app already accepts across kernel versions. This is
  option (f) in section 5.

What the app needs is therefore a question for the owner, not something this report can settle
(section 7, question 1): agreement of *re-evaluation* across platforms, or only that a file opens
the same everywhere and that a failure reproduces.

### 2.11 What D3 does not yet cover today

Added after review (findings 5, 14). These are gaps in what already exists, not costs:

- **Zero signs from `min` and `max`.** 05 records that x86 `f64::min` gives -0 on `[0, -0]`
  depending on operand order; aarch64 `fminnm` and wasm `f64.min` give -0 whatever the order
  (INFERRED from the ISA and wasm specifications). `Hull1d` and the extent folds keep
  `f64::min`/`max`, and the rule that "no result reads the sign of a zero end" is a doc rule that
  no hash or lint enforces. 05 says the variant that was tried "changed results no test or hash
  covers" (READ, 05). The lowering is an LLVM detail that can change between releases (D3t).
- **The tested targets are not the shipped ones.** Section 5.1.
- **The library `fma`.** D2 and D3 rest on each target's `fma` being correct. On
  `*-pc-windows-gnu` it is not: MinGW's `fma` is not fused, and `mul_add` gives wrong results
  (PUBLISHED, rust-lang/rust#140515; mingw-w64 bug 848). Until that is fixed, `windows-gnu` is
  outside the contract.
- **The clippy bans had holes.** `algebraic_*`, `f64::log`, `exp_m1`, `ln_1p`, `asinh`, `acosh`,
  `atanh`, and every `f32` transcendental were allowed. No code used them (REVIEWED; confirmed by
  the clippy run in section 8). They are banned as of the separate commit.

## 3. What other kernels and systems do

| System | What it promises | Label |
|---|---|---|
| Parasolid | No public statement on cross-platform bit identity was found. Siemens claims "zero tolerance for regressions" in its quality testing. Users see volume differences of about 3e-4 mm³ between Onshape and SOLIDWORKS on one part, both on Parasolid, and the cause is unanswered. A secondary blog (Novedge) says Parasolid emphasised determinism across platforms | PUBLISHED (blog, forum); Novedge UNVERIFIED; no first-party claim found |
| ACIS, CGM, C3D | No public statement found | not found |
| OCCT | Its regression tests compare with fixed tolerances (such as 1e-7). The tracker says complex algorithms "often cannot provide this precision, and the results are often different on LINUX and WINDOWS platforms", and warns against widening tolerances to hide real platform bugs (#27710). A `bopcurves` regression appeared only on Linux (#25531). #30012, #31849, #29751: topology that differs by platform | PUBLISHED (tracker), read when the first draft was written (2026-11-27); on re-check the same day the tracker did not resolve, so all four are UNVERIFIED now. Source not read (LGPL) |
| CGAL | Exact predicates with inexact constructions (Epick), so combinatorial decisions do not depend on rounding. Constructed coordinates may differ | PUBLISHED (manual) |
| Manifold | Topology is exact and geometry inexact. "We will choose a definition of manifoldness which relies solely on topology, so as to get consistent results." No claim of bit identity found. It is tested on GCC, LLVM and MSVC, and parallel through TBB | PUBLISHED (wiki, README) |
| `ifc-lite-geometry` (Rust mesh kernel) | Documents exact predicates whose "trip point is IDENTICAL on native x86_64 / aarch64 and on wasm32" | UNVERIFIED (search snippet) |
| Blender exact Boolean | A report of results that differ by platform (T90803) | UNVERIFIED (search result only) |
| Onshape | A single server-side evaluator, so the question does not arise. Topology IDs are "deterministic ids" tied to history | PUBLISHED, partly INFERRED (2.4) |
| Box2D v3 | Cross-platform determinism on x64 and ARM, under MSVC, GCC and Clang, checked in CI on every PR. Fast-math off ("never use this setting"), `-ffp-contract=off`, and the author's own approximate `sinf`, `cosf` and `atan2f`, portable but not correctly rounded, because `atan2f` differed by platform. "No noticeable performance impact", for a float32 engine with a few transcendental calls a step. Warns that "Compiler versions can break it". No rollback determinism | PUBLISHED (box2d.org, 2024-08) |
| Rapier | By default only locally deterministic: same machine, Rapier version and Rust compiler version. `enhanced-determinism` gives cross-platform identity on IEEE 754-2008 targets, including wasm. The feature is `["simba/libm_force", "parry3d/enhanced-determinism"]`; `libm_force` routes the functions through the `libm` crate, a portable port of musl's libm that is not faithful (496 ulp on `powf`, MEASURED in 04 §3). It cannot be combined with `simd8`. The parallel solver matches the serial one | PUBLISHED (rapier.rs); READ and REVIEWED (Cargo.toml, Apache-2.0) |
| Rust itself | RFC 3514 lists `+ - * / % abs copysign mul_add sqrt`, casts and comparisons as having IEEE 754-2008 semantics apart from NaN bits (`%` is C's `fmod`, not an IEEE operation). No contraction rule is stated; that rustc does not fuse follows from the exact semantics, and the RFC mentions contraction only among rejected alternatives. NaN sign and payload are non-deterministic; on wasm "even the underlying target behavior is non-deterministic", and const evaluation may choose a different NaN from run time. The baseline excludes flush-to-zero. Exceptions are i586 without SSE2 (x87 precision) and 32-bit ARM NEON (flush to zero). The `algebraic_*` methods are stable and opt in to reassociation and contraction | PUBLISHED (RFC 3514), REVIEWED; `algebraic_*` REVIEWED on 1.99.1 and 1.100.0 |
| CORE-MATH | Correctly rounded libm functions. The authors claim speed competitive with glibc. glibc now ships 21 correctly rounded single-precision functions from it | PUBLISHED (glibc list, FreeBSD numerics list) |

Reading of the precedent (INFERRED):

- **No CAD kernel is publicly known to offer bit identity, and they live with the consequences.**
  That is an absence of public evidence, not evidence of absence: an internal or contractual
  guarantee would not show up in a search. OCCT's tracker shows the cost of not having it:
  tolerance-widened tests and bugs found on one platform only. Their main users either evaluate in
  one place (Onshape) or treat each platform's result as authoritative for itself (desktop
  SOLIDWORKS, NX).
  - These kernels are C and C++ built on the platform libm and a compiler that may contract.
    Getting D3 there means fighting the toolchain.
  - In Rust, D3 is the language default for the basic operations. The remaining work is the libm,
    which solidcorex already owns, plus the gaps in 2.11.
- **The systems that need replay across machines do provide it, and say it is cheap.** These are
  the lockstep and rollback physics engines. Both get it from *portable* maths, not from faithful
  or correctly rounded maths: Box2D from its own approximations, Rapier from the `libm` crate.
  Neither says anything about a faithful portable libm, and their cost claims are for float32
  physics with few transcendental calls, so they say little about a double-precision kernel.
- **CGAL and Manifold show the robust alternative:** make the decisions exact, and let the
  coordinates differ. That is option (c)'s core, and it is what 2.9 asks for where a decision can
  be made exact.

## 4. Cost

### 4.1 What we have measured

| Figure | Value | Source |
|---|---|---|
| `fmath` per call against the platform libm (baseline x86-64) | 1.07x (`hypot`) to 3.72x (`log2`). `sin` and `cos` 1.72 to 1.74x, `acos` 3.38x, `cbrt` 1.90x, `powf` 3.22x | MEASURED, `fmath/mod.rs` docs; noise up to 35% between runs |
| Instructions per call | `cos` about 289 against glibc's 70; `acos` 304 against 71; `cbrt` 187 against 110 | MEASURED, 05 (callgrind) |
| `cubic` against the pin | 1,021 against 448 instructions, 207 against 90-97 us before polishing. The solver's own arithmetic matches (about 102 against 104); the gap is `fmath` | MEASURED, 05 |
| `cubic` and `quartic` after Newton polishing | 3.68x and 2.64x the pin's time (from 2.30x and 1.69x) | MEASURED, 04 §3 (11-27). This is the price of accuracy (median 0.26 ulp), not of bit identity |
| `fma` on baseline x86-64 | about 12% of `cubic`'s instructions, through `compiler_builtins`' runtime-dispatched `fma` | MEASURED, 05; the symbol MEASURED with `nm` here |
| `fmath` built with `-C target-cpu=native` | 30 to 40% faster (`sin` 21.8 ns, `ln` 11.3 ns, `powi` 35.2 ns), same `fmath` hashes | MEASURED, `fmath` docs, on one machine; `x86-64-v3` not measured |
| `two_prod` on wasm32 | 14.7 ns with software `fma`, 2.1 ns with Dekker's product, same bits | MEASURED, 05 |
| `libm` crate against glibc, speed | not measured | none |
| Ziv fast path (being built) | expected to recover about 30% of `cubic` | 05's estimate; INFERRED |
| The `xplat` CI legs | `fmath_pin` and `xplat_pin` take 0.1 s each run. The legs' wall time was not recorded | MEASURED, 05; CI time unmeasured |
| Corpus-level cost | **not measured.** No operation above layer 2 runs yet | none |

### 4.2 Which hot paths pay, and for what

Each cost is attributed to the rule that actually causes it (INFERRED):

| Cost | Caused by D3? | Caused by correct rounding? | Caused by the baseline target? | Recoverable while keeping D3 |
|---|---|---|---|---|
| `fmath` transcendentals, 1.1 to 3.7x libm | only the ban on the platform libm | **yes, most of it**: double-double evaluation and rounding tests | partly: `two_prod` calls `fma` | the Ziv fast path (about 30% of `cubic`, INFERRED). A faithful portable `fmath` would be faster, but it would have to be built and proved faithful, and its speed is unknown. The `libm` crate is portable and exists, but is not faithful (496 ulp) |
| The library `fma` in `two_prod` | no: the result is exact either way | yes: only the error-free products need it | **yes**: baseline x86-64 has no FMA, so each call goes through `compiler_builtins`' dispatch | build the app for `x86-64-v3` (INFERRED from the `native` figure; check the v3 hashes first) |
| No FMA contraction in evaluation (Horner, de Boor, dot products) | **yes**, if FMA were chosen per target. wasm32 has no FMA instruction (relaxed-simd's `relaxed_madd` is non-deterministic) | no | partly | a deliberate `eft::fma` gives the same bits everywhere, at a real price: on wasm32 a software `fma` (14.7 ns against 2.1 ns for a Dekker product, MEASURED; against a plain unfused `a * b + c` the ratio is larger and unmeasured), and on baseline x86-64 a call through the dispatch. Gain on FMA hardware, INFERRED: up to about 1.5x on latency-bound Horner loops |
| Lost reassociation and auto-vectorised reductions | yes, for reassociation by lane width. Rust never reassociates by default; the stable opt-in (`algebraic_*`) is platform-dependent, so D3 forbids it (now banned) | no | no | reductions with a fixed number of accumulators (say four, whatever the lane width) are identical everywhere and vectorise. This conflicts with the whole of 07 rule 5 (section 5, (e) item 4), and adopting it moves every affected result once |
| SIMD intrinsics | only for approximate instructions (`rcp`, `rsqrt`), per-target FMA, and `min`/`max` zero and NaN semantics | no | no | IEEE lane operations (add, mul, div, sqrt) are exact on SSE2, AVX2, AVX-512, NEON and simd128, as long as each lane keeps the scalar order. A runtime dispatch between lane widths is then fine too. 65 §3.1 already notes SoA blends are bit-identical without contraction |
| GPU computation in result paths | **yes, permanently.** GPU arithmetic is not IEEE-exact in general (ulp allowances on division and transcendentals, flushing permitted), and WebGPU's WGSL has no `f64` at all | no | no | none. Under (c)'s own argument (tessellation and evaluation feed decisions and stored values), D3 rules out kernel-side GPU evaluation, tessellation or SSI seeding. GPU work must stay outside result paths, as the app's display does; the 65 acceleration work and any later GPU research must respect that |
| Golden hashes and xplat legs | yes | no | no | already small; more legs cost CI minutes (5.1) |
| `f64::min`/`max` signed-zero handling | yes: the targets differ on `[0, -0]` (2.11) | no | no | an open gap, not a solved cost: add the zero cases to `xplat_pin`, or canonicalise zero ends in the extent constructors |

**What bit identity alone costs, then:**

- the gap between a portable libm of our choosing and the platform libm. The only portable option
  that exists today without correct rounding is the `libm` crate, which is not faithful and whose
  speed is unmeasured. So this cost is unmeasured;
- no per-target FMA in evaluation loops, and a real price where `eft::fma` is chosen anyway;
- no GPU in result paths;
- care with zero signs and NaN bits, which is not yet tested (2.11).

The large measured figures are correct rounding and the x86-64 baseline. The second has a remedy
that keeps the bits (`x86-64-v3`, once checked). The first is reduced by Ziv; past that, only a new
faithful `fmath` would help, and that is unbuilt.

### 4.3 The upkeep of correct rounding

Added after review (finding 12). `fmath` is a maintained component, and its cost is paid by
correct rounding, not by D3:

- 24 functions, each in double-double with its own reduction and rounding test;
- every new function a later layer needs must be correctly rounded too: `exp_m1`, `ln_1p`, the
  inverse hyperbolics, perhaps `erf` for fitting (all now banned from std, so they must come from
  `fmath`);
- the Ziv fast path needs a proven error bound for each function;
- uniqueness (5, (e) item 2) holds only if *every* input rounds correctly. Today's evidence is
  5,000 inputs per function within 0.501 ulp, "on every input tried" (READ, `fmath` docs). That is
  not a proof. The remedy is to add published hardest-to-round cases (Lefèvre and Muller's tables,
  or CORE-MATH's worst-case lists, after checking their licences meet the corpus bar) to
  `tests/fmath.rs`.

## 5. The options

These criteria apply to every option:

- **Truth over pin** (04 §1, 11-27) is about correctness against references. No option weakens it,
  since each keeps results within certified bounds.
- **The difftest** compares with the pin within tolerance on x86-64 only, plus a two-run bit check
  (D0). No option changes it. The pin itself uses the platform libm, so the pin is not D3, and a
  pin comparison on another platform would need its own tolerance anyway.

### 5.1 The contract, stated

The first draft asked the owner to approve "D3 on x86-64, aarch64 and wasm32, NaN bits excepted".
That cannot be tested or retired cleanly. Proposed wording, for (a) and (e):

> For one solidcorex commit, built with one Rust toolchain release, every public operation returns
> the same bits on each verified target triple, when run in the default floating-point
> environment (round to nearest, no flush-to-zero, no denormals-are-zero). The verified triples
> today are `x86_64-unknown-linux-gnu` (baseline), `aarch64-unknown-linux-gnu` and
> `wasm32-wasip1` under wasmtime. Excepted: the sign and payload of a NaN that comes from
> arithmetic (no result may depend on them; every NaN `fmath` returns is `f64::NAN`); and the sign
> of a zero end from `min`/`max`, until the zero cases are in the golden hashes. Not covered:
> other toolchain releases (checked at each toolchain bump, not promised); other kernel versions
> (D4); targets without SSE2-class IEEE arithmetic (i586, 32-bit ARM NEON); `*-pc-windows-gnu`
> until rust-lang/rust#140515 is fixed; any process whose other code changes the FP environment.
> This is an internal goal owed to SolidCraft, not a semver guarantee of the library.

Notes on the wording (INFERRED):

- **Shipped targets.** SolidCraft ships `wasm32-unknown-unknown` in V8, SpiderMonkey and
  JavaScriptCore, Windows, and macOS on aarch64 and x86-64. To bring them inside the contract,
  add hash legs in this order: `wasm32-unknown-unknown` under Node (V8), `x86_64-pc-windows-msvc`,
  `aarch64-apple-darwin`; and an `x86-64-v3` build of the x86 leg before recommending v3 to the
  app. This brings forward per-platform CI that the owner placed "at the end stages" (04 §1,
  11-26), so it is a question for him (section 7, question 5). The hash tests take 0.1 s, so the
  cost is runner minutes.
- **Toolchain.** The workspace already re-checks the hashes on aarch64 and wasm32 at every
  toolchain bump (04 §3, the Rust row). A further leg on current beta would warn before a release
  moves bits. Cheap; optional.
- **FP environment.** The kernel cannot guard this cheaply in a result path. Two options: a check
  at the app's start-up (and after loading plug-ins) that MXCSR or FPCR has its default value; or a
  small solidcorex helper the app calls. Either way it is a condition the app must keep, and the
  app should be told (section 7, question 8).
- **Public or internal.** The first draft argued that removing D3 later is cheap: "delete a rule".
  That holds only while no consumer relies on it. Once a public library documents D3, consumers
  may build shared caches or server renders on it, and removing it is a breaking change. So the
  proposal is internal until step 12 (01 §10.2), with the decision to publish it as a promise
  taken then, with the shipped-target legs green.
- **The `fma` dependence.** `fma` is fully specified by IEEE 754, so any correct implementation
  gives the same bits, and the hashes test that per target. A portable `fma` inside `eft` would
  remove the dependence, but on x86-64 machines with FMA hardware it would be slower than the
  dispatched instruction, and on wasm it is what runs already. Not proposed; `windows-gnu` is
  excluded instead (section 8, finding 5).

### (a) Keep full cross-platform bit identity, as today

- **Breaks:** nothing.
- **Speeds up:** nothing beyond the Ziv fast path. The measured gaps stay where the Ziv path
  falls short.
- **Testing:** today's two CI legs. Later (step 12), a golden table of record hashes per operation
  on three targets: one oracle.
- **Truth and difftest:** unaffected.
- **Weak point:** it pays for the x86-64 baseline and for not vectorising reductions without
  noticing. Neither is required by D3. And it leaves the 2.11 gaps open unless they are closed
  deliberately.

### (b) Same-platform determinism only; cross-platform results "equally correct within certified bounds"

- **Breaks:**
  - desktop and web agreement on near-threshold topology at one kernel version (2.4);
  - cross-platform reproduction of user files (2.7), except by running the user's own target;
  - sharing caches across platforms (2.5).
  - With the platform libm, D2 as well (section 2): glibc's choice by CPU and its version
    differences.
- **Keeps:** the app's platform-keyed cache, which already works this way.
- **Speeds up:**
  - Transcendentals at libm speed: `cos` 70 instructions against 289 (MEASURED). On `cubic` that
    is most of the 1,021 to 448 gap.
  - Per-target FMA in evaluation (INFERRED 10-30% in evaluation-heavy loops on FMA hardware).
- **Loses accuracy, by an unmeasured amount.** (b) means the *platform* libm: glibc, Apple's libm,
  the MSVC CRT, and in the browser whatever the wasm build links. Their error bounds were not
  measured here. (The first draft quoted the `libm` crate's 496 ulp, which is the wrong library
  for this option.) What is certain: interval code loses `fmath`'s proved 0.5 ulp bound and must
  widen by a stated bound for each libm it may meet, and results move with the libm's version.
- **Testing:** goldens per target, so three blessings, and a cross-target check that compares
  within certified bounds instead of by bits. That needs a bound for every hashed output.
  Anything without a certified bound cannot be checked across targets at all.
- **Truth and difftest:** unaffected in principle. In practice "equally correct within certified
  bounds" is checkable only where a bound exists, and topology has no bound: one face more is not
  "within tolerance".

### (c) Tiered: bit-identical for topology-deciding predicates and persisted data, free for pure evaluation and display

- **Breaks:** the line between the tiers is hard to hold in this kernel.
  - Tessellation is not display-only. The global solver seeds from tessellation-tolerance leaf
    trees (65 §2.1). SSI seeds come from the tessellation (65 §5).
  - Meshes are persisted, and edge samples must be a function of the edge alone so that cached
    face meshes stay watertight (01 §3.2 rule 2, from research 60). The app compares its edge
    polylines bit for bit (SolidCraft `perf.md`; REVIEWED).
  - Evaluation feeds every Newton step whose result is a vertex position that gets persisted.
- **Speeds up:** only paths that provably never feed a decision or a stored value. Today that is
  the app's GPU display path, which is outside the kernel. What share of kernel time the free tier
  would hold is **unmeasured**; the first draft's "a small share" had no basis. The gate measures
  it (item 6 of (e), build C).
- **Testing:** an audit, by type or lint, that no free value reaches a decision. This is new and
  ongoing work, and a leak is invisible until it causes a desync.
- **Truth and difftest:** unaffected.
- **What is rejected is the *free* tier with a leak audit.** (e) is itself a tiering by operation:
  it keeps D3 everywhere but allows chosen relaxations that are exact or the same on every target
  (per-site `eft::fma`, fixed-accumulator reductions, IEEE SIMD lanes), and it takes (c)'s sound
  core, certified and exact predicates (2.9), into every option.

### (d) Bit identity per target family

The families would be "FMA" (aarch64, `x86-64-v3`) and "no FMA" (wasm32, baseline x86-64).

- **Breaks:** desktop and web agreement. That is the pair SolidCraft most needs (2.3), since wasm
  is always in the "no FMA" family.
- **Speeds up:** FMA in evaluation loops on native (INFERRED, as in (b)). The transcendental gain
  from hardware FMA (30-40% with `native`, MEASURED) is also available under (a), because the bits
  are the same.
- **Testing:** two golden sets.
- **Truth and difftest:** unaffected.
- **Verdict:** it gives up the valuable pair to gain only the FMA-in-evaluation part.

### (e) Proposed: keep D3 as an internal goal with a stated contract, and decouple the costs that D3 does not cause

1. **Keep D3** under the contract in 5.1: internal, per toolchain, verified triples only, default FP
   environment, NaN and zero-sign exceptions stated. Close the 2.11 gaps: the clippy bans (done),
   the `[0, -0]` and `[-0, 0]` cases in `xplat_pin` for every fold (or canonical zero ends), and
   the shipped-target legs if the owner approves (section 7, question 5).
2. **Correct rounding stays, through the Ziv fast path.** Its unique value is not accuracy, since
   0.5 against 1 ulp is far inside every tolerance. It is that the answer is unique: `fmath` can be
   rewritten faster later, by Ziv or by CORE-MATH-style kernels, without moving a result or
   re-blessing a hash. That holds only if both versions round correctly on every input, so the
   hardest-case inputs go into `tests/fmath.rs` (4.3).
   - Change if the gate (item 6) shows the cost of correct rounding (build A against build B) too
     large at corpus level after Ziv. The next step would be a **new** faithful portable `fmath`,
     still D3. Nothing off the shelf is that: the `libm` crate is portable but reaches 496 ulp.
     A faithful variant would have to be written and its error bound proved per function, and its
     speed is unknown. It moves results by at most 1 ulp, which truth over pin accepts as equally
     correct within certified bounds. It needs the owner's ruling, because it gives up
     uniqueness.
3. **Recommend `x86-64-v3` builds to the app** for desktop x86-64, with a baseline fallback binary,
   once an `x86-64-v3` build passes the golden hashes in CI. Hardware FMA there should give `fmath`
   gains like the 30-40% measured with `native` (INFERRED from `native`; MEASURED on one machine)
   and drop the `fma` dispatch (12% of `cubic`, MEASURED). No kernel rule changes. This is not a
   library feature (01 §7.4: "none changes a result"; this one does not either).
4. **Allow, by rule, what D3 permits:**
   - IEEE-exact SIMD lanes, of any width, each lane in scalar order;
   - reductions over a fixed number of accumulators, fixed in source and independent of lane
     width. 07 rule 5 reads in full: "Every reduction over a variable-length set runs in id order,
     left to right. A parallel reduction collects per-item values and sums them in that order
     afterwards. There are no cross-lane horizontal sums." A fixed-accumulator reduction breaks
     its first sentence, not just the last. The amendment would keep id order *within* each
     accumulator and fix the combining order. Adopting it moves every affected result once, on
     every target, and each move is checked against the truth (04 §1, 11-27);
   - a deliberate `eft::fma` in a hot evaluation loop only where accuracy needs the fused
     rounding, not where native speed would gain. On wasm32 it is a software `fma` per
     multiply-add (14.7 ns, MEASURED, against a few ns or less for unfused arithmetic), and on
     baseline x86-64 a call through the dispatch.
5. **Add a fragility probe:** a CI job, off the default path and nightly, that builds with one
   perturbation and reports operations whose topology changes. Candidate perturbations:
   - `fmath` swapped for the `libm` crate (a large, portable perturbation);
   - evaluation contracted to FMA at named sites;
   - inputs nudged by one ulp.

   Each topology change is a near-threshold decision. It should become `Undecided` or be
   certified. This is the part of #1252 that bit identity hides (INFERRED; the method is new here).
6. **Gate: three builds of the same corpus, counted in instructions.**
   - **Build A, as built:** correctly rounded `fmath`, D3.
   - **Build B, D3 without correct rounding:** `fmath` replaced by the `libm` crate, local and
     uncommitted. It is portable, so D3 holds, and it stands in for the speed of a faithful
     portable `fmath`, which does not exist (it may be slower or faster than the `libm` crate;
     unknown). **A against B is the price of correct rounding**, with D3 kept.
   - **Build C, platform-free:** std libm, plus explicit `mul_add` or `algebraic_*` in the hot
     evaluation loops that build A's profile names, local and uncommitted. There is no build flag
     that makes rustc contract (rustc emits no `contract` flags; a backend `-C llvm-args` route is
     UNVERIFIED), so this is a source edit at named sites. **B against C is the price of D3
     itself.**
   - **Targets.** Count A, B and C on `x86_64-unknown-linux-gnu` built for `x86-64-v3` and on
     `aarch64-unknown-linux-gnu`, where hardware FMA exists. Count A and B on `wasm32`; there build
     C has no hardware FMA to gain, and the std libm on `wasm32-unknown-unknown` is itself a
     portable software libm (INFERRED, unverified), so C is close to B. Baseline x86-64 is
     measured for A only, to price the `x86-64-v3` recommendation.
   - **Unit.** Instruction counts from side-channel instrumentation (callgrind on native; on wasm,
     wasmtime's fuel count or a pinned-core time, labelled as such), not wall time, which moves by
     up to 35% between runs (`fmath` docs). Totals per operation and over the corpus.
   - **When.** First at step 4 (01 §10.2: mass, classification, primitives, extrude, Booleans on
     analytic bodies, tessellation). That corpus is transcendental-heavy, so it answers the
     correct-rounding question (A against B). Again at step 5, once NURBS work and fitting exist,
     and at step 7 with the full Boolean (B-spline SSI), for the D3 and FMA question (B against C).
     A step-4 result does not settle the FMA question.
   - **What triggers what** (thresholds JUDGED):
     - A exceeds B by more than 10% of corpus instructions after Ziv: put the faithful-`fmath`
       question (item 2) to the owner, with the per-function figures.
     - B exceeds C by more than 10% on `x86-64-v3` or aarch64 at step 5 or 7: revisit (b) or (c)
       for the named hot loops, with the figures in hand.
     - C's gain lies in loops that feed decisions or stored values: (c) stays rejected, whatever
       the figure, and the loops are candidates for item 4's per-site `eft::fma` on accuracy
       grounds only.
     - Neither: (e) stands; record the figures in 05.

For (e) against the other options:

- **Breaks:** nothing.
- **Speeds up:**
  - transcendentals by the Ziv recovery (about 30% of `cubic`, INFERRED);
  - more on `x86-64-v3` desktop builds (INFERRED from 30-40% MEASURED with `native`);
  - reductions and blends by vectorisation, up to 2x on 2-lane targets (INFERRED, 65 §3.1).
- **Testing:** today's legs, the zero-sign hash cases, the hardest-case `fmath` inputs, the
  shipped-target legs if approved, one nightly fragility job (INFERRED minutes), and the
  three-build measurement at steps 4, 5 and 7.
- **Truth and difftest:** unaffected. The fragility probe adds truth checks where it finds flips.

### (f) App-side: trust stored results across platforms, re-evaluate only what changes

Added after review (finding 10). Not a kernel option; it is listed so the owner can weigh it.

- **What:** the app reads a stored per-feature body cache whatever platform wrote it (the platform
  dropped from the key for reading, kept for writing), and re-evaluates only features downstream of
  an edit.
- **Gives without D3:** a file opens and looks the same everywhere; no re-evaluation in the
  browser on open.
- **Does not give:** agreement when two platforms re-evaluate the same feature near a threshold;
  cross-platform reproduction of a failure in re-evaluation.
- **Combines with (e):** yes. With D3 as well, re-evaluated features agree too, at one kernel
  version.

## 6. Adversarial pass

**Against keeping D3 at all.**

1. "No serious CAD kernel promises bit identity, and they power the industry. OCCT openly differs
   between Windows and Linux. Parasolid says nothing. You are holding solidcorex to a bar its
   competitors skipped, at a cost you have measured only on root finders."
2. "The real fix for 2.4 and 2.9 is certified decisions. Once decisions are certified, D3 buys
   nothing that matters. Coordinates that differ by an ulp are invisible to users, tests and
   names."
3. "Worse, D3 hides the fragility. #1252 was 'fixed' by making every platform take the same wrong
   branch. A kernel with D3 can ship near-threshold bugs that only a second platform would have
   exposed. You are removing your best detector."
4. "Your 'small cost' rests on Box2D's and Rapier's word and on layer 0. The heavy layers (SSI
   marching, fitting, tessellation, mass integration) are full of mul-add chains where FMA and
   free vectorisation are worth the most. Neither is measured, because none of them is ported."
5. "The app already keys its caches by platform. The benefit in 2.3 is hypothetical until the app
   changes, and the cost is paid now."
6. "Every kernel release re-evaluates every design and may move near-threshold results for every
   user at once. Platforms are a minor source of forks next to that. D3 protects the smaller
   door."

**Replies.**

1. Those kernels are C and C++ on the platform libm and contracting compilers, where D3 is
   expensive to get. In Rust it is the default for the basic operations (RFC 3514), and the libm
   is already ours. Their users also mostly evaluate in one place (Onshape) or on one platform.
   SolidCraft evaluates on desktop, web and CLI. And "promises" here means "no public statement
   found", which is weaker than it sounds.
2. Exact predicates would make D3 unnecessary for the decisions they cover. Certified three-way
   tests would not: where `Undecided` begins depends on rounding, so two platforms can disagree on
   whether a test decides (2.9). Most of this kernel's certified decisions will be three-way. So
   certification and D3 are complements, and the point is weaker than the first draft allowed.
3. This is a strong point, and (e) answers it directly. A deliberate perturbation build is a
   better detector than an accidental second platform: it is systematic, it runs nightly, and it
   reports instead of desyncing a user. Without D3, the "detector" fires in a user's browser as a
   broken reference.
4. True, and the first draft's gate could not answer it: step 4 has no NURBS work, rustc has no
   contraction flag, and one number mixed two prices. The revised gate (item 6) separates the
   price of correct rounding from the price of D3, counts instructions on named FMA targets, and
   runs again at steps 5 and 7, where the mul-add-heavy layers exist. Note too that D3 does not
   forbid FMA. It forbids choosing FMA per platform. A loop that needs fused rounding can use
   `eft::fma` everywhere, at a real price on wasm and baseline x86-64.
5. True. The first draft replied that removing D3 later is cheap. That holds only while no
   consumer relies on it; for a public library it is not (5.1). Hence the revised scope: an
   internal goal until step 12, decided then as a public promise or not. Kept internal, removing
   it is still a rule deletion.
6. Largely true, and it lowers the weight of 2.3 to 2.5. It does not touch 2.6 (one oracle), 2.7
   (reproducing a user's failure at the user's version), or 2.9 (certified decisions agreeing),
   which carry the case now. And (f) shows that the "opens the same everywhere" need can be met by
   the app without D3.

**What survives.**

- **(e) stands**, as an internal goal with a stated contract. It is kept because it is cheap where
  measured, its gaps are cheap to close, and its remaining benefits (one oracle, reproduction,
  agreement of certified decisions) are real. It is not kept because it is "correct", and it is
  not yet a public promise.
- **Point 3 changes the recommendation:** the fragility probe is part of (e), not an extra.
- **Point 4 changes the gate:** three builds, instruction counts, named targets, three runs.
- **Point 5 changes the scope:** internal until step 12.
- **Point 6 changes the question to the owner:** what the app needs (section 7, question 1).
- **Point 1 changes the framing.** The owner should read D3 as a product property (one design
  behaves the same everywhere, at one version), not as a correctness requirement. The ruling
  should say so, so that a future measured cost can retire it without reopening "correct".

## 7. Questions for the owner

1. What does SolidCraft need across platforms: (i) a file opens and looks the same everywhere;
   (ii) a user's failure reproduces on a developer's machine; or (iii) the same history
   *re-evaluated* on desktop and web gives the same topology, at one kernel version? (i) can be met
   by the app without D3 (option f); (ii) and (iii) need D3. Kernel releases already re-evaluate
   every design, so (iii) protects against platform forks only, not version forks. This report
   recommends keeping D3 for (ii), for one golden oracle, and so that certified decisions agree,
   whatever the answer on (iii).
2. Is D3 an internal goal owed to SolidCraft, or a public promise of the library? This report
   recommends internal, with the contract in 5.1, until step 12, then decided again with the
   shipped-target legs green. A public promise is hard to withdraw once consumers rely on it.
3. Should correct rounding be kept for uniqueness, with hardest-case inputs added to the `fmath`
   tests? If the gate shows it too costly after Ziv, the fallback is a new faithful portable
   `fmath` that would have to be written and proved; nothing off the shelf is faithful and
   portable. It moves results by at most 1 ulp and gives up uniqueness.
4. May solidcorex recommend `x86-64-v3` desktop builds to the app, once an `x86-64-v3` build
   passes the golden hashes? It is an app build choice that changes no bits.
5. You placed per-platform CI at the end stages (11-26). May the golden-hash legs be extended now,
   at a cost of runner minutes, to the shipped targets: `wasm32-unknown-unknown` under Node,
   `x86_64-pc-windows-msvc` and `aarch64-apple-darwin`, plus an `x86-64-v3` build? Without them
   the contract covers Linux and wasip1 only.
6. May 07 rule 5 be amended to allow fixed-accumulator reductions (id order within each
   accumulator, a fixed combining order)? It moves every affected result once, each checked
   against the truth.
7. Is the nightly fragility probe approved as a CI job?
8. May we tell the SolidCraft app, as an issue, that its cache key leaves out the Rust toolchain,
   and that a native library which sets flush-to-zero in its process breaks agreement (D2 and D3),
   so that it checks the FP environment at start-up?

## 8. Response to the review

Each finding of `reviews/review-67.md`, and what this revision did with it.

| # | Finding | Verdict | What changed |
|---|---|---|---|
| 1 | Rapier's maths is the `libm` crate, not faithful; speed unmeasured | accepted | §1 item 2, §3 Rapier row and reading, §4.2 row 1, (e) item 2: the faithful fallback is a new `fmath` variant, not off the shelf |
| 2 | The step-4 gate cannot measure what it gates | accepted | (e) item 6 rewritten: builds A/B/C, instruction counts, named targets, runs at steps 4, 5 and 7, triggers per comparison; reply 4 |
| 3 | Toolchain versions not considered | accepted | D3t row; §2 on consumers' rustc and the app's cache key; 5.1 "per toolchain"; question 8 |
| 4 | Fast-math is on stable (`algebraic_*`); clippy holes | accepted | §1 item 3, §4.2 row 4; the bans added in a separate commit (`f64::log`, `exp_m1`, `ln_1p`, `asinh`, `acosh`, `atanh`, `f32`/`f64` `algebraic_*`, every `f32` transcendental and `f32::mul_add`). `cargo clippy --workspace --all-targets -- -D warnings` is clean, so no existing code is hit; a throwaway probe showed each new path fires |
| 5 | Platform `fma` still linked; tested targets are not the shipped ones | accepted, except the portable `fma` | D2 row, 2.11, 5.1. `nm` confirms `compiler_builtins`' dispatched `fma` on 1.100.0 linux-gnu; the `fmath` module doc is corrected in the same separate commit. `windows-gnu` excluded (#140515). Rejected: a portable `fma` in `eft`. `fma` is fully specified, so any correct one gives the same bits, which the hashes test; a portable one would be slower on FMA hardware for no change of bits |
| 6 | FP environment can be changed by other code in the process | accepted | §2, 5.1, question 8. The `crtfastmath.o` mechanism stays INFERRED, untested. A debug assertion in kernel entry points is not proposed: it needs platform code in every entry path; a start-up check in the app does the job |
| 7 | Certified three-way tests still need D3 | accepted | 2.9 separates exact predicates from three-way tests; reply 2 |
| 8 | Evidence for one-ulp flips overstated (33, 39) | accepted | 2.7 and 2.9: 33 and 39 relabelled D1-only; OCCT #30012, #31849, #29751 added as UNVERIFIED |
| 9 | Contract imprecise; removal not cheap for a public library | accepted | 5.1 contract paragraph; internal until step 12; reply 5; question 2 |
| 10 | Kernel upgrades fork histories more than platforms; app-side route | accepted | 2.10, option (f), adversarial point 6, weights in 2.3-2.5 lowered, question 1 |
| 11 | (b)'s accuracy argued from the wrong library | accepted | (b): platform libm bounds unmeasured; the real losses stated |
| 12 | Uniqueness asserted, not proved; upkeep not costed | accepted | 4.3; (e) item 2; question 3 |
| 13 | Per-site `eft::fma` costs about 7x on wasm | accepted in substance, figure corrected | The 14.7 ns against 2.1 ns is software `fma` against a Dekker product, not against plain arithmetic; against an unfused multiply-add the ratio is larger and unmeasured. (e) item 4 now allows `eft::fma` only where accuracy needs it |
| 14 | Zero signs from `min`/`max` neither held nor tested | accepted | 2.11, §4.2 last row, 5.1 exception, (e) item 1 |
| 15 | GPU in result paths ruled out | accepted | §4.2 GPU row; wide SIMD stated as fine |
| 16 | (c) dismissed on an unmeasured share; (e) is partly tiered | accepted | (c): share unmeasured, measured by build C; (e) described as chosen exact relaxations |
| 17 | "No kernel promises" is absence of evidence | accepted | §1 item 8, §3 rows and reading; `ifc-lite-geometry` and Blender added as UNVERIFIED |
| 18 | OCCT tracker items could not be re-checked | accepted | §3 OCCT row dated and marked UNVERIFIED now; better-indexed items added, also UNVERIFIED (no re-check was possible here either) |
| 19 | RFC 3514 read further than the text | accepted | §1 item 3 and §3 Rust row quote the operation list, mark no-contraction as following from the semantics, add the wasm and const-eval NaN notes |
| 20 | `powi` bug misdescribed | accepted | 2.6 |
| 21 | Box2D v3, approximate trig | accepted | §3 Box2D row and reading |
| 22 | `x86-64-v3` gain measured with `native` | accepted | §1 item 2, (e) item 3: INFERRED from `native`; a v3 hash leg first; question 4 |
| 23 | Conflict with the whole of 07 rule 5 | accepted | (e) item 4 quotes the rule; results move once, truth-checked; question 6 |
| 24 | App keys its cache by OS as well | accepted | 2.3 |

No finding is rejected outright. Two are accepted in part: finding 5's portable `fma` and finding
13's sevenfold figure, for the reasons in the table.

## Sources

- solidcorex: 04 §1 and §3; 01 §2.2, §2.6, §7.4, §8, §9, §10.2; 02 §6; 05 (`fmath`, the
  benchmark-gaps, golden-hash and Rust 1.100 rows); 07 §7; research 33, 38, 39, 60, 65;
  `crates/scx-num/src/fmath/mod.rs` and `eft.rs`; `clippy.toml`; `.github/workflows/ci.yml`;
  `rust-toolchain.toml`; `reviews/review-67.md`.
- SolidCraft docs: `design-container.md` (model cache, key, references), `native-brep.md`,
  `kernel-smlib-history.md`, `perf.md`.
- tangency-labs/solidcore#1252 (fused multiply-add and the tangent-cylinder Boolean).
- rust-lang/rust#140515 (MinGW `fma` not fused); mingw-w64 bug 848.
- Box2D, "Determinism", https://box2d.org/posts/2024/08/determinism/
- Rapier, "Determinism", https://rapier.rs/docs/user_guides/rust/determinism/ ;
  `crates/rapier3d/Cargo.toml` (Apache-2.0).
- Rust RFC 3514, "Float semantics", https://rust-lang.github.io/rfcs/3514-float-semantics.html
- CGAL Kernel manual, https://doc.cgal.org/latest/Kernel_23/index.html
- Manifold wiki, https://github.com/elalish/manifold/wiki/Manifold-Library
- OCCT tracker #27710, #25531 (read 2026-11-27, not resolvable on re-check), #30012, #31849, #29751
  (search snippets), https://tracker.dev.opencascade.org/
- Onshape API architecture, https://onshape-public.github.io/docs/api-intro/architecture
- Parasolid quality blog, https://blogs.sw.siemens.com/plm-components/parasolid-delivers-30-years-of-quality-in-industrial-use/ ;
  SOLIDWORKS forum, Onshape against SOLIDWORKS volume differences; Novedge blog (UNVERIFIED).
- `ifc-lite-geometry` documentation; Blender T90803 (both search results, UNVERIFIED).
- CORE-MATH: glibc libc-alpha list (2022), FreeBSD numerics list (2025-02).
