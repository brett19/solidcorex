// Written for solidcorex. Apache-2.0.
//! The exact pcurve writer (63 §6.11): written onto every generated body (BR8) and onto
//! plane-section bodies built by hand, each finished and judged against truth by dense
//! sampling; every refusal by its cause.
//!
//! The dense judge stands in for T6 (no pcurve-agreement reference exists in the testkit):
//! at 1000 parameters per use the pcurve lifts onto its edge's curve within the zone, it
//! never jumps a period between neighbours, and on a surface with poles it neither runs
//! along a pole's line nor passes a pole inside itself (T7's `jumps`, `dog_leg`,
//! `passes_pole`).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::redundant_closure_for_method_calls
)]

use core::f64::consts::{FRAC_PI_2, TAU};

use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, FaceId, FaceRecord, PcurveWriteError,
    RegionId, ShellId, ShellRecord, UseGeom, UvSide, VertexId, VertexRecord,
};
use scx_curve::{Circle, Curve, Curve2, Line};
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_ssi::analytic::{Intersection, section_by_plane};
use scx_surface::{Closed, Cone, Cylinder, Plane, Sphere, Surface};
use scx_testkit::bodies::{Kind, generate};
use scx_testkit::topology::{dog_leg, jumps, passes_pole};

const FWD: Sense = Sense::Forward;
const REV: Sense = Sense::Reversed;

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn span(lo: f64, hi: f64) -> Extent1d {
    Extent1d::new(lo, hi).unwrap()
}

fn doc() -> Document {
    Document::default()
}

/// The surface's poles' `v`s.
fn pole_vs(s: &Surface) -> Vec<f64> {
    match s {
        Surface::Sphere(s) => s.poles().iter().map(|p| p.v).collect(),
        Surface::Cone(c) => vec![c.pole().v],
        _ => vec![],
    }
}

/// The dense judge over every use of every face of `b` with a curve edge.
fn judge(b: &Brep) {
    let zone = doc().resolution().zone().mm();
    for f in b.faces() {
        let s = f.surface();
        let dom = s.domain();
        let periods = [dom.u.period(), dom.v.period()];
        for u in f.uses() {
            let EdgeShape::Curve { curve, .. } = u.edge().shape() else {
                continue;
            };
            let UseGeom::Pcurve {
                curve: pc,
                interval,
            } = u.geom()
            else {
                panic!("a curve edge's use is a point");
            };
            assert!(u.bound().is_none(), "an exact write records no bound");
            let (lo, hi) = (interval.min(), interval.max());
            let uv: Vec<[f64; 2]> = (0..=1000)
                .map(|k| {
                    let t = lo + (hi - lo) * (f64::from(k) / 1000.0);
                    let p = pc.point(t);
                    let miss = (s.point(p) - curve.point(t)).length();
                    assert!(miss <= zone, "face {}: lift misses by {miss}", f.id());
                    [p.x, p.y]
                })
                .collect();
            assert_eq!(jumps(&uv, periods), None, "face {}: period jump", f.id());
            for v in pole_vs(s) {
                assert_eq!(
                    dog_leg(&uv, v, 1e-9, 1e-9),
                    None,
                    "face {}: dog-leg",
                    f.id()
                );
                assert_eq!(passes_pole(&uv, v, 1e-9), None, "face {}", f.id());
            }
        }
    }
}

/// Whether two pcurves agree up to whole periods: their points over the interval differ
/// by one whole-period vector.
fn agree(a: &Curve2, b: &Curve2, iv: Extent1d, periods: [Option<f64>; 2]) -> bool {
    let d0 = a.point(iv.min()) - b.point(iv.min());
    let whole = |g: f64, p: Option<f64>| p.map_or(0.0, |p| (g / p).round() * p);
    let shift = Vec2::new(whole(d0.x, periods[0]), whole(d0.y, periods[1]));
    (0..=16).all(|k| {
        let t = iv.min() + (iv.max() - iv.min()) * (f64::from(k) / 16.0);
        let d = a.point(t) - b.point(t) - shift;
        d.x.abs().max(d.y.abs()) <= 1e-9
    })
}

