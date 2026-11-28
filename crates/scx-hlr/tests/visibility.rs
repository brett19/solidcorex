// Written for solidcorex. Apache-2.0.
//! Visibility against hand-computed truth (research 74 §37, HL2): every span of every
//! drawn curve is checked at points inside it against a closed-form ray test (slabs for a
//! box, the quadratic for a cylinder), never against the pin's output.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::similar_names,
    clippy::many_single_char_names
)]

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, EdgeShape};
use scx_hlr::{
    Drawing, DrawnLine, HlrError, Pending, Scene, SilhouetteCurve, Source, View, Visibility,
    prepare,
};
use scx_num::budget::Stop;
use scx_num::error::ErrorKind;
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::{block, cone, cylinder, sphere, torus};

// ---- helpers -------------------------------------------------------------------------------

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn at(o: Point3) -> Frame {
    Frame::new(o, v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)).unwrap()
}

fn view(d: Vec3, up: Vec3) -> View {
    View::new(d, up).unwrap()
}

/// An axis-aligned box from `lo` to `hi`, built at the origin and placed by a frame.
fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> (Brep, Frame) {
    let doc = Document::default();
    let made = block(
        &Frame::WORLD,
        [mm(hi[0] - lo[0]), mm(hi[1] - lo[1]), mm(hi[2] - lo[2])],
        &doc,
    )
    .unwrap();
    (made.body, at(p(lo[0], lo[1], lo[2])))
}

fn draw_on(
    bodies: &[(&Brep, Frame)],
    view: &View,
    settings: &Settings,
    stop: &Stop,
) -> Result<Drawing, HlrError> {
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let mut cx = Cx::new(&doc, settings, &mut scratch, stop, &NoTrace);
    let scene: Scene<'_> = prepare(bodies, &mut cx)?;
    scene.hidden_lines(view, &mut cx)
}

fn draw(bodies: &[(&Brep, Frame)], view: &View) -> Drawing {
    draw_on(bodies, view, &Settings::new(), &Stop::new()).unwrap()
}

/// The world point of a drawn line at `t`.
fn point(bodies: &[(&Brep, Frame)], line: &DrawnLine, t: f64) -> Point3 {
    let (body, frame) = &bodies[line.body];
    let q = match &line.source {
        Source::Edge(e) => {
            let EdgeShape::Curve { curve, .. } = body.edge(*e).unwrap().shape() else {
                panic!("a drawn edge has a curve")
            };
            curve.point(t)
        }
        Source::Silhouette { curve, .. } => curve.point(t),
    };
    let o = frame.origin();
    o + *frame.x() * q.x + *frame.y() * q.y + *frame.z() * q.z
}

/// Whether the ray from `q` towards the eye (along `-d`) meets the closed box `[lo, hi]`
/// beyond `q` (slabs: the entry and exit parameters, an interval reaching past zero). A
/// ray that only touches the box (grazing an edge) meets it: a curve exactly behind an
/// occluder's edge is hidden, the closed-face convention.
fn box_hides(q: Point3, d: Vec3, lo: [f64; 3], hi: [f64; 3]) -> bool {
    let (qa, da) = ([q.x, q.y, q.z], [-d.x, -d.y, -d.z]);
    let (mut s0, mut s1) = (0.0f64, f64::INFINITY);
    for k in 0..3 {
        if da[k].abs() < 1e-300 {
            if qa[k] < lo[k] - 1e-9 || qa[k] > hi[k] + 1e-9 {
                return false;
            }
            continue;
        }
        let (a, b) = ((lo[k] - qa[k]) / da[k], (hi[k] - qa[k]) / da[k]);
        s0 = s0.max(a.min(b));
        s1 = s1.min(a.max(b));
    }
    s1 > 1e-7 && s1 - s0 > -1e-9
}

