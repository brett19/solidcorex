// Written for solidcorex. Apache-2.0.
//! `scx-algo`: `local_solve1d.rs` and `local_solve_nd.rs`.
//!
//! Every run compares the whole iteration: the outcome or the error (its `Display` text
//! and the pin's error it maps onto, class, operation, typed failure and cause), the
//! evaluator's log (each point evaluated, in order, with `signal_errors` or whether a
//! Jacobian was asked, and whether that call failed), and the solver's public state after
//! the run (iteration counter, current and previous values, brackets, found accuracy,
//! boundary hits, termination reason).
//!
//! The `*_clean` tests run every configuration with an evaluator that never fails. The
//! `*_fail` tests take a smaller set of configurations, count the calls of the clean run,
//! and repeat it failing at each call in turn (so the first evaluation, each Newton step,
//! the oscillation guesses, the bisection and Brent evaluations at the bracket ends, the
//! finite-difference Jacobian's evaluations, the scaled candidate and the best point's
//! reevaluation), once and from there on, with an error of each class
//! (`solvers::CLASSES`). The `usize` argument splits the configurations into 13 shares.

use crate::diff;
use crate::solvers::{Class, CLASSES, F1, FN};

/// Calls swept per configuration.
const SWEEP: usize = 48;

/// A 1-D run: `entry` 0 `solve_it`, 1 `solve_by_brent` minimising, 2 maximising,
/// 3 `solve_by_bisection`.
#[derive(Clone, Copy)]
struct Cfg1 {
    entry: u8,
    func: usize,
    ivl: Option<(f64, f64, bool)>,
    alg: i32,
    guess: f64,
    acc: f64,
    ab: (f64, f64),
    levels: u64,
}

const IVLS: [Option<(f64, f64, bool)>; 5] = [
    None,
    Some((-3.0, 3.0, false)),
    Some((0.0, 10.0, false)),
    Some((0.0, 1.0, true)),
    Some((1.0, 1.2, false)),
];

fn cfgs1(full: bool) -> Vec<Cfg1> {
    let mut out = Vec::new();
    let base = Cfg1 {
        entry: 0,
        func: 0,
        ivl: None,
        alg: 0,
        guess: 0.0,
        acc: 1e-9,
        ab: (0.0, 1.0),
        levels: 40,
    };
    for func in 0..F1 {
        let ivls: &[_] = if full { &IVLS } else { &IVLS[..4] };
        for &ivl in ivls {
            let algs: &[i32] = if full { &[0, 1, 2, 3] } else { &[0, 3] };
            for &alg in algs {
                let guesses: &[f64] = if full { &[0.0, 1.0, 2.5] } else { &[1.0] };
                for &guess in guesses {
                    let accs: &[f64] = if full { &[1e-9, 1e-3] } else { &[1e-3] };
                    for &acc in accs {
                        out.push(Cfg1 {
                            func,
                            ivl,
                            alg,
                            guess,
                            acc,
                            ..base
                        });
                    }
                }
            }
        }
        for entry in [1, 2] {
            for ab in [(-1.0, 2.0), (0.0, 1.0)] {
                for guess in [0.3, 0.5] {
                    for acc in [1e-12, 1e-6] {
                        if full || acc == 1e-6 {
                            out.push(Cfg1 {
                                entry,
                                func,
                                guess,
                                acc,
                                ab,
                                ..base
                            });
                        }
                    }
                }
            }
        }
        for ab in [(1.0, 2.0), (0.0, 1.0), (-3.0, 3.0)] {
            for levels in [0, 3, 40] {
                if full || levels == 40 {
                    out.push(Cfg1 {
                        entry: 3,
                        func,
                        ab,
                        levels,
                        ..base
                    });
                }
            }
        }
    }
    out
}

