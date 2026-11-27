// Written for solidcorex. Apache-2.0.
//! scx-algo's cases against the pin (targets `algo` and `algo_ic`). Each case is a group
//! `algo/<case>` holding `scx` and `pin` over the same seeded inputs, and a group
//! `algo/<case>/edge` over its edge inputs.
//!
//! The cases: the polynomial roots (`quadratic`, `cubic`, `quartic`: half from real roots,
//! half from random coefficients); the 1-D local solver on `t^3 + p t - q` (`solve1d`); the
//! N-D local solver on a circle and a line (`solve_nd`); and, on the same circle and line,
//! Newton's root (`newton2`) and Krawczyk's test (`krawczyk2`), with Krawczyk's test of
//! `x^2 - a` (`krawczyk1`) besides. The global solver's cases are
//! [`crate::algo_gsolve`]'s; the integrators' and Bernstein roots' are `algo_ode`'s.

use crate::case::{same, Case};
use crate::{edges, pin, seeded, NoError, Rng};

use scx_algo::krawczyk;
use scx_algo::local_solve1d::LocalSolve1d as XSolve1d;
use scx_algo::local_solve_nd::LocalSolveNd as XSolveNd;
use scx_algo::polynomial as xpoly;
use scx_num::interval::Ivl as XIvl;
use solidcore::blend::fillet_section::interval::Ivl as PIvl;
use solidcore::error::Result as PResult;
use solidcore::geom::local_solve1d::{EvalFunction, FunctionValue, LocalSolve1d as PSolve1d};
use solidcore::geom::local_solve_nd::{EvalNFunctions, LocalSolveNd as PSolveNd};
use solidcore::geom::matrix::Matrix;
use solidcore::geom::polynomial as ppoly;

const NAN: f64 = f64::NAN;
const INF: f64 = f64::INFINITY;
/// The accuracy every solve accepts, and the polynomials' zero tolerance.
const ACC: f64 = 1e-12;

/// `lead * (x - r0) (x - r1) ...`, lowest power first.
fn from_roots<const K: usize>(lead: f64, roots: &[f64]) -> [f64; K] {
    let mut c = [0.0; K];
    c[0] = lead;
    for (k, &r) in roots.iter().enumerate() {
        // multiply by (x - r): shift up, subtract r times
        for i in (0..=k + 1).rev() {
            let up = if i == 0 { 0.0 } else { c[i - 1] };
            c[i] = up - r * c[i];
        }
    }
    c
}

/// A degree `K - 1` polynomial: even draws from real roots in `[-3, 3]`, odd draws
/// random coefficients in `[-1, 1]`.
fn poly<const K: usize>(r: &mut Rng) -> [f64; K] {
    if r.next_u64().is_multiple_of(2) {
        let lead = r.range(0.5, 2.0);
        let roots: [f64; K] = r.array(-3.0, 3.0);
        from_roots(lead, &roots[..K - 1])
    } else {
        r.array(-1.0, 1.0)
    }
}

/// Edge polynomials: repeated roots, a zero or tiny leading coefficient, all zero, NaN and
/// infinite coefficients.
fn poly_edges<const K: usize>() -> Vec<[f64; K]> {
    let ones = [1.0; K];
    let mut out = vec![
        from_roots(1.0, &ones[..K - 1]),
        from_roots(1.0, &[0.0; K][..K - 1]),
        [0.0; K],
    ];
    let mut lead0 = from_roots::<K>(1.0, &[1.0, -2.0, 0.5, 3.0][..K - 1]);
    lead0[K - 1] = 0.0;
    out.push(lead0);
    lead0[K - 1] = 1e-300;
    out.push(lead0);
    let mut nan = [1.0; K];
    nan[0] = NAN;
    out.push(nan);
    let mut inf = [1.0; K];
    inf[K - 1] = INF;
    out.push(inf);
    out
}

