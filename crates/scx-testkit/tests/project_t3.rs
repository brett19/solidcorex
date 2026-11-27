// Written for solidcorex. Apache-2.0.
//! T3 (research 63 §8) checked against facts that do not use its closed forms:
//!
//! - **Certified orthogonality.** At every foot the implicit equation of the ideal surface of
//!   revolution, and the cross product of the residual `q - P` with its gradient, are
//!   evaluated in interval arithmetic over the foot's certified box (`point` give or take
//!   `point_radius`). Each must hold zero, and each must be narrow, so a foot off the
//!   surface, or with a residual not along the normal, by more than the box fails. Circles of
//!   feet are checked the same way at points of them.
//! - **Rotational symmetry.** Turning the query a quarter turn about the axis (exactly, in a
//!   frame of signed unit vectors), or mirroring it in a meridian plane (and, for the torus,
//!   in the equator), leaves the multiset of distance enclosures and kinds bit for bit; the
//!   points turn with it, the angle `u` moves by the turn and `v` stays. A turn by any angle
//!   (rounded) agrees to rounding.
//! - **Dense sampling.** No point of a dense grid of the parameterisation (and a fine grid
//!   round each minimum) is nearer than the certified nearest's lower end, for random,
//!   self-intersecting (horn and spindle) and degenerate queries alike.
//! - **Axis and apex.** Queries on the axis give circles, not points, and a query at or near
//!   the apex or a pinch is handled.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp
)]

use scx_num::hash::Rng;
use scx_testkit::Rat;
use scx_testkit::analytic::{Cone, Frame, Torus, V3};
use scx_testkit::bound::{self, Bound};
use scx_testkit::dd::{self, Dd};
use scx_testkit::project::{Kind, Locus, Nappes, Projection, StationaryCircle, Uv};
use scx_testkit::sample::{self, TorusKind};

// ---------------------------------------------------------------- exact helpers

type B3 = [Bound; 3];

fn rat(x: f64) -> Rat {
    Rat::from_f64(x).unwrap()
}

fn dd_rat(x: Dd) -> Rat {
    rat(x.hi) + rat(x.lo)
}

fn exact3(v: [f64; 3]) -> B3 {
    v.map(|c| Bound::exact(rat(c)))
}

/// The box `p +- radius` in each coordinate (it holds the Euclidean ball).
fn boxed(p: V3, radius: f64) -> B3 {
    let e = rat(radius);
    p.map(|c| {
        let m = dd_rat(c);
        Bound::new(&m - &e, &m + &e).unwrap()
    })
}

fn bdot(a: &B3, b: &B3) -> Bound {
    a[0].mul(&b[0]).add(&a[1].mul(&b[1])).add(&a[2].mul(&b[2]))
}

fn bsub(a: &B3, b: &B3) -> B3 {
    [a[0].sub(&b[0]), a[1].sub(&b[1]), a[2].sub(&b[2])]
}

fn badd(a: &B3, b: &B3) -> B3 {
    [a[0].add(&b[0]), a[1].add(&b[1]), a[2].add(&b[2])]
}

fn bscale(a: &B3, k: &Bound) -> B3 {
    [a[0].mul(k), a[1].mul(k), a[2].mul(k)]
}

fn bcross(a: &B3, b: &B3) -> B3 {
    [
        a[1].mul(&b[2]).sub(&a[2].mul(&b[1])),
        a[2].mul(&b[0]).sub(&a[0].mul(&b[2])),
        a[0].mul(&b[1]).sub(&a[1].mul(&b[0])),
    ]
}

fn holds_zero(b: &Bound) -> bool {
    b.contains(&Rat::zero())
}

fn width(b: &Bound) -> f64 {
    bound::up(&b.width())
}

// ---------------------------------------------------------------- the ideal surfaces

/// The implicit form `F` of an ideal surface of revolution about `(O, z)` and its gradient,
/// over a box; and, for a cone, the signed radius `r + t h` there (its nappe).
trait Revolution {
    fn frame(&self) -> &Frame;
    fn implicit(&self, p: &B3) -> (Bound, B3, Option<Bound>);
}

impl Revolution for Torus {
    fn frame(&self) -> &Frame {
        &self.frame
    }

