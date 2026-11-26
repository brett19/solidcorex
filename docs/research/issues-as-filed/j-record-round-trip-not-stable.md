Exporting a body to its record, importing the record and exporting again does not give the same record. For about a third of bodies it never settles: each further trip writes a record different from the last.

## Measured

Main, the app's path (`FlatFromBrep` then the record's import as `Body::smlib()` does it), at every record export while regenerating 63 designs (the recipes under `examples/`, the engine fixture designs and 11 STEP parts): 474 exports, then 5 more trips each. Temporary probe, not part of any change.

| | Bodies |
|---|---|
| Second record equals the first | 86 of 474 |
| Settles on a later trip (all records after it equal) | 226 |
| Still changing after 6 exports | 154 |

Fields that differ between the first and second record, in canonical order:

| Field | Bodies | Largest difference |
|---|---|---|
| UV curve points of plane and spline faces | 246 | |
| Plane frame: x axis / y axis / origin | 158 / 147 / 119 | origin moved up to 105 mm along its own plane |
| Face domains | 118 | |
| Cylinder radius | 96 | 2e-11 |
| Spline face control points | 17 | 1e-12 |
| Edge and vertex tolerance | 2 | 6e-8 |

In the bodies that never settle it is the UV curve data and the plane frames that keep changing.

The geometry stays where it was: mass properties after six trips differ from the first by at most 1.1e-14 relative. A plane with another origin and axes is the same plane.

## What the import does (read)

`crates/smlib/src/import.rs`, `plane()`: re-normalises the axes, forms y as z × x, and recomputes the face's range from the domain corners. Edge and vertex tolerances are taken as the larger of the entity's and the body's. The model extent is recomputed from the vertices. So values the record holds are derived again instead of taken as stored. Which of these keeps a record from settling is not established.

## Why it matters

- Saving a design that was opened and not changed can write a different file.
- A body built by an operation and the same body read from its record are not the same body, which is why every operation starts from the record's import (`crates/kernel/src/body.rs`, `from_smlib_record`), and why a live result is rebuilt before it is used again.

A record that is a fixed point after one trip would be enough for the first.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
