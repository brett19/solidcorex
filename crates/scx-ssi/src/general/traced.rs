// Written for solidcorex. Apache-2.0.
//! The general intersector's answer: branches, nodes and touches (research 73 §4.6, §4.7,
//! §4.8).

use std::sync::Arc;

use scx_curve::Curve2;
use scx_num::cert::{Cert, Claim, Method, Reads};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_nurbs::NurbsCurve;

use crate::analytic::{Contact, Overlap};

/// Where two surfaces meet within their extents, traced by the general method: curves,
/// the points where their ends meet, and isolated points, each in a fixed order (module
/// docs). All four empty is a proof that the surfaces do not meet within the extents
/// (pairs are dropped only by proof), final as the closed forms' `Empty` is.
///
/// Non-exhaustive: later rows may add fields.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub struct Traced {
    /// The curves.
    pub branches: Vec<Branch>,
    /// The points where branch ends meet: crossings, and splits at poles and seams.
    /// [`End::Node`] indexes this list.
    pub nodes: Vec<Node>,
    /// The isolated points.
    pub touches: Vec<Touch>,
    /// Where the surfaces are one surface within the coincidence: the same pieces as the
    /// closed forms' [`Intersection::Coincident`](crate::analytic::Intersection::Coincident)
    /// (convex polygons in both surfaces' uv, vertex for vertex), so a reader that takes
    /// one takes the other. A region lies here and not in `branches` when the surfaces
    /// agree over it: a curve is only where they cross or touch alone. The method that proves
    /// it (a field with every coefficient zero within its bound, row `SM6a`) puts it here;
    /// none does before that.
    pub coincident: Vec<Overlap>,
}

impl Traced {
    /// Whether the surfaces do not meet within the extents (proved).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.branches.is_empty()
            && self.nodes.is_empty()
            && self.touches.is_empty()
            && self.coincident.is_empty()
    }
}

/// One curve of a traced intersection: a 3D B-spline and a pcurve on each surface, all on
/// one parameter (73 §4.7b), with the certificate that bounds them (73 §4.8).
///
/// The parameter is the 3D curve's over its knots' domain. A pcurve that is a B-spline
/// shares the 3D curve's knot vector (the pin's invariant 4); one on an analytic surface may
/// instead be that surface's closed-form chart inverse of the 3D curve (73 §4.7b), exact at
/// the same parameter. The curve runs along `n_a x n_b`, as the closed forms' do.
#[derive(Clone, Debug, PartialEq)]
pub struct Branch {
    /// The curve in space.
    pub curve: Arc<NurbsCurve<3>>,
    /// Its uv on the first surface, then on the second.
    pub pcurves: [Curve2; 2],
    /// How the surfaces meet along it.
    pub contact: Contact,
    /// Where it starts and ends.
    pub ends: Ends,
    /// The proved bounds.
    pub certificate: Certificate,
}

/// Where a branch starts and ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ends {
    /// A closed loop: its end is its start, and it meets no node.
    Closed,
    /// An arc from `start` to `end` (along the curve's parameter).
    Open {
        /// Where the parameter's low end lies.
        start: End,
        /// Where the parameter's high end lies.
        end: End,
    },
}

/// One end of an open branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum End {
    /// It leaves the extents there, across the boundary of the one named.
    Rim(Rim),
    /// It meets node `.0` of [`Traced::nodes`].
    Node(usize),
}

/// Which extent's boundary a branch leaves across.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rim {
    /// The first surface's.
    A,
    /// The second surface's.
    B,
    /// Both at once (within the coincidence): a corner where the boundaries meet.
    Both,
}

/// A point where branch ends meet, with its uv on each surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    /// The point.
    pub point: Point3,
    /// Its uv on the first surface, then on the second.
    pub uv: [Point2; 2],
    /// Why the branches split there.
    pub kind: NodeKind,
}

/// Why branches meet at a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NodeKind {
    /// Two branches cross: a collinear-normal point within the coincidence whose relative
    /// form `II_A - II_B` is indefinite (Steinmetz, Viviani, Villarceau; 73 §4.6). Four arcs
    /// leave it along the form's null directions. A vertex of any edge through it.
    Crossing,
    /// A branch passes through a pole or apex of one surface, where that surface's uv is
    /// not continuous: split there, two arcs.
    Pole,
    /// A branch crosses a periodic seam of one surface's uv: split there, two arcs.
    Seam,
}

/// An isolated point: the surfaces touch there (a definite relative form within the
/// coincidence, at the deepest point, as the closed forms' depth rule places it), or meet
/// in a loop no larger than the coincidence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touch {
    /// The point.
    pub point: Point3,
    /// Its uv on the first surface, then on the second.
    pub uv: [Point2; 2],
}

/// The proved bounds of a branch (73 §4.8), over every knot span: the residual of its
/// pcurves on their surfaces, enclosed, and the distance from the 3D curve to the true
/// intersection, by parametric Krawczyk. Rounding is included in each.
///
/// The body layer stores them as `Basis::Certified` records with the inputs' keys
/// ([`Certificate::curve_cert`], [`OnSurface::cert`]); this crate knows no keys.
///
/// Non-exhaustive: row SM8 may keep per-span bounds as a field.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Certificate {
    /// A unique true intersection point lies within this of the 3D curve, on the plane
    /// across it, at every parameter.
    pub to_intersection: Length,
    /// Each pcurve's residual on its surface, against the 3D curve: the first surface's,
    /// then the second's.
    pub on_surface: [OnSurface; 2],
}

impl Certificate {
    /// The 3D curve's claim, as a record holds it: [`Claim::CurveToIntersection`] by
    /// [`Method::Krawczyk`], reading `reads` (the curve and both surfaces).
    #[must_use]
    pub const fn curve_cert(&self, reads: Reads) -> Cert {
        Cert {
            claim: Claim::CurveToIntersection,
            bound: self.to_intersection,
            method: Method::Krawczyk,
            reads,
        }
    }
}

/// A pcurve's bound on its surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OnSurface {
    /// The surface's closed-form chart inverse of the 3D curve: exact, no record (an exact
    /// kind carries none).
    Exact,
    /// `|S(p(t)) - c(t)|` is at most this, by an interval Bernstein or mean-value
    /// enclosure of the composition over each span.
    Within(Length),
}

impl OnSurface {
    /// The claim as a record holds it ([`Claim::PcurveOnSurface`] by [`Method::Interval`],
    /// reading `reads`), or `None` for an exact pcurve, which carries no record.
    #[must_use]
    pub const fn cert(self, reads: Reads) -> Option<Cert> {
        match self {
            Self::Exact => None,
            Self::Within(bound) => Some(Cert {
                claim: Claim::PcurveOnSurface,
                bound,
                method: Method::Interval,
                reads,
            }),
        }
    }
}