/// Whether the ray from `q` along `-d` meets the solid cylinder of radius `r` about the
/// vertical axis through `(cx, cy)`, `z` in `[z0, z1]`, beyond `q`.
fn cylinder_hides(q: Point3, d: Vec3, c: [f64; 2], r: f64, z: [f64; 2]) -> bool {
    let (dx, dy, dz) = (-d.x, -d.y, -d.z);
    let (ox, oy) = (q.x - c[0], q.y - c[1]);
    let a = dx * dx + dy * dy;
    let (mut s0, mut s1) = (0.0f64, f64::INFINITY);
    if a < 1e-300 {
        if fmath::hypot(ox, oy) > r {
            return false;
        }
    } else {
        let b = ox * dx + oy * dy;
        let cc = ox * ox + oy * oy - r * r;
        let disc = b * b - a * cc;
        if disc <= 0.0 {
            return false;
        }
        let root = disc.sqrt();
        s0 = s0.max((-b - root) / a);
        s1 = s1.min((-b + root) / a);
    }
    if dz.abs() > 1e-300 {
        let (a, b) = ((z[0] - q.z) / dz, (z[1] - q.z) / dz);
        s0 = s0.max(a.min(b));
        s1 = s1.min(a.max(b));
    } else if q.z < z[0] || q.z > z[1] {
        return false;
    }
    s1 > 1e-7 && s1 - s0 > 1e-7
}

/// Every span of every line agrees with `hides` at nine points inside it, every span is
/// decided, and a hidden span names a face of a body that hides it.
fn agrees(bodies: &[(&Brep, Frame)], d: &Drawing, sight: Vec3, hides: impl Fn(Point3) -> bool) {
    assert_eq!(d.exhausted, None);
    for line in &d.lines {
        for s in &line.spans {
            for k in 1..10 {
                let t = s.t.min() + s.t.length() * f64::from(k) / 10.0;
                let q = point(bodies, line, t);
                let want = hides(q);
                match s.visibility {
                    Visibility::Visible => {
                        assert!(
                            !want,
                            "{sight:?}: {:?} at {t} ({q:?}) is hidden: {line:?}",
                            line.source
                        );
                    }
                    Visibility::Hidden { .. } => {
                        assert!(
                            want,
                            "{sight:?}: {:?} at {t} ({q:?}) is seen: {line:?}",
                            line.source
                        );
                    }
                    other => panic!("{:?}: {other:?}", line.source),
                }
            }
        }
    }
}

/// The fraction of a line's length that is hidden.
fn hidden_length(line: &DrawnLine) -> f64 {
    line.spans
        .iter()
        .filter(|s| matches!(s.visibility, Visibility::Hidden { .. }))
        .map(|s| s.t.length())
        .sum()
}

fn edge_lines(d: &Drawing, body: usize) -> impl Iterator<Item = &DrawnLine> {
    d.lines
        .iter()
        .filter(move |l| l.body == body && matches!(l.source, Source::Edge(_)))
}

/// Oblique directions and the axes.
fn directions() -> Vec<Vec3> {
    vec![
        v(-1.0, -1.0, -1.0),
        v(1.0, -2.0, 0.5),
        v(-0.3, 0.7, -2.0),
        v(0.05, -0.02, 1.0),
        v(0.0, 1.0, 0.0),
        v(1.0, 0.0, 0.0),
    ]
}

fn up_for(d: Vec3) -> Vec3 {
    if d.cross(v(0.0, 0.0, 1.0)).length() > 0.1 * d.length() {
        v(0.0, 0.0, 1.0)
    } else {
        v(1.0, 0.0, 0.0)
    }
}

// ---- boxes ---------------------------------------------------------------------------------

#[test]
fn an_iso_cube_hides_the_three_back_edges() {
    let (b, f) = cuboid([0.0; 3], [1.0; 3]);
    let bodies = [(&b, f)];
    let d = draw(&bodies, &view(v(-1.0, -1.0, -1.0), v(0.0, 0.0, 1.0)));
    assert_eq!(edge_lines(&d, 0).count(), 12);
    assert!(d.lines.iter().all(|l| matches!(l.source, Source::Edge(_))));
    let mut hidden = Vec::new();
    for l in &d.lines {
        assert_eq!(
            l.spans.len(),
            1,
            "an edge of a lone cube is one span: {l:?}"
        );
        let mid = point(&bodies, l, l.spans[0].t.mid());
        if matches!(l.spans[0].visibility, Visibility::Hidden { .. }) {
            hidden.push(mid);
        }
    }
    // The three edges at the corner farthest from the eye, the origin.
    assert_eq!(hidden.len(), 3, "{hidden:?}");
    for m in hidden {
        let coords = [m.x, m.y, m.z];
        assert_eq!(coords.iter().filter(|c| **c == 0.0).count(), 2, "{m:?}");
    }
}

