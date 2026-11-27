// Written for solidcorex. Apache-2.0.
//! The on-quadric decision checked on exact rational images of every kind (in frames that
//! are neither orthogonal nor unit), kept through knot insertion, elevation and splitting,
//! and lost by a one-ulp change; the uv map (D13) against an independent double-double
//! evaluation of the standard-form arcs; and the closed-form point against the exact
//! equation.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use scx_num::hash::Rng;
use scx_testkit::analytic::Frame;
use scx_testkit::dd::{self, Dd};
use scx_testkit::nurbs::{Side, Surface};
use scx_testkit::quadric::{Error, Implicit, Layout, Order, Param};
use scx_testkit::Rat;

/// A random frame of small integers with independent axes, so every image point below is
/// an exact double.
fn int_frame(rng: &mut Rng) -> Frame {
    let mut int = |k: i64| (rng.next_u64() % (2 * k as u64 + 1)) as f64 - k as f64;
    loop {
        let mut v = || [int(3), int(3), int(3)];
        let (x, y, z) = (v(), v(), v());
        let det = x[0] * (y[1] * z[2] - y[2] * z[1]) - x[1] * (y[0] * z[2] - y[2] * z[0])
            + x[2] * (y[0] * z[1] - y[1] * z[0]);
        if det != 0.0 {
            let origin = [int(5), int(5), int(5)];
            return Frame { origin, x, y, z };
        }
    }
}

/// A plane curve: degree, knots, points, weights.
type Profile = (usize, Vec<f64>, Vec<[f64; 2]>, Vec<f64>);

/// The full unit circle as four rational quarter arcs whose weights are rational
/// (`w1^2 = w0 w2 / 2` on each arc): an exact circle with doubles.
fn circle() -> Profile {
    let pts = [
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        [-1.0, 1.0],
        [-1.0, 0.0],
        [-1.0, -1.0],
        [0.0, -1.0],
        [1.0, -1.0],
        [1.0, 0.0],
    ];
    let w = [1.0, 1.0, 2.0, 2.0, 4.0, 4.0, 8.0, 8.0, 16.0];
    let knots = vec![0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 4.0];
    (2, knots, pts.to_vec(), w.to_vec())
}

/// The same circle in the standard form: middle weights `cos(pi/4)`, rounded.
fn standard_circle() -> Profile {
    let (p, knots, pts, w) = circle();
    let c = 0.5f64.sqrt();
    let w = w
        .iter()
        .enumerate()
        .map(|(i, _)| if i % 2 == 1 { c } else { 1.0 })
        .collect();
    (p, knots.iter().map(|k| k / 4.0).collect(), pts, w)
}

/// The half meridian from the south pole to the north, `(rho, z)`, radius `r`.
fn meridian(r: f64) -> Profile {
    let pts = [[0.0, -r], [r, -r], [r, 0.0], [r, r], [0.0, r]];
    (
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 2.0],
        pts.to_vec(),
        vec![1.0, 1.0, 2.0, 2.0, 4.0],
    )
}

/// The segment from `a` to `b`, `(rho, z)`.
fn segment(a: [f64; 2], b: [f64; 2]) -> Profile {
    (1, vec![0.0, 0.0, 1.0, 1.0], vec![a, b], vec![1.0, 1.0])
}

/// The surface of revolution of `profile` (`(rho, z)`) about the frame's `z`, swept by
/// `around` (`(cos, sin)`), mapped through the frame: `P_ij = O + M (rho_j c_i, rho_j s_i, z_j)`.
fn revolve(frame: &Frame, around: &Profile, profile: &Profile) -> Surface<3> {
    let (pu, ku, cu, wu) = around;
    let (pv, kv, cv, wv) = profile;
    let mut points = Vec::new();
    let mut weights = Vec::new();
    for (c, a) in cu.iter().zip(wu) {
        for (m, b) in cv.iter().zip(wv) {
            let local = [m[0] * c[0], m[0] * c[1], m[1]];
            points.push(image(frame, local));
            weights.push(a * b);
        }
    }
    Surface::new((*pu, *pv), ku, kv, &points, Some(&weights)).unwrap()
}

fn image(f: &Frame, l: [f64; 3]) -> [f64; 3] {
    core::array::from_fn(|i| f.origin[i] + f.x[i] * l[0] + f.y[i] * l[1] + f.z[i] * l[2])
}

