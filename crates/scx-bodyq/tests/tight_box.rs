// Written for solidcorex. Apache-2.0.
//! `tight_box` against closed forms (research 74 §22, BW2): bodies of known extent, trimmed
//! patches whose extent is known, the pin's `issue_1054` and `issue_1799` cases judged against
//! the exact values (not the pin's output), and the memo's rules.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::similar_names
)]

use std::f64::consts::{FRAC_1_SQRT_2, TAU};
use std::sync::Arc;

use scx_bodyq::{BodyqError, tight_body_box, tight_box, tight_box_charged};
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, Entity, FaceRecord, LoopRecord,
    RegionId, ShellId, ShellRecord, UseGeom, VertexId, VertexRecord,
};
use scx_curve::{Circle, Curve, Curve2, Line, Line2};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{KnotVector, NurbsCurve};
use scx_nurbs_surface::{Net, NurbsSurface};
use scx_surface::{Sphere, Surface, pcurve_of};
use scx_testkit::bodies::{Kind, generate};

// ---- helpers -------------------------------------------------------------------------------

fn finish(d: Draft) -> Brep {
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

fn generated(kind: Kind, seed: u64, size: usize) -> Brep {
    finish(generate(kind, seed, size).unwrap().draft)
}

/// Runs `f` with a context over a fresh scratch.
fn with_cx<R>(scratch: &mut Scratch, settings: &Settings, f: impl FnOnce(&mut Cx<'_>) -> R) -> R {
    let doc = Document::default();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, settings, scratch, &stop, &NoTrace);
    f(&mut cx)
}

fn tight(body: &Brep, e: Entity) -> Extent3d {
    let mut scratch = Scratch::new();
    with_cx(&mut scratch, &Settings::new(), |cx| {
        tight_box(body, e, cx).unwrap()
    })
}

fn corners(b: &Extent3d) -> [f64; 6] {
    let (l, h) = (b.min(), b.max());
    [l.x, l.y, l.z, h.x, h.y, h.z]
}

fn zone(body: &Brep) -> f64 {
    body.resolution().zone().mm()
}

/// `got` holds `truth` and exceeds it by at most `slack` on every side (the sides are
/// `[min x, min y, min z, max x, max y, max z]`).
fn assert_tight(what: &str, got: &Extent3d, truth: [f64; 6], slack: f64) {
    let g = corners(got);
    for i in 0..6 {
        let out = if i < 3 {
            truth[i] - g[i]
        } else {
            g[i] - truth[i]
        };
        assert!(
            out >= -1e-12,
            "{what}: side {i} is {} and cuts the true {} (box {g:?}, true {truth:?})",
            g[i],
            truth[i]
        );
        assert!(
            out <= slack,
            "{what}: side {i} is {} past the true {} by {out:e}, over {slack:e} (box {g:?})",
            g[i],
            truth[i]
        );
    }
}

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

fn frame(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    Frame::new(
        pt(o),
        Vec3::new(z[0], z[1], z[2]),
        Vec3::new(x[0], x[1], x[2]),
    )
    .unwrap()
}

// ---- a small body builder ------------------------------------------------------------------

/// A sheet or wire body being written: the outside shell, both sides of every face.
struct B {
    d: Draft,
    outside: ShellId,
}

impl B {
    fn new() -> Self {
        let mut d = Draft::new();
        let outside = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
        Self { d, outside }
    }

    fn vertex(&mut self, p: [f64; 3]) -> VertexId {
        self.d.add_vertex(VertexRecord::new(pt(p))).unwrap()
    }

    fn edge(&mut self, curve: Curve, lo: f64, hi: f64, ends: [VertexId; 2]) -> EdgeId {
        let interval = Extent1d::new(lo, hi).unwrap();
        self.d
            .add_edge(EdgeGeom::new(EdgeShape::Curve { curve, interval }, ends))
            .unwrap()
    }

    fn line(&mut self, a: VertexId, b: VertexId) -> EdgeId {
        let p = self.d.vertex(a).unwrap().point();
        let q = self.d.vertex(b).unwrap().point();
        self.edge(Curve::Line(Line::new(p, q - p).unwrap()), 0.0, 1.0, [a, b])
    }

    /// A face of `surface` with one loop of `(edge, sense, pcurve, interval)` uses.
    fn face(&mut self, surface: Surface, uses: Vec<(EdgeId, Sense, Curve2, (f64, f64))>) {
        let uses = uses
            .into_iter()
            .map(|(e, s, c, (lo, hi))| {
                EdgeUse::new(
                    e,
                    s,
                    UseGeom::Pcurve {
                        curve: c,
                        interval: Extent1d::new(lo, hi).unwrap(),
                    },
                )
            })
            .collect();
        self.d
            .add_face(FaceRecord::new(
                surface,
                [self.outside, self.outside],
                vec![LoopRecord::new(uses)],
            ))
            .unwrap();
    }
}

fn line2(origin: [f64; 2], dir: [f64; 2]) -> Curve2 {
    Curve2::Line(Line2::new(Point2::new(origin[0], origin[1]), Vec2::new(dir[0], dir[1])).unwrap())
}

fn nurbs_curve(degree: usize, knots: &[f64], pts: &[[f64; 3]], w: Option<Vec<f64>>) -> Curve {
    Curve::Nurbs(Arc::new(
        NurbsCurve::<3>::new(
            KnotVector::new(degree, knots.to_vec()).unwrap(),
            pts.to_vec(),
            w,
        )
        .unwrap(),
    ))
}

/// A wire body of one edge on `curve` over `interval`.
fn wire(curve: &Curve, lo: f64, hi: f64) -> Brep {
    let mut d = Draft::new();
    let a = d.add_vertex(VertexRecord::new(curve.point(lo))).unwrap();
    let b = d.add_vertex(VertexRecord::new(curve.point(hi))).unwrap();
    let e = d
        .add_edge(EdgeGeom::new(
            EdgeShape::Curve {
                curve: curve.clone(),
                interval: Extent1d::new(lo, hi).unwrap(),
            },
            [a, b],
        ))
        .unwrap();
    let mut shell = ShellRecord::new(RegionId::OUTSIDE);
    shell.wires.push(e);
    d.add_shell(shell).unwrap();
    finish(d)
}

fn body_box(body: &Brep) -> Extent3d {
    let mut scratch = Scratch::new();
    with_cx(&mut scratch, &Settings::new(), |cx| {
        tight_body_box(body, cx).unwrap()
    })
}

// ---- closed bodies -------------------------------------------------------------------------

/// A body's tight box against its closed form: the box of the primitive, past it by at most
/// the entities' growth (the zone) and the search's tolerance.
#[test]
fn closed_bodies_are_boxed_to_their_closed_forms() {
    // Seed 0 is canonical: the cylinder is radius 1, height 2 about z; the cone radius 1,
    // height 1.
    let cyl = generated(Kind::Cylinder, 0, 3);
    let z = zone(&cyl);
    assert_tight(
        "cylinder",
        &body_box(&cyl),
        [-1.0, -1.0, 0.0, 1.0, 1.0, 2.0],
        2.0 * z,
    );
    let cone = generated(Kind::Cone, 0, 3);
    assert_tight(
        "cone",
        &body_box(&cone),
        [-1.0, -1.0, 0.0, 1.0, 1.0, 1.0],
        2.0 * z,
    );
    // The sphere and torus by their own surfaces.
    let sphere = generated(Kind::Sphere, 0, 3);
    let s = sphere
        .faces()
        .find_map(|f| match f.surface() {
            Surface::Sphere(s) => Some(*s),
            _ => None,
        })
        .unwrap();
    let (o, r) = (s.frame().origin(), s.radius().mm());
    assert_tight(
        "sphere",
        &body_box(&sphere),
        [o.x - r, o.y - r, o.z - r, o.x + r, o.y + r, o.z + r],
        2.0 * zone(&sphere),
    );
    let torus = generated(Kind::Torus, 0, 3);
    let t = torus
        .faces()
        .find_map(|f| match f.surface() {
            Surface::Torus(t) => Some(*t),
            _ => None,
        })
        .unwrap();
    let (o, big, small) = (t.frame().origin(), t.major().mm(), t.minor().mm());
    assert_tight(
        "torus",
        &body_box(&torus),
        [
            o.x - big - small,
            o.y - big - small,
            o.z - small,
            o.x + big + small,
            o.y + big + small,
            o.z + small,
        ],
        2.0 * zone(&torus),
    );
}

/// Every face of a generated curved body whose uv region is its uv box (a wall between
/// rulings, a lune, a torus patch) is boxed against dense samples of its surface: the box
/// holds every sample and is not past the samples' hull by more than the sampling's own error
/// and the growth.
#[test]
fn rectangular_patches_match_their_sampled_extent() {
    const N: usize = 400;
    for kind in [Kind::Cylinder, Kind::Sphere, Kind::Torus] {
        for seed in [0, 1, 2] {
            for size in [2, 3, 5] {
                let body = generated(kind, seed, size);
                let mut checked = 0;
                for face in body.faces() {
                    let rect = face.uses().all(|u| {
                        matches!(
                            u.geom(),
                            UseGeom::Pcurve {
                                curve: Curve2::Line(_) | Curve2::AngleLine(_),
                                ..
                            } | UseGeom::Point(_)
                        )
                    });
                    let uv = face.uv_box().unwrap();
                    if !rect || matches!(face.surface(), Surface::Plane(_)) {
                        continue;
                    }
                    // The region is the rectangle of the uses' ends (the uv box is a hull
                    // of enclosures, a little larger).
                    let (mut lo, mut hi) = (
                        Point2::new(f64::MAX, f64::MAX),
                        Point2::new(f64::MIN, f64::MIN),
                    );
                    for u in face.uses() {
                        if let UseGeom::Pcurve { curve, interval } = u.geom() {
                            for t in [interval.min(), interval.max()] {
                                let p = curve.point(t);
                                lo = Point2::new(lo.x.min(p.x), lo.y.min(p.y));
                                hi = Point2::new(hi.x.max(p.x), hi.y.max(p.y));
                            }
                        }
                    }
                    let _ = uv;
                    let mut hull = None::<Extent3d>;
                    for i in 0..=N {
                        for j in 0..=N {
                            let p = face.surface().point(Point2::new(
                                lo.x + (hi.x - lo.x) * i as f64 / N as f64,
                                lo.y + (hi.y - lo.y) * j as f64 / N as f64,
                            ));
                            let q = Extent3d::from_point(p).unwrap();
                            hull = Some(hull.map_or(q, |h| h.union(q)));
                        }
                    }
                    let got = tight(&body, Entity::Face(face.id()));
                    // The surface's size bounds the sampling error: a step of h along
                    // a radius R moves a coordinate by at most R h^2 / 2.
                    let h = (hi.x - lo.x).max(hi.y - lo.y) / N as f64;
                    let reach = got.max_dimension();
                    let slack = 2.0 * zone(&body) + reach * h * h + 1e-9;
                    let truth = corners(&hull.unwrap());
                    let g = corners(&got);
                    for i in 0..6 {
                        let out = if i < 3 {
                            truth[i] - g[i]
                        } else {
                            g[i] - truth[i]
                        };
                        assert!(
                            out >= -1e-12 && out <= slack,
                            "{kind:?} seed {seed} size {size} {}: side {i} {} vs sampled {} \
                             (slack {slack:e})",
                            face.id(),
                            g[i],
                            truth[i]
                        );
                    }
                    let loose = face.bounding_box().unwrap();
                    assert!(
                        corners(&loose)
                            .iter()
                            .zip(&g)
                            .enumerate()
                            .all(|(i, (l, t))| if i < 3 {
                                l <= &(t + 1e-12)
                            } else {
                                t <= &(l + 1e-12)
                            }),
                        "the tight box is inside the loose one"
                    );
                    checked += 1;
                }
                assert!(checked > 0, "{kind:?} has patches");
            }
        }
    }
}

// ---- a trimmed patch the loose box cannot follow --------------------------------------------

/// A cap of a sphere of radius `r` bounded by a small circle of angular radius `alpha` about
/// direction `c`, off the pole so the cap is one loop in uv.
fn cap(r: f64, c: [f64; 3], alpha: f64) -> Brep {
    let n = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    let c = [c[0] / n, c[1] / n, c[2] / n];
    let sphere = Surface::Sphere(
        Sphere::new(
            frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            Length::from_mm(r),
            Sense::Forward,
        )
        .unwrap(),
    );
    let centre = [
        r * fmath::cos(alpha) * c[0],
        r * fmath::cos(alpha) * c[1],
        r * fmath::cos(alpha) * c[2],
    ];
    // The circle's x axis is the part of world x across c.
    let f = frame(centre, c, [1.0, 0.0, 0.0]);
    let circle = Curve::Circle(
        Circle::new(f, Length::from_mm(r * fmath::sin(alpha)), Sense::Forward).unwrap(),
    );
    let res = Resolution::default();
    let pc = pcurve_of(&circle, &sphere, Extent1d::new(0.0, TAU).unwrap(), res).unwrap();
    // The region is the cap: try the loop one way, then the other, and keep the one whose
    // cap holds the point of the sphere along `c`.
    let inside = sphere_uv(&sphere, [r * c[0], r * c[1], r * c[2]]);
    for sense in [Sense::Forward, Sense::Reversed] {
        let mut b = B::new();
        let v = b.vertex({
            let p = circle.point(0.0);
            [p.x, p.y, p.z]
        });
        let e = b.edge(circle.clone(), 0.0, TAU, [v, v]);
        b.face(sphere.clone(), vec![(e, sense, pc.clone(), (0.0, TAU))]);
        let body = finish(b.d);
        let f = body.faces().next().unwrap();
        if f.contains_uv(inside) == Some(scx_brep::UvSide::Inside) {
            return body;
        }
    }
    panic!("neither sense gives the cap");
}

fn sphere_uv(s: &Surface, p: [f64; 3]) -> Point2 {
    let Surface::Sphere(s) = s else { panic!() };
    s.invert(pt(p), None).into_foot().at
}

/// The extent of the cap along `e`: `r cos(max(0, gamma - alpha))`.
fn cap_extent(r: f64, c: [f64; 3], alpha: f64, e: [f64; 3]) -> f64 {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cn = dot(c, c).sqrt();
    let gamma = fmath::acos((dot(c, e) / cn).clamp(-1.0, 1.0));
    r * fmath::cos((gamma - alpha).max(0.0))
}

/// A tilted cap's box is the cap's, where the box of the lat-long rectangle round it
/// reaches past (the corners of the rectangle are not in the cap).
#[test]
fn a_tilted_cap_is_boxed_to_the_cap_not_its_uv_hull() {
    let (r, alpha) = (2.0, 20f64.to_radians());
    for c in [
        [0.5, 0.0, 0.866_025_403_784_438_6],
        [0.4, 0.5, 0.7],
        [-0.6, 0.3, 0.5],
    ] {
        let body = cap(r, c, alpha);
        let f = body.faces().next().unwrap();
        let got = tight(&body, Entity::Face(f.id()));
        let loose = f.bounding_box().unwrap();
        let truth = [
            -cap_extent(r, c, alpha, [-1.0, 0.0, 0.0]),
            -cap_extent(r, c, alpha, [0.0, -1.0, 0.0]),
            -cap_extent(r, c, alpha, [0.0, 0.0, -1.0]),
            cap_extent(r, c, alpha, [1.0, 0.0, 0.0]),
            cap_extent(r, c, alpha, [0.0, 1.0, 0.0]),
            cap_extent(r, c, alpha, [0.0, 0.0, 1.0]),
        ];
        let slack = 3.0 * zone(&body) + 1e-9;
        assert_tight("cap", &got, truth, slack);
        // The loose box holds the cap too, and is the looser.
        assert_tight("loose cap", &loose, truth, 10.0);
        let vol = |b: &Extent3d| {
            let g = corners(b);
            (g[3] - g[0]) * (g[4] - g[1]) * (g[5] - g[2])
        };
        assert!(
            vol(&got) < vol(&loose),
            "tight {} against loose {}",
            vol(&got),
            vol(&loose)
        );
    }
}

// ---- issue_1054: the tight box of an edge on part of a NURBS curve --------------------------

/// The quadratic rational quarter circle of radius 50 from (50, 0) to (0, 50), parameter 0 to
/// 1 (the half point at parameter 1/2 is the 45 degree point).
fn quarter_circle() -> Curve {
    let s = FRAC_1_SQRT_2;
    nurbs_curve(
        2,
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        &[[50.0, 0.0, 0.0], [50.0, 50.0, 0.0], [0.0, 50.0, 0.0]],
        Some(vec![1.0, s, 1.0]),
    )
}

/// The degree-one curve through the peaks (0,0) (1,8) (2,0) (3,4) (4,0) (5,20) (6,0): the x
/// of a point is its parameter.
fn degree_one_peaks() -> Curve {
    nurbs_curve(
        1,
        &[0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 6.0],
        &[
            [0.0, 0.0, 0.0],
            [1.0, 8.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 4.0, 0.0],
            [4.0, 0.0, 0.0],
            [5.0, 20.0, 0.0],
            [6.0, 0.0, 0.0],
        ],
        None,
    )
}

/// The edge's tight box with the zone it is grown by (and the zone it was found within)
/// taken as the slack.
fn assert_wire(curve: &Curve, lo: f64, hi: f64, truth: [f64; 6]) {
    let body = wire(curve, lo, hi);
    let got = body_box(&body);
    assert_tight("wire", &got, truth, 3.0 * zone(&body));
    let edge = body.edges().next().unwrap();
    assert_eq!(tight(&body, Entity::Edge(edge.id())), got);
}

/// `whole_body_box_of_a_wire_uses_its_arc_interval`: half of the quarter arc stops at its own
/// 45 degree point; the whole arc is the control.
#[test]
fn a_wire_boxes_its_arc_interval_only() {
    let r45 = 50.0 / 2f64.sqrt();
    let c = quarter_circle();
    assert_wire(&c, 0.0, 0.5, [r45, 0.0, 0.0, 50.0, r45, 0.0]);
    assert_wire(&c, 0.0, 1.0, [0.0, 0.0, 0.0, 50.0, 50.0, 0.0]);
}

/// `whole_body_box_keeps_an_interior_peak_but_not_outside_knots`, and the knots on the
/// interval's boundary.
#[test]
fn a_wire_keeps_an_interior_kink_and_not_the_outside_knots() {
    let c = degree_one_peaks();
    assert_wire(&c, 2.5, 3.5, [2.5, 2.0, 0.0, 3.5, 4.0, 0.0]);
    assert_wire(&c, 0.0, 6.0, [0.0, 0.0, 0.0, 6.0, 20.0, 0.0]);
    assert_wire(&c, 2.0, 4.0, [2.0, 0.0, 0.0, 4.0, 4.0, 0.0]);
}

/// `sphere_cap_seam_box_stays_on_the_cap`: a quadratic rational arc in the xz-plane over polar
/// angle -30 to 60 degrees, cut at its pole: the first 30 degrees of the cap.
#[test]
fn a_seam_arc_stops_at_its_pole() {
    // Radius 50, from angle -30 deg (25, 0, 43.30) over the pole to 60 deg (-43.30, 0, 25):
    // three spans of the arc by its tangent-intersection control points.
    let r = 50.0;
    let (a0, a1) = ((-30f64).to_radians(), 60f64.to_radians());
    let mid = 0.5 * (a0 + a1);
    let half = 0.5 * (a1 - a0);
    let pt_at = |a: f64, k: f64| [r * k * fmath::sin(a), 0.0, r * k * fmath::cos(a)];
    // x = r sin(a), z = r cos(a): the angle a runs -30 to 60 degrees, so x falls from -25 to
    // 43.3; flip x so the pole (a = 0) is where x = 0 and x falls along the curve.
    let flip = |p: [f64; 3]| [-p[0], p[1], p[2]];
    let c = nurbs_curve(
        2,
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        &[
            flip(pt_at(a0, 1.0)),
            flip(pt_at(mid, 1.0 / fmath::cos(half))),
            flip(pt_at(a1, 1.0)),
        ],
        Some(vec![1.0, fmath::cos(half), 1.0]),
    );
    // The pole is at the parameter whose angle is 0: the rational arc's angle is not linear
    // in the parameter, so find it on the curve itself.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..200 {
        let m = 0.5 * (lo + hi);
        if c.point(m).x > 0.0 {
            lo = m;
        } else {
            hi = m;
        }
    }
    let pole = 0.5 * (lo + hi);
    let z30 = r * fmath::cos(30f64.to_radians());
    // The first 30 degrees: from (25, 0, 43.30) to the pole (0, 0, 50).
    assert_wire(&c, 0.0, pole, [0.0, 0.0, z30, 25.0, 0.0, 50.0]);
}

