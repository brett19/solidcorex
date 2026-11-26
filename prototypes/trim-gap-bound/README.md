# trim-gap-bound: prototype for research report 55

A study prototype of certified upper bounds on the trim-to-edge gap
`max_t |S(c(t)) - C(t)|` on offset surfaces and on approximated (procedural-blend) surfaces.
It is std-only apart from `scx-num` (taken by path), whose outward-rounded intervals carry
every enclosure. No solidcore code is in it: solidcore was read for its bounds and measured
separately (see `docs/research/55-certified-trim-gap.md`).

## What is here

- `src/counted.rs`: `Ci`, an interval that counts its operations, so each method's cost is
  given in interval operations as well as time.
- `src/jet.rs`: `Jet<T>`, a truncated Taylor jet (value and first three derivatives; the
  third is switched on only where a centred form needs it). Jets nest (`Jet<Jet<Ci>>`), which
  gives the mixed partials of a surface over a parameter box.
- `src/surface.rs`: the surfaces, evaluated generically over any scalar.
  - `Bezier`: a bicubic patch.
  - `Torus`: a torus `(R, r)`; `R = 0` is the unit sphere, with its pole.
  - `HermiteNet`: an `n x n` C^1 bicubic Hermite interpolant of another surface, standing
    in for a blend's approximating net.
- `src/curve.rs`: `Hermite<D>`, a C^1 piecewise-cubic Hermite curve (the trim in 2D, the
  edge in 3D).
- `src/gap.rs`: `Problem::certify`, the bisection of the trim's parameter range with four
  piece enclosures (`Method::Second`, `Centred`, `Lipschitz`, `Direct`) and two stops
  (`Stop::Tight`, within a thousandth of the largest proved value; `Stop::Within(tol)`, as
  soon as the bound is at most `tol`). The offset `Q = S + d N` is enclosed through the
  jets' chain rule, so `N`'s variation, the base's derivatives and the lower bound on
  `|S_u x S_v|` all come from one evaluation. `Problem::sampled_max` is the dense-sampling
  reference.
- `src/eps.rs`: `net_gap_bound`, a certified `max |B - S|` between a net `B` and the
  definition `S` it approximates, by bisection of the parameter domain (bilinear corners plus
  the tensor-product interpolation error, second partials in centred form from nested jets);
  `net_gap_sampled` is its sampled reference.
- `src/cases.rs`: the cases: a wavy Bézier patch offset both ways, a torus offset out, in, and
  to its cusp (`d = -r`), a sphere offset along trims that pass a distance `delta` from the
  pole, and a torus "blend" approximated by Hermite nets of three sizes.
- `src/bin/gap_bench.rs`: the measurements, printed as TSV sections (`offset`, `pole`,
  `blend`).
- `tests/bounds.rs`: every method's bound is at least the sampled maximum, the proved-attained
  value at most it, the tight bound within the stated ratio, the within-stop bound between the
  sampled maximum and the tolerance; near the pole the bound certifies and at it refuses; the
  composed blend bound (net gap plus `eps`) is at least the sampled gap on the definition.

## Running

```
cargo test --release
cargo run --release --bin gap-bench > bench.tsv
```

The bench takes about four minutes, nearly all in the 8 x 8 net's `eps` (which hits its box
cap; see the report).
