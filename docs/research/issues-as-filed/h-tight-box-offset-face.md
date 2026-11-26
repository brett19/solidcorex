The exact bounding box of a body with offset-surface faces can be wrong and very slow: on four corpus bodies it is 10 to 13 mm too large in z and takes 14 to 89 seconds per call, where other bodies take under 0.1 s.

## Measured

On main, `query::brep::calculate_tight_bounding_box` on bodies read from the corpus, after `create_missing_uv_trim_curves`. Temporary timing spans, not part of any change; single calls, times indicative.

| Body | Faces | Time per call | z range returned | z range of the solid |
|---|---|---|---|---|
| Fillet/vase1.smb | 19 (4 offset) | 14 to 22 s | ±89.6703842 | ±76.2 |
| Fillet/vase2.smb | 11 (4 offset) | 80 to 89 s | ±89.6703842 | ±76.2 |
| Fillet/vase3.smb | 19 (4 offset) | 70 s | ±89.6703842 | ±76.2 |
| Fillet/vase.smb | 11 (4 offset) | 36 s | z min −111.61 | offset faces reach −88.90 at most |

The solid's range is from sampling each offset face's trimmed region (about 11,000 points per face): no point exceeds 76.2 on vase1 to vase3. The 25 other corpus files, primitives, Boolean and fillet results take 0.088 s or less.

Nothing is kept between calls, so each call costs the same. A mass-property call computes this box (`mass/integrator/properties.rs`): on vase1, 14.59 s of a 14.65 s mass call. The box's size scales the mass integration's thresholds, so a box that is too large loosens them.

## Cause

All of the time is in the four offset faces of each body (6 to 37 s per face; every other face under 3 ms).

- The face's `uv_domain` is the offset surface's whole domain, while its trim covers only the middle: on vase1 face 8 the domain is [0,1] × [0,1] and the trim spans [0.19, 0.92] × [0.18, 0.82]. The untrimmed surface's corners reach z ±89.67.
- The directed-extremum search (`cell_directed_max`, query/face/p3.rs) solves each cell on the whole surface. An offset surface is not a B-spline, so the hull and monotone tests that prune cells do not apply. Nearly every solution lies outside the trim and classifies as unknown (575,693 of 575,811 on vase2), so the cell is never decided and is divided to the depth and division limits: about 16,385 cells per direction; 22 of 24 offset directions on vase2 hit the limit.
- When a cell hits the limit, the search takes the solver's value for that cell, a point of the surface outside the face. That is where ±89.67 comes from.
- The global solve inside the cells is 88.6% of the time (575,811 solves on vase2); classification is about 9%, every one through the fallback path.

## Tried

Restricting the search domain to the UV box of the face's outer-loop trims: vase1 13.6 s to 0.036 s, vase2 79.7 s to 10.6 s, vase3 69.8 s to 10.6 s, vase 36.3 s to 17.7 s, with z of vase1 to vase3 becoming −76.2 / 76.2001. It is not a fix as it stands: cells that still hit the limit overshoot (76.2001236 against 76.2), and on vase.smb the result is still 12 mm below the sampled faces.

## What a fix needs

- A cell that reaches the limit must not contribute a point that is not on the face.
- The search on a face must be bounded by the face, not by its surface's whole domain.

Both change the returned box on these bodies (towards the true one), so golden rows that record it will move, each with its derivation.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
