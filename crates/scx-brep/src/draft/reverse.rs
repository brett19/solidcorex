// Written for solidcorex. Apache-2.0.
//! A sheet turned over (research 75 MK17; the pin's `reverse_sheet`, `SmFace::SwapUV` on each
//! face): [`Draft::reverse_sheet`].
//!
//! Each face's normal is the other way at every point, and nothing else moves.
//!
//! - **The surface** is its own [`Surface::reversed`] along `v`: `S'(u, -v) = S(u, v)`
//!   (a sphere's latitude runs `1 - v`), exact for every kind. The same points, the other
//!   normal.
//! - **The pcurves** follow by the [`UvMap`](scx_surface::UvMap) the reversal returns, as a
//!   [`Draft::transform`] maps them (a line exactly, a kind that keeps its parameter by its
//!   own image, any other made again on the new surface).
//! - **The loops** run the other way round: the outer loop is counter-clockwise about the
//!   front, and the front is now the other side, so each loop's uses are taken in reverse
//!   order with each sense flipped. Each use's pcurve and interval are kept, since a use in
//!   [`Sense::Reversed`] runs its pcurve from the end of the interval.
//! - **The edges, vertices and shells are the same.** Both sides of a sheet's face are in
//!   one shell of the outside region, so neither the sides nor the regions change, and the
//!   edges keep their ids and curves.
//!
//! Reversing twice gives the sheet back: the negation of `v` is exact, and the loops return
//! in their order with their senses.
//!
//! A face of a solid (its sides in different shells, or in a solid region) is refused: with
//! its sides kept its solid would be inside out. So is an edge of more than two fins, whose
//! radial order depends on the surfaces' geometry and which this edit does not rewrite.

use core::fmt;

use scx_curve::ParamShift;
use scx_num::error::ErrorKind;
use scx_num::frame::Sense;
use scx_num::units::{Length, Resolution};
use scx_nurbs_surface::ParamAxis;

use super::Draft;
use super::transform::{EdgeImage, TransformError, use_image};
use crate::document::Document;
use crate::id::{EdgeId, FaceId};
use crate::record::{EdgeUse, LoopRecord};

/// Why [`Draft::reverse_sheet`] refused. The draft is unchanged.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ReverseError {
    /// The face is a side of a solid: its two sides are in different shells, or in a solid
    /// region.
    NotSheet {
        /// The face.
        face: FaceId,
    },
    /// The edge has more than two fins.
    NonManifold {
        /// The edge.
        edge: EdgeId,
    },
    /// A use's pcurve could not follow the reversed surface (the transform's causes).
    Pcurve(TransformError),
}

impl ReverseError {
    /// The kind: [`ErrorKind::Refused`] for a face of a solid, [`ErrorKind::Unsupported`]
    /// for an edge of more than two fins, and the pcurve cause's own.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::NotSheet { .. } => ErrorKind::Refused,
            Self::NonManifold { .. } => ErrorKind::Unsupported,
            Self::Pcurve(e) => e.kind(),
        }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NotSheet { .. } => "reverse_not_sheet",
            Self::NonManifold { .. } => "reverse_non_manifold",
            Self::Pcurve(e) => e.name(),
        }
    }
}

impl fmt::Display for ReverseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSheet { face } => {
                write!(f, "reverse: face {face} is a side of a solid, not a sheet")
            }
            Self::NonManifold { edge } => {
                write!(f, "reverse: edge {edge} has more than two fins")
            }
            Self::Pcurve(e) => write!(f, "reverse: {e}"),
        }
    }
}

impl core::error::Error for ReverseError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Pcurve(e) => Some(e),
            _ => None,
        }
    }
}

impl Draft {
    /// Turns every face over (module docs): the surfaces reversed, the loops run the other
    /// way, the same edges. The draft's faces must all be sheet faces.
    ///
    /// A [`Draft::finish`] after it reports each face as replaced and no edge, vertex or
    /// shell as changed beyond the fins' senses.
    ///
    /// # Errors
    ///
    /// [`ReverseError`], leaving the draft unchanged: a face that is a side of a solid, an
    /// edge of more than two fins, an edge with no curve yet, or a pcurve that cannot follow
    /// the reversed surface.
    pub fn reverse_sheet(&mut self, doc: &Document) -> Result<(), ReverseError> {
        if let Some(&edge) = self.pending.first() {
            return Err(ReverseError::Pcurve(TransformError::Pending { edge }));
        }
        for (id, f) in self.t.faces.iter() {
            let [a, b] = f.sides;
            let solid = |s| {
                self.t
                    .shells
                    .get(s)
                    .and_then(|s| self.t.regions.get(s.region))
                    .is_none_or(|r| r.solid)
            };
            if a != b || solid(a) {
                return Err(ReverseError::NotSheet { face: id });
            }
        }
        if let Some((edge, _)) = self.t.adjacency.iter().find(|(_, a)| a.fins.len() > 2) {
            return Err(ReverseError::NonManifold { edge });
        }
        let zone = doc.resolution().zone();
        let mut done = Vec::new();
        for (id, f) in self.t.faces.iter() {
            let (surface, uv) = f.surface.reversed(ParamAxis::V);
            let mut loops = Vec::with_capacity(f.loops.len());
            for l in &f.loops {
                let mut uses = Vec::with_capacity(l.uses.len());
                for u in l.uses.iter().rev() {
                    let g = self.t.edges.get(u.edge).ok_or(ReverseError::Pcurve(
                        TransformError::MissingEdge {
                            face: id,
                            edge: u.edge,
                        },
                    ))?;
                    let image = EdgeImage {
                        id: u.edge,
                        shape: g.shape.clone(),
                        accuracy: g.accuracy.clone(),
                        shift: ParamShift::NONE,
                    };
                    let reach = |r: Option<&std::sync::Arc<scx_num::cert::Recorded>>| {
                        r.map_or(zone, |r| r.acc.length())
                    };
                    let reach: Length = reach(u.bound.as_ref()).max(reach(g.accuracy.as_ref()));
                    let res = Resolution::new(reach * 2.0).unwrap_or_else(|| doc.resolution());
                    let geom = use_image(id, u, &image, &surface, &uv, res)
                        .map_err(ReverseError::Pcurve)?;
                    let mut out = EdgeUse::new(u.edge, flip(u.sense), geom);
                    out.bound = u.bound.clone();
                    uses.push(out);
                }
                loops.push(LoopRecord::new(uses));
            }
            done.push((id, surface, loops));
        }
        for (id, surface, loops) in done {
            self.edit_face(id, |f: &mut crate::record::FaceRecord| {
                f.surface = surface;
                f.loops = loops;
            });
        }
        Ok(())
    }
}

const fn flip(s: Sense) -> Sense {
    match s {
        Sense::Forward => Sense::Reversed,
        Sense::Reversed => Sense::Forward,
    }
}
