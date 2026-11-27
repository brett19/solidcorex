// Written for solidcorex. Apache-2.0.
//! The certified ellipse foot (T1, `Ellipse::stationary`) against facts that do not use its
//! quartic: the residual `(q - C(t)) . C'(t)` changes sign, certified, across each foot's
//! enclosure; an exact rational point of the evolute gives an exact double root; mirrored
//! and exactly moved queries give mirrored and equal answers; a circle matches its closed
//! form, with the centre reported as a whole circle of feet; and a dense `f64` sample finds
//! no point nearer than the certified nearest.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp
)]

use scx_num::fmath;
use scx_num::hash::Rng;
use scx_testkit::Rat;
use scx_testkit::analytic::{Ellipse, Frame};
use scx_testkit::bound::{self, Bound};
use scx_testkit::dd::{self, Dd};
use scx_testkit::project::{Enclosed, Kind, Locus, Nearest, Projection, Stationary};
use scx_testkit::sample;

const BITS: u32 = 140;

fn plain() -> Frame {
    Frame {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    }
}

fn ellipse(frame: Frame, a: f64, b: f64) -> Ellipse {
    Ellipse {
        frame,
        semi_x: a,
        semi_y: b,
    }
}

fn b3(v: [f64; 3]) -> [Bound; 3] {
    v.map(|x| Bound::of_f64(x).unwrap())
}

/// `(sin t, cos t)` enclosed for `t = t0 + t1 + t2`, each a double: the addition formula
/// over [`bound::sin_cos`] of each part, so `t` need not be a double.
fn sin_cos3(t: [f64; 3]) -> (Bound, Bound) {
    let mut s = Bound::exact(Rat::zero());
    let mut c = Bound::exact(Rat::one());
    for x in t {
        let (sx, cx) = bound::sin_cos(x, BITS).unwrap();
        (s, c) = (s.mul(&cx).add(&c.mul(&sx)), c.mul(&cx).sub(&s.mul(&sx)));
    }
    (s, c)
}

/// `(q - C(t)) . C'(t)` enclosed, from the frame as given: `-f'(t) / 2` for the squared
/// distance `f`, so `+` before a minimum and `-` after it.
fn residual(e: &Ellipse, q: [f64; 3], t: [f64; 3]) -> Bound {
    let (s, c) = sin_cos3(t);
    let f = &e.frame;
    let (o, x, y, q) = (b3(f.origin), b3(f.x), b3(f.y), b3(q));
    let (a, b) = (
        Rat::from_f64(e.semi_x).unwrap(),
        Rat::from_f64(e.semi_y).unwrap(),
    );
    let (ac, as_) = (c.mul_rat(&a), s.mul_rat(&a));
    let (bc, bs) = (c.mul_rat(&b), s.mul_rat(&b));
    let mut r = Bound::exact(Rat::zero());
    for i in 0..3 {
        let p = o[i].add(&ac.mul(&x[i])).add(&bs.mul(&y[i]));
        let dp = bc.mul(&y[i]).sub(&as_.mul(&x[i]));
        r = r.add(&q[i].sub(&p).mul(&dp));
    }
    r.rounded(BITS)
}

/// The two ends of an angle's enclosure as three doubles each.
fn ends(p: &Enclosed) -> [[f64; 3]; 2] {
    [
        [p.mid.hi, p.mid.lo, -p.radius],
        [p.mid.hi, p.mid.lo, p.radius],
    ]
}

/// Every foot that is not `Flat` has a certified sign change of the residual across its
/// enclosure, in the direction its kind says: a zero of `f'` inside it (continuity).
fn check_residuals(e: &Ellipse, q: [f64; 3], p: &Projection<Enclosed>) {
    for s in p.points() {
        let [lo, hi] = ends(&s.param);
        let (rl, rh) = (residual(e, q, lo), residual(e, q, hi));
        let (sl, sh) = (rl.signum(), rh.signum());
        let want = match s.kind {
            Kind::Min => (Some(1), Some(-1)),
            Kind::Max => (Some(-1), Some(1)),
            _ => continue,
        };
        assert_eq!(
            (sl, sh),
            want,
            "{:?} at {:?}: residuals {:?} {:?}",
            s.kind,
            s.param,
            rl.to_f64s(),
            rh.to_f64s()
        );
    }
}