#[test]
fn a_box_in_front_hides_the_middle_of_an_edge() {
    let (back, fb) = cuboid([0.0; 3], [1.0; 3]);
    let (front, ff) = cuboid([0.5, -2.0, 0.25], [1.5, -1.0, 0.75]);
    let bodies = [(&back, fb), (&front, ff)];
    let d = draw(&bodies, &view(v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)));
    // The back box's edge from (1, 0, 0) to (1, 0, 1).
    let line = edge_lines(&d, 0)
        .find(|l| {
            let m = point(&bodies, l, l.spans[0].t.mid());
            let e = point(&bodies, l, l.spans.last().unwrap().t.max());
            m.x == 1.0 && m.y == 0.0 && (e.x, e.y) == (1.0, 0.0)
        })
        .unwrap();
    assert_eq!(line.spans.len(), 3, "{line:?}");
    let z = |t: f64| point(&bodies, line, t).z;
    let (lo, hi) = (z(line.spans[1].t.min()), z(line.spans[1].t.max()));
    let (lo, hi) = (lo.min(hi), lo.max(hi));
    assert!(
        (lo - 0.25).abs() < 1e-12 && (hi - 0.75).abs() < 1e-12,
        "{line:?}"
    );
    assert!(matches!(
        line.spans[1].visibility,
        Visibility::Hidden { body: 1, .. }
    ));
    assert_eq!(line.spans[0].visibility, Visibility::Visible);
    assert_eq!(line.spans[2].visibility, Visibility::Visible);
}

#[test]
fn stacked_and_occluding_boxes_agree_with_slabs_in_every_view() {
    let boxes = [
        ([0.0, 0.0, 0.0], [4.0, 3.0, 2.0]),
        ([1.0, 0.5, 2.0], [3.0, 2.5, 3.5]),
        ([-2.0, -3.0, 0.5], [1.5, -1.0, 1.5]),
        ([2.5, 4.0, -1.0], [3.5, 6.0, 4.0]),
    ];
    let built: Vec<(Brep, Frame)> = boxes.iter().map(|(lo, hi)| cuboid(*lo, *hi)).collect();
    let bodies: Vec<(&Brep, Frame)> = built.iter().map(|(b, f)| (b, *f)).collect();
    for dir in directions() {
        let d = draw(&bodies, &view(dir, up_for(dir)));
        let sight = dir / dir.length();
        agrees(&bodies, &d, sight, |q| {
            boxes.iter().any(|(lo, hi)| box_hides(q, sight, *lo, *hi))
        });
        assert!(d.lines.iter().any(|l| hidden_length(l) > 0.0), "{dir:?}");
    }
}

#[test]
fn a_tilted_placement_agrees_with_slabs_in_the_boxes_own_frame() {
    let doc = Document::default();
    let tilt = Frame::new(p(3.0, -1.0, 2.0), v(0.3, -0.5, 0.8), v(1.0, 0.2, 0.1)).unwrap();
    let a = block(&Frame::WORLD, [mm(2.0), mm(2.0), mm(2.0)], &doc)
        .unwrap()
        .body;
    let b = block(&Frame::WORLD, [mm(1.0), mm(3.0), mm(0.5)], &doc)
        .unwrap()
        .body;
    // The second box sits on the first's top, in the same tilted frame, offset.
    let on_top = Frame::from_axes(
        tilt.origin() + *tilt.x() * 0.5 + *tilt.y() * (-0.5) + *tilt.z() * 2.0,
        *tilt.x(),
        *tilt.z(),
    )
    .unwrap();
    let bodies = [(&a, tilt), (&b, on_top)];
    let local = |q: Point3, f: &Frame| {
        let r = q - f.origin();
        p(r.dot(*f.x()), r.dot(*f.y()), r.dot(*f.z()))
    };
    for dir in directions() {
        let d = draw(&bodies, &view(dir, up_for(dir)));
        let sight = dir / dir.length();
        agrees(&bodies, &d, sight, |q| {
            let s = v(
                sight.dot(*tilt.x()),
                sight.dot(*tilt.y()),
                sight.dot(*tilt.z()),
            );
            box_hides(local(q, &tilt), s, [0.0; 3], [2.0; 3])
                || box_hides(local(q, &on_top), s, [0.0; 3], [1.0, 3.0, 0.5])
        });
    }
}