/// `contains_uv` on the writer's pcurves: just left of each curve use's middle is inside
/// the face, just right is outside (the boundary tree BR4 builds from them).
fn contains_checks(b: &Brep) -> usize {
    let mut n = 0;
    for f in b.faces() {
        let Some(bx) = f.uv_box() else { continue };
        let (w, h) = (
            bx.x_interval().max() - bx.x_interval().min(),
            bx.y_interval().max() - bx.y_interval().min(),
        );
        let size = w.max(h);
        let dom = f.surface().domain();
        let wraps = [(dom.u.period(), w), (dom.v.period(), h)]
            .iter()
            .any(|&(p, x)| p.is_some_and(|p| x >= p * (1.0 - 1e-9)));
        for u in f.uses() {
            if !matches!(u.edge().shape(), EdgeShape::Curve { .. }) {
                continue;
            }
            let UseGeom::Pcurve {
                curve: pc,
                interval,
            } = u.geom()
            else {
                continue;
            };
            let (lo, hi) = (interval.min(), interval.max());
            let (m, h) = (0.5 * (lo + hi), 1e-3 * (hi - lo));
            let mut t = pc.point(m + h) - pc.point(m - h);
            if u.sense() == REV {
                t = -t;
            }
            let left = Vec2::new(-t.y, t.x) * (1e-3 * size / t.length());
            let p = pc.point(m);
            assert_eq!(
                f.contains_uv(p + left),
                Some(UvSide::Inside),
                "face {}",
                f.id()
            );
            if wraps {
                continue;
            }
            let out = f.contains_uv(p - left);
            // The far side of a face's edge is outside, unless the face wraps a whole
            // period: then it is the face again, in the next window (skipped above).
            assert!(
                matches!(out, Some(UvSide::Outside)),
                "face {} ({:?}) use {}: right of a use is {out:?}",
                f.id(),
                f.surface(),
                u.position()
            );
            n += 1;
        }
    }
    n
}

/// Every generated body: each face rewritten by the writer agrees with the generator's
/// pcurves up to whole periods (here exactly: the same rules), finishes, and passes the
/// dense judge and `contains_uv`.
#[test]
fn writes_every_generated_body() {
    let mut uses = 0;
    let mut checked = 0;
    for kind in Kind::ALL {
        for seed in 0..5 {
            for size in 1..=4 {
                let g = generate(kind, seed, size).unwrap();
                let mut d = g.draft.clone();
                let faces: Vec<FaceId> = d.faces().map(|f| f.id()).collect();
                for &f in &faces {
                    d.write_pcurves(f, &doc()).unwrap();
                }
                for &f in &faces {
                    let (was, now) = (g.draft.face(f).unwrap(), d.face(f).unwrap());
                    let dom = now.surface().domain();
                    let periods = [dom.u.period(), dom.v.period()];
                    for (a, b) in was.uses().zip(now.uses()) {
                        uses += 1;
                        match (a.geom(), b.geom()) {
                            (
                                UseGeom::Pcurve { curve: x, interval },
                                UseGeom::Pcurve { curve: y, .. },
                            ) => assert!(
                                x == y || agree(x, y, *interval, periods),
                                "{} seed {seed} size {size} face {f}",
                                kind.name()
                            ),
                            (x, y) => assert_eq!(x, y),
                        }
                    }
                }
                let (b, _) = d
                    .finish(&doc())
                    .unwrap_or_else(|e| panic!("{} {seed} {size}: {}", kind.name(), e.cause));
                judge(&b);
                if seed < 2 {
                    checked += contains_checks(&b);
                }
            }
        }
    }
    assert!(
        uses > 1000 && checked > 100,
        "{uses} uses, {checked} contains checks"
    );
}

/// A draft with the outside's shell and a solid's.
fn solid() -> (Draft, [ShellId; 2]) {
    let mut d = Draft::new();
    let outer = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let r = d.add_region(true).unwrap();
    let inner = d.add_shell(ShellRecord::new(r)).unwrap();
    (d, [outer, inner])
}

fn vertex(d: &mut Draft, p: Point3) -> VertexId {
    d.add_vertex(VertexRecord::new(p)).unwrap()
}

