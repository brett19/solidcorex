// Written for solidcorex. Apache-2.0.
//! Every generated body kind against its closed form (`scx_testkit::solid`, T9), at the
//! world frame and at random rigid frames: the volume, the area, the first and second
//! moments, each within the accuracy asked, and each error estimate covering the error.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::f64::consts::TAU;

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document};
use scx_mass::{Mass, MassSpec, RelAccuracy, area, mass, volume};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::math::Affine;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::analytic::Frame;
use scx_testkit::bodies::{Kind, generate};
use scx_testkit::profile::Seg;
use scx_testkit::solid::{self, Solid};

/// The seed-0 body of `kind` at `size`, carried to `frame`.
fn body(kind: Kind, size: usize, frame: &Frame) -> Brep {
    let doc = Document::default();
    let mut d = generate(kind, 0, size).unwrap().draft;
    let v = |a: [f64; 3]| Vec3::new(a[0], a[1], a[2]);
    let o = frame.origin;
    let map = Affine::from_axes(
        v(frame.x),
        v(frame.y),
        v(frame.z),
        Point3::new(o[0], o[1], o[2]),
    );
    d.transform(&map, &doc).unwrap();
    d.finish(&doc).map_err(|u| u.cause).unwrap().0
}

/// A closed polyline through `points`, back to the first.
fn closed(points: &[[f64; 2]]) -> Vec<Seg> {
    (0..points.len())
        .map(|j| Seg::Line {
            start: points[j],
            end: points[(j + 1) % points.len()],
        })
        .collect()
}

/// The regular `m`-gon of circumradius 1 from angle 0, counter-clockwise or not.
fn polygon(m: usize, ccw: bool) -> Vec<Seg> {
    let at = |j: usize| {
        let phi = TAU * j as f64 / m as f64;
        [fmath::cos(phi), fmath::sin(phi)]
    };
    let mut points: Vec<[f64; 2]> = (0..m).map(at).collect();
    if !ccw {
        points.reverse();
    }
    closed(&points)
}

/// The closed form of the seed-0 `kind` body at `size`, at the world frame.
fn truth(kind: Kind, size: usize) -> Solid {
    let n = size.max(1);
    match kind {
        Kind::Block => solid::block(1.0, 1.0, 1.0),
        Kind::Cylinder => solid::cylinder(1.0, 2.0),
        Kind::Cone => solid::cone(1.0, 0.0, 1.0),
        Kind::Sphere => solid::sphere(1.0),
        Kind::Torus => solid::torus(2.0, 1.0),
        Kind::Prism => solid::extrusion(&[polygon(n + 3, true)], 1.0),
        Kind::Holed => {
            let sq = closed(&[[-2.0, -2.0], [2.0, -2.0], [2.0, 2.0], [-2.0, 2.0]]);
            solid::extrusion(&[sq, polygon(n + 3, false)], 1.0)
        }
        Kind::Cavity => {
            let outer = solid::block(3.0, 3.0, 3.0).unwrap();
            let inner = solid::block(1.0, 1.0, 1.0).unwrap();
            Ok(Solid {
                moments: outer.moments.minus(&inner.moments.translated([1.0; 3])),
                area: outer.area + inner.area,
            })
        }
        Kind::Stack => {
            let s = solid::block(1.0, 1.0, (n + 1) as f64).unwrap();
            Ok(Solid {
                moments: s.moments,
                area: s.area,
            })
        }
        Kind::Checker | Kind::Touching => {
            // The black unit cells of the (n+1) x (n+1) board: they meet only along
            // edges, so no face is shared and every face counts toward the area.
            let cell = solid::block(1.0, 1.0, 1.0).unwrap();
            let mut moments = solid::Moments::ZERO;
            let mut area = 0.0;
            for i in 0..=n {
                for j in 0..=n {
                    if (i + j) % 2 == 0 {
                        moments = moments.plus(&cell.moments.translated([i as f64, j as f64, 0.0]));
                        area += cell.area;
                    }
                }
            }
            Ok(Solid { moments, area })
        }
        Kind::QuadSheet | Kind::TriSheet => Ok(Solid {
            moments: solid::Moments::ZERO,
            area: 1.0,
        }),
    }
    .unwrap()
}

/// A random rotation (from a unit quaternion) and an origin out to `reach`, from `seed`.
fn frame(seed: u64, reach: f64) -> Frame {
    let mut s = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    let q = [next(), next(), next(), next()];
    let l = q.iter().map(|x| x * x).sum::<f64>().sqrt();
    let [a, b, c, d] = q.map(|x| x / l);
    Frame {
        origin: [next() * reach, next() * reach, next() * reach],
        x: [
            a * a + b * b - c * c - d * d,
            2.0 * (b * c + a * d),
            2.0 * (b * d - a * c),
        ],
        y: [
            2.0 * (b * c - a * d),
            a * a - b * b + c * c - d * d,
            2.0 * (c * d + a * b),
        ],
        z: [
            2.0 * (b * d + a * c),
            2.0 * (c * d - a * b),
            a * a - b * b - c * c + d * d,
        ],
    }
}

const WORLD: Frame = Frame {
    origin: [0.0; 3],
    x: [1.0, 0.0, 0.0],
    y: [0.0, 1.0, 0.0],
    z: [0.0, 0.0, 1.0],
};