#[test]
fn a_part_far_from_the_origin_draws_as_at_the_origin() {
    let dir = v(1.0, -2.0, 0.5);
    let w = view(dir, up_for(dir));
    let shape = |o: [f64; 3]| {
        [
            cuboid(o, [o[0] + 4.0, o[1] + 3.0, o[2] + 2.0]),
            cuboid(
                [o[0] + 1.0, o[1] - 2.0, o[2] + 0.5],
                [o[0] + 2.0, o[1] - 1.0, o[2] + 1.0],
            ),
        ]
    };
    let summary = |o: [f64; 3]| {
        let built = shape(o);
        let bodies: Vec<(&Brep, Frame)> = built.iter().map(|(b, f)| (b, *f)).collect();
        let d = draw(&bodies, &w);
        d.lines
            .iter()
            .map(|l| {
                let ends: Vec<(f64, bool)> = l
                    .spans
                    .iter()
                    .map(|s| {
                        (
                            s.t.length(),
                            matches!(s.visibility, Visibility::Hidden { .. }),
                        )
                    })
                    .collect();
                (l.source.clone(), ends)
            })
            .collect::<Vec<_>>()
    };
    let near = summary([0.0; 3]);
    let far = summary([1e6, -2e6, 5e5]);
    assert_eq!(near.len(), far.len());
    for ((sa, ea), (sb, eb)) in near.iter().zip(&far) {
        assert_eq!(sa, sb);
        assert_eq!(ea.len(), eb.len(), "{sa:?}: {ea:?} vs {eb:?}");
        for (a, b) in ea.iter().zip(eb) {
            assert_eq!(a.1, b.1);
            assert!((a.0 - b.0).abs() < 1e-6, "{sa:?}: {ea:?} vs {eb:?}");
        }
    }
}

#[test]
fn a_plate_just_above_a_box_in_a_large_scene_is_not_looked_through() {
    let (a, fa) = cuboid([0.0, 0.0, -10.0], [10.0, 10.0, 0.0]);
    let (b, fb) = cuboid([-5.0, -5.0, 0.005], [15.0, 15.0, 0.007]);
    let (c, fc) = cuboid([1e7, 0.0, 0.0], [1e7 + 1.0, 1.0, 1.0]);
    let bodies = [(&a, fa), (&b, fb), (&c, fc)];
    let d = draw(&bodies, &view(v(0.0, 0.0, -1.0), v(0.0, 1.0, 0.0)));
    for l in edge_lines(&d, 0) {
        for s in &l.spans {
            assert!(
                !matches!(s.visibility, Visibility::Visible),
                "{:?}: {s:?}",
                l.source
            );
        }
    }
}

#[test]
fn far_apart_bodies_keep_every_edge_of_the_near_one() {
    let (a, fa) = cuboid([0.0; 3], [5.0; 3]);
    let (b, fb) = cuboid([100_000.0; 3], [100_001.0; 3]);
    let bodies = [(&a, fa), (&b, fb)];
    let dir = v(-1.0, -0.7, -0.4);
    let d = draw(&bodies, &view(dir, v(0.0, 0.0, 1.0)));
    assert_eq!(edge_lines(&d, 0).count(), 12);
    let sight = dir / dir.length();
    agrees(&bodies, &d, sight, |q| {
        box_hides(q, sight, [0.0; 3], [5.0; 3])
            || box_hides(q, sight, [100_000.0; 3], [100_001.0; 3])
    });
}

