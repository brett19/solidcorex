# 67 - Cross-platform bit identity: what it is worth to a CAD kernel

**Status: proposed for review.** Nothing in this report is decided. It changes no code and no rule.

The owner's question (2026-11-27): "Can you do an analysis of how important it really is to a CAD
kernel that it has bit-precision across multiple platforms."

Today solidcorex requires bit-identical results on x86-64, aarch64 and wasm32 (04 §1, 11-26; 01
§8.1, §8.3; 07 §7 rules 1, 4, 5, 9). In practice that means four things:

- no fast-math;
- no platform intrinsics in result paths;
- its own correctly rounded `fmath` in place of the platform libm;
- the golden hashes `fmath_pin` and `xplat_pin`, checked on the `xplat` CI legs (02 §8a).

The goal is "the fastest *correct* kernel" (04 §1, 11-27). This report asks two things. Is
cross-platform bit identity part of "correct"? What does it buy, and what does it cost?

## Labels

- **READ**: read from solidcorex's code or docs, from the pin, from the SolidCraft app's docs, or
  from the source of a project with a permissive licence (Rapier, Apache-2.0). No LGPL or
  proprietary source was read: not OCCT, not glibc.
- **PUBLISHED**: stated in another project's documentation, papers, issue tracker or blog. These
  are claims by those projects, not checked here.
- **MEASURED**: figures already in 05, in `fmath`'s module docs, or in the research those cite.
  Nothing was measured for this report.
- **INFERRED**: reasoning or estimates made here. Every speed figure not labelled MEASURED is
  INFERRED.

## 1. Summary

1. **Bit identity is not part of "correct".** Correct means within a certified bound of the truth
   (04 §1, 11-27; 01 §9.2). Two platforms can both be correct and still differ in the last bit.
   Bit identity is a separate property: reproducibility. It is valuable to this kernel because of
   how SolidCraft uses a kernel. The desktop app, the browser app (wasm) and the CLI each evaluate
   the feature history locally, and none of them is the authority (section 2.3).
2. **Most of the measured cost is not the cost of bit identity.** It comes from two choices that
   sit beside it:
   - **Correct rounding.** `cos` costs about 289 instructions against glibc's 70, `acos` 304
     against 71, and `cbrt` 187 against 110 (MEASURED, 05). That is the price of correct
     rounding. A portable function that is merely faithful, built from plain IEEE operations,
     would give identical bits on every target at close to libm speed. Rapier does exactly this
     (section 3).
   - **The baseline x86-64 target, which has no FMA instruction.** The `fma` call costs about 12%
     of `cubic`'s instructions (MEASURED, 05). With `-C target-cpu=native`, `fmath` is 30 to 40%
     faster and gives the same bits (MEASURED, `fmath` docs).

   Rust already makes `+ - * / sqrt` IEEE-exact on these three targets, and it never fuses or
   reassociates on its own (PUBLISHED, RFC 3514). Bit identity itself therefore forbids only a few
   things:
   - the platform libm;
   - picking FMA or other instructions per CPU;
   - approximate instructions such as `rcp` and `rsqrt`;
   - reductions whose shape follows the hardware's lane width;
   - reading the sign of a zero from `min` or `max`, or the bits of a NaN.
3. **What it buys, concretely, for SolidCraft:**
   - The same design gives the same faces, the same face names and the same stored references on
     the desktop and in the browser.
   - A user's file reproduces a failure on the developer's machine, whatever the user's platform.
   - The model cache and content-addressed bodies could be shared across platforms. Today the app
     keys its cache by platform and evaluates the design again on any other platform (READ,
     SolidCraft `docs/design-container.md`).
   - There is one set of golden hashes instead of three.