fn polynomials(out: &mut Vec<Case>) {
    let q = seeded(31, poly::<3>);
    let q_e = edges(32, &poly_edges::<3>());
    for (name, v) in [("algo/quadratic", &q), ("algo/quadratic/edge", &q_e)] {
        out.push(Case::map(
            name,
            same(v),
            |k| xpoly::solve_quadratic(k, ACC),
            |k| ppoly::solve_quadratic(k, ACC),
        ));
    }
    let cu = seeded(33, poly::<4>);
    let cu_e = edges(34, &poly_edges::<4>());
    for (name, v) in [("algo/cubic", &cu), ("algo/cubic/edge", &cu_e)] {
        out.push(Case::map(
            name,
            same(v),
            |k| xpoly::solve_cubic(k, ACC),
            |k| ppoly::solve_cubic(k, ACC),
        ));
    }
    let qu = seeded(35, poly::<5>);
    let qu_e = edges(36, &poly_edges::<5>());
    for (name, v) in [("algo/quartic", &qu), ("algo/quartic/edge", &qu_e)] {
        out.push(Case::map(
            name,
            same(v),
            |k| xpoly::solve_quartic(k, ACC),
            |k| ppoly::solve_quartic(k, ACC),
        ));
    }
}

/// `t^3 + p t - q` from `guess`: `[p, q, guess]`.
type Cubic = [f64; 3];

/// The pin's evaluator of a [`Cubic`].
struct PinCubic(f64, f64);

impl EvalFunction for PinCubic {
    fn evaluate(&mut self, t: f64, out: &mut FunctionValue, _: bool) -> PResult<()> {
        out.f = t * t * t + self.0 * t - self.1;
        out.f_prime = 3.0 * t * t + self.0;
        out.found_answer = false;
        Ok(())
    }
}

/// The circle `(x - c)^2 + y^2 = r2` and the line `a x + b y = 1`, solved from
/// `guess`: `[c, r2, a, b, guess x, guess y]`.
type CircleLine = [f64; 6];

/// Its values and Jacobian (rows) at `x`.
fn circle_line(k: &CircleLine, x: [f64; 2]) -> ([f64; 2], [[f64; 2]; 2]) {
    let [c, r2, a, b, ..] = *k;
    let dx = x[0] - c;
    (
        [dx * dx + x[1] * x[1] - r2, a * x[0] + b * x[1] - 1.0],
        [[2.0 * dx, 2.0 * x[1]], [a, b]],
    )
}

/// The pin's evaluator of a [`CircleLine`].
struct PinCircleLine(CircleLine);

impl EvalNFunctions for PinCircleLine {
    fn evaluate(
        &mut self,
        x: &[f64],
        f: &mut [f64],
        jacobian: Option<&mut Matrix>,
    ) -> PResult<bool> {
        let at = [
            x.first().copied().unwrap_or(0.0),
            x.get(1).copied().unwrap_or(0.0),
        ];
        let (v, j) = circle_line(&self.0, at);
        for (d, s) in f.iter_mut().zip(v) {
            *d = s;
        }
        if let Some(m) = jacobian {
            for (i, row) in j.iter().enumerate() {
                for (k, &e) in row.iter().enumerate() {
                    m.set_at(i, k, e)?;
                }
            }
        }
        Ok(false)
    }
}

/// A circle and line meeting at a random point, and a guess near it.
fn circle_line_case(r: &mut Rng) -> CircleLine {
    let x = r.range(-2.0, 2.0);
    let y = r.range(0.5, 2.0)
        * if r.next_u64().is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
    let c = r.range(-1.0, 1.0);
    let a = r.range(-1.0, 1.0);
    let b = (1.0 - a * x) / y;
    let r2 = (x - c) * (x - c) + y * y;
    [c, r2, a, b, x + r.range(-0.3, 0.3), y + r.range(-0.3, 0.3)]
}

/// Edge circle-line systems: a singular Jacobian at the guess (the centre), a tangent
/// line (a double root), no meeting, NaN and infinite guesses, a zero circle.
const CIRCLE_LINE_EDGES: [CircleLine; 6] = [
    [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 1.0, 0.0, 0.9, 0.1],
    [0.0, 1.0, 0.25, 0.0, 1.0, 1.0],
    [0.0, 1.0, 1.0, 1.0, NAN, 0.0],
    [0.0, 1.0, 1.0, 1.0, INF, -INF],
    [0.0, 0.0, 1.0, 1.0, 0.5, 0.5],
];