#[test]
fn two_touching_boxes_draw_their_shared_edges_seen() {
    let (a, fa) = cuboid([0.0; 3], [10.0; 3]);
    let (b, fb) = cuboid([10.0, 0.0, 0.0], [20.0, 10.0, 10.0]);
    let bodies = [(&a, fa), (&b, fb)];
    let dir = v(-1.0, -0.7, -0.5);
    let d = draw(&bodies, &view(dir, v(0.0, 0.0, 1.0)));
    let sight = dir / dir.length();
    agrees(&bodies, &d, sight, |q| {
        box_hides(q, sight, [0.0; 3], [10.0; 3])
            || box_hides(q, sight, [10.0, 0.0, 0.0], [20.0, 10.0, 10.0])
    });
}

// ---- curved faces --------------------------------------------------------------------------

#[test]
fn a_cylinder_from_the_side_has_seen_rulings_and_half_hidden_rims() {
    let doc = Document::default();
    let c = cylinder(&Frame::WORLD, mm(2.0), mm(5.0), &doc)
        .unwrap()
        .body;
    let bodies = [(&c, Frame::WORLD)];
    // Slightly from above, so the top rim's back half is seen and the bottom's hidden.
    let dir = v(0.0, 1.0, -0.3);
    let d = draw(&bodies, &view(dir, v(0.0, 0.0, 1.0)));
    let rulings: Vec<&DrawnLine> = d
        .lines
        .iter()
        .filter(|l| {
            matches!(
                l.source,
                Source::Silhouette {
                    curve: SilhouetteCurve::Line(_),
                    ..
                }
            )
        })
        .collect();
    assert_eq!(rulings.len(), 2, "{d:#?}");
    for r in &rulings {
        assert!(
            r.spans.iter().all(|s| s.visibility == Visibility::Visible),
            "{r:?}"
        );
        // The ruling is at x = ±2, y = 0.
        let q = point(&bodies, r, r.spans[0].t.mid());
        assert!(
            (q.x.abs() - 2.0).abs() < 1e-12 && q.y.abs() < 1e-12,
            "{q:?}"
        );
    }
    let sight = dir / dir.length();
    agrees(&bodies, &d, sight, |q| {
        cylinder_hides(q, sight, [0.0, 0.0], 2.0, [0.0, 5.0])
    });
    // The bottom rim's back half (y > 0, seen through the side) is hidden; the top rim is
    // all seen.
    let rim = |z: f64| {
        edge_lines(&d, 0)
            .find(|l| {
                let q = point(&bodies, l, l.spans[0].t.mid());
                q.z == z && (fmath::hypot(q.x, q.y) - 2.0).abs() < 1e-9
            })
            .unwrap()
    };
    let bottom = rim(0.0);
    let total: f64 = bottom.spans.iter().map(|s| s.t.length()).sum();
    assert!(
        (hidden_length(bottom) / total - 0.5).abs() < 1e-9,
        "{bottom:?}"
    );
    assert_eq!(hidden_length(rim(5.0)), 0.0);
}

#[test]
fn a_box_behind_a_cylinder_agrees_with_the_closed_forms() {
    let doc = Document::default();
    let c = cylinder(&at(p(2.0, -3.0, 0.0)), mm(1.5), mm(4.0), &doc)
        .unwrap()
        .body;
    let (b, fb) = cuboid([0.0; 3], [4.0, 2.0, 3.0]);
    let bodies = [(&c, Frame::WORLD), (&b, fb)];
    for dir in [v(0.0, 1.0, 0.0), v(0.3, 1.0, -0.2), v(-0.5, 1.0, 0.4)] {
        let d = draw(&bodies, &view(dir, v(0.0, 0.0, 1.0)));
        let sight = dir / dir.length();
        agrees(&bodies, &d, sight, |q| {
            cylinder_hides(q, sight, [2.0, -3.0], 1.5, [0.0, 4.0])
                || box_hides(q, sight, [0.0; 3], [4.0, 2.0, 3.0])
        });
        assert!(
            edge_lines(&d, 1).any(|l| l
                .spans
                .iter()
                .any(|s| matches!(s.visibility, Visibility::Hidden { body: 0, .. }))),
            "{dir:?}"
        );
    }
}

