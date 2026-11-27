// Written for solidcorex. Apache-2.0.
//! `Draft::transform` (research 63 §6.5, BR6) against the pin's `SmBrep::Transform` and
//! `SmBrep::Mirror` (`topology::brep::mirror::{transform_matrix, mirror}`).
//!
//! The seed-0 generated bodies and the pin's primitives of the same solids (as `t_bodies`)
//! are moved by the same maps: a rigid motion, uniform scales up and down, a mirror, and a
//! mirror after a rigid motion. The two sides split their solids differently, so they are
//! not compared record for record; both are judged against the moved solid itself. Every
//! pin face's surface sampled over its uv domain, every pin vertex and edge sample, and
//! every generated vertex, edge sample, pcurve sample and face middle must lie on one of the
//! moved solid's sheets (a sheet of the solid at the inverse image); the moved block's
//! corners must be the same points on both sides. Each scx transform runs twice and must
//! give the same draft. An input is one point; a mismatch is one
//! off the moved solid, or a corner the sides disagree on.

use scx_brep::{Brep, Document, EdgeShape, UseGeom};
use scx_num::fmath::hypot;
use scx_num::math::Affine;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::{Kind, generate};
use solidcore::Kernel;
use solidcore::geom::axis2_placement::Axis2Placement;
use solidcore::primitive::api_primitives as ap;
use solidcore::topology::brep::mirror::{mirror, transform_matrix};

/// Off a sheet by at most this much, relative to the map's scale.
const TOL: f64 = 1e-9;

struct Solid {
    name: &'static str,
    kind: Kind,
    sheets: fn([f64; 3]) -> Vec<f64>,
}

fn rho(p: [f64; 3]) -> f64 {
    hypot(p[0], p[1])
}

