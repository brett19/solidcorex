// Written for solidcorex. Apache-2.0.
//! The CSG oracle against independent truth: plain-`f64` signed distances written here,
//! the analytic closest points, exact rational signs, symmetry, and agreement between
//! equivalent trees.
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
use scx_testkit::analytic::{self, Frame};
use scx_testkit::csg::{
    Csg, CsgError, FeatureKind, Primitive, ProbeSpec, Profile, Seg, Side, box_points,
};
use scx_testkit::rat::Rat;
use scx_testkit::sample;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

const ID: Frame = Frame {
    origin: [0.0; 3],
    x: [1.0, 0.0, 0.0],
    y: [0.0, 1.0, 0.0],
    z: [0.0, 0.0, 1.0],
};

fn at(origin: [f64; 3]) -> Frame {
    Frame { origin, ..ID }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sq(x: f64) -> f64 {
    x * x
}

fn len(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// Local coordinates in an orthonormal frame.
fn local(f: &Frame, p: [f64; 3]) -> [f64; 3] {
    let d = sub(p, f.origin);
    [dot(d, f.x), dot(d, f.y), dot(d, f.z)]
}

fn world(f: &Frame, q: [f64; 3]) -> [f64; 3] {
    core::array::from_fn(|k| f.origin[k] + q[0] * f.x[k] + q[1] * f.y[k] + q[2] * f.z[k])
}

fn leaf(f: &Frame, p: Primitive) -> Csg {
    Csg::leaf(f, p).unwrap()
}

// ------------------------------------------------------------ independent signed distances

/// The signed distance to the box `[lo, hi]` in `N` dimensions.
fn box_sd<const N: usize>(q: [f64; N], lo: [f64; N], hi: [f64; N]) -> f64 {
    let d: [f64; N] = core::array::from_fn(|k| {
        let c = 0.5 * (lo[k] + hi[k]);
        (q[k] - c).abs() - 0.5 * (hi[k] - lo[k])
    });
    let out = d.iter().map(|x| sq(x.max(0.0))).sum::<f64>().sqrt();
    let inside = d.iter().copied().fold(f64::NEG_INFINITY, f64::max).min(0.0);
    out + inside
}

fn seg_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let u = [b[0] - a[0], b[1] - a[1]];
    let w = [p[0] - a[0], p[1] - a[1]];
    let l2 = u[0] * u[0] + u[1] * u[1];
    let t = if l2 > 0.0 {
        ((w[0] * u[0] + w[1] * u[1]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (sq(w[0] - t * u[0]) + sq(w[1] - t * u[1])).sqrt()
}

/// The signed distance to the frustum through its meridian half-plane: the distance to the
/// bottom, slant and top segments (the axis is not boundary), signed by the trapezoid.
fn frustum_sd(q: [f64; 3], r0: f64, r1: f64, h: f64) -> f64 {
    let p = [(q[0] * q[0] + q[1] * q[1]).sqrt(), q[2]];
    let d = seg_dist(p, [0.0, 0.0], [r0, 0.0])
        .min(seg_dist(p, [r0, 0.0], [r1, h]))
        .min(seg_dist(p, [r1, h], [0.0, h]));
    let inside = p[1] > 0.0 && p[1] < h && p[0] < r0 + (r1 - r0) * p[1] / h;
    if inside { -d } else { d }
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Block,
    Cylinder,
    Frustum,
    Sphere,
    Torus,
}

/// The sharpest edge's interior angle: a max of planes reads `d sin(theta / 2)` at
/// distance `d` beyond an edge of angle `theta`, so the band widens by `1 / sin(theta / 2)`.
type Sharp = f64;

/// A random primitive of `kind`, its truth function (local signed distance), a local box,
/// and its sharpest edge.
#[allow(clippy::type_complexity)]
fn random(
    kind: Kind,
    rng: &mut Rng,
) -> (
    Primitive,
    Box<dyn Fn([f64; 3]) -> f64>,
    [f64; 3],
    [f64; 3],
    Sharp,
) {
    let s = |rng: &mut Rng| 0.5 + 4.5 * rng.next_f64();
    match kind {
        Kind::Block => {
            let size = [s(rng), s(rng), s(rng)];
            (
                Primitive::Block { size },
                Box::new(move |q| box_sd(q, [0.0; 3], size)),
                [0.0; 3],
                size,
                FRAC_PI_2,
            )
        }
        Kind::Cylinder => {
            let (r, h) = (s(rng), s(rng));
            (
                Primitive::Cylinder {
                    radius: r,
                    height: h,
                },
                Box::new(move |q| {
                    let rho = (q[0] * q[0] + q[1] * q[1]).sqrt();
                    box_sd([rho, q[2]], [-r, 0.0], [r, h])
                }),
                [-r, -r, 0.0],
                [r, r, h],
                FRAC_PI_2,
            )
        }
        Kind::Frustum => {
            let (r0, h) = (s(rng), s(rng));
            let r1 = if rng.next_f64() < 0.3 { 0.0 } else { s(rng) };
            let m = r0.max(r1);
            (
                Primitive::Frustum {
                    radius0: r0,
                    radius1: r1,
                    height: h,
                },
                Box::new(move |q| frustum_sd(q, r0, r1, h)),
                [-m, -m, 0.0],
                [m, m, h],
                {
                    let a = fmath::atan2(h, r0 - r1);
                    a.min(PI - a)
                },
            )
        }
        Kind::Sphere => {
            let r = s(rng);
            (
                Primitive::Sphere { radius: r },
                Box::new(move |q| len(q) - r),
                [-r; 3],
                [r; 3],
                PI,
            )
        }
        Kind::Torus => {
            let big = s(rng) + 0.5;
            let r = big * (0.1 + 0.8 * rng.next_f64());
            let m = big + r;
            (
                Primitive::Torus {
                    major: big,
                    minor: r,
                },
                Box::new(move |q| {
                    let rho = (q[0] * q[0] + q[1] * q[1]).sqrt();
                    (sq(rho - big) + q[2] * q[2]).sqrt() - r
                }),
                [-m, -m, -r],
                [m, m, r],
                PI,
            )
        }
    }
}

/// Soundness and tightness for one kind: every certain answer is farther than `delta` from
/// the boundary and on the right side; returns the largest `|sd| / delta` over `Band`
/// points, scaled by `sin(theta / 2)` for the sharpest edge, and the share certain.
fn check_kind(kind: Kind, seed: u64) -> (f64, f64) {
    let mut rng = Rng::new(seed);
    let mut reach: f64 = 0.0;
    let (mut certain, mut total) = (0usize, 0usize);
    for _ in 0..40 {
        let f = sample::frame(&mut rng, 10.0);
        let (prim, sd, lo, hi, sharp) = random(kind, &mut rng);
        let widen = fmath::sin_cos(0.5 * sharp).0;
        let csg = leaf(&f, prim);
        for &rel in &[0.3, 0.03, 1e-4] {
            let delta = rel * (hi[2] - lo[2]).max(hi[0] - lo[0]);
            for _ in 0..300 {
                let q: [f64; 3] = core::array::from_fn(|k| {
                    lo[k] + (hi[k] - lo[k]) * (1.4 * rng.next_f64() - 0.2)
                });
                let p = world(&f, q);
                let d = sd(local(&f, p));
                let tol = 1e-12 * 20.0;
                total += 1;
                match csg.side(p, delta) {
                    Side::In => {
                        assert!(d < -delta + tol, "{kind:?} In at sd {d}, delta {delta}");
                        certain += 1;
                    }
                    Side::Out => {
                        assert!(d > delta - tol, "{kind:?} Out at sd {d}, delta {delta}");
                        certain += 1;
                    }
                    Side::Band => reach = reach.max(widen * d.abs() / delta),
                }
            }
        }
    }
    (reach, certain as f64 / total as f64)
}

#[test]
fn each_primitive_is_sound_against_its_distance() {
    for (i, kind) in [
        Kind::Block,
        Kind::Cylinder,
        Kind::Frustum,
        Kind::Sphere,
        Kind::Torus,
    ]
    .into_iter()
    .enumerate()
    {
        let (reach, share) = check_kind(kind, 0x7_0000 + i as u64);
        println!("{kind:?}: band reaches {reach:.3} delta, {share:.3} certain");
        // A turned box's bounding box reaches 3 delta at its corner; past that only the
        // edges' max of planes widens it (scaled out above).
        assert!(reach < 3.0, "{kind:?} band reaches {reach} delta");
    }
}

#[test]
fn band_is_tight_in_an_axis_frame() {
    // Unrotated, the box maps to itself: the block's band is within sqrt 3 delta (the box's
    // corner), the sphere's within a hair of it.
    let csg = leaf(
        &ID,
        Primitive::Block {
            size: [2.0, 3.0, 4.0],
        },
    );
    let ball = leaf(&ID, Primitive::Sphere { radius: 2.0 });
    let mut rng = Rng::new(3);
    let delta = 0.01;
    for _ in 0..20_000 {
        let p: [f64; 3] = core::array::from_fn(|_| 5.0 * rng.next_signed_unit());
        if csg.side(p, delta) == Side::Band {
            let d = box_sd(p, [0.0; 3], [2.0, 3.0, 4.0]).abs();
            assert!(d <= 3f64.sqrt() * delta * (1.0 + 1e-9), "block band at {d}");
        }
        if ball.side(p, delta) == Side::Band {
            let d = (len(p) - 2.0).abs();
            assert!(d <= 3f64.sqrt() * delta * 1.01, "sphere band at {d}");
        }
    }
}

#[test]
fn sphere_and_torus_agree_with_the_analytic_closest_points() {
    let mut rng = Rng::new(11);
    let mut reach = [0.0f64; 2];
    for _ in 0..30 {
        let s = analytic::Sphere {
            frame: sample::frame(&mut rng, 5.0),
            radius: 1.0 + rng.next_f64(),
        };
        let t = sample::torus(&mut rng, sample::TorusKind::Ring);
        let a = leaf(&s.frame, Primitive::Sphere { radius: s.radius });
        let b = leaf(
            &t.frame,
            Primitive::Torus {
                major: t.major,
                minor: t.minor,
            },
        );
        for _ in 0..200 {
            let delta = 0.05 * rng.next_f64();
            let ps = sample::query(&mut rng, &s.frame, 1.5 * s.radius);
            if let Some(foot) = s.closest(ps) {
                let d = foot.distance.to_f64();
                let inside = len(sub(ps, s.frame.origin)) < s.radius;
                match a.side(ps, delta) {
                    Side::In => assert!(inside && d > delta * (1.0 - 1e-9)),
                    Side::Out => assert!(!inside && d > delta * (1.0 - 1e-9)),
                    Side::Band => reach[0] = reach[0].max(d / delta),
                }
            }
            let pt = sample::query(&mut rng, &t.frame, t.major + 2.0 * t.minor);
            if let Some(foot) = t.closest(pt) {
                let d = foot.distance.to_f64();
                let q = local(&t.frame, pt);
                let rho = (q[0] * q[0] + q[1] * q[1]).sqrt();
                let inside = (sq(rho - t.major) + q[2] * q[2]).sqrt() < t.minor;
                match b.side(pt, delta) {
                    Side::In => assert!(inside && d > delta * (1.0 - 1e-9)),
                    Side::Out => assert!(!inside && d > delta * (1.0 - 1e-9)),
                    Side::Band => reach[1] = reach[1].max(d / delta),
                }
            }
        }
    }
    println!(
        "band reaches: sphere {:.3} delta, torus {:.3} delta",
        reach[0], reach[1]
    );
    // A turned box of half-width delta has a bounding box of half-width up to sqrt 3 delta,
    // whose corner is 3 delta away; the torus adds the dependency of rho and z.
    assert!(
        reach[0] <= 3.0 * (1.0 + 1e-9) && reach[1] < 6.0,
        "{reach:?}"
    );
}

#[test]
fn a_skewed_frame_is_sound() {
    // Axes of length 2 and a shear: the sphere becomes an ellipsoid {o + R q : |q| <= 1}.
    let f = Frame {
        origin: [1.0, -2.0, 0.5],
        x: [2.0, 0.0, 0.0],
        y: [0.5, 2.0, 0.0],
        z: [0.0, 0.3, 1.5],
    };
    let csg = leaf(&f, Primitive::Sphere { radius: 1.0 });
    let mut rng = Rng::new(5);
    let (mut seen_in, mut seen_out) = (0, 0);
    for _ in 0..20_000 {
        let p: [f64; 3] = core::array::from_fn(|k| f.origin[k] + 4.0 * rng.next_signed_unit());
        // q = R^-1 (p - o), solved here by Cramer's rule in plain f64.
        let d = sub(p, f.origin);
        let det = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| {
            a[0] * (b[1] * c[2] - b[2] * c[1]) - b[0] * (a[1] * c[2] - a[2] * c[1])
                + c[0] * (a[1] * b[2] - a[2] * b[1])
        };
        let dr = det(f.x, f.y, f.z);
        let q = [
            det(d, f.y, f.z) / dr,
            det(f.x, d, f.z) / dr,
            det(f.x, f.y, d) / dr,
        ];
        let g = len(q) - 1.0;
        match csg.side(p, 1e-3) {
            // In world units the local distance shrinks by at most the largest singular
            // value (< 3), so a certain answer needs |g| > 1e-3 / 3.
            Side::In => {
                assert!(g < -1e-3 / 3.0);
                seen_in += 1;
            }
            Side::Out => {
                assert!(g > 1e-3 / 3.0);
                seen_out += 1;
            }
            Side::Band => assert!(g.abs() < 0.05),
        }
    }
    assert!(seen_in > 100 && seen_out > 100);
}

#[test]
fn half_space_signs_are_exact_near_the_plane() {
    // At width 0 the oracle answers the exact sign of (x cross y) . (p - o) (times the
    // determinant's sign), or Band; never the wrong side, at points ulps from the plane.
    let mut rng = Rng::new(17);
    let r = |x: f64| Rat::from_f64(x).unwrap();
    let mut certain = 0;
    for _ in 0..200 {
        let f = sample::frame(&mut rng, 3.0);
        let csg = leaf(&f, Primitive::HalfSpace);
        for _ in 0..50 {
            let q = [
                4.0 * rng.next_signed_unit(),
                4.0 * rng.next_signed_unit(),
                0.0,
            ];
            let mut p = world(&f, q);
            let k = (rng.next_u64() % 7) as i32 - 3;
            for _ in 0..k.abs() {
                p[2] = if k > 0 {
                    p[2].next_up()
                } else {
                    p[2].next_down()
                };
            }
            let n = [
                r(f.x[1]) * r(f.y[2]) - r(f.x[2]) * r(f.y[1]),
                r(f.x[2]) * r(f.y[0]) - r(f.x[0]) * r(f.y[2]),
                r(f.x[0]) * r(f.y[1]) - r(f.x[1]) * r(f.y[0]),
            ];
            let det =
                n[0].clone() * r(f.z[0]) + n[1].clone() * r(f.z[1]) + n[2].clone() * r(f.z[2]);
            let s = (0..3).fold(Rat::zero(), |acc, i| {
                acc + n[i].clone() * (r(p[i]) - r(f.origin[i]))
            });
            let sign = s.signum() * det.signum();
            match csg.side(p, 0.0) {
                Side::In => {
                    assert_eq!(sign, -1);
                    certain += 1;
                }
                Side::Out => {
                    assert_eq!(sign, 1);
                    certain += 1;
                }
                Side::Band => {}
            }
        }
    }
    assert!(certain > 0);
}

// ------------------------------------------------------------------ symmetry

#[test]
fn mirrored_points_get_mirrored_answers() {
    let shapes = [
        leaf(&ID, Primitive::Sphere { radius: 1.5 }),
        leaf(
            &ID,
            Primitive::Torus {
                major: 2.0,
                minor: 0.5,
            },
        ),
        leaf(
            &at([0.0, 0.0, -1.0]),
            Primitive::Cylinder {
                radius: 1.2,
                height: 2.0,
            },
        ),
        leaf(
            &at([-1.0, -1.5, -2.0]),
            Primitive::Block {
                size: [2.0, 3.0, 4.0],
            },
        ),
    ];
    let mut rng = Rng::new(23);
    for csg in &shapes {
        for _ in 0..5_000 {
            let p: [f64; 3] = core::array::from_fn(|_| 3.0 * rng.next_signed_unit());
            let delta = 0.1 * rng.next_f64();
            let s = csg.side(p, delta);
            for m in [
                [-1.0, 1.0, 1.0],
                [1.0, -1.0, 1.0],
                [1.0, 1.0, -1.0],
                [-1.0, -1.0, -1.0],
            ] {
                let q = [m[0] * p[0], m[1] * p[1], m[2] * p[2]];
                assert_eq!(csg.side(q, delta), s, "{p:?} mirrored by {m:?}");
            }
        }
    }
}

fn no_contradiction(a: Side, b: Side) -> bool {
    !matches!((a, b), (Side::In, Side::Out) | (Side::Out, Side::In))
}

#[test]
fn a_frame_turned_about_the_axis_changes_nothing() {
    let mut rng = Rng::new(29);
    for _ in 0..20 {
        let f = sample::frame(&mut rng, 4.0);
        let (s, c) = fmath::sin_cos(TAU * rng.next_f64());
        let g = Frame {
            x: core::array::from_fn(|k| c * f.x[k] + s * f.y[k]),
            y: core::array::from_fn(|k| -s * f.x[k] + c * f.y[k]),
            ..f
        };
        for prim in [
            Primitive::Torus {
                major: 2.0,
                minor: 0.7,
            },
            Primitive::Cylinder {
                radius: 1.0,
                height: 2.0,
            },
            Primitive::Frustum {
                radius0: 1.5,
                radius1: 0.5,
                height: 2.0,
            },
        ] {
            let (a, b) = (leaf(&f, prim.clone()), leaf(&g, prim));
            let (mut certain, mut n) = (0, 0);
            for _ in 0..2_000 {
                let p = world(&f, core::array::from_fn(|_| 3.0 * rng.next_signed_unit()));
                let (sa, sb) = (a.side(p, 1e-3), b.side(p, 1e-3));
                assert!(no_contradiction(sa, sb));
                n += 1;
                certain += usize::from(sa == sb && sa != Side::Band);
            }
            assert!(certain * 10 > n * 9);
        }
    }
}

// ------------------------------------------------------------------ equivalent trees

/// No point is certainly In for one tree and certainly Out for another; returns the share
/// of points where all agree with certainty.
fn agree(trees: &[Csg], lo: [f64; 3], hi: [f64; 3], delta: f64, seed: u64) -> f64 {
    let pts = box_points(lo, hi, 0.2, 4_000, seed);
    let mut both = 0;
    for p in &pts {
        let s: Vec<Side> = trees.iter().map(|t| t.side(*p, delta)).collect();
        for a in &s {
            for b in &s {
                assert!(no_contradiction(*a, *b), "{p:?}: {s:?}");
            }
        }
        both += usize::from(s.iter().all(|x| *x == s[0] && *x != Side::Band));
    }
    both as f64 / pts.len() as f64
}

/// The frame `f` re-axed so that its `z` is `n` (for half-spaces and slabs).
fn facing(origin: [f64; 3], a: [f64; 3], b: [f64; 3], n: [f64; 3]) -> Frame {
    Frame {
        origin,
        x: a,
        y: b,
        z: n,
    }
}

fn neg(v: [f64; 3]) -> [f64; 3] {
    v.map(|c| -c)
}

#[test]
fn a_block_is_six_half_spaces_three_slabs_and_an_extruded_rectangle() {
    let mut rng = Rng::new(31);
    for _ in 0..20 {
        let f = sample::frame(&mut rng, 5.0);
        let [a, b, c] = [
            1.0 + rng.next_f64(),
            1.0 + 2.0 * rng.next_f64(),
            0.5 + rng.next_f64(),
        ];
        let block = leaf(&f, Primitive::Block { size: [a, b, c] });
        let corner = world(&f, [a, b, c]);
        let halves = [
            facing(f.origin, f.y, f.z, neg(f.x)),
            facing(f.origin, f.z, f.x, neg(f.y)),
            facing(f.origin, f.x, f.y, neg(f.z)),
            facing(corner, f.y, f.z, f.x),
            facing(corner, f.z, f.x, f.y),
            facing(corner, f.x, f.y, f.z),
        ]
        .map(|g| leaf(&g, Primitive::HalfSpace));
        let six = halves.into_iter().reduce(Csg::intersection).unwrap();
        let slabs = [(f.y, f.z, f.x, a), (f.z, f.x, f.y, b), (f.x, f.y, f.z, c)]
            .map(|(u, v, n, h)| leaf(&facing(f.origin, u, v, n), Primitive::Slab { height: h }))
            .into_iter()
            .reduce(Csg::intersection)
            .unwrap();
        let rect = Profile::new(&[vec![
            Seg::Line {
                from: [0.0, 0.0],
                to: [a, 0.0],
            },
            Seg::Line {
                from: [a, 0.0],
                to: [a, b],
            },
            Seg::Line {
                from: [a, b],
                to: [0.0, b],
            },
            Seg::Line {
                from: [0.0, b],
                to: [0.0, 0.0],
            },
        ]])
        .unwrap();
        let ext = leaf(
            &f,
            Primitive::Extrusion {
                profile: rect,
                height: c,
            },
        );
        let (lo, hi) = block.bounds().unwrap();
        let share = agree(&[block, six, slabs, ext], lo, hi, 1e-3, rng.next_u64());
        assert!(share > 0.9, "agreement {share}");
    }
}

#[test]
fn a_cylinder_is_an_extruded_circle_two_half_arcs_and_a_straight_frustum() {
    let mut rng = Rng::new(37);
    for _ in 0..20 {
        let f = sample::frame(&mut rng, 5.0);
        let (r, h) = (0.5 + rng.next_f64(), 0.5 + 2.0 * rng.next_f64());
        let cyl = leaf(
            &f,
            Primitive::Cylinder {
                radius: r,
                height: h,
            },
        );
        let circle = Profile::new(&[vec![Seg::Circle {
            center: [0.0, 0.0],
            radius: r,
        }]])
        .unwrap();
        let t0 = TAU * rng.next_f64();
        let halves = Profile::new(&[vec![
            Seg::Arc {
                center: [0.0, 0.0],
                radius: r,
                start: t0,
                sweep: PI,
            },
            Seg::Arc {
                center: [0.0, 0.0],
                radius: r,
                start: t0 + PI,
                sweep: PI,
            },
        ]])
        .unwrap();
        let cw = Profile::new(&[vec![Seg::Arc {
            center: [0.0, 0.0],
            radius: r,
            start: t0,
            sweep: -TAU,
        }]])
        .unwrap();
        let trees = [
            cyl,
            leaf(
                &f,
                Primitive::Extrusion {
                    profile: circle,
                    height: h,
                },
            ),
            leaf(
                &f,
                Primitive::Extrusion {
                    profile: halves,
                    height: h,
                },
            ),
            leaf(
                &f,
                Primitive::Extrusion {
                    profile: cw,
                    height: h,
                },
            ),
            leaf(
                &f,
                Primitive::Frustum {
                    radius0: r,
                    radius1: r,
                    height: h,
                },
            ),
        ];
        let (lo, hi) = trees[0].bounds().unwrap();
        let share = agree(&trees, lo, hi, 1e-3, rng.next_u64());
        assert!(share > 0.9, "agreement {share}");
    }
}

#[test]
fn a_rounded_rectangle_is_two_blocks_and_four_cylinders() {
    let (w, l, rr, h) = (4.0, 3.0, 0.75, 1.0);
    let mut rng = Rng::new(41);
    for _ in 0..10 {
        let f = sample::frame(&mut rng, 3.0);
        let seg = |a: [f64; 2], b: [f64; 2]| Seg::Line { from: a, to: b };
        let arc = |c: [f64; 2], s: f64| Seg::Arc {
            center: c,
            radius: rr,
            start: s,
            sweep: FRAC_PI_2,
        };
        // The arcs' ends are rounded sines and cosines: the lines start where they end.
        let end = |c: [f64; 2], t: f64| {
            let (s, co) = fmath::sin_cos(t);
            [c[0] + rr * co, c[1] + rr * s]
        };
        let cs = [[w - rr, rr], [w - rr, l - rr], [rr, l - rr], [rr, rr]];
        let starts = [-FRAC_PI_2, 0.0, FRAC_PI_2, PI];
        let mut segs = Vec::new();
        for i in 0..4 {
            let (c, s) = (cs[i], starts[i]);
            segs.push(arc(c, s));
            let (nc, ns) = (cs[(i + 1) % 4], starts[(i + 1) % 4]);
            segs.push(seg(end(c, s + FRAC_PI_2), end(nc, ns)));
        }
        let profile = Profile::new(&[segs]).unwrap();
        let ext = leaf(&f, Primitive::Extrusion { profile, height: h });
        let blocks = leaf(
            &Frame {
                origin: world(&f, [rr, 0.0, 0.0]),
                ..f
            },
            Primitive::Block {
                size: [w - 2.0 * rr, l, h],
            },
        )
        .union(leaf(
            &Frame {
                origin: world(&f, [0.0, rr, 0.0]),
                ..f
            },
            Primitive::Block {
                size: [w, l - 2.0 * rr, h],
            },
        ));
        let union = cs.iter().fold(blocks, |acc, c| {
            acc.union(leaf(
                &Frame {
                    origin: world(&f, [c[0], c[1], 0.0]),
                    ..f
                },
                Primitive::Cylinder {
                    radius: rr,
                    height: h,
                },
            ))
        });
        let (lo, hi) = ext.bounds().unwrap();
        let share = agree(&[ext, union], lo, hi, 1e-3, rng.next_u64());
        assert!(share > 0.85, "agreement {share}");
    }
}

#[test]
fn union_commutes_and_de_morgan_holds_bit_for_bit() {
    let mut rng = Rng::new(43);
    for _ in 0..20 {
        let a = || {
            leaf(
                &sample::frame(&mut Rng::new(1), 1.0),
                Primitive::Sphere { radius: 1.0 },
            )
        };
        let b = || {
            leaf(
                &sample::frame(&mut Rng::new(2), 1.0),
                Primitive::Torus {
                    major: 1.0,
                    minor: 0.4,
                },
            )
        };
        let pairs = [
            (a().union(b()), b().union(a())),
            (a().intersection(b()), b().intersection(a())),
            (
                a().union(b()).complement(),
                a().complement().intersection(b().complement()),
            ),
            (
                a().intersection(b()).complement(),
                a().complement().union(b().complement()),
            ),
            (a().difference(b()), a().intersection(b().complement())),
        ];
        for _ in 0..500 {
            let p: [f64; 3] = core::array::from_fn(|_| 2.5 * rng.next_signed_unit());
            let d = 0.05 * rng.next_f64();
            for (x, y) in &pairs {
                assert_eq!(x.enclose(p, d), y.enclose(p, d));
            }
        }
    }
}

// ------------------------------------------------------------------ profiles

/// The exact crossing parity of an integer polygon about a point with half-integer
/// coordinates (never at a vertex's level), in `i128`; `None` on an edge.
fn crossing_inside(poly: &[[i64; 2]], p2: [i64; 2]) -> Option<bool> {
    // Work at twice the scale so p2 = 2 p is odd in both coordinates.
    let mut inside = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (ay, by) = (2 * a[1] as i128, 2 * b[1] as i128);
        let (ax, bx) = (2 * a[0] as i128, 2 * b[0] as i128);
        let (px, py) = (p2[0] as i128, p2[1] as i128);
        if (ay > py) != (by > py) {
            // x of the crossing compared with px, exactly: (px - ax)(by - ay) < (py - ay)(bx - ax)
            let lhs = (px - ax) * (by - ay);
            let rhs = (py - ay) * (bx - ax);
            if lhs == rhs {
                return None;
            }
            if (by > ay && lhs < rhs) || (by < ay && lhs > rhs) {
                inside = !inside;
            }
        }
    }
    Some(inside)
}

#[test]
fn winding_parity_matches_an_exact_crossing_test() {
    let mut rng = Rng::new(47);
    let mut checked = 0;
    for _ in 0..60 {
        let n = 3 + (rng.next_u64() % 12) as usize;
        // A star-shaped polygon (may be non-convex), integer vertices.
        let poly: Vec<[i64; 2]> = (0..n)
            .map(|i| {
                let t = TAU * (i as f64 + 0.8 * rng.next_f64()) / n as f64;
                let r = 10.0 + 30.0 * rng.next_f64();
                let (s, c) = fmath::sin_cos(t);
                [(r * c).round() as i64, (r * s).round() as i64]
            })
            .collect();
        let segs: Vec<Seg> = (0..n)
            .map(|i| {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                Seg::Line {
                    from: [a[0] as f64, a[1] as f64],
                    to: [b[0] as f64, b[1] as f64],
                }
            })
            .collect();
        // Reversing the loop and adding a far hole loop keep parity.
        let rev: Vec<Seg> = segs
            .iter()
            .rev()
            .map(|s| match *s {
                Seg::Line { from, to } => Seg::Line { from: to, to: from },
                other => other,
            })
            .collect();
        let profile = Profile::new(&[segs]).unwrap();
        let reversed = Profile::new(&[rev]).unwrap();
        for _ in 0..400 {
            let p2 = [
                2 * (rng.next_u64() % 100) as i64 - 99,
                2 * (rng.next_u64() % 100) as i64 - 99,
            ];
            let p = [p2[0] as f64 / 2.0, p2[1] as f64 / 2.0];
            let Some(truth) = crossing_inside(&poly, p2) else {
                continue;
            };
            assert_eq!(profile.inside(p), truth, "{poly:?} at {p:?}");
            assert_eq!(reversed.inside(p), truth);
            checked += 1;
        }
    }
    assert!(checked > 0);
}

#[test]
fn nested_loops_make_a_hole() {
    let ring = Profile::new(&[
        vec![Seg::Circle {
            center: [0.0, 0.0],
            radius: 2.0,
        }],
        vec![Seg::Arc {
            center: [0.0, 0.0],
            radius: 1.0,
            start: 0.3,
            sweep: -TAU,
        }],
    ])
    .unwrap();
    assert!(!ring.inside([0.0, 0.0]));
    assert!(ring.inside([1.5, 0.0]));
    assert!(ring.inside([0.0, -1.9]));
    assert!(!ring.inside([2.1, 0.0]));
    let d = ring.signed_distance([1.25, 0.0]).to_f64();
    assert!((d + 0.25).abs() < 1e-15);
    // An extruded ring is a cylinder minus a cylinder.
    let f = sample::frame(&mut Rng::new(53), 2.0);
    let tube = leaf(
        &f,
        Primitive::Extrusion {
            profile: ring,
            height: 1.0,
        },
    );
    let diff = leaf(
        &f,
        Primitive::Cylinder {
            radius: 2.0,
            height: 1.0,
        },
    )
    .difference(leaf(
        &f,
        Primitive::Cylinder {
            radius: 1.0,
            height: 1.0,
        },
    ));
    let (lo, hi) = tube.bounds().unwrap();
    assert!(agree(&[tube, diff], lo, hi, 1e-3, 59) > 0.9);
}

#[test]
fn arc_distances_are_the_closed_forms() {
    // A quarter arc from angle 0 to pi/2 closed by two radii: a pie slice.
    let r = 2.0;
    let pie = Profile::new(&[vec![
        Seg::Line {
            from: [0.0, 0.0],
            to: [r, 0.0],
        },
        Seg::Arc {
            center: [0.0, 0.0],
            radius: r,
            start: 0.0,
            sweep: FRAC_PI_2,
        },
        Seg::Line {
            from: [0.0, r],
            to: [0.0, 0.0],
        },
    ]])
    .unwrap();
    let mut rng = Rng::new(61);
    for _ in 0..2_000 {
        let p = [3.0 * rng.next_signed_unit(), 3.0 * rng.next_signed_unit()];
        let (rho, t) = ((p[0] * p[0] + p[1] * p[1]).sqrt(), fmath::atan2(p[1], p[0]));
        let to_arc = if (0.0..=FRAC_PI_2).contains(&t) {
            (rho - r).abs()
        } else {
            seg_dist(p, [r, 0.0], [r, 0.0]).min(seg_dist(p, [0.0, r], [0.0, r]))
        };
        let d =
            to_arc
                .min(seg_dist(p, [0.0, 0.0], [r, 0.0]))
                .min(seg_dist(p, [0.0, r], [0.0, 0.0]));
        let inside = p[0] > 0.0 && p[1] > 0.0 && rho < r;
        let got = pie.signed_distance(p).to_f64();
        assert!(
            (got - if inside { -d } else { d }).abs() < 1e-12,
            "{p:?}: {got} vs {d}"
        );
    }
}

// ------------------------------------------------------------------ points on purpose

fn spec(seed: u64) -> ProbeSpec {
    ProbeSpec {
        seed,
        per_face: 8,
        coincidence: 1e-6,
        extent: 5.0,
    }
}

#[test]
fn probes_on_a_block_sit_on_its_features_with_offsets_on_the_right_sides() {
    let f = sample::frame(&mut Rng::new(67), 3.0);
    let size = [1.0, 2.0, 3.0];
    let csg = leaf(&f, Primitive::Block { size });
    let c = 1e-6;
    let probes = csg.probes(&spec(1));
    let count = |k: FeatureKind| {
        probes
            .iter()
            .filter(|p| p.feature.kind == k && p.offset == 0.0)
            .count()
    };
    assert_eq!(count(FeatureKind::Face), 6 * 8);
    assert_eq!(count(FeatureKind::Edge), 12 * 8);
    assert_eq!(count(FeatureKind::Vertex), 8);
    for p in &probes {
        let d = box_sd(local(&f, p.point), [0.0; 3], size);
        assert!((d - p.offset).abs() < 1e-12 || p.feature.kind != FeatureKind::Face);
        if p.offset == 0.0 {
            assert!(d.abs() < 1e-12);
            assert_eq!(csg.side(p.point, 3f64.sqrt() * c), Side::Band);
        } else if p.offset.abs() == 10.0 * c {
            // Outward normals of a convex solid: plus is out, minus in. Off a face that is
            // certain; off an edge or vertex along one face's normal the point slides along
            // the other face, so inward may stay in the band.
            let want = if p.offset > 0.0 { Side::Out } else { Side::In };
            let got = csg.side(p.point, c);
            if p.feature.kind == FeatureKind::Face || p.offset > 0.0 {
                assert_eq!(got, want, "{p:?}");
            } else {
                assert!(got == want || got == Side::Band, "{p:?}");
            }
        }
    }
}

#[test]
fn every_on_point_is_in_the_band_for_each_primitive() {
    let profile = Profile::new(&[vec![
        Seg::Line {
            from: [0.0, 0.0],
            to: [2.0, 0.0],
        },
        Seg::Arc {
            center: [2.0, 1.0],
            radius: 1.0,
            start: -FRAC_PI_2,
            sweep: PI,
        },
        Seg::Line {
            from: [2.0, 2.0],
            to: [0.0, 2.0],
        },
        Seg::Line {
            from: [0.0, 2.0],
            to: [0.0, 0.0],
        },
    ]])
    .unwrap();
    let mut rng = Rng::new(71);
    let prims = [
        Primitive::HalfSpace,
        Primitive::Slab { height: 1.0 },
        Primitive::Block {
            size: [1.0, 1.5, 2.0],
        },
        Primitive::Cylinder {
            radius: 1.0,
            height: 2.0,
        },
        Primitive::Frustum {
            radius0: 1.5,
            radius1: 0.0,
            height: 2.0,
        },
        Primitive::Frustum {
            radius0: 0.5,
            radius1: 1.5,
            height: 1.0,
        },
        Primitive::Sphere { radius: 1.3 },
        Primitive::Torus {
            major: 2.0,
            minor: 0.5,
        },
        Primitive::Extrusion {
            profile,
            height: 1.5,
        },
    ];
    let c = 1e-6;
    for prim in prims {
        let f = sample::frame(&mut rng, 3.0);
        let csg = leaf(&f, prim.clone());
        let probes = csg.probes(&spec(rng.next_u64()));
        let on: Vec<_> = probes.iter().filter(|p| p.offset == 0.0).collect();
        assert!(on.len() >= 8, "{prim:?}: {} on-points", on.len());
        for p in &on {
            assert_eq!(
                csg.side(p.point, 3f64.sqrt() * c),
                Side::Band,
                "{prim:?} {p:?}"
            );
        }
        // Each face offset at 10 coincidences is certain at width one, on opposite sides.
        for p in probes
            .iter()
            .filter(|p| p.feature.kind == FeatureKind::Face && p.offset.abs() == 10.0 * c)
        {
            let back = [0, 1, 2].map(|k| p.point[k] - 2.0 * p.offset * p.dir[k]);
            let (s, t) = (csg.side(p.point, c), csg.side(back, c));
            assert!(
                s != Side::Band && t != Side::Band && s != t,
                "{prim:?} {p:?}: {s:?} {t:?}"
            );
        }
    }
}

#[test]
fn a_coplanar_union_has_no_probes_on_its_shared_face() {
    let a = leaf(
        &ID,
        Primitive::Block {
            size: [10.0, 10.0, 10.0],
        },
    );
    let b = leaf(
        &at([10.0, 0.0, 0.0]),
        Primitive::Block {
            size: [10.0, 10.0, 10.0],
        },
    );
    let csg = a.union(b);
    // The documented cost: the shared face is Band where the truth is In.
    assert_eq!(csg.side([10.0, 5.0, 5.0], 1e-3), Side::Band);
    assert_eq!(csg.side([10.01, 5.0, 5.0], 1e-3), Side::In);
    let probes = csg.probes(&spec(3));
    assert!(!probes.is_empty());
    for p in probes.iter().filter(|p| p.offset == 0.0) {
        let inner = (p.point[1] - 5.0).abs() < 4.9 && (p.point[2] - 5.0).abs() < 4.9;
        assert!(!((p.point[0] - 10.0).abs() < 1e-9 && inner), "{p:?}");
    }
}

#[test]
fn a_hole_through_a_block_gives_meets_on_both_surfaces() {
    let f = sample::frame(&mut Rng::new(73), 2.0);
    let block = leaf(
        &f,
        Primitive::Block {
            size: [4.0, 4.0, 2.0],
        },
    );
    let hole = leaf(
        &Frame {
            origin: world(&f, [2.0, 2.0, -1.0]),
            ..f
        },
        Primitive::Cylinder {
            radius: 1.0,
            height: 4.0,
        },
    );
    let csg = block.difference(hole);
    let probes = csg.probes(&spec(5));
    let meets: Vec<_> = probes
        .iter()
        .filter(|p| matches!(p.feature.kind, FeatureKind::Meet { .. }) && p.offset == 0.0)
        .collect();
    assert!(meets.len() >= 4, "{} meets", meets.len());
    for p in &meets {
        let q = local(&f, p.point);
        let rho = (sq(q[0] - 2.0) + sq(q[1] - 2.0)).sqrt();
        assert!((rho - 1.0).abs() < 1e-9, "{q:?}");
        assert!(q[2].abs() < 1e-9 || (q[2] - 2.0).abs() < 1e-9, "{q:?}");
        assert_eq!(csg.side(p.point, 3f64.sqrt() * 1e-6), Side::Band);
    }
    // The cylinder's faces outside the block give nothing.
    for p in probes.iter().filter(|p| p.offset == 0.0) {
        let q = local(&f, p.point);
        assert!(q[2] > -1e-9 && q[2] < 2.0 + 1e-9, "{p:?}");
    }
}

#[test]
fn probes_are_deterministic_and_follow_the_seed() {
    let csg = leaf(
        &ID,
        Primitive::Torus {
            major: 2.0,
            minor: 0.5,
        },
    )
    .union(leaf(
        &at([2.0, 0.0, 0.0]),
        Primitive::Sphere { radius: 0.8 },
    ));
    let (a, b, c) = (
        csg.probes(&spec(9)),
        csg.probes(&spec(9)),
        csg.probes(&spec(10)),
    );
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert!(
        a.iter()
            .any(|p| matches!(p.feature.kind, FeatureKind::Meet { .. }))
    );
}

// ------------------------------------------------------------------ census and errors

#[test]
fn a_recipe_keeps_most_points_certain() {
    let block = leaf(
        &ID,
        Primitive::Block {
            size: [10.0, 8.0, 4.0],
        },
    );
    let hole = leaf(
        &at([5.0, 4.0, -1.0]),
        Primitive::Cylinder {
            radius: 2.0,
            height: 6.0,
        },
    );
    let boss = leaf(
        &at([2.0, 2.0, 4.0]),
        Primitive::Frustum {
            radius0: 1.5,
            radius1: 1.0,
            height: 2.0,
        },
    );
    let csg = block.difference(hole).union(boss);
    let (lo, hi) = csg.bounds().unwrap();
    let pts = box_points(lo, hi, 0.1, 1_000, 77);
    let census = csg.census(&pts, 3f64.sqrt() * 1e-6);
    assert_eq!(census.inside + census.outside + census.band, 1_000);
    assert!(census.certain_fraction() >= 0.95, "{census:?}");
    assert!(census.inside > 100 && census.outside > 100);
    assert_eq!(pts, box_points(lo, hi, 0.1, 1_000, 77));
    for p in &pts {
        for k in 0..3 {
            let (c, h) = (0.5 * (lo[k] + hi[k]), 0.55 * (hi[k] - lo[k]));
            assert!((p[k] - c).abs() <= h * (1.0 + 1e-12));
        }
    }
}

#[test]
fn bad_recipes_are_refused() {
    let flat = Frame {
        z: [1.0, 0.0, 0.0],
        ..ID
    };
    assert_eq!(
        Csg::leaf(&flat, Primitive::HalfSpace).err(),
        Some(CsgError::Frame)
    );
    let nan = Frame {
        origin: [f64::NAN, 0.0, 0.0],
        ..ID
    };
    assert_eq!(
        Csg::leaf(&nan, Primitive::HalfSpace).err(),
        Some(CsgError::Frame)
    );
    for p in [
        Primitive::Sphere { radius: 0.0 },
        Primitive::Slab { height: -1.0 },
        Primitive::Block {
            size: [1.0, f64::INFINITY, 1.0],
        },
        Primitive::Frustum {
            radius0: 0.0,
            radius1: 0.0,
            height: 1.0,
        },
        Primitive::Frustum {
            radius0: -0.5,
            radius1: 1.0,
            height: 1.0,
        },
        Primitive::Torus {
            major: 1.0,
            minor: 0.0,
        },
    ] {
        assert!(matches!(Csg::leaf(&ID, p), Err(CsgError::Size { .. })));
    }
    assert_eq!(Profile::new(&[]).err(), Some(CsgError::ProfileShape));
    assert_eq!(Profile::new(&[vec![]]).err(), Some(CsgError::ProfileShape));
    let circle = Seg::Circle {
        center: [0.0, 0.0],
        radius: 1.0,
    };
    let line = Seg::Line {
        from: [0.0, 0.0],
        to: [1.0, 0.0],
    };
    assert_eq!(
        Profile::new(&[vec![circle, line]]).err(),
        Some(CsgError::ProfileShape)
    );
    assert!(matches!(
        Profile::new(&[vec![
            line,
            Seg::Line {
                from: [1.0, 0.0],
                to: [0.0, 1.0]
            }
        ]]),
        Err(CsgError::ProfileOpen { .. })
    ));
    let arc = Seg::Arc {
        center: [0.0, 0.0],
        radius: 1.0,
        start: 0.0,
        sweep: 7.0,
    };
    assert!(matches!(
        Profile::new(&[vec![arc]]),
        Err(CsgError::Size { .. })
    ));
    // Non-finite points and widths make no claim.
    let ball = leaf(&ID, Primitive::Sphere { radius: 1.0 });
    assert_eq!(ball.side([f64::NAN, 0.0, 0.0], 0.1), Side::Band);
    assert_eq!(ball.side([0.0; 3], -1.0), Side::Band);
    assert_eq!(ball.side([1e200, 0.0, 0.0], 0.1), Side::Band);
    assert_eq!(ball.side([0.0; 3], 0.1), Side::In);
}