fn edge(d: &mut Draft, c: Curve, iv: Extent1d, ends: [VertexId; 2]) -> EdgeId {
    d.add_edge(EdgeGeom::new(
        EdgeShape::Curve {
            curve: c,
            interval: iv,
        },
        ends,
    ))
    .unwrap()
}

fn face(
    d: &mut Draft,
    s: Surface,
    sides: [ShellId; 2],
    loops: &[Vec<(EdgeId, Sense)>],
) -> Result<FaceId, PcurveWriteError> {
    let records = d.exact_loops(&s, loops, &doc())?;
    Ok(d.add_face(FaceRecord::new(s, sides, records)).unwrap())
}

/// The one curve of a plane section from SS5's `section_by_plane`.
fn section(s: &Surface, plane: &Plane) -> (Curve, Extent1d) {
    let big = Extent2d::from_intervals(span(-100.0, 100.0), span(-100.0, 100.0));
    let se = if let Surface::Plane(_) = s {
        big
    } else {
        let d = s.domain();
        let u =
            d.u.period()
                .map_or_else(|| span(-100.0, 100.0), |p| span(0.0, p));
        let v = match d.v.period() {
            Some(p) => span(0.0, p),
            None => match s {
                Surface::Sphere(_) => span(0.0, 1.0),
                _ => span(-100.0, 100.0),
            },
        };
        Extent2d::from_intervals(u, v)
    };
    let Closed::Solved(Intersection::Pieces { curves, .. }) =
        section_by_plane(s, &se, plane, &big, Resolution::new(mm(1e-9)).unwrap())
    else {
        panic!("no section");
    };
    assert_eq!(curves.len(), 1);
    let c = curves.into_iter().next().unwrap();
    (c.curve, c.span)
}

/// Whether `c` runs counter-clockwise about `z` through `o` at `t`.
fn ccw(c: &Curve, t: f64, o: Point3, z: Vec3) -> bool {
    let (p, q) = (c.point(t), c.point(t + 1e-4));
    (p - o).cross(q - p).dot(z) > 0.0
}

/// A cylinder cut by the plane `z = 0` below and an oblique plane above: the top edge is
/// SS5's ellipse, whose pcurve on the wall is an `AngleWave2` and on the cap an `Ellipse2`.
fn oblique_cylinder(frame: Frame, r: f64, h: f64, k: f64) -> Draft {
    let (mut d, sides) = solid();
    let (o, x, z) = (frame.origin(), *frame.x(), *frame.z());
    let cyl = Surface::Cylinder(Cylinder::new(frame, mm(r), FWD).unwrap());
    let top = Plane::new(Frame::new(o + z * h, z - x * k, x).unwrap());
    let (ell, iv) = section(&cyl, &top);
    // The wall's loop runs the top edge clockwise about `z`, the cap's counter-clockwise.
    let up = ccw(&ell, iv.min(), o, z);
    let (wall_top, cap_top) = if up { (REV, FWD) } else { (FWD, REV) };
    let (a, b) = (iv.min(), iv.max());
    let pt = ell.point(a);
    let radial = {
        let w = pt - o;
        let w = w - z * w.dot(z);
        w * (1.0 / w.length())
    };
    let bot = o + radial * r;
    let (vb, vt) = (vertex(&mut d, bot), vertex(&mut d, pt));
    let circle = Curve::Circle(Circle::new(Frame::new(o, z, radial).unwrap(), mm(r), FWD).unwrap());
    let ec = edge(&mut d, circle, span(0.0, TAU), [vb, vb]);
    let et = edge(&mut d, ell, span(a, b), [vt, vt]);
    let seam = Curve::Line(Line::new(bot, pt - bot).unwrap());
    let es = edge(&mut d, seam, span(0.0, 1.0), [vb, vt]);
    face(
        &mut d,
        cyl,
        sides,
        &[vec![(ec, FWD), (es, FWD), (et, wall_top), (es, REV)]],
    )
    .unwrap();
    let bottom = Surface::Plane(Plane::new(Frame::new(o, -z, radial).unwrap()));
    face(&mut d, bottom, sides, &[vec![(ec, REV)]]).unwrap();
    face(&mut d, Surface::Plane(top), sides, &[vec![(et, cap_top)]]).unwrap();
    d
}

