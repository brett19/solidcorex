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

mod betti;
mod classify;
mod error;
mod geom;
mod near;
mod on;
mod rays;
mod side;
mod tight;

pub use betti::{Betti, betti};
pub use classify::{PointClass, classify_point};
pub use error::{BodyqDefect, BodyqError, BodyqRefusal, BodyqUnsupported};
pub use rays::RAYS;
pub use side::{Along, Sides, side_at_touch, sides_along};
pub use tight::{tight_body_box, tight_box, tight_box_charged};

#[cfg(test)]
mod tests;
