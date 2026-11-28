// Written for solidcorex. Apache-2.0.
//! The writer every constructor goes through: one draft with the outside and a solid
//! region decided at construction, vertices, edges made once and shared, and faces whose
//! loops get their exact pcurves from `Draft::exact_loops`.

use std::collections::BTreeMap;

use scx_brep::{
    Absent, Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, FaceId, FaceRecord, RegionId,
    ShellId, ShellRecord, VertexId, VertexRecord,
};
use scx_curve::{Circle, Curve, Line};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector3d::Point3;
use scx_surface::Surface;

use crate::error::{Defect, MakeError};

/// A body being written: one solid, or more by `begin_solid`, or a sheet.
pub(crate) struct Assembly<'d> {
    draft: Draft,
    doc: &'d Document,
    /// The shell of the outside region that faces are being added to: the front of every
    /// solid face. A sheet has none.
    outside: Option<ShellId>,
    /// The shell of the solid region faces are being added to: the back of every face. A
    /// sheet has none.
    solid: Option<ShellId>,
    /// The solid region faces are being added to. A sheet has none.
    region: Option<RegionId>,
    /// The straight edges made so far, by their ends in id order.
    lines: BTreeMap<(VertexId, VertexId), EdgeId>,
}

impl<'d> Assembly<'d> {
    /// An empty draft with the outside's shell and a solid region's.
    pub(crate) fn new(doc: &'d Document) -> Result<Self, MakeError> {
        let mut draft = Draft::new();
        let outside = draft.add_shell(ShellRecord::new(RegionId::OUTSIDE))?;
        let region = draft.add_region(true)?;
        let solid = draft.add_shell(ShellRecord::new(region))?;
        Ok(Self {
            draft,
            doc,
            outside: Some(outside),
            solid: Some(solid),
            region: Some(region),
            lines: BTreeMap::new(),
        })
    }

    /// An empty draft for sheets: no solid region, and a shell of its own for each face.
    pub(crate) fn sheets(doc: &'d Document) -> Self {
        Self {
            draft: Draft::new(),
            doc,
            outside: None,
            solid: None,
            region: None,
            lines: BTreeMap::new(),
        }
    }

    /// Starts another solid: a solid region with a shell of its own, and a shell of its own
    /// in the outside region (a shell must be connected, and two solids apart are not). The
    /// faces added from now on have them at their front and back.
    pub(crate) fn begin_solid(&mut self) -> Result<(), MakeError> {
        self.outside = Some(self.draft.add_shell(ShellRecord::new(RegionId::OUTSIDE))?);
        let region = self.draft.add_region(true)?;
        self.solid = Some(self.draft.add_shell(ShellRecord::new(region))?);
        self.region = Some(region);
        Ok(())
    }

    /// The shells in front of and behind the solid faces being added.
    pub(crate) fn sides(&self) -> Result<[ShellId; 2], MakeError> {
        match (self.outside, self.solid) {
            (Some(outside), Some(solid)) => Ok([outside, solid]),
            _ => Err(MakeError::Defect(Defect::NoSolid)),
        }
    }

    /// The shells either side of a new hollow in the solid being written: a void region's
    /// shell in front and a new shell of the solid behind (a shell must be connected, and
    /// the hollow's lining is apart from the solid's outer shell).
    pub(crate) fn hollow(&mut self) -> Result<[ShellId; 2], MakeError> {
        let region = self.region.ok_or(MakeError::Defect(Defect::NoSolid))?;
        let void = self.draft.add_region(false)?;
        let front = self.draft.add_shell(ShellRecord::new(void))?;
        let back = self.draft.add_shell(ShellRecord::new(region))?;
        Ok([front, back])
    }

    /// A new shell in the outside region for a sheet's connected piece: both sides of each
    /// of its faces.
    pub(crate) fn sheet_shell(&mut self) -> Result<[ShellId; 2], MakeError> {
        let shell = self.draft.add_shell(ShellRecord::new(RegionId::OUTSIDE))?;
        Ok([shell, shell])
    }

    pub(crate) fn vertex(&mut self, p: Point3) -> Result<VertexId, MakeError> {
        Ok(self.draft.add_vertex(VertexRecord::new(p))?)
    }

    /// An edge along `curve` over `interval` from `ends[0]` to `ends[1]`, not shared.
    pub(crate) fn edge(
        &mut self,
        curve: Curve,
        interval: Extent1d,
        ends: [VertexId; 2],
    ) -> Result<EdgeId, MakeError> {
        Ok(self
            .draft
            .add_edge(EdgeGeom::new(EdgeShape::Curve { curve, interval }, ends))?)
    }

