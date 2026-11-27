# 69 - Test sources: external cases, datasets and papers to turn into scx tests

Written for solidcorex, 2026-11-27. The question: which published test cases, conformance suites,
reference datasets and papers can become unit or integration tests for scx, under the corpus
licence bar (06; 02 §8: "Corpus data is permissive-licence only").

This note does not repeat what exists. `02-testing.md` gives the strategy: truth first, the pin as
evidence. Research 59 and 64 give the `scx-testkit` truths T1 to T8. Research 58 gives the STEP
corpus already in use (33 NIST files and 5 pin fixtures). This note adds sources outside the repo
and maps each one to a crate and a step of 01 §10.2.

Labels:

- **PRIMARY**: the licence was read on the source's own licence file or page on 2026-11-27. Where
  possible this was the raw file through the GitHub or GitLab API. Otherwise it was the page, read
  through a summarising fetch.
- **SNIPPET**: the licence comes from a search result or a secondary page.
- **UNVERIFIED**: the licence could not be confirmed.

Nothing was downloaded beyond licence files, directory listings and the first lines of one
CORE-MATH worst-case file and one ITF1788 file. Those were read only to confirm the format and
the header.

## 1. Rules applied

- **Usable.** Public domain, CC0, CC-BY, MIT, Apache-2.0, BSD, zlib and 0BSD are usable, as is an
  explicit unrestricted grant. Notice and attribution conditions are fine.
- **Not usable.** NC, ND, SA, GPL, LGPL and AGPL terms are not usable. Neither are research-only,
  participant-only or terms-of-service-bound terms, self-declared licences per item, or no
  licence at all. A questionable licence is treated as not usable.
- **Facts, not files.** A paper's, book's or standard's examples are cited and re-derived, never
  copied in bulk. A single worst-case input or a polynomial's definition is a fact. A file of
  them is a work, and its licence governs copying the file.
- **Copyleft kernels.** For OCCT, CGAL, SISL, GoTools, MPFR, glibc and CRlibm, only their
  licence statements, docs and papers were consulted. No code or test data was opened.
- **Truth never comes from another program's output** unless that output is itself a certified
  reference. Even then we recompute it with our own exact or high-precision references (`scx-testkit`).
  Validation properties embedded in STEP files are never used as expected values.

## 2. Numerics: `scx-num` (`fmath`, `eft`, intervals), `scx-algo` (polynomials, quadrature)

These serve step 1 (10.1) now, and step 12's per-target `fmath` table.

