// Written for solidcorex. Apache-2.0.
//! Test bodies built through `Draft`, and the T10 comparison.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;

use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, FaceRecord, RegionId, ShellId, ShellRecord,
    VertexId, VertexRecord,
};
use scx_curve::{Circle, Curve, Line};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Cylinder, Plane, Surface};
use scx_testkit::analytic;
use scx_testkit::bodies::made::{block, cone, cylinder, sphere, torus};
use scx_testkit::csg::{Csg, Primitive};

/// A one-solid body being written, every face's front the outside.
pub struct Builder {
    pub draft: Draft,
    doc: Document,
    outside: ShellId,
    solid: ShellId,
    lines: BTreeMap<(VertexId, VertexId), EdgeId>,
}

impl Builder {
    pub fn new() -> Self {
        let mut draft = Draft::new();
        let outside = draft
            .add_shell(ShellRecord::new(RegionId::OUTSIDE))
            .unwrap();
        let region = draft.add_region(true).unwrap();
        let solid = draft.add_shell(ShellRecord::new(region)).unwrap();
        Self {
            draft,
            doc: Document::default(),
            outside,
            solid,
            lines: BTreeMap::new(),
        }
    }

    pub fn vertex(&mut self, p: [f64; 3]) -> VertexId {
        self.draft
            .add_vertex(VertexRecord::new(Point3::new(p[0], p[1], p[2])))
            .unwrap()
    }

    fn point(&self, v: VertexId) -> Point3 {
        self.draft.vertex(v).unwrap().point()
    }

    /// The straight edge from `a` to `b`, made once, with the sense of a use from `a`.
    pub fn line(&mut self, a: VertexId, b: VertexId) -> (EdgeId, Sense) {
        let (lo, hi, sense) = if a < b {
            (a, b, Sense::Forward)
        } else {
            (b, a, Sense::Reversed)
        };
        if let Some(&e) = self.lines.get(&(lo, hi)) {
            return (e, sense);
        }
        let (p, q) = (self.point(lo), self.point(hi));
        let line = Line::new(p, q - p).unwrap();
        let e = self
            .draft
            .add_edge(EdgeGeom::new(
                EdgeShape::Curve {
                    curve: Curve::Line(line),
                    interval: Extent1d::new(0.0, 1.0).unwrap(),
                },
                [lo, hi],
            ))
            .unwrap();
        self.lines.insert((lo, hi), e);
        (e, sense)
    }

    /// The arc of the circle of `radius` about `frame`'s z over `[t0, t1]`, between `ends`.
    pub fn arc(
        &mut self,
        frame: Frame,
        radius: f64,
        t0: f64,
        t1: f64,
        ends: [VertexId; 2],
    ) -> EdgeId {
        let c = Circle::new(frame, Length::from_mm(radius), Sense::Forward).unwrap();
        self.draft
            .add_edge(EdgeGeom::new(
                EdgeShape::Curve {
                    curve: Curve::Circle(c),
                    interval: Extent1d::new(t0, t1).unwrap(),
                },
                ends,
            ))
            .unwrap()
    }

    pub fn face(&mut self, surface: Surface, loops: &[Vec<(EdgeId, Sense)>]) {
        let records = self.draft.exact_loops(&surface, loops, &self.doc).unwrap();
        self.draft
            .add_face(FaceRecord::new(
                surface,
                [self.outside, self.solid],
                records,
            ))
            .unwrap();
    }

    /// A planar face over the ring of vertices, anticlockwise seen from outside.
    pub fn polygon(&mut self, ring: &[VertexId]) {
        let pts: Vec<Point3> = ring.iter().map(|&v| self.point(v)).collect();
        // Newell's normal.
        let mut n = Vec3::new(0.0, 0.0, 0.0);
        for (i, a) in pts.iter().enumerate() {
            let b = pts[(i + 1) % pts.len()];
            n += Vec3::new(
                (a.y - b.y) * (a.z + b.z),
                (a.z - b.z) * (a.x + b.x),
                (a.x - b.x) * (a.y + b.y),
            );
        }
        let frame = Frame::new(pts[0], n, pts[1] - pts[0]).unwrap();
        let uses: Vec<_> = ring
            .iter()
            .zip(ring.iter().cycle().skip(1))
            .map(|(&a, &b)| self.line(a, b))
            .collect();
        self.face(Surface::Plane(Plane::new(frame)), &[uses]);
    }

