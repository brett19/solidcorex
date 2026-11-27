// Written for solidcorex. Apache-2.0.
//! `scx-testkit::bodies` (research 63 §6.4, BR8): the seed-0 generated bodies against the
//! pin's primitives of the same solid (`create_box`, `create_cylinder`, `create_cone`,
//! `create_sphere`, `create_torus`).
//!
//! The two sides split their solids differently (the pin's cylinder has one wall face and
//! a seam vertex pair; the generator's has `n` wall faces), so they are not compared record
//! for record. Both are checked against the solid itself instead: every pin face's surface,
//! sampled over its uv domain, lies on one of the solid's boundary sheets; every generated
//! vertex, edge sample and face middle lies on a sheet and in the solid's closure; the
//! generated surface kinds are the solid's (the pin's are printed only: it writes a
//! cylinder as a cone of half-angle 0); and the box's corners are the same points. An input is
//! one sampled point; a mismatch is one off the solid.

use std::collections::BTreeSet;

use scx_brep::{Draft, EdgeShape, UseGeom};
use scx_num::fmath::hypot;
use scx_num::vector2d::Point2;
use scx_testkit::bodies::{Kind, generate};
use solidcore::Kernel;
use solidcore::primitive::api_primitives as ap;

const TOL: f64 = 1e-9;

/// A solid by its boundary sheets (signed distance-like functions, zero on the sheet) and
/// its closure.
struct Solid {
    name: &'static str,
    kind: Kind,
    sheets: fn([f64; 3]) -> Vec<f64>,
    inside: fn([f64; 3]) -> bool,
    kinds: &'static [&'static str],
}

fn rho(p: [f64; 3]) -> f64 {
    hypot(p[0], p[1])
}

const SOLIDS: [Solid; 5] = [
    Solid {
        name: "block",
        kind: Kind::Block,
        sheets: |p| vec![p[0], p[0] - 1.0, p[1], p[1] - 1.0, p[2], p[2] - 1.0],
        inside: |p| p.iter().all(|&c| (-TOL..=1.0 + TOL).contains(&c)),
        kinds: &["plane"],
    },
    Solid {
        name: "cylinder",
        kind: Kind::Cylinder,
        sheets: |p| vec![rho(p) - 1.0, p[2], p[2] - 2.0],
        inside: |p| rho(p) <= 1.0 + TOL && (-TOL..=2.0 + TOL).contains(&p[2]),
        kinds: &["cylinder", "plane"],
    },
    Solid {
        name: "cone",
        kind: Kind::Cone,
        sheets: |p| {
            vec![
                (rho(p) - (1.0 - p[2])) * std::f64::consts::FRAC_1_SQRT_2,
                p[2],
            ]
        },
        inside: |p| (-TOL..=1.0 + TOL).contains(&p[2]) && rho(p) <= 1.0 - p[2] + TOL,
        kinds: &["cone", "plane"],
    },
    Solid {
        name: "sphere",
        kind: Kind::Sphere,
        sheets: |p| vec![hypot(rho(p), p[2]) - 1.0],
        inside: |p| hypot(rho(p), p[2]) <= 1.0 + TOL,
        kinds: &["sphere"],
    },
    Solid {
        name: "torus",
        kind: Kind::Torus,
        sheets: |p| vec![hypot(rho(p) - 2.0, p[2]) - 1.0],
        inside: |p| hypot(rho(p) - 2.0, p[2]) <= 1.0 + TOL,
        kinds: &["torus"],
    },
];

fn pin_body(name: &str, k: &mut Kernel) -> solidcore::topology::Brep {
    let o = solidcore::Point3::new(0.0, 0.0, 0.0);
    match name {
        "block" => ap::create_box(k, o, 1.0, 1.0, 1.0),
        "cylinder" => ap::create_cylinder(k, o, 1.0, 2.0),
        "cone" => ap::create_cone(k, o, 1.0, 0.0, 1.0),
        "sphere" => ap::create_sphere(k, o, 1.0),
        "torus" => ap::create_torus(k, o, 2.0, 1.0),
        _ => unreachable!(),
    }
    .unwrap()
}

fn scx_kind(s: &scx_surface::Surface) -> &'static str {
    use scx_surface::Surface as S;
    match s {
        S::Plane(_) => "plane",
        S::Cylinder(_) => "cylinder",
        S::Cone(_) => "cone",
        S::Sphere(_) => "sphere",
        S::Torus(_) => "torus",
        _ => "other",
    }
}

/// The pin's surface kind, for the record only: the pin writes a cylinder as a cone of half
/// angle 0 and a sphere or torus as a surface of revolution, so kinds are not compared
/// across the sides; the samples are.
fn pin_kind(s: &solidcore::surface::surface::Surface) -> String {
    let d = format!("{:?}", s.kind());
    d.split(['(', ' ', '{']).next().unwrap_or("").to_owned()
}

/// Tallies one point: within `TOL` of a sheet (and in the closure when asked).
#[derive(Default)]
struct Tally {
    inputs: usize,
    exact: usize,
    within: usize,
    bad: Vec<String>,
}

