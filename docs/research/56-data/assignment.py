"""Research 56's assignment of the pin's kernel files and items to crates (appendix B), as
changed by Re-review 2 of review-56 (E1 to E8), ruled on 11-27.

Paths are relative to the kernel crate's src/. Crate names drop the `scx-` prefix.
FILES: (prefix, crate); the longest matching prefix wins. A prefix ending in `/` matches a
directory, one ending in `*` matches by prefix, one ending in `.rs` matches only that file, and
any other matches the file of that name plus `.rs` and the directory of the same name.
"""
NOT_PORTED = '-'


def _rules(table):
    return [(p, c) for c, ps in table.items() for p in ps.split()]


FILES = _rules({
    'num': 'math.rs limits.rs geom.rs geom/vector2d geom/vector3d geom/matrix geom/transform '
           'geom/extent1d geom/extent2d geom/extent3d geom/periodic_extent1d geom/interval_vector '
           'geom/certified geom/tol geom/expansion geom/step_matrix geom/polar_box geom/pseudo_box '
           'geom/axis2_placement blend/fillet_section/interval blend/fillet_section/series '
           'blend/fillet_section/tay blend/fillet_section/taylor_model',
    'algo': 'geom/polynomial geom/global_solver geom/hash_table geom/tree geom/krawczyk '
            'geom/local_solve1d geom/local_solve_nd geom/solution_array geom/geom_utility '
            'geom/assert_array geom/point_set offset/construction/bernstein '
            'topology/brep/validation/spatial mass/integrator.rs',
    'nurbs': 'nurbs/ nurbs.rs',
    'nurbs-surface': 'nurbs/srf_* nurbs/nurbs_srf nurbs/iges geom/nurbs_enclosure',
    'curve': 'curve/ curve.rs nurbs/b_spline_curve nurbs/b_spline_curve_n_lib eval/ eval.rs',
    'surface': 'surface/ surface.rs nurbs/b_spline_surface nurbs/b_spline_surface_n_lib '
               'nurbs/gordon nurbs/n_sided geom/grid blend/fillet_section/ curve/crv_on_surf '
               'curve/iso_curve curve/tangent_field curve/geodesic_offset curve/crv_in_volume '
               'curve/projected_curve curve/contact nurbs/b_spline_curve/trim_image '
               # formerly geomq, merged here by the S9 trigger (4.3)
               'intersect.rs intersect/surface_drop_lib intersect/surface_drop_curve '
               'intersect/surface_tracer query/curve_surface_gap query/distance_bound query/deviation '
               'query/seam_bound surface/recognise surface/surface_point_solve geom/plane_height '
               'geom/facing_trough geom/axial_section '
               # E3: eval/ has no v2 counterpart as one unit; its surface half goes to the kinds here
               'eval/spline eval/collapse eval/uv eval/surface eval/sphere eval/pole eval/ruling '
               'eval/query',  # the record query reads both halves
    'planar': 'prism/ prism.rs cell_grid/ cell_grid.rs geom/paper_gaps curve/conic_projection '
              'curve/spline_projection curve/fit_projection',
    'sketch': 'eval/constraint',
    'teeth': 'teeth/ teeth.rs',
    'ssi': 'intersect/surface_intersector intersect/adv_surface_intersector',
    'brep': 'topology/ topology.rs heal/gap eval/pcurve query/poly geom/tools',
    # E2: mass splits out of bodyq onto brep; mass/integrator.rs itself, the ODE core, is algo's
    'mass': 'mass/ mass.rs',
    'bodyq': 'topology/vertex/corner query/ query.rs intersect/curve_class intersect/point_class '
             'intersect/api_intersectors intersect/topology_solver intersect/ray_tracer '
             'surface/face_evaluation surface/face_pair surface/face_pair_cylinder '
             'surface/face_surface surface/mid_joint surface/mid_sheet surface/cylinder_wall '
             'surface/side prism/recognition prism/measure eval/hole_stacks heal/face_props',
    'record': 'topology/api_files topology/part_file topology/database_io eval/label',
    'tess': 'query/tess',
    'check': 'topology/integrity topology/brep/validation',
    'hlr': 'hlr/ hlr.rs intersect/surface_silhouette blend/hcr eval/outline topology/api_projection',
    'stitch': 'heal/stitch heal/heal_data heal.rs',
    'boolean': 'boolean/ boolean.rs topology/proto_topology topology/brep/cutting topology/brep/planar_section '
               'primitive/primitive_creation/p1/boolean.rs '  # the planar boolean (P1)
               'intersect/curve_class/witness.rs',  # E1: the tangency witness, with face-face intersection
    'make': 'sweep/ sweep.rs primitive/ primitive.rs prism/extrude prism/sheet prism/placement '
            'nurbs/certificate intersect/views',
    'repair': 'heal/tolerance_defects',
    'local': 'blend/fillet_removal blend/identify_fillet blend/feature_executive '
             'topology/brep/side_extend',  # E8: extend is a local operation
    'taper': 'offset/draft',
    'sheetmetal': 'develop/ develop.rs',
    'mesh': 'mesh_reconstruction/ mesh_reduce/ mesh_reduce.rs',
    'blend': 'blend/ blend.rs',
    'offset': 'offset/ offset.rs',
    'weld': 'blend/weld_bead weld_symbol/ weld_symbol.rs',
    'frames': 'prism/member prism/gusset',
    NOT_PORTED: 'error.rs error/ kernel.rs lib.rs profile.rs geom/context geom/object '
                'geom/a_object geom/check_memo geom/content geom/attribute '
                'surface/surface_cache surface/trim_srf_cache topology/brep_cache',
})