    pub fn finish(self) -> Brep {
        self.draft.finish(&self.doc).map_err(|u| u.cause).unwrap().0
    }
}

/// A convex or not polyhedron: `faces` are vertex rings anticlockwise from outside.
pub fn polyhedron(vertices: &[[f64; 3]], faces: &[&[usize]]) -> Brep {
    let mut b = Builder::new();
    let ids: Vec<VertexId> = vertices.iter().map(|&p| b.vertex(p)).collect();
    for f in faces {
        let ring: Vec<VertexId> = f.iter().map(|&i| ids[i]).collect();
        b.polygon(&ring);
    }
    b.finish()
}

/// The rhombic dodecahedron about the origin: the cube corners `(+-1, +-1, +-1)` and the
/// axis points `(+-2, 0, 0)` and so on. Seen from the centre, every edge lies in a diagonal
/// plane, and the axes and body diagonals run through vertices.
pub fn rhombic_dodecahedron() -> Brep {
    let mut v: Vec<[f64; 3]> = Vec::new();
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for z in [-1.0, 1.0] {
                v.push([x, y, z]);
            }
        }
    }
    let axis = |i: usize, s: f64| {
        let mut p = [0.0; 3];
        p[i] = 2.0 * s;
        p
    };
    for i in 0..3 {
        for s in [-1.0, 1.0] {
            v.push(axis(i, s));
        }
    }
    let find = |p: [f64; 3]| v.iter().position(|q| *q == p).unwrap();
    // A face for each pair of axis points on different axes, one sign each: its other two
    // corners are the cube corners between them, off the third axis by +-1.
    let mut faces: Vec<Vec<usize>> = Vec::new();
    for i in 0..3 {
        for j in (i + 1)..3 {
            let k = 3 - i - j;
            for si in [-1.0, 1.0] {
                for sj in [-1.0, 1.0] {
                    let a = axis(i, si);
                    let b = axis(j, sj);
                    let mut c0 = [0.0; 3];
                    c0[i] = si;
                    c0[j] = sj;
                    let mut c1 = c0;
                    c0[k] = 1.0;
                    c1[k] = -1.0;
                    let mut ring = vec![find(a), find(c0), find(b), find(c1)];
                    // Anticlockwise about the outward normal (si, sj along i, j).
                    let pa = v[ring[0]];
                    let (p1, p2) = (v[ring[1]], v[ring[2]]);
                    let e1 = [p1[0] - pa[0], p1[1] - pa[1], p1[2] - pa[2]];
                    let e2 = [p2[0] - pa[0], p2[1] - pa[1], p2[2] - pa[2]];
                    let n = [
                        e1[1] * e2[2] - e1[2] * e2[1],
                        e1[2] * e2[0] - e1[0] * e2[2],
                        e1[0] * e2[1] - e1[1] * e2[0],
                    ];
                    let mut out = [0.0; 3];
                    out[i] = si;
                    out[j] = sj;
                    if n[0] * out[0] + n[1] * out[1] + n[2] * out[2] < 0.0 {
                        ring.reverse();
                    }
                    faces.push(ring);
                }
            }
        }
    }
    let rings: Vec<&[usize]> = faces.iter().map(Vec::as_slice).collect();
    polyhedron(&v, &rings)
}

/// The unit cube `[0, 1]^3`.
pub fn unit_cube() -> Brep {
    let v = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];
    polyhedron(
        &v,
        &[
            &[0, 3, 2, 1],
            &[4, 5, 6, 7],
            &[0, 1, 5, 4],
            &[1, 2, 6, 5],
            &[2, 3, 7, 6],
            &[3, 0, 4, 7],
        ],
    )
}