/// The parameter of the quarter circle at height `y`, found on the curve.
fn parameter_at_y(c: &Curve, y: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..200 {
        let m = 0.5 * (lo + hi);
        if c.point(m).y < y {
            lo = m;
        } else {
            hi = m;
        }
    }
    0.5 * (lo + hi)
}

/// `interval_knots_tighten_a_rational_arc_subdomain`: to height 25 the arc is at x from
/// sqrt(50^2 - 25^2) to 50.
#[test]
fn a_rational_arc_subdomain_is_tight() {
    let c = quarter_circle();
    let t = parameter_at_y(&c, 25.0);
    let x = (50f64 * 50.0 - 25.0 * 25.0).sqrt();
    assert_wire(&c, 0.0, t, [x, 0.0, 0.0, 50.0, 25.0, 0.0]);
}

// ---- issue_1799: a trimmed face's box is the face's, not the surface's ---------------------

/// The arc's knots: three quarter spans of a rational quadratic, double knots at 1/3, 2/3.
fn arc_knots() -> Vec<f64> {
    let (a, b) = (1.0 / 3.0, 2.0 / 3.0);
    vec![0.0, 0.0, 0.0, a, a, b, b, 1.0, 1.0, 1.0]
}

/// The arc's control points `(x, y)` and weights, angle 0 to 270 degrees.
fn arc_net() -> [(f64, f64, f64); 7] {
    let s = FRAC_1_SQRT_2;
    [
        (1.0, 0.0, 1.0),
        (1.0, 1.0, s),
        (0.0, 1.0, 1.0),
        (-1.0, 1.0, s),
        (-1.0, 0.0, 1.0),
        (-1.0, -1.0, s),
        (0.0, -1.0, 1.0),
    ]
}

