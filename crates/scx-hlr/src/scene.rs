// Written for solidcorex. Apache-2.0.
//! A scene: bodies placed by rigid frames, and the box that holds them.
//!
//! Each body keeps its own coordinates; the view is carried into them (its axes' components
//! along the body's frame) and the curves found there are placed back in the world. The
//! closed forms are computed in each surface's own frame, so a body far from the world's
//! origin, or a face far from its body's, loses no precision to the placement beyond the
//! rounding of the final move.

use scx_bodyq::tight_body_box;
use scx_brep::Brep;
use scx_brep::op::{Cx, Event};
use scx_num::extent3d::Extent3d;
use scx_num::frame::Frame;
use scx_num::vector3d::Point3;

use crate::error::HlrError;
use crate::outline::{self, Outline};
use crate::silhouette::{Silhouettes, of_body};
use crate::view::{View, place_point};

/// Bodies placed in the world, with the box that holds them.
#[derive(Debug, Clone)]
pub struct Scene<'a> {
    bodies: Vec<(&'a Brep, Frame)>,
    bounds: Option<Extent3d>,
}

/// The scene of `bodies`, each placed by its frame (body coordinates `(a, b, c)` are the
/// world point `o + a x + b y + c z`). The scene's box is each body's tight box
/// (`scx_bodyq::tight_body_box`: a sphere's box is its own, not its two poles') placed in
/// the world.
///
/// # Errors
///
/// [`HlrError::Stopped`]; [`HlrError::Bodyq`] when a body's tight box cannot be found;
/// [`HlrError::BadPlacement`] when a placed box is not finite.
pub fn prepare<'a>(bodies: &[(&'a Brep, Frame)], cx: &mut Cx<'_>) -> Result<Scene<'a>, HlrError> {
    cx.trace.event(Event::Phase("hlr.prepare"));
    let mut bounds: Option<Extent3d> = None;
    for (i, (body, frame)) in bodies.iter().enumerate() {
        cx.stop.check()?;
        let b = tight_body_box(body, cx)?;
        let placed = Extent3d::hull(b.corners().map(|p| place_point(frame, p)))
            .ok_or(HlrError::BadPlacement { body: i })?;
        bounds = Some(bounds.map_or(placed, |s| s.union(placed)));
    }
    Ok(Scene {
        bodies: bodies.to_vec(),
        bounds,
    })
}

impl Scene<'_> {
    /// The bodies and their placements.
    #[must_use]
    pub fn bodies(&self) -> &[(&Brep, Frame)] {
        &self.bodies
    }

    /// The box holding every body; `None` for an empty scene.
    #[must_use]
    pub const fn bounds(&self) -> Option<Extent3d> {
        self.bounds
    }

    /// The centre of the box: the origin to measure screen coordinates from
    /// ([`View::screen`]).
    #[must_use]
    pub fn centre(&self) -> Option<Point3> {
        self.bounds.map(|b| b.mid())
    }

    /// The box's largest side, in mm (0 for an empty scene).
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.bounds.map_or(0.0, |b| b.max_dimension())
    }

    /// Every body's silhouettes under `view`, in world coordinates, in the scene's order,
    /// on one budget.
    ///
    /// # Errors
    ///
    /// As [`crate::silhouettes`]; [`HlrError::BadPlacement`] when the view cannot be
    /// carried into a body's frame.
    pub fn silhouettes(&self, view: &View, cx: &mut Cx<'_>) -> Result<Vec<Silhouettes>, HlrError> {
        let mut budget = cx.budget();
        let mut out = Vec::with_capacity(self.bodies.len());
        for (i, (body, frame)) in self.bodies.iter().enumerate() {
            let local = view
                .in_frame(frame)
                .ok_or(HlrError::BadPlacement { body: i })?;
            let found = of_body(body, &local, &mut budget, cx.stop, cx.trace)?;
            let curves = found
                .curves
                .into_iter()
                .map(|s| s.placed(frame))
                .collect::<Result<Vec<_>, _>>()?;
            out.push(Silhouettes {
                curves,
                unsupported: found.unsupported,
            });
        }
        Ok(out)
    }

    /// Every body's outline stretches under `view`, in world coordinates, in the scene's
    /// order, on one budget.
    ///
    /// # Errors
    ///
    /// As [`crate::outline`]; [`HlrError::BadPlacement`] when the view cannot be carried
    /// into a body's frame.
    pub fn outlines(&self, view: &View, cx: &mut Cx<'_>) -> Result<Vec<Outline>, HlrError> {
        let mut budget = cx.budget();
        let mut out = Vec::with_capacity(self.bodies.len());
        for (i, (body, frame)) in self.bodies.iter().enumerate() {
            let local = view
                .in_frame(frame)
                .ok_or(HlrError::BadPlacement { body: i })?;
            let found = outline::of_body(body, &local, &mut budget, cx.stop, cx.trace)?;
            out.push(found.placed(frame));
        }
        Ok(out)
    }
}
