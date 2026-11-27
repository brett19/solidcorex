// Written for solidcorex. Apache-2.0.
//! scx-algo's integrators and Bernstein root finder against the pin (part of the `algo`
//! and `algo_ic` targets, through [`crate::algo::cases`]).
//!
//! The cases: one Cash-Karp step of `y' = (y1 + 1, -y0)` (`ode_step`); an integration of
//! `y' = (y1 + a, -y0)` from 0 to `T` (`ode_integrate2`); the quadrature of
//! `1 / (1 + c t^2)` over `[-1, 1]` (`quadrature`); and the roots on `[0, 1]` of a quintic
//! in Bernstein form (`bernstein_roots`). The four seeded sets are drawn in that order
//! from one stream, so they are the inputs scx-algo's development timings read.

use crate::case::{Case, same};
use crate::{BATCH, NoError, Rng, edges, pin_bern};

use scx_algo::bernstein::{RootLimits, bernstein_roots};
use scx_algo::ode as xode;
use solidcore::mass::integrator as pode;

const NAN: f64 = f64::NAN;
const INF: f64 = f64::INFINITY;
/// The accuracy every step, integration and quadrature asks for.
const ACC: f64 = 1e-9;
/// The integrations' first step.
const FIRST: f64 = 0.1;

/// A step from `[y0, unused, x, h]`: `y = (y0, 0.5)`, its derivative `(1.5, -y0)`.
type Step = [f64; 4];
/// An integration `[a', unused, T]`: `a = a' + 1.5`, from 0 to `T`.
type Integration = [f64; 3];
/// A quintic's Bernstein coefficients on `[0, 1]`.
type Quintic = [f64; 6];

/// The four seeded sets, drawn in order from one stream.
fn inputs() -> (Vec<Step>, Vec<Integration>, Vec<f64>, Vec<Quintic>) {
    let mut r = Rng::new(0x5eed_b3e5);
    let steps = (0..BATCH)
        .map(|_| {
            [
                r.range(-1.0, 1.0),
                r.range(-1.0, 1.0),
                r.range(0.0, 3.0),
                r.range(0.01, 0.5),
            ]
        })
        .collect();
    let ints = (0..BATCH)
        .map(|_| [r.range(-1.0, 1.0), r.range(-1.0, 1.0), r.range(1.0, 4.0)])
        .collect();
    let quads = (0..BATCH).map(|_| r.range(1.0, 30.0)).collect();
    let polys = (0..BATCH).map(|_| r.array(-1.0, 1.0)).collect();
    (steps, ints, quads, polys)
}

fn steps(out: &mut Vec<Case>, seeded: &[Step]) {
    // a zero, tiny, huge (shrunk many times) and backward step; NaN and infinite starts
    let e = edges(
        61,
        &[
            [0.3, 0.0, 1.0, 0.0],
            [0.3, 0.0, 1.0, 1e-300],
            [0.3, 0.0, 1.0, 1e3],
            [0.3, 0.0, 1.0, -0.3],
            [NAN, 0.0, 1.0, 0.1],
            [0.3, 0.0, INF, 0.1],
        ],
    );
    for (name, v) in [("algo/ode_step", seeded), ("algo/ode_step/edge", &e)] {
        out.push(Case::map(
            name,
            same(v),
            |&[y0, _, x, h]: &Step| {
                xode::step(
                    &[y0, 0.5],
                    &[1.5, -y0],
                    x,
                    h,
                    ACC,
                    [1.0, 1.0],
                    |_, y: &[f64; 2]| Ok::<_, NoError>([y[1] + 1.0, -y[0]]),
                )
            },
            |&[y0, _, x, h]: &Step| {
                let mut scale = [1.0, 1.0];
                pode::OdeIntegrator::default().step(
                    &[y0, 0.5],
                    &[1.5, -y0],
                    x,
                    h,
                    ACC,
                    &mut scale,
                    |_, y, dy| {
                        dy[0] = y[1] + 1.0;
                        dy[1] = -y[0];
                        Ok(())
                    },
                )
            },
        ));
    }
}

