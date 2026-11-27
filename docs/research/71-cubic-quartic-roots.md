# 71 - Cubic and quartic roots by critical-point bracketing

Status: **built and measured; proposed for review.** Branch `poly`. It replaces the closed
forms of `scx_algo::polynomial::solve_cubic` and `solve_quartic`.

The question: the closed forms (Cardano, and Ferrari through the resolvent cubic) broke
their own claim on 20 of the 33 hard-family cases in `scx-algo/tests/polynomial_families.rs`.
They lost, merged or misplaced roots at near-double roots, clusters and large coefficient
ratios. The cause was absolute tolerances (`zero_tol`) on the discriminant and on Ferrari's
`u` and `v`. Can the solvers be correct on every family, scale exactly, and stay fast?

Short answers:

- **Correct.** Every family case now meets a stated contract (§1) against exact Sturm
  truth, the separated simple roots are correctly rounded, and `2^a p(2^b x)` gives the
  same roots scaled, bit for bit. The CLOSED table of pinned failures is gone: the test is
  a plain check over all 34 cases.
- **Slower than the pin and than the closed forms.** Per 1,024 calls (callgrind), the
  cubic costs 1.17x the old scx closed form and 3.9x the pin; the quartic 1.82x and 4.5x.
  §4 has the tables and where the instructions go.
- **Results change** where the pin was wrong: root counts at clusters and near-double
  roots, positions past the closed form's lost digits, and the order (ascending now). The
  difftest judges every disagreement against the exact truth under the contract (§5).

## 1. The contract

With `S(x) = Σ |c_i| |x|^i` and `η = 2^-90`, and the pseudo-zero radius
`ρ(r) = min_k (k! η S(r) / |p^(k)(r)|)^(1/k)`:

- every real root `r` of the given double coefficients, within the double range, has a
  returned root within `0.5 ulp(r) + ρ(r)`;
- roots closer together than their radii may be reported once;
- every returned root is within that bound of a real root, or is a point where the exact
  `|p(x)| <= η S(x)` (a complex pair too near the axis to tell from a double root);
- the roots are distinct and ascending, at most the degree of them.

For a simple root `ρ` is about `η S / |p'|`, far below half an ulp unless the root is
ill conditioned, so separated roots are correctly rounded. For a root of multiplicity `m`
the `k = m` term rules: a double root within `sqrt(2 η S / |p''|)`.

Case against: a contract in `η` is stronger than plain double arithmetic can deliver, so
it costs compensated evaluation at every root (§3.3). A contract in `u = 2^-53` (the
backward error a plain Horner solver reaches) would be cheaper. Case for: callers
(curve inversion, curve-surface intersection) use these roots as seeds and as answers;
`η` makes every simple root the correctly rounded one, which is what "truth over pin"
asks, and it is the bound compensated Horner certifies. A looser contract would let
roots near clusters move by `sqrt(u)`, which is the failure the families exposed.

## 2. The method

The roots are bracketed by the critical points: the derivative's roots, from the stable
quadratic formula for a cubic, and recursively from the cubic solver (without finishing
steps) for a quartic. Between two consecutive critical points `p` is monotone, so a
change of sign holds exactly one root; beyond the outermost, the sign at infinity decides.

### 2.1 Sign tests

Plain Horner first, with the bound `γ_(2n-1) S(x)`; when the value is inside it,
compensated Horner (error-free transformations, `scx_num::eft`), whose bound is
`2 (u |v| + γ² S)`. At a critical point, compensated Newton steps on `p'` first move onto
it, and the test then also allows for the distance still left: the derivative has a root
within `δ = min_j (C(m, j) |p'(x)| / |p'^(j)(x)/j!|)^(1/j)` (the classical bound from the
elementary symmetric functions of the reciprocal root distances), with each Taylor
coefficient taken at its least after its own error. `p` there differs from `p(x)` by at
most the Taylor terms at that distance. A value within the bound is zero.

Case against: the first build bounded `δ` by `|p'| / |p''|` alone. The difftest found
`(x + 3)^4 + 1e-13`, where `p''` vanishes at the critical point with `p'`, so that bound
was infinite and the solver reported a root at -3 that is not one (|p| = 1e-13, far above
`η S`). Case for the fix: the `j = m` term uses the leading coefficient, exact and never
zero, so `δ` is always finite; the case is now in the families.

### 2.2 Starts

Every start is from an exact form solved in plain arithmetic:

- a cubic with critical points `c1 < c2` and values `v1`, `v2` is exactly
  `v1 + (v2 - v1)(3 t^2 - 2 t^3)` at `x = c1 + (c2 - c1) t`, so its inner root solves
  `3 t^2 - 2 t^3 = q`, and its outer roots `3 u^2 + 2 u^3 = q` beyond either end;
