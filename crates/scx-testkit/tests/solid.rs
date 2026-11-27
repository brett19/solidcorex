// Written for solidcorex. Apache-2.0.
//! The closed-form solids (research 72 T9) by independent routes: textbook formulas,
//! double-double evaluation of the stated error, Monte Carlo with a seeded generator and a
//! 5-sigma bound, polygon moments of finely discretised arcs, nested quadrature of a slot,
//! and invariance under rigid motion.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::unwrap_used,
    clippy::float_cmp
)]

use scx_num::fmath;
use scx_num::hash::Rng;
use scx_testkit::analytic::Frame;
use scx_testkit::dd::{self, Dd};
use scx_testkit::sample;
use scx_testkit::solid::{
    self, Hole, Loop, Moments, REL_ERR, Seg, Solid, SolidError, block, cone, cylinder, extrusion,
    gear, plate, sphere, sphere_cap, torus, torus_sector,
};

const PI: f64 = core::f64::consts::PI;

fn close(a: f64, b: f64, rel: f64, what: &str) {
    let scale = a.abs().max(b.abs()).max(f64::MIN_POSITIVE);
    assert!((a - b).abs() <= rel * scale, "{what}: {a} vs {b}");
}

fn close_m(a: &Moments, b: &Moments, rel: f64, what: &str) {
    let big = a
        .second
        .iter()
        .flatten()
        .fold(a.volume.abs(), |m, v| m.max(v.abs()));
    let tol = rel * big.max(1e-300);
    assert!(
        (a.volume - b.volume).abs() <= rel * a.volume.abs(),
        "{what}: volume"
    );
    for i in 0..3 {
        // first moments are judged against volume times the body's own extent
        assert!(
            (a.first[i] - b.first[i]).abs()
                <= rel * (a.first[i].abs() + b.first[i].abs() + a.volume),
            "{what}: first[{i}] {} vs {}",
            a.first[i],
            b.first[i]
        );
        for j in 0..3 {
            assert!(
                (a.second[i][j] - b.second[i][j]).abs() <= tol,
                "{what}: second[{i}][{j}] {} vs {}",
                a.second[i][j],
                b.second[i][j]
            );
        }
    }
}

fn mass_inertia(s: &Solid) -> [[f64; 3]; 3] {
    s.moments.inertia_about_centroid()
}

// ---------------------------------------------------------------------------------------
// Textbook formulas (route 1)

#[test]
fn block_cylinder_cone_sphere_match_the_textbook() {
    // block
    let b = block(2.0, 3.0, 5.0).unwrap();
    close(b.volume(), 30.0, 1e-15, "block V");
    close(b.area, 2.0 * (6.0 + 15.0 + 10.0), 1e-15, "block A");
    let i = mass_inertia(&b);
    close(i[0][0], 30.0 * (9.0 + 25.0) / 12.0, 1e-14, "block Ixx");
    close(i[2][2], 30.0 * (4.0 + 9.0) / 12.0, 1e-14, "block Izz");
    assert!(i[0][1].abs() < 1e-12);

    // cylinder: Izz = m r^2 / 2, Ixx = m (3 r^2 + h^2) / 12 about the centroid
    let c = cylinder(1.5, 4.0).unwrap();
    let m = PI * 2.25 * 4.0;
    close(c.volume(), m, 1e-14, "cyl V");
    close(
        c.area,
        2.0 * PI * 2.25 + 2.0 * PI * 1.5 * 4.0,
        1e-14,
        "cyl A",
    );
    let i = mass_inertia(&c);
    close(i[2][2], m * 2.25 / 2.0, 1e-13, "cyl Izz");
    close(i[0][0], m * (3.0 * 2.25 + 16.0) / 12.0, 1e-13, "cyl Ixx");
    close(c.moments.centroid()[2], 2.0, 1e-14, "cyl cz");

    // cone: V = pi r^2 h / 3, centroid h / 4 above the base, Izz = 3 m r^2 / 10,
    // Ixx about the centroid = 3 m (r^2 / 4 + h^2 / 20) ... about the apex 3m(r^2/20·... ) so
    // check the centroidal one: 3 m (4 r^2 + h^2) / 80.
    let k = cone(2.0, 0.0, 3.0).unwrap();
    let m = PI * 4.0 * 3.0 / 3.0;
    close(k.volume(), m, 1e-14, "cone V");
    close(k.moments.centroid()[2], 0.75, 1e-14, "cone cz");
    let i = mass_inertia(&k);
    close(i[2][2], 0.3 * m * 4.0, 1e-13, "cone Izz");
    close(
        i[0][0],
        3.0 * m * (4.0 * 4.0 + 9.0) / 80.0,
        1e-13,
        "cone Ixx",
    );
    close(
        k.area,
        PI * 2.0 * (4.0f64 + 9.0).sqrt() + PI * 4.0,
        1e-14,
        "cone A",
    );

    // frustum volume: pi h (r0^2 + r0 r1 + r1^2) / 3
    let f = cone(2.0, 1.0, 3.0).unwrap();
    close(f.volume(), PI * 3.0 * 7.0 / 3.0, 1e-14, "frustum V");
    // a frustum with equal radii is a cylinder
    let g = cone(1.5, 1.5, 4.0).unwrap();
    close_m(&g.moments, &c.moments, 1e-13, "cone(r, r) is cylinder");
    close(g.area, c.area, 1e-13, "cone(r, r) area");

    // sphere
    let s = sphere(1.25).unwrap();
    let m = 4.0 / 3.0 * PI * 1.25 * 1.25 * 1.25;
    close(s.volume(), m, 1e-14, "sphere V");
    close(s.area, 4.0 * PI * 1.5625, 1e-14, "sphere A");
    let i = mass_inertia(&s);
    for k in 0..3 {
        close(i[k][k], 0.4 * m * 1.5625, 1e-13, "sphere I");
    }
}