    /// The collapsed edge at `v`: a pole's or an apex's, used once by the face round it.
    pub(crate) fn collapsed(&mut self, v: VertexId) -> Result<EdgeId, MakeError> {
        Ok(self
            .draft
            .add_edge(EdgeGeom::new(EdgeShape::Collapsed, [v, v]))?)
    }

    /// The closed edge round the circle of `radius` about `frame`'s z axis, counter-clockwise
    /// about it over one turn from its one vertex, which is made at `frame`'s x axis.
    pub(crate) fn circle(
        &mut self,
        frame: Frame,
        radius: f64,
    ) -> Result<(EdgeId, VertexId), MakeError> {
        let v = self.vertex(frame.origin() + *frame.x() * radius)?;
        let circle = Circle::new(frame, Length::from_mm(radius), Sense::Forward)?;
        let turn = Extent1d::new(0.0, core::f64::consts::TAU)
            .ok_or(MakeError::Defect(Defect::Interval))?;
        Ok((self.edge(Curve::Circle(circle), turn, [v, v])?, v))
    }

    /// The straight edge between `a` and `b`, made the first time, with the sense of a use
    /// that runs from `a` to `b`. Its curve runs from the lower id to the higher over
    /// `[0, 1]`.
    pub(crate) fn line(&mut self, a: VertexId, b: VertexId) -> Result<(EdgeId, Sense), MakeError> {
        let (lo, hi, sense) = if a < b {
            (a, b, Sense::Forward)
        } else {
            (b, a, Sense::Reversed)
        };
        if let Some(&e) = self.lines.get(&(lo, hi)) {
            return Ok((e, sense));
        }
        let (p, q) = (self.point(lo)?, self.point(hi)?);
        let line = Line::new(p, q - p)?;
        let e = self.edge(Curve::Line(line), unit()?, [lo, hi])?;
        self.lines.insert((lo, hi), e);
        Ok((e, sense))
    }

    /// Adds the face on `surface` whose front is the outside and whose back is the solid,
    /// with `loops` (the first the outer one) of edges in their senses.
    pub(crate) fn face(
        &mut self,
        surface: Surface,
        loops: &[Vec<(EdgeId, Sense)>],
    ) -> Result<FaceId, MakeError> {
        let sides = self.sides()?;
        self.face_between(surface, loops, sides)
    }

    /// Adds the face on `surface` between the shells `sides` (front, back), with `loops` as
    /// [`Self::face`]'s.
    pub(crate) fn face_between(
        &mut self,
        surface: Surface,
        loops: &[Vec<(EdgeId, Sense)>],
        sides: [ShellId; 2],
    ) -> Result<FaceId, MakeError> {
        let records = self.draft.exact_loops(&surface, loops, self.doc)?;
        Ok(self
            .draft
            .add_face(FaceRecord::new(surface, sides, records))?)
    }

    /// Adds a sheet face on `surface`, with `loops` (the first the outer one) of edges in
    /// their senses, in a shell of its own in the outside region with both sides in it.
    pub(crate) fn sheet_face(
        &mut self,
        surface: Surface,
        loops: &[Vec<(EdgeId, Sense)>],
    ) -> Result<FaceId, MakeError> {
        let shell = self.draft.add_shell(ShellRecord::new(RegionId::OUTSIDE))?;
        let records = self.draft.exact_loops(&surface, loops, self.doc)?;
        Ok(self
            .draft
            .add_face(FaceRecord::new(surface, [shell, shell], records))?)
    }

    /// Checks the draft and finishes it. A refusal is the constructor's own defect.
    pub(crate) fn finish(self) -> Result<Brep, MakeError> {
        match self.draft.finish(self.doc) {
            Ok((body, _changes)) => Ok(body),
            Err(un) => Err(MakeError::Defect(Defect::Finish(un.cause))),
        }
    }

    fn point(&self, v: VertexId) -> Result<Point3, MakeError> {
        self.draft
            .vertex(v)
            .map(scx_brep::Vertex::point)
            .ok_or_else(|| MakeError::Defect(Defect::Absent(Absent { entity: v.into() })))
    }
}

/// `[0, 1]`, a straight edge's interval.
fn unit() -> Result<Extent1d, MakeError> {
    Extent1d::new(0.0, 1.0).ok_or(MakeError::Defect(Defect::Interval))
}
