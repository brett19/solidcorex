// Written for solidcorex. Apache-2.0.
//! Every tolerance the silhouettes use, named once, each with the reason for its size.
//!
//! The pin's HLR carries about 116 literal tolerances; here there are three, and none of them
//! decides which pieces of a silhouette are kept. They decide only where a silhouette may be
//! split; an extra split point changes nothing (both halves classify alike and are merged
//! again), so each tolerance is chosen to be generous, never to be tight.

use scx_num::extent3d::Extent3d;
use scx_num::units::{Accuracy, Resolution};

/// Below this length of the view's component across a cylinder's or cone's axis (the view
/// is a unit vector), the view is taken as along the axis and the face has no silhouette
/// line. The direction `axis × view` is then rounding noise, so no line built from it would
/// mean anything; and a line lost this way projects to at most `height × PARALLEL` on the
/// screen, below one rounding error of the height.
pub const PARALLEL: f64 = 16.0 * f64::EPSILON;

/// The rounding of one coordinate relative to its size: a point computed from a closed
/// form at coordinates of size `s` is within `ROUNDING × s` of the true point.
pub const ROUNDING: f64 = 8.0 * f64::EPSILON;

/// A face's tolerances: from the body's resolution, the face's size and how far the surface
/// the silhouette is computed on lies from the face's own (zero unless recognised).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tolerances {
    zone: f64,
    size: f64,
    deviation: f64,
    recognition: Accuracy,
}

impl Tolerances {
    /// The tolerances of a face held by `face_box`, on a body of resolution `res`. The
    /// recognition accuracy is the resolution's approximation accuracy (half its zone):
    /// a surface recognised within it is the same surface at the body's resolution.
    pub(crate) fn of(face_box: &Extent3d, res: Resolution) -> Option<Self> {
        let size = face_box
            .min()
            .max_dimension()
            .max(face_box.max().max_dimension());
        Some(Self {
            zone: res.zone().mm(),
            size,
            deviation: 0.0,
            recognition: Accuracy::new(res.approx())?,
        })
    }

    /// The same, for a silhouette computed on a surface `deviation` mm from the face's.
    pub(crate) const fn with_deviation(self, deviation: f64) -> Self {
        Self { deviation, ..self }
    }

    /// The accuracy a NURBS face is recognised as an analytic surface within.
    #[must_use]
    pub(crate) const fn recognition(&self) -> Accuracy {
        self.recognition
    }

    /// How near a boundary edge's crossing of a splitting plane must lie to a silhouette
    /// to split it, in mm: the larger of the body's zone, the edge's recorded accuracy and
    /// the rounding of the coordinates, plus the surface's deviation. The edge lies on the
    /// face's surface within the first two, the closed forms are exact within the third and
    /// the silhouette's surface is within the last of the face's.
    #[must_use]
    pub(crate) fn near(&self, edge_accuracy: Option<Accuracy>) -> f64 {
        let acc = edge_accuracy.map_or(0.0, |a| a.length().mm());
        self.zone.max(acc).max(ROUNDING * self.size) + self.deviation
    }
}
