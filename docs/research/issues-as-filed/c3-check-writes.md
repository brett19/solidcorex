`docs/validity-and-tolerances.md` states, as invariant 11: "A check is a function of the body: running it twice gives the same report, and the validate-only pass changes nothing (`validate_only`)." Today a validity check changes the body it checks.

## Measured

On main at 782ef9afc, `check_brep(k, b, CheckLevel::Level2)` on bodies made by one Boolean, comparing the body before and after the check:

| Body | Fields of the body that differ after the check |
|---|---|
| A ball less a box corner | 76 of 207: UV trim curves created on 6 edge uses; 28 edge-to-face gap caches, 28 vertex-to-edge and 14 vertex-to-face gap caches filled |
| A box joined to a cylinder | 153 of 432: UV trim curves created on 5 edge uses; 60 + 60 + 28 gap caches filled |
| A plate with a hole | 148 of 432: 60 + 60 + 28 gap caches filled |

No tolerance changed on these bodies and no later answer (volume, area, a second check) differed. A repeat of the check on a body with curved faces also builds its surface cache trees again each time.

## Where the writes come from

From reading the code and from changing `check_brep` to take the body by shared reference and following the compiler's errors:

- **Gap caches.** The gap calculators store their result on the edge use or vertex use when the cache is unset, and the tolerance pass and later tests then read the stored value.
- **UV trim curves.** The walk over faces creates a missing trim. The raise that can come with a new trim is suppressed only by `validate_only`, which is set around the tolerance pass and not around the walk.
- **A repair inside the check.** `fix_reversed` / `fix_reversed_on` move a control point of an edge curve or UV trim whose end turns back, and later tests read the repaired curve.
- **A missing NURB form** is made on parabola and hyperbola curves by their own validity routine.
- `validate_only` itself is set and restored on the body.

`solidcore-ops` works around this by copying the whole body before every check.

## Proposed change

In steps, each on its own:

1. The check computes gaps as values for its own use and does not store them on the body.
2. A check never creates a UV trim on the body it was given: where a face lacks trims it works on a private copy, as `check_face` already does for a face with no trims at all.
3. A turned-back end is reported, not repaired; the repair stays in heal and in the explicit fix path.
4. With those done, `check_brep` in validate-only mode takes the body by shared reference, and `solidcore-ops` checks the body itself without copying it.

Step 1 can change a report only where a stored gap was stale. Step 3 changes what a check leaves behind on bodies with a turned-back end.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