- a monotone cubic is `v + t1 d + t3 d^3` about its inflection point, so its root is
  `d = k u` with `u + u^3 = q`;
- a quartic between two critical points is the cubic form plus `c4 (c2 - c1)^4 t^2 (1 - t)^2`
  (the difference vanishes with its derivative at both ends and has the quartic's leading
  coefficient);
- a quartic's outer roots start from the quadratic left once its inner roots are divided
  out (synthetic division), when exactly two are known; else from a Taylor bound beyond
  the outermost critical point (each term that pushes `p` towards zero would reach it
  alone within `(|p| / |t_k|)^(1/k)`; the root is nearer than the nearest).

Each start is clamped into its bracket. Case against: the scalar solves are extra code,
and a start is only a start (the bracket and the sign tests carry correctness). Case for:
the cubic went from about 4.7 plain Newton steps a root to 1.0, the quartic's inner roots
from about 4.3 to fewer; instructions fell 28% (cubic) and 33% (quartic) from the first
bracketing build.

### 2.3 Newton, then polish

Plain Newton steps inside the bracket (bisection when a step leaves it, doubling from the
finite end of an outer bracket) until the step is below `2^-30` of `x`; then compensated
Newton steps (`polish`) until the step is within two ulps or the compensated value is
within its bound. `polish` is `#[inline(never)]`: on baseline x86-64 `two_prod` calls
`fma` through the runtime, and inlined, those calls spilled the plain loop's registers.
Without `finish` (the quartic's critical points), a plain value inside its bound stops
the search when the root is well conditioned (`γ S / |p'|` below a plain step).

Case against: the split doubles the loop code. Case for: about 1.1 compensated steps a
root, against 4 to 5 when every step was compensated.

### 2.4 Scaling and far-apart roots

`balanced` scales `2^m p(2^j y)` by powers of two so that the lowest and highest nonzero
coefficients balance and the largest is in `[1, 2)`; the roots scale back by `2^j`. The
exponents are `floor(log2)` of each coefficient, subnormals included, so the scaled
polynomial is the same for every `2^a p(2^b x)`, which the families test bit for bit. The
end coefficients come out normal and exact; an interior coefficient may round into the
subnormals, which moves `p` by at most `2^-1075 |y|^i`, below `2^-100 S(y)` whenever both
ends are at least `2^-975`.

When an end is far below the largest coefficient, the roots fall into groups far apart in
size: if the Newton polygon (the upper hull of `(i, log2 |c_i|)`) turns by 128 bits or
more at `k`, `p` has `k` roots near those of `c_0 + ... + c_k x^k` and the rest near those
of `c_k + ... + c_n x^(n - k)`, and at the roots of either part the other part's terms are
below `2^-120` of its `S`. Each part is solved alone (degree one by a division, degree two
by the same bracketing about its one critical point) and the roots are merged. The split
is looked for only when an end of the balanced polynomial is below `2^-60` of the largest,
which every split needs. A root past the double range (`x^2 = 2^2098`) is dropped.

Case against: two more code paths (the split and the subnormal scaling) for inputs no
geometric caller builds, `[-2.5, -1.8e308, 2e-5, 1234567]` or every coefficient the least
subnormal. Case for: the difftest's corpus has them, and without the split the scaled
polynomial either overflowed in its products or could not be scaled at all, so roots were
lost or invented: 249 difftest mismatches (90 cubic, 159 quartic) before this, 0 after.
The check is one compare on the common path.

### 2.5 Rejected

- **Dekker's split for `two_prod` in `scx_num::eft`** (to avoid the `fma` call on baseline
  x86-64): no instruction gain in this solver; reverted, `eft` unchanged.
- **Every Newton step compensated:** correct but 2.5x the instructions of the split.
- **Keeping the closed forms with tolerances scaled to the polynomial:** the closed forms
  lose digits by cancellation in `acos` and `cbrt` arguments whatever the threshold, so
  clustered roots would still need a bracketing fallback; one method is simpler.

## 3. Tests

