# 64 - Intersection truths: curve-surface (T2) and analytic SSI (T5)

Written for solidcorex, 2026-11-27. Research 63 §8 (as ruled in §12) lists two testkit truths
for intersections:

- **T2**, a line or conic against the analytic surfaces;
- **T5**, the analytic surface-surface sections.

This note records how they are built, how they check themselves, what they cost, and what
the pin's intersectors do against them. The labels are 59's:

- READ: read from the code or the docs.
- MEASURED: run here; the command is given.
- JUDGED: a design judgement.

## 1. What was added

In `crates/scx-testkit`, on the testkit's own types only:

- `analytic`'s `Frame`, its kinds, `Dd`, `Rat` and `exact::Int`.
- No `scx-curve`, `scx-surface` or `scx-ssi` type appears, so later API work cannot invalidate a
  truth.
- No dependency is new.

The new modules:

| Module | What it is |
|---|---|
| `poly` | Exact polynomials over the rationals, one module with `project`'s (T1, T3): one `Poly` over `Rat` and one `Root` (a `bound` interval and a multiplicity). Every real root is certified with its multiplicity (Yun's square-free factorisation, Sturm sequences, refinement to a given width); these truths ask for `2^-110`. |
| `curve_surface` | T2. Every intersection of a line, circle or ellipse with a plane, cylinder, cone, sphere or ring torus. Each hit has its multiplicity, its contact (crossing or tangent), its curve parameter with a radius, its STEP uv and its pin uv. Curves that lie on the surface are reported as such (§3). |
| `ssi` | T5. The analytic sections: plane-plane; a plane against a cylinder, cone or sphere; sphere-sphere; coaxial pairs (tori among them); a plane across a torus's axis. Each answer is the exact curve kind (point, line, circle, ellipse, hyperbola branch) with its parameters, its contact, and samples certified on both surfaces. It also gives WR-680's exact section of the pin's NURBS cylinder (§4). |