    /// `F = (|d|^2 + R^2 - r^2)^2 - 4 R^2 rho^2`, `rho^2 = |d|^2 - (d.z)^2 / z.z`; no root
    /// taken, so `z` need not be a unit vector.
    fn implicit(&self, p: &B3) -> (Bound, B3, Option<Bound>) {
        let (o, z) = (exact3(self.frame.origin), exact3(self.frame.z));
        let zz_inv = bdot(&z, &z).lo().recip().unwrap();
        let d = bsub(p, &o);
        let along = bdot(&d, &z).mul_rat(&zz_inv);
        let radial = bsub(&d, &bscale(&z, &along));
        let dd = bdot(&d, &d);
        let rho2 = bdot(&radial, &radial);
        let (big2, small2) = (
            rat(self.major) * rat(self.major),
            rat(self.minor) * rat(self.minor),
        );
        let s = dd.add(&Bound::exact(&big2 - &small2));
        let f = s.sqr().sub(&rho2.mul_rat(&(Rat::from(4) * &big2)));
        let grad = badd(
            &bscale(&d, &s.mul_rat(&Rat::from(4))),
            &bscale(&radial, &Bound::exact(Rat::from(-8) * &big2)),
        );
        (f, grad, None)
    }
}

impl Revolution for Cone {
    fn frame(&self) -> &Frame {
        &self.frame
    }

    /// `F = rho^2 - (r + t h)^2`, `h = d . z / |z|`, `t = tan a` certified: both nappes.
    fn implicit(&self, p: &B3) -> (Bound, B3, Option<Bound>) {
        let (o, z) = (exact3(self.frame.origin), exact3(self.frame.z));
        let zlen = bdot(&z, &z).sqrt(240).unwrap();
        let zu = bscale(&z, &zlen.recip(240).unwrap());
        let t = bound::tan(self.semi_angle, 240).unwrap();
        let d = bsub(p, &o);
        let h = bdot(&d, &zu);
        let radial = bsub(&d, &bscale(&zu, &h));
        let w = Bound::exact(rat(self.radius)).add(&t.mul(&h));
        let f = bdot(&radial, &radial).sub(&w.sqr());
        let two = Rat::from(2);
        let grad = bsub(
            &bscale(&radial, &Bound::exact(two.clone())),
            &bscale(&zu, &t.mul(&w).mul_rat(&two)),
        );
        (f, grad, Some(w))
    }
}

/// `F`, and `(q - P) x grad F`, hold zero over the box and are narrow; and the box is on the
/// admitted nappe of a cone.
fn check_orthogonal<S: Revolution>(s: &S, q: [f64; 3], p: &B3, nappes: Nappes, what: &str) {
    let (f, grad, w) = s.implicit(p);
    let res = bsub(&exact3(q), p);
    let cross = bcross(&res, &grad);
    let scale = 1.0 + bdot(&res, &res).to_dd().to_f64() + bdot(&grad, &grad).to_dd().to_f64();
    assert!(
        holds_zero(&f),
        "{what}: off the surface, F in {:?}",
        f.to_f64s()
    );
    assert!(
        width(&f) <= 1e-24 * scale * scale,
        "{what}: F too wide {}",
        width(&f)
    );
    for c in &cross {
        assert!(
            holds_zero(c),
            "{what}: residual not normal, {:?}",
            c.to_f64s()
        );
        assert!(
            width(c) <= 1e-24 * scale * scale,
            "{what}: cross too wide {}",
            width(c)
        );
    }
    if let (Some(w), Nappes::One) = (w, nappes) {
        assert!(
            w.hi().signum() >= 0,
            "{what}: beyond the apex {:?}",
            w.to_f64s()
        );
    }
}

/// A unit direction square to `n` from the frame's axes (exact frames only).
fn square_to(frame: &Frame, n: V3) -> [f64; 3] {
    let z = dd::lift(frame.z);
    let c = [
        n[1] * z[2] - n[2] * z[1],
        n[2] * z[0] - n[0] * z[2],
        n[0] * z[1] - n[1] * z[0],
    ];
    if dd::dot(&c, &c).to_f64() == 0.0 {
        frame.x // n along z: the circle is round the axis
    } else {
        frame.z // n square to z: a meridian circle
    }
}