fn arc_at(z: f64) -> Curve {
    let pts: Vec<[f64; 3]> = arc_net().iter().map(|&(x, y, _)| [x, y, z]).collect();
    let w = arc_net().iter().map(|&(_, _, w)| w).collect();
    nurbs_curve(2, &arc_knots(), &pts, Some(w))
}

/// The 270 degree cylinder of radius 1 about z, `z = v` over `[0, 1]`: three rational
/// quadratic spans along `u`, linear along `v`.
fn rational_cylinder() -> Surface {
    let pts: Vec<[f64; 3]> = arc_net()
        .iter()
        .flat_map(|&(x, y, _)| [[x, y, 0.0], [x, y, 1.0]])
        .collect();
    let w: Vec<f64> = arc_net().iter().flat_map(|&(_, _, w)| [w, w]).collect();
    Surface::Nurbs(Arc::new(
        NurbsSurface::<3>::new(
            KnotVector::new(2, arc_knots()).unwrap(),
            KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap(),
            Net::new(7, 2, pts).unwrap(),
            Some(Net::new(7, 2, w).unwrap()),
        )
        .unwrap(),
    ))
}

/// The rational cylinder trimmed to `0 <= z <= top`, one face of four iso-parametric edges.
fn trimmed_cylinder(top: f64) -> Brep {
    let mut b = B::new();
    let v00 = b.vertex([1.0, 0.0, 0.0]);
    let v10 = b.vertex([0.0, -1.0, 0.0]);
    let v11 = b.vertex([0.0, -1.0, top]);
    let v01 = b.vertex([1.0, 0.0, top]);
    let bottom = b.edge(arc_at(0.0), 0.0, 1.0, [v00, v10]);
    let right = b.line(v10, v11);
    let upper = b.edge(arc_at(top), 0.0, 1.0, [v01, v11]);
    let left = b.line(v00, v01);
    b.face(
        rational_cylinder(),
        vec![
            (
                bottom,
                Sense::Forward,
                line2([0.0, 0.0], [1.0, 0.0]),
                (0.0, 1.0),
            ),
            (
                right,
                Sense::Forward,
                line2([1.0, 0.0], [0.0, top]),
                (0.0, 1.0),
            ),
            (
                upper,
                Sense::Reversed,
                line2([0.0, top], [1.0, 0.0]),
                (0.0, 1.0),
            ),
            (
                left,
                Sense::Reversed,
                line2([0.0, 0.0], [0.0, top]),
                (0.0, 1.0),
            ),
        ],
    );
    finish(b.d)
}