/// Every kind, exactly, in a random integer frame.
fn exact_kinds(rng: &mut Rng) -> Vec<(Implicit, Surface<3>)> {
    let f = int_frame(rng);
    let c = circle();
    let mut out = vec![
        (
            Implicit::Cylinder {
                frame: f,
                radius: 3.0,
            },
            revolve(&f, &c, &segment([3.0, -1.0], [3.0, 2.0])),
        ),
        (
            // rho = 3 + z / 2 from z = -2 to 4
            Implicit::Cone {
                frame: f,
                radius: 3.0,
                slope: 0.5,
            },
            revolve(&f, &c, &segment([2.0, -2.0], [5.0, 4.0])),
        ),
        (
            Implicit::Sphere {
                frame: f,
                radius: 2.0,
            },
            revolve(&f, &c, &meridian(2.0)),
        ),
    ];
    // the torus: the meridian circle of radius 1 about rho = 3
    let (p, k, m, w) = circle();
    let tube: Vec<[f64; 2]> = m.iter().map(|q| [3.0 + q[0], q[1]]).collect();
    out.push((
        Implicit::Torus {
            frame: f,
            major: 3.0,
            minor: 1.0,
        },
        revolve(&f, &c, &(p, k, tube, w)),
    ));
    // a plane: any points in it, any positive weights
    let (nu, nv) = (3, 4);
    let mut pts = Vec::new();
    for i in 0..nu * nv {
        let l = [(i % 5) as f64 - 2.0, (i * 3 % 7) as f64 - 3.0, 0.0];
        pts.push(image(&f, l));
    }
    let w: Vec<f64> = (0..nu * nv).map(|i| 1.0 + (i % 3) as f64 * 0.75).collect();
    let knots_u = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let knots_v = [0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0, 2.0];
    out.push((
        Implicit::Plane { frame: f },
        Surface::new((2, 3), &knots_u, &knots_v, &pts, Some(&w)).unwrap(),
    ));
    out
}

#[test]
fn exact_rational_images_of_every_kind_are_on_it() {
    let mut rng = Rng::new(1);
    for (kind, s) in exact_kinds(&mut rng) {
        let v = kind.check(&s).unwrap();
        assert!(v.on(), "{kind:?} {v:?}");
        let (pu, pv) = s.degrees();
        let spans = |k: &[f64]| {
            let mut d = k.to_vec();
            d.dedup();
            d.len() - 1
        };
        let (ku, kv) = s.knots();
        let per = |p: usize, n: usize| (kind.degree() * p + 1) * n;
        assert_eq!(v.samples, per(pu, spans(ku)) * per(pv, spans(kv)));
    }
}

#[test]
fn on_is_kept_through_insertion_elevation_and_splitting() {
    let mut rng = Rng::new(2);
    for (kind, s) in exact_kinds(&mut rng) {
        if matches!(kind, Implicit::Torus { .. }) {
            continue; // the costliest; its base check is enough
        }
        let ((u0, u1), (v0, v1)) = s.domain();
        let (um, vm) = (u0 + (u1 - u0) * 0.375, v0 + (v1 - v0) * 0.625);
        let (left, right) = s.split_u(um).unwrap();
        let (low, high) = s.split_v(vm).unwrap();
        let changed = [
            s.insert_knot_u(um, 2).unwrap(),
            s.insert_knot_v(vm, 1).unwrap(),
            s.elevate_u(1).unwrap(),
            s.elevate_v(1).unwrap(),
            left,
            right,
            low,
            high,
        ];
        for t in changed {
            let v = kind.check(&t).unwrap();
            assert!(v.on(), "{kind:?} {v:?}");
        }
    }
}

#[test]
fn a_one_ulp_change_of_a_point_or_a_weight_is_off() {
    let mut rng = Rng::new(3);
    let f = int_frame(&mut rng);
    let kind = Implicit::Sphere {
        frame: f,
        radius: 2.0,
    };
    let (p, k, m, w) = meridian(2.0);
    let (cp, ck, cc, cw) = circle();
    for which in 0..2 {
        let (mut m, mut cw) = (m.clone(), cw.clone());
        if which == 0 {
            m[2][0] = m[2][0].next_up(); // the equator's radius
        } else {
            cw[3] = cw[3].next_up();
        }
        let s = revolve(
            &f,
            &(cp, ck.clone(), cc.clone(), cw),
            &(p, k.clone(), m, w.clone()),
        );
        let v = kind.check(&s).unwrap();
        assert!(!v.on(), "{which}");
        let worst = v.worst.unwrap();
        assert!(worst.distance > 0.0 && worst.distance < 1e-13, "{worst:?}");
    }
}

