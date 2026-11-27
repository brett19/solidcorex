// Written for solidcorex. Apache-2.0.
//! The pin's closest points on an ellipse, a torus and a cone, against the certified truths
//! of `scx_testkit::project` (T1, T3).
//!
//! scx has not ported these solvers, so there is no scx side and no pin defect entry: this
//! is a survey of the pin. Each case is the pin's `global_point_solve` (minimise) with its
//! answer's point evaluated by the pin, measured from the query in double-double, and put
//! against the truth's certified nearest distance. A case is
//! - *exact* when the pin's distance is the truth's to `1e-15` relative,
//! - *within tolerance* when to `TOL` relative,
//! - *wrong* otherwise (a farther stationary point, no answer, or an error).
//!
//! The wrong cases are the pin's: the truth is certified, and its own tests check it
//! against closed forms and a second method. Each test prints its counts, and pins the
//! wrong ones by their index in the seeded pool (`KNOWN_*`), so a change in the pin's answers,
//! or in the pool, shows. When scx ports a solver, these indices are its pin defects.

use scx_num::hash::Rng;
use scx_testkit::analytic::{Cone, Ellipse, Frame, Torus};
use scx_testkit::dd::{self, Dd};
use scx_testkit::project::{Nappes, Projection};
use scx_testkit::sample::{self, TorusKind};
use solidcore::curve::curve::Curve;
use solidcore::curve::curve::p1::GlobalPointSolveOptions;
use solidcore::curve::ellipse::p2::EllipseOptions;
use solidcore::geom::axis2_placement::Axis2Placement;
use solidcore::geom::extent1d::Extent1d;
use solidcore::geom::global_solver::{SolutionRequested, SolverOperation};
use solidcore::geom::vector2d::Point2;
use solidcore::geom::vector3d::Vec3;
use solidcore::kernel::Kernel;
use solidcore::surface::surface::Surface;
use solidcore::surface::surface::p4::NoObject;
use solidcore::surface::torus::{Torus as PinTorus, TorusOptions};

/// The relative distance error of an answer within tolerance: above the pin's own
/// convergence, far below a wrong stationary point.
const TOL: f64 = 1e-9;

/// Degrees per radian, as the pin takes its angles.
const DEG: f64 = 180.0 / core::f64::consts::PI;

#[derive(Debug, Default)]
struct Count {
    exact: usize,
    within: usize,
    wrong: Vec<usize>,
}

impl Count {
    /// Files case `i`: `pin` the pin's distance (`None`: no answer or an error), `truth` the
    /// certified nearest distance.
    fn file(&mut self, i: usize, pin: Option<f64>, truth: f64, what: &str) {
        match pin {
            Some(d) if (d - truth).abs() <= 1e-15 * (1.0 + truth) => self.exact += 1,
            Some(d) if (d - truth).abs() <= TOL * (1.0 + truth) => self.within += 1,
            _ => {
                println!("pin wrong #{i}: {what}: pin {pin:?}, truth {truth:e}");
                self.wrong.push(i);
            }
        }
    }

    fn report(&self, name: &str, n: usize, known: &[usize]) {
        println!(
            "difftest {name}: {n} inputs, {} exact, {} within tolerance, {} pin wrong (no scx side)",
            self.exact,
            self.within,
            self.wrong.len()
        );
        assert_eq!(self.wrong, known, "{name}: the pin's wrong cases changed");
    }
}

fn v3(p: [f64; 3]) -> Vec3 {
    Vec3::new(p[0], p[1], p[2])
}

fn distance(p: Vec3, q: [f64; 3]) -> f64 {
    let d = [
        Dd::new(p.x) - Dd::new(q[0]),
        Dd::new(p.y) - Dd::new(q[1]),
        Dd::new(p.z) - Dd::new(q[2]),
    ];
    dd::dot(&d, &d).sqrt().to_f64()
}

fn truth<P>(p: &Projection<P>) -> f64 {
    p.nearest_locus().unwrap().distance().to_dd().to_f64()
}

/// A frame with `z = x * y` to the rounding (the pin makes its own `z`).
fn frame(rng: &mut Rng) -> Frame {
    sample::frame(rng, 1.0)
}