The pin's uv layout (D13) is `quadric::Implicit::pin_layout`, on truth-n's `quadric::Layout`
(§2), and the surfaces' exact implicit forms are `quadric`'s, so each concept has one truth
(ruling 11-27; `pin_uv` and `curve_surface`'s own forms were folded onto it in the merge).

`sample` gained seeded generators: exact axis frames, revolved surfaces, frames moved along an
axis, and planes near an axis. `curve_surface::distance` is a point's first-order distance from a
surface: the exact implicit value over the double-double gradient's length.

In `crates/scx-difftest`, `tests/diff/pin_geom_truths.rs` is the dev-only check of the pin
against these truths (§5).

## 2. D13: the pin's uv (READ, then tested)

Read from the pin's canonical constructors (`swap_uv` and `inside_out` off, a full turn):

- Every angle direction is four standard quarter arcs from the frame's `x`. The end weights are
  1 and the middle weight is `cos(pi/4)`, rounded. There is one knot span per arc.
- The sphere's latitude is two such arcs from the south pole.
- On a quarter arc at local parameter `s`, the angle `phi` from the arc's start satisfies
  `tan(phi/2) = s / (s + sqrt2 (1 - s))`. The inverse is `s = sqrt2 t / (1 + (sqrt2 - 1) t)`,
  with `t = tan(phi/2)`.
- The knots differ by surface:
  - cylinder u: `k pi` over `[0, 4 pi]`, each rounded once, as the pin rounds them;
  - cone, sphere and torus angles: `k/4` over `[0, 1]`;
  - sphere latitude: `k/2`.
- The cylinder's and cone's v is the height, which is the STEP v.
- The plane's uv is the STEP uv. `pin_quadric`'s `pin_plane` checks the pin's NURB-to-STEP map
  of each plane against this identity layout, to `1e-11` (MEASURED: no failures).

The layout is `quadric::Implicit::pin_layout` (`Param::arcs` and `Param::Linear`). The
self-tests (`tests/pin_layout.rs`) cover three things:

- The quarter-arc map equals the rational quadratic arc itself, evaluated exactly at rational
  `s`.
- The two maps invert each other.
- The knots are the quarter turns.

The difftest's agreement of pin points with truth points is a second, independent check of the
layout on the cylinder and cone.

## 3. T2: curve-surface

### 3.1 Method

The method is exact, with one rounding: the cone's slope.

- Each surface is the image of its canonical form under its frame as given. The frame
  coordinates `L = M^-1 (P - C)` are exact rationals.
- Each surface is therefore exactly the zero set of a polynomial in `L`:
  - plane `L_z`;
  - cylinder `L_x^2 + L_y^2 - r^2`;
  - cone `L_x^2 + L_y^2 - (r + k L_z)^2`, which has both nappes;
  - sphere `|L|^2 - r^2`;
  - ring torus `(|L|^2 + R^2 - r^2)^2 - 4R^2(L_x^2 + L_y^2)`.
- The curves are parameterised as follows:
  - A line is linear in `t`.
  - A conic is a rational quadratic in `w = tan(t/2)`. The half turn `t = pi` is the root at
    infinity, read from the degree's drop.
- The substituted equation is an exact polynomial in the curve parameter. Its degree:
  - a line: 1 or 2 against a quadric, 4 against the torus;
  - a conic: 2 or 4 against a quadric, 8 against the torus.
- Roots come from `poly::real_roots`:
  - multiplicity 1 is a crossing;
  - multiplicity 2 or more is a tangency;
  - the zero polynomial means the curve lies on the surface.
- Parameters and uv are computed in double-double from the root's interval.
- The cone's `k = tan a` is held as the rational value of its double-double. This is the one
  rounding, within `2^-104` relative.

### 3.2 Self-tests

`tests/curve_surface.rs` has 15 tests and `tests/poly.rs` has 3.

- **Closed forms:**
  - a line through a sphere's centre meets it at `±r/d`;
  - the torus's equator line meets all four tube walls;
  - a line over the torus's top touches it twice;
  - tangent lines to each quadric give double roots;
  - a circle inscribed against a cylinder wall touches it four times;
  - a conic's half turn is found as the root at infinity;
  - curves that lie on their surfaces are reported as such.
- **Seeded, through constructed points:**
  - lines through two surface points meet the surface there;
  - conics through a surface point meet it there.
- **Independent methods:**
  - crossings are exactly the sign changes of the exact implicit value, and tangencies are not;
  - every hit is at zero distance by `analytic`'s closest point;
  - the first-order distance equals the closed form off a plane and a sphere;
  - a near-tangent line splits into two close roots or none, never one.
- **Typed causes:** bad inputs give `NonFinite`, `DegenerateFrame`, `DegenerateCurve` and
  `DegenerateSurface`. The pin uv is `None` only on the cone's far nappe.
- **`poly`:**
  - known rational roots come back exactly, with their multiplicities;
  - irrational roots are isolated and refined;
  - multiplicities agree with a rational factorisation.

### 3.3 Cost (MEASURED)

The command is `cargo test -p scx-testkit --test curve_surface`, with the crate optimised as in 59.
Exact Sturm sequences over big rationals were the first cost. Two changes reduced it:

- clearing to integer polynomials before Yun and Sturm;
- bisecting on the square-free factor's sign once a root is isolated.

| Case | Before | After |
|---|---|---|
| line × plane (linear) | 1.6 ms | 0.9 ms |
| line or conic × quadric (quadratic) | 13 to 33 ms | 1.2 to 1.6 ms |
| conic × quadric (quartic) | 39 to 186 ms | 3 to 5 ms |
| line × torus | 127 ms | 3 ms |
| conic × torus (degree 8) | 626 ms | about 70 ms |

The whole `curve_surface` file runs in 0.71 s and `poly` in 0.24 s.

## 4. T5: analytic SSI

### 4.1 Model

A surface is again the image of its canonical form under its frame as given.

**A plane section.** A plane is linear in any frame's `L`, so a plane section is a conic in
the plane's own two coordinates.

- It is decided exactly, in rationals, from the conic's invariants: `delta = AC - B^2`, the 3×3
  determinant, and the signs of `A` and `C`.
- It is mapped back to world coordinates by `M`. An affine map keeps the kind, so an ellipse
  stays an ellipse.
- Every parameter is exact up to one square root, which is taken last, in double-double.
- The kind is the kind in frame coordinates. A circle there is a world circle when the frame is
  orthonormal.

**Two curved surfaces.** They are a closed-form pair only when they share their axes, with `X`,
`Y` and `Z` equal bit for bit. Their `L` then differ by a shift.

- **Spheres:** the radical plane across one of them.
- **Coaxial pairs:** the shift is along `Z`. Their meridians, each `rho^2 = P(h)` or the torus's
  quartic, meet at circle heights. These come from an exact polynomial in the axial coordinate.
  A common root of the two meridians is a point on the axis, decided exactly by their gcd.

A plane meets a torus here only across the axis. Any other pair is `Error::NoClosedForm`.

### 4.2 Plane-quadric kinds

| Section | Reached by | Contact |
|---|---|---|
| ellipse or circle | `delta > 0`, `Det` ≠ 0 with the right sign | transversal |
| point | `delta > 0`, `Det = 0` (a tangent plane on a sphere; a cone's apex) | tangent |
| hyperbola, as two branches | `delta < 0`, `Det` ≠ 0 | transversal |
| two crossing lines | `delta < 0`, `Det = 0` (a plane through a cone's apex) | transversal |
| two parallel lines, one line, none | rank one: a cylinder cut along its axis | transversal, tangent, none |
| parabola | `delta = 0`, `Det` ≠ 0 | `NoClosedForm` |

Two cases need a word:

- **The parabola.** It needs a plane exactly parallel to a cone's generator. With the slope
  transcendental, no double input reaches it. The truth returns `NoClosedForm` rather than
  invent one (accepted 11-27).
- **Plane-torus sections** are closed form only across the torus's axis (circles). Every other
  plane-torus section is a quartic curve with no closed form here, and is `NoClosedForm`. This
  is a limit of T5, accepted for now (11-27): a truth for them needs a certified quartic curve.
- **The rank-one case.** The cone's `delta` is never the zero polynomial in `k`, so rank one is
  the cylinder alone.

### 4.3 The cone's slope band

`k = tan a` of a double `a` is transcendental. A rational configuration is therefore exactly
tangent to the true cone only when it is tangent for every slope.

The truth holds `k` within `2^-96` relative of `tan a`: the double-double rounding, with room. It
decides each predicate as a polynomial in `k`:

- zero when it is the zero polynomial;
- otherwise its sign at `k`, when no root lies within the band;
- otherwise `Error::Undecided`.

The unit test `the_slope_band_decides_or_refuses` checks all three outcomes.

### 4.4 Coaxial pairs with a cone

These are solved at `k` and then proved for every slope in the band, else `Undecided` (ruling
11-27: a truth must be proved). The answer's shape (whether the surfaces coincide, and the
rings' number, kinds and contacts) is fixed by the signs of a few polynomials in the slopes:

- the section polynomial `g(h)`'s coefficients, which fix its degree;
- for a quadratic `g`, its discriminant, which fixes the number of roots and their contact;
- `g` at each cone's apex (`k^d g(h_0 - r / k)`), which fixes which root is a point on the
  axis.

When each keeps its sign over the band, the roots move continuously without meeting, so the
shape at `k` is the shape at the true slope.

- **Quadric pairs** (`g` at most quadratic). Each slope is `k_i + e_i`. The polynomials are
  expanded in `e`, and a sign is proved when the constant term's size exceeds the sum of the
  other terms' sizes at the band's corner, or when the polynomial is zero. Two cones with the
  same slope share one `e`: the same cone moved along its axis has parallel generators for
  every slope, and its one circle is proved.
- **A cone against a torus** (`g` a quartic in `h`). There is one slope. The quartic's
  Sylvester resultant with its derivative (nonzero: its degree holds and it stays square-free)
  and its apex value are polynomials in `k` of degree at most 28 and 8. They are interpolated
  exactly at 30 nodes, and their signs are decided by §4.3's band test. A quartic with a
  double root for every slope is `Undecided`.

Unit test `the_band_bound_decides_or_refuses` checks the bound; `tests/ssi.rs`'s
`coaxial_cones_are_proved_over_the_slope_band` checks a moved cone and a cone through a
torus's tube, and the seeded coaxial pairs all decide. The `ssi` file's time is unchanged
(0.65 s before and after, MEASURED).

### 4.5 WR-680

`ssi::cylinder_section` is the exact section of a plane through **the pin's NURBS cylinder**.
That cylinder is four rational quadratic quarter-arc spans, weights `(1, cos(pi/4) rounded, 1)`,
knots `k pi` rounded, and the height linear.

- On span `k`, the cylinder is `C + N(s)/W(s) + v Z`.
- The plane holds at `v = H(s)/W(s)`, so the section is the rational quadratic with points
  `C + r(a_i X + b_i Y) + c_i Z` and the arc's weights.
- That is exactly on the pin's surface, with uv curve `(u(s), H(s)/W(s))`: `u` linear, `v` a
  ratio of quadratics.

`wr680_the_oblique_cylinder_section_is_exact_on_the_pins_nurbs_cylinder` checks, in rationals:

- each span point is on the plane exactly;
- it equals the pin's NURBS cylinder at `(u(s), v(s))` exactly;
- it is within `1e-15` of the true radius, the weight's rounding;
- `u(s)` is the pin's u of its angle to `1e-14`.

The generic `ssi::intersect` of the same pair gives one transversal ellipse.

What stays exact, and what does not:

- The section is exact on the pin's cylinder.
- It is not exact on the true circular cylinder, because `cos(pi/4)` is rounded.
- Nor is any plane section of a cylinder whose rim is not aligned with the arcs. No double
  weight makes a rational quadratic exactly circular.

### 4.6 Self-tests

`tests/ssi.rs` has 14 tests, plus the two unit tests above.

- **Closed forms:**
  - two planes meet in a line, coincide, or do not meet;
  - a sphere cut at height `h` is a circle of radius `sqrt(r^2 - h^2)`;
  - an oblique cylinder section is an ellipse with Apollonius's semi-axes;
  - planes along a cylinder give two lines, one tangent line, or none;
  - cone sections follow the plane's tilt: circle, ellipse, hyperbola, crossing lines, apex
    point;
  - two spheres meet on their radical plane;
  - coaxial quadrics meet in circles where their meridians cross;
  - tori meet planes, quadrics and tori across their axis;
  - coaxial cones are proved over the slope band (§4.4).
- **An independent method:** for seeded plane sections, coaxial pairs and sphere pairs, three
  things hold:
  - every sample is on both surfaces, with first-order gap below `1e-24`;
  - each sample's STEP uv and pin uv map back to it;
  - T2's line through a sample, in a random direction, meets both surfaces there.
- **Near-degenerate planes stay certified:**
  - within `1e-9` of tangent to a sphere, the answer is the small circle of radius
    `sqrt(2 r 1e-9)`, or nothing;
  - within `1e-6` of a cylinder's axis, it is the long ellipse, certified to its size.
- **Typed causes:**
  - a tilted plane across a torus, or an off-axis or rotated-axes pair, is `NoClosedForm`;
  - a negative radius, a NaN and dependent plane axes are `Surface(..)` causes.
- **WR-680** (§4.5).

The whole `ssi` file runs in 0.44 s (MEASURED, `cargo test -p scx-testkit --test ssi`).

## 5. The pin against the truths (MEASURED)

### 5.1 The check

The command is
`cargo test -p scx-difftest --test diff pin_geom_truths -- --nocapture`.

Each pin surface is the truth's surface, bounded:

- a plane to `±3`;
- a cylinder to a height of 6 about its origin;
- a cone to length 2;
- spheres and tori whole.

Lines are segments of half-length 2.

The truth's points inside both windows, off the edges by `1e-4`, must be on the pin's answer, and
the pin's answer must be on the truth's. The bounds:

- **Exact:** within `1e-14`.
- **A crossing point:** `1e-9 / sin(theta)`, clamped to `[1e-9, 1e-6]`.
- **A tangent point:** `1e-6`.
- **SSI:** `intersect_surfaces_within(.., Some(1e-7))`, and the pin's curves must be within
  `1e-6`.

A pin point on both truth objects that the truth lacks counts as a mismatch only when all three
hold:

- it is transversal (`sin ≥ 1e-3`);
- it is farther than ten times its gaps over the sine from every truth point;
- it would be the truth's error.

A mismatch fails the test. A pin defect is reported, not failed.

### 5.2 Results

| Test | Inputs | Exact | Within tolerance | Pin defects | Mismatches | Time alone |
|---|---|---|---|---|---|---|
| `pin_curve_surface_against_t2` | 15 | 10 | 5 | 0 | 0 | 0.13 s |
| `pin_tangent_lines_against_t2` | 5 | 0 | 4 | 1 | 0 | 0.22 to 0.23 s |
| `pin_plane_sections_against_t5` | 12 | 11 | 1 | 0 | 0 | 1.5 to 1.7 s |
| `pin_revolved_pairs_against_t5` | 12 | 11 | 1 | 0 | 0 | 0.82 to 0.85 s |
| `pin_tangent_sections_against_t5` | 6 | 6 | 0 | 0 | 0 | 0.02 s |

Times are with the pin built at `opt-level = 2` (ruling 9, §7). The five run in 1.8 s together.
The machine's load was about 10.

**Test time.** In a debug-built pin the tangent lines took 2.8 to 3.5 s and the revolved pairs
3.1 to 3.7 s, all of it in the pin's intersectors (one tangent line 0.87 s in
`intersect_curve_surface`; one coaxial cone pair 0.83 s in `intersect_surfaces_within` and
1.4 s evaluating its B-spline answer). Each truth takes under 2 ms. The first draft's 60
curve-surface inputs and 24 per SSI test were cut, and the tangent lines moved to their own
test. With the pin at `opt-level = 2` both are under 1 s and no further cut is needed.

The plane sections stay at 1.5 to 1.7 s with the pin optimised, so their time is not in the
pin (JUDGED: in the debug-built checks of the pin's answers; not profiled, as `perf` is not
permitted on the machine). Left to a perf follow-up.

**What the pin returns, as notes, not defects:**

- "ellipse" for sphere circles;
- B-spline curves for cone hyperbolas;
- two ellipse curves for some single-ellipse cone sections;
- for a near-tangent line that the truth says misses by rounding, a "touch".

### 5.3 Pin defects

The pin's intersectors are not ported in scx, so no `pin_defects.rs` entry is added. Until they
are, the defects are `KNOWN_*` lists in `pin_geom_truths.rs`, as in the surveys: each test
fails when its defective inputs change. `KNOWN_TANGENT_LINES` is `[2]` (defect 3);
`KNOWN_CURVE_SURFACE` is empty, with defects 1 and 2 described on it. When scx ports these
intersectors, all three become `pin_defects.rs` entries (ruling 11-27). Defects 1 and 2 were
found in the 60-input draft of `pin_curve_surface_against_t2` (seed `0x7264`). Defect
3 is in the current `pin_tangent_lines_against_t2` (seed `0x7265`).

**1. A line across a thin cylinder is reported as an overlap.**

- Line: through `[-0.2691840532677434, 0.4154960181554018, -1.0918370876288692]`.
- Cylinder:
  - origin `[-0.41413966457768536, 0.7423670879778235, -0.7008960904732076]`;
  - x `[-0.2537688335925677, 0.9583868064415818, -0.13075208730943297]`;
  - y `[-0.9651720312185574, -0.25978309429105273, -0.030914302094816937]`;
  - z `[-0.0635950410842803, 0.11835317131058164, 0.9909329934915966]`;
  - r `0.3020011791372873`.
- Pin: one overlap, off the surface by up to `2.57e-4`.
- Truth: two crossings, at `[-0.26939786093188917, 0.3637094591439217, -1.4517395418783583]`
  and `[-0.2691840532677434, 0.4154960181554011, -1.0918370876288743]`.
- Error: an overlap `2.57e-4` off the surface, in place of two points.

**2. A line tangent to a sphere is reported as an overlap.**

- Line: through `[1.39199638198125, -1.4720131312364129, -0.8082214959603395]`.
- Sphere: origin `[0.6506374909015007, -0.7740675028341935, -0.8922296467289534]`, r
  `1.0216645608928239`.
- Pin: one overlap, off the surface by up to `6.62e-4`.
- Truth: no points; the rounded line misses.
- Error: a curve segment `6.62e-4` off the surface.

Defects 1 and 2 have the same likely cause (JUDGED). The overlaps are within the pin's fixed
`0.001` curve-on-surface solve tolerance, so a near-tangent or shallow crossing is accepted as
"on the surface". scx keeps no fixed-tolerance overlap (ruling 11-27): an on-surface or overlap
answer is given only when it is exact or certified, and a tolerance-based overlap is one a
caller asks for with an explicit tolerance.

**3. A near-tangent line to a cone is reported as no intersection.**

- Line: through `[-0.3847209854774346, -0.7063326847433884, -0.49177426208596553]`.
- Cone:
  - origin `[-0.03069952128479425, 0.7368343293022122, -0.5169312845836505]`;
  - x `[0.013990326642423907, -0.7163201619656193, 0.6976314903457177]`;
  - y `[-0.8889211789917641, -0.3283807735567447, -0.31935122529617177]`;
  - z `[0.4578464898853442, -0.6156715789443451, -0.6413463171952004]`;
  - r `0.6360280772373341`;
  - semi-angle `0.7557757676454216`.
- Pin: an error, `intersect_curve_surface: no intersection was found`.
- Truth: two crossings, `1.1e-8` apart:
  `[-0.3847209909200621, -0.7063327209676573, -0.49177427449821987]` and
  `[-0.3847209800348097, -0.7063326485191367, -0.49177424967371713]`.
- Error: a contact within `1e-8` is missed entirely; at the `1e-6` tangent bound the pin
  should report a point.

## 6. 04 rows

The rows proposed here were ruled 11-27 by the coordinator and are in 04 §3, for Brett's
review.

## 7. Rulings (11-27, coordinator, for Brett's review)

1. **Shared axes for curved pairs.** Accepted: a closed form only when the axes are equal bit
   for bit; `NoClosedForm` otherwise.
2. **The cone band.** The `2^-96` band giving `Undecided` is accepted. The sampled three-point
   decision for coaxial pairs was not accepted; it is now proved, or `Undecided` (§4.4).
3. **Folding onto `quadric`.** Done in the merge: `pin_uv` and `curve_surface`'s implicit
   forms are `quadric::Layout` and `quadric::Implicit` (with a crate-private exact form), and
   `poly` is one module (§1).
4. **The misaligned WR-680 case** (§4.5). Accepted, as documented.
5. **The plane's uv.** Checked against the pin in `pin_quadric` (§2).
6. **The parabola** returns `NoClosedForm` (§4.2). Accepted.
7. **Plane-torus** sections across the axis only. Accepted for now; the limit is in §4.2.
8. **Overlaps.** scx keeps no fixed-tolerance overlap (§5.3); the defects are `KNOWN_*` lists
   until the intersectors are ported.
9. **Test time.** The pin and its record crate build at `opt-level = 2` in dev and test
   (`Cargo.toml`). Results do not change (Rust never reassociates floats): the full difftest
   is 373 tests, 0 failures, 0 mismatches. Times above (§5.2). The rebuild of the pin and its
   dependents at `opt-level = 2` took 367 s at load 10 (MEASURED; no opt-0 cold build was
   timed for comparison). The full difftest took 280 s wall against 237 s before, both at
   load about 10 with other builds sharing the machine, so not a like-for-like comparison.
