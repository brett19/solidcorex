// Written for solidcorex. Apache-2.0.
//! `scx-algo`: `krawczyk.rs` (with `inverse4` and `solve4`), against the pin's file made
//! public by the build script (its items are `pub(crate)` there).
//!
//! The callbacks' error is the pin's `SolidError` on both sides (the scx functions are
//! generic over it), so an injected error is compared whole (`Display`, class, cause). Each
//! callback logs its calls, and fails at a chosen call (`fail / 3`) with an error
//! (`fail % 3 == 1`) or a `None` (`fail % 3 == 2`), so the number of calls, the points and
//! boxes evaluated and the result are compared at every call position. `solve4`'s singular
//! system is compared by class, operation and failure (`kz_err`; its text differs by design,
//! report 53 section 8).

use crate::diff;
use crate::harness::List;
use crate::solvers::{pin_error, Class};
use solidcore::SolidError;
use std::cell::RefCell;

/// Whether call `k` of a callback with injection `fail` fails, and how.
fn inject(fail: usize, k: usize) -> Option<bool> {
    (fail % 3 != 0 && k == fail / 3).then_some(fail % 3 == 1)
}

diff!(kz_inverse4, |a: [[f64; 4]; 4], b: [f64; 4]| (
    m::krawczyk::inverse4(&a).map_err(|e| m::kz_err(&e)),
    m::krawczyk::solve4(&a, &b).map_err(|e| m::kz_err(&e)),
    m::krawczyk::inverse_of_columns4(&a),
));

diff!(kz_inverse23, |a: [f64; 4], c: [f64; 6], d: [f64; 3]| {
    let cols = [[c[0], c[1], c[2]], [c[3], c[4], c[5]], d];
    (
        m::krawczyk::inverse_of_columns2(&[[a[0], a[1]], [a[2], a[3]]]),
        m::krawczyk::inverse_of_columns3(&cols),
        m::krawczyk::inverse3(cols.map(m::v3)),
    )
});

diff!(kz_newton, |start: [f64; 2], p: [f64; 4], fail: usize| {
    // a circle of radius |p0| about (p1, 0) and the line p2 x + p3 y = 1
    let log = RefCell::new(Vec::new());
    let at = |y: [f64; 2]| -> Result<Option<([f64; 2], [[f64; 2]; 2])>, SolidError> {
        let k = log.borrow().len();
        log.borrow_mut().push(y);
        match inject(fail, k) {
            Some(true) => return Err(pin_error(Class::Cancelled)),
            Some(false) => return Ok(None),
            None => {}
        }
        let dx = y[0] - p[1];
        Ok(Some((
            [
                dx * dx + y[1] * y[1] - p[0] * p[0],
                p[2] * y[0] + p[3] * y[1] - 1.0,
            ],
            [[2.0 * dx, p[2]], [2.0 * y[1], p[3]]],
        )))
    };
    let r = m::krawczyk::newton(start, &at, &m::krawczyk::inverse_of_columns2);
    (r.map_err(|e| (e.to_string(), e)), log.into_inner())
});

diff!(kz_newton3, |start: [f64; 3], fail: usize| {
    // the sphere |y| = 2, the plane y0 + y1 + y2 = 1 and y0 = y1
    let log = RefCell::new(Vec::new());
    let at = |y: [f64; 3]| -> Result<Option<([f64; 3], [[f64; 3]; 3])>, SolidError> {
        let k = log.borrow().len();
        log.borrow_mut().push(y);
        match inject(fail, k) {
            Some(true) => return Err(pin_error(Class::OutOfFuel)),
            Some(false) => return Ok(None),
            None => {}
        }
        Ok(Some((
            [
                y[0] * y[0] + y[1] * y[1] + y[2] * y[2] - 4.0,
                y[0] + y[1] + y[2] - 1.0,
                y[0] - y[1],
            ],
            [
                [2.0 * y[0], 1.0, 1.0],
                [2.0 * y[1], 1.0, -1.0],
                [2.0 * y[2], 1.0, 0.0],
            ],
        )))
    };
    let r = m::krawczyk::newton(start, &at, &m::krawczyk::inverse_of_columns3);
    (r.map_err(|e| (e.to_string(), e)), log.into_inner())
});

diff!(kz_krawczyk1, |a: f64, mid: f64, fail: usize| {
    // x^2 - a about `mid`, preconditioned by 1 / (2 mid)
    use m::interval::Ivl;
    let vlog = RefCell::new(Vec::new());
    let jlog = RefCell::new(Vec::new());
    let value = |x: [Ivl; 1]| -> Result<Option<[Ivl; 1]>, SolidError> {
        let k = vlog.borrow().len() + jlog.borrow().len();
        vlog.borrow_mut().push(x);
        match inject(fail, k) {
            Some(true) => return Err(pin_error(Class::Unported)),
            Some(false) => return Ok(None),
            None => {}
        }
        Ok(Some([x[0] * x[0] - Ivl::from(a)]))
    };
    let jacobian = |x: [Ivl; 1]| -> Result<Option<[[Ivl; 1]; 1]>, SolidError> {
        let k = vlog.borrow().len() + jlog.borrow().len();
        jlog.borrow_mut().push(x);
        match inject(fail, k) {
            Some(true) => return Err(pin_error(Class::SmErr)),
            Some(false) => return Ok(None),
            None => {}
        }
        Ok(Some([[Ivl::from(2.0) * x[0]]]))
    };
    let c = [[1.0 / (2.0 * mid)]];
    let r = m::krawczyk::krawczyk_region([mid], c, &value, &jacobian);
    let r2 = m::krawczyk::krawczyk([mid], c, &value, &jacobian);
    (
        r.map_err(|e| (e.to_string(), e)),
        r2.map_err(|e| (e.to_string(), e)),
        vlog.into_inner(),
        jlog.into_inner(),
    )
});

