When a body with an offset-surface face is exported to its record, the spline written in the face's place can lie further from the face than the face's tolerance: 21 times further in the case below. The fit is aimed at half the body's largest tolerance, not at the face's own, and is checked by sampling.

This is separate from #13789 (store an offset face as its definition): it is about the stand-in that is written today.

## Reproduction

Main, the app's SMLib-form export (`FlatFromBrep`, `crates/smlib`), on the body of the regression recipe `examples/regressions/fins_joined_into_their_hub.json` (its Thicken makes one `OffsetSurface` face; body tolerance 2e-5, face tolerance 1e-5):

1. Export the body as the feature leaves it and compare the face with the spline in its record, at 410 points inside the trimmed face: largest normal distance 1.03e-6, mean 5.7e-7. Within the face's tolerance; volume 278.10468299 on the body, 278.10455154 on the record's import (4.7e-7 relative).
2. Shell that body (closed, 0.2 thick) without going through its record. The result has one `OffsetSurface` face (an offset of an offset). Export it and compare that face with the spline in its record, same sampling: largest normal distance **2.10e-4**, mean 1.33e-4, against a face tolerance of 1e-5. Volume 114.24154479 on the body, 114.27251103 on the record's import (2.7e-4 relative).

Measured with a temporary probe; step 2 operates on the kernel body directly.

## Reach

The app does not reach step 2 today: it starts every operation from the record's import, so it shells the spline of step 1, which gives spline faces, and that result stays within 6.4e-7 of the exact one. The export itself is the defect: a record whose face is outside the tolerance the record states for it.

## What the code does (read, not confirmed by experiment)

- The export fits a surface kind the record cannot hold to `approx_tol = zone / 2`, where `zone` is the largest zone tolerance in the body (`crates/smlib/src/core/convert.rs`, `max_zone_tol`), not the face's tolerance. After the shell some edge or vertex tolerance is large, which would put the target near the 2.1e-4 measured.
- The achieved distance it reports is the largest sampled distance, not a bound.

Whether that is the whole cause is not established.

## Also

`docs/native-brep.md` does not say that this export approximates any surface kind.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