/// `trimmed_rational_cylinder_box_is_exact_with_the_search`: the box is
/// (-1, -1, 0) to (1, 1, 0.3), judged to the zone.
#[test]
fn a_trimmed_rational_cylinder_is_boxed_exactly() {
    let body = trimmed_cylinder(0.3);
    let f = body.faces().next().unwrap();
    let got = tight(&body, Entity::Face(f.id()));
    assert_tight(
        "trimmed cylinder",
        &got,
        [-1.0, -1.0, 0.0, 1.0, 1.0, 0.3],
        3.0 * zone(&body),
    );
}

/// A quarter of a cylinder of radius 10 about z, -30 to 60 degrees, `z = 10 v` over `[0, 1]`:
/// its greatest x, 10, runs along a whole line of angle 0 inside it (#13753), which no
/// halving of `u` reaches. The whole domain is the face.
#[test]
fn a_ridge_along_a_face_is_boxed_to_the_ridge() {
    let (r, s) = (10.0, FRAC_1_SQRT_2);
    let at = |deg: f64, k: f64| {
        let a = deg.to_radians();
        [k * fmath::cos(a), k * fmath::sin(a)]
    };
    let net = [
        (at(-30.0, r), 1.0),
        (at(15.0, r / s), s),
        (at(60.0, r), 1.0),
    ];
    let pts: Vec<[f64; 3]> = net
        .iter()
        .flat_map(|&(p, _)| [[p[0], p[1], 0.0], [p[0], p[1], 10.0]])
        .collect();
    let w: Vec<f64> = net.iter().flat_map(|&(_, w)| [w, w]).collect();
    let knots_u = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let surface = Surface::Nurbs(Arc::new(
        NurbsSurface::<3>::new(
            KnotVector::new(2, knots_u.clone()).unwrap(),
            KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap(),
            Net::new(3, 2, pts).unwrap(),
            Some(Net::new(3, 2, w).unwrap()),
        )
        .unwrap(),
    ));
    let arc = |z: f64| {
        let p: Vec<[f64; 3]> = net.iter().map(|&(p, _)| [p[0], p[1], z]).collect();
        nurbs_curve(2, &knots_u, &p, Some(net.iter().map(|&(_, w)| w).collect()))
    };
    let corner = |z: f64, i: usize| {
        let p = net[i].0;
        [p[0], p[1], z]
    };
    let mut b = B::new();
    let v00 = b.vertex(corner(0.0, 0));
    let v10 = b.vertex(corner(0.0, 2));
    let v11 = b.vertex(corner(10.0, 2));
    let v01 = b.vertex(corner(10.0, 0));
    let bottom = b.edge(arc(0.0), 0.0, 1.0, [v00, v10]);
    let right = b.line(v10, v11);
    let upper = b.edge(arc(10.0), 0.0, 1.0, [v01, v11]);
    let left = b.line(v00, v01);
    b.face(
        surface,
        vec![
            (
                bottom,
                Sense::Forward,
                line2([0.0, 0.0], [1.0, 0.0]),
                (0.0, 1.0),
            ),
            (
                right,
                Sense::Forward,
                line2([1.0, 0.0], [0.0, 1.0]),
                (0.0, 1.0),
            ),
            (
                upper,
                Sense::Reversed,
                line2([0.0, 1.0], [1.0, 0.0]),
                (0.0, 1.0),
            ),
            (
                left,
                Sense::Reversed,
                line2([0.0, 0.0], [0.0, 1.0]),
                (0.0, 1.0),
            ),
        ],
    );
    let body = finish(b.d);
    let f = body.faces().next().unwrap();
    let got = tight(&body, Entity::Face(f.id()));
    assert_tight(
        "ridge",
        &got,
        [
            5.0,
            -5.0,
            0.0,
            10.0,
            10.0 * fmath::sin(60f64.to_radians()),
            10.0,
        ],
        3.0 * zone(&body),
    );
}

