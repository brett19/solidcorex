// Written for solidcorex. Apache-2.0.
//! Queries of a finished body: which region a point lies in, or which face, edge or vertex
//! it lies on (research 72 §3, as ruled in §12.4 row 2).
//!
//! [`classify_point`] answers [`PointClass::In`] a region or [`PointClass::On`] a boundary
//! entity. There is no `Unknown`: a point that cannot be decided is a [`BodyqError`] with a
//! cause. Callers that want material ask `body.region(r).is_solid()`.
//!
//! **The method** (72 §12.4 row 2), in three steps, each a closed form on analytic
//! geometry:
//!
//! 1. **On.** The point is `On` an entity within coincidence of it (`scx_num::units::
//!    coincident` of the body's resolution and the entity's accuracy): a vertex, else an
//!    edge, else a face whose foot's uv is `Inside` it. The lowest dimension wins, then the
//!    lowest id. A foot whose uv is only `Near` the face's boundary, with no edge within
//!    coincidence, is not `On` (72 §12.4 row 2).
//! 2. **Nearest clean hit.** Rays from the point along the fixed table [`RAYS`], each to
//!    the body box's far side. The first ray whose nearest hit is *clean* decides: the
//!    point lies in the region on the side of the hit face that faces the ray's start, and a
//!    ray with no hit leaves the body, so the point is in the outside region. A hit is clean
//!    when it is a [`Contact::Crossing`](scx_surface::Contact) whose uv is `Inside` the face
//!    (not `Near`), farther than coincidence from the point, with no other hit, no overlap
//!    and no edge or vertex of the body within coincidence of the ray up to coincidence past
//!    it. The last rule catches a ray through an edge that both faces' uv tests place just
//!    outside.
//! 3. **Nearest boundary point.** When no ray is clean, the nearest point of the boundary
//!    decides, found by closed-form projection onto the faces, edges and vertices the box
//!    tree names near the point: at a face's foot the face's side, at an edge or a vertex
//!    the side of the angle-weighted pseudonormal of the faces round it.
//!
//! **Reads only.** The body is `&Brep`. The cells it reads (the box tree, each face's
//! boundary tree) are filled on first read by `scx-brep`; the work charged to the budget
//! does not depend on whether they were warm.
//!
//! **Deterministic.** Faces are visited in id order (along a ray, by where the ray enters
//! their boxes, then id) and rays in table order, so the same
//! body and point give the same answer and the same work. The answer does not depend on
//! which clean ray decides: any clean ray gives the same region (a test checks every one).
//!
//! **Sides at a meeting** (72 §3.5 BQ2, §14.27). [`sides_along`] says which of the other
//! body's regions a face lies in on each side of a curve where a face of the other body
//! meets it, and [`side_at_touch`] round a point where the two touch: the Boolean's seeds
//! (BO4). Both read the two surfaces' normals and curvatures at the meeting (see
//! the note on [`sides_along`]).
//!
//! **Across an edge** (research 74 §25). [`tangent_all_along`] proves or refutes that the
//! faces across an edge meet within an angle at every point; [`continuity`] proves the
//! order of contact (G0 to G4) and, apart from it, measures the drops. Both enclose each
//! face's normal field along the edge curve with Taylor models, splitting the curve as the
//! proof needs under one budget.
//!
//! **Picking and stored references** (research 74 §44). [`pick_at`] returns every face, edge
//! or vertex within an aperture of each point, sorted by (distance, id); [`still_names`] says
//! which entities of a body descend, by the edits' [`Changes`](scx_brep::Changes), from stored
//! references, and which of them carry the stored point.
//!
//! **One surface** (research 74 §46). [`faces_on_one_surface`] grows a set of faces across the
//! edges where the faces are proved to lie on one surface (equal, or planes within the
//! resolution by `scx_surface::plane_bound`, ST5's proof), with the reason each other edge
//! stopped it.
//!
//! **Vertex joints and wall geometry** (research 74 §34, BW11). [`leaving`], [`turn_at`],
//! [`convex_at`] and [`joint_turn`] read a joint of edges at a vertex. [`facing_planes`],
//! [`facing_cylinders`], [`wall_holds`], [`planes_meeting`], [`move_edge_onto_line`],
//! [`wall_frame`] and the cap tests are the closed forms of a thin wall's mid-sheet, each
//! comparing against the [`Near`] it is given (a coincidence held over a reach) instead of a
//! fixed angle or length. [`face_surface`] views a face's surface, and the round-face family
//! ([`past_end`], [`stands_over`], [`beside_clear`], [`whole_band`], [`extent_in`],
//! [`grow_block`]) reads a body about a seat through probes the caller answers.

mod along;
mod betti;
mod cap;
mod classify;
mod continuity;
mod cylinder_wall;
mod error;
mod face_pair;
mod face_surface;
mod geom;
mod joint;
mod mid_joint;
mod mid_sheet;
mod near;
mod on;
mod one_surface;
mod pick_at;
mod rays;
mod reach;
mod recognize;
mod round_face;
mod side;
mod still;
mod tangent;
mod tight;

pub use betti::{Betti, betti};
pub use cap::{
    Lies, PlaneAt, PrismCut, PrismPart, Reach, curve_lies_on_face, is_face_on_plane,
    is_point_on_plane, is_square_plane_at_height, prism_part,
};
pub use classify::{PointClass, classify_point};
pub use continuity::{Continuity, ContinuityTolerances, Limit, Measured, Order, continuity};
pub use cylinder_wall::{WallFrame, Wrap, wall_frame};
pub use error::{BodyqDefect, BodyqError, BodyqRefusal, BodyqUnsupported};
pub use face_pair::{
    Apart, CylinderFace, Directions, Facing, FacingCylinders, FacingPlanes, OutwardPlane,
    axial_range_in, cylinder_face, facing_cylinders, facing_planes, material_toward_axis,
    outward_plane, round_directions,
};
pub use face_surface::{Closest, FaceSurface, face_surface};
pub use joint::{Convexity, Leaving, Missing, Turn, convex_at, joint_turn, leaving, turn_at};
pub use mid_joint::{Line3, Meeting, Moved, move_edge_onto_line, planes_meeting};
pub use mid_sheet::{Holds, Wall, from_frame, in_frame, onto_plane, polygon_area, wall_holds};
pub use one_surface::{NotCrossed, NotOneSurface, OneSurface, Stopped, faces_on_one_surface};
pub use pick_at::{Picked, pick_at};
pub use rays::RAYS;
pub use reach::Near;
pub use recognize::{
    Bore, Equivalence, Layer, Lone, NotPrism, Options as StackOptions, Piece, PrismSection,
    Recognition, Slot, Stack, Stacks, Why, bores, hole_stacks, prism_equivalent, prism_section,
    prism_section_charged, stack_bores,
};
pub use round_face::{
    BESIDE, Band, End, Extent, GrowBlock, Inward, Met, Probe, RIDGE, SAME_SIZE, SLOPE_TIE,
    SeatFrame, beside_clear, extent, extent_in, far_end, grow_block, into_seat, past_end,
    stands_over, whole_band,
};
pub use side::{Along, Sides, side_at_touch, sides_along};
pub use still::{Standing, Step, Stored, still_names};
pub use tangent::{Tangency, tangent_all_along};
pub use tight::{tight_body_box, tight_box, tight_box_charged};

#[cfg(test)]
mod tests;
