# 18 - edge-to-next-edge gap mismatch, Fillet/open_shell.smb (read-only code investigation)

Tree: sc-wt/cache-a2-gaps, solidcore/crates/solidcore/src (paths below relative to it). Nothing was run.
V = verified in code, I = inferred.

## Verdict: (a) a real stale cache. (b) is refuted. The write that made it stale is NOT pinned down.

## Why not (b) (V)
- Production fill: `edge_ccw_edge_gap` (topology/edgeuse/p1.rs:946-960) calls `calc_edge_ccw_edge_gap(k,b,eu,false,true)`.
  Probe calls it with `(true,false)` (p1.rs:1062). The 4th arg is `want_output`, the 5th `cache` (not "force").
  Both go through the same body: same `edge_t` from `edge.interval` and orientation (p1.rs:1087-1100) and the same
  `EdgeEdgeGap::with` (p1.rs:1103, 1110). `force_calc` only decides whether to recompute.
- `end_points` (heal/gap.rs:1045-1111) evaluates the two edges' CURVES at edge_t0/edge_t1 (not vertices), so the
  gap depends only on the curves, the intervals and the use orientations. No option differs, so the two numbers
  answer the same question.
- The fill stores the parameters with the gap (`set_edge_edge_gap`, gap.rs:449-468). For 2 and 57 stored and fresh
  parameters are identical (6.5997 / 0.0) yet the gap went 0.0 -> 9.48e-6. So the curve, or its parameterisation,
  under unchanged parameters changed after the fill. For 7 and 52 the CCW edge's end parameter went 1.0 -> 6.5997:
  its interval, its use's orientation, or which edge the CCW use sits on changed after the fill.

## Who fills and who reads (V)
- Only reader/filler in production: heal/heal_data/p2.rs:471 (`eu1::edge_ccw_edge_gap(k,b,eu,false)`, per edge,
  under `set_props_gap`), plus p2.rs:269 (`ee_gap_length`, reads the stored `gap_3d`). Every other hit of the field
  is the constructor (topology.rs:627) or a reset (edge.rs:397/400, brep/p2.rs:2383, brep/p5.rs:3735, p1.rs:1077).
- Runs at read: `read_from_data` (brep/p7.rs:801-821) -> `make_topology` -> `heal_made` -> `heal_brep(All)` when
  `healer_enabled && (data.healer_version() < HEALER_VERSION || always_heal)` (brep/p10.rs:526-528). The UV-trim
  check/rebuild runs after it (p7.rs:815-820). The cached value is filled by the heal, so "legacy healer on a body
  whose caches are unset" is CONFIRMED as the only production reader. The "untrusted arrays" half was not checked.

## What the PR covers (V)
- `edge::set_interval` (topology/edge.rs:392-405) clears, per use of the edge: its own `edge_ccw_edge_gap`, the CW
  neighbour's `edge_ccw_edge_gap`, `max_edge_face_gap`, and the vertex use's `vertex_edge_gap`.
- Every interval write I found in the heal/read paths goes through it: brep/p1.rs:2081/2108 (vertex drop),
  edge::reverse_orientation (edge.rs:2380), `replace_3d_curve` (edge.rs:2885, used by heal/tolerance_defects.rs:689,1199
  and face/p1.rs:2916), edge split p9.rs:1247-1329. The heal never writes `edge_interval` back to the body
  (heal_data/p2.rs:3506-3513 edits the props copy only). Direct `.interval =` writes are build-time only
  (brep/p9.rs:1683, before the heal) or constructor/test code (brep_constructor.rs:1163).
  => no interval writer in these paths bypasses the setter, so the interval route alone does not explain it.

## Writes that do NOT reset the cache (V that they bypass; I that one is the culprit)
1. In-place curve trim: topology/face/p1.rs:1963-1965 `edge.curve.as_mut().trim(k,&mut ivl,..)` re-parameterises or
   re-trims the edge curve with no interval/cache reset. Best match for 2/57 (same parameters, new curve shape) and
   for the `1.0 -> 6.5997` jump if the trim re-scales. Not shown to run on this file.
2. `edgeuse::set_orientation` (edgeuse/p1.rs:324) and `ccw`/`cw` relinking (brep/p4.rs:572/577, p9.rs:1385-1389
   edge split) clear nothing; a CCW edge use whose orientation or edge changed moves `ccw_edge_t` between min/max.
3. `edge::set_curve` (edge.rs:2909-2990) replaces the curve and resets nothing; only `replace_3d_curve` follows it
   with `set_interval`. Direct `.vertex.point =` writes (heal/stitch/p1.rs:3056 is test code; vertex/corner.rs:370/372)
   do not reset it either, but the gap uses curves, not vertex points.
Which of these ran on open_shell.smb is unknown without a trace.

## Tolerance link (I)
9.48e-6 is under the default 1e-5, so the fresh gap is not itself a defect; it is probably just the curve-to-curve
distance after the change. Nothing in the code ties it to the heal's tolerance raise or to the 3.92e-6 trim-start
shift; those touch other caches (edge_face / UV trims). Treat the link as unproven.

## Can a stale value change an answer? (I, with V parts)
- Consumers: `max_gap_3d_edge_edge` (the largest-gap use per edge) and the loop gap histogram (p2.rs:475-492),
  which feed the healer's tolerance sizing. A stale 0.0 for a true 9.5e-6 could only matter if the healer compares
  against a threshold between 0 and 9.5e-6 (i.e. it under-reports a gap by < 1e-5, below the default tolerance), and only for legacy-heal
  runs, where the cache is refilled on the fly when unset. Nothing outside the heal reads it. Impact: very low, not zero-proven.

## Recommended reset (I)
Whichever of the three writes above is confirmed; the cheapest check is a trace in the probe of open_shell.smb
that logs the fill-time and final interval/orientation/curve of the edges of uses 2, 7, 52, 57 and their CCW uses.
The open PR does NOT cover 1-3 (it covers only `set_interval`). If the culprit is the in-place trim, reset the
caches of the edge's uses and their CW neighbours in that loop (face/p1.rs:1963) or route it through a helper
that does.
