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
| `poly` | Exact polynomials over the rationals. Every real root is certified in a rational interval with its multiplicity (Yun's square-free factorisation, Sturm sequences, bisection to `2^-110` relative). |
| `pin_uv` | The pin's uv layout of the analytic surfaces (D13), in closed form and double-double (§2). |
| `curve_surface` | T2. Every intersection of a line, circle or ellipse with a plane, cylinder, cone, sphere or ring torus. Each hit has its multiplicity, its contact (crossing or tangent), its curve parameter with a radius, its STEP uv and its pin uv. Curves that lie on the surface are reported as such (§3). |
| `ssi` | T5. The analytic sections: plane-plane; a plane against a cylinder, cone or sphere; sphere-sphere; coaxial pairs (tori among them); a plane across a torus's axis. Each answer is the exact curve kind (point, line, circle, ellipse, hyperbola branch) with its parameters, its contact, and samples certified on both surfaces. It also gives WR-680's exact section of the pin's NURBS cylinder (§4). |

`gen` gained seeded generators: exact axis frames, revolved surfaces, frames moved along an
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
- The plane's uv is taken as the STEP uv. This is not checked against the pin (§7).

The self-tests (`tests/pin_uv.rs`) cover three things:

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
  `DegenerateSurface`. `pin_uv` is `None` only on the cone's far nappe.
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
  invent one (§7).
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

These are solved at the band's two ends and at `k`. The answer is `Undecided` when the three
answers' shapes differ: whether the surfaces coincide, or the rings' number, kinds or contacts.

This is a sampled decision, not a proved one. A shape change inside the band and back out again
would be missed. With a polynomial in `h` whose coefficients are at most quadratic in `k`, that
needs two roots in a `2^-96` band (JUDGED; for ruling, §7).

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

`tests/ssi.rs` has 13 tests, plus the unit test above.

- **Closed forms:**
  - two planes meet in a line, coincide, or do not meet;
  - a sphere cut at height `h` is a circle of radius `sqrt(r^2 - h^2)`;
  - an oblique cylinder section is an ellipse with Apollonius's semi-axes;
  - planes along a cylinder give two lines, one tangent line, or none;
  - cone sections follow the plane's tilt: circle, ellipse, hyperbola, crossing lines, apex
    point;
  - two spheres meet on their radical plane;
  - coaxial quadrics meet in circles where their meridians cross;
  - tori meet planes, quadrics and tori across their axis.
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
| `pin_curve_surface_against_t2` | 15 | 10 | 5 | 0 | 0 | 0.41 to 0.51 s |
| `pin_tangent_lines_against_t2` | 5 | 0 | 4 | 1 | 0 | 2.8 to 3.5 s |
| `pin_plane_sections_against_t5` | 12 | 11 | 1 | 0 | 0 | 1.1 to 1.3 s |
| `pin_revolved_pairs_against_t5` | 12 | 11 | 1 | 0 | 0 | 3.1 to 3.7 s |
| `pin_tangent_sections_against_t5` | 6 | 6 | 0 | 0 | 0 | 0.03 to 0.08 s |

The five run in 3.6 s together. The machine's load was about 9.

**Why two of the tests are slow.** The time is in the pin's debug-built intersectors, not in the
truths: each truth takes under 2 ms. The slow cases are these:

- One tangent line takes up to 0.87 s inside `intersect_curve_surface`.
- One coaxial cone pair takes 0.83 s inside `intersect_surfaces_within`, and then about 1.4 s
  evaluating its B-spline answer.

Two cuts were made to reach these times:

- The first draft had 60 curve-surface inputs and 24 in each SSI test. The counts were cut, and
  the tangent lines moved to their own test.
- `cargo xtask fast` does not run the difftest's tests, so the 1 s budget there is not at risk.
  These two tests are still above it, while keeping every surface kind covered (for ruling,
  §7).

**What the pin returns, as notes, not defects:**

- "ellipse" for sphere circles;
- B-spline curves for cone hyperbolas;
- two ellipse curves for some single-ellipse cone sections;
- for a near-tangent line that the truth says misses by rounding, a "touch".

### 5.3 Pin defects

The pin's intersectors are not ported in scx, so no `pin_defects.rs` entry is added. Defects 1
and 2 were found in the 60-input draft of `pin_curve_surface_against_t2` (seed `0x7264`). Defect
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
"on the surface".

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

## 6. Proposed 04 rows

These are proposed only. No 04 row is edited.

| Date | Ruling | Evidence |
|---|---|---|
| 11-27 | (Proposed) Curved-surface SSI truths are closed form only for frames sharing axes bit for bit (spheres anywhere, coaxial pairs); every other pair is `NoClosedForm` | A shared `L` up to a shift keeps every predicate exact; rotated axes would need rounding the truth cannot certify (64 §4.1) |
| 11-27 | (Proposed) A cone predicate inside the slope's `2^-96` band is `Undecided`, a typed cause, not a guess | `tan a` is transcendental; no double input is exactly tangent unless for every slope (64 §4.3) |
| 11-27 | (Proposed) WR-680's exactness is on the pin's NURBS cylinder, whose middle weight is `cos(pi/4)` rounded | The section is exact on that surface; no double weight makes a quadratic arc exactly circular (64 §4.5) |

## 7. For ruling

- **Shared axes for curved pairs** (§4.1). The alternative is certified rounding on rotated
  frames.
- **The cone band:** `Undecided` for plane sections, and the sampled three-point shape decision
  for coaxial pairs with a cone (§4.3, §4.4).
- **Folding onto `quadric::Layout`/`Implicit`.** `pin_uv` and `curve_surface`'s implicit forms
  overlap truth-n's `quadric::Layout`/`Implicit`. One of them should be folded onto the other
  once both land.
- **The misaligned WR-680 case** (§4.5). A plane section of a cylinder not aligned with the arcs
  is exact on the pin's surface only, never on the true cylinder.
- **The plane's uv** is not checked against the pin (§2).
- **The parabola** is unreachable from double inputs and returns `NoClosedForm` (§4.2).
- **Plane-torus** sections are supported only across the axis. Other plane-torus sections are
  quartic curves, with no closed form here.
- **The pin's overlap semantics.** Its fixed `0.001` overlap tolerance is behind defects 1 and
  2 (§5.3). Should scx's intersector keep any overlap test at all?
- **The tangent lines and revolved pairs tests take 3 to 4 s** in a debug build, all of it in
  the pin. Two options: accept this, since the difftest is not in `xtask fast`, or keep only
  three surface kinds for the tangent lines.
