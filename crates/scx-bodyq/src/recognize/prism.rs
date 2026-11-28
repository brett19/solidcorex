// Written for solidcorex. Apache-2.0.
//! [`prism_section`]: whether a body is a uniform prism along a frame's z axis, and its
//! section, levels and volume (research 74 §2.3, §33, BW6; the pin's `prism::measure`).
//!
//! **What is proved.** A body is a prism along `d` when every one of these holds, each a
//! closed form on the body's own faces and edges:
//!
//! 1. every face has the one solid region on one side and the outside on the other, and
//!    the solid sides are one shell (so there is no cavity, no wire and no second lump);
//! 2. every face is a *cap* (a plane square to `d`) or a *wall* (a plane parallel to `d` or a
//!    cylinder with its axis along `d`), with the stray of the face over its own tight box
//!    (the angle times the face's diagonal, `tight_box` of BW2) within the body's
//!    coincidence. A cone or NURBS face is first put through `scx_surface::recognize`
//!    over its uv box; one that is a plane or cylinder within the coincidence is one, its
//!    deviation added to the tolerance;
//! 3. there is one cap facing `+d` and one facing `-d`, the upper level above the lower by
//!    more than the coincidence;
//! 4. every vertex is on one of the two levels, every cap edge is a line or a circle about an
//!    axis along `d` with both ends at the cap's level, and every other edge is a line
//!    from one level to the other, straight along `d`;
//! 5. the two caps' loops are the same curves within the coincidence, from any start and cut
//!    into any pieces ([`piece::same_loop`]).
//!
//! Then the body is the region between the levels over the lower cap's section: the walls
//! stand on the cap loops (items 2 and 4), so the boundary projects to the section.
//!
//! **The tolerance.** One length: the largest coincidence of any face, edge or vertex
//! (`scx_num::units::coincident` of the body's resolution and accuracy), plus the rounding
//! of the frame's coordinates (and the deviation of any recognised face). No other
//! constant. A frame so far from the body that the rounding alone would exceed a quarter of
//! the coincidence is refused ([`BodyqRefusal::FrameTooFar`]), not answered at a lost
//! precision.
//!
//! **The areas.** Each loop's area is the sum of its pieces' Green terms, anchored at the
//! loop's first point, widened by the rounding of the terms (a multiple of the scale of
//! the products) and by the corridor the coincidence allows: the true boundary lies within
//! `c` of the stored one, so the region differs from the stored one by at most the tube of
//! radius `c` round the loops, `2 c P + pi c^2` for perimeter `P`. The volume is that
//! area times the height widened by `2 c`.
//!
//! **Not a prism is an answer; not decided is an error.** [`Recognition::NotPrism`] names
//! the entity that is not one and why, and holds whatever the faces not yet read would
//! say. A face that cannot be decided (a NURBS face that is no quadric within the
//! coincidence) is [`BodyqUnsupported::Nurbs`]; budget and stop are their errors; nothing
//! is read as "not a prism" that was in fact a failure.

use core::f64::consts::{PI, TAU};

use scx_brep::op::{Cx, Scratch};
use scx_brep::{Brep, EdgeId, EdgeShape, Entity, Face, FaceId, FaceSide, Loop};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::domain::{ParamDomain, UvDomain};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::interval::Ivl;
use scx_num::units::{Accuracy, Length, coincident};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_surface::Surface;

use super::piece::{Piece, loop_area, reverse_loop, same_loop};
use crate::error::{BodyqDefect, BodyqError, BodyqRefusal, BodyqUnsupported};
use crate::geom::recorded;
use crate::tight::{tight_body_box, tight_box_charged};

/// A uniform prism along a frame's z axis: its levels above the frame and its section in the
/// frame's `x, y` coordinates.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PrismSection {
    /// The level of the lower cap above the frame's origin, along its z axis.
    pub lo: f64,
    /// The level of the upper cap (`lo < hi`).
    pub hi: f64,
    /// The outer loop of the section, counter-clockwise, as the lower cap stores it.
    pub outer: Vec<Piece>,
    /// The holes, clockwise.
    pub holes: Vec<Vec<Piece>>,
    /// How far the levels and section may be off the body's caps: the coincidence, the
    /// rounding of the frame's coordinates and any recognised face's deviation.
    pub tolerance: Length,
    /// An enclosure of the section's area.
    pub area: Ivl,
    /// An enclosure of the body's volume.
    pub volume: Ivl,
}

