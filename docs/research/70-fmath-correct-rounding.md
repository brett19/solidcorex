# 70 - Correct rounding in `fmath`: the design and the error bounds

Written for solidcorex, 2026-11-27. **Status: done, for review.** This note covers the branch that
makes every `scx_num::fmath` function correctly rounded (round to nearest, ties to even). Before
it, `tests/fmath_worst.rs` (research 69, N1) found 3,174 misrounded results among CORE-MATH's
worst cases, and `powf` misrounded exact halfway points. After it, every count is 0, and the test
pins them there.

The proofs live in the code, on the constant each one justifies. This note gives the design, the
table of bounds with what was observed against them, and the arguments that are not about error
bounds: exactness and halfway points.

## 1. Sources

- A. Ziv, "Fast evaluation of elementary mathematical functions with correctly rounded last bit",
  ACM TOMS 17(3), 1991. The strategy: evaluate, test, evaluate more precisely.
- F. de Dinechin, C. Lauter, J.-M. Muller, "Fast and correctly rounded logarithms in
  double-precision", RAIRO-ITA 41, 2007, and the CRlibm documentation. The test
  `hi + fl(lo e) == hi` and its constant. `dd::rounds_to_hi` derives the constant afresh.
- J.-M. Muller et al., *Handbook of Floating-Point Arithmetic*, 2nd ed., 2018. Double-double
  error bounds, and Lindemann's theorem as used for exactness (§5).
- A. Sibidanov, P. Zimmermann, S. Glondu, "The CORE-MATH Project", ARITH 2022, and the project's
  code and `.wc` files (MIT). The overall shape: a fast double-double phase with a proven bound,
  then an accurate phase. The worst cases come from there too. No CORE-MATH code was copied: the
  polynomials, tables and reductions are `fmath`'s own, from before this branch.

## 2. Design

Each function takes up to three phases. Each phase ends in a rounding test with a proven radius.

1. **Fast.** This is the existing double-double evaluation, unchanged except that its low word is
   now kept. `cos`, `sin`, `tan`, `acos` and `cbrt` first run a plain-double kernel and test it.
2. **Precise** (trigonometric, inverse trigonometric, `cbrt`). A tighter double-double: a longer
   series, or the series tail carried in double-double. It runs at under 2% of calls.
3. **Accurate** (`accurate.rs`, on `big.rs`). The value recomputed in `Big`, a binary float with
   1 to 8 limbs of 64 bits, using integer arithmetic only. Each operation truncates toward zero,
   so its relative error is under `2^(1 - 64p)`. `ziv` runs a ladder of precisions (128, then 512
   bits; `pow` starts at 192, §4.6). It rounds at the first precision whose ball
   (`2^(C - 1 - 64p)`, `C = 48`) rounds to one double. Exact and halfway results are decided
   apart from the ladder (§5).

The tests, in `dd.rs`:

- `rounds_to_hi(hi, lo, e)`: Ziv's test, three operations, for a relative radius. It is proved
  for `eps < 2^-54 (1 - (1 + 2^-53)/e)`.
- `round_or(hi, lo, r, acc)`: `rounds_to_hi` with `e = 1 + 2^54 r`, for `r <= 2^-56`. When that
  fails it tries `round_ball`: whether both ends of `hi + lo ± r|hi|` round to the same double.
  The ball is a little sharper. Only after both fail does it call `acc`.
- `round_ball_or(hi, lo, err, acc)`: the ball test alone, for an absolute radius.
- `round_scaled_or(hi, lo, r, k, acc)`: tests the unscaled pair, then rounds `(hi + lo) 2^k` once.
  That covers subnormal and overflowing results. When the unscaled test fails, or the scaled
  result is subnormal, it tries the ball of the scaled ends (`#[cold]`). It is
  `#[inline(always)]`, which saved 7 instructions on `powf`.

Every radius is at least twice the proven bound, so the tests' own roundings (under `2^-107`
relative) sit inside the margin. A passing test keeps the double the phase computed. Where the old
code was already correct, the bits are the same; no pinned hash changed (§7).

## 3. Determinism