const STANDARD: Frame = Frame {
    origin: [0.0; 3],
    x: [1.0, 0.0, 0.0],
    y: [0.0, 1.0, 0.0],
    z: [0.0, 0.0, 1.0],
};

// ---------------------------------------------------------------- the ellipse (T1)

fn pin_ellipse(k: &Kernel, e: &Ellipse, q: [f64; 3]) -> Option<f64> {
    let f = &e.frame;
    let pe = solidcore::curve::ellipse::Ellipse::new(
        k,
        v3(f.origin),
        v3(f.x),
        v3(f.y),
        Extent1d::from_min_max(0.0, 360.0),
        e.semi_x,
        e.semi_y,
        &EllipseOptions::default(),
    )
    .ok()?;
    let c = Curve::from(pe);
    let sols = c
        .global_point_solve::<NoObject>(
            k,
            &c.natural_interval(),
            SolverOperation::Minimize,
            v3(q),
            1e-10,
            SolutionRequested::Single,
            &GlobalPointSolveOptions::default(),
        )
        .ok()?;
    sols.as_slice()
        .iter()
        .filter_map(|s| c.evaluate_point(k, s.start.param(0).ok()?).ok())
        .map(|p| distance(p, q))
        .min_by(f64::total_cmp)
}

pub const KNOWN_ELLIPSE: &[usize] = &[];

#[test]
fn pin_ellipse_closest_point_against_t1() {
    let k = Kernel::default();
    let mut rng = Rng::new(0x5901);
    let mut count = Count::default();
    let n = 48;
    for i in 0..n {
        let e = sample::ellipse(&mut rng);
        let q = if i % 3 == 0 {
            // near the evolute, where two stationary points nearly meet
            let lift = 0.1 * rng.next_signed_unit();
            sample::ellipse_evolute(&mut rng, &e, 1e-3, lift)
        } else {
            sample::query(&mut rng, &e.frame, 2.0)
        };
        let t = truth(&e.stationary(q).unwrap());
        count.file(
            i,
            pin_ellipse(&k, &e, q),
            t,
            &format!("ellipse {e:?} q {q:?}"),
        );
    }
    // The degenerate queries, in the standard frame: the centre, inside the evolute's cusp
    // on the major axis, the evolute's cusp itself, and a circle's axis.
    let fixed = [
        (2.0, 1.0, [0.0, 0.0, 0.0]),
        (2.0, 1.0, [0.5, 0.0, 0.0]),
        (2.0, 1.0, [1.5, 0.0, 0.0]),
        (0.75, 0.75, [0.0, 0.0, 0.5]),
    ];
    for (j, (semi_x, semi_y, q)) in fixed.into_iter().enumerate() {
        let e = Ellipse {
            frame: STANDARD,
            semi_x,
            semi_y,
        };
        let t = truth(&e.stationary(q).unwrap());
        count.file(
            n + j,
            pin_ellipse(&k, &e, q),
            t,
            &format!("ellipse {e:?} q {q:?}"),
        );
    }
    count.report("pin_ellipse_closest", n + fixed.len(), KNOWN_ELLIPSE);
}

// ---------------------------------------------------------------- the torus (T3)

fn surface_closest(k: &Kernel, s: &Surface, q: [f64; 3]) -> Option<f64> {
    let dom = s.natural_uv_domain_with_nurb(k).ok()?;
    let sols = s
        .global_point_solve::<NoObject>(
            k,
            None,
            &dom,
            SolverOperation::Minimize,
            v3(q),
            1e-10,
            None,
            SolutionRequested::Single,
        )
        .ok()?;
    sols.as_slice()
        .iter()
        .filter_map(|sol| {
            let uv = Point2::new(sol.start.param(0).ok()?, sol.start.param(1).ok()?);
            s.evaluate_point(k, uv).ok()
        })
        .map(|p| distance(p, q))
        .min_by(f64::total_cmp)
}

fn pin_torus(k: &Kernel, t: &Torus, q: [f64; 3]) -> Option<f64> {
    let f = &t.frame;
    let place = Axis2Placement::new(v3(f.origin), v3(f.x), v3(f.y));
    let pt = PinTorus::create_canonical(k, &place, t.major, t.minor, TorusOptions::default(), None)
        .ok()?;
    surface_closest(k, &Surface::from(pt), q)
}