#[test]
fn a_cylinder_along_its_axis_hides_nothing_of_its_own_and_hides_what_is_behind() {
    let doc = Document::default();
    let c = cylinder(&at(p(0.0, 0.0, 2.0)), mm(1.0), mm(1.0), &doc)
        .unwrap()
        .body;
    let (b, fb) = cuboid([-0.5, -3.0, 0.0], [0.5, 3.0, 1.0]);
    let bodies = [(&c, Frame::WORLD), (&b, fb)];
    let dir = v(0.0, 0.0, -1.0);
    let d = draw(&bodies, &view(dir, v(0.0, 1.0, 0.0)));
    let sight = dir / dir.length();
    // Seen along its axis, the cylinder's bottom rim lies behind its top cap.
    agrees(&bodies, &d, sight, |q| {
        cylinder_hides(q, sight, [0.0, 0.0], 1.0, [2.0, 3.0])
            || box_hides(q, sight, [-0.5, -3.0, 0.0], [0.5, 3.0, 1.0])
    });
}

#[test]
fn a_cone_and_a_sphere_hide_a_box_behind_them() {
    let doc = Document::default();
    let k = cone(&at(p(0.0, -3.0, 0.0)), [mm(1.0), mm(0.0)], mm(2.0), &doc)
        .unwrap()
        .body;
    let s = sphere(&at(p(3.0, -3.0, 1.0)), mm(1.0), &doc).unwrap().body;
    let (b, fb) = cuboid([-2.0, 0.0, 0.0], [5.0, 1.0, 2.0]);
    let bodies = [(&k, Frame::WORLD), (&s, Frame::WORLD), (&b, fb)];
    let dir = v(0.0, 1.0, 0.0);
    let d = draw(&bodies, &view(dir, v(0.0, 0.0, 1.0)));
    assert!(d.unsupported.is_empty());
    let sight = dir / dir.length();
    // Seen along y, the cone (radius 1 - z/2) and the sphere hide the box's front face
    // edges where their outlines cover them; nothing else hides anything.
    let cone_hides = |q: Point3| {
        let r = 1.0 - q.z / 2.0;
        (0.0..=2.0).contains(&q.z) && q.x.abs() < r && q.y > -3.0 + 1e-9
    };
    let sphere_hides = |q: Point3| fmath::hypot(q.x - 3.0, q.z - 1.0) < 1.0 && q.y > -3.0 + 1e-9;
    let own = |q: Point3| box_hides(q, sight, [-2.0, 0.0, 0.0], [5.0, 1.0, 2.0]);
    for line in edge_lines(&d, 2) {
        for s in &line.spans {
            for k in 1..10 {
                let t = s.t.min() + s.t.length() * f64::from(k) / 10.0;
                let q = point(&bodies, line, t);
                let want = own(q) || cone_hides(q) || sphere_hides(q);
                assert_eq!(
                    matches!(s.visibility, Visibility::Hidden { .. }),
                    want,
                    "{q:?}: {line:?}"
                );
            }
        }
    }
}

#[test]
fn a_torus_waits_on_step_five_where_its_contour_may_cross() {
    let doc = Document::default();
    let t = torus(&Frame::WORLD, mm(3.0), mm(1.0), &doc).unwrap().body;
    let (b, fb) = cuboid([-1.0, 5.0, -0.5], [1.0, 6.0, 0.5]);
    let bodies = [(&t, Frame::WORLD), (&b, fb)];
    let dir = v(0.2, 1.0, 0.5);
    let d = draw(&bodies, &view(dir, v(0.0, 0.0, 1.0)));
    // The torus's contour is drawn, waiting; whatever lies behind its box waits on it.
    assert!(d.lines.iter().any(|l| {
        l.body == 0
            && l.spans
                .iter()
                .all(|s| s.visibility == Visibility::Pending(Pending::TorusContour))
    }));
    assert!(
        d.lines
            .iter()
            .filter(|l| l.body == 1)
            .flat_map(|l| &l.spans)
            .all(|s| !matches!(s.visibility, Visibility::Uncertain))
    );
}

// ---- contracts -----------------------------------------------------------------------------