const SOLIDS: [Solid; 5] = [
    Solid {
        name: "block",
        kind: Kind::Block,
        sheets: |p| vec![p[0], p[0] - 1.0, p[1], p[1] - 1.0, p[2], p[2] - 1.0],
    },
    Solid {
        name: "cylinder",
        kind: Kind::Cylinder,
        sheets: |p| vec![rho(p) - 1.0, p[2], p[2] - 2.0],
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
    },
    Solid {
        name: "sphere",
        kind: Kind::Sphere,
        sheets: |p| vec![hypot(rho(p), p[2]) - 1.0],
    },
    Solid {
        name: "torus",
        kind: Kind::Torus,
        sheets: |p| vec![hypot(rho(p) - 2.0, p[2]) - 1.0],
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

/// One map, as both sides take it.
struct Map {
    name: &'static str,
    a: Affine,
    /// The scale the map applies to lengths.
    k: f64,
    /// The pin's form: a plane to mirror in, or a row-vector matrix.
    pin: PinMap,
}

enum PinMap {
    Matrix([[f64; 4]; 4]),
    Mirror { origin: Vec3, normal: Vec3 },
}

/// The pin's row-vector matrix of `a`: rows are the images of the axes, then the image of
/// the origin.
fn rows(a: &Affine) -> [[f64; 4]; 4] {
    let (x, y, z, t) = (a.x_axis(), a.y_axis(), a.z_axis(), a.translation_part());
    [
        [x.x, x.y, x.z, 0.0],
        [y.x, y.y, y.z, 0.0],
        [z.x, z.y, z.z, 0.0],
        [t.x, t.y, t.z, 1.0],
    ]
}

fn maps() -> Vec<Map> {
    let turn =
        Affine::rotation(Point3::new(0.3, -1.2, 0.7), Vec3::new(1.0, 2.0, -0.5), 0.9).unwrap();
    let rigid = Affine::translation(Vec3::new(4.0, -3.5, 12.25)) * turn;
    let up = Affine::uniform_scaling(Point3::new(1.5, -0.25, 2.0), 2.5).unwrap();
    let down = Affine::uniform_scaling(Point3::ORIGIN, 0.375).unwrap();
    let (o, n) = (Vec3::new(0.5, 1.0, -2.0), Vec3::new(0.3, -1.0, 0.8));
    let flip = Affine::reflection(Point3::new(o.x, o.y, o.z), n).unwrap();
    let flip_rigid = flip * rigid;
    vec![
        Map {
            name: "rigid",
            pin: PinMap::Matrix(rows(&rigid)),
            a: rigid,
            k: 1.0,
        },
        Map {
            name: "scale_up",
            pin: PinMap::Matrix(rows(&up)),
            a: up,
            k: 2.5,
        },
        Map {
            name: "scale_down",
            pin: PinMap::Matrix(rows(&down)),
            a: down,
            k: 0.375,
        },
        Map {
            name: "mirror",
            pin: PinMap::Mirror {
                origin: o,
                normal: n,
            },
            a: flip,
            k: 1.0,
        },
        Map {
            name: "mirror_rigid",
            pin: PinMap::Matrix(rows(&flip_rigid)),
            a: flip_rigid,
            k: 1.0,
        },
    ]
}

/// A placement whose xy plane is the plane through `origin` with normal `normal`.
fn placement(origin: Vec3, normal: Vec3) -> Axis2Placement {
    let n = normal * (1.0 / normal.length());
    let helper = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let x = helper.cross(n);
    let x = x * (1.0 / x.length());
    let pin = |v: Vec3| solidcore::math::Vec3::new(v.x, v.y, v.z);
    Axis2Placement::new(pin(origin), pin(x), pin(n.cross(x)))
}

#[derive(Default)]
struct Tally {
    inputs: usize,
    exact: usize,
    within: usize,
    bad: Vec<String>,
}

impl Tally {
    /// `p` on a sheet of the solid moved by `inv`'s inverse (distances scale by `k`).
    fn point(&mut self, s: &Solid, inv: &Affine, k: f64, p: [f64; 3], at: &dyn Fn() -> String) {
        self.inputs += 1;
        let q = inv.apply(Point3::new(p[0], p[1], p[2]));
        let off = (s.sheets)([q.x, q.y, q.z])
            .into_iter()
            .map(f64::abs)
            .fold(f64::INFINITY, f64::min)
            * k;
        if off > TOL * k.max(1.0) {
            self.bad.push(format!("{}: {p:?} off {off:e}", at()));
        } else if off == 0.0 {
            self.exact += 1;
        } else {
            self.within += 1;
        }
    }
}

fn pin_points(s: &Solid, m: &Map, inv: &Affine, t: &mut Tally) -> Vec<[f64; 3]> {
    let mut k = Kernel::default();
    let mut pin = pin_body(s.name, &mut k);
    match &m.pin {
        PinMap::Matrix(rows) => transform_matrix(&mut k, &mut pin, rows, true).unwrap(),
        PinMap::Mirror { origin, normal } => {
            mirror(&mut k, &mut pin, &placement(*origin, *normal)).unwrap();
        }
    }
    for (id, f) in pin.faces().iter() {
        let surface = f.surface.as_ref().unwrap();
        let (u, v) = (f.uv_domain.u_interval(), f.uv_domain.v_interval());
        for i in 0..=4 {
            for j in 0..=4 {
                let uv = solidcore::Point2::new(
                    u.min() + u.length() * f64::from(i) / 4.0,
                    v.min() + v.length() * f64::from(j) / 4.0,
                );
                let p = surface.evaluate_point(&k, uv).unwrap();
                t.point(s, inv, m.k, [p.x, p.y, p.z], &|| {
                    format!("pin {} face {id:?} at {uv:?}", m.name)
                });
            }
        }
    }
    for (id, e) in pin.edges().iter() {
        let Some(curve) = e.curve.as_ref() else {
            continue;
        };
        for i in 0..=8 {
            let x = e.interval.min() + e.interval.length() * f64::from(i) / 8.0;
            let p = curve.evaluate_point(&k, x).unwrap();
            t.point(s, inv, m.k, [p.x, p.y, p.z], &|| {
                format!("pin {} edge {id:?} at {x}", m.name)
            });
        }
    }
    pin.vertices()
        .iter()
        .map(|(id, v)| {
            let p = [v.point.x, v.point.y, v.point.z];
            t.point(s, inv, m.k, p, &|| format!("pin {} vertex {id:?}", m.name));
            p
        })
        .collect()
}

fn scx_points(s: &Solid, m: &Map, inv: &Affine, b: &Brep, t: &mut Tally, size: usize) {
    let at = |what: String| move || format!("scx {} n={size} {what}", m.name);
    for v in b.vertices() {
        let p = v.point();
        t.point(
            s,
            inv,
            m.k,
            [p.x, p.y, p.z],
            &at(format!("vertex {:?}", v.id())),
        );
    }
    for e in b.edges() {
        if let EdgeShape::Curve { curve, interval } = e.shape() {
            for i in 0..=8 {
                let x = interval.min() + (interval.max() - interval.min()) * f64::from(i) / 8.0;
                let p = curve.point(x);
                t.point(
                    s,
                    inv,
                    m.k,
                    [p.x, p.y, p.z],
                    &at(format!("edge {:?} at {x}", e.id())),
                );
            }
        }
    }
    for f in b.faces() {
        let mut sum = (0.0, 0.0, 0.0);
        for l in f.loops() {
            for u in l.uses() {
                let UseGeom::Pcurve { curve, interval } = &u.record().geom else {
                    continue;
                };
                for i in 0..=4 {
                    let x = interval.min() + (interval.max() - interval.min()) * f64::from(i) / 4.0;
                    let q = curve.point(x);
                    let p = f.surface().point(q);
                    t.point(
                        s,
                        inv,
                        m.k,
                        [p.x, p.y, p.z],
                        &at(format!("face {:?} pcurve", f.id())),
                    );
                    if i == 0 || i == 4 {
                        sum = (sum.0 + q.x, sum.1 + q.y, sum.2 + 1.0);
                    }
                }
            }
        }
        if sum.2 > 0.0 {
            let p = f.surface().point(Point2::new(sum.0 / sum.2, sum.1 / sum.2));
            t.point(
                s,
                inv,
                m.k,
                [p.x, p.y, p.z],
                &at(format!("face {:?} middle", f.id())),
            );
        }
    }
}

/// Matches each corner of one side to the nearest of the other: equal to the bit, or
/// within a rounding of the map.
fn corners(pin: &[[f64; 3]], scx: &Brep, m: &Map, t: &mut Tally) {
    let ours: Vec<[f64; 3]> = scx
        .vertices()
        .map(|v| {
            let p = v.point();
            [p.x, p.y, p.z]
        })
        .collect();
    t.inputs += 1;
    let dist = |a: [f64; 3], b: [f64; 3]| hypot(hypot(a[0] - b[0], a[1] - b[1]), a[2] - b[2]);
    let mut worst = 0.0_f64;
    for p in pin {
        let near = ours
            .iter()
            .map(|q| dist(*p, *q))
            .fold(f64::INFINITY, f64::min);
        worst = worst.max(near);
    }
    if ours.len() != pin.len() || worst > TOL * m.k.max(1.0) * 20.0 {
        t.bad.push(format!(
            "block corners under {}: {} vs {}, worst {worst:e}",
            m.name,
            pin.len(),
            ours.len()
        ));
    } else if worst == 0.0 {
        t.exact += 1;
    } else {
        t.within += 1;
    }
}

#[test]
fn transform_against_pin() {
    let doc = Document::default();
    for m in &maps() {
        let inv = m.a.inverse().unwrap();
        let mut t = Tally::default();
        let mut nondeterministic = 0;
        for s in &SOLIDS {
            let pin = pin_points(s, m, &inv, &mut t);
            for size in 1..=3 {
                let g = generate(s.kind, 0, size).unwrap();
                let b = g.draft.finish(&doc).unwrap().0;
                let mut d = b.edit();
                d.transform(&m.a, &doc).unwrap();
                let mut again = b.edit();
                again.transform(&m.a, &doc).unwrap();
                if format!("{d:?}") != format!("{again:?}") {
                    nondeterministic += 1;
                }
                let moved = d.finish(&doc).unwrap().0;
                scx_points(s, m, &inv, &moved, &mut t, size);
                if s.kind == Kind::Block && size == 1 {
                    corners(&pin, &moved, m, &mut t);
                }
            }
        }
        println!(
            "difftest transform_{}: {} inputs, {} exact, {} within tolerance, 0 pin defects, {} mismatches, {} nondeterministic",
            m.name,
            t.inputs,
            t.exact,
            t.within,
            t.bad.len(),
            nondeterministic
        );
        assert_eq!(nondeterministic, 0, "{}", m.name);
        assert!(
            t.bad.is_empty(),
            "{}: {:#?}",
            m.name,
            &t.bad[..t.bad.len().min(20)]
        );
    }
}