| # | Source | Contents | Licence | Verdict | Crate, step | Test we build | Truth |
|---|---|---|---|---|---|---|---|
| N1 | CORE-MATH worst cases, [gitlab.inria.fr/core-math/core-math](https://gitlab.inria.fr/core-math/core-math) `src/binary64/<fn>/<fn>.wc` | Hardest-to-round inputs per function and format, as hex floats one per line with `#` comments. `exp.wc` holds about 1.1 M lines and starts with its overflow and underflow edges. Most were computed with BaCSeL | MIT, [LICENSE](https://gitlab.inria.fr/core-math/core-math/-/blob/master/LICENSE). The MIT notice is in the `.wc` header, PRIMARY for `exp.wc`. Other `.wc` headers are UNVERIFIED and assumed the same | **usable**, keeping the notice | `scx-num` `fmath`; step 1, step 12 | For every `fmath` function, a committed subset of 1-5 k inputs per function (with the MIT header), each correctly rounded to nearest-even. The nightly run takes the full file. The present test is 5,000 log-uniform inputs plus a hand table (fmath docs), so this is far harder | Inputs only. The output is computed by the testkit at about 200-1000 bits, which needs a multi-precision reference larger than double-double; a Ziv-style test is enough because every case is known to need fewer than about 160 bits |
| N2 | Lefèvre and Muller, "Worst cases for correct rounding of the elementary functions in double precision" (INRIA RR-4044, 2000; ARITH 2001); [Muller's pages](https://perso.ens-lyon.fr/jean-michel.muller/) | Tables of hardest cases for exp, log, sin, cos, ... | no licence stated (SNIPPET and page) | **facts only**; N1 supersedes it | `scx-num`; step 1 | Cite in the test; take the cases through N1 | as N1 |
| N3 | Zimmermann et al., "Accuracy of mathematical functions in single, double, extended double and quadruple precision" ([papers page](https://members.loria.fr/PZimmermann/papers/)); data at [gitlab.inria.fr/zimmerma/math_accuracy](https://gitlab.inria.fr/zimmerma/math_accuracy) | The largest known error of each libm per function, with the input | repository has no LICENSE (PRIMARY: the tree lists none) | **files not usable; facts citable**; needs a decision (section 8) | `scx-num`; step 1 | The published worst inputs of glibc, AMD and others, re-derived and asserted at 0.5 ulp for `fmath` | testkit high precision |
| N4 | Berkeley TestFloat 3e and SoftFloat 3e, [ucb-bar/berkeley-testfloat-3](https://github.com/ucb-bar/berkeley-testfloat-3) | Generator of IEEE 754 vectors (add, sub, mul, div, **fma**, sqrt, rem, roundToInt, conversions) in every rounding mode, with flags | BSD-3-Clause, [COPYING.txt](https://github.com/ucb-bar/berkeley-testfloat-3/blob/master/COPYING.txt) (PRIMARY) | **usable** | `scx-num` `eft`; step 1, step 12 | Generate f64 `mulAdd` vectors (levels 1 and 2), check the software fused multiply-add in `eft` bit for bit, and check `two_sum` and `two_prod` exactness on the same operands. This is the one primitive `fmath`'s cross-platform claim rests on | SoftFloat output, an independent implementation; we also check exactness by big integers |
| N5 | ARM optimized-routines, [ARM-software/optimized-routines](https://github.com/ARM-software/optimized-routines) `math/test/testcases/` | `.tst` vectors of function, input, expected output and flags; special values and edges | MIT OR Apache-2.0 WITH LLVM-exception, [LICENSE](https://github.com/ARM-software/optimized-routines/blob/master/LICENSE) (PRIMARY) | **usable** | `scx-num` `fmath`; step 1 | Only the special-value rows (NaN, infinities, signed zero, subnormal edges) against `fmath`'s C99 table. The general expected values target faithful, not correct, rounding | the vector, for exact special values only |
| N6 | RLIBM, [rutgers-apl/rlibm](https://github.com/rutgers-apl/rlibm) | Correctly rounded f32 and posit library with an MPFR-checked suite | BSD-2-Clause, [LICENSE](https://github.com/rutgers-apl/rlibm/blob/master/LICENSE) (PRIMARY) | usable, low value | none now | Technique only: exhaustive f32 testing. For an f32 `fmath` (none planned) we would test all 2^32 inputs ourselves | own reference |
| N7 | ITF1788, [oheim/ITF1788](https://github.com/oheim/ITF1788) `itl/` | IEEE 1788-2015 interval test suite in a small DSL (`testcase { op args = result; }`), with hex bounds and decorations, 19 files | mixed, per file (PRIMARY, headers read): 11 `libieeep1788_*.itl` are Apache-2.0 ([licence](https://www.apache.org/licenses/LICENSE-2.0)); 5 (`abs_rev`, `atan2`, `ieee1788-constructors`, `ieee1788-exceptions`, `pow_rev`) carry the all-permissive grant ("Copying and distribution ... permitted in any medium without royalty provided the copyright notice and this notice are preserved"); `c-xsc.itl`, `fi_lib.itl` and `mpfi.itl` are LGPL-2.1 | **usable for the 16 permissive files; not the 3 LGPL ones** | `scx-num` `interval`; step 1 | A small parser in `scx-testkit` that runs the Apache and permissive files against `Interval` for the operations scx has: add, sub, mul, div, sqrt, elementary functions, set operations and reverse operations. Where 1788 requires tightness and scx only promises containment, the case becomes a containment check plus a measured width ratio | hand-derived by the authors from 1788 semantics and cross-checked across libraries; we also check containment with the testkit's high-precision reference |
| N8 | IntervalArithmetic.jl, [JuliaIntervals/IntervalArithmetic.jl](https://github.com/JuliaIntervals/IntervalArithmetic.jl) | Interval library tests, partly ITF1788-derived | MIT, [LICENSE.md](https://github.com/JuliaIntervals/IntervalArithmetic.jl/blob/master/LICENSE.md) (PRIMARY); test file headers UNVERIFIED | usable, redundant with N7 | `scx-num`; step 1 | none unless N7 leaves a gap | n/a |
| N9 | Polynomial root families: Wilkinson, Mignotte (x^n - 2(ax - 1)^2), Chebyshev, Legendre and Hermite, (x - 1)^k, clusters x^n - eps, Kirkpatrick, Mandelbrot. These are the families benchmarked in Bini and Robol's MPSolve papers (MPSolve itself is GPL-3, [COPYING](https://github.com/robol/MPSolve/blob/master/COPYING), not opened) and in Sagraloff's ANewDsc papers | Definitions in papers | mathematics; generated by us | **usable** (no copying) | `scx-algo` `polynomial` and Bernstein roots; step 1, step 2 | A testkit generator of these families in exact integer coefficients, converted to Bernstein form on chosen intervals. Each enclosure must contain the true roots with the right multiplicities, and isolation must separate clusters down to a stated gap. Research 53 found the quartic's error on clustered roots; this pins it | exact roots where closed (Wilkinson, Chebyshev); otherwise Sturm sequences in big integers in the testkit |
| N10 | Zeng, MultRoot (ACM TOMS Algorithm 835, 2004); Mørken and Reimers, "An unconditionally convergent method for computing zeros of splines and polynomials" (Math. Comp. 76, 2007) | Multiple-root and spline-zero test polynomials | software: no licence found (UNVERIFIED); papers: cite only | **facts only** | `scx-algo`; step 2, step 5 (spline zeros) | The multiplicity patterns ((x-1)^10 (x-2)^5 ...) and the spline-zero examples, rebuilt from the papers' definitions | exact |
| N11 | Genz test functions (Genz 1984, "Testing multidimensional integration routines"); QUADPACK's test integrals (Piessens et al. 1983, book) | Six parametric families (oscillatory, product peak, corner peak, Gaussian, C0, discontinuous) and singular-endpoint integrals | papers and book: cite only. QUADPACK's netlib code has no licence text (PRIMARY readme); "public domain" is SNIPPET | **facts only** | `scx-algo` quadrature; step 1, step 4 (`scx-mass`) | Seeded parameters for the families in 1-3 D, error against truth, and the error estimate is never below the true error | closed forms for all six families, evaluated in the testkit |
| N12 | Bailey, Jeyabalan and Li, "A comparison of three high-precision quadrature schemes" (Experimental Mathematics 14, 2005) | About 15 integrals with closed forms, endpoint singularities among them | paper: cite only | **facts only** | `scx-algo`; step 1 | As N11, for the singular-endpoint and infinite-derivative cases | closed forms |
| N13 | CRlibm | Correctly rounded libm and its vectors | LGPL-2.1 (PRIMARY, COPYING.LIB of a mirror) | **not usable** | none | none; N1 covers it | n/a |
| N14 | UCBTEST (Hough and Kahan, [netlib.org/fp](https://netlib.org/fp/)); IBM FPgen; Paranoia | IEEE test programs and vectors | UCBTEST: mixed per-file notices ("most of the source code is NOT public domain", PRIMARY readme); FPgen: no download or terms found (UNVERIFIED); Paranoia: no licence text (PRIMARY) | **not usable** as files | `scx-num`; step 1 | Techniques only: Kahan's near-halfway mul, div and sqrt operands, generated from the published method | own exact check |

## 3. NURBS: `scx-nurbs`, `scx-nurbs-surface`, `scx-curve`, `scx-surface`

These serve step 2 (evaluation, knots, analytic kinds) and step 5 (all NURBS work, fitting). The
testkit already has exact rational evaluation, insertion and elevation (59). What outside sources
add is **independent conventions**: worked answers that were not produced by our own reference.

| # | Source | Contents | Licence | Verdict | Crate, step | Test we build | Truth |
|---|---|---|---|---|---|---|---|
| G1 | Piegl and Tiller, *The NURBS Book*, 2nd ed. (Springer 1997) | Worked examples: basis functions on U = {0,0,0,1,2,3,4,4,5,5,5}, p = 2, with derivatives (ch. 2); the rational circle in its 9- and 7-point forms (ch. 4, 7); knot insertion and refinement, knot removal, degree elevation and reduction (ch. 5); point inversion and projection (ch. 6, algorithms only) | book copyright: **cite and re-derive** | **facts only** | `scx-nurbs`; step 2, step 5 | One test per cited example, naming the example number: the basis values at given u, the span index, the circle weights giving x^2 + y^2 = 1 exactly, insertion and removal round trips, and elevation shape equality. Chapter and example numbers were quoted from memory and are **UNVERIFIED**; check them against the book before citing | re-derived exactly in rationals by the testkit |
| G2 | Farin, *Curves and Surfaces for CAGD*; Rogers, *An Introduction to NURBS*; Cohen, Riesenfeld and Elber, *Geometric Modeling with Splines* | Worked examples, blossoming identities, conversions | books: cite only | **facts only** | `scx-nurbs`; step 2, step 5 | Blossom identities (symmetry, diagonal, multi-affinity) as property tests; Rogers' worked rational examples as cited cases (numbers UNVERIFIED) | exact rationals |
| G3 | verb, [pboyer/verb](https://github.com/pboyer/verb) `test/` (`testCore.js`, `testEval.js`, `testGeom.js`) | NURBS evaluation, knot insertion, derivatives, closest point, intersection cases | MIT, [LICENSE](https://github.com/pboyer/verb/blob/master/LICENSE) (PRIMARY) | **usable** | `scx-nurbs`, `scx-curve`; step 2, step 5 | Mine the case list (inputs whose answer is analytic or re-derivable) and re-state each one in testkit form. No code is copied; MIT would allow it, but we keep the truth our own | testkit exact references; the verb assertion only as a cross-check |
| G4 | tinyspline, [msteinbeck/tinyspline](https://github.com/msteinbeck/tinyspline) | B-spline evaluation, insertion, splitting, derivative and conversion tests | MIT, [LICENSE](https://github.com/msteinbeck/tinyspline/blob/master/LICENSE) (PRIMARY) | **usable** | `scx-nurbs`; step 2, step 5 | As G3; especially the splitting and Bézier-decomposition edge cases (multiplicities at ends, unclamped vectors) | testkit |
| G5 | NURBS-Python (geomdl), [orbingol/NURBS-Python](https://github.com/orbingol/NURBS-Python) | Tests and sample control-net files; fitting (interpolation and approximation) | MIT, [LICENSE](https://github.com/orbingol/NURBS-Python/blob/5.x/LICENSE) (PRIMARY); provenance of each data file UNVERIFIED (some may come from the NURBS Book) | **usable for the code's cases**; data files unclear | `scx-nurbs`; step 5 (fitting) | Interpolation and approximation cases: the fit passes through its data, the residual is no larger than the stated bound, and the parameterisation (chord, centripetal) matches the definition | definitional; least squares in exact rationals for small cases |
| G6 | curvo, [mattatz/curvo](https://github.com/mattatz/curvo) (Rust) | NURBS curves and surfaces, intersection, boolean of curves, tests and benches | MIT, [LICENSE](https://github.com/mattatz/curvo/blob/main/LICENSE) (PRIMARY) | **usable**, small | `scx-curve`; step 5 | Curve-curve intersection and closest-point case list | testkit |
| G7 | splinepy, [tataratat/splinepy](https://github.com/tataratat/splinepy) | Spline library with tests | MIT with a BSD notice for SciPy-derived parts ([LICENSE](https://github.com/isosuite/splinepy/blob/main/LICENSE); PRIMARY via the agent, the SPDX field reads NOASSERTION) | usable, keep notices; low priority | `scx-nurbs`; step 5 | none beyond G3 to G5 | n/a |
| G8 | OpenNURBS, [mcneel/opennurbs](https://github.com/mcneel/opennurbs) | `.3dm` examples and `example_test` | the repo's LICENSE opens "All Rights Reserved" (PRIMARY); research 66 found the grant on the vendor page instead. The `.3dm` models' terms are UNVERIFIED | **unclear**; needs a decision (section 8) | none until decided | none | n/a |
| G9 | truck, [ricosjp/truck](https://github.com/ricosjp/truck) | NURBS, shape construction and shape-ops tests | Apache-2.0, [LICENSE](https://github.com/ricosjp/truck/blob/master/LICENSE) (PRIMARY) | **usable** | `scx-nurbs` to `scx-boolean`; step 2 to step 7 | See B3; the geometry tests add little to G3 | testkit |
| G10 | SINTEF GoTools; SISL | Spline libraries and data | AGPL-3.0 (GoTools, repo page, PRIMARY); SISL is AGPL (SNIPPET) | **not usable**; not opened | none | none | n/a |

## 4. Projection and intersection: `scx-surface`, `scx-curve`, `scx-ssi`

These serve step 2 (analytic projection, curve-surface intersection, analytic SSI) and step 5
(B-spline SSI). T1 to T5 already give exact truths for the analytic kinds (59, 64). The gaps are
freeform projection, freeform SSI and hard topology.

| # | Source | Contents | Licence | Verdict | Crate, step | Test we build | Truth |
|---|---|---|---|---|---|---|---|
| I1 | ssi-corpus, [jyviko/ssi-corpus](https://github.com/jyviko/ssi-corpus) | 119 SSI cases as JSON, one file per case: the surfaces as typed parameters or nets, the truth sampled densely, the expected topology (branches, isolated points, singular points, coincident regions), the coverage tolerance and a note on the truth's origin. 94 are our own constructions in 9 classes; 25 come from Yang, Jia and Yan 2023 (TOG 42(6) 211), 6 with the paper's data and 19 fitted to its figures. It ships a Python scorer with verdicts from CRASH to PASS | MIT OR Apache-2.0, [LICENSE-MIT](https://github.com/jyviko/ssi-corpus/blob/main/LICENSE-MIT) and [LICENSE-APACHE](https://github.com/jyviko/ssi-corpus/blob/main/LICENSE-APACHE) (PRIMARY). The README states the Yang paper is CC-BY; that is **UNVERIFIED** here (the ACM page was not reached) | **usable** | `scx-ssi`; step 2 (the analytic classes), step 5 (B-spline and rational) | A Rust reader and scorer in `scx-testkit` (the verdicts re-implemented; the Python scorer is not run in CI) and a test per class. The analytic-pair cases join T5 now; the rest gate step 5. The README reports SMLib's `GlobalSurfaceIntersect` at 81 of 119 and SolidCraft's face-face intersection at 47, at 1e-6 | the cases' closed forms (most) or the stated construction; the 19 figure-fitted cases have their own exact intersection by construction |
| I2 | Dupont, Lazard, Lazard and Petitjean, "Near-optimal parameterization of the intersection of quadrics" I-III (J. Symbolic Comput. 43, 2008) | Exact parametrisations of every quadric-pair intersection type (smooth quartic, nodal, cuspidal, two conics, lines), the classification by Segre symbol | papers: cite and re-derive. The QI library is "free for non-commercial use" (SNIPPET; the site refused the fetch) | **papers usable as facts; QI not usable, not opened** | `scx-ssi`; step 2 (T5 widening), step 5 | Extend T5 to non-coaxial quadric pairs: one case per Segre type, built so the parametrisation has rational or one-square-root coefficients. Check the library's branches against it: count, singular points, closed or open | exact parametrisation derived by hand from the paper's method, checked on both surfaces in exact arithmetic |
| I3 | Ye and Maekawa, "Differential geometry of intersection curves of two surfaces" (CAGD 16, 1999); Patrikalakis and Maekawa, *Shape Interrogation for Computer Aided Design and Manufacturing* (Springer 2002; free online edition from MIT, terms UNVERIFIED) | Curvature, torsion and higher derivatives of transversal and **tangential** intersection curves; catalogues of tangent, loop and singular cases | cite only | **facts only** | `scx-ssi`; step 5 | Derivative checks on traced curves at tangential contact (where the first-order direction is undefined), for constructed tangent pairs (sphere inside a cylinder, plane tangent to a torus along a circle) | closed forms for the constructed pairs; the paper's formulas evaluated in double-double |
| I4 | Sederberg and Meyers, "Loop detection in surface patch intersections" (CAGD 5, 1988); Hohmeyer, loop detection (PhD thesis, Berkeley 1992); Grandine and Klein, "A new approach to the surface intersection problem" (CAGD 14, 1997) | Loop criteria and their hard cases (small closed loops inside a patch, nearly tangent pairs) | cite only | **facts only** | `scx-ssi`; step 5 | A seeded family of small loops: two bicubic patches whose intersection is a circle of radius r inside one patch, for r from 1e-1 down to 1e-7 of the patch size. It must be found, never missed | exact by construction (a perturbed plane against a paraboloid-like patch with a known circular section) |
| I5 | Point projection papers: Ma and Hewitt, "Point inversion and projection for NURBS curve and surface" (CAGD 20, 2003); Hu and Wallner, "A second order algorithm for orthogonal projection onto curves and surfaces" (CAGD 22, 2005); Chen et al., "A torus patch approximation approach for point projection on surfaces" (CAD 41, 2009) | Test curves and surfaces with points to project, some with timing tables | cite only; whether the papers give exact answers is **UNVERIFIED** | **facts only** | `scx-surface` NURBS inversion; step 2 (SU4), step 5 | Their test shapes rebuilt where the control points are published. Projection must be global (every stationary point found) and must pass the residual orthogonality check of 59 §7 | testkit: exact residual orthogonality, and all stationary points by Bernstein subdivision in exact rationals for low-degree cases |
| I6 | Manifold, [elalish/manifold](https://github.com/elalish/manifold) `test/` | Mesh-kernel tests; see B2 | Apache-2.0 | see B2 | | | |

## 5. Bodies, Booleans, mass, tessellation, HLR (later steps)

### 5.1 Datasets

| # | Source | Contents | Licence | Verdict | Crate, step | Test we build | Truth |
|---|---|---|---|---|---|---|---|
| D1 | NIST MBE PMI test models, [download page](https://www.nist.gov/el/systems-integration-division-73400/mbe-pmi-validation-and-conformance-testing/download-free-cad) | CTC 01-05, FTC 06-11, STC, HTC (holes), MTC (an assembly) and D2MI, in STEP AP242 and AP203 plus native formats | "The test cases, CAD models, and STEP files can be used without any restrictions." Acknowledgement is requested; the NIST logo may not be used in promotion (PRIMARY, read on the page) | **usable**; already used by `scx-p21` (58) | `scx-p21` now; `scx-step`, `scx-check`, `scx-stitch` in step 6; `scx-mass` and `scx-tess` in step 4 and step 6 | Fetched at test time, never committed. Every file imports and passes the check. Mass and area are tested by properties only: convergence under refinement, invariance under a rigid move and agreement with an independent mesh estimate (02 §5). The embedded validation properties are never the expected values | properties and independent estimates |
| D2 | MFCAD, [hducg/MFCAD](https://github.com/hducg/MFCAD) | STEP prismatic parts with machining-feature labels per face, for ML research. The model count is UNVERIFIED (about 15 k per a derivative) | MIT, [LICENSE](https://github.com/hducg/MFCAD/blob/master/LICENSE) (PRIMARY) | **usable** | `scx-step`, `scx-check`; step 6. `scx-boolean`; step 7 | A sampled subset fetched at test time: import, check, record round trip, and mass invariance under a rigid move. Its feature labels give a cheap check of face counts after import | properties; labels for face counts only |
| D3 | ABC dataset, [deep-geometry.github.io/abc-dataset](https://deep-geometry.github.io/abc-dataset/) | About 1 M Onshape models | creators keep copyright under Onshape's Terms of Use (site); the paper's MIT claim conflicts | **not usable** | | | |
| D4 | DeepCAD, Text2CAD, SketchGraphs, SolidGen, truck's `resources4truck` ABC files | ABC- or Onshape-derived | DeepCAD has no data licence and inherits ABC (PRIMARY repo); Text2CAD is CC BY-NC-SA 4.0 (SNIPPET); the others are Onshape-derived | **not usable** | | | |
| D5 | Fusion 360 Gallery, BRepNet data | Fusion-derived reconstruction and segmentation sets | non-commercial research only, no redistribution ([LICENSE.md](https://github.com/AutodeskAILab/Fusion360GalleryDataset/blob/master/LICENSE.md)); BRepNet is CC BY-NC-SA 4.0 (PRIMARY README) | **not usable** | | | |
| D6 | CAD-Recode | About 1 M CadQuery programs | cc-by-nc-4.0 (PRIMARY, Hugging Face card) | **not usable** | | | |
| D7 | Thingi10K; MFCAD++; MCB (Mechanical Components Benchmark); ModelNet; GrabCAD; TraceParts | Meshes and scraped models | per-item self-declared licences (Thingi10K); an unversioned CC BY with the host's reservation of text and data mining (MFCAD++); third-party terms of service (MCB, GrabCAD, TraceParts); no open licence (ModelNet) | **not usable** | | | |
| D8 | FreeCAD library ([FreeCAD/FreeCAD-library](https://github.com/FreeCAD/FreeCAD-library)); KiCad 3D packages | Part libraries | FreeCAD library: CC-BY 3.0 self-attested per contributor (SNIPPET); KiCad: CC-BY-SA with an exception | **not usable** (self-declared; SA) | | | |
| D9 | CAx-IF and MBx-IF test rounds, [mbx-if.org test rounds](https://www.mbx-if.org/home/cax/testrounds/); Recommended Practices, [mbx-if.org](https://www.mbx-if.org/home/cax/recpractices/) | Test-suite PDFs and participant models; Recommended Practices such as Geometric and Assembly Validation Properties v4.6 and Tessellated 3D Geometry v1.1 | test PDFs are public but carry no licence (PRIMARY); the page footer reads only "MBx Interoperability Forum © 2026"; older suites restrict models to participants (SNIPPET) | **models not usable; practices citable** | `scx-step`; step 6 | Cite the validation-property definitions (how volume, area and centroid are defined and with what tolerance) for `scx-step`'s writer, and write our own small STEP files that exercise each rule. No CAx-IF model is used | our own construction |
| D10 | STEP Tools sample files | Sample STEP files | "All Rights Reserved", informational and non-commercial copying | **not usable** | | | |
| D11 | NASA 3D Resources | Spacecraft models (mostly meshes) | NASA media policy is usually public domain, but individual items can differ (UNVERIFIED) | **unclear**, low value (meshes) | none | none | n/a |

There is **no large permissive B-rep corpus** beyond D1 and D2. Test bodies for steps 4 and 7 to 10
are therefore recipes built in code (02 §8), as the constructions below are.

### 5.2 Boolean, fillet and robustness constructions

| # | Source | Contents | Licence | Verdict | Crate, step | Test we build | Truth |
|---|---|---|---|---|---|---|---|
| B1 | Fornjot, [hannobraun/fornjot](https://github.com/hannobraun/fornjot) `models/` | Eight example models: cuboid, holes, spacer, split, star and others | 0BSD, [LICENSE.md](https://github.com/hannobraun/fornjot/blob/main/LICENSE.md) (PRIMARY: "Permission to use, copy, modify, and/or distribute this software for any purpose") | **usable** | `scx-make`, `scx-boolean`; step 4, step 7 | Re-built as `scx-testkit` recipes with a size knob: extrude, holes and split. The star profile also seeds the long fillet chains of step 9 | analytic volume and area of each recipe |
| B2 | Manifold, [elalish/manifold](https://github.com/elalish/manifold) `test/` (`boolean_test.cpp`, `boolean_complex_test.cpp`, `properties_test.cpp`, `manifold_fuzz.cpp`, `polygon_corpus.h`) and `samples/` | Mesh Boolean cases: coplanar and shared faces, nearly tangent spheres, the Menger sponge, perturbation sweeps, fuzz-found regressions committed as tests; `test/models/*.obj` (for example `Cray_left.obj`, `self_intersectA.obj`, `openscad-nonmanifold-crash.obj`) | Apache-2.0, [LICENSE](https://github.com/elalish/manifold/blob/master/LICENSE) (PRIMARY). The origin of the OBJ meshes (several named after user reports) is UNVERIFIED | **case descriptions usable; OBJ files unclear** (section 8) | `scx-boolean`; step 4 (analytic), step 7 | The constructions re-stated as B-rep recipes: Menger sponge (level 1-3), coplanar box stacks offset by 0, 1 ulp and the resolution, sphere against sphere at gaps from the resolution down to touching, and a cube rotated through 1e-9 to 1e-3 rad against itself. The OBJ files are not used: they are meshes, and their provenance is unclear | analytic volume (the Menger level-n volume is (20/27)^n) and the algebra of 02 §5 |
| B3 | truck, [ricosjp/truck](https://github.com/ricosjp/truck) `truck-modeling/examples`, `truck-shapeops` tests | Bottle, punched cube, torus-punched cube, cube in cube, tsudumi | Apache-2.0 (PRIMARY) | **usable** | `scx-make`, `scx-boolean`; step 4, step 7 | The torus-punched cube and the bottle as recipes: a torus-plane and torus-box Boolean, and a sweep with a fillet-like neck | analytic: the torus volume 2 pi^2 R r^2, Pappus for revolves |
| B4 | Cherchi et al., "Interactive and robust mesh Booleans" (TOG 41(6), 2022; arXiv 2205.14151) | 3,814 random Booleans on 7,628 cleaned Thingi10K meshes | data inherits Thingi10K: **not usable**; method citable | **technique only** | `scx-boolean`; step 7 | Random pairing of the recipe bodies at seeded poses, every result checked for closedness, manifoldness, validity and the volume algebra | properties |
| B5 | Blend surveys: Vida, Martin and Várady, "A survey of blending methods that use parametric surfaces" (CAD 26, 1994); Choi and Ju, "Constant-radius blending in surface modelling" (CAD 21, 1989; citation from memory, UNVERIFIED) | Blend definitions, hard cases (concave and convex corners, setbacks, vanishing radius) | cite only | **facts only**; no fillet model set exists under a usable licence | `scx-blend`; step 9 | Recipe parts: a box edge of radius r (volume removed per edge (1 - pi/4) r^2 L, exact); a cylinder-plane fillet (a torus section, exact); three-edge convex and mixed corners; a radius too large, giving a typed `Unsupported` or error and never wrong geometry. Research 61 §4.4 gives the section checks | closed forms; the tangency and radius checks of 02 §5 |

### 5.3 Mass properties

| # | Source | Contents | Licence | Verdict | Crate, step | Test we build | Truth |
|---|---|---|---|---|---|---|---|
| M1 | Closed forms: box, wedge, cylinder, cone and frustum, sphere and spherical cap, torus; Pappus-Guldinus for any revolve; the parallel-axis theorem | Volume, area, centroid, inertia tensor | mathematics | **usable** | `scx-mass`; step 4 | Exact values, then the metamorphic set: a translation moves the centroid only, a rotation R gives R I R^T, a uniform scale s gives V s^3, A s^2 and I s^5, a mirror keeps the volume, and the volume algebra of 02 §5 holds | closed forms in double-double |
| M2 | Mirtich, "Fast and accurate computation of polyhedral mass properties" (J. Graphics Tools 1(2), 1996), [page](https://people.eecs.berkeley.edu/~jfc/mirtich/massProps.html); Lien and Kajiya (IEEE CG&A 4(10), 1984); Eberly, "Polyhedral mass properties (revisited)" (Geometric Tools) | Divergence-theorem reduction to face and edge integrals | papers: cite. Mirtich's C code has no licence on its page (PRIMARY): **not used** | **facts only** | `scx-mass`; step 4 | An independent estimate in the testkit for curved bodies: the divergence theorem over a fine tessellation with Richardson extrapolation. For polyhedra the answer is exact in rationals | exact for polyhedra; extrapolated otherwise |
| M3 | Gauss-Legendre exactness | A rule of n points is exact for degree 2n - 1 | mathematics | **usable** | `scx-algo` quadrature, `scx-mass`; step 1, step 4 | Polynomial integrands at exactly 2n - 1 and 2n, and per-span integration of B-spline faces where the integrand is polynomial (a planar B-spline face's area) | exact rationals |

### 5.4 Tessellation and HLR

No permissively licensed reference set was found for either. Tests are properties.

| # | Source | Test we build | Truth |
|---|---|---|---|
| H1 | none found; properties only (`scx-tess`; step 4) | The chord bound holds: the largest distance from a triangle to its surface is within the stated tolerance, checked by sampling against the testkit's exact surfaces. The mesh is watertight: every interior edge has two triangles, and the shared edge vertices are identical across faces. Orientation is consistent. The mesh volume and area converge to the analytic values at the expected order | analytic surfaces |
| H2 | none found; properties only (`scx-hlr`; step 6) | Silhouettes of a sphere, cylinder, cone and torus for a given view, in closed form. Visibility of sampled edge points is checked by an independent ray cast against the B-rep. A symmetric part seen from mirror views gives mirrored drawings | closed-form silhouettes; ray-cast classification |

## 6. Techniques (property-based and metamorphic)

Taken from docs and papers only; nothing here copies code.

| # | Source | Technique | Where in scx |
|---|---|---|---|
| P1 | Manifold (README, test tree; PRIMARY) | libFuzzer harnesses on Booleans and polygon triangulation. A fuzz-found failure is committed as a test (DISABLED until fixed). Invariants on every result: manifoldness, genus, volume, area, bounding box. ASan and UBSan in CI | the nightly fuzz tier of 02 §8; `scx-boolean`, `scx-planar` (triangulation of a sketch), step 4 to step 7 |
| P2 | CGAL docs (the exact-predicates, exact-constructions kernel; filtered predicates) | Every floating predicate is checked against an exact rational evaluation on adversarial near-degenerate inputs: collinear, cospherical, points one ulp off a plane | `scx-num`, `scx-algo` predicates; `scx-planar`; step 1, step 5. The testkit's big integers already allow it |
| P3 | Fornjot (validation layer; README, PRIMARY for the licence; the validation design is UNVERIFIED here) | Validation run after every operation as a test oracle: structure, coincidence of edge and curve, closed loops | already the plan: `check` takes `&Brep` (02 §5) |
| P4 | Random CSG trees (B4's method; GraphicsFuzz, ECOOP 2020, for metamorphic testing of compilers) | A random tree of primitives at seeded poses, with an **independent oracle**: point membership by evaluating the tree itself on the primitives' implicit forms, sampled on a grid and compared with `scx-bodyq` classification and the mass estimate. Shrink a failing tree to a minimal one | `scx-boolean`, `scx-bodyq`; step 4, step 7. The oracle needs no Boolean |
| P5 | Metamorphic relations (02 §5 has most) | Additions: De Morgan through complement inside a large box; a jitter of 1e-9 changes the volume continuously and the topology only where a contact appears; idempotence A ∩ A = A; a cut then a join restores the face count as well as the volume | `scx-boolean`; step 7 |
| P6 | Exhaustive small domains (N6) | Every f32 input for an f32 function; every degree, multiplicity and knot pattern up to a small size for NURBS knot operations | `scx-nurbs` insertion, removal and elevation; step 2, step 5 |

## 7. The ten to adopt first

Ranked by value now (the steps in progress, then the next step), times the confidence in the
licence, divided by the cost.

1. **N1, CORE-MATH worst cases (MIT).** The present `fmath` test samples 5,000 inputs per
   function; these are the inputs that break correctly rounded libraries. It needs a
   multi-precision reference in the testkit. Step 1 and step 12.
2. **N4, TestFloat `mulAdd` vectors (BSD-3).** The software fused multiply-add in `eft` carries the
   whole cross-platform claim (67). Step 1.
3. **N7, ITF1788's 16 permissive files (Apache-2.0 and the all-permissive grant).** A ready
   conformance suite for `Interval`, which the certified bounds stand on. A small parser. Step 1.
4. **N9, the polynomial root families (generated).** The quartic's error on clustered roots (53)
   is known and untested against a family. Exact truths by Sturm sequences. Step 1 and step 2.
5. **I1, ssi-corpus (MIT OR Apache-2.0).** 119 cases with exact topology, the only SSI set with
   known answers; its analytic classes join T5 now and the rest gate step 5's B-spline SSI.
6. **I2, the quadric-pair parametrisations (Dupont et al. 2008, re-derived).** Widens T5 from
   coaxial and plane sections to every quadric pair, with exact branches and singular points.
   Step 2 and step 5.
7. **D1, NIST MBE PMI (unrestricted grant).** Already the STEP corpus. Next it becomes the
   import-and-check corpus (step 6) and the mass and tessellation property corpus (step 4).
8. **M1 and M2, mass closed forms and the divergence-theorem estimate.** The truths `scx-mass`
   needs at step 4, where the end-to-end difftest by outcome begins.
9. **B1 to B3, the Fornjot, Manifold and truck constructions as recipes (0BSD, Apache-2.0).** The
   first Boolean suite with analytic volumes (step 4 analytic Booleans, then step 7).
10. **G1, G3 and G4: the NURBS Book examples (cited) and the verb and tinyspline case lists (MIT).**
    These are independent conventions for evaluation, insertion, removal and splitting. Removal is
    not in the testkit yet (59 §7). Step 2 and step 5.

Next after these: P4 (random CSG with an implicit oracle) at step 7, N11 and N12 (quadrature) when
`scx-mass` integrates per span, D2 (MFCAD) at step 6, and B5's fillet recipes at step 9.

## 8. Licences that need the owner's decision

1. **N3 and N2: worst-case inputs taken from sources with no licence** (Zimmermann's
   `math_accuracy` repository; Muller's TMD tables). Single inputs are facts. Is citing and
   re-deriving a few dozen of them, with the source named, acceptable? Recommended: yes, but take
   bulk inputs only from N1, which is MIT.
2. **N7: Heimlich's all-permissive grant** on five ITF1788 files. It is an explicit grant with a
   notice condition, which reads as within the bar. It is not on the named list, so it needs a
   one-line ruling.
3. **G8: OpenNURBS.** The repository's LICENSE reads "All Rights Reserved", and the grant is on
   McNeel's page (research 66). Nothing here needs it; recommended: leave it out.
4. **B2: Manifold's `test/models/*.obj`.** The repository is Apache-2.0, but several meshes are named
   after user bug reports, so their origin is unclear. Recommended: leave them out and re-state the
   cases as recipes.
5. **I1: the Yang et al. 2023 cases inside ssi-corpus.** The corpus is MIT OR Apache-2.0 and states
   that the paper is CC-BY. That statement was not confirmed on the ACM page. Six cases carry the
   paper's published data; 19 are reconstructions from figures. Recommended: confirm the paper's
   licence once, or exclude the 6 data cases until then.
6. **D9: the CAx-IF Recommended Practices.** These are public PDFs with only a copyright footer.
   Citing their definitions is fine. Recommended: never take their test models.

## 9. Not verified

- The exact chapter and example numbers of G1 and G2 (quoted from memory).
- Whether the projection papers in I5 publish exact answers.
- The licence headers of CORE-MATH `.wc` files other than `exp.wc`.
- The contents of ARM's `testcases/`.
- IBM FPgen's terms.
- QUADPACK's public-domain status.
- QI's licence text (the site refused the fetch).
- The MFCAD model count.
- NASA 3D Resources' terms per item.
- The free online edition of Patrikalakis and Maekawa.
