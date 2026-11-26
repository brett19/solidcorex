A B-rep that stores "use the default" for its model size estimate answers with whatever the kernel's context holds, and a scaling transform of any body writes that context. So scaling one body changes the model size estimate reported for a different body.

## Reproduction

On main at 782ef9afc, default kernel.

1. Create a 100 x 100 x 10 box, X. `geom::tol::brep_model_size_estimate` of X is `50.0`.
2. Create another box, Y, and scale it by 1000 with `topology::brep::mirror::transform_matrix(.., scale_tolerances = true)` (what `solidcore-ops` `Body::transform` always passes).
3. Ask X again: `17320.50811032979`.

On a fresh kernel, where only X was made, step 3 answers `50.0`. Nothing touched X between the two reads.

After step 2 the kernel's context also holds `zone_tol_3d = 1e-4` in place of `1e-5`. The values stay until the next `new_brep` puts the defaults back, which most operations do, so the window is: a scaling transform, then a read or operation on another body before any body is created.

In the same run, 1,451 other answers about X were unchanged (tolerances, volume, area, validity lines, a UV classification grid, a round and its resulting body), so the dependence is on this value and its readers.

## Cause

- `Brep::set_this_model_size_estimate` writes the kernel's context as well as the body (`topology/brep/p1.rs`); the scaling transform calls it (`topology/brep/p5.rs`), as does heal's `fix_tol_sizes`.
- `Brep::this_model_size_estimate(k, true)` falls back to the context for a body that stores the default.
- Readers of the fallback: heal (`heal/heal_data/p2.rs`), crease splitting (`topology/brep/p8.rs`), and the keys of the curve and face cache memos, where a changed value is a miss, not a wrong result.

The two kernel pools do not reset these context values between calls. #13672 names the context as one of the things that make a result depend on call history.

## Proposed change

The model size estimate, the large-to-small ratio and the zone tolerance a body is measured with belong to that body. A body that stores the default answers with the kernel's configured default, never with a value another body left behind, and a transform of one body writes only that body.

## Expected result changes

Cases where a scaling transform or a heal preceded a read of another body's estimate without a new body in between. No goldens row is known to move; to be measured.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
