// Written for solidcorex. Apache-2.0.
//! Truths of the blend (task MK12): closed forms, never the pin's output.

use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::certificate::SpanBound;

use super::{BlendError, BlendRefusal, EndOrder, SectionFrame, section_blend};

type Arr = [f64; 3];

fn frame(p: Arr) -> SectionFrame {
    SectionFrame::at(Point3::new(p[0], p[1], p[2]))
}

fn v3(a: Arr) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

fn close(a: Arr, b: Arr, tol: f64) -> bool {
    let d =
        (fmath::powi(a[0] - b[0], 2) + fmath::powi(a[1] - b[1], 2) + fmath::powi(a[2] - b[2], 2))
            .sqrt();
    let n = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    d <= tol * (1.0 + n)
}

fn at(b: &super::Blended, u: f64, v: f64) -> Arr {
    b.surface.point([u, v], [Approach::Below; 2]).unwrap()
}

/// `S^(i, j)` at `(u, v)`.
fn partial<const N: usize>(b: &super::Blended, u: f64, v: f64, i: usize, j: usize) -> Arr {
    let jet = b.surface.jet::<N>([u, v], [Approach::Below; 2]).unwrap();
    jet.get(i, j).unwrap()
}

const G0: [EndOrder; 2] = [EndOrder::G0, EndOrder::G0];

/// Two sections held at G0 are the ruled surface between them: `(1 - u) A(v) + u B(v)`, A and B
/// the lines through their stations (the pin's `section_blend.rs:47`).
#[test]
fn two_sections_at_g0_are_the_ruled_surface() {
    let a = [[0.0, 0.0, 0.0], [4.0, 0.0, 1.0]];
    let b = [[0.0, 3.0, 2.0], [4.0, 5.0, 0.0]];
    let sections = vec![a.map(frame).to_vec(), b.map(frame).to_vec()];
    let blend = section_blend(&sections, G0, [0.0, 0.0]).unwrap();
    assert_eq!(blend.surface.degree(scx_nurbs_surface::ParamAxis::U), 1);
    for i in 0..=5 {
        for j in 0..=5 {
            let (u, v) = (f64::from(i) / 5.0, f64::from(j) / 5.0);
            let line = |s: &[Arr; 2], v: f64| [0, 1, 2].map(|k| s[0][k] * (1.0 - v) + s[1][k] * v);
            let (av, bv) = (line(&a, v), line(&b, v));
            let want = [0, 1, 2].map(|k| av[k] * (1.0 - u) + bv[k] * u);
            let got = at(&blend, u, v);
            assert!(close(got, want, 1e-12), "({u}, {v}): {got:?} vs {want:?}");
        }
    }
    assert!(blend.through < 1e-11, "{}", blend.through);
}

/// Five sections of five stations each of the parabolic cylinder `(2 v, t, t^2)` at
/// `t = 0, 1/4, .., 1`, `v = 0, 1/4, .., 1`, the ends' frames the cylinder's own derivatives at
/// G2 (the crown chosen so the definition's speed is the parameter's): the surface is the
/// cylinder, between the stations as well as on them (the pin's `section_blend.rs:132`).
#[test]
fn a_quadratic_s_sections_and_frames_blend_to_the_quadratic() {
    let run = 5;
    let step = 1.0 / f64::from(run - 1);
    let point = |t: f64, v: f64| [2.0 * v, t, t * t];
    let d1 = |t: f64| [0.0, 1.0, 2.0 * t];
    let d2 = [0.0, 0.0, 2.0];
    let mut sections = Vec::new();
    let mut crown = [0.0; 2];
    for i in 0..run {
        let t = f64::from(i) * step;
        let mut section = Vec::new();
        for v in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let mut f = frame(point(t, v));
            if i == 0 || i == run - 1 {
                let (other, sign) = if i == 0 {
                    (point(step, v), 1.0)
                } else {
                    (point(1.0 - step, v), -1.0)
                };
                let speed = (1.0 + 4.0 * t * t).sqrt();
                let chord = {
                    let p = point(t, v);
                    (fmath::powi(p[1] - other[1], 2) + fmath::powi(p[2] - other[2], 2)).sqrt()
                };
                crown[usize::from(i != 0)] = speed * 3.0 * step / chord - 1.0;
                let d = d1(t);
                f.leave = v3([
                    d[0] * sign / speed,
                    d[1] * sign / speed,
                    d[2] * sign / speed,
                ]);
                f.bend = v3(d2.map(|x| x / (speed * speed)));
            }
            section.push(f);
        }
        sections.push(section);
    }
    let blend = section_blend(&sections, [EndOrder::G2, EndOrder::G2], crown).unwrap();
    for i in 0..=8 {
        for j in 0..=8 {
            let (t, v) = (f64::from(i) / 8.0, f64::from(j) / 8.0);
            let got = at(&blend, t, v);
            assert!(close(got, point(t, v), 1e-12), "({t}, {v}): {got:?}");
        }
    }
    // the end derivatives are the cylinder's, at the stations and between them
    for v in [0.0, 0.3, 0.5, 1.0] {
        for t in [0.0, 1.0] {
            let got = partial::<2>(&blend, t, v, 1, 0);
            assert!(close(got, d1(t), 1e-10), "{got:?}");
            let got = partial::<2>(&blend, t, v, 2, 0);
            assert!(close(got, d2, 1e-9), "{got:?}");
        }
    }
}