fn frames() -> Vec<Frame> {
    vec![
        Frame::WORLD,
        Frame::new(
            Point3::new(1.0, -2.0, 0.5),
            Vec3::new(0.3, 0.4, 1.0),
            Vec3::new(1.0, 0.0, -0.2),
        )
        .unwrap(),
        Frame::new(
            Point3::new(-3.0, 4.0, 2.0),
            Vec3::new(-1.0, 0.2, 0.1),
            Vec3::new(0.0, 1.0, 0.3),
        )
        .unwrap(),
    ]
}

#[test]
fn writes_an_oblique_cylinder_section() {
    for frame in frames() {
        for (r, h, k) in [(1.0, 3.0, 0.5), (2.5, 4.0, -1.2), (0.7, 1.0, 0.0)] {
            let d = oblique_cylinder(frame, r, h, k);
            let wall = d.faces().next().unwrap();
            let kinds: Vec<&str> = wall
                .uses()
                .map(|u| match u.geom() {
                    UseGeom::Pcurve {
                        curve: Curve2::AngleWave(_),
                        ..
                    } => "wave",
                    UseGeom::Pcurve {
                        curve: Curve2::AngleLine(_),
                        ..
                    } => "angle",
                    UseGeom::Pcurve {
                        curve: Curve2::Line(_),
                        ..
                    } => "line",
                    _ => "other",
                })
                .collect();
            // A level top (k = 0) is a parallel: SS5 gives a circle, an `AngleLine2`.
            let top = if k == 0.0 { "angle" } else { "wave" };
            assert_eq!(kinds, ["angle", "line", top, "line"]);
            let (b, _) = d.finish(&doc()).unwrap_or_else(|e| panic!("{}", e.cause));
            judge(&b);
            contains_checks_wall_free(&b);
        }
    }
}

/// `contains_uv` on the caps only: the wall wraps a whole period, so the far side of its
/// seam is the wall again.
fn contains_checks_wall_free(b: &Brep) {
    for f in b
        .faces()
        .filter(|f| matches!(f.surface(), Surface::Plane(_)))
    {
        let bx = f.uv_box().unwrap();
        let c = Point2::new(
            0.5 * (bx.x_interval().min() + bx.x_interval().max()),
            0.5 * (bx.y_interval().min() + bx.y_interval().max()),
        );
        assert_eq!(f.contains_uv(c), Some(UvSide::Inside));
        let far = Point2::new(bx.x_interval().max() + 1.0, c.y);
        assert_eq!(f.contains_uv(far), Some(UvSide::Outside));
    }
}

/// A spherical cap above the plane `z = c`: SS5's parallel, a seam meridian up to the
/// north pole, the pole's collapsed edge (a whole-turn segment) and the seam back down;
/// and the planar disk under it.
fn cap(frame: Frame, r: f64, c: f64) -> Draft {
    let (mut d, sides) = solid();
    let (o, z) = (frame.origin(), *frame.z());
    let sphere = Surface::Sphere(Sphere::new(frame, mm(r), FWD).unwrap());
    let cut = Plane::new(Frame::new(o + z * c, z, *frame.x()).unwrap());
    let (par, iv) = section(&sphere, &cut);
    let up = ccw(&par, iv.min(), o, z);
    let (cap_par, disk_par) = if up { (FWD, REV) } else { (REV, FWD) };
    let (a, b) = (iv.min(), iv.max());
    let pa = par.point(a);
    let w = pa - o - z * c;
    let radial = w * (1.0 / w.length());
    let lat = fmath::asin(c / r);
    let north = o + z * r;
    let (va, vn) = (vertex(&mut d, pa), vertex(&mut d, north));
    let ep = edge(&mut d, par, span(a, b), [va, va]);
    // The meridian through `pa`: t the latitude, x radial, y up.
    let mer = Curve::Circle(
        Circle::new(Frame::new(o, radial.cross(z), radial).unwrap(), mm(r), FWD).unwrap(),
    );
    let em = edge(&mut d, mer, span(lat, FRAC_PI_2), [va, vn]);
    let en = d
        .add_edge(EdgeGeom::new(EdgeShape::Collapsed, [vn, vn]))
        .unwrap();
    face(
        &mut d,
        sphere,
        sides,
        &[vec![(ep, cap_par), (em, FWD), (en, FWD), (em, REV)]],
    )
    .unwrap();
    let disk = Surface::Plane(Plane::new(Frame::new(o + z * c, -z, radial).unwrap()));
    face(&mut d, disk, sides, &[vec![(ep, disk_par)]]).unwrap();
    d
}

