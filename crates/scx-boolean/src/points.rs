// Written for solidcorex. Apache-2.0.
//! Stage 3: merged points (the 11-27 merged-points row; 72 §12.4 row 4).
//!
//! Contacts within coincidence of each other are one place. The components are fixed
//! before anything merges: two contacts join when they name the same vertex or their
//! midpoints lie within the larger of their two reaches; the components are the transitive
//! closure. Each component is one [`Point`]:
//!
//! 1. On each side, an edge member within coincidence of one of the edge's end vertices is
//!    that vertex; members are kept once each; an edge is dropped when one of its ends is
//!    a member, and a face when one of its edges or vertices is.
//! 2. Two vertices of one side left in one point: within coincidence of each other they are
//!    [`BooleanRefusal::VerticesWithinCoincidence`]; only joined through the chain they are
//!    [`BooleanUnsupported::Chain`] (the row's nearest-vertex anchoring is not built yet).
//! 3. The point is `a`'s vertex if it has one, else `b`'s, else the mean of the members'
//!    points; its gap is the largest distance from a member's own point to it.
//!
//! The vertex clearance of one operand is checked only where the other operand meets it:
//! two close vertices nothing meets cannot change the result.

use std::collections::BTreeMap;

use scx_brep::VertexId;
use scx_num::vector3d::{Point3, Vec3};

use crate::contact::{Contact, Member, On, Touched};
use crate::error::{BooleanError, BooleanRefusal, BooleanUnsupported};
use crate::geom::{Operands, Side, recorded};

/// One merged point: where the operands meet, as one place.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Point {
    /// The point.
    pub(crate) at: Point3,
    /// The operand vertex that is the point, if any: `a`'s before `b`'s.
    pub(crate) anchor: Option<(Side, VertexId)>,
    /// The largest distance from a member's own point to `at`, in mm: the accuracy a vertex
    /// made here records (as `Measured`, when above the resolution).
    pub(crate) gap: f64,
    /// The entities of `a`, then of `b`, that pass through the point, each in key order:
    /// at most one vertex per side, then edges, then faces.
    pub(crate) on: [Vec<On>; 2],
    /// Whether an edge touches a surface here without crossing it.
    pub(crate) tangent: bool,
}

/// A union-find over contact indices; the root of a set is its lowest index.
struct Sets(Vec<usize>);

impl Sets {
    fn find(&mut self, mut i: usize) -> usize {
        while let Some(&p) = self.0.get(i) {
            if p == i {
                break;
            }
            let gp = self.0.get(p).copied().unwrap_or(p);
            if let Some(slot) = self.0.get_mut(i) {
                *slot = gp;
            }
            i = p;
        }
        i
    }

    fn join(&mut self, i: usize, j: usize) {
        let (ri, rj) = (self.find(i), self.find(j));
        let (lo, hi) = (ri.min(rj), ri.max(rj));
        if let Some(slot) = self.0.get_mut(hi) {
            *slot = lo;
        }
    }
}