/// The ramp of `spans` knot spans: a rational quadratic along `u` (x rises from 0 to 10,
/// every other weight 0.8, z = 0.02 x^2 over the control points) and one linear span across
/// `v` (y from 0 to 1). Its z rises along u and does not change across v.
fn ramp(spans: usize) -> (NurbsSurface<3>, Vec<f64>, Vec<[f64; 3]>, Vec<f64>) {
    let n = spans + 2;
    let mut knots = vec![0.0; 3];
    knots.extend((1..spans).map(|i| i as f64 / spans as f64));
    knots.extend([1.0; 3]);
    let wt = |i: usize| if i % 2 == 1 { 0.8 } else { 1.0 };
    let xz = |i: usize| {
        let x = 10.0 * i as f64 / (n - 1) as f64;
        (x, 0.02 * x * x)
    };
    let pts: Vec<[f64; 3]> = (0..n)
        .flat_map(|i| {
            let (x, z) = xz(i);
            [[x, 0.0, z], [x, 1.0, z]]
        })
        .collect();
    let w: Vec<f64> = (0..n).flat_map(|i| [wt(i), wt(i)]).collect();
    let s = NurbsSurface::<3>::new(
        KnotVector::new(2, knots.clone()).unwrap(),
        KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap(),
        Net::new(n, 2, pts).unwrap(),
        Some(Net::new(n, 2, w.clone()).unwrap()),
    )
    .unwrap();
    let side = |y: f64| -> Vec<[f64; 3]> { (0..n).map(|i| [xz(i).0, y, xz(i).1]).collect() };
    let _ = side;
    (
        s,
        knots,
        (0..n).map(|i| [xz(i).0, 0.0, xz(i).1]).collect(),
        (0..n).map(wt).collect(),
    )
}