#[test]
fn writes_a_spherical_cap_with_a_whole_turn_pole() {
    for frame in frames() {
        for (r, c) in [(1.0, 0.3), (2.0, -1.5), (0.5, 0.0)] {
            let d = cap(frame, r, c);
            let s = d.faces().next().unwrap();
            let period = s.surface().domain().u.period().unwrap();
            let pole = s.uses().nth(2).unwrap();
            let UseGeom::Pcurve {
                curve: Curve2::Line(l),
                ..
            } = pole.geom()
            else {
                panic!("pole segment");
            };
            // The face lies below the north pole: the segment runs a whole turn toward -u.
            assert!((l.direction().x + period).abs() <= 1e-12 * period);
            assert_eq!(l.direction().y, 0.0);
            let (b, _) = d.finish(&doc()).unwrap_or_else(|e| panic!("{}", e.cause));
            judge(&b);
            contains_checks_wall_free(&b);
        }
    }
}

/// A pole edge used in reverse gets its segment reversed: the same segment, run the
/// other way over `[0, 1]`.
#[test]
fn a_reversed_pole_use_runs_its_segment_backward() {
    let g = generate(Kind::Sphere, 3, 3).unwrap();
    let mut d = g.draft.clone();
    let f = d.faces().next().unwrap().id();
    let fwd = d.face(f).unwrap().record().clone();
    let loops: Vec<Vec<(EdgeId, Sense)>> = fwd
        .loops
        .iter()
        .map(|l| {
            l.uses
                .iter()
                .map(|u| {
                    let collapsed =
                        matches!(d.edge(u.edge).unwrap().shape(), Some(EdgeShape::Collapsed));
                    (u.edge, if collapsed { REV } else { u.sense })
                })
                .collect()
        })
        .collect();
    let rev = d.exact_loops(&fwd.surface, &loops, &doc()).unwrap();
    for (a, b) in fwd.loops[0].uses.iter().zip(&rev[0].uses) {
        let (UseGeom::Pcurve { curve: x, .. }, UseGeom::Pcurve { curve: y, .. }) =
            (&a.geom, &b.geom)
        else {
            panic!()
        };
        if a.sense == b.sense {
            assert_eq!(x, y);
        } else {
            let d0 = x.point(0.0) - y.point(1.0);
            let d1 = x.point(1.0) - y.point(0.0);
            assert!(d0.length().max(d1.length()) <= 1e-12, "{x:?} {y:?}");
        }
    }
    // Written into the face (its fins re-sensed with it), the body still finishes.
    d.edit_face(f, |r| r.loops = rev).unwrap();
    let (b, _) = d.finish(&doc()).unwrap_or_else(|e| panic!("{}", e.cause));
    judge(&b);
}