The branch adds only `+ - * /`, `sqrt`, the `eft` transformations and integer arithmetic, so it
gives the same bits on every IEEE target. `big.rs` uses `u64`/`u128` limbs and an `i64` exponent,
so nothing overflows on the way. Which phase decides an input depends only on the input. The
golden hashes (`fmath_pin`, `xplat_pin`) pass unchanged on x86_64, and under wasmtime on
`wasm32-wasip1`.

## 4. Bounds

"Bound" is the proven error of the phase against the true value. "Radius" is what the test
receives (twice the bound, or more). "Seen" is the largest error a probe measured against
`scx_testkit::mp` on random and worst-case inputs, where one was run. All values are relative
unless marked absolute.

### 4.1 `exp`, `exp2`, `sinh`, `cosh`, `tanh`

| path | bound | radius |
|---|---|---|
| `exp_core`, `exp_reduced` | `2^-68` | `2^-67` (`EXP_ERR`) |
| `sinh`/`tanh` series, `|a|` small | `2^-68`, `2^-67` | `2^-67`, `2^-66` |
| `cosh` through `e^a` | `2^-68` | `2^-67` |
| `sinh`, `tanh` through `e^a` | `2^-68` of `e^a`, absolute | ball, times the cancellation (`coth a < 8.1`, `2q/tanh a < 7.1`), plus `2^-99` |

`sinh` and `tanh` state their error absolutely. The cost of cancellation then shows up as a
fallback only where it happens.

### 4.2 `ln`, `log2`, `log10`

`split`'s reduced logarithm is within `2^-73.5` absolute plus `2^-100` relative everywhere. In the
two intervals next to 1 (`c = 1`, `|r| < 2^-7`) it is also within `2^-66.5` relative: the series
tail is evaluated to `2^-52.5 |r|^3`. The probe saw `2^-74.7` and `2^-66.95`. The old docs claimed
`2^-70` relative; that is untrue next to 1 and is corrected.

- For `e != 0`, `|h| >= 0.149`, so a relative test at `2^-70.75` covers the absolute bound.
- For `e = 0`, `|h| < 2^-9` gets a relative ball at `2^-65.5`. Elsewhere the ball is
  `abs + 2^-98 |h|`, with `abs` = `2^-72.5`, `2^-71.94` or `2^-73.68` in each function's units.

### 4.3 `sin`, `cos`, `sin_cos`, `tan`

The reduction: no double lies within `2^-60.89` of a multiple of `pi/2`. So the reduced argument's
relative error is at most `2^-66.4` for the fast kernel. For the precise kernel it is `2^-104`
relative plus `2^-127.3` absolute. The old module doc claimed `2^-120` absolute, which was untrue.

| path | bound | radius / test | seen |
|---|---|---|---|
| fast kernel (doubles) | `2^-61.93` with the reduction | `e = 1 + 2^-6`, good for `2^-60.02` | `2^-65.4` |
| precise double-double | `2^-82.6` | ball `2^-81.5 |h| + 2^-126` | |

The old `2^-63` middle kernel was replaced. As a middle stage its `2^-64.5` radius sent 0.2-0.5%
of calls to the fallback. `sin` now takes the fast path, which only `cos` had before. `tan`
divides the fast pair and runs the same test.

### 4.4 `asin`, `acos`, `atan`, `atan2`

| path | bound | radius | seen |
|---|---|---|---|
| fast (`atan_dd` etc.) | `2^-65.3` | `2^-64.3` | `2^-67.28`, `2^-66.98`, `2^-67.24` (`atan`, `asin`, `atan2`) |
| `fast_acos` (cubic) | `2^-64` (`2^-65.8` by the budget) | `e = 1 + 2^-7`, good for `2^-61.01` | `2^-66.01` |
| precise (`finish_precise`) | `2^-79` | `2^-78` | `2^-81.62` to `2^-81.99` |

The fast bound covers four terms: the series truncation (`d^11/11`, under `2^-73.5 |d|`), the
ratio `d` (one corrected division of double-double sums), the table (within `2^-100`), and the
`pi/2 -` and `pi -` steps (`2^-104`).