fn solvers(out: &mut Vec<Case>) {
    let cubic = |r: &mut Rng| [r.range(0.5, 3.0), r.range(-5.0, 5.0), r.range(-2.0, 2.0)];
    let s1 = seeded(41, cubic);
    // a flat start, Newton's two-cycle (t^3 - 2t + 2 from 0), a NaN guess, an infinite q
    let s1_e = edges(
        42,
        &[
            [0.0, 0.0, 0.0],
            [-2.0, -2.0, 0.0],
            [1.0, 1.0, NAN],
            [1.0, INF, 0.0],
        ],
    );
    for (name, v) in [("algo/solve1d", &s1), ("algo/solve1d/edge", &s1_e)] {
        out.push(Case::map(
            name,
            same(v),
            |&[p, q, g]: &Cubic| {
                XSolve1d {
                    acceptable_accuracy: ACC,
                    ..XSolve1d::new()
                }
                .solve(
                    |t| Ok::<_, NoError>((t * t * t + p * t - q, 3.0 * t * t + p)),
                    g,
                )
            },
            |&[p, q, g]: &Cubic| PSolve1d::new().solve_it(&mut PinCubic(p, q), g, ACC),
        ));
    }
    let nd = seeded(43, circle_line_case);
    let nd_e = edges(44, &CIRCLE_LINE_EDGES);
    for (name, v) in [("algo/solve_nd", &nd), ("algo/solve_nd/edge", &nd_e)] {
        out.push(Case::map(
            name,
            same(v),
            |k: &CircleLine| {
                XSolveNd::<2> {
                    acceptable_accuracy: ACC,
                    ..XSolveNd::new()
                }
                .solve(|x| Ok::<_, NoError>(circle_line(k, *x)), [k[4], k[5]])
            },
            |k: &CircleLine| {
                PSolveNd::new(&mut PinCircleLine(*k), None, None).solve_it(&[k[4], k[5]], ACC)
            },
        ));
    }
}

/// The Jacobian's columns, from its rows.
fn columns<T: Copy>(j: [[T; 2]; 2]) -> [[T; 2]; 2] {
    [[j[0][0], j[1][0]], [j[0][1], j[1][1]]]
}

/// `[mid x, mid y]`, the circle-line system and the approximate inverse (rows) at `mid`:
/// the system's root, moved by about `1e-10`.
type KCase = ([f64; 2], CircleLine, [[f64; 2]; 2]);

fn kcase(k: &CircleLine, mid: [f64; 2]) -> KCase {
    let (_, [[p, q], [r, s]]) = circle_line(k, mid);
    let det = p * s - q * r;
    (mid, *k, [[s / det, -q / det], [-r / det, p / det]])
}

