"""Proposed mapping of kernel source files to target libraries (report 49).  JUDGED, not measured.

lib_of(relpath) -> (lib, confidence, note).  Rules are matched by longest prefix of the file path
relative to src/.  Confidence: H = doc comment and measured references agree, M = plausible,
L = uncertain (mixed content, split by function not by file, or the target layering is unsettled).
RANK gives the extraction level; libs with equal rank are peers (should not depend on each other).
"""
RANK = {
    'num': 0, 'profile': 0, 'algo': 1, 'nurbs': 2,
    'evalrec': 3,    # not in the architecture doc: evaluators on plain records (eval/)
    'planar': 4,     # not in the architecture doc: the planar-profile core of prism/
    'curve': 5, 'surface': 5,
    'geomq': 6, 'brep': 7, 'topo': 8,
    # operation crates (peers)
    'blend': 9, 'boolean': 9, 'offset': 9, 'sweep': 9, 'primitive': 9, 'heal': 9, 'hlr': 9, 'develop': 9,
    'prism': 9, 'cell_grid': 9, 'hatch': 9, 'mesh_reduce': 9, 'defeature': 9,
    'record_convert': 10,
    'umbrella': 11,   # SolidError/Result/EntityRef, Kernel object, lib.rs
}
ORDER = sorted(RANK, key=lambda k: (RANK[k], k))

# (prefix, lib, conf, note); longest prefix wins.  A prefix ending in '/' matches a directory.
R = []


def add(lib, conf, note, *prefixes):
    for p in prefixes:
        R.append((p[:-3] if p.endswith('.rs') else p, lib, conf, note))


# ---- num / algo from geom/, math, limits
add('num', 'H', 'vector/extent/box value types', 'math.rs', 'geom/vector2d', 'geom/vector3d', 'geom/extent1d', 'geom/extent2d',
    'geom/extent3d', 'geom/periodic_extent1d', 'geom/pseudo_box', 'geom/polar_box', 'geom/point_set', 'geom/axis2_placement',
    'geom/step_matrix', 'geom/expansion', 'geom/interval_vector', 'geom/certified', 'geom.rs')
add('num', 'M', 'transform / geometric utilities / limits / dense matrices', 'geom/transform', 'geom/geom_utility', 'limits.rs', 'geom/plane_height', 'geom/matrix')
add('num', 'M', 'tolerance comparisons (are_same, is_zero, tol3d, constants) are scalar; the entity-tolerance helpers in the same file refer to bodies and must move to brep', 'geom/tol')
add('num', 'L', 'content digest; section helpers; unclear home', 'geom/content', 'geom/axial_section', 'geom/facing_trough')
add('num', 'M', 'interval arithmetic and Taylor models now inside blend/fillet_section', 'blend/fillet_section/interval',
    'blend/fillet_section/tay', 'blend/fillet_section/series', 'blend/fillet_section/taylor_model')
add('num', 'H', 'error type of geometry utilities', 'error/geom')
add('algo', 'H', 'solvers, trees, grids', 'geom/polynomial', 'geom/local_solve1d', 'geom/local_solve_nd',
    'geom/global_solver', 'geom/krawczyk', 'geom/solution_array', 'geom/tree', 'geom/grid', 'geom/hash_table')
# ---- nurbs
add('nurbs', 'H', 'NLib routines', 'nurbs/', 'nurbs.rs')
add('nurbs', 'H', 'error type', 'error/nurbs')
add('curve', 'H', 'SmBSplineCurve kernel-geometry wrapper', 'nurbs/b_spline_curve')   # prefix matches _n_lib too
add('surface', 'H', 'SmBSplineSurface kernel-geometry wrapper', 'nurbs/b_spline_surface')
add('surface', 'M', 'builds surfaces from curve networks', 'nurbs/gordon', 'nurbs/n_sided')
add('sweep', 'M', 'certificate of a railed loft edge (refs offset, query)', 'nurbs/certificate')
# ---- evalrec
add('evalrec', 'M', 'evaluators on plain records', 'eval/', 'eval.rs')
# ---- planar core of prism
add('planar', 'M', 'planar profile core', 'prism/profile', 'prism/export_geometry', 'prism/import_geometry', 'prism/path_export',
    'prism/recognition', 'prism/fit', 'prism/placement', 'prism/gusset', 'prism/member', 'prism/extrude')