- `tests/polynomial_families.rs`: 34 cubic and quartic cases (Wilkinson, scaled and
  geometric; Mignotte; Chebyshev; clusters; near-double roots to `2^-40`; exact multiple
  roots; complex pairs near the axis; research 53's quartic; `(x + 3)^4 + 2^-43`) against
  Sturm truth: `closed_forms_meet_the_contract`, `separated_roots_are_correctly_rounded`,
  `closed_forms_scale_exactly`. Before: 20 of 33 failed (pinned in CLOSED); after: 0, and
  CLOSED is removed.
- `tests/xplat_pin.rs`: the roots hash is unchanged by the starts (same converged bits) and
  identical on x86-64 and wasm32.
- The unit test `extreme_coefficients`: every coefficient the least subnormal, roots split
  far apart (one subnormal, two near `±1.2e151`), `x^4 = 2^1074`, and a pair of roots past
  the double range dropped.
- The unit tests and `properties.rs` keep passing.

## 4. Cost

Instructions per batch of 1,024 calls (`algo_ic`, callgrind):

| case | old scx (closed form, polished) | pin | this branch | / old | / pin |
|---|---|---|---|---|---|
| cubic | 1,512,682 | 458,538 | 1,772,776 | 1.17 | 3.87 |
| cubic edge | 209,556 | 104,954 | 261,577 | 1.25 | 2.49 |
| quartic | 1,893,421 | 761,284 | 3,450,960 | 1.82 | 4.53 |
| quartic edge | 891,862 | 341,194 | 1,311,960 | 1.47 | 3.85 |

The split and the subnormal scaling (§2.4) cost 1.1% (cubic), 0.8% (cubic edge), 1.2%
(quartic) and 0.8% (quartic edge) of the build before them: the exponents are now taken
for subnormals too, and the precondition is tested. Their first form cost 6.3% on the
cubic; the exponent fields are now added in one pass when every input and result is
normal, the roots scale back by one multiplication when their size is known to keep them
normal, and the general paths are cold.

Wall time per batch of 1,024 calls (criterion, pinned to one core, two alternating runs
of each build; the range covers both):

| case | old scx | pin | this branch | / old | / pin |
|---|---|---|---|---|---|
| cubic | 191-193 us | 66-70 us | 234-242 us | 1.24 | 3.5 |
| cubic edge | 21.0-21.7 us | 9.4-9.8 us | 20.6-21.3 us | 0.98 | 2.2 |
| quartic | 234-235 us | 112-115 us | 505-513 us | 2.17 | 4.5 |
| quartic edge | 99-111 us | 44-46 us | 139-141 us | 1.33 | 3.2 |

The quartic's time grew more than its instructions (2.17x against 1.82x); not yet
profiled for why (candidates: the runtime `fma` calls, divisions in the bracket steps,
mispredicted branches between the plain and compensated paths).

Where the quartic's instructions go (callgrind inclusive, per batch of 1,024 cubics and
1,024 quartics, functions kept out of line to see them): the derivative's cubic about
0.8M, plain Newton about 0.9M, compensated polish 0.9M (of which compensated Horner 0.7M),
the sign tests 0.3M. Open: fewer plain steps for a quartic's outer roots when the inner
roots do not leave a quadratic (about 5 a root), and the compensated Horner's `fma` calls
on baseline x86-64 (an `x86-64-v3` build inlines them; research 67).

## 5. Pin and callers

Difftest: the three pin defects `poly_cubic`, `poly_quartic` and `poly_from_roots` now
check scx's roots against the contract and the exact truth, whatever the pin returns; a
quadratic (the degree dropped below the tolerance) must still match the pin's to the bit.
Counts after the change (release build): `poly_cubic` 3,121 inputs, 1,281 exact, 789
within tolerance, 1,051 judged against the truth, 0 mismatches; `poly_quartic` 3,121
inputs, 1,272 exact, 241 within tolerance, 1,608 judged against the truth, 0 mismatches;
`poly_from_roots` 106 inputs, 15 exact, 91 judged against the truth, 0 mismatches;
`poly_quadratic` 3,131 exact; 0 nondeterministic throughout. The curve and inversion
tests that call the solvers (`curve_surface_intersect`, `curve_*_invert*`,
`oblique_cylinder_pcurve` and the rest of `t_curve`) are 0 mismatches. The exact truth
is slow: the three pin-defect tests take about 7 minutes together in release.

Callers: `scx-curve`'s curve inversion (the quartic fallback) and curve-surface
intersection keep passing. What they see: roots ascending (the closed form's order was
not), a root count that is right at clusters and near-double roots (the pin merged or
dropped them), and a coefficient that is not finite gives no roots instead of NaNs.

## 6. Speed

Branch `pf`, 11-27: make the solvers faster without leaving the contract (§1), and change
no result unless the change is proven equally or more correct.

Instructions per batch of 1,024 calls (`algo_ic`, callgrind), and wall time (criterion,
pinned to one core, two alternating rounds of each build; the range covers both):

| case | before | after | change | pin | after / pin |
|---|---|---|---|---|---|
| cubic | 1,772,776 | 1,696,725 | -4.3% | 458,538 | 3.70 |
| cubic edge | 261,577 | 262,165 | +0.2% | 104,954 | 2.50 |
| quartic | 3,450,960 | 3,513,126 | +1.8% | 761,284 | 4.61 |
| quartic edge | 1,310,931 | 1,231,221 | -6.1% | 341,194 | 3.61 |