def _items(rows):
    out = {}
    for f, names, c in rows:
        for n in names.split():
            out[(f, n)] = c
    return out


# Single items: (defining file, item) -> crate.
ITEMS = _items([
    ('geom/tol.rs', 'TopologyToUpdate', 'brep'),
    # the span's ends on its curve become a curve function; the span itself stays in algo
    ('curve/curve_cache.rs', 'ends_on', 'curve'),
    # a Surface method that runs a curve classification becomes the classification's (K1)
    ('surface/surface/p2.rs', 'curve_on_classify', 'bodyq'),
    # the clearance proof's face contact classifies a point on the face, so the proof and the
    # reach queries go to bodyq with classification (E4); check keeps the reports (C3)
    ('topology/brep/validation/pointers.rs', 'TUBE_PAIRS tube_clears check_vertex_clearance '
     'vertex_neighbourhood_edges face_contact', 'bodyq'),
    # Brep's own validity queries go up to check with their callers; the rank-deficient
    # transform is a check operation and Brep::transform refuses it (T1). E8 calls
    # is_outward_solid an orientation read for bodyq, but at the pin it runs the full check
    # (check_brep) before the volume sign, so it stays with the check
    ('topology/brep.rs', 'outward_solid_failure outward_touching_solids_failure '
     'is_valid_and_outward valid_and_outward_failure is_outward_solid', 'check'),
    ('topology/brep_constructor.rs', 'validate_pointers', 'check'),
    ('topology/brep_constructor.rs', 'end_shell', 'stitch'),
    # a Surface function kept with Brep's TurnToNURBS
    ('topology/brep/p3.rs', 'with_nurb', 'surface'),
    # a guard on the face cache's state, which mass asks before integrating (the mass re-test)
    ('query/face/p2.rs', 'classification_builds_tree', 'brep'),
    # geometry validity runs surface intersections; it is the check's, on ssi (E8, 01 2.3)
    ('surface/surface/assert_valid.rs', 'curve_bounded_surface_assert_valid', 'check'),
    ('topology/brep/validation/assert_valid.rs', 'standalone_curve_assert_valid', 'check'),
    ('topology/brep/mirror.rs', 'degenerate_low_work_transform', 'check'),
    ('topology/api_files.rs', 'BIG_ULONG OP_SLOTS', 'brep'),
    ('intersect/api_intersectors.rs', 'curve_crossings', 'surface'),
    ('curve/curve/p4.rs', 'plane_global_curve_intersect', 'surface'),
    ('nurbs/certificate.rs', 'span_bound node_on', 'surface'),
    ('intersect/api_intersectors.rs', 'OP_SURFACES CROSSING_WITNESS_REL CROSSING_WITNESS_STEPS', 'ssi'),
    ('topology/proto_topology/p1/copy_brep_map.rs', 'impl:Default', 'brep'),
    # the read-status helpers (ignore_read) test this reason beside SolidError, in solvers too
    ('topology/database_io.rs', 'NESTING_LIMIT_REASON MAX_READ_NESTING', 'num'),
    ('topology/proto_topology/p1/owning_topology.rs', 'TYPE', 'brep'),
    ('prism/recognition.rs', 'RecognitionVector conic_ellipse EllipseScalar loop_circle loop_ellipse', 'planar'),
    ('nurbs/iges.rs', 'clamp_curve', 'nurbs'),
    ('geom/plane_height.rs', 'is_square_plane_at', 'curve'),
    ('query/tangent_edge.rs', 'bernstein_roots', 'algo'),
    ('eval/label.rs', 'curve_interval', 'brep'),
    ('topology/api_files.rs', 'add_curve_output_slots', 'brep'),
    ('topology/api_projection.rs', 'plane_projection', 'surface'),
    ('intersect/api_intersectors.rs', 'intersect_curves CurveIntersections', 'curve'),
    ('intersect/api_intersectors.rs', 'intersect_surfaces intersect_surfaces_within '
     'refuse_points_where_a_plane_crosses', 'ssi'),
    ('surface/surface/p2.rs', 'MAX_GEOMETRY_NESTING', 'curve'),
    ('topology/integrity.rs', 'derived_face_shell derived_face_loops derived_loop_face '
     'derived_loop_edges derived_shell_faces', 'brep'),
    ('topology/brep/validation/pointers.rs', 'edge_tube_reaches GROWTH_PAST_RATIO vertex_ball_reaches', 'bodyq'),
    ('topology/vertex/corner.rs', 'xsect_tol_3d_of_brep', 'brep'),
    ('blend/fillet_corner/p3.rs', 'join_with join_with_gap', 'curve'),
    ('blend/fillet_exact.rs', 'classify_blend_spine circle_spine_station', 'surface'),
    ('blend/fillet_executive/api.rs', 'create_face_from_surface', 'make'),
    # the planar boolean is a method of the primitive builder at the pin; it is boolean's (4.6, P1)
    ('primitive/primitive_creation/p1/primitives.rs', 'boolean_2d boolean_2d_keyed', 'boolean'),
    ('primitive/primitive_creation/p1/primitives.rs', 'Boolean2d', 'boolean'),
    # the two-surface continuity measure that the Coons seams test (S9: the family moves whole)
    ('query/continuity.rs', 'seam_normal_angle continuity_over_surfaces continuity_on_surfaces', 'surface'),
    # re-review R2: the boxes need only brep
    ('query/brep.rs', 'calculate_bounding_box', 'brep'),
    ('query/brep.rs', 'calculate_tight_bounding_box', 'bodyq'),
    ('query/face/p1.rs', 'calculate_bounding_box calculate_bounding_box_in', 'brep'),
    # E4: the tight face box classifies the points where it is extreme, so it is bodyq's
    ('query/face/p3.rs', 'calculate_tight_bounding_box calculate_tight_bounding_box_into', 'bodyq'),
    ('query/face/p1.rs', 'tight_face_box', 'bodyq'),
    ('query/edge.rs', 'calculate_bounding_box calculate_tight_bounding_box', 'brep'),
    ('geom/geom_utility/p2.rs', 'mult_4x4', 'num'),
    # rulings on B3
    ('topology/brep_constructor.rs', 'drop_limit', 'curve'),
    ('surface/implicit.rs', 'curve_on_quadrics_with_constants spine_on_offsets', 'curve'),
    # re-review R0
    ('topology/shell.rs', 'ShellRayWorld', 'bodyq'),
    ('topology/face/p4.rs', 'VertexSector', 'bodyq'),
    # the call-edge proof (9.2): curve helpers that sat in surface files, and the reverse
    ('blend/fillet_section/certificate.rs', 'curve_derivs_over curve_derivs_over_about derivs_over',
     'curve'),
    ('eval/pcurve.rs', 'segment', 'curve'),
    ('surface/procedural_blend.rs', 'count_area_sum', 'num'),
    ('geom/geom_utility/p1.rs', 'solve_quadratic_eqn', 'num'),
    ('geom/global_solver.rs', 'surface_projected_may_contain_answers', 'surface'),
    ('surface/implicit.rs', 'curve_segments piece_turn sub_segment', 'curve'),
    ('nurbs/b_spline_curve/p3.rs', 'stretch_clears_on_surface first_fundamental_form trim_metric '
     'trim_turn_on_surface', 'surface'),
    # Re-review 2, E1: the operations that sat in brep files as methods of the body classes
    ('topology/brep/p1.rs', 'brep_intersect_mut_with_partial brep_intersect_self_with_partial', 'boolean'),
    ('topology/face/p3.rs', 'face_intersect_breps_cached', 'boolean'),
    ('topology/brep/p1.rs', 'create_parallel_projection_curves', 'bodyq'),
    # E2: a guard on a cache's state goes with the cells
    ('mass/integrator/properties.rs', 'require_classification_trees', 'brep'),
    # E3: the zero interval goes down with Ivl; IGES splits by entity, curve half to nurbs
    ('geom/nurbs_enclosure.rs', 'IZ', 'num'),
    ('nurbs/iges.rs', 'iges_126_curve correct_curve_knots clamp_dir correct_dir', 'nurbs'),
    # E4: brep keeps the face cells (01 3.1, 3.2): face box, uv box and boundary tree fills
    ('query/face/p2.rs', 'face_surface_cache owned_surface_cache solve_surface_cache_of_face '
     'surface_cache_state_of_face read_surface_cache_of_face surface_cache_of_face '
     'note_face_surface_cache face_cache_tree impl:CacheFace', 'brep'),
    # a surface's plane section is an ssi operation; the face section trims by classification
    # and is boolean's (01 1.1: boolean holds sections)
    ('topology/brep/planar_section.rs', 'surface_planar_section_curves create_plane_from_bbox', 'ssi'),
    # an edge's length is a read of the edge's curve over its interval; its one caller is bodyq's
    ('mass/integrator/properties.rs', 'edge_length', 'brep'),
    # a vertex position from three faces is a two-surface refine; its users are the merge's
    ('topology/vertex/corner.rs', 'calc_corner_from_faces is_on_face_corner', 'boolean'),
    # E3: the curve half of eval/ keeps the curve evaluators; a curve's extent over a surface
    # parameterisation is the surface half's
    ('eval/form.rs', 'extent extent_in extent_raw half_turn', 'surface'),
    # the Bezier basis helpers the curve certificate reads are plain NURBS arithmetic
    ('geom/nurbs_enclosure.rs', 'pw_at at1 at2', 'nurbs'),
    ('blend/fillet_section/station.rs', 'span sided_span knot ders_basis smax', 'nurbs'),
    ('eval/spline.rs', 'onto_knots BOUNDARY_ROUNDING', 'nurbs'),  # the free function (line 79)
    # a sweep that stitches, classifies and runs the outward check is an operation above check;
    # make is the lowest crate that holds all it calls
    ('topology/brep/local_operation.rs', 'radial_sweep contract_complete_cone', 'make'),
])