/// `a_wall_one_span_across_is_divided_only_along_its_spans`: the ramp trimmed to
/// `0.5 <= x <= 6.3` across all of y, its +z and -z extremes where the trim cuts it. The box is
/// the trimmed ramp's: z from its height at x = 0.5 to that at x = 6.3.
fn trimmed_ramp(spans: usize) -> (Brep, f64, f64) {
    let (s, knots, row, w) = ramp(spans);
    let surface = Surface::Nurbs(Arc::new(s));
    let at_x = |x: f64| {
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..200 {
            let m = 0.5 * (lo + hi);
            if surface.point(Point2::new(m, 0.5)).x < x {
                lo = m;
            } else {
                hi = m;
            }
        }
        0.5 * (lo + hi)
    };
    let (u0, u1) = (at_x(0.5), at_x(6.3));
    let side = |y: f64| {
        let p: Vec<[f64; 3]> = row.iter().map(|q| [q[0], y, q[2]]).collect();
        nurbs_curve(2, &knots, &p, Some(w.clone()))
    };
    let (bottom, top) = (side(0.0), side(1.0));
    let mut b = B::new();
    let v00 = b.vertex({
        let p = bottom.point(u0);
        [p.x, p.y, p.z]
    });
    let v10 = b.vertex({
        let p = bottom.point(u1);
        [p.x, p.y, p.z]
    });
    let v11 = b.vertex({
        let p = top.point(u1);
        [p.x, p.y, p.z]
    });
    let v01 = b.vertex({
        let p = top.point(u0);
        [p.x, p.y, p.z]
    });
    let eb = b.edge(bottom, u0, u1, [v00, v10]);
    let er = b.line(v10, v11);
    let et = b.edge(top, u0, u1, [v01, v11]);
    let el = b.line(v00, v01);
    b.face(
        surface.clone(),
        vec![
            (eb, Sense::Forward, line2([0.0, 0.0], [1.0, 0.0]), (u0, u1)),
            (er, Sense::Forward, line2([u1, 0.0], [0.0, 1.0]), (0.0, 1.0)),
            (et, Sense::Reversed, line2([0.0, 1.0], [1.0, 0.0]), (u0, u1)),
            (
                el,
                Sense::Reversed,
                line2([u0, 0.0], [0.0, 1.0]),
                (0.0, 1.0),
            ),
        ],
    );
    let z = |x: f64| surface.point(Point2::new(at_x(x), 0.5)).z;
    (finish(b.d), z(0.5), z(6.3))
}