/// Why a body is not a prism along the frame's z axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Why {
    /// A face without the solid on one side and the outside on the other, or whose solid
    /// side is another shell than the first face's.
    Sides,
    /// A plane or cylinder that is neither square to the direction nor along it, over its
    /// own size.
    Slanted,
    /// A sphere, a torus, or a cone that is no plane or cylinder: not swept along a line.
    Kind,
    /// More or fewer caps than one facing each way, or two levels not apart.
    Caps,
    /// A vertex on neither level.
    Vertex,
    /// A cap edge that is no line or no circle about an axis along the direction, or off
    /// its cap's level; an edge that is no line from one level to the other, straight
    /// along the direction; a collapsed edge.
    Edge,
    /// An edge no face uses.
    Wire,
    /// The two caps' loops are not the same curves.
    CapsDiffer,
}

/// The entity that is not part of a prism, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotPrism {
    /// The entity.
    pub at: Entity,
    /// Why.
    pub why: Why,
}

/// What [`prism_section`] found.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Recognition {
    /// A prism, proved.
    Prism(PrismSection),
    /// Not a prism along the direction, proved by one entity.
    NotPrism(NotPrism),
}

/// The uniform prism `body` is along `frame`'s z axis, when its faces and edges prove it one
/// (see the module's documentation for what is proved). The section is in `frame`'s `x, y`
/// coordinates and the levels along its z, measured from its origin.
///
/// The tight boxes of the faces (BW2) are kept in `cx.scratch`; the work is charged to the
/// call's budget, one unit per entity read.
///
/// # Errors
///
/// [`BodyqRefusal::EmptyBody`] for a body with no face; [`BodyqRefusal::FrameTooFar`] for a
/// frame whose coordinates cannot hold the body's coincidence; [`BodyqUnsupported::Nurbs`]
/// for a face that is a NURBS surface no quadric within the coincidence (or a surface kind
/// this query does not know); [`BodyqError::Exhausted`] and [`BodyqError::Cancelled`]; and
/// the defects of [`tight_box`](crate::tight_box) and of the recognition of a face.
pub fn prism_section(
    body: &Brep,
    frame: &Frame,
    cx: &mut Cx<'_>,
) -> Result<Recognition, BodyqError> {
    let mut budget = cx.budget();
    let size = tight_body_box(body, cx)?;
    prism_section_charged(
        body,
        frame,
        size.max_dimension(),
        cx.scratch,
        &mut budget,
        cx.stop,
    )
}

/// [`prism_section`] charging a budget the caller keeps, for a caller that reads many bodies
/// or directions under one budget. `reach` is the largest absolute coordinate of the body
/// (`tight_body_box(..).max_dimension()`): it sets the precision the frame's coordinates must
/// hold.
///
/// # Errors
///
/// As [`prism_section`].
pub fn prism_section_charged(
    body: &Brep,
    frame: &Frame,
    reach: f64,
    scratch: &mut Scratch,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Recognition, BodyqError> {
    let mut run = Run {
        body,
        frame,
        reach,
        scratch,
        budget,
        stop,
    };
    match run.go() {
        Ok(s) => Ok(Recognition::Prism(s)),
        Err(Halt::No(n)) => Ok(Recognition::NotPrism(n)),
        Err(Halt::Err(e)) => Err(e),
    }
}

/// What ends a run before it has a section.
enum Halt {
    No(NotPrism),
    Err(BodyqError),
}

impl From<BodyqError> for Halt {
    fn from(e: BodyqError) -> Self {
        Self::Err(e)
    }
}

impl From<scx_num::budget::Exhausted> for Halt {
    fn from(e: scx_num::budget::Exhausted) -> Self {
        Self::Err(e.into())
    }
}

impl From<scx_num::budget::Stopped> for Halt {
    fn from(e: scx_num::budget::Stopped) -> Self {
        Self::Err(e.into())
    }
}

fn no<T>(at: Entity, why: Why) -> Result<T, Halt> {
    Err(Halt::No(NotPrism { at, why }))
}

/// A cap found: its face, its level and its outward direction along `d` (`+1` or `-1`).
struct Cap {
    face: FaceId,
    level: f64,
    up: bool,
}

struct Run<'a> {
    body: &'a Brep,
    frame: &'a Frame,
    reach: f64,
    scratch: &'a mut Scratch,
    budget: &'a mut Budget,
    stop: &'a Stop,
}