#[test]
fn drawing_twice_gives_the_same_bits() {
    let doc = Document::default();
    let c = cylinder(&at(p(1.0, -3.0, 0.0)), mm(1.5), mm(4.0), &doc)
        .unwrap()
        .body;
    let (b, fb) = cuboid([0.0; 3], [4.0, 2.0, 3.0]);
    let bodies = [(&c, Frame::WORLD), (&b, fb)];
    let w = view(v(0.3, 1.0, -0.2), v(0.0, 0.0, 1.0));
    let a = draw(&bodies, &w);
    let b2 = draw(&bodies, &w);
    let bits = |d: &Drawing| {
        d.lines
            .iter()
            .flat_map(|l| {
                l.spans
                    .iter()
                    .map(|s| (s.t.min().to_bits(), s.t.max().to_bits()))
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(a, b2);
    assert_eq!(bits(&a), bits(&b2));
}

#[test]
fn a_stopped_call_and_a_small_budget_say_so() {
    let (a, fa) = cuboid([0.0; 3], [4.0, 3.0, 2.0]);
    let (b, fb) = cuboid([1.0, -2.0, 0.5], [2.0, -1.0, 1.0]);
    let bodies = [(&a, fa), (&b, fb)];
    let w = view(v(1.0, -2.0, 0.5), v(0.0, 0.0, 1.0));
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        draw_on(&bodies, &w, &Settings::new(), &stop).unwrap_err(),
        HlrError::Stopped
    );
    // Every budget either refuses (nothing classified yet) or draws with the spans not
    // reached uncertain, and a budget large enough draws everything.
    let full = draw(&bodies, &w);
    let mut saw_partial = false;
    for limit in 0..4000 {
        match draw_on(
            &bodies,
            &w,
            &Settings::new().with_budget(limit),
            &Stop::new(),
        ) {
            Err(e) => assert_eq!(e.kind(), ErrorKind::Exhausted, "{limit}"),
            Ok(d) if d.exhausted.is_some() => {
                saw_partial = true;
                assert_eq!(d.lines.len(), full.lines.len());
                for (l, f) in d.lines.iter().zip(&full.lines) {
                    for s in &l.spans {
                        if s.visibility != Visibility::Uncertain {
                            // What is decided agrees with the full drawing.
                            let m = s.t.mid();
                            let g = f.spans.iter().find(|x| x.t.min() <= m && m <= x.t.max());
                            // A span hidden before the budget ran out names a face that
                            // hides it, not always the nearest.
                            let kind = |v: Visibility| match v {
                                Visibility::Hidden { .. } => None,
                                other => Some(other),
                            };
                            assert_eq!(
                                g.map(|x| kind(x.visibility)),
                                Some(kind(s.visibility)),
                                "{limit}"
                            );
                        }
                    }
                }
            }
            Ok(d) => {
                assert_eq!(d, full, "{limit}");
                break;
            }
        }
    }
    assert!(saw_partial);
}

#[test]
fn a_seam_is_not_drawn_and_a_sphere_draws_only_its_outline() {
    let doc = Document::default();
    let s = sphere(&Frame::WORLD, mm(2.0), &doc).unwrap().body;
    let bodies = [(&s, Frame::WORLD)];
    let d = draw(&bodies, &view(v(0.3, 1.0, -0.4), v(0.0, 0.0, 1.0)));
    let outlines: Vec<&DrawnLine> = d
        .lines
        .iter()
        .filter(|l| matches!(l.source, Source::Silhouette { .. }))
        .collect();
    assert_eq!(outlines.len(), 1, "{d:#?}");
    assert!(
        outlines[0]
            .spans
            .iter()
            .all(|s| s.visibility == Visibility::Visible)
    );
    // Any edge drawn (a sphere's seam or pole is not) is checked against the ball.
    let sight = v(0.3, 1.0, -0.4) / v(0.3, 1.0, -0.4).length();
    agrees(&bodies, &d, sight, |q| {
        // A point on the sphere is hidden when the ray towards the eye enters it.
        let r = q - p(0.0, 0.0, 0.0);
        r.dot(-sight) < -1e-9
    });
}