#[test]
fn a_ramp_trimmed_across_its_spans_is_boxed_to_its_trims() {
    for spans in [16, 128] {
        let (body, z0, z1) = trimmed_ramp(spans);
        let f = body.faces().next().unwrap();
        let got = tight(&body, Entity::Face(f.id()));
        assert_tight(
            "ramp",
            &got,
            [0.5, 0.0, z0, 6.3, 1.0, z1],
            3.0 * zone(&body),
        );
    }
}

// ---- the memo ------------------------------------------------------------------------------

/// A warm call gives the same bits and charges the same work as the cold one, so a budget
/// stops at the same place; a budget below the work is `Exhausted` both ways.
#[test]
fn a_hit_equals_the_fill_and_charges_the_same_work() {
    let body = generated(Kind::Torus, 1, 3);
    let stop = Stop::new();
    let mut scratch = Scratch::new();
    let entities: Vec<Entity> = body.faces().map(|f| Entity::Face(f.id())).collect();
    let mut cold = Vec::new();
    let mut cold_work = Vec::new();
    for &e in &entities {
        let mut budget = Budget::unlimited();
        cold.push(tight_box_charged(&body, e, &mut scratch, &mut budget, &stop).unwrap());
        cold_work.push(budget.used());
    }
    assert!(!scratch.is_empty());
    let filled = scratch.len();
    for (i, &e) in entities.iter().enumerate() {
        let mut budget = Budget::unlimited();
        let warm = tight_box_charged(&body, e, &mut scratch, &mut budget, &stop).unwrap();
        assert_eq!(
            corners(&warm),
            corners(&cold[i]),
            "warm is cold, bit for bit"
        );
        assert_eq!(
            budget.used(),
            cold_work[i],
            "a hit charges the work recorded"
        );
        assert!(cold_work[i] > 0);
    }
    assert_eq!(scratch.len(), filled, "a hit fills nothing");
    // A fresh scratch gives the same work again.
    let mut fresh = Scratch::new();
    let mut budget = Budget::unlimited();
    tight_box_charged(&body, entities[0], &mut fresh, &mut budget, &stop).unwrap();
    assert_eq!(budget.used(), cold_work[0]);
    // The same limit stops cold and warm alike.
    let limit = cold_work[0] - 1;
    let mut cold_scratch = Scratch::new();
    let mut b1 = Budget::new(limit);
    let r1 = tight_box_charged(&body, entities[0], &mut cold_scratch, &mut b1, &stop);
    let mut b2 = Budget::new(limit);
    let r2 = tight_box_charged(&body, entities[0], &mut scratch, &mut b2, &stop);
    assert_eq!(r1, Err(BodyqError::Exhausted { limit }));
    assert_eq!(r2, Err(BodyqError::Exhausted { limit }));
    assert!(
        cold_scratch.is_empty() || cold_scratch.len() < filled,
        "an unfinished fill keeps nothing"
    );
}

