// Written for solidcorex. Apache-2.0.
//! Bodies for the edge-along tests (`tangent_edge`, `continuity`): made bodies and
//! extrusions from `scx-testkit`, and small sheets written through `Draft` where no made
//! body has the edge (a tilted wall, a NURBS crease, a NURBS face).
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use scx_bodyq::{Tangency, tangent_all_along};
use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, FaceId, FaceRecord, LoopRecord,
    RegionId, ShellId, ShellRecord, UseGeom, VertexId, VertexRecord,
};
use scx_curve::{Curve, Curve2, Ellipse, Line, Line2};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{KnotVector, NurbsCurve};
use scx_nurbs_surface::{Net, NurbsSurface};
use scx_planar::{Region, Segment};
use scx_surface::{Cylinder, Plane, Surface, pcurve_of};
use scx_testkit::bodies::made::{ExtrudeFaces, Span, extrude};

pub fn doc() -> Document {
    Document::default()
}

pub fn deg(d: f64) -> f64 {
    d.to_radians()
}

/// The verdict at `angle`, unbudgeted.
pub fn tangency(body: &Brep, e: EdgeId, angle: f64) -> Tangency {
    tangent_all_along(body, e, angle, &mut Budget::unlimited(), &Stop::new()).unwrap()
}

/// The edges whose two fins are on `a` and `b`, in id order.
pub fn between(body: &Brep, a: FaceId, b: FaceId) -> Vec<EdgeId> {
    body.edges()
        .filter(|e| {
            let f: Vec<FaceId> = e.fins().map(|f| f.face).collect();
            f.len() == 2 && ((f[0] == a && f[1] == b) || (f[0] == b && f[1] == a))
        })
        .map(scx_brep::Edge::id)
        .collect()
}

/// The edges with both fins on one face (seams), not collapsed.
pub fn seams(body: &Brep) -> Vec<EdgeId> {
    body.edges()
        .filter(|e| {
            let f: Vec<FaceId> = e.fins().map(|f| f.face).collect();
            f.len() == 2 && f[0] == f[1] && matches!(e.shape(), EdgeShape::Curve { .. })
        })
        .map(scx_brep::Edge::id)
        .collect()
}

/// The edge curve's interval.
pub fn span_of(body: &Brep, e: EdgeId) -> (f64, f64) {
    match body.edge(e).unwrap().shape() {
        EdgeShape::Curve { interval, .. } => (interval.min(), interval.max()),
        EdgeShape::Collapsed => panic!("collapsed"),
    }
}

fn p2(p: [f64; 2]) -> Point2 {
    Point2::new(p[0], p[1])
}

/// The polygon through `pts` extruded from 0 to `h` along z.
pub fn extrude_polygon(pts: &[[f64; 2]], h: f64) -> (Brep, ExtrudeFaces) {
    let n = pts.len();
    let segs = (0..n)
        .map(|i| Segment::line(p2(pts[i]), p2(pts[(i + 1) % n])))
        .collect();
    extruded(segs, h)
}

pub fn extruded(segs: Vec<Segment>, h: f64) -> (Brep, ExtrudeFaces) {
    let d = doc();
    let region = Region::new(segs, vec![], d.resolution()).unwrap();
    let m = extrude(
        &region,
        &Frame::WORLD,
        Span::new(Length::from_mm(0.0), Length::from_mm(h)),
        &d,
    )
    .unwrap();
    (m.body, m.faces)
}

/// A stadium (two lines and two half circles of radius 2) extruded by 3.
pub fn stadium() -> (Brep, ExtrudeFaces) {
    extruded(
        vec![
            Segment::line(p2([-6.0, 5.0]), p2([6.0, 5.0])),
            Segment::arc(
                p2([6.0, 7.0]),
                p2([6.0, 5.0]),
                p2([6.0, 9.0]),
                Sense::Forward,
            ),
            Segment::line(p2([6.0, 9.0]), p2([-6.0, 9.0])),
            Segment::arc(
                p2([-6.0, 7.0]),
                p2([-6.0, 9.0]),
                p2([-6.0, 5.0]),
                Sense::Forward,
            ),
        ],
        3.0,
    )
}