#[test]
fn the_standard_form_with_a_rounded_weight_is_off_by_its_rounding() {
    let mut rng = Rng::new(4);
    let f = int_frame(&mut rng);
    let kind = Implicit::Cylinder {
        frame: f,
        radius: 3.0,
    };
    let s = revolve(&f, &standard_circle(), &segment([3.0, 0.0], [3.0, 1.0]));
    let v = kind.check(&s).unwrap();
    // only the samples off the arcs' ends and middles move
    assert!(v.off > 0, "{v:?}");
    let d = v.worst.unwrap().distance;
    let size = f.x.iter().chain(&f.y).map(|x| x.abs()).fold(0.0, f64::max) * 3.0;
    assert!(d > 0.0 && d < 1e-15 * size, "{d}");
}

#[test]
fn bad_inputs_are_errors() {
    let mut rng = Rng::new(5);
    let (kind, s) = exact_kinds(&mut rng).swap_remove(2);
    let Implicit::Sphere { mut frame, radius } = kind else {
        panic!()
    };
    frame.z = frame.x;
    let flat = Implicit::Sphere { frame, radius };
    assert_eq!(flat.check(&s), Err(Error::SingularFrame));
    frame.z = [f64::NAN, 0.0, 1.0];
    let nan = Implicit::Sphere { frame, radius };
    assert_eq!(nan.check(&s), Err(Error::NonFinite));
    let neg = Surface::new(
        (1, 1),
        &[0.0, 0.0, 1.0, 1.0],
        &[0.0, 0.0, 1.0, 1.0],
        &[[0.0; 3]; 4],
        Some(&[1.0, -1.0, 1.0, 1.0]),
    )
    .unwrap();
    assert_eq!(kind.check(&neg), Err(Error::NonPositiveWeight));
    let x = 1.0f64;
    let narrow = Surface::new(
        (1, 1),
        &[x, x, x.next_up(), x.next_up()],
        &[0.0, 0.0, 1.0, 1.0],
        &[[0.0; 3]; 4],
        None,
    )
    .unwrap();
    assert_eq!(kind.check(&narrow), Err(Error::NarrowSpan));
}

// ---------------------------------------------------------------- the uv map

/// The standard-form arcs evaluated directly: the rational quadratic on the arc of span
/// `i` with end weights 1 and middle weight `cos(h)`, and its angle by `atan2`.
fn arc_angle(start: Dd, sweep: Dd, n: usize, i: usize, s: Dd) -> Dd {
    let arc = sweep / n as f64;
    let h = arc * 0.5;
    let a0 = start + arc * i as f64;
    let (s0, c0) = dd::sin_cos(a0);
    let (s2, c2) = dd::sin_cos(a0 + arc);
    let (sm, cm) = dd::sin_cos(a0 + h);
    let one = Dd::new(1.0);
    let (b0, b1, b2) = ((one - s) * (one - s), s * (one - s) * 2.0, s * s);
    // the middle control point is at radius 1 / cos(h); weighted, that is (cm, sm)
    let x = b0 * c0 + b1 * cm + b2 * c2;
    let y = b0 * s0 + b1 * sm + b2 * s2;
    // (the weight function, b0 + b1 cos(h) + b2, divides both and leaves the angle)
    let mut t = dd::atan2(y, x);
    // unwrap to the arc's neighbourhood
    let two_pi = dd::consts().pi * 2.0;
    while (t - a0).hi < -dd::consts().pi.hi {
        t = t + two_pi;
    }
    while (t - a0).hi > dd::consts().pi.hi {
        t = t - two_pi;
    }
    t
}

#[test]
fn the_arc_map_is_the_standard_form_arcs_angle() {
    let mut rng = Rng::new(6);
    let pi = dd::consts().pi;
    let mut worst: f64 = 0.0;
    for k in 0..40 {
        let sweep = pi * (0.1 + 1.9 * rng.next_f64()) * if k % 3 == 0 { -1.0 } else { 1.0 };
        let start = pi * rng.next_signed_unit();
        let (t0, t1) = (rng.next_signed_unit() * 3.0, 4.0 + rng.next_f64());
        let param = Param::arcs((t0, t1), start, sweep);
        let Param::Arcs { breaks, .. } = &param else {
            panic!()
        };
        let n = breaks.len() - 1;
        assert_eq!(n, Param::arc_count(sweep));
        for _ in 0..20 {
            let i = (rng.next_u64() % n as u64) as usize;
            let (b0, b1) = (breaks[i], breaks[i + 1]);
            let t = b0 + (b1 - b0) * rng.next_f64();
            let s = Dd::diff(t, b0) / Dd::diff(b1, b0);
            let want = arc_angle(start, sweep, n, i, s);
            let got = param.to_step(t);
            worst = worst.max((got - want).abs().hi);
            let back = param.to_nurb(got);
            assert!(
                (back - Dd::new(t)).abs().hi < 1e-28 * (1.0 + t.abs()),
                "{t} {back:?}"
            );
        }
    }
    assert!(worst < 1e-28, "{worst}");
}

