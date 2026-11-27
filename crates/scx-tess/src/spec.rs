// Written for solidcorex. Apache-2.0.
//! [`TessSpec`]: what a tessellation is asked to hold.

use scx_num::units::{Length, Resolution};

use crate::error::{TessError, TessRefusal};

/// What a tessellation holds: a chord bound, and optionally a turning bound and a longest
/// segment. Every value is checked when the spec is made, against the resolution of the
/// document its bodies belong to; a body of another resolution is refused
/// ([`TessRefusal::Resolution`]).
///
/// - **Chord.** No point of a segment or triangle lies farther than `chord` from the curve
///   or surface it stands for, and no point of those farther from the mesh. A proved bound,
///   not a target (research 72 §6.2, §12.4 row 6).
/// - **Angle** (radians). No segment turns its curve's tangent by more, and none is longer
///   than the length over which an adjacent face's normal could turn by more.
/// - **Edge length.** No segment is longer.
///
/// There is no default: an invalid chord is refused, never replaced (the pin's 0.05 mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TessSpec {
    chord: Length,
    resolution: Resolution,
    angle: Option<f64>,
    edge_length: Option<Length>,
}

impl TessSpec {
    /// A spec holding `chord`, for bodies of resolution `resolution`.
    ///
    /// # Errors
    ///
    /// [`TessRefusal::Chord`] for a chord that is not finite or not above the resolution's
    /// zone: a bound below what the document can tell apart proves nothing.
    pub fn new(chord: Length, resolution: Resolution) -> Result<Self, TessError> {
        if above(chord, resolution) {
            Ok(Self {
                chord,
                resolution,
                angle: None,
                edge_length: None,
            })
        } else {
            Err(TessError::Refused(TessRefusal::Chord { value: chord.mm() }))
        }
    }

    /// The same, holding the turning bound `radians` too.
    ///
    /// # Errors
    ///
    /// [`TessRefusal::Angle`] for an angle that is not finite or not above zero.
    pub fn with_angle(self, radians: f64) -> Result<Self, TessError> {
        if radians.is_finite() && radians > 0.0 {
            Ok(Self {
                angle: Some(radians),
                ..self
            })
        } else {
            Err(TessError::Refused(TessRefusal::Angle { value: radians }))
        }
    }

    /// The same, with no segment longer than `length`.
    ///
    /// # Errors
    ///
    /// [`TessRefusal::EdgeLength`] for a length that is not finite or not above the
    /// resolution's zone.
    pub fn with_edge_length(self, length: Length) -> Result<Self, TessError> {
        if above(length, self.resolution) {
            Ok(Self {
                edge_length: Some(length),
                ..self
            })
        } else {
            Err(TessError::Refused(TessRefusal::EdgeLength {
                value: length.mm(),
            }))
        }
    }

    /// The chord bound.
    #[must_use]
    pub const fn chord(&self) -> Length {
        self.chord
    }

    /// The resolution the spec was checked against.
    #[must_use]
    pub const fn resolution(&self) -> Resolution {
        self.resolution
    }

    /// The turning bound, in radians.
    #[must_use]
    pub const fn angle(&self) -> Option<f64> {
        self.angle
    }

    /// The longest segment.
    #[must_use]
    pub const fn edge_length(&self) -> Option<Length> {
        self.edge_length
    }

    /// Refuses a body of another resolution.
    pub(crate) fn check(&self, body: Resolution) -> Result<(), TessError> {
        if body == self.resolution {
            Ok(())
        } else {
            Err(TessError::Refused(TessRefusal::Resolution {
                spec: self.resolution.zone().mm(),
                body: body.zone().mm(),
            }))
        }
    }
}

fn above(l: Length, res: Resolution) -> bool {
    l.mm().is_finite() && l.mm() > res.zone().mm()
}
