// Written for solidcorex. Apache-2.0.
//! The pin's NURBs of the analytic surfaces, its NURB-to-STEP uv map and its analytic
//! evaluation, checked against the closed forms (`scx_testkit::quadric`, research 59 §9):
//!
//! - **on**: the pin's stored NURB (its homogeneous points, taken exactly) against the
//!   kind's exact equation, as the worst first-order distance over the decision's grid.
//!   No pin NURB can be exactly on its kind: the standard form's middle weights are
//!   `cos` of half an arc, rounded. The bound is the rounding's effect.
//! - **layout**: the pin's NURB at a parameter against the closed-form point there under
//!   the D13 layout (standard-form arcs over the NURB's spans, or linear), which confirms
//!   the layout as the NURB's own parameterisation.
//! - **map**: the pin's `convert_uv_from_nurbs_to_step` against the layout's closed form.
//! - **eval**: the pin's `evaluate` at the NURB parameter (its point is the STEP
//!   evaluation at the converted parameter) and its `evaluate_step_point` at the true STEP
//!   parameter, against the closed-form point.
//!
//! The kinds: spheres, tori, bounded cones and cylinders over full and partial domains,
//! with `swap_uv` and `inside_out`, and planes, in random frames. Each test prints
//! `pin quadric <kind>: <n> surfaces, worst on <d>, layout <l>, map <m> deg, eval <e>,
//! step <s>; <f> failures` (distances relative to the surface's size), then each failure:
//! the input, the pin's answer, the truth and the error.

use scx_num::hash::Rng;
use scx_testkit::analytic::Frame;
use scx_testkit::dd::{self, Dd};
use scx_testkit::nurbs::{Side, Surface as Nurb};
use scx_testkit::quadric::{Implicit, Layout, Order, Param};
use solidcore::geom::extent2d::Extent2d;
use solidcore::surface::cone::p1::Cone;
use solidcore::surface::plane::Plane;
use solidcore::surface::sphere::{Sphere, SphereOptions};
use solidcore::surface::surface::p2::Surface;
use solidcore::surface::torus::{Torus, TorusOptions};
use solidcore::{Kernel, Point2, Point3, Vec2, Vec3};

/// The bounds a failure is reported past: distances relative to the size, degrees.
const ON_TOL: f64 = 1e-14;
const POINT_TOL: f64 = 1e-13;
const MAP_TOL: f64 = 1e-11;

/// Whether an error is over its tolerance; a NaN error is over every tolerance.
fn exceeds(error: f64, tolerance: f64) -> bool {
    error.is_nan() || error > tolerance
}

fn p3(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}

