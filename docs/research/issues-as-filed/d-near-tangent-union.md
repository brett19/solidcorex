A cylinder lying along a box's top face, sunk 1e-6 into it, united with the box gives a body that fails its validity check. The same union with the cylinder exactly tangent to the face, and the difference with the sunk cylinder, both give valid bodies.

## Reproduction

On main at 782ef9afc, default kernel, through the kernel's API:

1. `create_box` at the origin, 40 x 30 x 10.
2. `create_cylinder` of radius 5 and length 50, turned so its axis runs along X, with its base at (-5, 15, 15 - 1e-6). Its lowest line is then 1e-6 below the box's top face (z = 10).
3. `boolean_union(box, cylinder)` succeeds.
4. `check_brep` on the result reports one line:

   `SmEdge::Test #9 Bad Sector Ang Sequence: An Edge->Edgeuse/RadialEdgeuse pair of radial angles do not increase monotonically (offset=-180.00229, tolerance= 1e-12)`

| Case | Union | Difference | Intersection |
|---|---|---|---|
| Axis at z = 15 (tangent along a line) | valid | valid | valid |
| Axis at z = 15 - 1e-6 (sunk 1e-6) | **invalid, as above** | valid | not run |

Each Boolean here intersects three face pairs. No tolerance was raised during the union and no edge curve was rewritten (counted with temporary counters).

## What I have not established

Which edge the report is about, or why the radial order of the faces around it comes out reversed; whether a smaller or larger sink depth behaves the same. The sunk strip is 1e-6 deep and about 6.3e-3 wide, under the default tolerance of 1e-5 in depth but not in width.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
