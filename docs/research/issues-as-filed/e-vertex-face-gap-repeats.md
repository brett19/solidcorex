Within one validity check, the gap between a vertex and a face is computed again for vertex uses whose value was already computed in the same check with the same inputs. On a second check of an unchanged body every computation is a repeat.

## Measured

On main, `check_brep` at level 2 (level 0 gives the same numbers), logging every computation in `calc_vertex_face_gap` with the inputs it reads (vertex point, seed, the UV trim end, the surface, the tolerances) and its result bits. Temporary instrumentation, not part of any change.

| Body | Check | Computations | First for that vertex use | Repeat, same inputs, same result | Repeat, an input changed |
|---|---|---|---|---|---|
| 40 x 16 x 10 box joined to an R8 x 20 cylinder | 1st | 114 | 60 | 44 | 10 |
| 10 x 10 x 10 box joined to an r4 x 10 cylinder | 1st | 159 | 84 | 47 | 28 |
| Fillet/blade_hub_1.smb | 1st | 919 | 468 | 76 | 375 |
| A ball less a box corner | 1st | 55 | 28 | 9 | 18 |
| A box with one edge filleted | 1st | 102 | 60 | 42 | 0 |
| The same five bodies | 2nd | 101 / 134 / 936 / 44 / 84 | 0 | all | 0 |

No repeat with the same inputs gave a different result (1,517 comparisons). In the second check the first computation for each vertex use equals that use's last value from the first check bit for bit. On blade_hub_1's second check these are 936 of the check's 2,594 UV point solves.

Each computation is one local point solve and one surface evaluation, plus two surface drops where the edge use has no trim. None built a surface cache.

## Cause

No caller forces a recalculation. `calc_vertex_face_gap` itself clears the cached gap of every use of the same vertex on the same face, including the use on the mate face use, and stores only its own. Two mate uses therefore keep clearing each other, and `is_within_xsect_tol_3d_of_faces` (topology/vertex/corner.rs), called per vertex by the check, finds the cache unset and computes again. Nearly all the same-input repeats are of this kind. This matches the upstream routine, where the copy to the sibling uses is commented out.

The repeats where an input changed follow the check's own creation of UV trims (15 to 360 per body here); on blade_hub_1, 305 of those 375 gave bit-identical results and 70 differed at about 1e-13.

## Proposed change

A computation stops clearing the cached gap of the use on the mate face use, whose inputs it does not change. The clearing of sibling uses on the same face use stays: the tolerance pass reads the start use's stored gap after computing the end use, and on a seam those are siblings.

Evidence required: the check report and every stored gap value unchanged on the corpus and on Boolean and fillet results; the computation counts above before and after.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