`atan2`'s branch for a tiny ratio returned `y/x`, and that misrounds when the quotient is
subnormal: a midpoint is possible there, and a quotient just below `2^-1022` can round up to it.
Now only a quotient strictly above `2^-1022` is returned directly, and the rest go to the
fallback. On that branch `t^2/3 < 2^-121`. A normal `y/x` is not a midpoint, and it lies no
closer to one than `2^-108` of itself: a midpoint has 54 significant bits, so `y - m x` is a
nonzero multiple of a fixed unit. So `atan(y/x)` rounds as `y/x` does. The comment at the
branch in `inverse.rs` gives the proof. The worst-case counter-example was `y =
5.13067100162297e-290`, `x = 2.305843009213694e18`.

### 4.5 `hypot`, `cbrt`

| path | bound | radius |
|---|---|---|
| `hypot` double-double root | `2^-99` | `2^-98`, then the exact search |
| `cbrt` fast | `2^-65` | `e = 1 + 2^-9`, good for `2^-63.01` |
| `cbrt_dd` | `2^-96` | `2^-95`, then the exact search |

Unscaled `hypot` roots, the common case, return `hi` straight after `rounds_to_hi`. Scaled roots
and roots that fail the test go out of line. So `hypot` got cheaper.

### 4.6 `powi`, `powf`

- **`powi`.** The radius is `(|n| + 1) 2^-102`. Each `dd::mul` of normalised pairs is within
  `2^-103`. Squaring doubles a relative error, so the `j`-th square is within `(2^j - 1) 2^-103`.
  The products for the set bits of `|n|` sum to at most `|n| 2^-103`, and the reciprocal adds one
  more.
- **`powf`.** The radius is `2^-66.9 + 2^-72.5 |y|`. The first term is `exp_core`'s `2^-68`
  twice, plus `z`'s own roundings at `|z| <= 746`. The second is `ln_dd`'s global `2^-73.5`
  absolute bound, twice, times `|y|`: an absolute error in `z` is a relative error in `e^z`. The
  fast path has no branch. When the radius would pass `2^-56` (`|y|` above about `2^16.5`), a
  cold path uses the relative bound near 1. There, `|ln x| < 2^-9`, the error is at most `2^-52.5
  |r|^3 < 2^-70.4 |ln x|`, and the radius is `2^-66.9 + 2^-69.4 |z|`. Otherwise it goes straight
  to `accurate::pow`.

  Checked against `mp::pow` on 4,000 inputs with `x` within `2^-20` of 1 and `|y|` up to
  `2^56`: 0 mismatches (a scratch test, not committed).
- **The accurate ladder for `pow`.** It starts at 192 bits (`big::ziv_on(&[3, 8], C + 12, ...)`).
  At 128 bits the ball of `C + 12` is `2^-69`, wider than the fast radius, so it could never
  decide a case the fast test had failed, and every fallback paid for both levels. Starting at
  192 bits cut the fallback from 172k to 62.6k instructions.

## 5. Exact results and halfway points

A Ziv loop never ends on an input whose true result is a midpoint between two doubles, because no
precision separates it from the midpoint. A result that is itself a double is no problem: the
test passes once the ball is under half an ulp. So each function needs one of two things: an
argument that midpoints cannot occur, or an exact decision.

- **`exp`, `exp2`, `sinh`, `cosh`, `tanh`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, and
  `atan2` off its tiny branch.** By Lindemann's theorem, `e^x` is transcendental for algebraic
  `x != 0`, and so are the trigonometric and inverse trigonometric values at nonzero algebraic
  arguments. A double is rational, so none of these results is a midpoint (a dyadic rational).
  The exceptions are the arguments with rational results (`0` for all of them, and `acos(1) =
  0`), which come out exact. `exp2(x)` is rational only for integer `x`, where it is an exact power
  of two. `atan2(y, x)` with `y/x` rational is `atan` of a rational, which is transcendental
  unless the rational is 0.
- **`ln`, `log2`, `log10`.** `ln x` for a rational `x != 1` is transcendental. `log2 x` is
  rational only for a power of two, and is then an integer. `log10 x` is rational only for an
  integer power of 10, and is then an integer. Integers are doubles, never midpoints.
- **`hypot`.** `sqrt(a^2 + b^2)` can be a midpoint: `(3k, 4k)` with `5k` needing 54 bits. When
  the double-double test fails, `accurate::hypot_exact` compares the exact sum of squares (under
  240 bits) with the squares of the midpoints next to the candidate (108 bits). The integer
  comparison is exact, and an equal case is a tie to even.
