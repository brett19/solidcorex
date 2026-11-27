# Bugs found in other software

Defects found in software we use or depend on, kept as a record rather than filed (04 §1, 11-27).
Each row says what is wrong, how it was found, and what solidcorex does about it. Defects in the
pinned solidcore are not listed here; they live in `crates/scx-difftest/tests/diff/pin_defects.rs`,
each judged against the truth.

| Found | Software | Defect | Evidence | What solidcorex does |
|---|---|---|---|---|
| 11-27 | SolidCraft (the app) | Its evaluation cache key leaves out the Rust toolchain, so a design cached under one rustc release can be served to a build made with another, where results may differ by a rounding (research 67, D3t) | Research 67 §2.11 and review 67 finding 3 | Nothing yet; noted for the app switch (step 13) |
| 11-27 | SolidCraft (the app) | No check of the floating-point environment at start-up: a native library loaded into the desktop app's process can turn on flush-to-zero, which changes results (research 67 finding 6; the mechanism is inferred) | Research 67 §2.11 | Nothing yet; noted for the app switch (step 13) |
| 11-27 | Rust's MinGW target (`windows-gnu`) | MinGW's `fma` gives wrong results | rust-lang/rust#140515 (open when read) | `windows-gnu` is excluded from the contract in research 67 §5.1 |
| 11-27 | The `libm` crate | `powf` measured up to 496 ulp from the correctly rounded value | 04 §3 | Not used; `scx_num::fmath` is correctly rounded |
