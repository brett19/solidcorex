// Written for solidcorex. Apache-2.0.
//! The exact pcurve writer (`Draft::write_pcurves`, research 63 §6.11) against the pin's
//! exact pcurves (`solidcore::eval::pcurve::exact_pcurves`) on every generated body.
//!
//! Each face of each body is rewritten by the writer and finished; its surface and its
//! edges' curves are given to the pin as its own plain-data records, their frames read off
//! scx's geometry by evaluation at STEP parameters (and checked against the pin's formulas
//! before use). The pin's pcurves are in STEP's parameters (angles in radians), so each of
//! its points is mapped by the surface's `from_step` before the comparison. The pin works
//! each use on its own (a seam's two uses get one pcurve, nothing is closed into a loop),
//! so the two agree up to a whole period per use.
//!
//! An input is one curve use. It is exact when every sample agrees to the bit after the
//! period shift, within tolerance when within [`UV_TOL`] of the uv's size. Past it, each
//! side is judged against truth: its pcurve's lift must be on the edge's curve within the
//! zone. The pin off and scx on is a pin defect; scx off is a mismatch, and so is a
//! disagreement where both lift onto the curve (two placements of one edge cannot both be
//! the face's). A use the pin has no exact pcurve for is judged on scx's side alone. Prints
//! `difftest pcurve_write: <n> inputs, <e> exact, <w> within tolerance, <p> pin defects,
//! <m> mismatches, <d> nondeterministic`, then the pin's missing pcurves.

#![allow(clippy::expect_used, clippy::panic)]

use scx_brep::{Brep, Document, EdgeShape, FaceId, UseGeom};
use scx_curve::Curve;
use scx_num::fmath;
use scx_num::frame::Sense;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_surface::Surface;
use scx_testkit::bodies::{Kind, generate};
use solidcore::eval::pcurve::{EdgeUse, eval, exact_pcurves};
use solidcore::eval::{Curve as PinCurve, Frame as PinFrame, Surface as PinSurface};

/// Samples per use.
const SAMPLES: u32 = 32;
/// The agreement bound in uv, relative to the uv's size (and absolute below 1).
const UV_TOL: f64 = 1e-9;
/// The pin's tolerance for its lifting check.
const PIN_TOL: f64 = 1e-9;
/// How closely a converted record must reproduce scx's geometry.
const CONVERT_TOL: f64 = 1e-12;

type P3 = [f64; 3];

fn a3(p: Point3) -> P3 {
    [p.x, p.y, p.z]
}
fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: P3) -> f64 {
    dot(a, a).sqrt()
}
fn mid(a: P3, b: P3) -> P3 {
    mul(add(a, b), 0.5)
}

/// `uv` at STEP's `(u, v)` on `s`.
fn from_step(s: &Surface, p: Point2) -> Point2 {
    match s {
        Surface::Plane(x) => x.from_step(p),
        Surface::Cylinder(x) => x.from_step(p),
        Surface::Cone(x) => x.from_step(p),
        Surface::Sphere(x) => x.from_step(p),
        Surface::Torus(x) => x.from_step(p),
        _ => unreachable!("only the generated kinds"),
    }
}

/// The pin's point of `s` at STEP's `(u, v)`, by the formulas its records document.
fn pin_point(s: &PinSurface, u: f64, v: f64) -> P3 {
    let on = |f: &PinFrame, rho: f64, h: f64| {
        let z = cross(f.x_axis, f.y_axis);
        add(
            f.origin,
            add(
                mul(
                    add(mul(f.x_axis, fmath::cos(u)), mul(f.y_axis, fmath::sin(u))),
                    rho,
                ),
                mul(z, h),
            ),
        )
    };
    match s {
        PinSurface::Plane {
            origin,
            u_axis,
            v_axis,
        } => add(*origin, add(mul(*u_axis, u), mul(*v_axis, v))),
        PinSurface::Cylinder { frame, radius } => on(frame, *radius, v),
        PinSurface::Cone {
            frame,
            radius_at_origin,
            half_angle,
        } => on(frame, radius_at_origin + v * fmath::tan(*half_angle), v),
        PinSurface::Sphere { frame, radius } => {
            on(frame, radius * fmath::cos(v), radius * fmath::sin(v))
        }
        PinSurface::Torus {
            frame,
            major_radius,
            minor_radius,
            tube_start,
        } => {
            let w = v + tube_start;
            on(
                frame,
                major_radius + minor_radius * fmath::cos(w),
                minor_radius * fmath::sin(w),
            )
        }
        _ => unreachable!(),
    }
}