# Types that move with their inherent impls: (defining file, type) -> crate.
TYPES = _items([
    ('topology/brep/validation/pointers.rs', 'Obstacle Cell VertexNeighbourhood FaceContact', 'bodyq'),
    ('topology.rs', 'ObjectHandle', 'surface'),
    ('blend/fillet_section/section.rs', 'GnLawRound', 'surface'),
    ('intersect/surface_intersector/p1.rs', 'TsectPnt', 'surface'),
    ('topology/brep/p2.rs', 'MergeCurvesObjects RayFireResult SharedBoundary', 'bodyq'),
    ('topology/brep/p7.rs', 'SelfClassification', 'bodyq'),
    ('topology/face/p2.rs', 'FaceCurveRun QueryBrep', 'bodyq'),
    ('topology/face/p3.rs', 'FaceIntersectCaches FaceIntersection', 'bodyq'),
    ('topology/face/p4.rs', 'FaceLineCache HasSeamProblemOptions SeamCheck UvLineIntervals', 'bodyq'),
    ('topology/shell.rs', 'Objects ShellRaySector ShellRayState ShellRayInput ShellRayAttempt', 'bodyq'),
    ('query/face/p2.rs', 'BrepCacheFace FaceSurfaceCache', 'brep'),
    ('topology/brep/planar_section.rs', 'SurfaceSection', 'ssi'),
    ('surface/surf_of_extrusion/p2.rs', 'ClassifyHandle', 'bodyq'),
    ('query/brep.rs', 'LazyBrep', 'brep'),
    ('blend/fillet_section/certificate.rs', 'NurbsCurve', 'curve'),
    ('blend/blend_corner/mixed/algebra.rs', 'Side Crv', 'surface'),
    ('blend/blend_setback.rs', 'Face', 'surface'),
    ('blend/fillet_exact.rs', 'ClassInputs', 'surface'),
    ('blend/fillet_exact.rs', 'QuadricKind BlendQuadric BlendSpine ExactBlend', 'surface'),
    ('offset/draft.rs', 'DraftDefinition', 'surface'),
    ('sweep/section_blend.rs', 'SectionBlendDefinition', 'surface'),
    ('curve/curve_cache.rs', 'BezierSpan BezierAux1d', 'algo'),
    ('surface/extend.rs', 'ExtendRefusal', 'nurbs'),
    ('eval/spline.rs', 'Nurbs', 'nurbs-surface'),
    # E3: the curve spline evaluator is the curve half; the intersection curve is a procedural
    # kind over two surfaces (01 7.3), so its evaluator is the surface half's
    ('eval/spline.rs', 'EvalCurve', 'curve'),
    ('eval/curve.rs', 'Intersection', 'surface'),
    ('blend/fillet_section/station.rs', 'Mat', 'nurbs'),
    ('surface/side.rs', 'Bracket', 'num'),
    ('nurbs/b_spline_surface/p1.rs', 'SurfParamType', 'curve'),
    ('nurbs/b_spline_surface_n_lib/p2.rs', 'OrientType', 'curve'),
    ('surface/implicit.rs', 'conic_turn QuadricEquation BPoly Turn CurveSegment', 'curve'),
    ('topology/proto_topology/p1/copy_brep_map.rs', 'CopyBrepMap', 'brep'),
])