/// Two opposite points of the circle, each a box, and the circle's distance holds theirs.
fn check_circle<S: Revolution>(s: &S, q: [f64; 3], c: &StationaryCircle, nappes: Nappes) {
    let e = exact3(square_to(s.frame(), c.normal));
    let slack = 1e-28 * (1.0 + c.centre.iter().map(|x| x.hi.abs()).sum::<f64>());
    let centre = boxed(c.centre, slack);
    for sign in [1, -1] {
        let r = c.radius.mul_rat(&Rat::from(sign));
        let p = badd(&centre, &bscale(&e, &r));
        check_orthogonal(s, q, &p, nappes, "circle");
        let res = bsub(&exact3(q), &p);
        let d2 = bdot(&res, &res);
        assert!(d2.meets(&c.distance.sqr()), "circle distance");
    }
}

fn check_all<S: Revolution>(s: &S, q: [f64; 3], p: &Projection<Uv>, nappes: Nappes) {
    for l in &p.loci {
        match l {
            Locus::Point(st) if st.kind == Kind::Apex => {
                // A singular point: no normal. It is on the surface (F holds zero).
                let (f, _, _) = s.implicit(&boxed(st.point, st.point_radius));
                assert!(holds_zero(&f), "apex off the surface");
            }
            Locus::Point(st) => {
                let b = boxed(st.point, st.point_radius);
                check_orthogonal(s, q, &b, nappes, &format!("{:?}", st.kind));
                let res = bsub(&exact3(q), &b);
                assert!(bdot(&res, &res).meets(&st.distance.sqr()), "foot distance");
            }
            Locus::Circle(c) => check_circle(s, q, c, nappes),
        }
    }
}

// ---------------------------------------------------------------- frames and queries

/// An exactly orthonormal frame (a signed permutation) at a dyadic origin.
fn exact_frame(rng: &mut Rng) -> Frame {
    let o: [f64; 3] = core::array::from_fn(|_| (rng.next_signed_unit() * 64.0).round() / 64.0);
    let perms = [[0, 1, 2], [1, 2, 0], [2, 0, 1]];
    let p = perms[(rng.next_u64() % 3) as usize];
    let s = if rng.next_u64() & 1 == 0 { 1.0 } else { -1.0 };
    let e = |i: usize, k: f64| {
        let mut v = [0.0; 3];
        v[i] = k;
        v
    };
    Frame {
        origin: o,
        x: e(p[0], 1.0),
        y: e(p[1], s),
        z: e(p[2], s),
    }
}

/// `O + a x + b y + c z`, exact for an exact frame and dyadic `a, b, c`.
fn local(f: &Frame, [a, b, c]: [f64; 3]) -> [f64; 3] {
    core::array::from_fn(|i| f.origin[i] + a * f.x[i] + b * f.y[i] + c * f.z[i])
}

/// A dyadic coordinate in `[-scale, scale]`, on a grid of `2^-12`.
fn dyadic(rng: &mut Rng, scale: f64) -> f64 {
    (rng.next_signed_unit() * scale * 4096.0).round() / 4096.0
}

/// Local query coordinates: random, on the axis, in the equator, or on the axis at a given
/// height.
fn local_query(rng: &mut Rng, case: usize) -> [f64; 3] {
    let mut l = [dyadic(rng, 3.0), dyadic(rng, 3.0), dyadic(rng, 3.0)];
    match case % 6 {
        0 => {
            l[0] = 0.0;
            l[1] = 0.0;
        }
        1 => l[2] = 0.0,
        _ => {}
    }
    l
}

// ---------------------------------------------------------------- certified orthogonality

#[test]
fn torus_feet_are_certified_on_the_surface_and_normal() {
    let mut rng = Rng::new(31);
    for kind in [TorusKind::Ring, TorusKind::Horn, TorusKind::Spindle] {
        for case in 0..12 {
            // Exact frames half the time, rotated ones (the ideal shape about O, z) else.
            let frame = if case % 2 == 0 {
                exact_frame(&mut rng)
            } else {
                sample::frame(&mut rng, 1.0)
            };
            let t = Torus {
                frame,
                ..sample::torus(&mut rng, kind)
            };
            let q = if case % 2 == 0 {
                local(&frame, local_query(&mut rng, case / 2))
            } else {
                sample::query(&mut rng, &frame, 3.0)
            };
            let p = t.stationary(q).unwrap();
            check_all(&t, q, &p, Nappes::Both);
        }
    }
    // The degenerate queries of an exact frame: centre, axis, the tube's centre circle, a
    // pinch, in the hole.
    let frame = exact_frame(&mut rng);
    for (major, minor, l) in [
        (2.0, 0.5, [0.0, 0.0, 0.0]),
        (2.0, 0.5, [0.0, 0.0, 1.5]),
        (2.0, 0.5, [2.0, 0.0, 0.0]),
        (1.0, 1.0, [0.0, 0.0, 0.0]),
        (1.0, 1.0, [0.25, 0.0, 0.0]),
        (0.75, 1.25, [0.0, 0.0, 1.0]),
        (0.75, 1.25, [0.0, 0.0, 0.5]),
        (0.75, 1.25, [0.75, 0.0, 0.0]),
    ] {
        let t = Torus {
            frame,
            major,
            minor,
        };
        let q = local(&frame, l);
        check_all(&t, q, &t.stationary(q).unwrap(), Nappes::Both);
    }
}