/// One 1-D run under `m`: (outcome or error, log, state).
macro_rules! run1 {
    ($c:expr, $k:expr, $persist:expr, $class:expr) => {{
        let c: Cfg1 = $c;
        let mut ev = m::sv::Rec1::new(c.func, $k, $persist, $class);
        let mut s = match c.ivl {
            None => m::sv::local_solve1d::LocalSolve1d::new(),
            Some((a, b, p)) => m::sv::local_solve1d::LocalSolve1d::with_interval(
                m::sv::extent1d::Extent1d::from_min_max(a, b),
                p,
            ),
        };
        if let Some(alg) = m::sv::local_solve1d::SolverAlgorithm::from_code(c.alg) {
            s.set_solver_algorithm(alg);
        }
        let (a, b) = c.ab;
        let r = match c.entry {
            0 => s.solve_it(&mut ev, c.guess, c.acc).map(|o| (o.found, o.t, 0.0)),
            1 | 2 => s
                .solve_by_brent(&mut ev, a, c.guess, b, c.acc, c.entry == 2)
                .map(|(t, f)| (false, t, f)),
            _ => {
                let (fa, fb) = (crate::solvers::f1(c.func, a).0, crate::solvers::f1(c.func, b).0);
                let (lo, hi, flo, fhi) = if fa <= fb { (a, b, fa, fb) } else { (b, a, fb, fa) };
                s.solve_by_bisection(&mut ev, c.levels, lo, hi, flo, fhi)
                    .map(|(t, f)| (false, t, f))
            }
        };
        let iv = |v: &m::sv::local_solve1d::IterationValue| (v.t, v.f, v.f_prime, v.prev_step);
        let state = (
            (s.iter, iv(&s.curr), iv(&s.prev1), iv(&s.prev2)),
            (
                s.have_lower_bracket,
                s.lower_bracket_t,
                s.lower_bracket_f,
                s.have_upper_bracket,
                s.upper_bracket_t,
                s.upper_bracket_f,
            ),
            (
                s.found_accuracy(),
                s.lower_bound_hits(),
                s.upper_bound_hits(),
                s.termination_reason().code(),
            ),
        );
        (r.map_err(m::sv::pin), ev.log, state)
    }};
}

diff!(solve1d_clean, |share: usize| {
    let mut out = Vec::new();
    for (i, c) in cfgs1(true).into_iter().enumerate() {
        if i % 13 == share {
            out.push(run1!(c, usize::MAX, false, Class::SmErr));
        }
    }
    out
});

diff!(solve1d_fail, |share: usize| {
    let mut out = Vec::new();
    for (i, c) in cfgs1(false).into_iter().enumerate() {
        if i % 13 != share {
            continue;
        }
        let clean = run1!(c, usize::MAX, false, Class::SmErr);
        let calls = clean.1.len().min(SWEEP);
        out.push(clean);
        for k in 0..calls {
            for persist in [false, true] {
                for class in CLASSES {
                    out.push(run1!(c, k, persist, class));
                }
            }
        }
    }
    out
});

diff!(solve1d_default_evaluate, || {
    let mut s = m::sv::local_solve1d::LocalSolve1d::with_interval(
        m::sv::extent1d::Extent1d::from_min_max(0.0, 1.0),
        false,
    );
    let a = s.solve_it(&mut m::sv::Base1, 0.5, 1e-9).map(|o| (o.found, o.t));
    let b = s
        .solve_by_brent(&mut m::sv::Base1, 0.0, 0.5, 1.0, 1e-9, false)
        .map(|_| ());
    let c = m::sv::local_solve1d::LocalSolve1d::new()
        .solve_it(&mut m::sv::Base1, 0.5, 1e-9)
        .map(|o| (o.found, o.t));
    (a.map_err(m::sv::pin), b.map_err(m::sv::pin), c.map_err(m::sv::pin))
});

diff!(solve1d_queue, |size: usize| {
    let q = m::sv::local_solve1d::IterQueue1d::try_new(size as u64).is_ok();
    let big = m::sv::local_solve1d::IterQueue1d::try_new(u64::MAX)
        .map(|_| ())
        .map_err(|e| e.to_string());
    let mut q2 = m::sv::local_solve1d::IterQueue1d::new(size as u64);
    let mut osc = Vec::new();
    for i in 0..20u32 {
        q2.add(&m::sv::local_solve1d::IterationValue {
            prev_step: if i % 2 == 0 { 1.0 } else { -1.01 },
            ..Default::default()
        });
        osc.push((q2.is_oscillating(2), q2.is_oscillating(3)));
    }
    (q, big, osc)
});

/// An N-D run.
#[derive(Clone, Copy)]
struct CfgN {
    func: usize,
    ivl: Option<[[f64; 2]; 2]>,
    periodic: bool,
    handler: u8,
    cutting: bool,
    guess: [f64; 2],
    maxit: u64,
    acc: f64,
}

