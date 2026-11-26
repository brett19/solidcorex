An edge use caches its gap to the next edge use, and a vertex use caches its gap to its edge. Changing the edge's interval does not clear either cache, so later reads return the gap measured on the old interval.

## Reproduction

On main at 782ef9afc, default kernel, on a box joined to a cylinder.

| Gap, after shrinking one edge's interval by 10 % at each end with `topology::edge::set_interval` | Body that had cached the gap before the edit | Identical body that had not |
|---|---|---|
| `edgeuse::p1::edge_ccw_edge_gap` | `0.0` | `4.0` |
| `vertexuse::vertex_edge_gap` | `0.0` | `4.0` |

A forced recalculation on the first body also gives `4.0`. After `check_brep(Level2)` on both, the stored gap fields still differ on one edge use and two vertex uses.

This reproduction calls the setter directly. I have not yet shown an operation that changes an interval after the gaps were cached and then reads them; that is the next thing to establish, since it decides how far the stale value reaches.

## Cause

- `edge::set_interval` (`topology/edge.rs`) sends no notification and resets nothing.
- `edge_ccw_edge_gap` (`topology/edgeuse/p1.rs`) and `vertex_edge_gap` (`topology/vertexuse.rs`) compute only when their cache is not initialised. No notification resets `edge_ccw_edge_gap` at all.

## Proposed change

Every edit that changes what a cached gap measures (the edge's interval, its curve, its tolerance, a vertex's point, a UV trim curve) resets the gaps that depend on it, in the setter.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