/// The pin's bar running into the post (`brep_query_cache_lifetime`): the union of the
/// 40 x 16 x 10 box at the origin and the R8 x 20 cylinder about (40, 8), written as the
/// union's faces. The bar's sides `y = 0` and `y = 16` touch the post along its rulings at
/// `x = 40`; the bar's end `x = 40` lies inside the post and is gone.
pub fn bar_into_post() -> Brep {
    use core::f64::consts::{FRAC_PI_2, PI, TAU};
    let mut b = Builder::new();
    let z = Vec3::new(0.0, 0.0, 1.0);
    let x = Vec3::new(1.0, 0.0, 0.0);
    let axis = |h: f64| Frame::new(Point3::new(40.0, 8.0, h), z, x).unwrap();
    let [a0, b0, c0, d0] = [
        [0.0, 0.0, 0.0],
        [40.0, 0.0, 0.0],
        [40.0, 16.0, 0.0],
        [0.0, 16.0, 0.0],
    ]
    .map(|p| b.vertex(p));
    let [a1, b1, c1, d1] = [
        [0.0, 0.0, 10.0],
        [40.0, 0.0, 10.0],
        [40.0, 16.0, 10.0],
        [0.0, 16.0, 10.0],
    ]
    .map(|p| b.vertex(p));
    let s0 = b.vertex([48.0, 8.0, 0.0]);
    let s1 = b.vertex([48.0, 8.0, 20.0]);
    // The bottom's half circle beyond the bar, split at the seam; the bar top's half inside.
    let right_lo = b.arc(axis(0.0), 8.0, 1.5 * PI, TAU, [b0, s0]);
    let right_hi = b.arc(axis(0.0), 8.0, 0.0, FRAC_PI_2, [s0, c0]);
    let left = b.arc(axis(10.0), 8.0, FRAC_PI_2, 1.5 * PI, [c1, b1]);
    let cap = b.arc(axis(20.0), 8.0, 0.0, TAU, [s1, s1]);
    let (seam, up) = b.line(s0, s1);
    let f = Sense::Forward;
    let r = Sense::Reversed;
    // Bottom, normal -z: clockwise seen from above.
    let l = [b.line(a0, d0), b.line(d0, c0)];
    let l2 = [b.line(b0, a0)];
    b.face(
        Surface::Plane(Plane::new(
            Frame::new(Point3::new(0.0, 0.0, 0.0), -z, x).unwrap(),
        )),
        &[vec![l[0], l[1], (right_hi, r), (right_lo, r), l2[0]]],
    );
    // The bar's top, normal +z: round the bar, back along the post's inner half.
    let t = [b.line(a1, b1), b.line(c1, d1), b.line(d1, a1)];
    b.face(
        Surface::Plane(Plane::new(
            Frame::new(Point3::new(0.0, 0.0, 10.0), z, x).unwrap(),
        )),
        &[vec![t[0], (left, r), t[1], t[2]]],
    );
    b.polygon(&[a0, b0, b1, a1]);
    b.polygon(&[d0, d1, c1, c0]);
    b.polygon(&[a0, a1, d1, d0]);
    // The post's wall: from the seam's foot round the bottom to the bar, up its side, round
    // inside the bar's top, down its other side, on to the seam, up it, back round the
    // cap's rim, down the seam.
    let wall =
        Surface::Cylinder(Cylinder::new(axis(0.0), Length::from_mm(8.0), Sense::Forward).unwrap());
    let up_c = b.line(c0, c1);
    let down_b = b.line(b1, b0);
    b.face(
        wall,
        &[vec![
            (right_hi, f),
            up_c,
            (left, f),
            down_b,
            (right_lo, f),
            (seam, up),
            (cap, r),
            (seam, up.reversed()),
        ]],
    );
    b.face(Surface::Plane(Plane::new(axis(20.0))), &[vec![(cap, f)]]);
    b.finish()
}