fn measure(b: &Brep, acc: f64) -> Mass {
    let spec = MassSpec::new(RelAccuracy::new(acc).unwrap()).with_about(Point3::new(0.0, 0.0, 0.0));
    mass(
        b,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

/// `got` against `want`, within `acc` of `scale`, and `err` covering the miss.
fn close(what: &str, got: f64, want: f64, err: f64, acc: f64, scale: f64) {
    let miss = (got - want).abs();
    assert!(
        miss <= acc * scale,
        "{what}: {got} against {want}, miss {miss:e}, scale {scale}"
    );
    assert!(
        miss <= err.max(1e-13 * scale),
        "{what}: miss {miss:e} not covered by estimate {err:e}"
    );
}

fn check(kind: Kind, size: usize, at: &Frame, acc: f64) {
    let b = body(kind, size, at);
    let want = truth(kind, size).place(at);
    let m = measure(&b, acc);
    let name = kind.name();
    let w = want.moments;
    let len = 1.0 + at.origin.iter().map(|x| x.abs()).fold(0.0, f64::max) + 4.0;
    let vs = w.volume.abs().max(want.area);
    close(
        &format!("{name} volume"),
        m.volume,
        w.volume,
        m.error.volume,
        acc,
        vs,
    );
    close(
        &format!("{name} area"),
        m.area,
        want.area,
        m.error.area,
        acc,
        want.area,
    );
    let first = [m.first.x, m.first.y, m.first.z];
    let e_first = [m.error.first.x, m.error.first.y, m.error.first.z];
    for i in 0..3 {
        close(
            &format!("{name} first {i}"),
            first[i],
            w.first[i],
            e_first[i],
            acc,
            vs * len,
        );
    }
    let s = m.second;
    let es = m.error.second;
    let got = [[s.xx, s.xy, s.zx], [s.xy, s.yy, s.yz], [s.zx, s.yz, s.zz]];
    let err = [
        [es.xx, es.xy, es.zx],
        [es.xy, es.yy, es.yz],
        [es.zx, es.yz, es.zz],
    ];
    for i in 0..3 {
        for j in 0..3 {
            let what = format!("{name} second {i}{j}");
            close(
                &what,
                got[i][j],
                w.second[i][j],
                err[i][j],
                acc,
                vs * len * len,
            );
        }
    }
}

/// Every kind at the world frame, at sizes 1 to 3.
#[test]
fn every_kind_holds_its_closed_form() {
    for kind in Kind::ALL {
        for size in 1..=3 {
            check(kind, size, &WORLD, 1e-9);
        }
    }
}

/// Every kind at random rigid frames.
#[test]
fn every_kind_holds_at_random_frames() {
    for kind in Kind::ALL {
        for seed in 1..=4 {
            check(kind, 2, &frame(seed, 10.0), 1e-8);
        }
    }
}

/// The single-value reads agree with the full measurement.
#[test]
fn volume_and_area_agree_with_mass() {
    let acc = RelAccuracy::new(1e-10).unwrap();
    for kind in Kind::ALL {
        let b = body(kind, 2, &frame(7, 3.0));
        let want = truth(kind, 2);
        let v = volume(
            &b,
            acc,
            &mut Scratch::new(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        let a = area(
            &b,
            acc,
            &mut Scratch::new(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        let scale = want.area.max(want.volume().abs());
        assert!(
            (v.value - want.volume()).abs() <= 1e-10 * scale,
            "{}: {v:?}",
            kind.name()
        );
        assert!(
            (a.value - want.area).abs() <= 1e-10 * want.area,
            "{}: {a:?}",
            kind.name()
        );
    }
}

/// The pin's `precise_area_holds_its_accuracy` (its quarter annuli need a sweep this
/// workspace does not have yet): every curved kind's area and volume at each accuracy from
/// 1e-2 to 1e-10 at a random frame, within it, the estimate covering the miss.
#[test]
fn every_accuracy_is_held() {
    for kind in [Kind::Cylinder, Kind::Cone, Kind::Sphere, Kind::Torus] {
        let at = frame(11, 2.0);
        let b = body(kind, 1, &at);
        let want = truth(kind, 1);
        for e in 2..=10 {
            let acc = fmath::powi(10.0, -e);
            let r = RelAccuracy::new(acc).unwrap();
            let a = area(
                &b,
                r,
                &mut Scratch::new(),
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap();
            let miss = (a.value - want.area).abs();
            assert!(
                miss <= acc * want.area,
                "{} 1e-{e}: area miss {miss:e}",
                kind.name()
            );
            assert!(
                miss <= a.error.max(1e-14),
                "{} 1e-{e}: miss {miss:e}, estimate {:e}",
                kind.name(),
                a.error
            );
            let v = volume(
                &b,
                r,
                &mut Scratch::new(),
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap();
            let miss = (v.value - want.volume()).abs();
            assert!(
                miss <= acc * want.area,
                "{} 1e-{e}: volume miss {miss:e}",
                kind.name()
            );
            assert!(
                miss <= v.error.max(1e-13),
                "{} 1e-{e}: miss {miss:e}, estimate {:e}",
                kind.name(),
                v.error
            );
        }
    }
}