#[test]
fn cone_feet_are_certified_on_the_surface_and_normal() {
    let mut rng = Rng::new(32);
    for case in 0..24 {
        let frame = if case % 2 == 0 {
            exact_frame(&mut rng)
        } else {
            sample::frame(&mut rng, 1.0)
        };
        let c = Cone {
            frame,
            ..sample::cone(&mut rng)
        };
        let q = if case % 2 == 0 {
            local(&frame, local_query(&mut rng, case / 2))
        } else {
            sample::query(&mut rng, &frame, 3.0)
        };
        for nappes in [Nappes::One, Nappes::Both] {
            check_all(&c, q, &c.stationary(q, nappes).unwrap(), nappes);
        }
    }
    // On the axis either side of the apex, near it, and in its plane.
    let frame = exact_frame(&mut rng);
    for (radius, l) in [
        (0.0, [0.0, 0.0, 2.0]),
        (0.0, [0.0, 0.0, -2.0]),
        (0.0, [0.0, 0.0, dd::pow2(-40)]),
        (0.0, [dd::pow2(-40), 0.0, 0.0]),
        (0.5, [0.0, 0.0, 0.0]),
        (0.5, [1.0, 0.0, -0.5]),
    ] {
        let c = Cone {
            frame,
            radius,
            semi_angle: 0.7,
        };
        let q = local(&frame, l);
        for nappes in [Nappes::One, Nappes::Both] {
            check_all(&c, q, &c.stationary(q, nappes).unwrap(), nappes);
        }
    }
}

// ---------------------------------------------------------------- rotational symmetry