fn v3(a: [f64; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

fn rad(deg: f64) -> Dd {
    dd::consts().pi * deg / 180.0
}

fn deg(x: Dd) -> Dd {
    x * 180.0 / dd::consts().pi
}

/// The pin's stored NURB as the reference's surface, every double exactly; non-rational
/// (the `NOW` weight sentinel) as weight 1.
fn exact_nurb(n: &solidcore::nurbs::srf_basic::Surface) -> Nurb<3> {
    let cpts = n.cpts();
    let flat: Vec<_> = cpts.iter().flatten().collect();
    let weighted: Vec<[f64; 3]> = flat.iter().map(|c| [c.x, c.y, c.z]).collect();
    let weights: Vec<f64> = flat
        .iter()
        .map(|c| {
            if c.w == solidcore::nurbs::math::NOW {
                1.0
            } else {
                c.w
            }
        })
        .collect();
    Nurb::from_homogeneous(n.degrees(), n.u_knots(), n.v_knots(), &weighted, &weights).unwrap()
}

fn breaks(knots: &[f64]) -> Vec<f64> {
    let mut b = knots.to_vec();
    b.dedup();
    b
}

/// One direction: an angular sweep over `[a0, a1]` degrees (reversed: from `a1` to `a0`)
/// as arcs over the NURB's spans, or a length over `[s0, s1]` (reversed likewise).
#[derive(Clone, Copy, Debug)]
enum Dir {
    Angle(f64, f64),
    Length(f64, f64),
}

fn param(dir: Dir, knots: &[f64], reversed: bool) -> Param {
    let b = breaks(knots);
    let (t0, t1) = (b[0], *b.last().unwrap());
    match dir {
        Dir::Angle(a0, a1) => {
            let (start, end) = if reversed { (a1, a0) } else { (a0, a1) };
            Param::Arcs {
                breaks: b,
                start: rad(start),
                sweep: rad(end) - rad(start),
            }
        }
        Dir::Length(s0, s1) => Param::Linear {
            nurb: (t0, t1),
            step: if reversed { (s1, s0) } else { (s0, s1) },
        },
    }
}

/// A case: the pin's surface, its NURB, the kind, the STEP directions (STEP's u, v) and
/// a description.
struct Case {
    what: String,
    pin: Surface,
    step_point: Box<dyn Fn(Point2) -> Option<Point3>>,
    nurb: Nurb<3>,
    kind: Implicit,
    dirs: (Dir, Dir),
    size: f64,
}

#[derive(Default)]
struct Tally {
    surfaces: usize,
    on: f64,
    layout: f64,
    map: f64,
    eval: f64,
    step: f64,
    failures: Vec<String>,
    layouts: Vec<String>,
}

fn dist(a: &[Dd; 3], b: [f64; 3]) -> f64 {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| (*x - Dd::new(y)).hi).collect();
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The layout among the orders and directions whose closed form is the NURB's points.
fn fit_layout(c: &Case, rng: &mut Rng) -> (Layout, f64, String) {
    let (ku, kv) = c.nurb.knots();
    let ts: Vec<(f64, f64)> = (0..12).map(|_| (rng.next_f64(), rng.next_f64())).collect();
    let ((u0, u1), (v0, v1)) = c.nurb.domain();
    let mut best: Option<(Layout, f64, String)> = None;
    for order in [Order::Same, Order::Swapped] {
        let (du, dv) = match order {
            Order::Same => c.dirs,
            Order::Swapped => (c.dirs.1, c.dirs.0),
        };
        for (ru, rv) in [(false, false), (false, true), (true, false), (true, true)] {
            let layout = Layout {
                u: param(du, ku, ru),
                v: param(dv, kv, rv),
                order,
            };
            let err = ts
                .iter()
                .map(|&(a, b)| {
                    let t = (u0 + (u1 - u0) * a, v0 + (v1 - v0) * b);
                    let got = c.nurb.point(t, (Side::Above, Side::Above)).unwrap().rounded;
                    let e = dist(&c.kind.point_at(&layout, t), got) / c.size;
                    // a layout that cannot be evaluated (one arc of a full turn) fits nothing
                    if e.is_nan() { f64::INFINITY } else { e }
                })
                .fold(0.0, f64::max);
            if best.as_ref().is_none_or(|b| err < b.1) {
                best = Some((layout, err, format!("{order:?} reversed u {ru} v {rv}")));
            }
        }
    }
    best.unwrap()
}

fn check(c: &Case, rng: &mut Rng, t: &mut Tally) {
    let k = Kernel::default();
    t.surfaces += 1;
    let v = c.kind.check(&c.nurb).unwrap();
    let on = v.worst.map_or(0.0, |w| w.distance) / c.size;
    t.on = t.on.max(on);
    if exceeds(on, ON_TOL) {
        t.failures.push(format!(
            "{} on: worst {:?}, relative {on:e}",
            c.what, v.worst
        ));
    }
    let (layout, fit, name) = fit_layout(c, rng);
    t.layout = t.layout.max(fit);
    t.layouts.push(format!("{}: {name}", c.what));
    if exceeds(fit, POINT_TOL) {
        t.failures.push(format!(
            "{} layout: no layout fits, best {name} at {fit:e}",
            c.what
        ));
        return;
    }
    let ((u0, u1), (v0, v1)) = c.nurb.domain();
    let angular = |d: Dir| matches!(d, Dir::Angle(..));
    for i in 0..24 {
        let (a, b) = match i {
            0 => (0.0, 0.0),
            1 => (1.0, 1.0),
            2 => (0.5, 0.5),
            _ => (rng.next_f64(), rng.next_f64()),
        };
        let tuv = (u0 + (u1 - u0) * a, v0 + (v1 - v0) * b);
        let step = layout.to_step((Dd::new(tuv.0), Dd::new(tuv.1)));
        let truth = c.kind.point(step);
        // the map, in the pin's units (degrees for angles)
        let in_units = |x: Dd, d: Dir| if angular(d) { deg(x) } else { x };
        let want = (in_units(step.0, c.dirs.0), in_units(step.1, c.dirs.1));
        match c
            .pin
            .convert_uv_from_nurbs_to_step(&k, Point2::new(tuv.0, tuv.1))
        {
            Ok(p) => {
                let e = (Dd::new(p.x) - want.0)
                    .abs()
                    .hi
                    .max((Dd::new(p.y) - want.1).abs().hi);
                // an angle a full turn away is the same point
                let e = if e > 359.0 && e < 361.0 {
                    (e - 360.0).abs()
                } else {
                    e
                };
                t.map = t.map.max(e);
                if exceeds(e, MAP_TOL) {
                    t.failures.push(format!(
                        "{} map: nurb {tuv:?}: pin {:?}, truth ({:?}, {:?}), error {e:e}",
                        c.what,
                        (p.x, p.y),
                        want.0.hi,
                        want.1.hi
                    ));
                }
            }
            Err(e) => t
                .failures
                .push(format!("{} map: nurb {tuv:?}: pin error {e}", c.what)),
        }
        match c
            .pin
            .evaluate(&k, Point2::new(tuv.0, tuv.1), 0, 0, Default::default())
        {
            Ok(rows) => {
                let p = rows[0][0];
                let e = dist(&truth, [p.x, p.y, p.z]) / c.size;
                t.eval = t.eval.max(e);
                if exceeds(e, POINT_TOL) {
                    t.failures.push(format!(
                        "{} eval: nurb {tuv:?}: pin {:?}, truth {:?}, relative error {e:e}",
                        c.what,
                        (p.x, p.y, p.z),
                        truth.map(|x| x.hi)
                    ));
                }
            }
            Err(e) => t
                .failures
                .push(format!("{} eval: nurb {tuv:?}: pin error {e}", c.what)),
        }
        let at = Point2::new(want.0.hi, want.1.hi);
        // the truth at the rounded STEP parameter the pin is given
        let truth_at = c.kind.point((
            if angular(c.dirs.0) {
                rad(at.x)
            } else {
                Dd::new(at.x)
            },
            if angular(c.dirs.1) {
                rad(at.y)
            } else {
                Dd::new(at.y)
            },
        ));
        if let Some(p) = (c.step_point)(at) {
            let e = dist(&truth_at, [p.x, p.y, p.z]) / c.size;
            t.step = t.step.max(e);
            if exceeds(e, POINT_TOL) {
                t.failures.push(format!(
                    "{} step: step {:?}: pin {:?}, truth {:?}, relative error {e:e}",
                    c.what,
                    (at.x, at.y),
                    (p.x, p.y, p.z),
                    truth_at.map(|x| x.hi)
                ));
            }
        }
    }
}

fn report(name: &str, t: &Tally) {
    println!(
        "pin quadric {name}: {} surfaces, worst on {:e}, layout {:e}, map {:e} deg, eval {:e}, \
         step {:e}; {} failures",
        t.surfaces,
        t.on,
        t.layout,
        t.map,
        t.eval,
        t.step,
        t.failures.len()
    );
    for l in &t.layouts {
        println!("  layout {l}");
    }
    for f in t.failures.iter().take(40) {
        println!("  FAIL {f}");
    }
}

fn frame(rng: &mut Rng) -> Frame {
    scx_testkit::sample::frame(rng, 10.0)
}

/// Full and partial domains, degrees: `(u0, u1, v0, v1)`.
fn sphere_domains() -> [[f64; 4]; 3] {
    [
        [0.0, 360.0, -90.0, 90.0],
        [30.0, 200.0, -40.0, 70.0],
        [-20.0, 60.0, 0.0, 90.0],
    ]
}

#[test]
fn pin_sphere() {
    let mut rng = Rng::new(0x5ce1);
    let mut t = Tally::default();
    for dom in sphere_domains() {
        for (swap_uv, inside_out) in [(false, false), (true, false), (false, true), (true, true)] {
            let f = frame(&mut rng);
            let r = 0.5 + 4.0 * rng.next_f64();
            let k = Kernel::default();
            let opts = SphereOptions {
                swap_uv,
                inside_out,
                circle_nurb: None,
            };
            let ext = Extent2d::from_bounds(dom[0], dom[2], dom[1], dom[3]);
            let mut s = Sphere::new(&k, p3(f.origin), v3(f.x), v3(f.y), ext, r, opts).unwrap();
            s.make_nurb(&k).unwrap();
            let nurb = exact_nurb(s.base().base().gw_nurb().unwrap());
            let sp = s.clone();
            let case = Case {
                what: format!(
                    "sphere r {r} domain {dom:?} swap_uv {swap_uv} inside_out {inside_out}"
                ),
                step_point: Box::new(move |uv| sp.evaluate_step_point(uv).ok()),
                pin: Surface::from(s),
                nurb,
                kind: Implicit::Sphere {
                    frame: f,
                    radius: r,
                },
                dirs: (Dir::Angle(dom[0], dom[1]), Dir::Angle(dom[2], dom[3])),
                size: r + 10.0,
            };
            check(&case, &mut rng, &mut t);
        }
    }
    report("sphere", &t);
}

#[test]
fn pin_torus() {
    let mut rng = Rng::new(0x7025);
    let mut t = Tally::default();
    let domains = [
        [0.0, 360.0, 0.0, 360.0],
        [10.0, 100.0, -60.0, 250.0],
        [45.0, 300.0, 90.0, 180.0],
    ];
    for dom in domains {
        for (swap_uv, inside_out) in [(false, false), (true, false), (false, true), (true, true)] {
            let f = frame(&mut rng);
            let minor = 0.25 + rng.next_f64();
            let major = minor + 0.5 + 3.0 * rng.next_f64();
            let k = Kernel::default();
            let opts = TorusOptions {
                swap_uv,
                inside_out,
                circle_nurb: None,
            };
            let ext = Extent2d::from_bounds(dom[0], dom[2], dom[1], dom[3]);
            let mut s =
                Torus::new(&k, p3(f.origin), v3(f.x), v3(f.y), ext, major, minor, opts).unwrap();
            s.make_nurb(&k).unwrap();
            let nurb = exact_nurb(s.base().base().gw_nurb().unwrap());
            let sp = s.clone();
            let case = Case {
                what: format!(
                    "torus R {major} r {minor} domain {dom:?} swap_uv {swap_uv} inside_out {inside_out}"
                ),
                step_point: Box::new(move |uv| sp.evaluate_step_point(uv).ok()),
                pin: Surface::from(s),
                nurb,
                kind: Implicit::Torus {
                    frame: f,
                    major,
                    minor,
                },
                dirs: (Dir::Angle(dom[0], dom[1]), Dir::Angle(dom[2], dom[3])),
                size: major + minor + 10.0,
            };
            check(&case, &mut rng, &mut t);
        }
    }
    report("torus", &t);
}

#[test]
fn pin_cone_and_cylinder() {
    let mut rng = Rng::new(0xc0e);
    let mut t = Tally::default();
    let sweeps = [(0.0, 360.0), (20.0, 110.0), (-30.0, 200.0)];
    for (start, end) in sweeps {
        for (swap_uv, inside_out) in [(false, false), (true, false), (false, true), (true, true)] {
            for cylinder in [false, true] {
                let f = frame(&mut rng);
                // dyadic radii and height, so the slope is exact
                let bot = 1.0 + (rng.next_u64() % 16) as f64 / 4.0;
                let top = if cylinder {
                    bot
                } else {
                    (rng.next_u64() % 20) as f64 / 4.0 + 0.25
                };
                let h = [2.0, 4.0, 8.0][(rng.next_u64() % 3) as usize];
                let k = Kernel::default();
                let (o, x, y) = (p3(f.origin), v3(f.x), v3(f.y));
                let mut s = if cylinder {
                    Cone::new_cylinder_bounded(
                        o, x, y, bot, start, end, h, swap_uv, inside_out, true,
                    )
                } else {
                    Cone::new_bounded(o, x, y, bot, top, start, end, h, swap_uv, inside_out, true)
                };
                s.make_nurb(&k).unwrap();
                let nurb = exact_nurb(s.rev().base().gw_nurb().unwrap());
                let kind = if cylinder {
                    Implicit::Cylinder {
                        frame: f,
                        radius: bot,
                    }
                } else {
                    Implicit::Cone {
                        frame: f,
                        radius: bot,
                        slope: Dd::new((top - bot) / h),
                    }
                };
                let sp = s.clone();
                let case = Case {
                    what: format!(
                        "{} bot {bot} top {top} h {h} sweep ({start}, {end}) swap_uv {swap_uv} \
                         inside_out {inside_out}",
                        if cylinder { "cylinder" } else { "cone" }
                    ),
                    step_point: Box::new(move |uv| sp.evaluate_step_point(uv).ok()),
                    pin: Surface::from(s),
                    nurb,
                    kind,
                    dirs: (Dir::Angle(start, end), Dir::Length(0.0, h)),
                    size: bot.max(top) + h + 10.0,
                };
                check(&case, &mut rng, &mut t);
            }
        }
    }
    report("cone and cylinder", &t);
}

#[test]
fn pin_plane() {
    let mut rng = Rng::new(0x91a);
    let mut t = Tally::default();
    for _ in 0..6 {
        let f = frame(&mut rng);
        let (u0, v0) = (rng.next_signed_unit() * 5.0, rng.next_signed_unit() * 5.0);
        let (u1, v1) = (
            u0 + 0.5 + 4.0 * rng.next_f64(),
            v0 + 0.5 + 4.0 * rng.next_f64(),
        );
        let ext = Extent2d::from_bounds(u0, v0, u1, v1);
        let mut s = Plane::new(p3(f.origin), v3(f.x), v3(f.y), Vec2::new(1.0, 1.0), &ext);
        s.make_nurb().unwrap();
        let nurb = exact_nurb(s.base().gw_nurb().unwrap());
        let case = Case {
            what: format!("plane domain ({u0}, {v0})..({u1}, {v1})"),
            step_point: Box::new(|_| None),
            pin: Surface::from(s),
            nurb,
            kind: Implicit::Plane { frame: f },
            dirs: (Dir::Length(u0, u1), Dir::Length(v0, v1)),
            size: 30.0,
        };
        check(&case, &mut rng, &mut t);
    }
    report("plane", &t);
}
