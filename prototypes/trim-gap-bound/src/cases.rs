//! The study's cases, shared by the bench and the tests.

use core::f64::consts::{FRAC_PI_2, PI};

use crate::gap::Problem;
use crate::surface::{Bezier, HermiteNet, Torus};

/// A wavy bicubic Bézier patch on the unit square.
pub fn wavy_patch() -> Bezier {
    let z = [[0.0, 0.25, -0.1, 0.05], [0.2, -0.3, 0.35, 0.1], [-0.15, 0.4, -0.25, 0.2], [0.05, -0.1, 0.3, 0.0]];
    let mut p = [[[0.0; 3]; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            p[i][j] = [i as f64 / 3.0, j as f64 / 3.0 + 0.08 * (i as f64 - 1.5), z[i][j]];
        }
    }
    Bezier { p }
}

/// A UV curve across the unit square.
pub fn square_curve(t: f64) -> ([f64; 2], [f64; 2]) {
    ([0.15 + 0.7 * t, 0.5 + 0.25 * (PI * t).sin()], [0.7, 0.25 * PI * (PI * t).cos()])
}

/// A UV curve across the first quadrant.
pub fn quadrant_curve(t: f64) -> ([f64; 2], [f64; 2]) {
    let s = (PI * t).sin();
    ([0.2 + 1.1 * t, 0.3 + 0.8 * t + 0.2 * s], [1.1, 0.8 + 0.2 * PI * (PI * t).cos()])
}

/// A UV curve rising to `v = pi/2 - delta` (the sphere's pole at `delta = 0`) at `t = 1/2`.
pub fn pole_curve(delta: f64) -> impl Fn(f64) -> ([f64; 2], [f64; 2]) + Copy {
    move |t: f64| {
        let top = FRAC_PI_2 - delta - 0.3;
        let s = (PI * t).sin();
        ([0.2 + 1.1 * t, 0.3 + top * s * s], [1.1, top * 2.0 * s * PI * (PI * t).cos()])
    }
}

/// The offset of the wavy patch along `square_curve`.
pub fn bezier_offset(d: f64, n_trim: usize, n_edge: usize) -> Problem<Bezier> {
    Problem::fitted(wavy_patch(), d, square_curve, [0.0, 1.0], n_trim, n_edge)
}

/// The offset of the quarter torus (3, 1) along `quadrant_curve`.
pub fn torus_offset(d: f64, n_trim: usize, n_edge: usize) -> Problem<Torus> {
    Problem::fitted(Torus { big: 3.0, small: 1.0 }, d, quadrant_curve, [0.0, 1.0], n_trim, n_edge)
}

/// The offset of the unit sphere octant along `pole_curve(delta)`.
pub fn sphere_offset(d: f64, delta: f64, n_trim: usize, n_edge: usize) -> Problem<Torus> {
    Problem::fitted(Torus { big: 0.0, small: 1.0 }, d, pole_curve(delta), [0.0, 1.0], n_trim, n_edge)
}

/// The blend case: the definition is the quarter torus (3, 1) (a rolling-ball blend of
/// constant radius is a torus), the net its `n x n` Hermite interpolant. The edge lies on
/// the definition; the trim is fitted to the same UV curve. Returns the problem on the
/// net, the problem on the definition (same trim and edge), and the net.
pub fn blend(n: usize, n_trim: usize, n_edge: usize) -> (Problem<HermiteNet>, Problem<Torus>, HermiteNet) {
    let def = Torus { big: 3.0, small: 1.0 };
    let on_def = Problem::fitted(def.clone(), 0.0, quadrant_curve, [0.0, 1.0], n_trim, n_edge);
    let net = HermiteNet::of(&def, n, n);
    let on_net = Problem { surf: net.clone(), d: 0.0, trim: on_def.trim.clone(), edge: on_def.edge.clone() };
    (on_net, on_def, net)
}
