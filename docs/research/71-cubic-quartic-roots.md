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

Wall time per batch (criterion, pinned to one core): WALL_TABLE

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
DIFFTEST_COUNTS

Callers: `scx-curve`'s curve inversion (the quartic fallback) and curve-surface
intersection keep passing. What they see: roots ascending (the closed form's order was
not), a root count that is right at clusters and near-double roots (the pin merged or
dropped them), and a coefficient that is not finite gives no roots instead of NaNs.