/// What the writer refuses, by cause.
#[test]
fn refusals_by_cause() {
    let f = frames()[1];
    let (o, x, y, z) = (f.origin(), *f.x(), *f.y(), *f.z());
    let sphere = Surface::Sphere(Sphere::new(f, mm(1.0), FWD).unwrap());
    let cone = Surface::Cone(Cone::new(f, mm(1.0), 0.4, FWD).unwrap());
    let (mut d, _) = solid();
    let doc = doc();
    let err =
        |d: &Draft, s: &Surface, l: Vec<(EdgeId, Sense)>| d.exact_loops(s, &[l], &doc).unwrap_err();

    // An oblique section of a sphere is a circle with no closed-form pcurve there.
    let tilted = Plane::new(Frame::new(o + z * 0.2, z + x * 0.7, y).unwrap());
    let (c, iv) = section(&sphere, &tilted);
    let v = vertex(&mut d, c.point(iv.min()));
    let e = edge(&mut d, c, iv, [v, v]);
    let got = err(&d, &sphere, vec![(e, FWD)]);
    assert_eq!(got, PcurveWriteError::NeedsFit { edge: e });
    assert_eq!(
        (got.kind(), got.name()),
        (ErrorKind::Unsupported, "pcurve_needs_fit")
    );

    // So is an oblique section of a cone: SS5 gives an ellipse.
    let tilted = Plane::new(Frame::new(o + z * 2.0, z + x * 0.3, y).unwrap());
    let (c, iv) = section(&cone, &tilted);
    assert!(matches!(c, Curve::Ellipse(_)));
    let v = vertex(&mut d, c.point(iv.min()));
    let e = edge(&mut d, c, iv, [v, v]);
    assert_eq!(
        err(&d, &cone, vec![(e, FWD)]),
        PcurveWriteError::NeedsFit { edge: e }
    );

    // A circle off the sphere is refused, not fitted.
    let off = Curve::Circle(Circle::new(Frame::new(o, z, x).unwrap(), mm(1.5), FWD).unwrap());
    let v = vertex(&mut d, off.point(0.0));
    let e = edge(&mut d, off, span(0.0, TAU), [v, v]);
    let got = err(&d, &sphere, vec![(e, FWD)]);
    assert!(matches!(got, PcurveWriteError::OffSurface { edge, .. } if edge == e));
    assert_eq!(
        (got.kind(), got.name()),
        (ErrorKind::Refused, "pcurve_off_surface")
    );

    // An edge that leaves the plane is refused even when its first sample past the
    // tolerance is within the edge's reach: the split takes the largest miss (FU1). Here
    // the first miss past half the zone is 0.75 zone, the last 6 zones.
    let zone = doc.resolution().zone().mm();
    let plane = Surface::Plane(Plane::new(f));
    let leaving = Curve::Line(Line::new(o, x + z * (6.0 * zone)).unwrap());
    let (a, b) = (
        vertex(&mut d, leaving.point(0.0)),
        vertex(&mut d, leaving.point(1.0)),
    );
    let e = edge(&mut d, leaving, span(0.0, 1.0), [a, b]);
    let got = err(&d, &plane, vec![(e, FWD)]);
    assert!(
        matches!(got, PcurveWriteError::OffSurface { edge, at, miss }
            if edge == e && at == 1.0 && miss > 5.0 * zone),
        "{got:?}"
    );

    // A meridian across the north pole: split it there.
    let mer = Curve::Circle(Circle::new(Frame::new(o, -y, x).unwrap(), mm(1.0), FWD).unwrap());
    let (a, b) = (
        vertex(&mut d, mer.point(1.0)),
        vertex(&mut d, mer.point(2.0)),
    );
    let e = edge(&mut d, mer.clone(), span(1.0, 2.0), [a, b]);
    let got = err(&d, &sphere, vec![(e, FWD)]);
    assert!(matches!(got, PcurveWriteError::CrossesPole { edge, .. } if edge == e));

    // A parallel alone winds round the sphere: it needs a seam.
    let par = Curve::Circle(
        Circle::new(
            Frame::new(o + z * 0.5, z, x).unwrap(),
            mm(0.75f64.sqrt()),
            FWD,
        )
        .unwrap(),
    );
    let v = vertex(&mut d, par.point(0.0));
    let e = edge(&mut d, par, span(0.0, TAU), [v, v]);
    assert_eq!(
        err(&d, &sphere, vec![(e, FWD)]),
        PcurveWriteError::Winds { lp: 0 }
    );

    // A lune with no pole edges meets itself across the pole's line.
    let (s, n) = (vertex(&mut d, o - z), vertex(&mut d, o + z));
    let m0 = edge(&mut d, mer_at(f, 0.3), span(-FRAC_PI_2, FRAC_PI_2), [s, n]);
    let m1 = edge(&mut d, mer_at(f, 1.1), span(-FRAC_PI_2, FRAC_PI_2), [s, n]);
    let got = err(&d, &sphere, vec![(m1, FWD), (m0, REV)]);
    assert_eq!(got, PcurveWriteError::PoleNeedsEdge { lp: 0, at: 1 });
    assert_eq!(got.name(), "pcurve_pole_needs_edge");

    // Two edges that do not meet.
    let l0 = edge(
        &mut d,
        Curve::Line(Line::new(o, x).unwrap()),
        span(0.0, 1.0),
        [s, n],
    );
    let l1 = edge(
        &mut d,
        Curve::Line(Line::new(o + y, x).unwrap()),
        span(0.0, 1.0),
        [n, s],
    );
    let plane = Surface::Plane(Plane::new(f));
    let got = err(&d, &plane, vec![(l0, FWD), (l1, FWD)]);
    assert!(matches!(
        got,
        PcurveWriteError::LoopOpen { lp: 0, at: 1, .. }
    ));

    // A loop of a collapsed edge alone, and two in a row.
    let c0 = d
        .add_edge(EdgeGeom::new(EdgeShape::Collapsed, [n, n]))
        .unwrap();
    let c1 = d
        .add_edge(EdgeGeom::new(EdgeShape::Collapsed, [n, n]))
        .unwrap();
    assert_eq!(
        err(&d, &sphere, vec![(c0, FWD)]),
        PcurveWriteError::NoCurveUse { lp: 0 }
    );
    let got = err(
        &d,
        &sphere,
        vec![(m1, FWD), (c0, FWD), (c1, FWD), (m0, REV)],
    );
    assert_eq!(got, PcurveWriteError::CollapsedRun { lp: 0, at: 2 });

    // A pending edge, a missing edge and a missing face.
    let p = d.add_edge_pending([s, n]).unwrap();
    assert_eq!(
        err(&d, &sphere, vec![(p, FWD)]),
        PcurveWriteError::Pending { edge: p }
    );
    let gone = EdgeId::from_raw(10_000);
    assert_eq!(err(&d, &sphere, vec![(gone, FWD)]).name(), "absent");
    assert_eq!(
        d.write_pcurves(FaceId::from_raw(10_000), &doc)
            .unwrap_err()
            .name(),
        "absent"
    );
}