/// `s` as the pin's record, its frame read off `s` at STEP parameters.
fn pin_surface(s: &Surface) -> PinSurface {
    use std::f64::consts::{FRAC_PI_2, PI};
    let e = |u: f64, v: f64| a3(s.point(from_step(s, Point2::new(u, v))));
    // The frame of the circle through `e(0, v)`, `e(pi / 2, v)`, `e(pi, v)`.
    let ring = |v: f64| {
        let o = mid(e(0.0, v), e(PI, v));
        let r = len(sub(e(0.0, v), o));
        let x = mul(sub(e(0.0, v), o), 1.0 / r);
        let y = mul(sub(e(FRAC_PI_2, v), o), 1.0 / r);
        (o, r, x, y)
    };
    let out = match s {
        Surface::Plane(_) => PinSurface::Plane {
            origin: e(0.0, 0.0),
            u_axis: sub(e(1.0, 0.0), e(0.0, 0.0)),
            v_axis: sub(e(0.0, 1.0), e(0.0, 0.0)),
        },
        Surface::Cylinder(_) => {
            let (origin, radius, x_axis, y_axis) = ring(0.0);
            PinSurface::Cylinder {
                frame: PinFrame {
                    origin,
                    x_axis,
                    y_axis,
                },
                radius,
            }
        }
        Surface::Cone(_) => {
            let (o0, r0, x0, y0) = ring(0.0);
            let (o1, r1, x1, y1) = ring(1.0);
            let (x_axis, y_axis) = if r0 >= r1 { (x0, y0) } else { (x1, y1) };
            let z = cross(x_axis, y_axis);
            let h = dot(sub(o1, o0), z);
            PinSurface::Cone {
                frame: PinFrame {
                    origin: o0,
                    x_axis,
                    y_axis,
                },
                radius_at_origin: r0,
                half_angle: fmath::atan((r1 - r0) / h),
            }
        }
        Surface::Sphere(_) => {
            let (origin, radius, x_axis, y_axis) = ring(0.0);
            PinSurface::Sphere {
                frame: PinFrame {
                    origin,
                    x_axis,
                    y_axis,
                },
                radius,
            }
        }
        Surface::Torus(_) => {
            let (origin, outer, x_axis, y_axis) = ring(0.0);
            let inner = len(sub(e(0.0, PI), origin));
            PinSurface::Torus {
                frame: PinFrame {
                    origin,
                    x_axis,
                    y_axis,
                },
                major_radius: 0.5 * (outer + inner),
                minor_radius: 0.5 * (outer - inner),
                tube_start: 0.0,
            }
        }
        _ => unreachable!(),
    };
    for i in 0..=6 {
        for j in 0..=6 {
            let (u, v) = (-3.0 + f64::from(i), -1.4 + 0.4 * f64::from(j));
            let d = len(sub(pin_point(&out, u, v), e(u, v)));
            assert!(
                d <= CONVERT_TOL,
                "{s:?} as {out:?}: off by {d:e} at ({u}, {v})"
            );
        }
    }
    out
}

/// The pin's point of `c` at `t`, by the formulas its records document.
fn pin_curve_point(c: &PinCurve, t: f64) -> P3 {
    match c {
        PinCurve::Line { start, end } => add(*start, mul(sub(*end, *start), t)),
        PinCurve::Circle { frame, radius } => add(
            frame.origin,
            mul(
                add(
                    mul(frame.x_axis, fmath::cos(t)),
                    mul(frame.y_axis, fmath::sin(t)),
                ),
                *radius,
            ),
        ),
        PinCurve::Ellipse { frame, radii } => add(
            frame.origin,
            add(
                mul(frame.x_axis, radii[0] * fmath::cos(t)),
                mul(frame.y_axis, radii[1] * fmath::sin(t)),
            ),
        ),
        _ => unreachable!(),
    }
}