type Res<T> = Result<T, Halt>;

impl Run<'_> {
    fn d(&self) -> UnitVec3 {
        self.frame.z()
    }

    fn local(&self, p: Point3) -> Point2 {
        let v = p - self.frame.origin();
        Point2::new(v.dot(*self.frame.x()), v.dot(*self.frame.y()))
    }

    fn level(&self, p: Point3) -> f64 {
        (p - self.frame.origin()).dot(*self.frame.z())
    }

    fn charge(&mut self, n: u64) -> Res<()> {
        self.budget.charge(n)?;
        self.stop.check()?;
        Ok(())
    }

    fn go(&mut self) -> Res<PrismSection> {
        let body = self.body;
        let Some(first) = body.faces().next().map(Face::id) else {
            return Err(Halt::Err(BodyqError::Refused(BodyqRefusal::EmptyBody)));
        };
        let c0 = body_coincidence(body)?;
        // The rounding of a local coordinate: the difference of two points no further from
        // the origin than `reach` and the frame's, three products and two sums.
        let o = self.frame.origin();
        let far = o.x.abs().max(o.y.abs()).max(o.z.abs()) + self.reach;
        let proj = 16.0 * f64::EPSILON * far;
        if !(proj.is_finite() && proj <= 0.25 * c0) {
            return Err(Halt::Err(BodyqError::Refused(BodyqRefusal::FrameTooFar)));
        }
        let mut c = c0 + proj;

        // 1 and 2: sides, then each face's kind.
        let mut solid = None;
        let mut caps: Vec<Cap> = Vec::new();
        let mut deviation = 0.0_f64;
        for face in body.faces() {
            self.charge(1)?;
            let id = face.id();
            let Some(side) = solid_side(face) else {
                return no(Entity::Face(id), Why::Sides);
            };
            let shell = face.shell(side).id();
            match solid {
                None => solid = Some(shell),
                Some(s) if s != shell => return no(Entity::Face(id), Why::Sides),
                Some(_) => {}
            }
            let (surface, dev) = self.quadric(face, c)?;
            deviation = deviation.max(dev);
            let cf = c + dev;
            let size = self.face_size(id)?;
            let d = self.d();
            match surface {
                Straight::Plane(n, origin) => {
                    let n = if side == FaceSide::Back { n } else { -n };
                    let across = n.cross(*d).length();
                    let along = n.dot(*d);
                    if across * size <= cf {
                        let level = self.level(origin);
                        caps.push(Cap {
                            face: id,
                            level,
                            up: along > 0.0,
                        });
                    } else if along.abs() * size > cf {
                        return no(Entity::Face(id), Why::Slanted);
                    }
                }
                Straight::Axis(a) => {
                    if a.cross(*d).length() * size > cf {
                        return no(Entity::Face(id), Why::Slanted);
                    }
                }
                Straight::Curved => return no(Entity::Face(id), Why::Kind),
            }
        }
        c += deviation;

        // 3: one cap each way.
        let mut lo_cap = None;
        let mut hi_cap = None;
        for cap in &caps {
            let slot = if cap.up { &mut hi_cap } else { &mut lo_cap };
            if slot.replace(cap).is_some() {
                return no(Entity::Face(cap.face), Why::Caps);
            }
        }
        let (Some(lo_cap), Some(hi_cap)) = (lo_cap, hi_cap) else {
            let at = caps.first().map_or(first, |k| k.face);
            return no(Entity::Face(at), Why::Caps);
        };
        let (lo, hi) = (lo_cap.level, hi_cap.level);
        if hi - lo <= c || !(hi - lo).is_finite() {
            return no(Entity::Face(hi_cap.face), Why::Caps);
        }

        // 4: vertices, then edges.
        for v in body.vertices() {
            self.charge(1)?;
            let l = self.level(v.point());
            if (l - lo).abs() > c && (l - hi).abs() > c {
                return no(Entity::Vertex(v.id()), Why::Vertex);
            }
        }
        let lo_loops = self.cap_loops(lo_cap, c)?;
        let hi_loops = self.cap_loops(hi_cap, c)?;
        let mut on_cap: Vec<EdgeId> = lo_loops
            .iter()
            .chain(&hi_loops)
            .flat_map(|l| l.edges.iter().copied())
            .collect();
        on_cap.sort();
        for edge in body.edges() {
            self.charge(1)?;
            let id = edge.id();
            if edge.is_wire() {
                return no(Entity::Edge(id), Why::Wire);
            }
            if on_cap.binary_search(&id).is_ok() {
                continue;
            }
            let EdgeShape::Curve { curve, .. } = edge.shape() else {
                return no(Entity::Edge(id), Why::Edge);
            };
            let [a, b] = edge.ends();
            let (Some(a), Some(b)) = (body.vertex(a), body.vertex(b)) else {
                return Err(Halt::Err(BodyqError::Defect(BodyqDefect::NoEdgeBox {
                    edge: id,
                })));
            };
            let (pa, pb) = (a.point(), b.point());
            let straight = matches!(curve, Curve::Line(_))
                && self.local(pa).distance(self.local(pb)) <= c
                && ((self.level(pa) - lo).abs() <= c && (self.level(pb) - hi).abs() <= c
                    || (self.level(pa) - hi).abs() <= c && (self.level(pb) - lo).abs() <= c);
            if !straight {
                return no(Entity::Edge(id), Why::Edge);
            }
        }

        // 5: the loops of the two caps are the same curves.
        if lo_loops.len() != hi_loops.len() {
            return no(Entity::Face(hi_cap.face), Why::CapsDiffer);
        }
        let mut taken = vec![false; hi_loops.len()];
        for l in &lo_loops {
            let found = hi_loops.iter().enumerate().position(|(i, h)| {
                !taken.get(i).copied().unwrap_or(true) && same_loop(&l.pieces, &h.pieces, c)
            });
            let Some(i) = found else {
                return no(Entity::Face(lo_cap.face), Why::CapsDiffer);
            };
            if let Some(t) = taken.get_mut(i) {
                *t = true;
            }
        }

        self.section(lo, hi, lo_loops, c, lo_cap.face)
    }

    /// The area and volume, with the loops oriented.
    fn section(
        &mut self,
        lo: f64,
        hi: f64,
        loops: Vec<CapLoop>,
        c: f64,
        cap: FaceId,
    ) -> Res<PrismSection> {
        let mut outer = None;
        let mut holes = Vec::new();
        let (mut area, mut scale, mut perimeter) = (0.0, 0.0, 0.0);
        for l in loops {
            self.charge(1)?;
            let (a, s) = loop_area(&l.pieces);
            let positive = l.outer;
            let pieces = l.pieces;
            perimeter += pieces.iter().map(Piece::length).sum::<f64>();
            area += a.abs() * if positive { 1.0 } else { -1.0 };
            scale += s;
            if positive {
                if outer.replace(pieces).is_some() {
                    return no(Entity::Face(cap), Why::CapsDiffer);
                }
            } else {
                holes.push(pieces);
            }
        }
        let Some(outer) = outer else {
            return no(Entity::Face(cap), Why::Caps);
        };
        // Rounding of the Green terms (a few operations each, trigonometry included), then
        // the corridor of the coincidence.
        let err = 256.0 * f64::EPSILON * (scale + 1.0) + 2.0 * c * perimeter + PI * c * c;
        let area = Ivl::outward_ulp(area - err, area + err);
        let height = Ivl::outward_ulp(hi - lo - 2.0 * c, hi - lo + 2.0 * c);
        let volume = area.mul_ulp(height);
        Ok(PrismSection {
            lo,
            hi,
            outer,
            holes,
            tolerance: Length::from_mm(c),
            area,
            volume,
        })
    }

    /// The diagonal of a face's tight box.
    fn face_size(&mut self, face: FaceId) -> Res<f64> {
        let b = tight_box_charged(
            self.body,
            Entity::Face(face),
            self.scratch,
            self.budget,
            self.stop,
        )?;
        let (lo, hi) = (b.min(), b.max());
        Ok(Vec3::new(hi.x - lo.x, hi.y - lo.y, hi.z - lo.z).length())
    }

    /// What a face's surface is for a prism: a plane (its natural normal), a surface ruled
    /// along an axis, or neither; with the deviation of a recognised quadric.
    fn quadric(&mut self, face: Face<'_>, c: f64) -> Res<(Straight, f64)> {
        let id = face.id();
        let surface = face.surface();
        match surface {
            Surface::Plane(p) => Ok((
                Straight::Plane(*p.normal(Point2::new(0.0, 0.0)), p.frame().origin()),
                0.0,
            )),
            Surface::Cylinder(cy) => Ok((Straight::Axis(*cy.frame().z()), 0.0)),
            Surface::Sphere(_) | Surface::Torus(_) => Ok((Straight::Curved, 0.0)),
            Surface::Cone(_) | Surface::Nurbs(_) => {
                // The box is rounded outward, so it can poke past a closed end of the surface's
                // domain (a cone's apex); the face lies inside the domain.
                let over = face
                    .uv_box()
                    .and_then(|b| within(b, &surface.domain()))
                    .ok_or(Halt::Err(BodyqError::Defect(BodyqDefect::NoUvBox {
                        face: id,
                    })))?;
                let acc = Accuracy::new(Length::from_mm(c))
                    .ok_or(Halt::Err(BodyqError::Defect(BodyqDefect::Tolerance)))?;
                let found = scx_surface::recognize(surface, &over, acc, self.budget, self.stop)
                    .map_err(|e| BodyqError::of_surface(e, id))?;
                match found {
                    Some(r) => {
                        let dev = r.deviation.mm();
                        match r.surface {
                            // The recognised plane faces as the given surface's normal does
                            // at the middle of the extent (SU7).
                            Surface::Plane(p) => Ok((
                                Straight::Plane(
                                    *p.normal(Point2::new(0.0, 0.0)),
                                    p.frame().origin(),
                                ),
                                dev,
                            )),
                            Surface::Cylinder(cy) => Ok((Straight::Axis(*cy.frame().z()), dev)),
                            Surface::Cone(_) | Surface::Sphere(_) | Surface::Torus(_) => {
                                Ok((Straight::Curved, dev))
                            }
                            _ => Err(Halt::Err(BodyqError::Unsupported(
                                BodyqUnsupported::Nurbs {
                                    at: Entity::Face(id),
                                },
                            ))),
                        }
                    }
                    None => match surface {
                        Surface::Cone(_) => Ok((Straight::Curved, 0.0)),
                        _ => Err(Halt::Err(BodyqError::Unsupported(
                            BodyqUnsupported::Nurbs {
                                at: Entity::Face(id),
                            },
                        ))),
                    },
                }
            }
            _ => Err(Halt::Err(BodyqError::Unsupported(
                BodyqUnsupported::Nurbs {
                    at: Entity::Face(id),
                },
            ))),
        }
    }

    /// A cap's loops as planar pieces: each loop's pieces in the frame, which loop is the
    /// outer, and the edges it stands on.
    fn cap_loops(&mut self, cap: &Cap, c: f64) -> Res<Vec<CapLoop>> {
        let Some(face) = self.body.face(cap.face) else {
            return Err(Halt::Err(BodyqError::Defect(BodyqDefect::NoFaceBox {
                face: cap.face,
            })));
        };
        let mut out = Vec::new();
        for lp in face.loops() {
            self.charge(1)?;
            out.push(self.cap_loop(lp, cap, c)?);
        }
        Ok(out)
    }

    fn cap_loop(&mut self, lp: Loop<'_>, cap: &Cap, c: f64) -> Res<CapLoop> {
        let mut pieces = Vec::with_capacity(lp.len());
        let mut edges = Vec::with_capacity(lp.len());
        for u in lp.uses() {
            let edge = u.edge();
            let id = edge.id();
            let EdgeShape::Curve { curve, interval } = edge.shape() else {
                return no(Entity::Edge(id), Why::Edge);
            };
            let (p0, p1) = (u.start().point(), u.end().point());
            if (self.level(p0) - cap.level).abs() > c || (self.level(p1) - cap.level).abs() > c {
                return no(Entity::Edge(id), Why::Edge);
            }
            let reversed = u.sense() == scx_num::frame::Sense::Reversed;
            let piece = match curve {
                Curve::Line(_) => Piece::Line {
                    a: self.local(p0),
                    b: self.local(p1),
                },
                Curve::Circle(circle) => {
                    let r = circle.radius().mm();
                    if circle.frame().z().cross(*self.d()).length() * r > c {
                        return no(Entity::Edge(id), Why::Edge);
                    }
                    let center = self.local(circle.frame().origin());
                    let (t0, t1) = (interval.min(), interval.max());
                    let (a, m) = (circle.point(t0), circle.point(0.5 * (t0 + t1)));
                    let (a, m) = (self.local(a), self.local(m));
                    let turn =
                        (a.x - center.x) * (m.y - center.y) - (a.y - center.y) * (m.x - center.x);
                    let mag = t1 - t0;
                    // The angle parameter of a circle is in radians: a turn is `mag`.
                    let mag = if (TAU - mag).abs() * r <= c { TAU } else { mag };
                    let sweep = if turn >= 0.0 { mag } else { -mag };
                    let start = fmath::atan2(a.y - center.y, a.x - center.x);
                    let arc = Piece::Arc {
                        center,
                        radius: r,
                        start,
                        sweep,
                    };
                    // The loop runs the edge forward or backward: its edge-direction piece
                    // above is from `t0`; a reversed use runs from `t1`.
                    if reversed { arc.reversed() } else { arc }
                }
                _ => return no(Entity::Edge(id), Why::Edge),
            };
            pieces.push(piece);
            edges.push(id);
        }
        // The two caps run their loops opposite ways (their normals are opposite): both are
        // read counter-clockwise for the outer loop and clockwise for a hole, so the two
        // can be compared.
        let outer = lp.index() == 0;
        let (area, _) = loop_area(&pieces);
        let pieces = if (area > 0.0) == outer {
            pieces
        } else {
            reverse_loop(&pieces)
        };
        Ok(CapLoop {
            pieces,
            edges,
            outer,
        })
    }
}