/// Two arcs tangent inside one another at `(3, 0)`: the half circle of radius 3 about the
/// origin above, the line back along the axis, the half circle of radius 2 about `(1, 0)`
/// below; extruded by 2.
pub fn nested_arcs() -> (Brep, ExtrudeFaces) {
    extruded(
        vec![
            Segment::arc(
                p2([0.0, 0.0]),
                p2([3.0, 0.0]),
                p2([-3.0, 0.0]),
                Sense::Forward,
            ),
            Segment::line(p2([-3.0, 0.0]), p2([-1.0, 0.0])),
            Segment::arc(
                p2([1.0, 0.0]),
                p2([-1.0, 0.0]),
                p2([3.0, 0.0]),
                Sense::Forward,
            ),
        ],
        2.0,
    )
}

// ---- sheets written through Draft ------------------------------------------------------------

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

/// A sheet being written: every face's both sides the outside.
struct Sheet {
    d: Draft,
    outside: ShellId,
    curves: std::collections::BTreeMap<EdgeId, (Curve, Extent1d)>,
    points: std::collections::BTreeMap<VertexId, Point3>,
}

impl Sheet {
    fn new() -> Self {
        let mut d = Draft::new();
        let outside = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
        Self {
            d,
            outside,
            curves: std::collections::BTreeMap::new(),
            points: std::collections::BTreeMap::new(),
        }
    }

    fn vertex(&mut self, p: Point3) -> VertexId {
        let v = self.d.add_vertex(VertexRecord::new(p)).unwrap();
        self.points.insert(v, p);
        v
    }

    fn edge(&mut self, curve: Curve, lo: f64, hi: f64, ends: [VertexId; 2]) -> EdgeId {
        let interval = Extent1d::new(lo, hi).unwrap();
        let e = self
            .d
            .add_edge(EdgeGeom::new(
                EdgeShape::Curve {
                    curve: curve.clone(),
                    interval,
                },
                ends,
            ))
            .unwrap();
        self.curves.insert(e, (curve, interval));
        e
    }

    fn line(&mut self, a: VertexId, b: VertexId) -> EdgeId {
        let (p, q) = (self.points[&a], self.points[&b]);
        self.edge(Curve::Line(Line::new(p, q - p).unwrap()), 0.0, 1.0, [a, b])
    }

    fn curve_of(&self, e: EdgeId) -> (Curve, Extent1d) {
        self.curves[&e].clone()
    }

    /// A face of `surface` with one loop of `uses`, each with its pcurve; `turn` writes the
    /// loop the other way round.
    fn face_with(&mut self, surface: Surface, uses: Vec<(EdgeId, Sense, Curve2)>, turn: bool) {
        let mut uses: Vec<EdgeUse> = uses
            .into_iter()
            .map(|(e, s, c)| {
                let (_, interval) = self.curve_of(e);
                let s = if turn { s.reversed() } else { s };
                EdgeUse::new(e, s, UseGeom::Pcurve { curve: c, interval })
            })
            .collect();
        if turn {
            uses.reverse();
        }
        self.d
            .add_face(FaceRecord::new(
                surface,
                [self.outside, self.outside],
                vec![LoopRecord::new(uses)],
            ))
            .unwrap();
    }

    /// As [`Sheet::face_with`], each pcurve the closed form `pcurve_of` gives.
    fn face(&mut self, surface: Surface, uses: &[(EdgeId, Sense)], turn: bool) {
        let res = doc().resolution();
        let uses = uses
            .iter()
            .map(|&(e, s)| {
                let (c, i) = self.curve_of(e);
                (e, s, pcurve_of(&c, &surface, i, res).unwrap())
            })
            .collect();
        self.face_with(surface, uses, turn);
    }
}

/// The first of the four ways to write two faces' loops that `finish` takes.
fn first_finished(build: impl Fn(bool, bool) -> Sheet) -> Brep {
    let mut why = Vec::new();
    for (a, b) in [(false, false), (true, false), (false, true), (true, true)] {
        match build(a, b).d.finish(&doc()) {
            Ok((body, _)) => return body,
            Err(u) => why.push(format!("{:?}", u.cause)),
        }
    }
    panic!("no way round finishes: {why:?}");
}