/// The helix `(r cos a, r sin a, h a)` at `a`: its point, unit tangent, curvature vector and the
/// arc-length derivative of its curvature vector (`h = 0`: a circle).
fn helix(r: f64, h: f64, a: f64) -> (Arr, Arr, Arr, Arr) {
    let c = (r * r + h * h).sqrt();
    let (sin, cos) = (fmath::sin(a), fmath::cos(a));
    (
        [r * cos, r * sin, h * a],
        [-r * sin / c, r * cos / c, h / c],
        [-r * cos / (c * c), -r * sin / (c * c), 0.0],
        [r * sin / (c * c * c), -r * cos / (c * c * c), 0.0],
    )
}

fn square_to(v: Arr, t: Arr) -> Arr {
    let d = v[0] * t[0] + v[1] * t[1] + v[2] * t[2];
    [v[0] - t[0] * d, v[1] - t[1] * d, v[2] - t[2] * d]
}

/// G3 at both ends, the frames a circle's and a helix's (whose curvature vector turns with its
/// torsion, so `bend_change` has a part square to the tangent), five stations on helices of
/// growing radius from `a = 0.3` to `a = 1.5`. At every station the surface leaves with
/// `d_u = t s`, `d_uu = kappa s^2` and the part of `d_uuu` square to `t` equal to
/// `s^3 dkappa/ds`'s, the helix's own derivatives at the constant speed `s` of the definition;
/// the last section's frames point back (the pin's `g3_ends_hold_...`).
#[test]
fn g3_ends_hold_a_circle_s_and_a_helix_s_frames() {
    let (a0, a1) = (0.3, 1.5);
    let crown = [0.2, -0.1];
    let neg = |v: Arr| v.map(|x| -x);
    for (h, circle) in [(0.0, true), (0.6, false)] {
        let radius = |j: u32| 1.0 + 0.25 * f64::from(j);
        let build = |a: f64, last: bool, j: u32| {
            let (p, t, kappa, dk) = helix(radius(j), h, a);
            let mut f = frame(p);
            f.leave = v3(if last { neg(t) } else { t });
            f.bend = v3(kappa);
            let dk = if circle { [0.0; 3] } else { dk };
            f.bend_change = v3(if last { neg(dk) } else { dk });
            f
        };
        let first: Vec<_> = (0..5).map(|j| build(a0, false, j)).collect();
        let last: Vec<_> = (0..5).map(|j| build(a1, true, j)).collect();
        let blend = section_blend(
            &[first.clone(), last.clone()],
            [EndOrder::G3, EndOrder::G3],
            crown,
        )
        .unwrap();
        for j in 0..5u32 {
            let v = f64::from(j) / 4.0;
            let (p, q) = (first[j as usize].p, last[j as usize].p);
            let chord =
                (fmath::powi(p.x - q.x, 2) + fmath::powi(p.y - q.y, 2) + fmath::powi(p.z - q.z, 2))
                    .sqrt();
            for (u, a, end) in [(0.0, a0, 0usize), (1.0, a1, 1)] {
                let (_, t, kappa, dk) = helix(radius(j), h, a);
                let speed = chord / 3.0 * (1.0 + crown[end]);
                let want = [
                    t.map(|x| x * speed),
                    kappa.map(|x| x * speed * speed),
                    square_to(dk, t).map(|x| x * fmath::powi(speed, 3)),
                ];
                let scale = speed.max(fmath::powi(speed, 3));
                for (o, w) in want.into_iter().enumerate() {
                    let d = partial::<3>(&blend, u, v, o + 1, 0);
                    let got = if o == 2 { square_to(d, t) } else { d };
                    let miss = (fmath::powi(got[0] - w[0], 2)
                        + fmath::powi(got[1] - w[1], 2)
                        + fmath::powi(got[2] - w[2], 2))
                    .sqrt();
                    assert!(
                        miss <= 1e-9 * (scale + 1.0),
                        "h {h}, station {j}, u {u}, order {}: {got:?} vs {w:?}",
                        o + 1
                    );
                }
            }
        }
    }
}