#[test]
fn the_arc_count_is_the_pins_rule() {
    let deg = |d: f64| dd::consts().pi * d / 180.0;
    for (d, n) in [
        (45.0, 1),
        (90.5, 1),
        (91.0, 2),
        (180.0, 2),
        (181.0, 3),
        (270.0, 3),
        (271.0, 4),
        (360.0, 4),
    ] {
        assert_eq!(Param::arc_count(deg(d)), n, "{d}");
        assert_eq!(Param::arc_count(-deg(d)), n, "{d}");
    }
}

#[test]
fn the_linear_map_round_trips_and_the_layout_swaps() {
    let lin = Param::Linear {
        nurb: (0.0, 5.0),
        step: (2.0, -8.0),
    };
    assert_eq!(lin.to_step(0.0), Dd::new(2.0));
    assert_eq!(lin.to_step(5.0), Dd::new(-8.0));
    assert_eq!(lin.to_nurb(Dd::new(-3.0)), Dd::new(2.5));
    let arcs = Param::arcs((0.0, 1.0), Dd::new(0.0), dd::consts().pi * 2.0);
    let same = Layout {
        u: arcs.clone(),
        v: lin.clone(),
        order: Order::Same,
    };
    let swapped = Layout {
        u: lin,
        v: arcs,
        order: Order::Swapped,
    };
    let (a, b) = same.to_step((0.3, 1.0));
    assert_eq!(swapped.to_step((1.0, 0.3)), (a, b));
    let (x, y) = swapped.to_nurb((a, b));
    assert!((x - Dd::new(1.0)).abs().hi < 1e-30 && (y - Dd::new(0.3)).abs().hi < 1e-30);
}

/// `Rat(hi) + Rat(lo)`: a double-double exactly.
fn exact(p: &[Dd; 3]) -> [Rat; 3] {
    p.map(|x| &Rat::from_f64(x.hi).unwrap() + &Rat::from_f64(x.lo).unwrap())
}

#[test]
fn the_closed_form_point_satisfies_the_exact_equation() {
    let mut rng = Rng::new(7);
    let mut worst: f64 = 0.0;
    for _ in 0..30 {
        let frame = scx_testkit::gen::frame(&mut rng, 4.0);
        let kinds = [
            Implicit::Plane { frame },
            Implicit::Cylinder { frame, radius: 1.5 },
            Implicit::Cone {
                frame,
                radius: 1.5,
                slope: -0.3,
            },
            Implicit::Sphere { frame, radius: 2.5 },
            Implicit::Torus {
                frame,
                major: 3.0,
                minor: 0.75,
            },
        ];
        for kind in kinds {
            let uv = (
                Dd::new(7.0 * rng.next_signed_unit()),
                Dd::new(2.0 * rng.next_signed_unit()),
            );
            let d = kind.distance(&exact(&kind.point(uv))).unwrap();
            worst = worst.max(d);
        }
    }
    assert!(worst < 1e-29, "{worst}");
}

#[test]
fn the_point_at_a_nurb_parameter_is_the_standard_form_sphere() {
    // The standard-form sphere (four arcs around, two from pole to pole) agrees with the
    // closed form under its layout to its weights' rounding.
    let mut rng = Rng::new(8);
    let frame = int_frame(&mut rng);
    let kind = Implicit::Sphere { frame, radius: 2.0 };
    let (p, k, m, _) = meridian(2.0);
    let c = 0.5f64.sqrt();
    let s = revolve(
        &frame,
        &standard_circle(),
        &(
            p,
            k.iter().map(|x| x / 2.0).collect(),
            m,
            vec![1.0, c, 1.0, c, 1.0],
        ),
    );
    let pi = dd::consts().pi;
    let layout = Layout {
        u: Param::arcs((0.0, 1.0), Dd::new(0.0), pi * 2.0),
        v: Param::arcs((0.0, 1.0), pi * -0.5, pi),
        order: Order::Same,
    };
    for _ in 0..50 {
        let t = (rng.next_f64(), rng.next_f64());
        let want = kind.point_at(&layout, t);
        let got = s.point(t, (Side::Above, Side::Above)).unwrap().rounded;
        for (g, w) in got.iter().zip(&want) {
            assert!(
                (Dd::new(*g) - *w).abs().hi < 1e-14,
                "{t:?} {got:?} {want:?}"
            );
        }
    }
}