/// A scoop: the plane `z = 0` inside the ellipse arc `t in [-0.5, pi + 0.5]` closed by its
/// chord, and the wall of radius `r` rising from that arc, its axis tilted by `tilt` from z
/// towards x, to the height 2. Returns the sheet and the arc. The wall meets the floor at
/// `pi / 2 - tilt` at `t = 0` and `pi / 2 + tilt` at `t = pi`, between them in between.
pub fn tilted_sheet(tilt: f64, r: f64) -> (Brep, EdgeId) {
    let axis = Vec3::new(fmath::sin(tilt), 0.0, fmath::cos(tilt));
    let (t0, t1) = (-0.5, std::f64::consts::PI + 0.5);
    let h = 2.0;
    let ellipse = |o: Point3| {
        Curve::Ellipse(
            Ellipse::new(
                Frame::new(o, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0)).unwrap(),
                Length::from_mm(r / fmath::cos(tilt)),
                Length::from_mm(r),
                Sense::Forward,
            )
            .unwrap(),
        )
    };
    let bottom = ellipse(Point3::ORIGIN);
    let top = ellipse(Point3::ORIGIN + axis * (h / fmath::cos(tilt)));
    let wall = Surface::Cylinder(
        Cylinder::new(
            // the seam at -y, outside the patch
            Frame::new(Point3::ORIGIN, axis, Vec3::new(0.0, -1.0, 0.0)).unwrap(),
            Length::from_mm(r),
            Sense::Forward,
        )
        .unwrap(),
    );
    let floor = Surface::Plane(Plane::new(Frame::WORLD));
    let body = first_finished(|ta, tb| {
        let mut s = Sheet::new();
        let v0 = s.vertex(bottom.point(t0));
        let v1 = s.vertex(bottom.point(t1));
        let w0 = s.vertex(top.point(t0));
        let w1 = s.vertex(top.point(t1));
        let arc = s.edge(bottom.clone(), t0, t1, [v0, v1]);
        let chord = s.line(v1, v0);
        let up0 = s.line(v0, w0);
        let up1 = s.line(v1, w1);
        let crest = s.edge(top.clone(), t0, t1, [w0, w1]);
        s.face(
            floor.clone(),
            &[(arc, Sense::Forward), (chord, Sense::Forward)],
            ta,
        );
        s.face(
            wall.clone(),
            &[
                (arc, Sense::Reversed),
                (up0, Sense::Forward),
                (crest, Sense::Forward),
                (up1, Sense::Reversed),
            ],
            tb,
        );
        s
    });
    // the arc is the one edge on an ellipse with two fins
    let rim = body
        .edges()
        .find(|e| {
            e.fins().len() == 2
                && matches!(
                    e.shape(),
                    EdgeShape::Curve {
                        curve: Curve::Ellipse(_),
                        ..
                    }
                )
        })
        .map(scx_brep::Edge::id);
    (body, rim.unwrap())
}

fn nurbs_line() -> Curve {
    // a straight line along x from 0 to 4 at an uneven speed: quadratic, interior knots
    let pts: Vec<[f64; 3]> = [0.0, 0.5, 2.0, 3.0, 4.0]
        .iter()
        .map(|&x| [x, 0.0, 0.0])
        .collect();
    Curve::Nurbs(Arc::new(
        NurbsCurve::<3>::new(
            KnotVector::new(2, vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0]).unwrap(),
            pts,
            None,
        )
        .unwrap(),
    ))
}