| case | before | after | pin | after / pin |
|---|---|---|---|---|
| cubic | 234-248 us | 209-217 us | 66-72 us | 3.1 |
| cubic edge | 20.5-21.3 us | 21.4-23.5 us | 9.5-9.9 us | 2.3 |
| quartic | 525-606 us | 483-492 us | 112-131 us | 4.3 |
| quartic edge | 147-167 us | 120-125 us | 45-46 us | 2.7 |

The quartic's instructions rose while its time fell: most of the rise is the contract
fixes below (more starts that are correct), and the time is the compensated Horner's new
shape. The cubic edge time is within the noise of the two rounds (its instructions are
unchanged).

What changed:

- **Compensated Horner in three passes.** The plain Horner pass keeps each step's
  multiplier and `two_sum` error; a second pass takes the `two_prod` errors; a third
  sums the error polynomial. On baseline x86-64 `two_prod` calls `fma` through the
  runtime and every xmm register is caller saved, so in one pass the Horner chain's live
  values spilled around each call; in its own pass the call has nothing live but the
  error array. Same operations in the same order on each value, so the same bits.
  The single-pass form is 1.5% fewer instructions on the cubic but slower in time
  (218-226 us cubic, 520-555 us quartic, against 209-217 and 483-492).
- **Lazy Taylor form.** A sample's Taylor coefficients (for the outer start and the
  monotone start) are computed only when a start needs them, not at every sign test.
- **Inner starts measured from the nearer end.** The cubic model's `t` is taken from the
  end nearer the root (`smooth_inner` returns `t` and which end), so a root near a
  critical point keeps its digits; the quartic model gets its ends swapped to match.
- **Outer starts from the same model.** Beyond a critical point the outer root solves
  the model's quadratic in closed form (`outer_beyond`), from the inner model's `t`,
  falling back to `smooth_outer`.

Contract fixes found on the way (each a miss or a returned non-root on `main`; the scans
below check every case against exact Sturm counts and sign changes):

- **Overshoot near a critical point.** An outer search started short of the root, near a
  critical point where `p'` is small, took one giant Newton step past the root and then
  crawled back. The first long step that overshoots the bracket's finite end is now
  clamped to the Taylor bound beyond the root (`outer_start`), when that bound lies
  between `x` and the step. The Taylor bound is not always past the root (its terms can
  have mixed signs), so it only shortens a step, never replaces the bracket.
- **Groups below the split.** Roots in groups up to `2^126` apart (below the 128-bit
  split of §2.4) make plain Newton from a far start move by a factor `1 - 1/k` a step
  (`k` roots in the group, at most 4), at least 0.41 bits a step; over the double range
  (`2^2098`) that is up to about 5,100 steps, and the old shared limit of 100 ran out,
  leaving a non-root or a missed root. The plain loop now has its own limit
  `PLAIN_STEPS = 8192`; the polish limit stays 100. No measured cost (the same counts
  with 100). Unit test `roots_in_groups_far_apart` (one cubic, three quartics, each
  verified exactly).
- **Quartic critical points near zero.** The quartic's derivative roots near zero lost
  their digits when the inner model start was measured from the far end (fixed by the
  nearer-end start above).

Scans (20,000 random cubics and quartics at each of two coefficient-spread classes,
against exact truth; zero-coefficient and `tol = 1e-12` inputs left out, the latter
dropping to the pin's quadratic): misses or non-roots before 34, 10, 111, 7; after 0 in
every class. Over the full 4,000,000-input local corpus against `main`'s outputs, 2,908
results differ: 1,699 where `main` broke the contract and this branch meets it, 0 the
other way, 0 where both break it; and 22 where both meet it, in each of which this
branch's root is the closer to the exact root (0.09 to 0.46 ulp against 1.87 to 1,091
ulp). The rest are inputs outside the contract (`tol = 1e-12`: 1,012 now meet it, 90
fail on both as before; zero coefficients: 77 now meet it).

Tried and dropped:

- **Dekker's split for the `fma`** (an exact product without the runtime call, also as a
  filtered sign test): the same bits, but more instructions and no faster; reverted.
- **Aitken extrapolation across the slow groups:** it lands at a group's centre, where
  `p'` is near zero, and the next step jumps far; 11 cases regressed. The step limit
  replaced it.
- **Stopping polish early with a certified bound:** the plain bound `γ S` never
  certifies polish's `2 (u |v| + γ² S)` stop, so a cheaper stop changes bits and is not
  proven better; not done.
- **Research 65 §2.5's trig-free cubic:** not built. It would only give starts, and the
  starts already take about one plain Newton step a root (§2.2); the cost is in polish and
  the critical points, which it does not touch.

Remaining gap: the quartic is 4.3x the pin in time and the cubic 3.1x. The cost is the
contract: compensated evaluation at every root (the pin's closed forms stop at plain
double), the critical points of a quartic (a cubic solve inside), and the runtime `fma`
on baseline x86-64 (an `x86-64-v3` build inlines it; research 67).