/// A cap's loop.
struct CapLoop {
    pieces: Vec<Piece>,
    edges: Vec<EdgeId>,
    outer: bool,
}

/// `b` cut to the ends `domain` has (none for a periodic or unbounded direction), `None` when
/// that leaves nothing.
fn within(b: Extent2d, domain: &UvDomain) -> Option<Extent2d> {
    let cut = |e: Extent1d, d: ParamDomain| -> Option<Extent1d> {
        match d {
            ParamDomain::Bounded(g) => e.intersect(g),
            ParamDomain::AtLeast { from, .. } => Extent1d::new(e.min().max(from), e.max()),
            ParamDomain::AtMost { to, .. } => Extent1d::new(e.min(), e.max().min(to)),
            // A periodic direction is at most one period: the rounded box of a face that
            // winds once round can be wider by an ulp.
            ParamDomain::Periodic { period, .. } if e.length() > period => {
                Extent1d::new(e.min(), e.min() + period)
            }
            _ => Some(e),
        }
    };
    Some(Extent2d::from_intervals(
        cut(b.x_interval(), domain.u)?,
        cut(b.y_interval(), domain.v)?,
    ))
}

/// What a face's surface is for a prism.
enum Straight {
    /// A plane, by its natural normal and a point on it.
    Plane(Vec3, Point3),
    /// A surface ruled along this axis (a cylinder's).
    Axis(Vec3),
    /// Neither.
    Curved,
}

