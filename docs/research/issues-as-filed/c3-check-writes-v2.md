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

The gap caches and the other writes are different things and are treated differently.

**The gap caches stay.** They are derived values, and the passes that follow a check read them. Measured: a validate-only check, including the level-0 check inside every outward-solid proof, is what fills a body's gap caches for the operation that follows. With the check storing nothing, the tolerance update after a Boolean on a box joined to a cylinder computes 42 vertex-to-edge, 84 vertex-to-face and 42 edge-to-face gaps where today it computes 0, 66 and 0. Removing the storing would make every operation on a proved body recompute them, for no change in any answer. What these caches need is sound invalidation (#20939 and the writes like it), and later a way to be filled that does not need a writable body.

**The writes of meaning go**, in steps, each on its own:

1. A check never creates a UV trim on the body it was given: where a face lacks trims it works on a private copy, as `check_face` already does for a face with no trims at all.
2. A turned-back end is reported, not repaired; the repair stays in heal and in the explicit fix path.
3. `validate_only` becomes an argument of the pass instead of a flag set and restored on the body.

Step 2 changes what a check leaves behind on bodies with a turned-back end. With these done, the only thing a validate-only check writes is derived values.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