/// The loci as (distance, kind, is a circle), sorted: what a symmetry must keep exactly.
fn signature(p: &Projection<Uv>) -> Vec<(Rat, Rat, Kind, bool)> {
    let mut v: Vec<_> = p
        .loci
        .iter()
        .map(|l| {
            let d = l.distance();
            (
                d.lo().clone(),
                d.hi().clone(),
                l.kind(),
                matches!(l, Locus::Circle(_)),
            )
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    v
}

fn angle_gap(a: Dd, b: Dd) -> f64 {
    let tau = dd::consts().pi * 2.0;
    let mut d = (a - b).to_f64() % tau.to_f64();
    if d > tau.to_f64() / 2.0 {
        d -= tau.to_f64();
    } else if d < -tau.to_f64() / 2.0 {
        d += tau.to_f64();
    }
    d.abs()
}

/// Every point of `a`, moved by `map`, is a point of `b` of the same distance and kind, at
/// the same `v` (or `-v` when `flip_v`) and `u` moved by `du` (unless on the axis).
fn check_moved(
    a: &Projection<Uv>,
    b: &Projection<Uv>,
    map: impl Fn([f64; 3]) -> [f64; 3],
    du: Dd,
    flip_v: bool,
) {
    for s in a.points() {
        let to = dd::lift(map(dd::round(s.point)));
        let hit = b.points().find(|t| {
            let r: V3 = core::array::from_fn(|i| t.point[i] - to[i]);
            let gap = dd::dot(&r, &r).sqrt().to_f64();
            gap <= s.point_radius + t.point_radius + 1e-14 && t.distance == s.distance
        });
        assert!(hit.is_some(), "no image of {s:?}");
        let hit = hit.unwrap();
        assert_eq!(hit.kind, s.kind);
        let v = if flip_v {
            Dd::new(0.0) - s.param.1.mid
        } else {
            s.param.1.mid
        };
        let rad = s.param.1.radius + hit.param.1.radius + 1e-28;
        assert!(angle_gap(hit.param.1.mid, v) <= rad, "v moved");
        if s.param.0.radius < 1.0 {
            // u is pinned (not a singular point, where any u will do)
            let rad = s.param.0.radius + hit.param.0.radius + 1e-28;
            assert!(
                angle_gap(hit.param.0.mid, s.param.0.mid + du) <= rad,
                "u moved"
            );
        }
    }
}

#[test]
fn quarter_turns_and_mirrors_keep_everything_exactly() {
    let mut rng = Rng::new(33);
    let quarter = dd::consts().pi * 0.5;
    for case in 0..24 {
        let frame = exact_frame(&mut rng);
        let l = local_query(&mut rng, case);
        let [a, b, c] = l;
        let turned = [-b, a, c];
        let mirrored = [a, -b, c];
        let to = |m: [f64; 3]| local(&frame, m);
        // Points move with the query: the map in local coordinates.
        let inv = |p: [f64; 3]| {
            let d: [f64; 3] = core::array::from_fn(|i| p[i] - frame.origin[i]);
            let dot = |e: [f64; 3]| d[0] * e[0] + d[1] * e[1] + d[2] * e[2];
            [dot(frame.x), dot(frame.y), dot(frame.z)]
        };
        let turn = |p: [f64; 3]| {
            let [x, y, z] = inv(p);
            to([-y, x, z])
        };
        let flip = |p: [f64; 3]| {
            let [x, y, z] = inv(p);
            to([x, -y, z])
        };
        let equator = |p: [f64; 3]| {
            let [x, y, z] = inv(p);
            to([x, y, -z])
        };
        let kind = [TorusKind::Ring, TorusKind::Horn, TorusKind::Spindle][case % 3];
        let t = Torus {
            frame,
            ..sample::torus(&mut rng, kind)
        };
        let p0 = t.stationary(to(l)).unwrap();
        let p1 = t.stationary(to(turned)).unwrap();
        let p2 = t.stationary(to(mirrored)).unwrap();
        let p3 = t.stationary(to([a, b, -c])).unwrap();
        assert_eq!(signature(&p0), signature(&p1), "torus turn {case}");
        assert_eq!(signature(&p0), signature(&p2), "torus mirror {case}");
        assert_eq!(signature(&p0), signature(&p3), "torus equator {case}");
        assert_eq!(
            p0.nearest_locus().unwrap().distance(),
            p1.nearest_locus().unwrap().distance()
        );
        check_moved(&p0, &p1, turn, quarter, false);
        check_moved(&p0, &p3, equator, Dd::new(0.0), true);
        // A mirror turns u over: check the points and kinds only.
        for s in p0.points() {
            let to = dd::lift(flip(dd::round(s.point)));
            assert!(p2.points().any(|t| {
                t.kind == s.kind && (0..3).all(|i| (t.point[i] - to[i]).to_f64().abs() <= 1e-14)
            }));
        }
        let cone = Cone {
            frame,
            ..sample::cone(&mut rng)
        };
        for nappes in [Nappes::One, Nappes::Both] {
            let p0 = cone.stationary(to(l), nappes).unwrap();
            let p1 = cone.stationary(to(turned), nappes).unwrap();
            let p2 = cone.stationary(to(mirrored), nappes).unwrap();
            assert_eq!(signature(&p0), signature(&p1), "cone turn {case}");
            assert_eq!(signature(&p0), signature(&p2), "cone mirror {case}");
            check_moved(&p0, &p1, turn, quarter, false);
        }
    }
}

#[test]
fn any_turn_about_the_axis_agrees_to_rounding() {
    let mut rng = Rng::new(34);
    for case in 0..24 {
        let frame = sample::frame(&mut rng, 1.0);
        let l = local_query(&mut rng, case + 1);
        let theta = Dd::new(6.0 * rng.next_signed_unit());
        let (s, c) = dd::sin_cos(theta);
        let (a, b) = (Dd::new(l[0]), Dd::new(l[1]));
        let turned = [(c * a - s * b).to_f64(), (s * a + c * b).to_f64(), l[2]];
        let t = Torus {
            frame,
            ..sample::torus(&mut rng, TorusKind::Ring)
        };
        let (d0, d1) = (
            t.stationary(local(&frame, l)).unwrap(),
            t.stationary(local(&frame, turned)).unwrap(),
        );
        let near = |p: &Projection<Uv>| p.nearest_locus().unwrap().distance().to_dd().to_f64();
        assert!((near(&d0) - near(&d1)).abs() <= 1e-13, "torus {case}");
        assert_eq!(d0.loci.len(), d1.loci.len());
        let cone = Cone {
            frame,
            ..sample::cone(&mut rng)
        };
        let (e0, e1) = (
            cone.stationary(local(&frame, l), Nappes::One).unwrap(),
            cone.stationary(local(&frame, turned), Nappes::One).unwrap(),
        );
        assert!((near(&e0) - near(&e1)).abs() <= 1e-13, "cone {case}");
    }
}

// ---------------------------------------------------------------- dense sampling

/// The sample nearest `q` over a `n x n` grid of `(u, v)` in the given ranges.
fn sampled_min(
    point: impl Fn(Dd, Dd) -> V3,
    q: [f64; 3],
    (u0, u1): (f64, f64),
    (v0, v1): (f64, f64),
    n: usize,
) -> f64 {
    let ql = dd::lift(q);
    let mut best = f64::INFINITY;
    for i in 0..=n {
        for j in 0..=n {
            let u = u0 + (u1 - u0) * i as f64 / n as f64;
            let v = v0 + (v1 - v0) * j as f64 / n as f64;
            let p = point(Dd::new(u), Dd::new(v));
            let r: V3 = core::array::from_fn(|k| p[k] - ql[k]);
            best = best.min(dd::dot(&r, &r).sqrt().to_f64());
        }
    }
    best
}

/// The sample's nearest is no nearer than the certified nearest's lower end (less the
/// double-double evaluation error), and round each minimum a fine grid is not either.
fn check_sampled(
    point: impl Fn(Dd, Dd) -> V3 + Copy,
    q: [f64; 3],
    p: &Projection<Uv>,
    v_range: (f64, f64),
    v_ok: impl Fn(f64) -> bool,
    what: &str,
) {
    let lo = bound::down(p.nearest_locus().unwrap().distance().lo());
    let slack = 1e-26;
    let pi = core::f64::consts::PI;
    let coarse = sampled_min(point, q, (-pi, pi), v_range, 72);
    assert!(coarse >= lo - slack, "{what}: sample {coarse} below {lo}");
    for s in p.points().filter(|s| s.kind == Kind::Min) {
        let (u, v) = (s.param.0.mid.to_f64(), s.param.1.mid.to_f64());
        let h = 1e-3;
        let (va, vb) = (v - h, v + h);
        if !(v_ok(va) && v_ok(vb)) {
            continue;
        }
        let fine = sampled_min(point, q, (u - h, u + h), (va, vb), 12);
        assert!(fine >= lo - slack, "{what}: fine sample {fine} below {lo}");
        // And the minimum is a local minimum of the samples: none round it is below it.
        let ds = bound::down(s.distance.lo());
        assert!(fine >= ds - slack, "{what}: below a minimum");
    }
}

#[test]
fn dense_samples_never_beat_the_certified_torus_nearest() {
    let mut rng = Rng::new(35);
    for kind in [TorusKind::Ring, TorusKind::Horn, TorusKind::Spindle] {
        for case in 0..6 {
            let frame = exact_frame(&mut rng);
            let t = Torus {
                frame,
                ..sample::torus(&mut rng, kind)
            };
            let q = local(&frame, local_query(&mut rng, case));
            let p = t.stationary(q).unwrap();
            let pt = |u: Dd, v: Dd| *t.derivs((u, v), 0).get(0, 0).unwrap();
            let pi = core::f64::consts::PI;
            check_sampled(pt, q, &p, (-pi, pi), |_| true, &format!("{kind:?} {case}"));
        }
    }
}

#[test]
fn dense_samples_never_beat_the_certified_cone_nearest() {
    let mut rng = Rng::new(36);
    for case in 0..12 {
        let frame = exact_frame(&mut rng);
        let c = Cone {
            frame,
            ..sample::cone(&mut rng)
        };
        let q = local(&frame, local_query(&mut rng, case));
        let (sa, ca) = dd::sin_cos(Dd::new(c.semi_angle));
        let apex_v = -c.radius / (sa / ca).to_f64();
        let pt = |u: Dd, v: Dd| *c.derivs((u, v), 0).get(0, 0).unwrap();
        for nappes in [Nappes::One, Nappes::Both] {
            let p = c.stationary(q, nappes).unwrap();
            let (range, low) = match nappes {
                Nappes::One => ((apex_v, apex_v + 8.0), apex_v),
                Nappes::Both => ((apex_v - 8.0, apex_v + 8.0), f64::NEG_INFINITY),
            };
            check_sampled(
                pt,
                q,
                &p,
                range,
                |v| v >= low,
                &format!("{nappes:?} {case}"),
            );
        }
    }
}

// ---------------------------------------------------------------- axis and apex

#[test]
fn axis_queries_give_circles_and_apex_queries_the_apex() {
    let mut rng = Rng::new(37);
    for _ in 0..8 {
        let frame = exact_frame(&mut rng);
        let h = dyadic(&mut rng, 3.0);
        let q = local(&frame, [0.0, 0.0, h]);
        // A ring torus: two circles, near and far, and nothing else.
        let t = Torus {
            frame,
            ..sample::torus(&mut rng, TorusKind::Ring)
        };
        let p = t.stationary(q).unwrap();
        assert!(
            p.loci.iter().all(|l| matches!(l, Locus::Circle(_))),
            "{p:?}"
        );
        assert_eq!(p.loci.len(), 2);
        let kinds: Vec<Kind> = p.loci.iter().map(Locus::kind).collect();
        assert!(kinds.contains(&Kind::Min) && kinds.contains(&Kind::Max));
        // The near circle is at sqrt(h^2 + R^2) - r, enclosed.
        let near = (Dd::prod(t.major, t.major) + Dd::prod(h, h))
            .sqrt()
            .to_f64()
            - t.minor;
        assert!((p.nearest_locus().unwrap().distance().to_dd().to_f64() - near).abs() <= 1e-15);
        // A spindle torus: the circles and both pinches.
        let s = Torus {
            frame,
            ..sample::torus(&mut rng, TorusKind::Spindle)
        };
        let p = s.stationary(q).unwrap();
        assert_eq!(p.points().filter(|s| s.kind == Kind::Apex).count(), 2);
        assert!(p.loci.iter().any(|l| matches!(l, Locus::Circle(_))));
        // A cone with its apex at the frame's origin: the query on the axis above has a
        // circle and the apex; below, one nappe has the apex alone.
        let c = Cone {
            frame,
            radius: 0.0,
            semi_angle: 0.25 + rng.next_f64(),
        };
        for nappes in [Nappes::One, Nappes::Both] {
            let p = c.stationary(q, nappes).unwrap();
            let apex = p.points().filter(|s| s.kind == Kind::Apex).count();
            let circles = p
                .loci
                .iter()
                .filter(|l| matches!(l, Locus::Circle(_)))
                .count();
            assert_eq!(apex, 1);
            let want = if h > 0.0 || nappes == Nappes::Both {
                1
            } else {
                0
            };
            assert_eq!(
                circles,
                if h == 0.0 { 0 } else { want },
                "{h} {nappes:?} {p:?}"
            );
            // The circle's distance is |h| sin a.
            if let Some(Locus::Circle(cc)) = p.loci.iter().find(|l| matches!(l, Locus::Circle(_))) {
                let want = h.abs() * dd::sin_cos(Dd::new(c.semi_angle)).0.to_f64();
                assert!((cc.distance.to_dd().to_f64() - want).abs() <= 1e-15);
            }
        }
        // At the apex exactly: distance zero, the apex alone.
        let p = c.stationary(frame.origin, Nappes::Both).unwrap();
        assert_eq!(p.loci.len(), 1);
        assert_eq!(*p.loci[0].distance(), Bound::exact(Rat::zero()));
        // A hair below the apex, inside the dual cone: the apex is the nearest, at the hair's distance.
        let hair = dd::pow2(-40); // exact beside a dyadic origin
        let p = c
            .stationary(local(&frame, [hair / 8.0, 0.0, -hair]), Nappes::One)
            .unwrap();
        let n = p.nearest_locus().unwrap();
        assert_eq!(n.kind(), Kind::Apex);
        assert!((n.distance().to_dd().to_f64() - hair * (65.0f64 / 64.0).sqrt()).abs() <= 1e-28);
    }
}