/// The side of a face the material is on, when the other side is the outside.
fn solid_side(face: Face<'_>) -> Option<FaceSide> {
    let (f, b) = (
        face.shell(FaceSide::Front).region(),
        face.shell(FaceSide::Back).region(),
    );
    match (f.is_solid(), b.is_solid()) {
        (false, true) if f.is_outside() => Some(FaceSide::Back),
        (true, false) if b.is_outside() => Some(FaceSide::Front),
        _ => None,
    }
}

/// The largest coincidence of any face, edge or vertex of a body, in mm.
pub(crate) fn body_coincidence(body: &Brep) -> Result<f64, BodyqError> {
    let mut acc = Accuracy::ZERO;
    let mut take = |a: Option<Accuracy>| {
        if let Some(a) = a
            && a > acc
        {
            acc = a;
        }
    };
    for f in body.faces() {
        take(f.accuracy());
    }
    for e in body.edges() {
        take(recorded(e.accuracy()));
    }
    for v in body.vertices() {
        take(recorded(v.accuracy()));
    }
    let c = coincident(body.resolution(), acc, Accuracy::ZERO).mm();
    if c.is_finite() && c > 0.0 {
        Ok(c)
    } else {
        Err(BodyqError::Defect(BodyqDefect::Tolerance))
    }
}