#[test]
fn cap_and_torus_match_the_textbook() {
    let (r, h) = (2.0, 0.75);
    let cap = sphere_cap(r, h).unwrap();
    close(
        cap.volume(),
        PI * h * h * (3.0 * r - h) / 3.0,
        1e-14,
        "cap V",
    );
    close(
        cap.moments.centroid()[2],
        3.0 * (2.0 * r - h) * (2.0 * r - h) / (4.0 * (3.0 * r - h)),
        1e-13,
        "cap cz",
    );
    close(
        cap.area,
        2.0 * PI * r * h + PI * h * (2.0 * r - h),
        1e-14,
        "cap A",
    );
    // the hemisphere: V = 2 pi r^3 / 3, centroid 3 r / 8, area 3 pi r^2
    let hemi = sphere_cap(r, r).unwrap();
    close(hemi.volume(), 2.0 * PI * 8.0 / 3.0, 1e-14, "hemi V");
    close(hemi.moments.centroid()[2], 0.375 * r, 1e-14, "hemi cz");
    close(hemi.area, 3.0 * PI * 4.0, 1e-14, "hemi A");
    // thickness 2 r is the sphere
    close_m(
        &sphere_cap(r, 2.0 * r).unwrap().moments,
        &sphere(r).unwrap().moments,
        1e-13,
        "cap(2r) is sphere",
    );

    // torus: V = 2 pi^2 R r^2, A = 4 pi^2 R r, Izz = m (R^2 + 3 r^2 / 4),
    // Ixx = m (R^2 / 2 + 5 r^2 / 8)
    let (big, small) = (3.0, 0.8);
    let t = torus(big, small).unwrap();
    let m = 2.0 * PI * PI * big * small * small;
    close(t.volume(), m, 1e-14, "torus V");
    close(t.area, 4.0 * PI * PI * big * small, 1e-14, "torus A");
    let i = mass_inertia(&t);
    close(
        i[2][2],
        m * (big * big + 0.75 * small * small),
        1e-13,
        "torus Izz",
    );
    close(
        i[0][0],
        m * (big * big / 2.0 + 0.625 * small * small),
        1e-13,
        "torus Ixx",
    );
    // a half: half the volume, the cut adds two disks, and the centroid is on the bisector
    let half = torus_sector(big, small, PI).unwrap();
    close(half.volume(), m / 2.0, 1e-14, "half torus V");
    close(
        half.area,
        2.0 * PI * PI * big * small + 2.0 * PI * small * small,
        1e-14,
        "half torus A",
    );
    let c = half.moments.centroid();
    close(
        c[1],
        2.0 * (big * big + small * small / 4.0) / (PI * big),
        1e-13,
        "half torus cy",
    );
    assert!(c[0].abs() < 1e-14 && c[2] == 0.0);
    // two halves make the whole
    let other = torus_sector(big, small, PI).unwrap().moments.rotated([
        [-1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
    ]);
    close_m(&half.moments.plus(&other), &t.moments, 1e-13, "two halves");
}

#[test]
fn typed_errors() {
    assert_eq!(block(0.0, 1.0, 1.0), Err(SolidError::Dimension("dx")));
    assert_eq!(cylinder(1.0, f64::NAN), Err(SolidError::Dimension("h")));
    assert_eq!(cone(0.0, 0.0, 1.0), Err(SolidError::Dimension("r0 + r1")));
    assert_eq!(sphere_cap(1.0, 2.5), Err(SolidError::Range("h")));
    assert_eq!(torus(1.0, 1.0), Err(SolidError::Range("r")));
    assert_eq!(torus_sector(2.0, 1.0, 7.0), Err(SolidError::Range("sweep")));
    let holes = [Hole {
        x: 5.0,
        y: 5.0,
        r: 6.0,
    }];
    assert_eq!(plate(10.0, 10.0, 1.0, &holes), Err(SolidError::Apart));
    let touching = [
        Hole {
            x: 3.0,
            y: 3.0,
            r: 1.0,
        },
        Hole {
            x: 5.0,
            y: 3.0,
            r: 1.0,
        },
    ];
    assert_eq!(plate(10.0, 10.0, 1.0, &touching), Err(SolidError::Apart));
    // a slot wider than the pitch
    assert_eq!(gear(50.0, 10.0, 60, 45.0, 6.0, 0.0), Err(SolidError::Apart));
    let open = Loop {
        start: [0.0, 0.0],
        segs: vec![Seg::Line { to: [1.0, 0.0] }, Seg::Line { to: [1.0, 1.0] }],
    };
    assert_eq!(
        extrusion(&[open], 1.0),
        Err(SolidError::Profile("loop is not closed"))
    );
    let bad_arc = Loop {
        start: [0.0, 0.0],
        segs: vec![
            Seg::Arc {
                to: [3.0, 0.0],
                center: [1.0, 0.0],
                ccw: true,
            },
            Seg::Line { to: [0.0, 0.0] },
        ],
    };
    assert_eq!(
        extrusion(&[bad_arc], 1.0),
        Err(SolidError::Profile("arc radii differ"))
    );
    let clockwise = Loop {
        start: [0.0, 0.0],
        segs: vec![
            Seg::Line { to: [0.0, 1.0] },
            Seg::Line { to: [1.0, 0.0] },
            Seg::Line { to: [0.0, 0.0] },
        ],
    };
    assert_eq!(
        extrusion(&[clockwise], 1.0),
        Err(SolidError::Profile("area is not positive"))
    );
}

// ---------------------------------------------------------------------------------------
// The stated error, against double-double (route 2)

fn pi_dd() -> Dd {
    dd::consts().pi
}

fn within(c: f64, r: Dd, largest: f64, what: &str) {
    let err = ((Dd::new(c) - r).to_f64()).abs();
    assert!(
        err <= REL_ERR * largest,
        "{what}: error {err} over {}",
        REL_ERR * largest
    );
}

fn asin_dd(x: Dd) -> Dd {
    dd::atan2(x, (Dd::new(1.0) - x * x).sqrt())
}

#[test]
fn results_are_within_the_stated_error() {
    let pi = pi_dd();
    let (r, h) = (Dd::new(1.25), Dd::new(3.5));
    // sphere
    let s = sphere(1.25).unwrap();
    within(
        s.volume(),
        pi * r * r * r * 4.0 / 3.0,
        s.volume(),
        "sphere V",
    );
    within(s.area, pi * r * r * 4.0, s.area, "sphere A");
    // cylinder
    let c = cylinder(1.25, 3.5).unwrap();
    within(c.volume(), pi * r * r * h, c.volume(), "cyl V");
    within(c.area, pi * r * r * 2.0 + pi * r * h * 2.0, c.area, "cyl A");
    // torus
    let t = torus(3.0, 0.8).unwrap();
    let (rb, rs) = (Dd::new(3.0), Dd::new(0.8));
    within(
        t.volume(),
        pi * pi * rb * rs * rs * 2.0,
        t.volume(),
        "torus V",
    );
    within(t.area, pi * pi * rb * rs * 4.0, t.area, "torus A");
    // plate_12: twelve holes of radius 2 through a 100 x 100 x 10 block
    let holes: Vec<Hole> = (0..12)
        .map(|i| Hole {
            x: 20.0 + 20.0 * (i % 4) as f64,
            y: 25.0 + 25.0 * (i / 4) as f64,
            r: 2.0,
        })
        .collect();
    let p = plate(100.0, 100.0, 10.0, &holes).unwrap();
    within(
        p.volume(),
        Dd::new(100000.0) - pi * 480.0,
        100000.0,
        "plate_12 V",
    );
    // A = 2 (1e4 - 12 pi 4) + 4 * 100 * 10 + 12 * 2 pi 2 * 10
    within(
        p.area,
        (Dd::new(10000.0) - pi * 48.0) * 2.0 + Dd::new(4000.0) + pi * 480.0,
        20000.0,
        "plate_12 A",
    );
    // gear_60: R 50, h 10, root 45, gap = 2 pi 45 / 60 * 0.45, phase 0.1234
    let gap = 2.0 * PI * 45.0 / 60.0 * 0.45;
    let g = gear(50.0, 10.0, 60, 45.0, gap, 0.1234).unwrap();
    let (rr, hh, gp) = (Dd::new(50.0), Dd::new(10.0), Dd::new(gap));
    let half = gp / 2.0;
    let rim = (rr * rr - half * half).sqrt();
    let slot = half * rim + rr * rr * asin_dd(half / rr) - Dd::new(45.0) * gp;
    let vol = hh * (pi * rr * rr - slot * 60.0);
    within(g.volume(), vol, vol.to_f64(), "gear V");
    let theta = asin_dd(half / rr);
    let area = (pi * rr * rr - slot * 60.0) * 2.0
        + hh * (pi * rr * 2.0 - rr * theta * 120.0)
        + (gp * hh + (rim - Dd::new(45.0)) * hh * 2.0) * 60.0;
    within(g.area, area, area.to_f64(), "gear A");
}

// ---------------------------------------------------------------------------------------
// Monte Carlo (route 3): a seeded generator, the body as a membership test, 5 sigma

type Member<'a> = &'a dyn Fn([f64; 3]) -> bool;

fn to_local(f: &Frame, p: [f64; 3]) -> [f64; 3] {
    let d = [p[0] - f.origin[0], p[1] - f.origin[1], p[2] - f.origin[2]];
    let dot = |a: [f64; 3]| a[0] * d[0] + a[1] * d[1] + a[2] * d[2];
    [dot(f.x), dot(f.y), dot(f.z)]
}

fn to_world(f: &Frame, l: [f64; 3]) -> [f64; 3] {
    core::array::from_fn(|i| f.origin[i] + f.x[i] * l[0] + f.y[i] * l[1] + f.z[i] * l[2])
}

/// The ten integrands `1, x, y, z, xx, xy, xz, yy, yz, zz` in world coordinates.
fn integrands(p: [f64; 3]) -> [f64; 10] {
    [
        1.0,
        p[0],
        p[1],
        p[2],
        p[0] * p[0],
        p[0] * p[1],
        p[0] * p[2],
        p[1] * p[1],
        p[1] * p[2],
        p[2] * p[2],
    ]
}

fn flatten(m: &Moments) -> [f64; 10] {
    [
        m.volume,
        m.first[0],
        m.first[1],
        m.first[2],
        m.second[0][0],
        m.second[0][1],
        m.second[0][2],
        m.second[1][1],
        m.second[1][2],
        m.second[2][2],
    ]
}

/// Estimates the moments of the body at `frame` by uniform samples in the world box around
/// the local box `lo..hi`, and checks the truth lies within 5 sigma of each (plus a floor of
/// rounding).
fn mc_check(
    what: &str,
    truth: &Solid,
    frame: &Frame,
    lo: [f64; 3],
    hi: [f64; 3],
    member: Member,
    n: usize,
    seed: u64,
) {
    let corners: Vec<[f64; 3]> = (0..8)
        .map(|k| {
            to_world(
                frame,
                [
                    if k & 1 == 0 { lo[0] } else { hi[0] },
                    if k & 2 == 0 { lo[1] } else { hi[1] },
                    if k & 4 == 0 { lo[2] } else { hi[2] },
                ],
            )
        })
        .collect();
    let wlo: [f64; 3] =
        core::array::from_fn(|i| corners.iter().map(|c| c[i]).fold(f64::MAX, f64::min));
    let whi: [f64; 3] =
        core::array::from_fn(|i| corners.iter().map(|c| c[i]).fold(f64::MIN, f64::max));
    let boxv = (0..3).map(|i| whi[i] - wlo[i]).product::<f64>();
    let mut rng = Rng::new(seed);
    let (mut sum, mut sq) = ([0.0f64; 10], [0.0f64; 10]);
    for _ in 0..n {
        let p: [f64; 3] = core::array::from_fn(|i| wlo[i] + (whi[i] - wlo[i]) * rng.next_f64());
        let l = to_local(frame, p);
        if (0..3).all(|i| lo[i] <= l[i] && l[i] <= hi[i]) && member(l) {
            let f = integrands(p);
            for k in 0..10 {
                sum[k] += f[k];
                sq[k] += f[k] * f[k];
            }
        }
    }
    let placed = truth.place(frame);
    let t = flatten(&placed.moments);
    let nf = n as f64;
    for k in 0..10 {
        let mean = sum[k] / nf;
        let var = (sq[k] / nf - mean * mean).max(0.0);
        let est = boxv * mean;
        let sigma = boxv * (var / nf).sqrt();
        assert!(
            (est - t[k]).abs() <= 5.0 * sigma + 1e-9 * t[0].abs(),
            "{what}: component {k}: truth {} estimate {est} sigma {sigma}",
            t[k]
        );
    }
}

fn frames(seed: u64, count: usize) -> Vec<Frame> {
    let mut rng = Rng::new(seed);
    let mut v = vec![Frame {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    }];
    v.extend((0..count).map(|_| sample::frame(&mut rng, 3.0)));
    v
}

#[test]
fn primitives_by_monte_carlo_at_random_frames() {
    let n = 300_000;
    for (k, f) in frames(7, 2).iter().enumerate() {
        let seed = 100 + k as u64;
        let b = block(1.0, 2.0, 1.5).unwrap();
        mc_check(
            "block",
            &b,
            f,
            [0.0; 3],
            [1.0, 2.0, 1.5],
            &|_| true,
            n,
            seed,
        );
        let c = cylinder(0.8, 2.0).unwrap();
        mc_check(
            "cylinder",
            &c,
            f,
            [-0.8, -0.8, 0.0],
            [0.8, 0.8, 2.0],
            &|p| p[0] * p[0] + p[1] * p[1] <= 0.64,
            n,
            seed,
        );
        let kz = cone(1.0, 0.4, 1.7).unwrap();
        mc_check(
            "frustum",
            &kz,
            f,
            [-1.0, -1.0, 0.0],
            [1.0, 1.0, 1.7],
            &|p| {
                let r = 1.0 + (0.4 - 1.0) * p[2] / 1.7;
                p[0] * p[0] + p[1] * p[1] <= r * r
            },
            n,
            seed,
        );
        let s = sphere(1.1).unwrap();
        mc_check(
            "sphere",
            &s,
            f,
            [-1.1; 3],
            [1.1; 3],
            &|p| p[0] * p[0] + p[1] * p[1] + p[2] * p[2] <= 1.21,
            n,
            seed,
        );
        let cap = sphere_cap(1.1, 0.7).unwrap();
        mc_check(
            "cap",
            &cap,
            f,
            [-1.1, -1.1, 0.4],
            [1.1, 1.1, 1.1],
            &|p| p[0] * p[0] + p[1] * p[1] + p[2] * p[2] <= 1.21 && p[2] >= 0.4,
            n,
            seed,
        );
        for sweep in [PI, 4.0, 2.0 * PI] {
            let t = torus_sector(1.5, 0.5, sweep).unwrap();
            mc_check(
                "torus sector",
                &t,
                f,
                [-2.0, -2.0, -0.5],
                [2.0, 2.0, 0.5],
                &|p| {
                    let rho = (p[0] * p[0] + p[1] * p[1]).sqrt();
                    let mut a = fmath::atan2(p[1], p[0]);
                    if a < 0.0 {
                        a += 2.0 * PI;
                    }
                    (rho - 1.5) * (rho - 1.5) + p[2] * p[2] <= 0.25 && a < sweep
                },
                n,
                seed,
            );
        }
    }
}

fn pitch_grid() -> Vec<Hole> {
    (0..12)
        .map(|i| Hole {
            x: 20.0 + 20.0 * (i % 4) as f64,
            y: 25.0 + 25.0 * (i / 4) as f64,
            r: 2.0,
        })
        .collect()
}

#[test]
fn plate_by_monte_carlo() {
    let holes = pitch_grid();
    let p = plate(100.0, 100.0, 10.0, &holes).unwrap();
    let member = |q: [f64; 3]| {
        (0.0..=100.0).contains(&q[0])
            && (0.0..=100.0).contains(&q[1])
            && (0.0..=10.0).contains(&q[2])
            && holes
                .iter()
                .all(|h| (q[0] - h.x) * (q[0] - h.x) + (q[1] - h.y) * (q[1] - h.y) > h.r * h.r)
    };
    for (k, f) in frames(11, 1).iter().enumerate() {
        mc_check(
            "plate",
            &p,
            f,
            [0.0; 3],
            [100.0, 100.0, 10.0],
            &member,
            400_000,
            200 + k as u64,
        );
    }
    // holes' removal is exact bookkeeping: the plate plus its holes is the block
    let mut with_holes = p.moments;
    for h in &holes {
        with_holes = with_holes.plus(
            &cylinder(h.r, 10.0)
                .unwrap()
                .moments
                .translated([h.x, h.y, 0.0]),
        );
    }
    close_m(
        &with_holes,
        &block(100.0, 100.0, 10.0).unwrap().moments,
        1e-13,
        "plate + holes",
    );
}

fn gear_member(phase: f64, gap: f64, count: u32) -> impl Fn([f64; 3]) -> bool {
    move |q: [f64; 3]| {
        if !(0.0..=10.0).contains(&q[2]) || q[0] * q[0] + q[1] * q[1] > 2500.0 {
            return false;
        }
        for i in 0..count {
            let a = phase + 2.0 * PI * f64::from(i) / f64::from(count);
            let (s, c) = fmath::sin_cos(a);
            let (lx, ly) = (c * q[0] + s * q[1], -s * q[0] + c * q[1]);
            if lx >= 45.0 && ly.abs() <= gap / 2.0 {
                return false;
            }
        }
        true
    }
}

#[test]
fn gear_60_by_monte_carlo_and_the_documented_formula() {
    let gap = 2.0 * PI * 45.0 / 60.0 * 0.45;
    let g = gear(50.0, 10.0, 60, 45.0, gap, 0.1234).unwrap();
    // the formula of research 72 §2.4, written out again here
    let half = gap / 2.0;
    let integral =
        half * (2500.0 - half * half).sqrt() + 2500.0 * fmath::asin(half / 50.0) - 45.0 * gap;
    close(
        g.volume(),
        10.0 * (PI * 2500.0 - 60.0 * integral),
        1e-13,
        "gear V",
    );
    // Monte Carlo: the slots are 3% of the volume, so this is a coarse check of the moments
    // on top of the exact checks above and below
    let member = gear_member(0.1234, gap, 60);
    for (k, f) in frames(13, 1).iter().enumerate() {
        mc_check(
            "gear",
            &g,
            f,
            [-50.0, -50.0, 0.0],
            [50.0, 50.0, 10.0],
            &member,
            400_000,
            300 + k as u64,
        );
    }
    // the plain disk's value less sixty slots, each by nested quadrature (route 4)
    let slot = extrusion(
        &[Loop {
            start: [45.0, -half],
            segs: vec![
                Seg::Line {
                    to: [(2500.0 - half * half).sqrt(), -half],
                },
                Seg::Arc {
                    to: [(2500.0 - half * half).sqrt(), half],
                    center: [0.0, 0.0],
                    ccw: true,
                },
                Seg::Line { to: [45.0, half] },
                Seg::Line { to: [45.0, -half] },
            ],
        }],
        1.0,
    )
    .unwrap();
    // int_{-g/2}^{g/2} of (X^k - root^k) / k and the y-weighted forms, by 3-point Gauss
    // on 4000 panels
    let panels = 4000;
    let (mut a, mut qx, mut qy, mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let w = [5.0 / 18.0, 8.0 / 18.0, 5.0 / 18.0];
    let o = 0.5 * 0.6f64.sqrt();
    let t = [0.5 - o, 0.5, 0.5 + o];
    let dy = gap / panels as f64;
    for p in 0..panels {
        for (wi, ti) in w.iter().zip(t) {
            let y = -half + dy * (p as f64 + ti);
            let x = (2500.0 - y * y).sqrt();
            let wt = wi * dy;
            a += wt * (x - 45.0);
            qx += wt * (x * x - 45.0 * 45.0) / 2.0;
            qy += wt * y * (x - 45.0);
            xx += wt * (x * x * x - 45.0 * 45.0 * 45.0) / 3.0;
            xy += wt * y * (x * x - 45.0 * 45.0) / 2.0;
            yy += wt * y * y * (x - 45.0);
        }
    }
    let m = &slot.moments;
    close(m.volume, a, 1e-10, "slot A");
    close(m.first[0], qx, 1e-10, "slot Qx");
    assert!(
        (m.first[1] - qy).abs() < 1e-9 && qy.abs() < 1e-9,
        "slot Qy: symmetric in y"
    );
    close(m.second[0][0], xx, 1e-10, "slot Ixx");
    assert!(
        (m.second[0][1] - xy).abs() < 1e-9 && xy.abs() < 1e-9,
        "slot Ixy: symmetric in y"
    );
    close(m.second[1][1], yy, 1e-10, "slot Iyy");
    // symmetric slots 6 degrees apart: the gear's centroid is on the axis to rounding, and
    // is exactly the disk's when the phase turns the slots over each other
    let c = g.moments.centroid();
    assert!(
        c[0].abs() < 1e-9 && c[1].abs() < 1e-9,
        "gear centroid {c:?}"
    );
    close(c[2], 5.0, 1e-14, "gear cz");
}

// ---------------------------------------------------------------------------------------
// Profiles of lines and arcs against polygon moments of finely cut arcs (route 5)

fn discretise(lp: &Loop, per_arc: usize) -> Vec<[f64; 2]> {
    let mut pts = vec![lp.start];
    let mut p0 = lp.start;
    for s in &lp.segs {
        match *s {
            Seg::Line { to } => {
                pts.push(to);
                p0 = to;
            }
            Seg::Arc { to, center, ccw } => {
                let (a, b) = (
                    [p0[0] - center[0], p0[1] - center[1]],
                    [to[0] - center[0], to[1] - center[1]],
                );
                let r = (a[0] * a[0] + a[1] * a[1]).sqrt();
                let t0 = fmath::atan2(a[1], a[0]);
                let mut t1 = fmath::atan2(b[1], b[0]);
                if ccw {
                    while t1 <= t0 {
                        t1 += 2.0 * PI;
                    }
                } else {
                    while t1 >= t0 {
                        t1 -= 2.0 * PI;
                    }
                }
                for k in 1..=per_arc {
                    let t = t0 + (t1 - t0) * k as f64 / per_arc as f64;
                    let (s, c) = fmath::sin_cos(t);
                    pts.push([center[0] + r * c, center[1] + r * s]);
                }
                p0 = to;
            }
        }
    }
    pts
}

/// Polygon `A, Qx, Qy, Ixx, Ixy, Iyy, perimeter` by the shoelace sums.
fn polygon(pts: &[[f64; 2]]) -> [f64; 7] {
    let mut o = [0.0; 7];
    for w in pts.windows(2) {
        let ([x0, y0], [x1, y1]) = (w[0], w[1]);
        let c = x0 * y1 - x1 * y0;
        o[0] += c / 2.0;
        o[1] += c * (x0 + x1) / 6.0;
        o[2] += c * (y0 + y1) / 6.0;
        o[3] += c * (x0 * x0 + x0 * x1 + x1 * x1) / 12.0;
        o[4] += c * (2.0 * x0 * y0 + x0 * y1 + x1 * y0 + 2.0 * x1 * y1) / 24.0;
        o[5] += c * (y0 * y0 + y0 * y1 + y1 * y1) / 12.0;
        o[6] += ((x1 - x0) * (x1 - x0) + (y1 - y0) * (y1 - y0)).sqrt();
    }
    o
}

fn check_profile(what: &str, loops: &[Loop], h: f64) {
    let mut poly = [0.0; 7];
    for lp in loops {
        for (a, b) in poly.iter_mut().zip(polygon(&discretise(lp, 40_000))) {
            *a += b;
        }
    }
    let s = extrusion(loops, h).unwrap();
    let m = &s.moments;
    // the polygon's error is O(1/n^2) in the arc angle: 1e-8 relative with room
    let tol = 1e-8;
    close(m.volume, poly[0] * h, tol, what);
    close(m.first[0], poly[1] * h, tol, what);
    close(m.first[1], poly[2] * h, tol, what);
    close(m.second[0][0], poly[3] * h, tol, what);
    close(m.second[0][1], poly[4] * h, tol, what);
    close(m.second[1][1], poly[5] * h, tol, what);
    close(m.first[2], m.volume * h / 2.0, 1e-13, what);
    close(m.second[2][2], m.volume * h * h / 3.0, 1e-13, what);
    close(m.second[0][2], poly[1] * h * h / 2.0, tol, what);
    close(s.area, 2.0 * poly[0] + poly[6] * h, tol, what);
    close(solid::profile_area(loops).unwrap(), poly[0], tol, what);
}

#[test]
fn line_and_arc_extrusions_match_polygon_moments() {
    // a stadium with a round hole
    let stadium = Loop {
        start: [0.0, 0.0],
        segs: vec![
            Seg::Line { to: [4.0, 0.0] },
            Seg::Arc {
                to: [4.0, 2.0],
                center: [4.0, 1.0],
                ccw: true,
            },
            Seg::Line { to: [0.0, 2.0] },
            Seg::Arc {
                to: [0.0, 0.0],
                center: [0.0, 1.0],
                ccw: true,
            },
        ],
    };
    let hole = Loop {
        start: [2.5, 1.0],
        segs: vec![Seg::Arc {
            to: [2.5, 1.0],
            center: [2.0, 1.0],
            ccw: false,
        }],
    };
    check_profile("stadium", core::slice::from_ref(&stadium), 1.7);
    check_profile("stadium with a hole", &[stadium.clone(), hole.clone()], 1.7);
    let a = solid::profile_area(&[stadium.clone(), hole]).unwrap();
    close(a, 8.0 + PI - PI * 0.25, 1e-13, "stadium area closed form");
    // a quarter-circle bulge on a triangle
    check_profile(
        "bulge",
        &[Loop {
            start: [0.0, 0.0],
            segs: vec![
                Seg::Line { to: [3.0, 0.0] },
                Seg::Arc {
                    to: [4.5, 1.5],
                    center: [3.0, 1.5],
                    ccw: true,
                },
                Seg::Line { to: [0.0, 3.0] },
                Seg::Line { to: [0.0, 0.0] },
            ],
        }],
        2.5,
    );
    // an arc that goes clockwise and one over three quarters of a circle, off the origin
    check_profile(
        "concave and reflex arcs",
        &[Loop {
            start: [10.0, 10.0],
            segs: vec![
                Seg::Line { to: [13.0, 10.0] },
                Seg::Arc {
                    to: [14.5, 8.5],
                    center: [13.0, 8.5],
                    ccw: false,
                },
                Seg::Line { to: [14.5, 12.0] },
                Seg::Line { to: [10.0, 12.0] },
                Seg::Line { to: [10.0, 10.0] },
            ],
        }],
        0.9,
    );
    check_profile(
        "three quarters",
        &[Loop {
            start: [-1.0, 2.0],
            segs: vec![
                Seg::Arc {
                    to: [-2.0, 3.0],
                    center: [-2.0, 2.0],
                    ccw: true,
                },
                Seg::Line { to: [-1.0, 2.0] },
            ],
        }],
        1.3,
    );
}

#[test]
fn a_circle_profile_is_a_cylinder_and_a_rectangle_is_a_block() {
    let circle = Loop {
        start: [1.5, 0.0],
        segs: vec![Seg::Arc {
            to: [1.5, 0.0],
            center: [0.0, 0.0],
            ccw: true,
        }],
    };
    let c = extrusion(&[circle], 4.0).unwrap();
    let want = cylinder(1.5, 4.0).unwrap();
    close_m(&c.moments, &want.moments, 1e-13, "circle extrusion");
    close(c.area, want.area, 1e-13, "circle extrusion area");
    let rect = Loop {
        start: [0.0, 0.0],
        segs: vec![
            Seg::Line { to: [2.0, 0.0] },
            Seg::Line { to: [2.0, 3.0] },
            Seg::Line { to: [0.0, 3.0] },
            Seg::Line { to: [0.0, 0.0] },
        ],
    };
    let r = extrusion(&[rect], 5.0).unwrap();
    let want = block(2.0, 3.0, 5.0).unwrap();
    close_m(&r.moments, &want.moments, 1e-13, "rect extrusion");
    close(r.area, want.area, 1e-13, "rect extrusion area");
}

// ---------------------------------------------------------------------------------------
// Rigid motion

fn solids() -> Vec<(&'static str, Solid)> {
    vec![
        ("block", block(1.0, 2.0, 3.0).unwrap()),
        ("cylinder", cylinder(0.7, 2.0).unwrap()),
        ("cone", cone(1.0, 0.0, 2.0).unwrap()),
        ("frustum", cone(1.0, 0.5, 2.0).unwrap()),
        ("sphere", sphere(1.3).unwrap()),
        ("cap", sphere_cap(1.3, 0.4).unwrap()),
        ("half torus", torus_sector(2.0, 0.5, PI).unwrap()),
        ("torus", torus(2.0, 0.5).unwrap()),
        ("plate", plate(100.0, 100.0, 10.0, &pitch_grid()).unwrap()),
        ("gear", gear(50.0, 10.0, 60, 45.0, 2.12, 0.1234).unwrap()),
    ]
}

fn mul_vec(f: &Frame, v: [f64; 3]) -> [f64; 3] {
    core::array::from_fn(|i| f.x[i] * v[0] + f.y[i] * v[1] + f.z[i] * v[2])
}

#[test]
fn rigid_motion_moves_the_moments_as_it_moves_points() {
    for (name, s) in solids() {
        let c0 = s.moments.centroid();
        let i0 = mass_inertia(&s);
        let tr0 = i0[0][0] + i0[1][1] + i0[2][2];
        let fro0: f64 = i0.iter().flatten().map(|v| v * v).sum();
        for f in frames(21, 4).iter().skip(1) {
            let p = s.place(f);
            assert_eq!(p.volume(), s.volume(), "{name}: volume is unchanged");
            assert_eq!(p.area, s.area, "{name}: area is unchanged");
            // the centroid is a point of the body: it moves as the points do
            let want = to_world(f, c0);
            let got = p.moments.centroid();
            let scale = fmath::cbrt(s.volume()).max(1.0)
                + f.origin[0].abs()
                + f.origin[1].abs()
                + f.origin[2].abs();
            for k in 0..3 {
                assert!(
                    (got[k] - want[k]).abs() <= 1e-12 * scale,
                    "{name}: centroid"
                );
            }
            // the inertia about the centroid is turned: R I R^T; its trace and norm stay
            let i = mass_inertia(&p);
            let tr = i[0][0] + i[1][1] + i[2][2];
            let fro: f64 = i.iter().flatten().map(|v| v * v).sum();
            close(tr, tr0, 1e-9, name);
            close(fro, fro0, 1e-9, name);
            // R I0 R^T entrywise, through the images of the unit vectors
            let cols: Vec<[f64; 3]> = (0..3)
                .map(|k| {
                    let col: [f64; 3] = core::array::from_fn(|r| i0[r][k]);
                    mul_vec(f, col)
                })
                .collect();
            for a in 0..3 {
                for b in 0..3 {
                    // (R I0 R^T)[a][b] = sum_k (R I0)[a][k] R[b][k]
                    let want: f64 = (0..3).map(|k| cols[k][a] * [f.x, f.y, f.z][k][b]).sum();
                    assert!(
                        (i[a][b] - want).abs() <= 1e-9 * fro0.sqrt(),
                        "{name}: inertia [{a}][{b}]"
                    );
                }
            }
            // and placing in two steps is placing by the composed frame
            let g = Frame {
                origin: [0.5, -0.25, 2.0],
                x: [0.0, 1.0, 0.0],
                y: [0.0, 0.0, 1.0],
                z: [1.0, 0.0, 0.0],
            };
            let twice = s.place(&g).place(f);
            let composed = Frame {
                origin: to_world(f, g.origin),
                x: mul_vec(f, g.x),
                y: mul_vec(f, g.y),
                z: mul_vec(f, g.z),
            };
            let once = s.place(&composed);
            close_m(&twice.moments, &once.moments, 1e-11, name);
        }
    }
}

#[test]
fn a_rotated_gear_by_a_tooth_is_the_same_gear() {
    // turning the slots by one pitch permutes them: the moments are those of the unturned
    let gap = 2.12;
    let a = gear(50.0, 10.0, 60, 45.0, gap, 0.0).unwrap();
    let b = gear(50.0, 10.0, 60, 45.0, gap, 2.0 * PI / 60.0).unwrap();
    close_m(&a.moments, &b.moments, 1e-12, "gear turned a pitch");
    close(a.area, b.area, 1e-13, "gear area");
}