# Edges deleted by a design change in 56 (9.3):
# (source file prefix or None, source unit name or None, target file prefix, target unit name or None)
_ONSURF = ('curve/crv_on_surf curve/iso_curve curve/tangent_field curve/geodesic_offset '
           'curve/crv_in_volume curve/projected_curve').split()
REMOVE = [
    # Re-review 2 E3: the intersection curve is a procedural kind over two surfaces (01 7.3); like
    # the on-surface kinds (S6) its dispatch arms leave the curve evaluator with it
    *[('eval/curve.rs', None, 'eval/curve.rs', n) for n in ('Intersection', 'solve')],
    *[('eval/curve.rs', n, 'eval/curve.rs', n) for n in ('point', 'jet')],
    ('eval/curve.rs', 'new', 'eval/surface.rs', None),
    # E2: the reader decodes the exact stored form (01 1.1 row 6, 11.1: stored form not a fixed
    # point); it does not heal what it reads, so make_topology loses its heal_made call
    ('topology/brep/p10.rs', 'make_topology', 'topology/brep/p10.rs', 'heal_made'),
    # E4, incidental edges from edits into classification. A mirror keeps each shell's region,
    # which is stored (01 2.1: the shell names its region, RegionId(0) is the infinite one), so it
    # does not find the infinite region again; a transform rewrites geometry and keeps ids, the
    # pcurves on its EdgeUses and its gaps (01 2.4, 2.1), so it does not re-cover moved trims
    ('topology/brep/mirror.rs', 'mirror_in', 'topology/brep/p5.rs', 'find_and_set_infinite_region'),
    *[('topology/brep/mirror.rs', 'mirror_in', 'query/brep.rs', n) for n in ('BrepQuery', 'of_brep')],
    ('topology/brep/p5.rs', 'transform_in', 'topology/edgeuse/p3.rs', 'cover_moved_trims'),
    # E4, the face cells brep keeps. A cell's read set is its face record and the geometry of the
    # edges and vertices it names (01 3.2 rule 2), so a fill cannot call the body classifier: the
    # boundary tree labels its leaves from the face's own loops. The face box is the box of the
    # surface, edges and vertices; the pin's `tight` flag becomes a separate tight box, a bodyq
    # read (it classifies its extreme points), so the box fill no longer branches into it
    ('query/face/p2.rs', 'impl:CacheFace', 'intersect/point_class.rs', None),
    ('query/face/p2.rs', 'impl:CacheFace', 'query/face/p2.rs', 'point_classify'),
    ('query/face/p1.rs', 'calculate_bounding_box_in', 'query/face/p1.rs', 'tight_face_box'),
    # mass uses a box only for the midpoint it integrates about (a precision shift), which the body
    # box cell gives (01 3.1); it does not need the tight box
    ('mass/integrator/properties.rs', 'estimate', 'query/brep.rs', 'calculate_tight_bounding_box'),
    # end_shell's structure check is Draft::finish's (01 2.3), in brep, not the check's
    ('topology/brep_constructor.rs', 'end_shell', 'topology/brep_constructor.rs', 'validate_pointers'),
    # geometric validity belongs to the check, which only reads (01 2.3): a surface's own validity
    # no longer dispatches into the curve-bounded and offset checks, which run intersections; the
    # check calls them
    ('surface/surface/assert_valid.rs', 'assert_valid_with_walk', 'surface/surface/assert_valid.rs',
     'curve_bounded_surface_assert_valid'),
    ('surface/surface/assert_valid.rs', 'assert_valid_with_walk', 'surface/surface/assert_valid/wrapped.rs',
     'offset_surface_assert_valid'),
    # the clearance proof returns the reaching vertices and edges; check turns them into reports (C3)
    ('topology/brep/validation/pointers.rs', None, 'topology/brep/validation.rs', 'Reports'),
    ('topology/brep/validation/pointers.rs', 'check_vertex_clearance',
     'topology/brep/validation/pointers.rs', 'report_vertex_exclusion'),
    ('topology/brep/mirror.rs', 'transform_matrix_with_notifications', 'topology/brep/mirror.rs',
     'degenerate_low_work_transform'),
    # AnyCurve: the on-surface variants leave Curve; their dispatch arms move with them (3.3, S6).
    *[('curve/curve', None, t, None) for t in _ONSURF],
    # curve's own wrappers (offset, composite) bound their nesting with a curve-only walk; the
    # cross-kind walk stays in surface (9.3, N1)
    ('curve/curve/p2.rs', 'clone_for_evaluation_depth', 'surface/surface/p2.rs', 'Surface'),
    *[('curve/', None, 'surface/surface/p2.rs', n) for n in
      ('check_geometry_nesting', 'NestedGeometry', 'check_curve_evaluation_copy_depth')],
    # optional-surface branches (9.3, O1): the surface-free method no longer forwards to the
    # on-surface one with `None`; the on-surface one, and the trim checks of WR-560 that took
    # `trim_surface: Option<&Surface>`, are their own functions in surface
    ('curve/curve/p6.rs', 'deform_to_end_point_targets', 'curve/curve/p6.rs',
     'deform_to_end_point_targets_on_surface'),
    ('curve/line.rs', 'assert_valid', 'curve/line.rs', 'assert_valid_on'),
    ('curve/ellipse/p1.rs', 'assert_valid', 'curve/ellipse/p1.rs', 'assert_valid_on'),
    ('nurbs/b_spline_curve/p3.rs', 'DerivedOverrides', 'surface/surface/p2.rs', 'Surface'),
    *[('nurbs/b_spline_curve/p3.rs', 'assert_valid_with', t, n) for t, n in (
        ('nurbs/b_spline_curve/p3.rs', 'stretch_clears_on_surface'),
        ('nurbs/b_spline_curve/p3.rs', 'trim_metric'),
        ('nurbs/b_spline_curve/p3.rs', 'trim_turn_on_surface'),
        ('nurbs/b_spline_curve/trim_image.rs', 'radius_clears_on_surface'))],
    # the surface-projected intersection's branch test goes with the on-surface kinds (S6)
    ('geom/global_solver.rs', 'branch_may_contain_answers', 'geom/global_solver.rs',
     'surface_projected_may_contain_answers'),
    ('curve/curve/p2.rs', 'may_contain_answers', 'geom/global_solver.rs',
     'surface_projected_may_contain_answers'),
    # knots of an on-surface or in-volume curve: cut at the curve's crossings with the knot
    # lines or planes by curve intersection, not by a curve classification (9.3, K1)
    ('curve/crv_on_surf', None, 'intersect/curve_class/', 'uv_discontinuity_intervals'),
    ('curve/crv_in_volume', None, 'intersect/curve_class/', 'volume_discontinuity_intervals'),
    ('surface/surf_of_extrusion/p2.rs', 'intersect_with_plane_on', 'surface/surf_of_extrusion/p2.rs', 'ClassifyHandle'),
    ('surface/surf_of_extrusion/p2.rs', 'intersect_with_plane_on', 'surface/surface/p2.rs', 'curve_on_classify'),
    # the planar boolean is a free function over two sheets, not a method of a primitive
    # builder, so merge's 2-D path no longer makes a builder to call it (P1)
    ('boolean/merge/p3.rs', 'boolean_2d', 'primitive/primitive_creation/p1.rs', 'PrimitiveCreation'),
    ('boolean/merge/p3.rs', 'boolean_2d', 'primitive/primitive_creation/p1.rs', 'new'),
    # the same for the extrusion-plane line: it is cut at its crossings with the surface's
    # natural uv boundaries, so its interval classification goes (K1)
    ('surface/surf_of_extrusion/p2.rs', 'intersect_with_plane_on', 'intersect/curve_class/', None),
    ('surface/surf_of_extrusion/p2.rs', 'intersect_with_plane_on', 'intersect/point_class.rs', None),
]