fn gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(2.0 * std::f64::consts::PI);
    d.min(2.0 * std::f64::consts::PI - d)
}

/// No point of a dense `f64` sample of the curve is nearer than the certified nearest's
/// lower end, less the sample's own rounding.
fn check_sample(e: &Ellipse, q: [f64; 3], p: &Projection<Enclosed>, n: usize) {
    let near = p.nearest_locus().unwrap().distance();
    let lo = bound::down(near.lo());
    let f = &e.frame;
    let size: f64 =
        (0..3).map(|i| f.origin[i].abs() + q[i].abs()).sum::<f64>() + e.semi_x + e.semi_y;
    let slack = 64.0 * f64::EPSILON * size;
    for i in 0..n {
        let t = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
        let (s, c) = fmath::sin_cos(t);
        let d: f64 = (0..3)
            .map(|k| {
                let pt = f.origin[k] + e.semi_x * c * f.x[k] + e.semi_y * s * f.y[k];
                (pt - q[k]) * (pt - q[k])
            })
            .sum::<f64>()
            .sqrt();
        assert!(d >= lo - slack, "sample at {t} is {d}, nearest {lo}");
    }
}

fn check_all(e: &Ellipse, q: [f64; 3]) -> Projection<Enclosed> {
    let p = e.stationary(q).unwrap();
    check_residuals(e, q, &p);
    check_sample(e, q, &p, 4096);
    p
}

#[test]
fn residual_changes_sign_across_every_foot() {
    let mut rng = Rng::new(631);
    for _ in 0..16 {
        let e = sample::ellipse(&mut rng);
        let q = sample::query(&mut rng, &e.frame, 2.5);
        let p = check_all(&e, q);
        assert!(matches!(p.points().count(), 2..=4));
        // Simple feet only, so every one had its sign change checked.
        assert!(p.points().all(|s| matches!(s.kind, Kind::Min | Kind::Max)));
    }
    // Near the evolute, where two feet nearly merge, and lifted off the plane.
    for case in 0..12 {
        let e = sample::ellipse(&mut rng);
        let gap = [1e-6, 1e-9, 1e-12][case % 3];
        let lift = if case % 2 == 0 { 0.0 } else { 0.5 };
        let q = sample::ellipse_evolute(&mut rng, &e, gap, lift);
        check_all(&e, q);
    }
}

#[test]
fn exact_evolute_point_gives_a_double_root() {
    // a = 75, b = 50: the centre of curvature for (cos t, sin t) = (3/5, 4/5) is
    // ((a^2 - b^2)/a cos^3 t, -(a^2 - b^2)/b sin^3 t) = (9, -32), exactly. Its foot
    // (45, 40), at s = tan(t/2) = 1/2, is a double root: stationary, neither a minimum
    // nor a maximum; the other two feet stay simple.
    let e = ellipse(plain(), 75.0, 50.0);
    let q = [9.0, -32.0, 0.0];
    let p = check_all(&e, q);
    assert_eq!(p.points().count(), 3, "{p:?}");
    let t = dd::atan(Dd::new(0.5)) * 2.0;
    let double: Vec<&Stationary<Enclosed>> = p.points().filter(|s| s.kind == Kind::Flat).collect();
    assert_eq!(double.len(), 1, "{p:?}");
    let s = double[0];
    assert!((s.param.mid - t).abs().to_f64() <= s.param.radius + 1e-30);
    assert!(
        s.distance.sqr().contains(&Rat::from(6480)),
        "{:?}",
        s.distance
    );
    assert!((s.point[0].to_f64() - 45.0).abs() < 1e-25);
    assert!((s.point[1].to_f64() - 40.0).abs() < 1e-25);
    // The residual is exactly zero at the exact foot: (q - C) . C' with C' = (-a sin, b cos).
    let r = (9.0 - 45.0) * (-75.0 * 0.8) + (-32.0 - 40.0) * (50.0 * 0.6);
    assert_eq!(r, 0.0);
    // ... and has one sign on both sides, as a double root's must (probed 2^-20 away: at
    // the enclosure's own ends it is of order radius^2, below the enclosures' width).
    let h = 1.0 / 1048576.0;
    let (rl, rh) = (
        residual(&e, q, [s.param.mid.hi, s.param.mid.lo, -h]),
        residual(&e, q, [s.param.mid.hi, s.param.mid.lo, h]),
    );
    assert!(rl.signum().is_some() && rl.signum() == rh.signum());
    // Moved by 2^-20 to either side of the evolute, the double root splits in two or
    // goes, and no Flat point is left.
    let inside = ellipse(plain(), 75.0, 50.0).stationary([9.0 - 1.0 / 1048576.0, -32.0, 0.0]);
    let outside = ellipse(plain(), 75.0, 50.0).stationary([9.0 + 1.0 / 1048576.0, -32.0, 0.0]);
    let (inside, outside) = (inside.unwrap(), outside.unwrap());
    let counts = [inside.points().count(), outside.points().count()];
    assert!(counts == [4, 2] || counts == [2, 4], "{counts:?}");
    for p in [&inside, &outside] {
        assert!(p.points().all(|s| s.kind != Kind::Flat));
    }
}