/// Whether two bodies are the same uniform prism: the same levels and the same section,
/// found along one direction in the frame built there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Equivalence {
    /// A direction along which both are prisms with the same levels and the same section.
    Same,
    /// The first is a prism along some direction, and along every such direction the second
    /// is not, or has other levels or another section.
    Different,
    /// The first is no prism along any direction its planar faces name.
    NoPrism,
}

/// Whether `a` and `b` are the same uniform prism (research 74 §33; the pin's `equivalent`).
///
/// The directions tried are the outward normals of `a`'s planar faces, each direction once,
/// in face id order, in a frame at `a`'s first vertex with its x axis the world axis least
/// along the direction. The two are the same when, along one direction, both are prisms and
/// their levels and loops agree within the larger of the two tolerances. Where `a` is a
/// prism along none, the answer is [`Equivalence::NoPrism`].
///
/// # Errors
///
/// The errors of [`prism_section_charged`] for either body, the first met: an undecided
/// face (a NURBS face) in either body is an error, not a difference.
pub fn prism_equivalent(a: &Brep, b: &Brep, cx: &mut Cx<'_>) -> Result<Equivalence, BodyqError> {
    let mut budget = cx.budget();
    let reach_a = tight_body_box(a, cx)?.max_dimension();
    let reach_b = tight_body_box(b, cx)?.max_dimension();
    let Some(origin) = a.vertices().next().map(scx_brep::Vertex::point) else {
        return Err(BodyqError::Refused(BodyqRefusal::EmptyBody));
    };
    let mut directions: Vec<UnitVec3> = Vec::new();
    let size = reach_a.max(reach_b);
    let c = body_coincidence(a)?.max(body_coincidence(b)?);
    for f in a.faces() {
        let Surface::Plane(p) = f.surface() else {
            continue;
        };
        let n = p.frame().z();
        // Each direction once, either way round.
        if !directions.iter().any(|d| d.cross(*n).length() * size <= c) {
            directions.push(n);
        }
    }
    let mut a_prism = false;
    for d in directions {
        let Some(frame) = frame_along(origin, d) else {
            continue;
        };
        let Recognition::Prism(sa) =
            prism_section_charged(a, &frame, reach_a, cx.scratch, &mut budget, cx.stop)?
        else {
            continue;
        };
        a_prism = true;
        let Recognition::Prism(sb) =
            prism_section_charged(b, &frame, reach_b, cx.scratch, &mut budget, cx.stop)?
        else {
            continue;
        };
        let tol = sa.tolerance.mm().max(sb.tolerance.mm());
        if (sa.lo - sb.lo).abs() > tol || (sa.hi - sb.hi).abs() > tol {
            continue;
        }
        if sa.holes.len() != sb.holes.len() || !same_loop(&sa.outer, &sb.outer, tol) {
            continue;
        }
        let mut taken = vec![false; sb.holes.len()];
        let all = sa.holes.iter().all(|h| {
            sb.holes
                .iter()
                .enumerate()
                .position(|(i, g)| !taken.get(i).copied().unwrap_or(true) && same_loop(h, g, tol))
                .is_some_and(|i| {
                    if let Some(t) = taken.get_mut(i) {
                        *t = true;
                    }
                    true
                })
        });
        if all {
            return Ok(Equivalence::Same);
        }
    }
    Ok(if a_prism {
        Equivalence::Different
    } else {
        Equivalence::NoPrism
    })
}

/// The frame at `origin` with z along `d` and x the world axis `d` is least along.
fn frame_along(origin: Point3, d: UnitVec3) -> Option<Frame> {
    let axis = if d.x.abs() <= d.y.abs() && d.x.abs() <= d.z.abs() {
        Vec3::new(1.0, 0.0, 0.0)
    } else if d.y.abs() <= d.z.abs() {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    Frame::new(origin, *d, axis)
}