/// The components of `contacts`, each a list of contact indices in order, the components
/// in order of their lowest index.
fn components(contacts: &[Contact]) -> Vec<Vec<usize>> {
    let mut sets = Sets((0..contacts.len()).collect());
    // Shared vertices.
    let mut first: BTreeMap<(Side, VertexId), usize> = BTreeMap::new();
    for (i, c) in contacts.iter().enumerate() {
        for (side, m) in Side::BOTH.into_iter().zip(c.members) {
            if let On::Vertex(v) = m.on {
                match first.get(&(side, v)) {
                    Some(&j) => sets.join(i, j),
                    None => {
                        first.insert((side, v), i);
                    }
                }
            }
        }
    }
    // Midpoints within reach: a sweep along x, the window the largest reach.
    let mids: Vec<Point3> = contacts.iter().map(Contact::mid).collect();
    let window = contacts.iter().map(|c| c.reach).fold(0.0, f64::max);
    let mut by_x: Vec<usize> = (0..contacts.len()).collect();
    by_x.sort_by(|&i, &j| {
        let (p, q) = (mids.get(i), mids.get(j));
        p.map(|p| p.x)
            .partial_cmp(&q.map(|q| q.x))
            .unwrap_or(core::cmp::Ordering::Equal)
            .then(i.cmp(&j))
    });
    for (k, &i) in by_x.iter().enumerate() {
        let (Some(pi), Some(ci)) = (mids.get(i), contacts.get(i)) else {
            continue;
        };
        for &j in by_x.iter().skip(k + 1) {
            let (Some(pj), Some(cj)) = (mids.get(j), contacts.get(j)) else {
                continue;
            };
            if pj.x - pi.x > window {
                break;
            }
            if pi.distance(*pj) <= ci.reach.max(cj.reach) {
                sets.join(i, j);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..contacts.len() {
        groups.entry(sets.find(i)).or_default().push(i);
    }
    groups.into_values().collect()
}

/// One side's members of a component, normalised (step 1 of the module's method), in key
/// order.
fn normalise(
    ops: Operands<'_>,
    touched: &Touched,
    side: Side,
    members: Vec<Member>,
) -> Vec<Member> {
    let body = ops.of(side);
    // An edge member within coincidence of an end is that end.
    let mut ms: Vec<Member> = members
        .into_iter()
        .map(|m| {
            let On::Edge { edge, .. } = m.on else {
                return m;
            };
            let Some(ed) = body.edge(edge) else {
                return m;
            };
            ed.ends()
                .into_iter()
                .filter_map(|v| body.vertex(v))
                .find(|v| {
                    m.point.distance(v.point())
                        <= ops.coincidence(recorded(v.accuracy()), recorded(ed.accuracy()))
                })
                .map_or(m, |v| Member {
                    on: On::Vertex(v.id()),
                    point: v.point(),
                })
        })
        .collect();
    // Once each, the first found kept: a stable sort keeps contact order within a key.
    ms.sort_by_key(|m| m.on.key());
    ms.dedup_by_key(|m| m.on.key());
    let vertices: Vec<VertexId> = ms
        .iter()
        .filter_map(|m| match m.on {
            On::Vertex(v) => Some(v),
            _ => None,
        })
        .collect();
    let edges: Vec<_> = ms
        .iter()
        .filter_map(|m| match m.on {
            On::Edge { edge, .. } => Some(edge),
            _ => None,
        })
        .collect();
    ms.retain(|m| match m.on {
        On::Vertex(_) => true,
        On::Edge { edge, .. } => body
            .edge(edge)
            .is_none_or(|e| !e.ends().iter().any(|v| vertices.contains(v))),
        On::Face { face, .. } => touched.parts(side, face).is_none_or(|p| {
            !p.vertices.iter().any(|v| vertices.contains(v))
                && !p.edges.iter().any(|e| edges.contains(e))
        }),
    });
    ms
}

/// The one vertex of `side` among `ms`, or the refusal or chain when there are several.
fn one_vertex(
    ops: Operands<'_>,
    side: Side,
    ms: &[Member],
) -> Result<Option<VertexId>, BooleanError> {
    let body = ops.of(side);
    let vs: Vec<VertexId> = ms
        .iter()
        .filter_map(|m| match m.on {
            On::Vertex(v) => Some(v),
            _ => None,
        })
        .collect();
    let (Some(&lo), Some(&next)) = (vs.first(), vs.get(1)) else {
        return Ok(vs.first().copied());
    };
    let operand = side.operand();
    for (i, &v) in vs.iter().enumerate() {
        for &w in vs.iter().skip(i + 1) {
            let (Some(p), Some(q)) = (body.vertex(v), body.vertex(w)) else {
                continue;
            };
            let c = ops.coincidence(recorded(p.accuracy()), recorded(q.accuracy()));
            if p.point().distance(q.point()) <= c {
                return Err(BooleanError::Refused(
                    BooleanRefusal::VerticesWithinCoincidence {
                        operand,
                        vertices: [v, w],
                    },
                ));
            }
        }
    }
    Err(BooleanError::Unsupported(BooleanUnsupported::Chain {
        operand,
        vertices: [lo, next],
    }))
}

/// The order of two points: by their members' keys, `a`'s then `b`'s, then position.
fn order(p: &Point, q: &Point) -> core::cmp::Ordering {
    let keys = |x: &Point| -> [Vec<(u8, u32)>; 2] {
        let [a, b] = &x.on;
        [
            a.iter().map(On::key).collect(),
            b.iter().map(On::key).collect(),
        ]
    };
    keys(p)
        .cmp(&keys(q))
        .then(p.at.x.total_cmp(&q.at.x))
        .then(p.at.y.total_cmp(&q.at.y))
        .then(p.at.z.total_cmp(&q.at.z))
}

/// Stage 3: the merged points of `contacts`, in order (see [`order`]).
///
/// # Errors
///
/// [`BooleanRefusal::VerticesWithinCoincidence`] and [`BooleanUnsupported::Chain`], for
/// the first point in order that has either.
pub(crate) fn merge(
    ops: Operands<'_>,
    touched: &Touched,
    contacts: &[Contact],
) -> Result<Vec<Point>, BooleanError> {
    let mut drafts = Vec::new();
    for group in components(contacts) {
        let mut sides: [Vec<Member>; 2] = [Vec::new(), Vec::new()];
        let mut tangent = false;
        for c in group.iter().filter_map(|&i| contacts.get(i)) {
            let [ma, mb] = c.members;
            let [sa, sb] = &mut sides;
            sa.push(ma);
            sb.push(mb);
            tangent |= c.tangent;
        }
        let [sa, sb] = sides;
        let on = [
            normalise(ops, touched, Side::A, sa),
            normalise(ops, touched, Side::B, sb),
        ];
        drafts.push((on, tangent));
    }
    // The order of the points does not depend on the order contacts were found in.
    let mut points: Vec<(Point, [Vec<Member>; 2])> = drafts
        .into_iter()
        .map(|(on, tangent)| {
            let all: Vec<Point3> = on.iter().flatten().map(|m| m.point).collect();
            let mean = mean(&all);
            let point = Point {
                at: mean,
                anchor: None,
                gap: 0.0,
                on: [
                    on[0].iter().map(|m| m.on).collect(),
                    on[1].iter().map(|m| m.on).collect(),
                ],
                tangent,
            };
            (point, on)
        })
        .collect();
    points.sort_by(|(p, _), (q, _)| order(p, q));
    let mut out = Vec::with_capacity(points.len());
    for (mut point, [ma, mb]) in points {
        let va = one_vertex(ops, Side::A, &ma)?;
        let vb = one_vertex(ops, Side::B, &mb)?;
        let anchor = va
            .map(|v| (Side::A, v))
            .or_else(|| vb.map(|v| (Side::B, v)));
        if let Some((side, v)) = anchor
            && let Some(vx) = ops.of(side).vertex(v)
        {
            point.at = vx.point();
        }
        point.anchor = anchor;
        point.gap = ma
            .iter()
            .chain(&mb)
            .map(|m| m.point.distance(point.at))
            .fold(0.0, f64::max);
        out.push(point);
    }
    Ok(out)
}

/// The mean of `ps`, summed in the given order; the origin for none.
fn mean(ps: &[Point3]) -> Point3 {
    let Some(&first) = ps.first() else {
        return Point3::new(0.0, 0.0, 0.0);
    };
    let d = ps
        .iter()
        .skip(1)
        .fold(Vec3::new(0.0, 0.0, 0.0), |d, &p| d + (p - first));
    #[expect(clippy::cast_precision_loss, reason = "a point's members are few")]
    let n = ps.len() as f64;
    first + d * (1.0 / n)
}