# The design graph (5.1 as changed by Re-review 2): each crate's direct dependencies.
# The four edges ruled 11-27 after the re-measure (56 Appendix C.2) are included.
DEPS = {
    'num': '', 'algo': 'num', 'nurbs': 'num',
    'nurbs-surface': 'nurbs algo', 'curve': 'nurbs algo',
    'surface': 'nurbs-surface curve', 'planar': 'curve',
    'sketch': 'planar', 'teeth': 'planar',
    'ssi': 'surface', 'brep': 'surface', 'bodyq': 'brep planar ssi', 'record': 'brep', 'tess': 'brep',
    'mass': 'brep',
    'check': 'bodyq ssi mass', 'stitch': 'bodyq', 'boolean': 'bodyq ssi stitch', 'hlr': 'bodyq',
    'make': 'boolean stitch planar check hlr', 'repair': 'boolean stitch check',
    'mesh': 'tess make mass', 'deform': 'stitch check',
    'local': 'make mass', 'taper': 'make', 'sheetmetal': 'make',
    'blend': 'local repair', 'offset': 'blend taper',
    'weld': 'blend', 'frames': 'make', 'features': 'offset',
}

# Sinking (prove.py step 5a): largest helper set that may move down, and crates that keep theirs.
SINK_CAP = 2000
NO_SINK = ('record',)
# bodyq rises whole too (E4): an edit that classifies goes above brep with its classification
RISE_WHOLE = ('record', 'boolean', 'stitch', 'ssi', 'bodyq')
# Whole sinks (B.4): the part of the second crate that the first one's code reaches moves down.
# Re-review 2 drops brep <- bodyq (E4: classification stays in bodyq) and ssi <- check (E8).
SINK_WHOLE = [('surface', 'blend')]
SINK_TYPES = True
HUBS = ('topology/', 'topology.rs', 'surface/surface', 'curve/curve', 'surface.rs', 'curve.rs')