fn cfgsn(full: bool) -> Vec<CfgN> {
    let mut out = Vec::new();
    let ivls: &[Option<[[f64; 2]; 2]>] = if full {
        &[None, Some([[0.0, 1.5], [0.0, 1.0]]), Some([[-2.0, 2.0], [-2.0, 2.0]])]
    } else {
        &[None, Some([[0.0, 1.5], [0.0, 1.0]])]
    };
    for func in 0..FN {
        for &ivl in ivls {
            for periodic in [false, true] {
                if (periodic && ivl.is_none()) || (periodic && !full) {
                    continue;
                }
                for handler in 0..3u8 {
                    if !full && handler != 1 {
                        continue;
                    }
                    for cutting in [false, true] {
                        let guesses: &[[f64; 2]] = if full {
                            &[[0.0, 0.0], [0.5, 0.0], [1.2, -0.7]]
                        } else {
                            &[[0.0, 0.0], [0.5, 0.0]]
                        };
                        for &guess in guesses {
                            for maxit in [100, 3] {
                                for acc in [1e-12, 1e-3] {
                                    if !full && (maxit != 100 || acc != 1e-3) {
                                        continue;
                                    }
                                    out.push(CfgN {
                                        func,
                                        ivl,
                                        periodic,
                                        handler,
                                        cutting,
                                        guess,
                                        maxit,
                                        acc,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// One N-D run under `m`: (solution or error, log, state).
macro_rules! runn {
    ($c:expr, $k:expr, $persist:expr, $class:expr) => {{
        let c: CfgN = $c;
        let mut ev = m::sv::RecN::new(c.func, $k, $persist, $class);
        let ivls = c.ivl.map(|a| {
            let mut e = m::sv::extent1d::ExtentNd::new(2);
            for (i, [lo, hi]) in a.into_iter().enumerate() {
                if let Some(s) = e.get_mut(i) {
                    *s = m::sv::extent1d::Extent1d::from_min_max(lo, hi);
                }
            }
            e
        });
        let per = [true, false];
        let periodicities = c.periodic.then_some(&per[..]);
        let mut s = m::sv::local_solve_nd::LocalSolveNd::new(&mut ev, ivls.as_ref(), periodicities);
        s.set_step_cutting_enabled(c.cutting);
        s.set_maximum_iterations(c.maxit);
        let handler = match c.handler {
            0 => m::sv::local_solve_nd::BoundaryHandler::TotalBoundaryHits,
            1 => m::sv::local_solve_nd::BoundaryHandler::HitsAllBoundaries,
            _ => m::sv::local_solve_nd::BoundaryHandler::IgnoreBoundaryHits,
        };
        s.set_boundary_handler(handler, 3);
        let r = s.solve_it(&c.guess, c.acc).map(|n| (n.found, n.x));
        let state = (
            s.termination_reason().code(),
            s.current_iteration(),
            s.guess_unevaluated(),
        );
        (r.map_err(m::sv::pin), ev.log, state)
    }};
}

diff!(solve_nd_clean, |share: usize| {
    let mut out = Vec::new();
    for (i, c) in cfgsn(true).into_iter().enumerate() {
        if i % 13 == share {
            out.push(runn!(c, usize::MAX, false, Class::SmErr));
        }
    }
    out
});

diff!(solve_nd_fail, |share: usize| {
    let mut out = Vec::new();
    for (i, c) in cfgsn(false).into_iter().enumerate() {
        if i % 13 != share {
            continue;
        }
        let clean = runn!(c, usize::MAX, false, Class::SmErr);
        let calls = clean.1.len().min(SWEEP);
        out.push(clean);
        for k in 0..calls {
            for persist in [false, true] {
                for class in CLASSES {
                    out.push(runn!(c, k, persist, class));
                }
            }
        }
    }
    out
});

diff!(solve_nd_jacobian, |a: f64, b: f64| {
    let mut ev = m::sv::RecN::new(0, usize::MAX, false, Class::SmErr);
    let mut out = Vec::new();
    for ivl in [None, Some([[-2.0, 2.0], [-2.0, 2.0]]), Some([[a, a], [b, b]])] {
        let e = ivl.map(|v: [[f64; 2]; 2]| {
            v.map(|[lo, hi]| m::sv::extent1d::Extent1d::from_min_max(lo, hi))
        });
        let mut x = [a, b];
        let f = [0.0, 0.0];
        let mut j = m::sv::matrix::Matrix::new(2, 2);
        let r = m::sv::local_solve_nd::compute_jacobian(
            &mut ev,
            &mut x,
            &f,
            &mut j,
            e.as_ref().map(|v| &v[..]),
        );
        let ja: Vec<f64> = (0..4).map(|i| j.at(i / 2, i % 2).unwrap_or(f64::NAN)).collect();
        out.push((r.map_err(m::sv::pin), x, ja));
    }
    (out, ev.log)
});