/// The meridian of `f`'s unit sphere at azimuth `a`, from the south pole at `-pi/2`.
fn mer_at(f: Frame, a: f64) -> Curve {
    let (x, y, z) = (*f.x(), *f.y(), *f.z());
    let radial = x * fmath::cos(a) + y * fmath::sin(a);
    Curve::Circle(
        Circle::new(
            Frame::new(f.origin(), radial.cross(z), radial).unwrap(),
            mm(1.0),
            FWD,
        )
        .unwrap(),
    )
}

/// An ellipse on a plane keeps its kind: an `Ellipse2`.
#[test]
fn a_plane_cap_of_a_section_is_an_ellipse() {
    let d = oblique_cylinder(Frame::WORLD, 1.0, 2.0, 0.6);
    let top = d.faces().nth(2).unwrap();
    assert!(matches!(
        top.uses().next().unwrap().geom(),
        UseGeom::Pcurve {
            curve: Curve2::Ellipse(_),
            ..
        }
    ));
}

/// A failed write leaves the face as it was.
#[test]
fn a_refused_write_changes_nothing() {
    let g = generate(Kind::Cylinder, 2, 3).unwrap();
    let mut d = g.draft.clone();
    let f = d.faces().next().unwrap().id();
    let before = d.face(f).unwrap().record().clone();
    // Give one of its edges a curve off its surface.
    let e = before.loops[0].uses[0].edge;
    let far = Curve::Line(Line::new(Point3::new(100.0, 0.0, 0.0), Vec3::X).unwrap());
    d.edge_mut(e).unwrap().shape = EdgeShape::Curve {
        curve: far,
        interval: span(0.0, 1.0),
    };
    assert!(d.write_pcurves(f, &doc()).is_err());
    assert_eq!(d.face(f).unwrap().record().loops, before.loops);
}