/// The feet as `(angle, kind, distance)`, the angle mapped by `m`.
fn feet(p: &Projection<Enclosed>, m: impl Fn(f64) -> f64) -> Vec<(f64, Kind, (f64, f64))> {
    p.points()
        .map(|s| (m(s.param.mid.hi), s.kind, s.distance.to_f64s()))
        .collect()
}

fn same_feet(a: &[(f64, Kind, (f64, f64))], b: &[(f64, Kind, (f64, f64))]) {
    assert_eq!(a.len(), b.len(), "{a:?} vs {b:?}");
    for x in a {
        let y = b.iter().find(|y| gap(x.0, y.0) < 1e-14);
        assert!(y.is_some(), "{x:?} not in {b:?}");
        let y = y.unwrap();
        assert_eq!(x.1, y.1, "{x:?} vs {y:?}");
        let (dx, dy) = (x.2.0, y.2.0);
        assert!((dx - dy).abs() <= 1e-15 * dx.max(1.0), "{x:?} vs {y:?}");
    }
}

#[test]
fn mirrored_and_moved_queries_give_matching_feet() {
    let mut rng = Rng::new(632);
    let pi = std::f64::consts::PI;
    for _ in 0..12 {
        let a = (1.0 + 3.0 * rng.next_f64() * 64.0).round() / 64.0;
        let b = (a * (0.2 + 0.8 * rng.next_f64()) * 64.0).round() / 64.0;
        let e = ellipse(plain(), a, b);
        let q: [f64; 3] = core::array::from_fn(|_| (rng.next_signed_unit() * 256.0).round() / 64.0);
        let base = check_all(&e, q);
        let want = feet(&base, |t| t);
        // Mirrored in y: t -> -t; in x: t -> pi - t; in the plane: t -> t.
        let my = e.stationary([q[0], -q[1], q[2]]).unwrap();
        same_feet(&want, &feet(&my, |t| -t));
        let mx = e.stationary([-q[0], q[1], q[2]]).unwrap();
        same_feet(&want, &feet(&mx, |t| pi - t));
        let mz = e.stationary([q[0], q[1], -q[2]]).unwrap();
        same_feet(&want, &feet(&mz, |t| t));
        // Moved exactly: a signed permutation frame at a dyadic origin, the query with it.
        let o: [f64; 3] = core::array::from_fn(|_| (rng.next_signed_unit() * 64.0).round() / 8.0);
        let moved = Frame {
            origin: o,
            x: [0.0, 0.0, -1.0],
            y: [1.0, 0.0, 0.0],
            z: [0.0, -1.0, 0.0],
        };
        let mq = [o[0] + q[1], o[1] - q[2], o[2] - q[0]];
        let m = ellipse(moved, a, b).stationary(mq).unwrap();
        same_feet(&want, &feet(&m, |t| t));
        assert_eq!(
            matches!(base.nearest, Nearest::Unique(_)),
            matches!(m.nearest, Nearest::Unique(_))
        );
    }
    // On an axis the answer is its own mirror: feet come in pairs t, -t (or are 0, pi),
    // and the nearest pair is tied.
    let e = ellipse(plain(), 2.0, 1.0);
    for q in [[0.75, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, -0.25, 1.0]] {
        let p = check_all(&e, q);
        let on_x = q[1] == 0.0;
        same_feet(
            &feet(&p, |t| t),
            &feet(&p, |t| if on_x { -t } else { pi - t }),
        );
    }
    let p = e.stationary([0.75, 0.0, 0.0]).unwrap();
    assert!(
        matches!(&p.nearest, Nearest::Tied(v) if v.len() == 2),
        "{:?}",
        p.nearest
    );
}