/// The memo is keyed by what the answer reads: the same face of another body (other ids)
/// hits, and a changed curve does not.
#[test]
fn the_memo_follows_the_bits_not_the_ids() {
    let a = generated(Kind::Sphere, 0, 3);
    let b = generated(Kind::Sphere, 0, 3);
    let stop = Stop::new();
    let mut scratch = Scratch::new();
    let f = a.faces().next().unwrap().id();
    let mut b1 = Budget::unlimited();
    let x = tight_box_charged(&a, Entity::Face(f), &mut scratch, &mut b1, &stop).unwrap();
    let n = scratch.len();
    let mut b2 = Budget::unlimited();
    let y = tight_box_charged(&b, Entity::Face(f), &mut scratch, &mut b2, &stop).unwrap();
    assert_eq!(corners(&x), corners(&y));
    assert_eq!(scratch.len(), n, "the equal body's face hit");
    // Another seed is another sphere: new entries.
    let c = generated(Kind::Sphere, 1, 3);
    let mut b3 = Budget::unlimited();
    tight_box_charged(&c, Entity::Face(f), &mut scratch, &mut b3, &stop).unwrap();
    assert!(scratch.len() > n);
}

/// A shell's and a region's boxes are the hull of their faces', and an entity the body does not
/// have is refused.
#[test]
fn shells_regions_and_absent_entities() {
    let body = generated(Kind::Block, 0, 1);
    let whole = body_box(&body);
    for r in body.regions().filter(|r| r.is_solid()) {
        assert_eq!(
            corners(&tight(&body, Entity::Region(r.id()))),
            corners(&whole)
        );
    }
    let mut scratch = Scratch::new();
    let none = Entity::Face(scx_brep::FaceId::from_raw(9999));
    let r = with_cx(&mut scratch, &Settings::new(), |cx| {
        tight_box(&body, none, cx)
    });
    assert!(matches!(r, Err(BodyqError::Refused(_))), "{r:?}");
    let empty = finish(Draft::new());
    let r = with_cx(&mut scratch, &Settings::new(), |cx| {
        tight_body_box(&empty, cx)
    });
    assert!(matches!(r, Err(BodyqError::Refused(_))), "{r:?}");
}
