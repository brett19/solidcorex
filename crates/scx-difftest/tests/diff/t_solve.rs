// Written for solidcorex. Apache-2.0.
//! `scx-algo`: `local_solve1d.rs` and `local_solve_nd.rs`.
//!
//! Every run compares the whole iteration: the outcome or the error (its `Display` text
//! and the pin's error it maps onto, class, operation, typed failure and cause), the
//! evaluator's log (each point evaluated, in order, with whether a Jacobian was asked, and
//! whether that call failed), and the solver's state after a 1-D solve (iteration counter,
//! current and previous values, brackets, found accuracy, boundary hits, termination
//! reason) or an N-D solve's report (termination reason, iterations, whether the guess
//! went unevaluated). A failed 1-D solve's state is not compared: scx returns it only with
//! an outcome.
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
        let ivls: &[_] = if full || func == 0 { &IVLS } else { &IVLS[..4] };
        for &ivl in ivls {
            let algs: &[i32] = if full || matches!(func, 1 | 9) {
                &[0, 1, 2, 3]
            } else {
                &[0, 3]
            };
            for &alg in algs {
                let guesses: &[f64] = if full {
                    &[0.0, 1.0, 2.5, 1.2]
                } else if func == 0 {
                    &[1.0, 1.2]
                } else if matches!(func, 1 | 9 | 10) {
                    &[0.0, 1.0]
                } else {
                    &[1.0]
                };
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

/// One 1-D run under `m`: (outcome or error, log).
macro_rules! run1 {
    ($c:expr, $k:expr, $persist:expr, $class:expr) => {{
        let c: Cfg1 = $c;
        let mut rec = crate::solvers::Rec1::new(c.func, $k, $persist, $class);
        let (a, b) = c.ab;
        let r = match c.entry {
            0 => m::sv::solve1d(&mut rec, c.ivl, c.alg, c.guess, c.acc)
                .map(|((found, t), st)| ((found, t, 0.0), Some(st))),
            1 | 2 => m::sv::brent(&mut rec, (a, c.guess, b), c.acc, c.entry == 2)
                .map(|(t, f)| ((false, t, f), None)),
            _ => {
                let (fa, fb) = (
                    crate::solvers::f1(c.func, a).0,
                    crate::solvers::f1(c.func, b).0,
                );
                let (lo, hi) = if fa <= fb {
                    ((a, fa), (b, fb))
                } else {
                    ((b, fb), (a, fa))
                };
                m::sv::bisection(&mut rec, c.levels, lo, hi).map(|(t, f)| ((false, t, f), None))
            }
        };
        (r, rec.log)
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

diff!(solve1d_queue, |size: usize| m::sv::queue(
    core::num::NonZeroUsize::new(size).unwrap_or(core::num::NonZeroUsize::MIN)
));

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
        &[
            None,
            Some([[0.0, 1.5], [0.0, 1.0]]),
            Some([[-2.0, 2.0], [-2.0, 2.0]]),
        ]
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
                            &[[0.0, 0.0], [0.5, 0.0], [1.2, -0.7], [1.0, 4.0]]
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

/// One N-D run under `m`: (report or error, log).
macro_rules! runn {
    ($c:expr, $k:expr, $persist:expr, $class:expr) => {{
        let c: CfgN = $c;
        let mut rec = crate::solvers::RecN::new(c.func, $k, $persist, $class);
        let r = m::sv::solve_nd(
            &mut rec, c.ivl, c.periodic, c.handler, c.cutting, c.guess, c.maxit, c.acc,
        );
        (r, rec.log)
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

// The point interval at `(a, b)` is skipped where an end is not a plain finite value: it has
// no scx extent (`plain_ends`).
diff!(solve_nd_jacobian, |a: f64, b: f64| {
    let mut rec = crate::solvers::RecN::new(0, usize::MAX, false, Class::SmErr);
    let mut out = Vec::new();
    let point = crate::facade::plain_ends(&[a, b]).then_some([[a, a], [b, b]]);
    let ivls = [None, Some([[-2.0, 2.0], [-2.0, 2.0]])];
    for ivl in ivls.into_iter().chain(point.map(Some)) {
        out.push(m::sv::jacobian_at(&mut rec, [a, b], ivl));
    }
    (out, rec.log)
});

diff!(solve_nd_sizes, |n: usize| m::sv::solve_diag(n % 5));

// The default's interval is not compared: scx's default domain is the whole line, with
// no interval to read (the pin's is the unused inside-out `Extent1d::new()`).
// An interval end that is not a plain finite value has no scx extent (`plain_ends`).
diff!(solve1d_settings, |code: i32, lo: f64, hi: f64, acc: f64| {
    crate::facade::plain_ends(&[lo, hi]).then(|| m::sv::settings(code, lo, hi, acc))
});