/// The T10 twin of [`bar_into_post`].
pub fn bar_into_post_csg() -> Csg {
    let world = world();
    Csg::leaf(
        &world,
        Primitive::Block {
            size: [40.0, 16.0, 10.0],
        },
    )
    .unwrap()
    .union(
        Csg::leaf(
            &analytic::Frame {
                origin: [40.0, 8.0, 0.0],
                ..world
            },
            Primitive::Cylinder {
                radius: 8.0,
                height: 20.0,
            },
        )
        .unwrap(),
    )
}

pub fn world() -> analytic::Frame {
    analytic::Frame {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    }
}

/// The T10 frame of a `scx-num` frame.
pub fn csg_frame(f: &Frame) -> analytic::Frame {
    let a = |v: Vec3| [v.x, v.y, v.z];
    let o = f.origin();
    analytic::Frame {
        origin: [o.x, o.y, o.z],
        x: a(f.x().as_vec()),
        y: a(f.y().as_vec()),
        z: a(f.z().as_vec()),
    }
}

/// The one solid region of a one-solid body.
pub fn solid(body: &Brep) -> RegionId {
    body.regions()
        .find(|r| r.is_solid())
        .map(scx_brep::Region::id)
        .expect("a solid region")
}

/// The make primitives in a world frame, a turned one and a far one, each with its T10
/// twin.
pub fn primitives() -> Vec<(String, Brep, Csg)> {
    let turned = |o: [f64; 3]| {
        Frame::new(
            Point3::new(o[0], o[1], o[2]),
            Vec3::new(0.3, -0.5, 0.81),
            Vec3::new(1.0, 0.2, 0.0),
        )
        .unwrap()
    };
    let frames = [
        (
            "world",
            Frame::new(
                Point3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 0.0),
            )
            .unwrap(),
        ),
        ("turned", turned([3.1, -2.7, 1.9])),
        ("far", turned([1.2e4, -0.8e4, 0.5e4])),
    ];
    let d = Document::default();
    let mut out = Vec::new();
    for (name, f) in frames {
        let cf = csg_frame(&f);
        let leaf = |p| Csg::leaf(&cf, p).unwrap();
        out.push((
            format!("block {name}"),
            block(
                &f,
                [
                    Length::from_mm(3.0),
                    Length::from_mm(2.0),
                    Length::from_mm(1.5),
                ],
                &d,
            )
            .unwrap()
            .body,
            leaf(Primitive::Block {
                size: [3.0, 2.0, 1.5],
            }),
        ));
        out.push((
            format!("cylinder {name}"),
            cylinder(&f, Length::from_mm(1.2), Length::from_mm(2.5), &d)
                .unwrap()
                .body,
            leaf(Primitive::Cylinder {
                radius: 1.2,
                height: 2.5,
            }),
        ));
        for (r0, r1) in [(1.5, 0.6), (1.4, 0.0), (0.0, 1.1)] {
            out.push((
                format!("cone {r0} {r1} {name}"),
                cone(
                    &f,
                    [Length::from_mm(r0), Length::from_mm(r1)],
                    Length::from_mm(2.0),
                    &d,
                )
                .unwrap()
                .body,
                leaf(Primitive::Frustum {
                    radius0: r0,
                    radius1: r1,
                    height: 2.0,
                }),
            ));
        }
        out.push((
            format!("sphere {name}"),
            sphere(&f, Length::from_mm(1.3), &d).unwrap().body,
            leaf(Primitive::Sphere { radius: 1.3 }),
        ));
        out.push((
            format!("torus {name}"),
            torus(&f, Length::from_mm(2.0), Length::from_mm(0.6), &d)
                .unwrap()
                .body,
            leaf(Primitive::Torus {
                major: 2.0,
                minor: 0.6,
            }),
        ));
    }
    out
}
