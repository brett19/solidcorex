# Check gap storage: counts and production sequences (step 1 of #21105, parked)

Date 2026-11-23. Branch `cache-c3-check-gaps`: local only, nothing beyond the claim commit is on the fork. Draft PR #21106 is unchanged.

Decision (coordinator): do not remove gap storage from validate-only checks. On main a check is what warms a body's gap caches for the operation that follows. Removing that makes the following operation recompute the gaps, on the common path, and no answer changes.

## (a) Counts

### Where the counters sat
The counters are temporary and live only in the scratch worktree; they were never committed to the branch and never pushed. They are in `src/zz_probe.rs` (thread-local, active only between `start()` and `stop()`). Each counter is called once per computation at the top of the four calculators, after their preconditions and before the result is stored:
- vertex/edge: `topology/vertexuse.rs` `calc_vertex_edge_gap`, just before "set output" (`computed(0)`)
- vertex/face: `topology/vertexuse.rs` `calc_vertex_face_gap`, after `compute_uv_point` (`computed(1)`)
- edge/face: `topology/edgeuse/p1.rs` `calc_max_edge_face_gap`, before "set Edge/Face gap" (`computed(2)`)
- edge/ccw-edge: `topology/edgeuse/p1.rs` `calc_edge_ccw_edge_gap`, before "set m_sEdgeCCWEdgeGap3d" (`computed(3)`)

They count every computation, whether or not it is cached (cache=false calls count too). Vectors read [vertex/edge, vertex/face, edge/face, edge/ccw-edge].

### The bodies
- Booleans, built cold (no gap cache initialised):
  - bar_into_post: box 40x16x10 united with a cylinder of radius 8 and height 20 at (40,8,0)
  - box_into_cylinder: box 10x10x10 united with a cylinder of radius 4 and height 10 at (10,5,0)
  - ball_less_corner: sphere of radius 5 less a box 10x10x10 at (1,1,1)
  - plate_with_hole: plate 40x30x10 less a cylinder of radius 6 and height 20 at (20,15,-5)
- 28 upstream test files, read from `upstream/TestFiles/pt_TestFiles` with the read's trim-cover raises undone. They arrive with their vertex/edge, edge/face and edge/edge caches initialised and some vertex/face caches initialised; blade_hub_1, for example, arrives with [468,114,468,468] caches initialised.