diff!(kz_krawczyk2, |start: [f64; 2], p: [f64; 4], fail: usize| {
    // the circle and line of `kz_newton`, about that side's Newton root from `start`
    use m::interval::Ivl;
    let at = |y: [f64; 2]| -> Result<Option<([f64; 2], [[f64; 2]; 2])>, SolidError> {
        let dx = y[0] - p[1];
        Ok(Some((
            [
                dx * dx + y[1] * y[1] - p[0] * p[0],
                p[2] * y[0] + p[3] * y[1] - 1.0,
            ],
            [[2.0 * dx, p[2]], [2.0 * y[1], p[3]]],
        )))
    };
    let Ok(Some(mid)) = m::krawczyk::newton(start, &at, &m::krawczyk::inverse_of_columns2) else {
        return None;
    };
    let Ok(Some((_, jm))) = at(mid) else {
        return None;
    };
    let c = m::krawczyk::inverse_of_columns2(&jm)?;
    let log = RefCell::new(Vec::new());
    let value = |x: [Ivl; 2]| -> Result<Option<[Ivl; 2]>, SolidError> {
        let k = log.borrow().len();
        log.borrow_mut().push(x);
        match inject(fail, k) {
            Some(true) => return Err(pin_error(Class::PreparationRequired)),
            Some(false) => return Ok(None),
            None => {}
        }
        let dx = x[0] - Ivl::from(p[1]);
        Ok(Some([
            dx * dx + x[1] * x[1] - Ivl::from(p[0] * p[0]),
            Ivl::from(p[2]) * x[0] + Ivl::from(p[3]) * x[1] - Ivl::from(1.0),
        ]))
    };
    let jacobian = |x: [Ivl; 2]| -> Result<Option<[[Ivl; 2]; 2]>, SolidError> {
        let k = log.borrow().len();
        log.borrow_mut().push(x);
        match inject(fail, k) {
            Some(true) => return Err(pin_error(Class::FaceTrimsUnavailable)),
            Some(false) => return Ok(None),
            None => {}
        }
        let two = Ivl::from(2.0);
        Ok(Some([
            [two * (x[0] - Ivl::from(p[1])), Ivl::from(p[2])],
            [two * x[1], Ivl::from(p[3])],
        ]))
    };
    let r = m::krawczyk::krawczyk_region(mid, c, &value, &jacobian);
    Some((mid, c, r.map_err(|e| (e.to_string(), e)), log.into_inner()))
});

diff!(kz_operator, |y: [f64; 4],
                    jl: [f64; 4],
                    jw: [f64; 4],
                    x: [f64; 6]| {
    use m::interval::Ivl;
    let y = [[y[0], y[1]], [y[2], y[3]]];
    let iv = |a: f64, w: f64| Ivl::new(a, a + w.abs());
    let jac = [
        [iv(jl[0], jw[0]), iv(jl[1], jw[1])],
        [iv(jl[2], jw[2]), iv(jl[3], jw[3])],
    ];
    let mid = [x[0], x[1]];
    let g = [iv(x[2], 0.0), iv(x[3], 0.0)];
    let bx = [
        iv(x[0] - x[4].abs(), 2.0 * x[4]),
        iv(x[1] - x[5].abs(), 2.0 * x[5]),
    ];
    let (c, q) = m::krawczyk::contraction(&y, &jac);
    (
        (c, q),
        m::krawczyk::start_minus_y(mid.map(Ivl::from), &y, &g),
        m::krawczyk::plus_c_times(g, &c, &bx),
        m::krawczyk::k_operator(&mid, &y, &g, &c, &bx),
    )
});

diff!(kz_taylor, |c: List<[f64; 2]>,
                  rem: [f64; 2],
                  y: f64,
                  d: [f64; 2]| {
    use m::krawczyk::KNumber;
    let t = m::tm(&c, rem);
    let i = m::iv(d);
    let (cm, q) = m::krawczyk::contraction(&[[y]], &[[t]]);
    let (ci, qi) = m::krawczyk::contraction(&[[y]], &[[i]]);
    (
        (t.scaled(y), KNumber::hull(t), i.scaled(y), KNumber::hull(i)),
        (cm, q, ci, qi),
        m::krawczyk::start_minus_y([t], &[[y]], &[t]),
        m::krawczyk::plus_c_times([t], &cm, &[t]),
    )
});

diff!(kz_constants, || (
    m::krawczyk::KRAWCZYK_STEPS,
    m::krawczyk::NEWTON_STEPS
));