add('prism', 'M', 'prism operations on bodies', 'prism/read', 'prism/measure', 'prism/sheet', 'prism.rs')
# ---- curve / surface
add('curve', 'H', 'curve kinds', 'curve/', 'curve.rs')
add('curve', 'H', 'error type', 'error/curve')
add('record_convert', 'M', 'constructors from record numbers / STEP intervals', 'curve/from_parameters', 'curve/arc_of', 'curve/step_edge',
    'surface/from_parameters', 'surface/face_evaluation', 'surface/face_pair', 'surface/face_pair_cylinder', 'surface/face_surface',
    'surface/step_seam', 'surface/step_window', 'surface/cylinder_wall', 'nurbs/iges')
add('surface', 'M', 'volume maps live with surfaces', 'curve/crv_in_volume', 'curve/geodesic_offset', 'curve/tangent_field')
add('surface', 'H', 'surface kinds', 'surface/', 'surface.rs')
add('offset', 'M', 'sheet mid-surface helpers', 'surface/mid_joint', 'surface/mid_sheet')
add('surface', 'H', 'error type', 'error/surface')
add('surface', 'M', 'tessellation cache of a trimmed surface', 'query/tess')
# ---- geomq
add('geomq', 'M', 'surface/surface intersection, tracing, dropping, ray tracing', 'intersect/adv_surface_intersector',
    'intersect/surface_intersector', 'intersect/surface_tracer', 'intersect/surface_silhouette', 'intersect/surface_drop_curve',
    'intersect/surface_drop_lib', 'intersect/ray_tracer', 'intersect/views', 'intersect.rs')
add('geomq', 'M', 'geometry-only queries', 'query/curve', 'query/deviation', 'query/distance_bound', 'query/normal',
    'query/point_identity', 'query/line_meeting', 'query/vertex_meet', 'query/curve_surface_gap', 'query/seam_bound')
add('geomq', 'M', 'error type', 'error/intersect')
# ---- brep
add('brep', 'M', 'the body: ids, arenas, entity accessors and edit primitives', 'topology.rs', 'topology/stamped', 'topology/topology',
    'topology/integrity', 'topology/brep_constructor', 'topology/vertex', 'topology/vertexuse', 'topology/edge', 'topology/edgeuse',
    'topology/loopuse', 'topology/shell', 'topology/brep/p1', 'topology/brep/p2', 'topology/brep/p3', 'topology/brep/p4',
    'topology/brep/p5', 'topology/brep/p6', 'topology/brep/p8', 'topology/brep/p9', 'topology/face/p1', 'topology/face/p2',
    'topology/face/p4', 'topology/face', 'topology/brep')
add('brep', 'M', 'attributes, change notification, per-session context, tolerance policy', 'geom/attribute', 'geom/a_object',
    'geom/object', 'geom/context')
add('brep', 'M', 'error type', 'error/topology')
# ---- topo
add('topo', 'M', 'trims, validity, section, traversal, trees on bodies', 'topology/face/p3', 'topology/face/assert_tests',
    'topology/face/curve_on', 'topology/edgeuse/p3', 'topology/edge/edge_form', 'topology/brep/validation', 'topology/brep/enclosing',
    'topology/brep/outer_shells', 'topology/brep/planar_section', 'topology/brep/sheet', 'topology/brep/side_extend',
    'topology/brep/trim_extend', 'topology/brep/seam_turn', 'topology/brep/primitive_outward', 'topology/brep/vertex_fans',
    'topology/brep/mirror', 'topology/trimming_tools', 'topology/brep_cache', 'topology/topology_traverser')
add('topo', 'M', 'queries on bodies', 'query/api_queries', 'query/brep', 'query/contact', 'query/continuity', 'query/edge',
    'query/edge_turn', 'query/face', 'query/face_surface', 'query/faces_continue', 'query/flush_contact', 'query/loop',
    'query/member_turn', 'query/poly', 'query/poly_properties', 'query/tangent_edge', 'query.rs')
