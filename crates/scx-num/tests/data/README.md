# Vendored test inputs

## `core-math/`: CORE-MATH worst cases (MIT)

Subsets of the CORE-MATH project's binary64 worst-case files, used by
`tests/fmath_worst.rs`.

- Origin: https://gitlab.inria.fr/core-math/core-math, files
  `src/binary64/<function>/<function>.wc`.
- Commit: `b464d7d0280f4c5091b8d2fff80b6cb03ab1d37f` (2026-11-27).
- Licence: MIT. Each file keeps the project's MIT notice, its first 20 lines, unchanged.
  The project's `LICENSE` file states the same MIT terms for all its code. Every one
  of the 18 source files carries the same header, compared line for line.

There is one file for each `fmath` function that has a `.wc` file:

- `exp`, `exp2`, `log` (`fmath::ln`), `log2`, `log10`
- `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`
- `sinh`, `cosh`, `tanh`
- `cbrt`, `hypot`, `pow` (`fmath::powf`)

`sincos.wc` is `sin.wc` followed by `cos.wc`, so `fmath::sin_cos` is checked on the
`sin` and `cos` subsets. `powi` has no file.

How each subset was taken:

1. Read the file's data lines, with comments and blank lines dropped.
2. Keep the first 32. These are the hand-picked inputs at the top of each file:
   overflow, underflow and subnormal edges, and special values.
3. From the rest, `R` lines, keep 968 at an even stride: line `i * R / 968` (integer
   division) for `i < 968`.
4. That makes 1,000 lines per function. The second comment line of each file gives
   its `R` and the size of the full file.

Lines are `x` or `x,y` in C99 hex, as in the source files. `atan2` lines are `y,x`.

A full file holds 29,000 to 2,000,000 inputs. To run every input, use the ignored test
`full_core_math_files` against a checkout (see `tests/fmath_worst.rs`).

