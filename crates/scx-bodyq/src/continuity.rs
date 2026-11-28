// Written for solidcorex. Apache-2.0.
//! [`continuity`]: the order of contact the two faces across an edge are proved to have,
//! kept apart from the drops measured across it (research 74 §25).

use scx_brep::{Brep, EdgeId};
use scx_num::budget::{Budget, Stop};
use scx_num::interval::Ivl;
use scx_num::units::Length;

use crate::along::{Claim, Edge, Faces, Proof, angle_claim, tolerance};
use crate::error::BodyqError;

/// An order of contact across an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Order {
    /// The faces meet: no gap.
    G0,
    /// Their normals agree.
    G1,
    /// Their normal sections' curvatures across the edge agree.
    G2,
    /// So do those curvatures' rates along the section.
    G3,
    /// And the rates' rates.
    G4,
}

impl Order {
    /// The orders, lowest first.
    pub const ALL: [Self; 5] = [Self::G0, Self::G1, Self::G2, Self::G3, Self::G4];
}

/// How far apart the faces may be at each order and still count as continuous there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContinuityTolerances {
    /// G0: the distance between the faces' feet of a point of the edge.
    pub gap: Length,
    /// G1: the angle between the normals, radians, below pi.
    pub angle: f64,
    /// G2: the jump in normal-section curvature, per mm.
    pub curvature: f64,
    /// G3: the jump in its arc-length rate, per mm^2.
    pub curvature_rate: f64,
    /// G4: the jump in that rate's rate, per mm^3.
    pub curvature_accel: f64,
}

/// A drop measured across the edge at one parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    /// The edge curve's parameter.
    pub at: f64,
    /// The drop's certified bracket there (whole where it has none, e.g. no normal).
    pub bracket: Ivl,
}

/// Why the proof stopped below G4.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Limit {
    /// The order fails, certified: at `at` the drop is above its tolerance.
    Refuted {
        /// The order that fails.
        order: Order,
        /// The witness parameter.
        at: f64,
        /// The drop's bracket there.
        drop: Ivl,
    },
    /// The order could be neither proved nor refuted: at `at` the bracket straddles its
    /// tolerance.
    Undecided {
        /// The order left open.
        order: Order,
        /// The parameter where it could not be told.
        at: f64,
        /// The drop's bracket there.
        drop: Ivl,
    },
}

/// The proved order and the measured drops of an edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Continuity {
    /// The highest order proved all along the edge, each lower order proved first; `None`
    /// when not even G0 is.
    pub proved: Option<Order>,
    /// What stopped the proof at the next order; `None` exactly when G4 is proved.
    pub limit: Option<Limit>,
    /// At each order (G0 to G4), the largest drop among stations spread evenly over the
    /// edge, ends included: a measure, not a proof (a drop between stations may be larger).
    pub measured: [Measured; 5],
}

/// The stations the drops are measured at.
const STATIONS: usize = 9;

/// The order of contact proved across `edge`, and the drops measured there.
///
/// The orders are proved in turn, G0 to G4, each all along the edge (as
/// [`tangent_all_along`](crate::tangent_all_along) proves G1), and the first that is not
/// proved is the `limit`: refuted with a witness, or undecided. One `budget` covers every
/// order and every station. The drops are measured at [`STATIONS`] parameters whatever was
/// proved, so a G1 edge still reports its curvature jump.
///
/// The drops: G0 the distance between the two faces' feet of the curve's point; G1 the
/// angle between their oriented normals; G2 to G4 the jumps in the curvature of the normal
/// section across the edge (the plane of the normal and the direction square to the curve),
/// in its arc-length rate, and in that rate's rate, each read from the face's implicit
/// equation at its foot. A seam (one analytic surface on both sides, oriented alike) is G4
/// with zero drops.
///
/// # Errors
///
/// As [`tangent_all_along`](crate::tangent_all_along), with any tolerance not finite or
/// negative refused.
pub fn continuity(
    body: &Brep,
    edge: EdgeId,
    tol: &ContinuityTolerances,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Continuity, BodyqError> {
    let claims = claims_of(tol)?;
    let e = Edge::of(body, edge)?;
    along(&e, &claims, budget, stop)
}

/// The five claims, G0 to G4, at `tol`.
pub(crate) fn claims_of(tol: &ContinuityTolerances) -> Result<[Claim; 5], BodyqError> {
    let gap = tolerance(tol.gap.mm())?;
    Ok([
        Claim::Gap(Ivl::from(gap) * Ivl::from(gap)),
        angle_claim(tol.angle)?,
        Claim::Bend(0, tolerance(tol.curvature)?),
        Claim::Bend(1, tolerance(tol.curvature_rate)?),
        Claim::Bend(2, tolerance(tol.curvature_accel)?),
    ])
}

/// [`continuity`] of an edge already read (a sheet seam's too): the proof and the drops.
pub(crate) fn along(
    e: &Edge<'_>,
    claims: &[Claim; 5],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Continuity, BodyqError> {
    let Faces::Two(f) = &e.faces else {
        let none = Measured {
            at: e.lo,
            bracket: Ivl::from(0.0),
        };
        return Ok(Continuity {
            proved: Some(Order::G4),
            limit: None,
            measured: [none; 5],
        });
    };
    let mut proved = None;
    let mut limit = None;
    for (order, claim) in Order::ALL.into_iter().zip(claims.iter().copied()) {
        match claim.along(e, f, budget, stop)? {
            Proof::Holds(_) => proved = Some(order),
            Proof::Fails(at, drop) => {
                limit = Some(Limit::Refuted { order, at, drop });
                break;
            }
            Proof::Open(at, drop) => {
                limit = Some(Limit::Undecided { order, at, drop });
                break;
            }
        }
    }
    let mut measured = [Measured {
        at: e.lo,
        bracket: Ivl::from(0.0),
    }; 5];
    for (slot, claim) in measured.iter_mut().zip(claims.iter().copied()) {
        let mut worst: Option<Measured> = None;
        for i in 0..STATIONS {
            stop.check()?;
            budget.charge(1)?;
            #[allow(clippy::cast_precision_loss)]
            let w = i as f64 / (STATIONS - 1) as f64;
            let at = if i + 1 == STATIONS {
                e.hi
            } else {
                e.lo + (e.hi - e.lo) * w
            };
            let bracket = claim.drop_at(e, f, at);
            if worst.is_none_or(|d| bracket.hi > d.bracket.hi) {
                worst = Some(Measured { at, bracket });
            }
        }
        if let Some(d) = worst {
            *slot = d;
        }
    }
    Ok(Continuity {
        proved,
        limit,
        measured,
    })
}