/// The floor `z = 0` and the wall `y = 0` over `[0, 4]` each, meeting square along x on a
/// NURBS curve with interior knots. Returns the sheet and the crease.
pub fn nurbs_crease() -> (Brep, EdgeId) {
    let floor = Surface::Plane(Plane::new(Frame::WORLD));
    let wall = Surface::Plane(Plane::new(
        Frame::new(
            Point3::ORIGIN,
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap(),
    ));
    let crease = nurbs_line();
    let body = first_finished(|ta, tb| {
        let mut s = Sheet::new();
        let a = s.vertex(pt([0.0, 0.0, 0.0]));
        let b = s.vertex(pt([4.0, 0.0, 0.0]));
        let fa = s.vertex(pt([4.0, 4.0, 0.0]));
        let fb = s.vertex(pt([0.0, 4.0, 0.0]));
        let wa = s.vertex(pt([4.0, 0.0, 4.0]));
        let wb = s.vertex(pt([0.0, 0.0, 4.0]));
        let e = s.edge(crease.clone(), 0.0, 3.0, [a, b]);
        let f1 = s.line(b, fa);
        let f2 = s.line(fa, fb);
        let f3 = s.line(fb, a);
        let w1 = s.line(b, wa);
        let w2 = s.line(wa, wb);
        let w3 = s.line(wb, a);
        s.face(
            floor.clone(),
            &[
                (e, Sense::Forward),
                (f1, Sense::Forward),
                (f2, Sense::Forward),
                (f3, Sense::Forward),
            ],
            ta,
        );
        s.face(
            wall.clone(),
            &[
                (w3, Sense::Reversed),
                (w2, Sense::Reversed),
                (w1, Sense::Reversed),
                (e, Sense::Reversed),
            ],
            tb,
        );
        s
    });
    let e = body
        .edges()
        .find(|e| {
            matches!(
                e.shape(),
                EdgeShape::Curve {
                    curve: Curve::Nurbs(_),
                    ..
                }
            )
        })
        .unwrap()
        .id();
    (body, e)
}

/// The plane `z = 0` over `[0, 4] x [0, 4]` and a flat NURBS patch rising from its edge
/// along x at 45 degrees. Returns the sheet, the shared edge and the NURBS face.
pub fn nurbs_meets_plane() -> (Brep, EdgeId, FaceId) {
    let floor = Surface::Plane(Plane::new(Frame::WORLD));
    // S(u, v) = (4 u, -2 v, 2 v): u along x, v up and out at 45 degrees
    let patch = Surface::Nurbs(Arc::new(
        NurbsSurface::<3>::new(
            KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap(),
            KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap(),
            Net::new(
                2,
                2,
                vec![
                    [0.0, 0.0, 0.0],
                    [0.0, -2.0, 2.0],
                    [4.0, 0.0, 0.0],
                    [4.0, -2.0, 2.0],
                ],
            )
            .unwrap(),
            None,
        )
        .unwrap(),
    ));
    let line2 = |o: [f64; 2], d: [f64; 2]| {
        Curve2::Line(Line2::new(Point2::new(o[0], o[1]), Vec2::new(d[0], d[1])).unwrap())
    };
    let body = first_finished(|ta, tb| {
        let mut s = Sheet::new();
        let a = s.vertex(pt([0.0, 0.0, 0.0]));
        let b = s.vertex(pt([4.0, 0.0, 0.0]));
        let fa = s.vertex(pt([4.0, 4.0, 0.0]));
        let fb = s.vertex(pt([0.0, 4.0, 0.0]));
        let pa = s.vertex(pt([4.0, -2.0, 2.0]));
        let pb = s.vertex(pt([0.0, -2.0, 2.0]));
        let e = s.line(a, b);
        let f1 = s.line(b, fa);
        let f2 = s.line(fa, fb);
        let f3 = s.line(fb, a);
        let p1 = s.line(b, pa);
        let p2 = s.line(pa, pb);
        let p3 = s.line(pb, a);
        s.face(
            floor.clone(),
            &[
                (e, Sense::Forward),
                (f1, Sense::Forward),
                (f2, Sense::Forward),
                (f3, Sense::Forward),
            ],
            ta,
        );
        // uv pcurves of the patch's boundary, each at its line's parameter in [0, 1]
        s.face_with(
            patch.clone(),
            vec![
                (e, Sense::Reversed, line2([0.0, 0.0], [1.0, 0.0])),
                (p3, Sense::Reversed, line2([0.0, 1.0], [0.0, -1.0])),
                (p2, Sense::Reversed, line2([1.0, 1.0], [-1.0, 0.0])),
                (p1, Sense::Reversed, line2([1.0, 0.0], [0.0, 1.0])),
            ],
            tb,
        );
        s
    });
    let face = body
        .faces()
        .find(|f| matches!(f.surface(), Surface::Nurbs(_)))
        .unwrap()
        .id();
    let plane = body
        .faces()
        .find(|f| matches!(f.surface(), Surface::Plane(_)))
        .unwrap()
        .id();
    let e = between(&body, plane, face)[0];
    (body, e, face)
}
