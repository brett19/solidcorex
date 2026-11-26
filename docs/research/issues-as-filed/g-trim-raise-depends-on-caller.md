The same UV trim curve on the same body leaves a different edge tolerance, and then vertex tolerance, depending on which routine creates it first.

## Reproduction

On main, read Fillet/blade_hub_1.smb with the plain read. Then either:

- run `check_brep` (level 0 or 2): edge 19's tolerance afterwards is 3.8597e-5; or
- call `create_missing_uv_trim_curves` first: edge 19's tolerance is 4.1805e-5.

The trim curves and intervals are identical in the two cases. The same kind of difference shows on blade_hub_2, blade_hub_3, cliff_1, cliff_2 and revolution_mold; on blade_hub_1 the raise differs on 99 of 120 trims between the two routes, and 28 edges and 43 vertices end with different tolerances.

## Cause

`get_or_create_uv_trim_curve` raises the edge through `trim_raise_distance(.., created.max_dist_to_surf)` (topology/edgeuse/p3.rs). That distance decides whether the edge raises at all and is the raise wherever the proved bound is not used. Its value depends on the caller's `want_deviation` argument:

- without it, the distance is `sqrt(dist² + dev²)`: the drop distance and the trim's own fit deviation (`set_outputs` and `finish`, edgeuse/p3.rs);
- with it, the distance is `dist` alone.

`want_deviation = false`: the face routine (`face::p3::create_uv_trim_curves`), `create_missing_uv_trim_curves` (called by the record export) and most other callers. `want_deviation = true`: `normalized_evaluate`, which is what the validity check's walk uses, so it builds most of the trims a first check leaves on a body.

A flag that asks for an extra output changes what is written to the body.

## Which is right

`docs/validity-and-tolerances.md` §8 reads a tolerance as a claim that the exact boundary lies within it, sound "exactly when τ bounds the distance from the stored entity to the exact one". By that reading the raise must cover the trim's whole distance from the edge, fit deviation included, so the `want_deviation = true` path under-covers. Neither path is an enclosure; where the proved bound applies, `trim_raise_distance` already uses it.

## Proposed change

The raise does not depend on `want_deviation`: both paths raise by the same distance, the one that includes the fit deviation. Tolerances left by a check's walk rise on the bodies above (goldens will move, each with its derivation). Under #21105 a check will stop leaving trims on the body at all, which removes the commonest way to reach the smaller raise but not the dependence itself.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
