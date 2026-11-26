# 40: why the live fins body and its record measure different volumes

Design `fins_joined_into_their_hub`, the 8-face body from Thicken1 (hash 9790652661782265372).
Live volume 278.1046829948353; rebuilt from its record 278.1045515436817 (relative 4.7e-7).
Probe: scratch branch `cache-finsvol-m` (off `cache-massreload-m`), run on the build server. Raw output is in
`count-kit/40-data/fins.jsonl`. Labels: MEASURED (this probe or 39's data), READ (code), INFERRED.

## Cause

**(c) A real geometry difference, in the representation.** Live face 7 is an `OffsetSurface`: the
exact definition that Thicken builds. The record holds a B-spline in its place, and a rebuild
measures that B-spline. Faces 0–6 are the same in both bodies (MEASURED). Each body's volume is
right for its own surfaces (MEASURED: it does not change with accuracy, origin, box or kernel). The
two values differ because the surfaces differ by much less than the tolerance. Spread over face 7's
area of 231.76 mm², the 1.31e-4 mm³ gap is a mean normal offset of 5.7e-7 mm. That is 6% of the
face tolerance of 1e-5 mm (INFERRED from the MEASURED numbers).

## Entity-by-entity comparison (MEASURED, `info_live` vs `info_reb`)

- **Faces 0–6.** These are B-splines in both bodies. Degrees, knots, multiplicities and control
  points are bit-identical, and so are uv_domain, the natural domain and the tolerance (1e-5). The
  only difference: the live body carries no weight array, while the record has explicit weights,
  all 1.0. That is the same rational-free surface.
- **Face 7.**
  - The live face's kind is `OffsetSurface(...)` and the record's is `BSpline(...)`.
  - uv_domain [0,1]×[0.31430,0.68570] and the tolerance match.
  - The face boxes do not match:
    - live tight box: x −4.525..4.890, y −0.036..30.041, z 1.312..9.848;
    - record tight box: x −3.622..4.431, y 0.002..30.003, z 1.785..9.348.
- **Edges.** There are 18 in each body but in a different order. Tight boxes and intervals agree
  once edges are matched (the intervals swap places between [0,1] and arc length). No edge kind
  or tolerance differs.
- `info_live_after`, taken after every measurement, is byte-identical to `info_live`, so measuring
  does not mutate the body.

## Hypotheses tested

1. **Hidden state: eliminated.**
   - The `out_of_bounds_enabled`/`continues_past_domain` flags, uv_domain and tolerances are equal
     on every face. Face 7's flags could only be read on the record side, since the live face 7 is
     not a B-spline.
   - A direct re-measure and the "after" snapshot repeat the live value bit for bit (MEASURED).
   - Report 39 already showed that copies, a clone of a clone, a reversed order and a trim-stripped
     live body all keep the live value (MEASURED in 39's data).
2. **Entity comparison: done (above).** The only geometric difference is the kind of face 7.
3. **Kernel state: eliminated.** On a fresh kernel the live body gives 278.1046829948353 and the
   rebuilt body 278.1045515436817, the same as on the pool kernel (MEASURED).
4. **Order, copies and trims: eliminated** (report 39; MEASURED there).
5. **Box and origin: eliminated** (this was the lead from 39's data).
   - The live tight box is 0.5 mm larger, but swapping boxes leaves each body's own value:
     `m_live_rebbox` = 278.10468299483534 and `m_reb_livebox` = 278.1045515436817.
   - Moving the origin by 1 mm along x, y or z, or to the world origin, moves each value by at most
     6e-13.
   - Accuracy from 1e-9 to 1e-12 moves each value by at most 1e-13.
   - So the integrator has converged and each surface set is closed: this is not integrator error
     (MEASURED).
   - The larger live box comes from face 7's tight box. The face-extreme search splits cells on the
     exact subpatch only for B-spline surfaces; an offset surface is solved on itself (READ,
     `query/face/p3.rs` `face_directed_max_with`, `by_patch`). That the offset box is looser for this
     reason is INFERRED.
6. **Which value is right.**
   - For the exact definition, the live value is right. 278.1046829948353 is the converged volume
     of the Thicken offset surface. The record's value is the converged volume of its certified
     B-spline stand-in, within tolerance (INFERRED from 5).
   - The tessellation cannot decide at this level. It meshes the record body, and its volume
     converges only to about 1e-5 relative (MEASURED):

     | chord | volume |
     |---|---|
     | 1e-3 | 278.0866 |
     | 3e-4 | 278.1001 |
     | 1e-4 | 278.1038 |

     A Richardson extrapolation gives about 278.105 ± 1e-3. The chord-3e-5 row (278.073 from 470k
     triangles) breaks the trend, presumably a triangle budget, and is unreliable.
   - In 39's split, the difference is 7.7e-5 in the face integrals plus 5.4e-5 in face 7's gap
     closure strip. The record's B-spline face 7 sits farther from its edges than the offset
     surface does (closure −5.4e-5 vs −3.5e-8). These per-face splits depend on the origin; only
     the total is meaningful (MEASURED in 39's terms).

## Census (MEASURED, report 39's `out2.jsonl`)

Across the 63 designs, 474 bodies have both values. Only one has a relative live-vs-rebuilt volume
difference above 1e-12: this body, at 4.7e-7. Only 2 have different tight boxes: this one (0.499)
and `front_triangle_hull_triangle` (7e-15, rounding). So the offset-definition-vs-record gap shows
up in volume only here.

## Consequence for the cache plan (INFERRED)

A record round-trip is not identity for a body that carries a surface definition the record cannot
hold. Such a body's measured properties change within tolerance on reload, and its tight box can
change by far more than tolerance (0.5 mm here). Either compare cached mass values with a tolerance
scaled by face tolerance × area, or make the record carry the offset definition.

Scratch: branch `cache-finsvol-m` (local and the build server side ref only; never pushed to GitHub or
the fork). The build-server worktree has been removed.