fn krawczyks(out: &mut Vec<Case>) {
    // x^2 - a about a mid near its root
    let k1 = seeded(51, |r| {
        let root = r.range(1.0, 3.0);
        [root * root, root * (1.0 + 1e-9 * r.range(-1.0, 1.0))]
    });
    // no root, a double root at the mid (an infinite preconditioner), a NaN mid, a huge a
    let k1_e = edges(52, &[[-1.0, 1.0], [0.0, 0.0], [4.0, NAN], [1e300, 1e150]]);
    for (name, v) in [("algo/krawczyk1", &k1), ("algo/krawczyk1/edge", &k1_e)] {
        out.push(Case::map(
            name,
            same(v),
            |&[a, m]: &[f64; 2]| {
                krawczyk::krawczyk(
                    [m],
                    [[1.0 / (2.0 * m)]],
                    |x: [XIvl; 1]| Ok::<_, NoError>(Some([x[0] * x[0] - XIvl::from(a)])),
                    |x: [XIvl; 1]| Ok(Some([[XIvl::from(2.0) * x[0]]])),
                )
            },
            |&[a, m]: &[f64; 2]| {
                pin::krawczyk(
                    [m],
                    [[1.0 / (2.0 * m)]],
                    &|x: [PIvl; 1]| Ok(Some([x[0] * x[0] - PIvl::from(a)])),
                    &|x: [PIvl; 1]| Ok(Some([[PIvl::from(2.0) * x[0]]])),
                )
            },
        ));
    }

    let sys = seeded(53, circle_line_case);
    let sys_e = edges(54, &CIRCLE_LINE_EDGES);
    for (name, v) in [("algo/newton2", &sys), ("algo/newton2/edge", &sys_e)] {
        out.push(Case::map(
            name,
            same(v),
            |k: &CircleLine| {
                krawczyk::newton([k[4], k[5]], |y| {
                    let (f, j) = circle_line(k, y);
                    Ok::<_, NoError>(Some((f, columns(j))))
                })
            },
            |k: &CircleLine| {
                pin::newton(
                    [k[4], k[5]],
                    &|y| {
                        let (f, j) = circle_line(k, y);
                        Ok(Some((f, columns(j))))
                    },
                    &pin::inverse_of_columns2,
                )
            },
        ));
    }

    // Newton's root, moved by about 1e-10, as the mid
    let roots: Vec<KCase> = seeded(55, circle_line_case)
        .iter()
        .filter_map(|k| {
            let at = krawczyk::newton([k[4], k[5]], |y| {
                let (f, j) = circle_line(k, y);
                Ok::<_, NoError>(Some((f, columns(j))))
            });
            let root = at.ok().flatten()?;
            Some(kcase(k, root.map(|v| v * (1.0 + 1e-10))))
        })
        .collect();
    let roots_e: Vec<KCase> = edges(56, &CIRCLE_LINE_EDGES)
        .iter()
        .map(|k| kcase(k, [k[4], k[5]]))
        .collect();
    for (name, v) in [
        ("algo/krawczyk2", &roots),
        ("algo/krawczyk2/edge", &roots_e),
    ] {
        out.push(Case::map(
            name,
            same(v),
            |(mid, k, inv): &KCase| {
                let [cx, r2, a, b, ..] = *k;
                krawczyk::krawczyk(
                    *mid,
                    *inv,
                    |x: [XIvl; 2]| {
                        let dx = x[0] - XIvl::from(cx);
                        Ok::<_, NoError>(Some([
                            dx * dx + x[1] * x[1] - XIvl::from(r2),
                            XIvl::from(a) * x[0] + XIvl::from(b) * x[1] - XIvl::from(1.0),
                        ]))
                    },
                    |x: [XIvl; 2]| {
                        let two = XIvl::from(2.0);
                        Ok(Some([
                            [two * (x[0] - XIvl::from(cx)), XIvl::from(a)],
                            [two * x[1], XIvl::from(b)],
                        ]))
                    },
                )
            },
            |(mid, k, inv): &KCase| {
                let [cx, r2, a, b, ..] = *k;
                pin::krawczyk(
                    *mid,
                    *inv,
                    &|x: [PIvl; 2]| {
                        let dx = x[0] - PIvl::from(cx);
                        Ok(Some([
                            dx * dx + x[1] * x[1] - PIvl::from(r2),
                            PIvl::from(a) * x[0] + PIvl::from(b) * x[1] - PIvl::from(1.0),
                        ]))
                    },
                    &|x: [PIvl; 2]| {
                        let two = PIvl::from(2.0);
                        Ok(Some([
                            [two * (x[0] - PIvl::from(cx)), PIvl::from(a)],
                            [two * x[1], PIvl::from(b)],
                        ]))
                    },
                )
            },
        ));
    }
}

/// Every case, in order.
#[must_use]
pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    polynomials(&mut out);
    solvers(&mut out);
    krawczyks(&mut out);
    crate::algo_gsolve::cases(&mut out);
    crate::algo_ode::cases(&mut out);
    out
}