fn integrations(out: &mut Vec<Case>, seeded: &[Integration]) {
    // an empty, a tiny, a backward and a long range; a NaN rate
    let e = edges(
        62,
        &[
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1e-12],
            [0.0, 0.0, -3.0],
            [0.0, 0.0, 40.0],
            [NAN, 0.0, 2.0],
        ],
    );
    for (name, v) in [
        ("algo/ode_integrate2", seeded),
        ("algo/ode_integrate2/edge", &e),
    ] {
        out.push(Case::map(
            name,
            same(v),
            |&[a, _, t]: &Integration| {
                let a = a + 1.5;
                xode::OdeIntegrator::<2>::new(ACC, FIRST, 0.0).integrate(
                    0.0,
                    t,
                    |_, y: &[f64; 2]| Ok::<_, NoError>([y[1] + a, -y[0]]),
                )
            },
            |&[a, _, t]: &Integration| {
                let a = a + 1.5;
                let mut v = [0.0, 0.0];
                let counts = pode::OdeIntegrator::default().integrate(
                    &mut v,
                    0.0,
                    t,
                    pode::OdeOptions {
                        accuracy: ACC,
                        initial_step: FIRST,
                        minimum_step: 0.0,
                        scale: None,
                        quadrature: false,
                        rounding: None,
                        rate: None,
                    },
                    |_, y, dy| {
                        dy[0] = y[1] + a;
                        dy[1] = -y[0];
                        Ok(())
                    },
                );
                counts.map(|c| (v, c))
            },
        ));
    }
}

fn quadratures(out: &mut Vec<Case>, seeded: &[f64]) {
    // a constant, a sharp peak, a near pole at both ends, a near constant, NaN
    let e = edges(63, &[0.0, 1e6, -0.999, 1e-12, NAN]);
    for (name, v) in [("algo/quadrature", seeded), ("algo/quadrature/edge", &e)] {
        out.push(Case::map(
            name,
            same(v),
            |&c: &f64| {
                xode::Quadrature::new(ACC)
                    .integrate(-1.0, 1.0, |t| Ok::<_, NoError>(1.0 / (1.0 + c * t * t)))
            },
            |&c: &f64| pode::integrate(-1.0, 1.0, ACC, |t| Ok(1.0 / (1.0 + c * t * t))),
        ));
    }
}

fn roots(out: &mut Vec<Case>, seeded: &[Quintic]) {
    let RootLimits {
        depth,
        subdivisions,
    } = RootLimits::DEFAULT;
    // zero, a double root (s - 1/2)^2 (spending the limits), roots at both ends,
    // alternating signs, a constant, tiny, NaN and infinite coefficients
    let e = edges(
        64,
        &[
            [0.0; 6],
            [0.25, 0.05, -0.05, -0.05, 0.05, 0.25],
            [0.0, 1.0, -1.0, 1.0, -1.0, 0.0],
            [1.0, -1.0, 1.0, -1.0, 1.0, -1.0],
            [1.0; 6],
            [1e-300, -1e-300, 1e-300, -1e-300, 1e-300, -1e-300],
            [1.0, NAN, -1.0, 1.0, -1.0, 1.0],
            [1.0, -INF, 1.0, 1.0, 1.0, 1.0],
        ],
    );
    for (name, v) in [
        ("algo/bernstein_roots", seeded),
        ("algo/bernstein_roots/edge", &e),
    ] {
        out.push(Case::map(
            name,
            same(v),
            |c: &Quintic| bernstein_roots(c, 0.0, 1.0, RootLimits::DEFAULT),
            move |c: &Quintic| {
                let mut work = subdivisions;
                let mut found = Vec::new();
                let isolated = pin_bern::bernstein_roots(c, 0.0, 1.0, depth, &mut work, &mut found);
                (found, isolated)
            },
        ));
    }
}

/// Every case, in order.
pub fn cases(out: &mut Vec<Case>) {
    let (s, i, q, p) = inputs();
    steps(out, &s);
    integrations(out, &i);
    quadratures(out, &q);
    roots(out, &p);
}