/// `c` as the pin's record on the same parameter, or `None` for a kind the generator does
/// not make.
fn pin_curve(c: &Curve) -> Option<PinCurve> {
    use std::f64::consts::{FRAC_PI_2, PI};
    let e = |t: f64| a3(c.point(t));
    let conic = || {
        let o = mid(e(0.0), e(PI));
        let (a, b) = (sub(e(0.0), o), sub(e(FRAC_PI_2), o));
        let (ra, rb) = (len(a), len(b));
        (o, ra, rb, mul(a, 1.0 / ra), mul(b, 1.0 / rb))
    };
    let out = match c {
        Curve::Line(_) => PinCurve::Line {
            start: e(0.0),
            end: e(1.0),
        },
        Curve::Circle(_) => {
            let (origin, r, _, x_axis, y_axis) = conic();
            PinCurve::Circle {
                frame: PinFrame {
                    origin,
                    x_axis,
                    y_axis,
                },
                radius: r,
            }
        }
        Curve::Ellipse(_) => {
            let (origin, ra, rb, x_axis, y_axis) = conic();
            PinCurve::Ellipse {
                frame: PinFrame {
                    origin,
                    x_axis,
                    y_axis,
                },
                radii: [ra, rb],
            }
        }
        _ => return None,
    };
    for k in -4..=4 {
        let t = 0.7 * f64::from(k);
        let d = len(sub(pin_curve_point(&out, t), e(t)));
        assert!(d <= CONVERT_TOL, "{c:?} as {out:?}: off by {d:e} at {t}");
    }
    Some(out)
}

#[derive(Default)]
struct Tally {
    inputs: usize,
    exact: usize,
    within: usize,
    defects: Vec<String>,
    mismatches: Vec<String>,
    missing: Vec<String>,
    nondeterministic: usize,
}

/// The largest distance from a pcurve's lift (`uv` per sample) to the curve.
fn miss(s: &Surface, c: &Curve, ts: &[f64], uv: &[Point2]) -> f64 {
    ts.iter()
        .zip(uv)
        .map(|(&t, &p)| (s.point(p) - c.point(t)).length())
        .fold(0.0, f64::max)
}