#[test]
fn circle_matches_the_closed_form_and_its_centre_is_a_circle() {
    let mut rng = Rng::new(633);
    for _ in 0..12 {
        let r = (0.5 + 2.0 * rng.next_f64() * 64.0).round() / 64.0;
        let e = ellipse(plain(), r, r);
        let q: [f64; 3] = core::array::from_fn(|_| (rng.next_signed_unit() * 256.0).round() / 64.0);
        if q[0] == 0.0 && q[1] == 0.0 {
            continue;
        }
        let p = check_all(&e, q);
        // Feet at atan2(qy, qx) and the opposite, at sqrt((rho -+ r)^2 + z^2).
        let rho2 = Rat::from_f64(q[0] * q[0] + q[1] * q[1]).unwrap();
        let rho = Bound::exact(rho2).sqrt(BITS).unwrap();
        let (rb, z2) = (
            Bound::exact(Rat::from_f64(r).unwrap()),
            Bound::exact(Rat::from_f64(q[2] * q[2]).unwrap()),
        );
        let near = rho.sub(&rb).sqr().add(&z2);
        let far = rho.add(&rb).sqr().add(&z2);
        assert_eq!(p.points().count(), 2);
        let t = fmath::atan2(q[1], q[0]);
        let Nearest::Unique(i) = p.nearest else {
            panic!("{:?}", p.nearest)
        };
        let n = p.loci[i].point().unwrap();
        assert!(gap(n.param.mid.hi, t) < 1e-15, "{:?} vs {t}", n.param);
        assert_eq!(n.kind, Kind::Min);
        assert!(n.distance.sqr().meets(&near), "{:?}", n.distance);
        let f = p.points().find(|s| s.kind == Kind::Max).unwrap();
        assert!(gap(f.param.mid.hi, t + std::f64::consts::PI) < 1e-15);
        assert!(f.distance.sqr().meets(&far));
    }
    // The centre, in the plane and above it: every point is a foot, reported as one circle.
    for h in [0.0, 0.375] {
        let e = ellipse(plain(), 1.5, 1.5);
        let p = e.stationary([0.0, 0.0, h]).unwrap();
        assert_eq!(p.loci.len(), 1, "{p:?}");
        assert_eq!(p.nearest, Nearest::Unique(0));
        let Locus::Circle(c) = &p.loci[0] else {
            panic!("{p:?}")
        };
        assert_eq!(c.kind, Kind::Flat);
        assert_eq!(c.radius, Bound::exact(Rat::from_f64(1.5).unwrap()));
        let want = Rat::from_f64(1.5 * 1.5 + h * h).unwrap();
        assert!(c.distance.sqr().contains(&want), "{:?}", c.distance);
        // Every residual is exactly zero there: check a few angles, certified.
        for t in [0.0, 1.0, -2.5] {
            let r = residual(&e, [0.0, 0.0, h], [t, 0.0, 0.0]);
            assert!(r.contains(&Rat::zero()) && r.radius() < 1e-35);
        }
    }
}