/// Two spindle tori (`R < r`) where the pin answers the nearest point of the query's own
/// meridian half-plane, a local minimum, and misses the nearer one on the inner sheet across
/// the axis (a dense scan of the surface agrees with the truth to the scan's spacing):
/// #5, `R = 1.3558`, `r = 2.4325`: the pin 1.40730, the truth 0.81314;
/// #20, `R = 1.1456`, `r = 2.0119`: the pin 1.47287, the truth 0.45222.
pub const KNOWN_TORUS: &[usize] = &[5, 20];

#[test]
fn pin_torus_closest_point_against_t3() {
    let k = Kernel::default();
    let mut rng = Rng::new(0x5903);
    let mut count = Count::default();
    let n = 36;
    for i in 0..n {
        let kind = [TorusKind::Ring, TorusKind::Horn, TorusKind::Spindle][i % 3];
        let t = Torus {
            frame: frame(&mut rng),
            ..sample::torus(&mut rng, kind)
        };
        let q = sample::query(&mut rng, &t.frame, 3.0);
        let tr = truth(&t.stationary(q).unwrap());
        count.file(i, pin_torus(&k, &t, q), tr, &format!("torus {t:?} q {q:?}"));
    }
    // The degenerate queries, in the standard frame: on a ring's axis, a horn's centre, a
    // spindle's pinch, on a ring's tube centre circle.
    let fixed = [
        (1.0, 0.5, [0.0, 0.0, 0.75]),
        (1.0, 1.0, [0.0, 0.0, 0.0]),
        (0.75, 1.25, [0.0, 0.0, 1.0]),
        (1.0, 0.5, [1.0, 0.0, 0.0]),
    ];
    for (j, (major, minor, q)) in fixed.into_iter().enumerate() {
        let t = Torus {
            frame: STANDARD,
            major,
            minor,
        };
        let tr = truth(&t.stationary(q).unwrap());
        count.file(
            n + j,
            pin_torus(&k, &t, q),
            tr,
            &format!("torus {t:?} q {q:?}"),
        );
    }
    count.report("pin_torus_closest", n + fixed.len(), KNOWN_TORUS);
}

// ---------------------------------------------------------------- the cone (T3)

fn pin_cone(k: &Kernel, c: &Cone, q: [f64; 3]) -> Option<f64> {
    let f = &c.frame;
    let pc = solidcore::surface::cone::Cone::new_unbounded(
        k,
        v3(f.origin),
        v3(f.x),
        v3(f.y),
        c.radius,
        c.semi_angle * DEG,
        false,
        false,
    )
    .ok()?;
    surface_closest(k, &Surface::from(pc), q)
}

pub const KNOWN_CONE: &[usize] = &[];

#[test]
fn pin_cone_closest_point_against_t3() {
    let k = Kernel::default();
    let mut rng = Rng::new(0x5905);
    let mut count = Count::default();
    let n = 36;
    for i in 0..n {
        let c = Cone {
            frame: frame(&mut rng),
            ..sample::cone(&mut rng)
        };
        let q = sample::query(&mut rng, &c.frame, 3.0);
        // The pin's cone is the nappe holding the reference circle, apex to infinity.
        let tr = truth(&c.stationary(q, Nappes::One).unwrap());
        count.file(i, pin_cone(&k, &c, q), tr, &format!("cone {c:?} q {q:?}"));
    }
    // The degenerate queries, in the standard frame: the apex, on the axis above it and
    // below it, and on the axis of a cone with a reference circle.
    let quarter = core::f64::consts::FRAC_PI_4;
    let fixed = [
        (0.0, [0.0, 0.0, 0.0]),
        (0.0, [0.0, 0.0, 2.0]),
        (0.0, [0.0, 0.0, -2.0]),
        (0.5, [0.0, 0.0, 0.25]),
    ];
    for (j, (radius, q)) in fixed.into_iter().enumerate() {
        let c = Cone {
            frame: STANDARD,
            radius,
            semi_angle: quarter,
        };
        let tr = truth(&c.stationary(q, Nappes::One).unwrap());
        count.file(
            n + j,
            pin_cone(&k, &c, q),
            tr,
            &format!("cone {c:?} q {q:?}"),
        );
    }
    count.report("pin_cone_closest", n + fixed.len(), KNOWN_CONE);
}