4. **It does not buy robustness, and it can hide fragility.** The pin's own history shows this.
   One rounding difference made a tangent-cylinder Boolean unsound on aarch64. The fix built every
   platform with `-ffp-contract=off`, which "hides this case but does not fix it" (READ,
   tangency-labs/solidcore#1252). A decision that flips on one ulp is a correctness defect on every
   platform. Bit identity only makes all platforms take the same wrong branch.
5. **Recommendation (option e, section 5): keep cross-platform bit identity as the contract, and
   stop paying for things it does not require.**
   - Land the Ziv fast path, so correctly rounded functions run near libm speed.
   - Let the app build its desktop x86-64 binaries for `x86-64-v3`, which has hardware FMA and
     gives the same bits.
   - Allow IEEE-exact SIMD, and reductions with a fixed number of accumulators.
   - Add a fragility probe: build with a perturbed arithmetic and look for topology that flips.
   - Re-decide with one corpus-level measurement at the vertical slice (01 §10.2 step 4).
6. **Strongest argument against it (section 6):** no commercial kernel is known to promise bit
   identity. The cost has been measured only on layers 0 and 1, not on the layers whose hot loops
   are heavy on evaluation. And the property breeds false confidence. Part of this survives, and
   it becomes the measurement gate in point 5.

## 2. Who depends on it, and how

Five strengths of determinism need to be kept apart:

| | Property | Who guarantees it today |
|---|---|---|
| D0 | The same process, run twice, gives the same bits | the difftest runs every case twice (05; 02 §6) |
| D1 | The same bits for any thread count, cache state, call order, or save and load | 01 §8.1, 8.3, tested |
| D2 | The same binary on different machines of one ISA (one with AVX2 and FMA, one without) | holds because nothing dispatches on the CPU in a way that changes bits; `two_prod` dispatches, but its result is exact either way (READ, `eft.rs`) |
| D3 | The same source built for different targets (x86-64, aarch64, wasm32) | `fmath`, no contraction, the golden hashes (05) |
| D4 | Different versions of the kernel | not promised; truth over pin allows results to move (04 §1, 11-27) |

D3 is the subject here. D0 and D1 are needed whatever is decided about D3 (owner, 11-26: "the same
operation executed twice always yields the same answer").

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

### 2.1 Same-machine determinism (D0, D1)

This is required by the owner's words and by the design. Memos and cells must not change results
(07 rule 8). Budgets are summed in index order (04 §3, 11-27), and the warnings a cache replays
are part of the result (04 §3, 11-27). Without D1 a warm cache would answer differently from a
cold one, and parallel runs would answer differently from serial ones. None of this depends on D3.
**Weight: essential, and independent of this question.**

### 2.2 The same build on different machines (D2)

The user's laptop, a CI runner, and a support engineer's machine are all x86-64 but have different
CPUs. A bug report replayed there must reproduce. With our own `fmath` and no dispatch that changes
bits, D2 holds today. **Weight: high. Option (b) puts it at risk (above).**

### 2.3 Different platforms: desktop, web and server

These facts are from SolidCraft's docs (READ):

- **The design file is the timeline.** It carries `document.json` plus an optional model cache
  (`design-container.md`, "Model cache"). A design is evaluated locally on the desktop, "as design
  reading in the CLI and browser does". The browser build is wasm, and "everything below the apps
  must build for `wasm32-unknown-unknown`" (READ, quoted in research 03).
- **The model cache is keyed by platform:** "A cache is read back only when the format, the
  producer, the evaluation source, the platform, the kernel and both digests match: the same source
  compiled for the same platform computes the same numbers on the same kernel."
  - So the app today assumes D2, not D3.
  - A design saved on the desktop and opened in the browser is evaluated again in wasm. That is
    the slowest place to do it. OCCT users report STEP reading is much slower in wasm than native
    (PUBLISHED, OCCT forum).
- **The app's own native-B-rep golden allows "1e-10 relative/absolute platform rounding"** for
  floating coefficients (`native-brep.md`). The app does not assume D3 today either.

What D3 would add for the app (INFERRED; each needs an app change):

- The platform could be dropped from the cache key, so a browser opens a desktop-saved model
  without evaluating it again.
- A server-side thumbnail, render or export would match what the user's client shows.

**Weight: medium today, because the app already copes. It becomes high once the app shares caches
or evaluates on a server for a client.**

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
   solidcore#1252 is exactly this case, on aarch64 with contraction.
2. Faces are named from the history (`Changes`, `carry`, `scx-fates`; 01 §2.6). The extra face
   gets a name the other platform never made.
3. Feature 9 is a fillet that stores a reference to "the edge between face 7.3 and face 7.5". It
   finds no edge on one platform. The app's documented response to a name that finds nothing is
   that "the features holding them ask for their references to be picked again"
   (`design-container.md`).
4. If two people exchange the file and re-pick, the history now forks by platform.

With D3, both platforms take the same branch. The design is then consistent: possibly consistently
fragile, but the same everywhere.

Onshape avoids the problem by design. Its feature list is regenerated by its modelling servers, and
its "Topology ID is sometimes referred to as a Deterministic ID" (PUBLISHED, Onshape developer
docs; that the servers do the regeneration is INFERRED from its changelog's "modeling server").
There is one evaluator, so cross-platform identity never comes up.

**Weight: high for a product that evaluates on more than one platform, as SolidCraft does.**

### 2.5 Caching keyed by result hashes

`cache/bodies.bin` stores each distinct body once, keyed by the SHA-256 of its encoding (READ,
`design-container.md`). Within one platform that needs D1. Across platforms, content addressing
(a shared or server cache of bodies, or deduplicated uploads) needs D3. Without D3 the same design
yields different hashes on desktop and web, so it never hits a shared cache. **Weight: medium; it
matters only once caches are shared.**

### 2.6 Regression testing with golden files

- solidcorex has no bit dumps. The difftest compares with the pin within tolerance (01 §9.1).
- The bit-exact checks are the golden hashes: 24 for `fmath`, 6 for `scx-algo`, in 0.1 s. They
  are identical on all three targets (MEASURED, 05).
- Without D3 each hash needs a value per target. 07 §7 rejected that because "it doubles the
  oracle". In practice that means three blessings per change, and a mismatch on one target cannot
  be told apart from a bug there.
- The first aarch64 run of the hashes found a real bug: `powi` returned NaN for `|x| >= 2^1023`.
  It surfaced only as a NaN sign that differs between x86 and aarch64 (MEASURED, 05; 04 §3,
  11-27). A per-platform golden would have blessed it.

**Weight: high, and cheap to keep.**

### 2.7 Bug reproduction from a user's file

Users run on Windows and macOS on x86-64 or Apple silicon, and in browsers. Developers and CI run
on Linux x86-64 and aarch64. With D3, any user's file reproduces bit for bit on any developer
machine. Without it, a failure that depends on a near-threshold decision may not reproduce at all.
That is exactly the class of failure that is hardest to find by other means. Research 33 shows how
small the trigger can be. Settling a body first changed four validity-check reports, through trims
kinked at cos 0.9957 against a limit of 0.99985, and ray answers moved by up to 3.6e-14 with read
order alone (MEASURED in the pin). **Weight: high.**

### 2.8 Topology naming and persistent ids

Ids are dense per body, and names come from `Changes` (01 §2.2, §2.6). Names survive geometric
change that keeps the topology. They break only when the topology differs, so D3 matters here only
through 2.9. **Weight: indirect.**

### 2.9 Tolerance decisions that flip on one ulp

This is where every other dependency gets its force. Evidence that such flips happen:

- solidcore#1252: a tangent-cylinder Boolean is sound or unsound depending on whether `a*b+c` is
  fused. The opposite case, which fails without contraction, was filed alongside (READ).
- Research 39: face order alone moves mass in 173 of 474 bodies (MEASURED in the pin, quoted in
  01 §2.2).
- Research 33: four validity reports flipped with read order (MEASURED in the pin).

The two remedies are different:

- **Same bits everywhere** gives the same branch everywhere. It does not give the right branch.
- **Certified decisions** give the right branch, or an explicit `Undecided`, whatever the
  rounding. Examples are the certified section of `scx-num`, exact predicates, and the Krawczyk
  uniqueness tests (65 §2.1). CGAL's kernels are built on this idea: they "provide exact geometric
  predicates", while constructions may be inexact (PUBLISHED, CGAL Kernel manual).

D3 is a safety net under the second remedy, not a replacement for it. The kernel needs certified
decisions anyway: 01 §9.2 makes truth the oracle. **Weight: high, but mostly as an argument for
certified predicates. The safety net is cheap, so keep it as well.**

## 3. What other kernels and systems do

| System | What it promises | Label |
|---|---|---|
| Parasolid | No public statement on cross-platform bit identity was found. Siemens claims "zero tolerance for regressions" in its quality testing. Users see volume differences of about 3e-4 mm³ between Onshape and SOLIDWORKS on one part, both on Parasolid, and the cause is unanswered | PUBLISHED (blog, forum); no claim found |
| ACIS | No public statement found | not found |
| OCCT | Its regression tests compare with fixed tolerances (such as 1e-7). The tracker says complex algorithms "often cannot provide this precision, and the results are often different on LINUX and WINDOWS platforms". It warns against widening tolerances to hide real platform bugs (#27710). A `bopcurves` regression appeared only on Linux (#25531) | PUBLISHED (tracker). Source not read (LGPL) |
| CGAL | Exact predicates with inexact constructions (Epick), so combinatorial decisions do not depend on rounding. Constructed coordinates may differ | PUBLISHED (manual) |
| Manifold | Topology is exact and geometry inexact. "We will choose a definition of manifoldness which relies solely on topology, so as to get consistent results." No claim of bit identity found. It is tested on GCC, LLVM and MSVC, and parallel through TBB | PUBLISHED (wiki, README) |
| Onshape | A single server-side evaluator, so the question does not arise. Topology IDs are "deterministic ids" tied to history | PUBLISHED, partly INFERRED (2.4) |
| Box2D v3.1 | Cross-platform determinism on x64 and ARM, under MSVC, GCC and Clang, checked in CI on every PR. Fast-math off ("never use this setting"), `-ffp-contract=off`, and its own `sinf`, `cosf` and `atan2f`, because `atan2f` differed by platform. "No noticeable performance impact", because it was designed in from the start. No rollback determinism | PUBLISHED (box2d.org, 2024-08) |
| Rapier | By default only locally deterministic: same machine, version and compiler. `enhanced-determinism` gives cross-platform identity on IEEE 754-2008 targets, including wasm. The feature is `simba/libm_force` plus Parry's equivalent, that is, portable software libm. It cannot be combined with `simd8`. The parallel solver matches the serial one | PUBLISHED (rapier.rs); READ (Cargo.toml, Apache-2.0) |
| Rust itself | `+ - * / % sqrt mul_add`, casts and comparisons are IEEE 754-2008 exact apart from NaN bits. Contraction into FMA is ruled out. NaN sign and payload are non-deterministic. Exceptions are i586 without SSE2 (x87 precision) and 32-bit ARM NEON (flush to zero) | PUBLISHED (RFC 3514) |
| CORE-MATH | Correctly rounded libm functions. The authors claim speed competitive with glibc. glibc now ships 21 correctly rounded single-precision functions from it | PUBLISHED (glibc list, FreeBSD numerics list) |

Reading of the precedent (INFERRED):

- **The CAD kernels do not offer bit identity, and they live with the consequences.** OCCT's
  tracker shows the cost: tolerance-widened tests and bugs found on one platform only. The
  commercial kernels say nothing. Their main users either evaluate in one place (Onshape) or treat
  each platform's result as authoritative for itself (desktop SOLIDWORKS, NX).
  - These kernels are C and C++ built on the platform libm and a compiler that may contract.
    Getting D3 there means fighting the toolchain.
  - In Rust, D3 is the language default for the basic operations. The remaining work is the libm,
    which solidcorex already owns.
- **The systems that need replay across machines do provide it, and say it is cheap.** These are
  the lockstep and rollback physics engines. Box2D reports no noticeable cost. Rapier's
  determinism comes from portable software math, not correctly rounded math.
- **CGAL and Manifold show the robust alternative:** make the decisions exact, and let the
  coordinates differ. That is option (c)'s core, and it is what 2.9 asks for anyway.

## 4. Cost

### 4.1 What we have measured

| Figure | Value | Source |
|---|---|---|
| `fmath` per call against the platform libm (baseline x86-64) | 1.07x (`hypot`) to 3.72x (`log2`). `sin` and `cos` 1.72 to 1.74x, `acos` 3.38x, `cbrt` 1.90x, `powf` 3.22x | MEASURED, `fmath/mod.rs` docs; noise up to 35% between runs |
| Instructions per call | `cos` about 289 against glibc's 70; `acos` 304 against 71; `cbrt` 187 against 110 | MEASURED, 05 (callgrind) |
| `cubic` against the pin | 1,021 against 448 instructions, 207 against 90-97 us before polishing. The solver's own arithmetic matches (about 102 against 104); the gap is `fmath` | MEASURED, 05 |
| `cubic` and `quartic` after Newton polishing | 3.68x and 2.64x the pin's time (from 2.30x and 1.69x) | MEASURED, 04 §3 (11-27). This is the price of accuracy (median 0.26 ulp), not of bit identity |
| `fma` on baseline x86-64 | about 12% of `cubic`'s instructions, through compiler_builtins | MEASURED, 05 |
| `fmath` built with `-C target-cpu=native` | 30 to 40% faster (`sin` 21.8 ns, `ln` 11.3 ns, `powi` 35.2 ns), same bits | MEASURED, `fmath` docs |
| `two_prod` on wasm32 | 14.7 ns with software `fma`, 2.1 ns with Dekker's product, same bits | MEASURED, 05 |
| Ziv fast path (being built) | expected to recover about 30% of `cubic` | 05's estimate; INFERRED |
| The `xplat` CI legs | `fmath_pin` and `xplat_pin` take 0.1 s each run. The legs' wall time was not recorded | MEASURED, 05; CI time unmeasured |
| Corpus-level cost | **not measured.** No operation above layer 2 runs yet | none |

### 4.2 Which hot paths pay, and for what

Each cost is attributed to the rule that actually causes it (INFERRED):

| Cost | Caused by D3? | Caused by correct rounding? | Caused by the baseline target? | Recoverable while keeping D3 |
|---|---|---|---|---|
| `fmath` transcendentals, 1.1 to 3.7x libm | only the ban on the platform libm | **yes, most of it**: double-double evaluation and rounding tests | partly: `two_prod` calls `fma` | Ziv fast path (about 30% of `cubic`, INFERRED). A faithful portable function would be near libm speed (as Rapier's libm is) but moves results by up to 1 ulp |
| The `fma` dispatch in `two_prod` | no: the result is exact either way | yes: only the error-free products need it | **yes**: baseline x86-64 has no FMA | build the app for `x86-64-v3` (MEASURED 30-40% on `fmath`, same bits) |
| No FMA contraction in evaluation (Horner, de Boor, dot products) | **yes**, if FMA were chosen per target. wasm32 has no FMA instruction (relaxed-simd's `relaxed_madd` is non-deterministic) | no | partly | a deliberate `eft::fma` gives the same bits everywhere at wasm's cost (software `fma`, about 15 ns). Gain on native, INFERRED: up to about 1.5x on latency-bound Horner loops |
| Lost reassociation and auto-vectorised reductions | no. Rust never reassociates by default, and fast-math is not available in stable Rust | no | no | reductions with a fixed number of accumulators (say four, whatever the lane width) are identical everywhere and vectorise. This needs a change to 07 rule 5 ("no cross-lane horizontal sums") |
| SIMD intrinsics | only for approximate instructions (`rcp`, `rsqrt`), per-target FMA, and `min`/`max` zero and NaN semantics | no | no | IEEE lane operations (add, mul, div, sqrt) are exact on SSE2, NEON and simd128. 65 §3.1 already notes SoA blends are bit-identical without contraction |
| Golden hashes and xplat legs | yes | no | no | already small |
| `f64::min`/`max` signed-zero handling | yes: x86 `min` gives -0 on `[0, -0]` | no | no | a 3-D-only fold needs a ruling on zero signs (05) |

**What bit identity alone costs, then, is small:**

- the gap between a portable faithful libm and the platform libm (INFERRED near 1x, on Rapier's
  and Box2D's evidence);
- no per-target FMA in evaluation loops;
- care with zero signs and NaN bits.

The large measured figures are correct rounding and the x86-64 baseline. Both have remedies that
keep the bits.

## 5. The options

These criteria apply to every option:

- **Truth over pin** (04 §1, 11-27) is about correctness against references. No option weakens it,
  since each keeps results within certified bounds.
- **The difftest** compares with the pin within tolerance on x86-64 only, plus a two-run bit check
  (D0). No option changes it. The pin itself uses the platform libm, so the pin is not D3, and a
  pin comparison on another platform would need its own tolerance anyway.

### (a) Keep full cross-platform bit identity, as today

- **Breaks:** nothing.
- **Speeds up:** nothing beyond the Ziv fast path. The measured gaps stay where the Ziv path
  falls short.
- **Testing:** today's two CI legs. Later (step 12), a golden table of record hashes per operation
  on three targets: one oracle.
- **Truth and difftest:** unaffected.
- **Weak point:** it pays for the x86-64 baseline and for not vectorising reductions without
  noticing. Neither is required by D3.

### (b) Same-platform determinism only; cross-platform results "equally correct within certified bounds"

- **Breaks:**
  - desktop and web agreement on near-threshold topology (2.4);
  - cross-platform reproduction of user files (2.7);
  - sharing caches across platforms (2.5).
  - With the platform libm, D2 as well (section 2): glibc's choice by CPU and its version
    differences.
- **Keeps:** the app's platform-keyed cache, which already works this way.
- **Speeds up:**
  - Transcendentals at libm speed: `cos` 70 instructions against 289 (MEASURED). On `cubic` that
    is most of the 1,021 to 448 gap. Libm loses correct rounding, so interval code loses its
    proved 0.5 ulp bound: the `libm` crate reaches 496 ulp on `powf` (MEASURED, 04 §3).
  - Per-target FMA in evaluation (INFERRED 10-30% in evaluation-heavy loops on FMA hardware).
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
    polylines bit for bit (SolidCraft `perf.md`).
  - Evaluation feeds every Newton step whose result is a vertex position that gets persisted.
- **Speeds up:** only paths that provably never feed a decision or a stored value. Today that is
  the app's GPU display path, which is outside the kernel. INFERRED: a small share of kernel time.
- **Testing:** an audit, by type or lint, that no free value reaches a decision. This is new and
  ongoing work, and a leak is invisible until it causes a desync.
- **Truth and difftest:** unaffected.
- **Its sound core is the certified-predicate idea (2.9).** That belongs in every option, not as a
  tier.

### (d) Bit identity per target family

The families would be "FMA" (aarch64, `x86-64-v3`) and "no FMA" (wasm32, baseline x86-64).

- **Breaks:** desktop and web agreement. That is the pair SolidCraft most needs (2.3), since wasm
  is always in the "no FMA" family.
- **Speeds up:** FMA in evaluation loops on native (INFERRED, as in (b)), and the transcendentals
  by 30-40% (MEASURED with `native`). But the second gain is also available under (a), because the
  bits are the same.
- **Testing:** two golden sets.
- **Truth and difftest:** unaffected.
- **Verdict:** it gives up the valuable pair to gain only the FMA-in-evaluation part.

### (e) Proposed: keep D3 as the contract, and decouple the costs that D3 does not cause

1. **Keep D3** on x86-64, aarch64 and wasm32, with the contract stated precisely:
   - IEEE 754 targets with SSE2-class arithmetic, so i586 and 32-bit ARM NEON are excluded
     (RFC 3514);
   - NaN bits excepted where they come from arithmetic. `fmath` already returns the one
     `f64::NAN` (04 §3, the NaN-hash row).
2. **Correct rounding stays, through the Ziv fast path.** Its unique value is not accuracy, since
   0.5 against 1 ulp is far inside every tolerance. It is that the answer is unique: `fmath` can be
   rewritten faster later, by Ziv or by CORE-MATH-style kernels, without moving a single result or
   re-blessing a hash.
   - Change if the Ziv path recovers too little at corpus level (item 6). The next step would be a
     faithful portable `fmath`, still D3, as Rapier does. That moves results by at most 1 ulp,
     which truth over pin accepts as equally correct within certified bounds. It needs the
     owner's ruling, because it gives up uniqueness.
3. **Recommend `x86-64-v3` builds to the app** for desktop x86-64, with a baseline fallback binary.
   Hardware FMA there gives `fmath` 30-40% (MEASURED, `native`) and drops the 12% dispatch
   (MEASURED) with identical bits, so no kernel rule changes. This is not a library feature (01
   §7.4: "none changes a result"; this one does not either).
4. **Allow, by rule, what D3 permits:**
   - IEEE-exact SIMD lanes;
   - reductions over a fixed number of accumulators, fixed in source and independent of lane width
     (amend 07 rule 5);
   - a deliberate `eft::fma` in a hot evaluation loop where it is more accurate, accepting wasm's
     software cost. This trade is decided per site, on accuracy and on wasm speed.
5. **Add a fragility probe:** a CI job, off the default path and nightly, that builds with one
   perturbation and reports operations whose topology changes. Candidate perturbations:
   - `fmath` swapped for a faithful variant;
   - evaluation contracted to FMA;
   - inputs nudged by one ulp.

   Each topology change is a near-threshold decision. It should become `Undecided` or be
   certified. This is the part of #1252 that bit identity hides (INFERRED; the method is new here).
6. **Gate.** At the vertical slice (01 §10.2 step 4), run the end-to-end difftest corpus twice:
   once as built, and once with a local, uncommitted "platform-free" variant (std libm, FMA
   contraction where LLVM can). The gap is the price of D3 plus correct rounding at corpus level.
   - Change if: after Ziv the gap exceeds 10% of corpus time (JUDGED threshold).
   - Then revisit (b) or (c) for the named hot paths, with the figures in hand.

For (e) against the other options:

- **Breaks:** nothing.
- **Speeds up:**
  - transcendentals by the Ziv recovery (about 30% of `cubic`, INFERRED);
  - 30-40% more on `x86-64-v3` desktop builds (MEASURED, `native`);
  - reductions and blends by vectorisation, up to 2x on 2-lane targets (INFERRED, 65 §3.1).
- **Testing:** today's legs, plus one nightly fragility job (INFERRED minutes), plus one
  corpus-level measurement at step 4.
- **Truth and difftest:** unaffected. The fragility probe adds truth checks where it finds flips.

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

**Replies.**

1. Those kernels are C and C++ on the platform libm and contracting compilers, where D3 is
   expensive to get. In Rust it is the default for the basic operations (RFC 3514), and the libm
   is already ours. Their users also mostly evaluate in one place (Onshape) or on one platform.
   SolidCraft evaluates on desktop, web and CLI, and that is the case Box2D and Rapier built D3
   for.
2. Certified decisions are the right fix, and (e) asks for them (item 5). But no kernel certifies
   every decision. The pin certifies almost none (65 §2.1: "nothing is certified"). Until the last
   uncertified threshold is gone, D3 is what keeps a file consistent between a desktop and a
   browser.
3. This is the strongest point, and (e) answers it directly. A deliberate perturbation build is a
   better detector than an accidental second platform: it is systematic, it runs nightly, and it
   reports instead of desyncing a user. Without D3, the "detector" fires in a user's browser as a
   broken reference.
4. True, and unanswerable without measurement. So (e) includes the step-4 gate: one corpus run
   with a platform-free variant. Note too that D3 does not forbid FMA. It forbids choosing FMA per
   platform. A loop that gains by FMA can use `eft::fma` everywhere, at a cost only on wasm.
5. True. But removing D3 later is cheap: delete a rule. Adding it later is expensive: find every
   site that came to depend on per-platform freedom. Keeping the stricter rule while it is cheap
   is the option that stays open.

**What survives.**

- **(e) stands**, with point 4 promoted from a caveat to the gate. D3 is kept because it is cheap
  where measured and its main benefit is real for SolidCraft's architecture. It is not kept
  because it is "correct".
- **Point 3 changes the recommendation:** the fragility probe is part of (e), not an extra.
- **Point 1 changes the framing.** The owner should read D3 as a product property (one design
  behaves the same everywhere), not as a correctness requirement. The ruling should say so, so
  that a future measured cost can retire it without reopening "correct".

## 7. Questions for the owner

1. Is cross-platform bit identity a product requirement (the same design behaves identically in
   the browser and on the desktop), or only a goal? This report recommends treating it as a
   product requirement, re-decided at the step-4 gate.
2. Should correct rounding be kept for uniqueness, with faithful rounding as the named fallback if
   the Ziv path falls short? Faithful rounding moves results by at most 1 ulp and gives up
   uniqueness.
3. May solidcorex recommend `x86-64-v3` desktop builds to the app? It is an app build choice that
   changes no bits.
4. May 07 rule 5 allow fixed-accumulator reductions?
5. Is the nightly fragility probe approved as a CI job?

## Sources

- solidcorex: 04 §1 and §3; 01 §2.2, §2.6, §7.4, §8, §9, §10.2; 02 §6 and §8a; 05 (`fmath`, the
  benchmark rows, golden hashes); 07 §7; research 33, 38, 39, 60, 65; `crates/scx-num/src/fmath/mod.rs`
  and `eft.rs`.
- SolidCraft docs: `design-container.md` (model cache, key, references), `native-brep.md`,
  `kernel-smlib-history.md`, `perf.md`.
- tangency-labs/solidcore#1252 (fused multiply-add and the tangent-cylinder Boolean).
- Box2D, "Determinism", https://box2d.org/posts/2024/08/determinism/
- Rapier, "Determinism", https://rapier.rs/docs/user_guides/rust/determinism/ ;
  `crates/rapier3d/Cargo.toml` (Apache-2.0).
- Rust RFC 3514, "Float semantics", https://rust-lang.github.io/rfcs/3514-float-semantics.html
- CGAL Kernel manual, https://doc.cgal.org/latest/Kernel_23/index.html
- Manifold wiki, https://github.com/elalish/manifold/wiki/Manifold-Library
- OCCT tracker #27710, #25531, https://tracker.dev.opencascade.org/
- Onshape API architecture, https://onshape-public.github.io/docs/api-intro/architecture
- Parasolid quality blog, https://blogs.sw.siemens.com/plm-components/parasolid-delivers-30-years-of-quality-in-industrial-use/ ;
  SOLIDWORKS forum, Onshape against SOLIDWORKS volume differences.
- CORE-MATH: glibc libc-alpha list (2022), FreeBSD numerics list (2025-02).
