// Moved from solidcore `solidcore/crates/solidcore/src/geom/geom_utility.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmGeomUtility.cpp`,
// `source/SMLib/inc/SmGeomUtility.h`. The file is split over several plan units
// (`geom.geom_utility_p1`, `_p2`, ...); each lives in its own submodule.
// Changes: the numbered partitions `p1` and `p2` become modules named by what they hold
// (`plane`, `line`, `polygon`, `fit`); the sort helpers moved to `crate::sort`. The
// matrix helpers, `solve_quadratic_eqn` and `vector_project_to_plane` are not moved: scx-num
// and `crate::polynomial` hold them already. Results unchanged except where each module's
// Deviations say.
//! Free geometric helpers (`smgu_*`): planes, lines and segments, polygons and triangles,
//! best fits.
//!
//! Upstream: `source/SMLib/src/SmGeomUtility.cpp`, `source/SMLib/inc/SmGeomUtility.h`.
//!
//! Where the pin's `smgu_*` functions went:
//!
//! | Upstream | Here |
//! |---|---|
//! | `smgu_IntersectThreePlanes`, `smgu_IntersectTwoPlanes`, `smgu_PlanePointDistance`, `smgu_PointProjectToPlane` | [`plane`] |
//! | `smgu_LinePointDistance`, `smgu_LineClosestPoint`, `smgu_LineLineClosestPoint`, `smgu_LinePlaneIntersect` (both), `smgu_SegmentPointDistance`, `smgu_SegmentPointAngleDegToPerp` | [`line`] |
//! | `smgu_PolygonArea`, `smgu_TriangleAreaSquared`, `smgu_TriangleDecomposition`, `smgu_ParallelogramCrossVectors` | [`polygon`] |
//! | `smgu_BestFitPointToPoints`, `smgu_BestFitLineToPoints`, `smgu_BestFitPlaneToPoints` | [`fit`] |
//! | `smgu_AddSorted`, `smgu_ShellSort_double_array`, `smgu_DoubleLessThan` | [`crate::sort`] |
//! | `smgu_VecTripleProduct`, `smgu_3x3Determinate`, `smgu_3x3Inverse`, `smgu_4x4Determinate`, `smgu_4x4Inverse`, `smgu_4x4Multiply` | `scx_num::small_matrix` (already there) |
//! | `smgu_Determinant3Vectors` | `scx_num::vector3d::Vec3::triple_product` (`a · (b × c)`: the same value, rounded differently from the six-term expansion; `crate::pseudo_box` keeps the six terms where a point on a face must classify as upstream's does) |
//! | `smgu_3x3Copy`, `smgu_4x4Copy` | none: arrays are `Copy` |
//! | `smgu_SolveQuadraticEqn` | [`crate::polynomial::solve_quadratic`] (the pin's is a one-line forward to it) |
//! | `smgu_VectorProjectToPlane` | `scx_num::vector3d::Vec3::project_to_plane` (the same expression) |

pub mod fit;
pub mod line;
pub mod plane;
pub mod polygon;
