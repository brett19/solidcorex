A vertex on an edge whose curve is a tangent field can be given a tolerance of twice its distance from the origin. The gap between the vertex and the "end" of that curve is measured by evaluating the curve as if it returned a position; a tangent field returns a cross-tangent vector of length at most 1.

## Reproduction

Main; the kernel test `issue_2656_blend_regressions::a_tangent_field_edge_at_the_smooth_vertex_keeps_the_cap` (tests/blend) is the reproduction:

1. A 20 mm box; round the edge through (10, 20, 20) at r3.
2. Replace the curve of the round's end arc on x = 0, from (0, 17, 20) to (0, 20, 17), with a `TangentField` curve.
3. Round the edge through (0, 8.5, 20) at r1.

During step 3 the vertex at (0, 17, 20), 26.58 from the origin, goes from tolerance 4e-5 to **53.15**, and the one at (0, 20, 17) to 51.74. The body is 20 mm across. The UV gap at both corners was 0.

The test passes because it asserts `Ok` and 9 faces. The body it gets:

- fails `check_brep` at level 2 (edge and vertex tests, with offsets 51.7 and 143.6);
- is not a manifold solid: the oversized vertex makes the stitch merge it away and 5 edges stay open, which the finish accepts as an open result;
- has volume 0, where the rounded box's is 7957.72.

(The test's own comment says the body is not checked as a solid. The input already fails one edge test on the tangent-field edge before step 3.)

## Where

Two places measure a vertex-to-curve-end gap on an edge without regard to the curve's kind:

1. the corner loop of the face-level trim routine (`topology::face::p3`, `create_uv_trim_curves_body`), which raises the vertex to twice the gap;
2. `farthest_end_gap` in `make_edge_in_face_with_vu_with_curve_identity` (`topology/brep/p9.rs`).

## Tried

Skipping an edge whose curve has no positions at both places (a scratch change): the same test returns `Ok` with 9 faces, a manifold solid of volume 7957.723438, every vertex tolerance at most 4e-5, and level 2 reporting only the input's own edge finding. The blend test binary goes from 865 passed and 1 failed to 866 passed (the 1 was run with another unpublished change applied); every golden unit's output is unchanged.

## Not established

Whether a body made by the kernel's own operations, without a curve being swapped by hand, reaches either place with a tangent-field edge.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