### The variants
- main: 8865da5c27.
- head, fresh: the branch with a check-owned table, ignoring every body cache during a validate-only check.
- head, warm: the same, but a body cache that is already initialised is read (the head's code plus a scratch-only toggle).

The report text was identical on main and on the head, for every body, in both variants (hash of the `BrepCheck` debug string). Two checks in a row gave the same report everywhere. Failures: test8 has 4 lines and fillet_ts has 1; every other body has 0.

### Item 1: one validate-only check (Level 2)

| body | main | head, fresh | head, warm |
|---|---|---|---|
| bar_into_post | [29,114,30,0] | [29,114,30,0] | [29,114,30,0] |
| box_into_cylinder | [42,159,42,0] | [42,159,42,0] | [42,159,42,0] |
| ball_less_corner | [14,55,14,0] | [14,55,14,0] | [14,55,14,0] |
| plate_with_hole | [28,114,30,0] | [28,114,30,0] | [28,114,30,0] |
| Fillet/blade_hub_1.smb | [0,919,0,0] | [234,936,378,0] | [0,919,0,0] |
| Fillet/blade_hub_2.smb | [0,637,0,0] | [162,648,262,0] | [0,637,0,0] |
| Fillet/blade_hub_3.smb | [0,996,0,0] | [252,1008,408,0] | [0,996,0,0] |
| Fillet/bottle.smb | [0,305,0,0] | [80,321,80,0] | [0,305,0,0] |
| Fillet/box_extrusion2.smb | [0,160,0,0] | [41,169,42,0] | [0,160,0,0] |
| Fillet/case_ext.smb | [0,32,0,0] | [12,35,9,0] | [0,32,0,0] |
| Fillet/case_int.smb | [0,18,0,0] | [4,20,5,0] | [0,18,0,0] |
| Fillet/cliff_1.smb | [0,919,0,0] | [234,936,378,0] | [0,919,0,0] |
| Fillet/cliff_2.smb | [0,637,0,0] | [162,648,262,0] | [0,637,0,0] |
| Fillet/corner_box.smb | [0,113,0,0] | [30,120,30,0] | [0,113,0,0] |
| Fillet/cyl_cyl_2191.smb | [0,220,0,0] | [53,231,70,0] | [0,220,0,0] |
| Fillet/cyl_cyl_shell.smb | [0,57,0,0] | [14,60,14,0] | [0,57,0,0] |
| Fillet/nurb_box1.smb | [0,67,0,0] | [18,72,18,0] | [0,67,0,0] |
| Fillet/open_shell.smb | [0,117,0,0] | [42,123,31,0] | [0,117,0,0] |
| Fillet/shell_fillet2.smb | [0,53,0,0] | [20,56,14,0] | [0,53,0,0] |
| Fillet/sphere_dif_box.smb | [0,106,0,0] | [28,113,28,0] | [0,106,0,0] |
| Fillet/sphere_plane.smb | [0,22,0,0] | [5,25,6,0] | [0,22,0,0] |
| Fillet/test1.smb | [0,22,0,0] | [4,25,6,0] | [0,22,0,0] |
| Fillet/test8.smb | [0,38,0,0] | [9,42,10,0] | [0,38,0,0] |
| Fillet/torii_fillet.smb | [0,251,0,0] | [66,264,66,0] | [0,251,0,0] |
| Fillet/torii_unioned.smb | [0,37,0,0] | [5,41,10,0] | [0,37,0,0] |
| Fillet/vase1.smb | [0,367,0,0] | [96,389,108,0] | [0,367,0,0] |
| Fillet/vase2.smb | [0,187,0,0] | [48,206,80,0] | [0,187,0,0] |
| Fillet/vase3.smb | [0,386,0,0] | [96,423,164,0] | [0,386,0,0] |
| Shell/ShellBrep_983.smb | [0,228,0,0] | [60,240,60,0] | [0,228,0,0] |
| Solids/fillet_ts.smb | [0,127,0,0] | [68,136,34,0] | [0,127,0,0] |
| Solids/revolution2.smb | [0,494,0,0] | [129,528,132,0] | [0,494,0,0] |
| Solids/revolution_mold.smb | [0,672,0,0] | [176,704,196,0] | [0,672,0,0] |

### Item 3: second check in a row (count of the second)

| body | main | head, fresh | head, warm |
|---|---|---|---|
| bar_into_post | [0,101,0,0] | [29,111,30,0] | [29,111,30,0] |
| box_into_cylinder | [0,134,0,0] | [42,152,42,0] | [42,152,42,0] |
| ball_less_corner | [0,44,0,0] | [14,51,14,0] | [14,51,14,0] |
| plate_with_hole | [0,107,0,0] | [28,114,30,0] | [28,114,30,0] |
| Fillet/blade_hub_1.smb | [0,936,0,0] | [234,936,234,0] | [0,919,0,0] |
| Fillet/blade_hub_2.smb | [0,648,0,0] | [162,648,162,0] | [0,637,0,0] |
| Fillet/blade_hub_3.smb | [0,1008,0,0] | [252,1008,252,0] | [0,996,0,0] |
| Fillet/bottle.smb | [0,321,0,0] | [80,321,80,0] | [0,305,0,0] |
| Fillet/box_extrusion2.smb | [0,169,0,0] | [41,169,42,0] | [0,160,0,0] |
| Fillet/case_ext.smb | [0,34,0,0] | [12,35,9,0] | [0,32,0,0] |
| Fillet/case_int.smb | [0,20,0,0] | [4,20,5,0] | [0,18,0,0] |
| Fillet/cliff_1.smb | [0,936,0,0] | [234,936,234,0] | [0,919,0,0] |
| Fillet/cliff_2.smb | [0,648,0,0] | [162,648,162,0] | [0,637,0,0] |
| Fillet/corner_box.smb | [0,120,0,0] | [30,120,30,0] | [0,113,0,0] |
| Fillet/cyl_cyl_2191.smb | [0,216,0,0] | [53,217,54,0] | [0,208,0,0] |
| Fillet/cyl_cyl_shell.smb | [0,60,0,0] | [14,60,14,0] | [0,57,0,0] |
| Fillet/nurb_box1.smb | [0,72,0,0] | [18,72,18,0] | [0,67,0,0] |
| Fillet/open_shell.smb | [0,122,0,0] | [42,123,31,0] | [0,117,0,0] |
| Fillet/shell_fillet2.smb | [0,56,0,0] | [20,56,14,0] | [0,53,0,0] |
| Fillet/sphere_dif_box.smb | [0,113,0,0] | [28,113,28,0] | [0,106,0,0] |
| Fillet/sphere_plane.smb | [0,25,0,0] | [5,25,6,0] | [0,22,0,0] |
| Fillet/test1.smb | [0,25,0,0] | [4,25,6,0] | [0,22,0,0] |
| Fillet/test8.smb | [0,42,0,0] | [9,42,10,0] | [0,38,0,0] |
| Fillet/torii_fillet.smb | [0,264,0,0] | [66,264,66,0] | [0,251,0,0] |
| Fillet/torii_unioned.smb | [0,40,0,0] | [5,41,10,0] | [0,37,0,0] |
| Fillet/vase1.smb | [0,384,0,0] | [96,384,96,0] | [0,364,0,0] |
| Fillet/vase2.smb | [0,192,0,0] | [48,192,48,0] | [0,181,0,0] |
| Fillet/vase3.smb | [0,384,0,0] | [96,384,96,0] | [0,366,0,0] |
| Shell/ShellBrep_983.smb | [0,240,0,0] | [60,240,60,0] | [0,228,0,0] |
| Solids/fillet_ts.smb | [0,136,0,0] | [68,136,34,0] | [0,127,0,0] |
| Solids/revolution2.smb | [0,525,0,0] | [129,528,132,0] | [0,494,0,0] |
| Solids/revolution_mold.smb | [0,704,0,0] | [176,704,176,0] | [0,672,0,0] |

### Item 2a: update_tolerances_after_boolean, then a check (count of the check; the operand-matching trial's order)

| body | main | head, fresh | head, warm |
|---|---|---|---|
| bar_into_post | [0,114,0,0] | [29,114,30,0] | [0,114,0,0] |
| box_into_cylinder | [0,159,0,0] | [42,159,42,0] | [0,159,0,0] |
| ball_less_corner | [0,55,0,0] | [14,55,14,0] | [0,55,0,0] |
| plate_with_hole | [0,114,0,0] | [28,114,30,0] | [0,114,0,0] |
| Fillet/blade_hub_1.smb | [0,936,0,0] | [234,936,378,0] | [0,936,0,0] |
| Fillet/blade_hub_2.smb | [0,648,0,0] | [162,648,262,0] | [0,648,0,0] |
| Fillet/blade_hub_3.smb | [0,1008,0,0] | [252,1008,408,0] | [0,1008,0,0] |
| Fillet/bottle.smb | [0,321,0,0] | [80,321,80,0] | [0,321,0,0] |
| Fillet/box_extrusion2.smb | [0,169,0,0] | [41,169,42,0] | [0,169,0,0] |
| Fillet/case_ext.smb | [0,35,0,0] | [12,35,9,0] | [0,35,0,0] |
| Fillet/case_int.smb | [0,20,0,0] | [4,20,5,0] | [0,20,0,0] |
| Fillet/cliff_1.smb | [0,936,0,0] | [234,936,378,0] | [0,936,0,0] |
| Fillet/cliff_2.smb | [0,648,0,0] | [162,648,262,0] | [0,648,0,0] |
| Fillet/corner_box.smb | [0,120,0,0] | [30,120,30,0] | [0,120,0,0] |
| Fillet/cyl_cyl_2191.smb | [0,231,0,0] | [53,231,70,0] | [0,231,0,0] |
| Fillet/cyl_cyl_shell.smb | [0,60,0,0] | [14,60,14,0] | [0,60,0,0] |
| Fillet/nurb_box1.smb | [0,72,0,0] | [18,72,18,0] | [0,72,0,0] |
| Fillet/open_shell.smb | [0,123,0,0] | [42,123,31,0] | [0,123,0,0] |
| Fillet/shell_fillet2.smb | [0,56,0,0] | [20,56,14,0] | [0,56,0,0] |
| Fillet/sphere_dif_box.smb | [0,113,0,0] | [28,113,28,0] | [0,113,0,0] |
| Fillet/sphere_plane.smb | [0,25,0,0] | [5,25,6,0] | [0,25,0,0] |
| Fillet/test1.smb | [0,25,0,0] | [4,25,6,0] | [0,25,0,0] |
| Fillet/test8.smb | [0,42,0,0] | [9,42,10,0] | [0,42,0,0] |
| Fillet/torii_fillet.smb | [0,264,0,0] | [66,264,66,0] | [0,264,0,0] |
| Fillet/torii_unioned.smb | [0,41,0,0] | [5,41,10,0] | [0,41,0,0] |
| Fillet/vase1.smb | [0,389,0,0] | [96,389,108,0] | [0,389,0,0] |
| Fillet/vase2.smb | [0,200,0,0] | [48,206,80,0] | [0,200,0,0] |
| Fillet/vase3.smb | [0,409,0,0] | [96,423,164,0] | [0,409,0,0] |
| Shell/ShellBrep_983.smb | [0,240,0,0] | [60,240,60,0] | [0,240,0,0] |
| Solids/fillet_ts.smb | [0,136,0,0] | [68,136,34,0] | [0,136,0,0] |
| Solids/revolution2.smb | [0,528,0,0] | [129,528,132,0] | [0,528,0,0] |
| Solids/revolution_mold.smb | [0,704,0,0] | [176,704,196,0] | [0,704,0,0] |

### Item 2b: a check, then update_tolerances_after_boolean (count of the update)

| body | main | head, fresh | head, warm |
|---|---|---|---|
| bar_into_post | [0,50,0,0] | [29,60,30,0] | [29,60,30,0] |
| box_into_cylinder | [0,66,0,0] | [42,84,42,0] | [42,84,42,0] |
| ball_less_corner | [0,21,0,0] | [14,28,14,0] | [14,28,14,0] |
| plate_with_hole | [0,53,0,0] | [28,60,30,0] | [28,60,30,0] |
| Fillet/blade_hub_1.smb | [0,468,0,0] | [0,451,0,0] | [0,451,0,0] |
| Fillet/blade_hub_2.smb | [0,324,0,0] | [0,313,0,0] | [0,313,0,0] |
| Fillet/blade_hub_3.smb | [0,504,0,0] | [0,492,0,0] | [0,492,0,0] |
| Fillet/bottle.smb | [0,160,0,0] | [0,144,0,0] | [0,144,0,0] |
| Fillet/box_extrusion2.smb | [0,84,0,0] | [0,75,0,0] | [0,75,0,0] |
| Fillet/case_ext.smb | [0,17,0,0] | [0,15,0,0] | [0,15,0,0] |
| Fillet/case_int.smb | [0,10,0,0] | [0,8,0,0] | [0,8,0,0] |
| Fillet/cliff_1.smb | [0,468,0,0] | [0,451,0,0] | [0,451,0,0] |
| Fillet/cliff_2.smb | [0,324,0,0] | [0,313,0,0] | [0,313,0,0] |
| Fillet/corner_box.smb | [0,60,0,0] | [0,53,0,0] | [0,53,0,0] |
| Fillet/cyl_cyl_2191.smb | [0,107,0,0] | [0,99,0,0] | [0,99,0,0] |
| Fillet/cyl_cyl_shell.smb | [0,28,0,0] | [0,25,0,0] | [0,25,0,0] |
| Fillet/nurb_box1.smb | [0,36,0,0] | [0,31,0,0] | [0,31,0,0] |
| Fillet/open_shell.smb | [0,61,0,0] | [0,56,0,0] | [0,56,0,0] |
| Fillet/shell_fillet2.smb | [0,28,0,0] | [0,25,0,0] | [0,25,0,0] |
| Fillet/sphere_dif_box.smb | [0,56,0,0] | [0,49,0,0] | [0,49,0,0] |
| Fillet/sphere_plane.smb | [0,12,0,0] | [0,9,0,0] | [0,9,0,0] |
| Fillet/test1.smb | [0,12,0,0] | [0,9,0,0] | [0,9,0,0] |
| Fillet/test8.smb | [0,20,0,0] | [0,16,0,0] | [0,16,0,0] |
| Fillet/torii_fillet.smb | [0,132,0,0] | [0,119,0,0] | [0,119,0,0] |
| Fillet/torii_unioned.smb | [0,19,0,0] | [0,16,0,0] | [0,16,0,0] |
| Fillet/vase1.smb | [0,192,0,0] | [0,172,0,0] | [0,172,0,0] |
| Fillet/vase2.smb | [0,96,0,0] | [0,85,0,0] | [0,85,0,0] |
| Fillet/vase3.smb | [0,192,0,0] | [0,174,0,0] | [0,174,0,0] |
| Shell/ShellBrep_983.smb | [0,120,0,0] | [0,108,0,0] | [0,108,0,0] |
| Solids/fillet_ts.smb | [0,68,0,0] | [0,59,0,0] | [0,59,0,0] |
| Solids/revolution2.smb | [0,261,0,0] | [0,230,0,0] | [0,230,0,0] |
| Solids/revolution_mold.smb | [0,352,0,0] | [0,320,0,0] | [0,320,0,0] |

### Reference: update_tolerances_after_boolean alone on a freshly built body

| body | main | head |
|---|---|---|
| bar_into_post | [29,60,30,0] | [29,60,30,0] |
| box_into_cylinder | [42,84,42,0] | [42,84,42,0] |
| ball_less_corner | [14,28,14,0] | [14,28,14,0] |
| plate_with_hole | [28,60,30,0] | [28,60,30,0] |
| Fillet/blade_hub_1.smb | [0,451,0,0] | [0,451,0,0] |
| Fillet/blade_hub_2.smb | [0,313,0,0] | [0,313,0,0] |
| Fillet/blade_hub_3.smb | [0,492,0,0] | [0,492,0,0] |
| Fillet/bottle.smb | [0,144,0,0] | [0,144,0,0] |
| Fillet/box_extrusion2.smb | [0,75,0,0] | [0,75,0,0] |
| Fillet/case_ext.smb | [0,15,0,0] | [0,15,0,0] |
| Fillet/case_int.smb | [0,8,0,0] | [0,8,0,0] |
| Fillet/cliff_1.smb | [0,451,0,0] | [0,451,0,0] |
| Fillet/cliff_2.smb | [0,313,0,0] | [0,313,0,0] |
| Fillet/corner_box.smb | [0,53,0,0] | [0,53,0,0] |
| Fillet/cyl_cyl_2191.smb | [0,97,0,0] | [0,97,0,0] |
| Fillet/cyl_cyl_shell.smb | [0,25,0,0] | [0,25,0,0] |
| Fillet/nurb_box1.smb | [0,31,0,0] | [0,31,0,0] |
| Fillet/open_shell.smb | [0,56,0,0] | [0,56,0,0] |
| Fillet/shell_fillet2.smb | [0,25,0,0] | [0,25,0,0] |
| Fillet/sphere_dif_box.smb | [0,49,0,0] | [0,49,0,0] |
| Fillet/sphere_plane.smb | [0,9,0,0] | [0,9,0,0] |
| Fillet/test1.smb | [0,9,0,0] | [0,9,0,0] |
| Fillet/test8.smb | [0,16,0,0] | [0,16,0,0] |
| Fillet/torii_fillet.smb | [0,119,0,0] | [0,119,0,0] |
| Fillet/torii_unioned.smb | [0,16,0,0] | [0,16,0,0] |
| Fillet/vase1.smb | [0,172,0,0] | [0,172,0,0] |
| Fillet/vase2.smb | [0,85,0,0] | [0,85,0,0] |
| Fillet/vase3.smb | [0,173,0,0] | [0,173,0,0] |
| Shell/ShellBrep_983.smb | [0,108,0,0] | [0,108,0,0] |
| Solids/fillet_ts.smb | [0,59,0,0] | [0,59,0,0] |
| Solids/revolution2.smb | [0,230,0,0] | [0,230,0,0] |
| Solids/revolution_mold.smb | [0,320,0,0] | [0,320,0,0] |

### Level 0 (main only)
The Level 0 check computes gaps exactly like Level 2. On main, Level 0 followed by Level 1:

| body | Level 0 | then Level 1 |
|---|---|---|
| box_into_cylinder | [42,159,42,0] | [0,134,0,0] |
| ball_less_corner | [14,55,14,0] | [0,44,0,0] |
| plate_with_hole | [28,114,30,0] | [0,107,0,0] |
| blade_hub_1 | [0,919,0,0] | [0,936,0,0] |
| vase3 | [0,386,0,0] | [0,384,0,0] |

The head's Level 0 run was stopped before it printed.

### Reading the counts
- Item 1, one check:
  - Cold bodies: the head equals main in both variants.
  - Files arriving with initialised caches: "fresh" adds about one vertex/edge and one or more edge/face computations per gap. For blade_hub_1 that is +234 vertex/edge, +378 edge/face and +17 vertex/face.
  - "warm" equals main.
- Item 2: every sequence of "check, then a pass that reads gaps on the same body" costs the pass's full cold count on the head, because the check no longer leaves anything behind. On box_into_cylinder the update goes from [0,66,0,0] to [42,84,42,0], which is the update's count when run alone.
  - The operand-matching trial ("update, then check") is equal to main only with "warm".
- Item 3, a second check in a row:
  - Cold bodies: the head recomputes, for example box_into_cylinder [0,134,0,0] becomes [42,152,42,0].
  - Files with initialised caches: equal to main or lower with "warm".
- Item 4: the cost of "fresh" over "warm" is the corpus rows of item 1 and of item 2a. With fresh, edge/face is computed more than once per gap; see section 4.

## (b) Production sequences in which a check warms caches that a later pass on the SAME body reads
Paths are under `solidcore/crates/`. Line numbers are at 8865da5c27 / fc3ea32243 (unchanged in these files).

The outward-solid proof runs a Level 0 check on the real body:
- `solidcore/src/topology/brep.rs:234` `valid_and_outward_failure`
- reached through `is_valid_and_outward` (:216, :220), `outward_solid_failure` (:166, :173), `outward_touching_solids_failure` (:196, :203) and `is_outward_solid` (:103)
- `solidcore/src/topology/brep/primitive_outward.rs:114` `is_outward_primitive` (Level 0 on the body)

Its callers that then operate on the same body:
- `solidcore-ops/src/body.rs:566` `Expect::of(k, input)`: the operation's input, which the operation then works on. Callers include offset.rs:401, :445, :530, :648; heal.rs:226; stitch.rs:371; and blend.rs:2330 (on a copy: `as_given = brep_copy(input)`, so no benefit there).
- `solidcore-ops/src/body.rs:590` / `:593` `Expect::of_held`: `is_outward_primitive` or `prove` on the input when no proof is held (contract `Proved`, body.rs:617).
- `solidcore-ops/src/body.rs:124` `prove_naming`, and `:683`: the proof of an operation's result, which becomes the stored body that the next operation reads.
- `solidcore/src/topology/brep/local_operation.rs:442`: `is_outward_solid(tool)`, then the tool is used in the Boolean.
- `solidcore/src/offset/wall_room.rs:210`: `is_outward_solid(b)`, then the wall-room measurement on `b`.
- `solidcore/src/topology/brep/side_extend.rs:355`: a Level 2 check on the extended sheet `nb`, which is returned (to solidcore-ops/src/extend.rs) for later use.
- `solidcore/src/blend/fillet_executive/session/finish.rs:342` `check_kept`: a Level 2 check on the result, only when an open result was kept.

Passes that read gaps with `force_calc = false` after such a check:
- `update_tolerances_after_boolean` (topology/brep/validation/wide_vertices.rs:22): measured, item 2b.
- The Boolean's operand tolerance matching: `boolean/operand_tolerances.rs:57-80` runs `tightened`, that is `update_tolerances_after_boolean`, on `coarse`, a proved operand.
- The offset's `validate_and_update_tolerances` (offset/offset_executive/p2.rs:2121, update mode).
- Heal (heal_data/p2.rs reads the cache fields directly).

Checks on a copy, which never warm anything a later pass reads:
- ops `Body::check`: `solidcore-ops/src/check.rs:122` through `measure_body` (session.rs:763-768, `brep_copy`)
- `validation::soundness` / `standing` (soundness.rs:98, :129). Every caller passes a copy: local_operation.rs:179, :535, :760; mirror.rs:980 is a test helper.
- `mirror.rs:365`: check of a trial copy
- `boolean/operand_tolerances.rs:71`: check of `trial`, a `brep_copy` of `coarse`. On main this check reads the caches that `tightened(trial)` just stored (item 2a).
- `solidcore-ops/src/blend.rs:2330`: copy

## (c) Branch and scratch state

Scratch branches, logs and their clean-up are recorded outside this repository.

## 3. The stale vertex/face gap finding
- Reproduction: no kernel operation left the cache stale. The test set it by hand, on `ap::create_box(k, origin(), 1.0, 2.0, 3.0)`:
  1. A first `check_brep` Level 2 returns no lines.
  2. The test takes the first vertex use `vu` and writes `vertex_face_gap.vertexuse = Some(vu)` and `vertex_face_gap.gap_3d = 1.0` (an initialised cache holding a wrong value).
  3. It checks again.
- Main's second check returns these two lines:
  - `SmBrep::Test #3 Bad Gap: Vertex/Face gap across Edgeuse end larger than tol (offset=     1, tolerance= 5e-06) [owner SmEdgeuse]`
  - `SmVertex::Test #1 Bad Vertex Gap: Vertex-Face Gap exceeds XSectTol3d for one (or more) Faces (offset=     1, tolerance= 2e-05) [owner SmVertex]`
- With the gap recalculated, the box is valid:
  - the first check of the same box returns `[]`, and so does the head, which ignores the stored value;
  - main with the caches reset before the check reports the same as the cached run on all 32 bodies;
  - main's check itself calls `is_within_xsect_tol_3d_of_faces` (vertex/corner.rs:117) with `force_calc = false` here, so it trusted the stale value.
- What this shows: main's check believes any initialised cache. A stale cache would make a valid body fail. I did not find a kernel sequence that leaves one stale.
  - The writes that reset it are `set_uv_trim_curve` (edgeuse/p1.rs), `notify` (edgeuse/p2.rs) and `init_vertex_face_gap`.
  - Candidates worth auditing: writes that move a vertex point, a face surface or a trim, and do not go through those.

## 4. Repeated gap computations within one check
- On MAIN, one Level 2 check computes each vertex/face gap several times:
  - box_into_cylinder: 159 vertex/face computations for 42 distinct gaps stored (84 vertex uses, siblings shared); a second check on the warm body still makes 134.
  - blade_hub_1: 919 for 468 vertex uses; the second check makes 936.
  - The vertex/edge and edge/face gaps are computed once per gap on main (42 and 42 for 84 uses on box_into_cylinder; 0 on files with initialised caches).
- The probe's write sites over the whole run on main show where the vertex/face repeats come from:
  - `vertex/corner.rs:146`: 4372 writes, after the resets at :137 when `force_calc` is true (`is_within_xsect_tol_3d_of_faces`)
  - `assert_valid/brep.rs:371` and `:373`: 1921 and 2225 writes
  - query/face/p2.rs:1615: 78; face/p2.rs:3422: 33
- So nearly all repeats are the `force_calc = true` resets in `is_within_xsect_tol_3d_of_faces` and the resets that follow sibling and UV-trim writes. Whether they can be avoided without changing a result is not proven: `force_calc = true` exists to get a fresh value. A value already computed in the same check, with no write to its inputs since, could be reused.
- The 378 against 234 edge/face computations on blade_hub_1 under "fresh" come from UV trim creation inside the check:
  - `set_uv_trim_curve` / `notify` reset entries that are then computed again;
  - `fit_tolerance` computed without caching when no cache was set; 2d1af8dba2 took the count from 770 to 378.
  - On main, blade_hub_1's edge/face caches arrive initialised, so main computes 0 there.
- The candidate improvement on main is the vertex/face repeats (about 3.8 per gap on the cold Booleans, about 2 per vertex use on the files). Proving any reuse sound needs the write audit in section 3 first.
