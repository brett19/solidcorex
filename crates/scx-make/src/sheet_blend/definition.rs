// Written for solidcorex. Apache-2.0.
//! The section blend's definition record: what [`section_blend`] was built from and the
//! motions applied to its surface since, so the surface is rebuilt from it bit for bit.

use scx_num::math::Affine;
use scx_nurbs_surface::{NurbsSurface, ParamAxis};

use super::error::SheetBlendError;
use crate::blend::{Blended, EndOrder, SectionFrame, section_blend};

/// What a section blend was built from: its sections, end orders and crowns, and the
/// motions applied to it since, first to last.
///
/// The pin's `SectionBlendDefinition` with its `create`, `rebuild`, `recertify` and
/// `transform`. A motion is kept as an affine map applied to the rebuilt surface's control
/// points, not pushed into the frames: a scaling that is not uniform does not map a
/// curvature vector to the mapped curve's, so the frames could not carry it exactly, while
/// the map of the control points is the mapped surface exactly ([`NurbsSurface::transformed`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SectionBlendDefinition {
    sections: Vec<Vec<SectionFrame>>,
    orders: [EndOrder; 2],
    crown: [f64; 2],
    motions: Vec<Affine>,
}

impl SectionBlendDefinition {
    /// The blend of `sections` with `orders` and `crown` (as [`section_blend`]) and its
    /// definition, which rebuilds it.
    ///
    /// # Errors
    ///
    /// [`SheetBlendError::Blend`] with [`section_blend`]'s error; no definition is made of
    /// input that is not a blend.
    pub fn new(
        sections: Vec<Vec<SectionFrame>>,
        orders: [EndOrder; 2],
        crown: [f64; 2],
    ) -> Result<(Self, Blended), SheetBlendError> {
        let blended = section_blend(&sections, orders, crown)?;
        Ok((
            Self {
                sections,
                orders,
                crown,
                motions: Vec::new(),
            },
            blended,
        ))
    }

    /// The sections.
    #[must_use]
    pub fn sections(&self) -> &[Vec<SectionFrame>] {
        &self.sections
    }

    /// The end orders, first and last section.
    #[must_use]
    pub const fn orders(&self) -> [EndOrder; 2] {
        self.orders
    }

    /// The crowns, first and last section.
    #[must_use]
    pub const fn crown(&self) -> [f64; 2] {
        self.crown
    }

    /// The motions applied since the blend was built, first to last.
    #[must_use]
    pub fn motions(&self) -> &[Affine] {
        &self.motions
    }

    /// The same definition followed by the motion `a`: its rebuild is this one's surface
    /// mapped by `a`.
    #[must_use]
    pub fn transformed(&self, a: Affine) -> Self {
        let mut motions = self.motions.clone();
        motions.push(a);
        Self {
            motions,
            ..self.clone()
        }
    }

    /// The surface the definition stands for: the blend, then each motion.
    ///
    /// # Errors
    ///
    /// [`SheetBlendError::Blend`] when the blend fails (never for a definition from
    /// [`Self::new`], whose sections were a blend), [`SheetBlendError::Motion`] when a motion
    /// does not map the surface.
    pub fn rebuild(&self) -> Result<NurbsSurface<3>, SheetBlendError> {
        let mut surface = section_blend(&self.sections, self.orders, self.crown)?.surface;
        for (index, a) in self.motions.iter().enumerate() {
            surface = surface
                .transformed(a)
                .map_err(|cause| SheetBlendError::Motion { index, cause })?;
        }
        Ok(surface)
    }

    /// Whether `surface` is the definition's, bit for bit, in some orientation of its
    /// parameter square (`u` and `v` each reversed or not, and exchanged or not): a face may
    /// hold its surface turned, and the turns are exact.
    ///
    /// # Errors
    ///
    /// As [`Self::rebuild`].
    pub fn recertify(&self, surface: &NurbsSurface<3>) -> Result<bool, SheetBlendError> {
        let own = self.rebuild()?;
        let turns = |s: NurbsSurface<3>| {
            let u = s.reversed(ParamAxis::U);
            let v = s.reversed(ParamAxis::V);
            let uv = u.reversed(ParamAxis::V);
            [s, u, v, uv]
        };
        let swapped = own.swapped();
        Ok(turns(own)
            .into_iter()
            .chain(turns(swapped))
            .any(|t| &t == surface))
    }
}