/// Every curve use of `face` against the pin's.
fn face(b: &Brep, id: FaceId, zone: f64, at: &str, t: &mut Tally) {
    let f = b.face(id).unwrap();
    let s = f.surface();
    let pin_s = pin_surface(s);
    let dom = s.domain();
    let periods = [dom.u.period(), dom.v.period()];
    // The curve uses, loop by loop, with the pin's record of each one's curve.
    let mut ours = Vec::new();
    for l in f.loops() {
        let mut lp = Vec::new();
        for u in l.uses() {
            let e = u.edge();
            let EdgeShape::Curve { curve, interval } = e.shape() else {
                continue;
            };
            let UseGeom::Pcurve { curve: pc, .. } = u.geom() else {
                panic!("{at}: a curve edge's use is a point");
            };
            let pin_c = pin_curve(curve).expect("a generated curve kind");
            lp.push((
                e.id(),
                curve.clone(),
                *interval,
                u.sense(),
                pc.clone(),
                pin_c,
            ));
        }
        ours.push(lp);
    }
    let edges: Vec<_> = ours.iter().flatten().map(|x| x.0).collect();
    let index = |e| u32::try_from(edges.iter().position(|&x| x == e).unwrap()).unwrap();
    let loops: Vec<Vec<EdgeUse<'_>>> = ours
        .iter()
        .map(|lp| {
            lp.iter()
                .map(|(e, _, iv, sense, _, pin_c)| EdgeUse {
                    edge: index(*e),
                    curve: pin_c,
                    interval: [iv.min(), iv.max()],
                    forward: *sense == Sense::Forward,
                    tolerance: 0.0,
                    given: None,
                })
                .collect()
        })
        .collect();
    let pin = exact_pcurves(&pin_s, &loops, PIN_TOL, None);
    for (w, lp) in ours.iter().enumerate() {
        for (i, (_, curve, iv, _, pc, _)) in lp.iter().enumerate() {
            t.inputs += 1;
            let ts: Vec<f64> = (0..=SAMPLES)
                .map(|k| iv.min() + (iv.max() - iv.min()) * f64::from(k) / f64::from(SAMPLES))
                .collect();
            let a: Vec<Point2> = ts.iter().map(|&x| pc.point(x)).collect();
            let a_miss = miss(s, curve, &ts, &a);
            let here = format!("{at} face {id} loop {w} use {i} ({curve:?})");
            let pin_pc = match &pin {
                Ok(p) => p[w][i].clone(),
                Err(e) => {
                    t.missing
                        .push(format!("{here}: pin refused the face: {e:?}"));
                    None
                }
            };
            let Some(pin_pc) = pin_pc else {
                if pin.is_ok() {
                    t.missing.push(format!("{here}: pin has no exact pcurve"));
                }
                if a_miss > zone {
                    t.mismatches
                        .push(format!("{here}: scx's lift misses by {a_miss:e}"));
                } else {
                    t.within += 1;
                }
                continue;
            };
            let b: Option<Vec<Point2>> = ts
                .iter()
                .map(|&x| eval(&pin_pc, x).map(|[u, v]| from_step(s, Point2::new(u, v))))
                .collect();
            let Some(b) = b else {
                t.defects
                    .push(format!("{here}: the pin's pcurve does not evaluate"));
                continue;
            };
            let whole = |g: f64, p: Option<f64>| p.map_or(0.0, |p| (g / p).round() * p);
            let d0 = a[0] - b[0];
            let shift = [whole(d0.x, periods[0]), whole(d0.y, periods[1])];
            let mut exact = true;
            let mut worst = 0.0_f64;
            for (p, q) in a.iter().zip(&b) {
                let (dx, dy) = (p.x - q.x - shift[0], p.y - q.y - shift[1]);
                exact &= dx == 0.0 && dy == 0.0;
                let size = p.x.abs().max(p.y.abs()).max(1.0);
                worst = worst.max(dx.abs().max(dy.abs()) / size);
            }
            if exact {
                t.exact += 1;
            } else if worst <= UV_TOL {
                t.within += 1;
            } else {
                let b_miss = miss(s, curve, &ts, &b);
                if a_miss <= zone && b_miss > zone {
                    t.defects.push(format!(
                        "{here}: the pin's lift misses by {b_miss:e} (scx {a_miss:e})"
                    ));
                } else {
                    t.mismatches.push(format!(
                        "{here}: differ by {worst:e} in uv; lifts miss by {a_miss:e} (scx), \
                         {b_miss:e} (pin)"
                    ));
                }
            }
        }
    }
}

#[test]
fn pcurve_write_against_pin() {
    let doc = Document::default();
    let zone = doc.resolution().zone().mm();
    let mut t = Tally::default();
    for kind in Kind::ALL {
        for seed in 0..3 {
            for size in 1..=3 {
                let write = || {
                    let mut d = generate(kind, seed, size).unwrap().draft;
                    let faces: Vec<FaceId> = d.faces().map(|f| f.id()).collect();
                    for &f in &faces {
                        d.write_pcurves(f, &doc).unwrap();
                    }
                    (d.finish(&doc).unwrap().0, faces)
                };
                let (b, faces) = write();
                let (again, _) = write();
                if format!("{b:?}") != format!("{again:?}") {
                    t.nondeterministic += 1;
                }
                let at = format!("{} seed {seed} size {size}", kind.name());
                for f in faces {
                    face(&b, f, zone, &at, &mut t);
                }
            }
        }
    }
    println!(
        "difftest pcurve_write: {} inputs, {} exact, {} within tolerance, {} pin defects, {} \
         mismatches, {} nondeterministic",
        t.inputs,
        t.exact,
        t.within,
        t.defects.len(),
        t.mismatches.len(),
        t.nondeterministic
    );
    for d in &t.defects {
        println!("  pin defect: {d}");
    }
    println!("  the pin has no exact pcurve for {} uses", t.missing.len());
    for m in t.missing.iter().take(8) {
        println!("  {m}");
    }
    for m in t.mismatches.iter().take(20) {
        println!("  mismatch: {m}");
    }
    assert!(t.inputs > 1000, "{} inputs", t.inputs);
    assert_eq!(t.nondeterministic, 0);
    assert!(t.mismatches.is_empty(), "{} mismatches", t.mismatches.len());
}