add('topo', 'M', 'classification and solving against topology', 'intersect/curve_class', 'intersect/point_class',
    'intersect/topology_solver', 'intersect/api_intersectors')
add('topo', 'M', 'healer property/gap records are used by topology, query and intersect (63+28+6 refs), so they sit with the body algorithms', 'heal/face_props', 'heal/gap')
add('topo', 'M', 'mass properties and bounds (quadrature core could go to algo)', 'mass/', 'mass.rs')
add('topo', 'L', 'selection tools and AssertValid bookkeeping on bodies', 'geom/tools', 'geom/assert_array')
add('topo', 'M', 'error type', 'error/mass')
add('num', 'L', 'query failure type is used from nurbs upward; lowest user decides', 'error/query')
# ---- ops crates
add('boolean', 'M', 'boolean and its proto topology; planar cut', 'boolean/', 'boolean.rs', 'topology/proto_topology', 'topology/brep/cutting')
add('boolean', 'M', 'error type', 'error/boolean')
add('blend', 'H', 'blends', 'blend/', 'blend.rs')
add('blend', 'H', 'error type', 'error/blend')
add('hlr', 'M', 'hidden curve removal / projections', 'blend/hcr', 'topology/api_projection', 'hlr/', 'hlr.rs')
add('defeature', 'M', 'defeaturing executive', 'blend/feature_executive', 'error/defeature')
add('offset', 'H', 'offset, shell, thicken', 'offset/', 'offset.rs', 'topology/brep/local_operation')
add('offset', 'H', 'error type', 'error/offset')
add('sweep', 'H', 'sweeps', 'sweep/', 'sweep.rs')
add('sweep', 'H', 'error type', 'error/sweep')
add('primitive', 'H', 'primitives and SM_API creators', 'primitive/', 'primitive.rs')
add('primitive', 'H', 'error type', 'error/primitive')
add('heal', 'H', 'heal and stitch', 'heal/', 'heal.rs')
add('heal', 'H', 'error type', 'error/heal')
add('develop', 'H', 'sheet-metal development', 'develop/', 'develop.rs')
add('prism', 'M', 'prism root files and facade', 'prism/')
add('cell_grid', 'H', 'cell grids', 'cell_grid/', 'cell_grid.rs')
add('hatch', 'H', 'hatching', 'hatch/', 'hatch.rs')
add('mesh_reduce', 'H', 'mesh reduction (depends on nothing but error)', 'mesh_reduce/', 'mesh_reduce.rs')
add('profile', 'H', 'opt-in profiler', 'profile.rs')
# ---- record / convert
add('record_convert', 'M', 'serialized B-rep data and database I/O', 'topology/database_io', 'topology/part_file', 'topology/api_files',
    'topology/brep_data', 'topology/brep_data_write', 'topology/geometry_write', 'topology/brep/p7', 'topology/brep/p10')
# ---- umbrella
add('umbrella', 'M', 'SolidError / Result / EntityRef umbrella (each crate gets its own enum; this wraps them)', 'error.rs')
add('umbrella', 'L', 'Kernel object (settings, rng, cancel, caches, counters); must be split: Tolerances/Rng/CancelToken low, Caches/Counters high',
    'kernel.rs')
add('umbrella', 'H', '', 'lib.rs')
add('umbrella', 'M', 'Sm* error code constants and Failure record', 'error/')

R.sort(key=lambda x: -len(x[0]))


SPECIAL = {'nurbs/b_spline_curve', 'nurbs/b_spline_surface'}   # also match the *_n_lib files


def lib_of(rel):
    base = rel[:-3] if rel.endswith('.rs') else rel
    for p, lib, conf, note in R:
        if p.endswith('/'):
            if rel.startswith(p): return lib, conf, note
        elif base == p or base.startswith(p + '/') or (p in SPECIAL and base.startswith(p)):
            return lib, conf, note
    raise KeyError(rel)
