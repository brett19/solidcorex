An edge use caches its largest edge-to-face gap. A scaling transform moves the body's curves and surfaces without clearing that cache, so after a scale by s every cached edge-to-face gap is still the value measured before the scale: wrong by a factor of s. Readers that trust the cache then check and set tolerances against the wrong gap.

## Reproduction

On main at 782ef9afc, default kernel. Two bodies whose intersection curves are not exact on both surfaces: two crossed cylinders joined, and a ball drilled off-centre. Each is built with a Boolean, has its gaps cached by the tolerance pass that follows a Boolean in `solidcore-ops` (`update_tolerances_after_boolean`), and is then scaled with `topology::brep::mirror::transform_matrix(.., scale_tolerances = true)`. A twin body is treated the same way and has its gap caches cleared after the scale.

Crossed cylinders, largest edge-to-face gap:

| Scale | Cached after the scale | Recalculated |
|---|---|---|
| 1000 | 4.264e-6 | 4.264e-3 |
| 0.001 | 4.264e-6 | 4.264e-9 |

What follows from the stale value:

- **Scaled down by 0.001 (and by 0.0001):** `check_brep` on the scaled body reports "SmBrep::Test #4 Bad Gap: Edge/Surface gap across Edgeuse larger than tol (offset=2.6540623e-06, tolerance=1e-06)". The offset it reports is the gap from before the scale; the body's real gap is a thousand times smaller. The tolerance update then raises four edge tolerances and four vertex tolerances to about 5.3e-6 and 2.3e-6, where the twin keeps 1e-6.
- **Scaled up by 1000:** the tolerance tightening a Boolean applies to its operands (`boolean::operand_tolerances`) regrows edge tolerances from the cached gaps, leaving 5.3e-6 on an edge whose real gap is 2.65e-3. A check of a copy of that body then reports "Bad Gap ... (offset=0.0026540623, tolerance=5.3081247e-06)". The twin's tolerances and check differ.

The off-centre drilled ball behaves the same way at all three scales. Bodies whose gaps are zero to rounding (a box joined to a cylinder, a ball less a box corner) show the stale cache but no changed tolerance or verdict.

## Cause

- The transform moves vertices through the point setter, which resets the vertex-use gap caches, but transforms curves and surfaces after a `PreEdit` notification only, which does not reset `Edgeuse::max_edge_face_gap` (`topology/brep/p5.rs`, `transform_in`).
- `edge_face_gap` and the tolerance passes read the cache when it is initialised and recompute only when asked to force it.

Related, read in the code but not reproduced through an operation: `topology::edge::set_interval` resets no gap cache (calling it directly leaves `edge_ccw_edge_gap` and `vertex_edge_gap` answering 0.0 where a fresh computation gives 4.0), and some edits write `Edge::interval` without the setter. `edge_ccw_edge_gap` has no reader outside the legacy healer, which runs on bodies whose caches are unset.

## Proposed change

A transform with a scale other than 1 clears (or scales) the cached edge-to-face and edge-to-edge gaps of every edge use it moves. `edge::set_interval` clears the gap caches that depend on the interval, so the setter is correct on its own.

## Expected result changes

Bodies scaled after their gaps were cached: tolerances and validity verdicts follow the real gaps. Each moved row needs its reason.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