/// G1 leaves along `leave` at the speed the definition gives, and arrives against its `leave`;
/// with three sections the chord is to the neighbour and the interval is 1/2.
#[test]
fn g1_ends_leave_at_the_definition_s_speed() {
    let lift = [0.0, 0.0, 1.0];
    let back = [0.0, 1.0, -1.0];
    let dist = |a: Arr, b: Arr| {
        (fmath::powi(a[0] - b[0], 2) + fmath::powi(a[1] - b[1], 2) + fmath::powi(a[2] - b[2], 2))
            .sqrt()
    };
    let make = |y: f64, z: f64, leave: Arr| -> Vec<SectionFrame> {
        [0.0, 2.0]
            .iter()
            .map(|&x| {
                let mut f = frame([x, y, z]);
                f.leave = v3(leave);
                f
            })
            .collect()
    };
    let (a, b) = (make(0.0, 0.0, lift), make(3.0, 1.0, back));
    let mid = make(1.5, 0.3, [0.0; 3]);
    let crown = [0.5, -0.25];
    let ends = [EndOrder::G1; 2];
    let blend = section_blend(&[a, b.clone()], ends, crown).unwrap();
    let unit_back = back.map(|x| x / 2.0_f64.sqrt());
    for (j, v) in [0.0, 1.0].into_iter().enumerate() {
        let x = [0.0, 2.0][j];
        let chord = dist([x, 0.0, 0.0], [x, 3.0, 1.0]);
        let d0 = partial::<1>(&blend, 0.0, v, 1, 0);
        let want0 = lift.map(|c| c * chord / 3.0 * (1.0 + crown[0]));
        assert!(close(d0, want0, 1e-12), "{d0:?} vs {want0:?}");
        let d1 = partial::<1>(&blend, 1.0, v, 1, 0);
        let want1 = unit_back.map(|c| -c * chord / 3.0 * (1.0 + crown[1]));
        assert!(close(d1, want1, 1e-12), "{d1:?} vs {want1:?}");
    }
    let a = make(0.0, 0.0, lift);
    let blend = section_blend(&[a, mid, b], ends, crown).unwrap();
    for (j, v) in [0.0, 1.0].into_iter().enumerate() {
        let x = [0.0, 2.0][j];
        let d0 = partial::<1>(&blend, 0.0, v, 1, 0);
        let want0 = lift.map(|c| c * dist([x, 0.0, 0.0], [x, 1.5, 0.3]) / 3.0 * 1.5 * 2.0);
        assert!(close(d0, want0, 1e-12), "{d0:?} vs {want0:?}");
        let d1 = partial::<1>(&blend, 1.0, v, 1, 0);
        let want1 = unit_back.map(|c| -c * dist([x, 3.0, 1.0], [x, 1.5, 0.3]) / 3.0 * 0.75 * 2.0);
        assert!(close(d1, want1, 1e-12), "{d1:?} vs {want1:?}");
    }
}

fn refusal<T: core::fmt::Debug>(r: Result<T, BlendError>) -> BlendRefusal {
    match r {
        Err(BlendError::Refused(r)) => r,
        other => panic!("not a refusal: {other:?}"),
    }
}

