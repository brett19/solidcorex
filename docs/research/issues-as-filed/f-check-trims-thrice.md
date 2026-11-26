The first validity check of a body whose faces lack UV trim curves builds every missing trim three times, twice on copies of the body that are then discarded. Where a trim needs the certified deviation bound, that is most of the check's time.

## Measured

On main, `check_brep` at level 2 on a body read from a corpus file with the plain read (level 0 gives the same times). Temporary timing spans and counters, not part of any change; times are indicative (minimum of 5 runs, spans add 5 to 8%).

| Fillet/blade_hub_1.smb, after | Step | First check | Second check |
|---|---|---|---|
| a plain read (0.22 s) | n/a | 3.75 s | 0.034 s |
| `create_missing_uv_trim_curves` | 1.23 s, 120 trims | 0.111 s | 0.034 s |
| a volume computation | 1.36 s, 120 trims | 0.110 s | 0.034 s |

Fillet/blade_hub_3.smb and Fillet/cliff_1.smb show the same pattern (4.07 s and 3.73 s first checks).

Where the first check's time goes on blade_hub_1:

- 95 to 99% is the certified bound on a new trim's distance from its edge (`trim_raise_distance` → `trim_deviation_bound` → `trim_image_gap_bound_within`): about 3.83 s of 3.84 to 4.0 s. The tolerance pass is 4 ms and the edge, vertex and loop walks 76 ms.
- The check builds 360 trims for 120 edge uses: 120 in the first pointer pass (1.31 s), 120 in the second pointer pass (1.30 s), and 100 + 20 in the face walk and after (1.27 s). The two pointer passes build on a copy of the body (`clone_standing_for_self`) and discard it; the last builds on the body itself, which is why a second check builds none.
- Of the 100 full bound computations, 51 repeat an earlier one's exact inputs; those repeats take 1.97 s of 3.82 s (54 of 102 on blade_hub_3, 51 of 100 on cliff_1).
- Within one bound, the second of its three bisections evaluates again 21,954 of its 72,028 pieces with the same interval as a piece of the first (248 ms).

Bodies made by a Boolean of a ball or a cylinder with a box build 15 to 18 trims in a check and never reach the bound; their first check is about 10 ms.

## Proposed change

A check builds each missing trim once: the trims it will leave on the body are built before the passes that today build them on a discarded copy, or those passes take the trims from the body. The check's report, the trims and every tolerance must be unchanged; the evidence is the count of trim builds and bound computations per check before and after.

The repeated pieces inside one bound are a separate, smaller item.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