impl Tally {
    fn point(&mut self, s: &Solid, p: [f64; 3], closure: bool, at: &dyn Fn() -> String) {
        self.inputs += 1;
        let off = (s.sheets)(p)
            .into_iter()
            .map(f64::abs)
            .fold(f64::INFINITY, f64::min);
        if off > TOL || (closure && !(s.inside)(p)) {
            self.bad.push(format!("{}: {p:?} off {off:e}", at()));
        } else if off == 0.0 {
            self.exact += 1;
        } else {
            self.within += 1;
        }
    }
}

fn scx_points(s: &Solid, d: &Draft, t: &mut Tally, size: usize) {
    for v in d.vertices() {
        let p = v.point();
        t.point(s, [p.x, p.y, p.z], true, &|| {
            format!("scx n={size} vertex {:?}", v.id())
        });
    }
    for e in d.edges() {
        if let Some(EdgeShape::Curve { curve, interval }) = e.shape() {
            for k in 0..=8 {
                let x = interval.min() + (interval.max() - interval.min()) * f64::from(k) / 8.0;
                let p = curve.point(x);
                t.point(s, [p.x, p.y, p.z], true, &|| {
                    format!("scx n={size} edge {:?} at {x}", e.id())
                });
            }
        }
    }
    for f in d.faces() {
        // The middle of the outer loop's pcurve ends: inside a face of a split surface.
        let mut sum = (0.0, 0.0, 0.0);
        for u in f.loops().next().unwrap().uses() {
            let ends = match u.edge().unwrap().shape().unwrap() {
                EdgeShape::Curve { interval, .. } => [interval.min(), interval.max()],
                EdgeShape::Collapsed => [0.0, 1.0],
            };
            let UseGeom::Pcurve { curve, .. } = u.geom() else {
                continue;
            };
            for x in ends {
                let q = curve.point(x);
                sum = (sum.0 + q.x, sum.1 + q.y, sum.2 + 1.0);
            }
        }
        let p = f.surface().point(Point2::new(sum.0 / sum.2, sum.1 / sum.2));
        t.point(s, [p.x, p.y, p.z], true, &|| {
            format!("scx n={size} face {:?}", f.id())
        });
    }
}

#[test]
fn bodies_against_pin_primitives() {
    for s in &SOLIDS {
        let mut t = Tally::default();
        let mut k = Kernel::default();
        let pin = pin_body(s.name, &mut k);
        let mut pin_kinds = BTreeSet::new();
        for (id, f) in pin.faces().iter() {
            let surface = f.surface.as_ref().unwrap();
            pin_kinds.insert(pin_kind(surface));
            let (u, v) = (f.uv_domain.u_interval(), f.uv_domain.v_interval());
            for i in 0..=4 {
                for j in 0..=4 {
                    let uv = solidcore::Point2::new(
                        u.min() + u.length() * f64::from(i) / 4.0,
                        v.min() + v.length() * f64::from(j) / 4.0,
                    );
                    let p = surface.evaluate_point(&k, uv).unwrap();
                    // A plane's domain may reach past its trim: on a sheet only.
                    t.point(s, [p.x, p.y, p.z], false, &|| {
                        format!("pin face {id:?} at {uv:?}")
                    });
                }
            }
        }
        for size in 1..=4 {
            let d = generate(s.kind, 0, size).unwrap().draft;
            let again = generate(s.kind, 0, size).unwrap().draft;
            assert_eq!(
                format!("{d:?}"),
                format!("{again:?}"),
                "{} nondeterministic",
                s.name
            );
            let kinds: BTreeSet<_> = d.faces().map(|f| scx_kind(f.surface())).collect();
            let want: BTreeSet<_> = s.kinds.iter().copied().collect();
            t.inputs += 1;
            if kinds == want {
                t.exact += 1;
            } else {
                t.bad
                    .push(format!("n={size} kinds {kinds:?}, want {want:?}"));
            }
            scx_points(s, &d, &mut t, size);
        }
        if s.kind == Kind::Block {
            let key = |p: [f64; 3]| p.map(f64::to_bits);
            let pin_corners: BTreeSet<_> = pin
                .vertices()
                .iter()
                .map(|(_, v)| key([v.point.x, v.point.y, v.point.z]))
                .collect();
            let d = generate(Kind::Block, 0, 1).unwrap().draft;
            let scx_corners: BTreeSet<_> = d
                .vertices()
                .map(|v| {
                    let p = v.point();
                    key([p.x, p.y, p.z])
                })
                .collect();
            t.inputs += 1;
            if pin_corners == scx_corners {
                t.exact += 1;
            } else {
                t.bad.push("box corners differ".into());
            }
        }
        println!(
            "difftest bodies_{} pin surface kinds: {pin_kinds:?}",
            s.name
        );
        println!(
            "difftest bodies_{}: {} inputs, {} exact, {} within tolerance, 0 pin defects, {} mismatches, 0 nondeterministic",
            s.name,
            t.inputs,
            t.exact,
            t.within,
            t.bad.len()
        );
        assert!(
            t.bad.is_empty(),
            "{}: {:#?}",
            s.name,
            &t.bad[..t.bad.len().min(10)]
        );
    }
}