/// The refusals, typed, and each bound accepted at its limit; the names are unique.
#[test]
fn malformed_blends_are_refused() {
    let line = |y: f64| vec![frame([0.0, y, 0.0]), frame([1.0, y, 0.0])];
    let stations = |y: f64, n: u32| -> Vec<SectionFrame> {
        (0..n).map(|j| frame([f64::from(j), y, 0.0])).collect()
    };
    let flat = [0.0, 0.0];
    assert!(matches!(
        refusal(section_blend(&[line(0.0)], G0, flat)),
        BlendRefusal::Sections { count: 1 }
    ));
    assert!(matches!(
        refusal(section_blend(
            &[stations(0.0, 1), stations(1.0, 1)],
            G0,
            flat
        )),
        BlendRefusal::Stations { count: 1 }
    ));
    assert!(matches!(
        refusal(section_blend(&[line(0.0), stations(1.0, 3)], G0, flat)),
        BlendRefusal::Uneven {
            section: 1,
            count: 3,
            expected: 2
        }
    ));
    for (c, ok) in [
        (-0.9, true),
        (-0.900_000_1, false),
        (3.0, true),
        (3.000_000_1, false),
    ] {
        let r = section_blend(&[line(0.0), line(1.0)], G0, [c, 0.0]);
        if ok {
            assert!(r.is_ok(), "crown {c}");
        } else {
            assert!(matches!(refusal(r), BlendRefusal::Crown { end: 0, .. }));
        }
    }
    assert!(matches!(
        refusal(section_blend(&[line(0.0), line(1.0)], G0, [f64::NAN, 0.0])),
        BlendRefusal::Crown { .. }
    ));
    let mut bad = line(1.0);
    bad[1].p = Point3::new(1.0, f64::INFINITY, 0.0);
    assert!(matches!(
        refusal(section_blend(&[line(0.0), bad], G0, flat)),
        BlendRefusal::NotFinite {
            section: 1,
            station: 1
        }
    ));
    // sections that meet at a station
    let mut meet = line(1.0);
    meet[1].p = Point3::new(1.0, 0.0, 0.0);
    assert!(matches!(
        refusal(section_blend(&[line(0.0), meet], G0, flat)),
        BlendRefusal::Meet {
            section: 0,
            station: 1
        }
    ));
    // a G1 end with no direction, but a G0 end ignores the frame
    let g1 = [EndOrder::G1, EndOrder::G0];
    assert!(matches!(
        refusal(section_blend(&[line(0.0), line(1.0)], g1, flat)),
        BlendRefusal::NoDirection { end: 0, station: 0 }
    ));
    // a G3 end whose tangent leaves no room for the change of curvature is the same refusal
    let kinds = [
        BlendError::Refused(BlendRefusal::Sections { count: 0 }),
        BlendError::Refused(BlendRefusal::Stations { count: 0 }),
        BlendError::Refused(BlendRefusal::Uneven {
            section: 0,
            count: 0,
            expected: 0,
        }),
        BlendError::Refused(BlendRefusal::Crown { end: 0, value: 0.0 }),
        BlendError::Refused(BlendRefusal::NotFinite {
            section: 0,
            station: 0,
        }),
        BlendError::Refused(BlendRefusal::NoDirection { end: 0, station: 0 }),
        BlendError::Refused(BlendRefusal::Meet {
            section: 0,
            station: 0,
        }),
        BlendError::Refused(BlendRefusal::Derivative { end: 0, station: 0 }),
        BlendError::Defect { why: "x" },
    ];
    let mut names: Vec<_> = kinds.iter().map(BlendError::name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), kinds.len());
    assert!(kinds.iter().all(|e| !e.to_string().is_empty()));
}

/// The certificate: every section's curve is a rail the surface is proved to pass through,
/// at rounding, and a surface moved off it is bounded above the move.
#[test]
fn the_rail_bound_proves_each_section() {
    let curve = |y: f64, z: f64| -> Vec<SectionFrame> {
        (0..5)
            .map(|j| {
                let x = f64::from(j) / 4.0;
                frame([x, y + 0.3 * fmath::sin(x * 3.0), z + x * x])
            })
            .collect()
    };
    let sections = vec![
        curve(0.0, 0.0),
        curve(1.0, 0.4),
        curve(2.1, -0.2),
        curve(3.0, 0.5),
    ];
    let blend = section_blend(&sections, G0, [0.0, 0.0]).unwrap();
    let tol = Accuracy::new(Length::from_mm(1e-6)).unwrap();
    for i in 0..4 {
        let b = blend
            .rail_bound(i, tol, &mut Budget::unlimited(), &Stop::new())
            .unwrap()
            .unwrap();
        let SpanBound::Bounded(len) = b else {
            panic!("a bound holds")
        };
        assert!(len.mm() < 1e-9, "section {i}: {len:?}");
    }
    assert!(
        blend
            .rail_bound(4, tol, &mut Budget::unlimited(), &Stop::new())
            .unwrap()
            .is_none()
    );
    assert!(blend.section(4).is_none());
}
