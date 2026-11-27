// Written for solidcorex. Apache-2.0.
//! The operands as sides, coincidence, and the two feet every test uses.

use scx_brep::{Brep, Edge, EdgeShape, Entity, Face, Operand, UvSide};
use scx_num::budget::{Budget, Stop};
use scx_num::cert::Recorded;
use scx_num::units::{Accuracy, coincident};
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;

use crate::error::{BooleanDefect, BooleanError, BooleanUnsupported};

/// Which operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Side {
    /// Operand 0, `a`: the body the result is built in.
    A,
    /// Operand 1, `b`.
    B,
}

impl Side {
    /// Both, in order.
    pub(crate) const BOTH: [Self; 2] = [Self::A, Self::B];

    /// The operand number.
    pub(crate) const fn operand(self) -> Operand {
        match self {
            Self::A => Operand::from_raw(0),
            Self::B => Operand::from_raw(1),
        }
    }

    /// The other side.
    pub(crate) const fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// The two operands.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Operands<'a> {
    /// Operand 0.
    pub(crate) a: &'a Brep,
    /// Operand 1.
    pub(crate) b: &'a Brep,
}

impl<'a> Operands<'a> {
    /// The body on `side`.
    pub(crate) const fn of(self, side: Side) -> &'a Brep {
        match side {
            Side::A => self.a,
            Side::B => self.b,
        }
    }

    /// The coincidence distance of two entities of accuracies `p` and `q`, in mm: the
    /// largest of the operation's resolution and the two (one document, so one resolution).
    pub(crate) fn coincidence(self, p: Option<Accuracy>, q: Option<Accuracy>) -> f64 {
        coincident(
            self.a.resolution(),
            p.unwrap_or(Accuracy::ZERO),
            q.unwrap_or(Accuracy::ZERO),
        )
        .mm()
    }
}

/// The accuracy of a recorded bound.
pub(crate) fn recorded(r: Option<&Recorded>) -> Option<Accuracy> {
    r.map(|r| r.acc)
}

/// The nearest point of a face's surface to `p`, its uv placed in the face's uv box, and
/// where that uv lies against the face (`None` when the face's test cannot tell).
pub(crate) struct FaceFoot {
    /// The uv.
    pub(crate) uv: Point2,
    /// The point.
    pub(crate) point: Point3,
    /// The distance from `p`.
    pub(crate) distance: f64,
    /// Where the uv lies against the face.
    pub(crate) side: Option<UvSide>,
}

/// The foot of `p` on `face` of `side`: a closed form on every analytic kind, one unit.
pub(crate) fn face_foot(
    face: Face<'_>,
    side: Side,
    p: Point3,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<FaceFoot, BooleanError> {
    budget.charge(1)?;
    let operand = side.operand();
    let uv_box = face.uv_box().ok_or_else(|| {
        BooleanError::Defect(BooleanDefect::NoUvBox {
            operand,
            face: face.id(),
        })
    })?;
    let foot = face
        .surface()
        .invert(p, None, Some(&uv_box), budget, stop)
        .map_err(|e| BooleanError::of_surface(e, operand, face.id()))?
        .into_foot();
    Ok(FaceFoot {
        uv: foot.at,
        point: foot.point,
        distance: foot.distance,
        side: face.contains_uv(foot.at),
    })
}

/// The foot of `p` on `edge` of `side` inside its interval: its parameter, point and
/// distance; `None` for a collapsed edge or a foot outside the interval (the nearest point
/// is then an end, which the vertex tests cover). One unit.
pub(crate) fn edge_foot(
    edge: Edge<'_>,
    side: Side,
    p: Point3,
    budget: &mut Budget,
) -> Result<Option<(f64, Point3, f64)>, BooleanError> {
    let EdgeShape::Curve { curve, interval } = edge.shape() else {
        return Ok(None);
    };
    budget.charge(1)?;
    let inv = curve.invert(p, Some(interval)).map_err(|_| {
        BooleanError::Unsupported(BooleanUnsupported::Nurbs {
            operand: side.operand(),
            at: Entity::Edge(edge.id()),
        })
    })?;
    Ok(inv.is_inside().then(|| {
        let f = inv.foot();
        (f.at, f.point, f.distance)
    }))
}
