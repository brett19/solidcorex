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

## `testfloat/`: Berkeley TestFloat `f64_mulAdd` cases (BSD-3-Clause)

Test cases for IEEE 754 double fused multiply-add, used by `tests/testfloat_muladd.rs`.
Each line is `a b c result flags`, bit patterns in hex; `result` is `a * b + c` rounded
once to nearest, ties to even, and `flags` are the exception flags (not checked).

- Origin: Berkeley TestFloat 3e, https://github.com/ucb-bar/berkeley-testfloat-3, commit
  `a9c849f1b0eb0264b626d9686ffae167d996e3be` (2025-03-07), built against Berkeley
  SoftFloat 3e, https://github.com/ucb-bar/berkeley-softfloat-3, commit
  `afdc157e71f0d97508301ac0965defead49c679b` (2026-11-25), with the default `8086-SSE`
  specialisation (its default NaN is `FFF8000000000000`).
- `f64_mulAdd_level1.txt`: `testfloat_gen -seed 1 -level 1 f64_mulAdd`, all 6,133,248
  cases, keeping the first and every 1,533rd after it: 4,001 cases.
- `f64_mulAdd_level2.txt`: `testfloat_gen -seed 1 -forever f64_mulAdd` (level 2 cases,
  without end), the first 2,000. The output is the same on every run with the same seed.
- Each file starts with four comment lines naming its command and the licence.

The cases are generated output of TestFloat, which is under this licence:

```text
License for Berkeley TestFloat Release 3e

John R. Hauser
2018 January 20

The following applies to the whole of TestFloat Release 3e as well as to
each source file individually.

Copyright 2011, 2012, 2013, 2014, 2015, 2016, 2017, 2018 The Regents of the
University of California.  All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

 1. Redistributions of source code must retain the above copyright notice,
    this list of conditions, and the following disclaimer.

 2. Redistributions in binary form must reproduce the above copyright
    notice, this list of conditions, and the following disclaimer in the
    documentation and/or other materials provided with the distribution.

 3. Neither the name of the University nor the names of its contributors
    may be used to endorse or promote products derived from this software
    without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS "AS IS", AND ANY
EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE, ARE
DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE FOR ANY
DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
(INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND
ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```