- **`cbrt`.** A midpoint is `m 2^k` with `m` odd and 54 bits wide. Its cube is `m^3 2^(3k)`,
  where `m^3` is odd and over 159 bits wide, so it is never a double. No tie arises. `accurate::cbrt_exact` decides by the exact cube of the midpoint against `x`.
- **`powf`, `powi`.** `accurate::exact_pow` first checks whether `|x|^y` is a dyadic rational.
  Write `|x| = M 2^E` with `M` odd.
  - For `M = 1`, the result is `2^(Ey)`, dyadic only for an integer `Ey`.
  - Otherwise `y` must be `N/2^K` (`N` odd, `K <= 5`), `M` must be a perfect `2^K`-th power
    `R^(2^K)`, and `2^K` must divide `E`. The result is `R^N 2^(EN/2^K)`, computed exactly in
    `Big` and rounded once. Ties come out to even: `powf(1781, 5)` and `powf(194701.5625, 2.5)`
    are now right and are asserted in `tests/fmath.rs`.

  Any other `x^y` is irrational (Gelfond-Schneider for irrational algebraic `y`; a rational
  `y = N/D` in lowest terms with `D` not a power of two would need `x` to be a perfect `D`-th
  power of a rational whose root has a denominator other than a power of two). So it is not a
  midpoint. `powi` uses the same check before squaring in `Big`.

## 6. Cost

All figures were measured 2026-11-27 off the repository. Instructions per call are from callgrind
over the inputs of the `fmath` docs' speed table, fallbacks included. "Fallback" is the share of
calls that reached the accurate phase, out of 10^6. "Precise" is the share that reached the
middle double-double.

| function | before | after | fallback | precise |
|---|---|---|---|---|
| `exp` | 165.0 | 169.9 | 2.7e-4 | |
| `exp2` | 158.0 | 162.0 | 2.9e-4 | |
| `sinh` | 214.5 | 237.6 | 3.4e-4 | |
| `cosh` | 199.0 | 215.6 | 3.6e-4 | |
| `tanh` | 223.8 | 244.1 | 3.6e-5 | |
| `ln` | 189.5 | 194.8 | 2.4e-5 | |
| `log2` | 207.5 | 215.7 | 6e-6 | |
| `log10` | 213.5 | 225.7 | 3.2e-5 | |
| `sin` | 298.0 | 237.3 | 0 | 1.6e-2 |
| `cos` | 237.7 | 238.5 | 0 | 1.6e-2 |
| `tan` | 427.4 | 337.3 | 0 | 1.6e-2 |
| `sin_cos` | 397.4 | 356.3 | 0 | 1.6e-2 |
| `atan` | 195.8 | 206.0 | 0 | 5.6e-4 |
| `atan2` | 294.1 | 305.4 | 0 | 5.8e-4 |
| `asin` | 308.7 | 317.0 | 0 | 5.2e-4 |
| `acos` | 278.4 | 270.0 | 0 | 7.7e-3 |
| `hypot` | 188.0 | 179.0 | 0 | |
| `cbrt` | 123.3 | 114.4 | 0 | 2.0e-3 |
| `powf` | 443.5 | 466.2 | 1.4e-4 | |
| `powi` | 351.3 | 367.7 | 0 | |

- **Where the added instructions go.** The test itself costs 3 to 6 operations. Most of the rest
  is the low word, which the old `.0` results let the compiler drop.
- **Fallback costs.** About 4.7k instructions for `ln`, and 62.6k for `powf` (8.7 per call
  amortised).
- **Rewrites.** `sin`, `tan`, `sin_cos`, `acos`, `cbrt` and `hypot` got faster from the
  restructuring around the tests: the fast paths, and cold code moved out of line.

## 7. Pins

`tests/fmath_pin.rs` (24 hashes) and `crates/scx-algo/tests/xplat_pin.rs` (6 hashes) are
unchanged. None of their inputs was misrounded before, and every correct result keeps its bits.
`tests/fmath_worst.rs` pins 0 for every function.

## 8. Open

- The full CORE-MATH `.wc` files (millions of inputs, the ignored test) were not run on this
  branch. Only the vendored subsets were.
- The accurate phase's `C = 48` is a single constant with room to spare (the largest budget is
  under `2^16`). Per-function constants would shorten the 512-bit runs, which no measured input
  reaches.
