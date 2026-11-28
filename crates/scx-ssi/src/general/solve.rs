// Written for solidcorex. Apache-2.0.
//! Every root of a square system in a box, each proved by Krawczyk, or a typed refusal
//! (research 73 §4.4, row SM5): the solver behind the critical-point solve and the seeds.
//!
//! A box is dropped when a component of the system excludes 0 over it, or when Krawczyk's
//! operator `K(X)` misses it (no root). Where `K(X')` lies inside the box `X'` (the box grown
//! by an eighth of each side, so a root on a face of `X` is inside `X'`), `X'` holds exactly
//! one root; it is refined by Newton and a tight Krawczyk box ([`krawczyk_region`]) and kept
//! when it touches the start box. Otherwise the box is halved across the side whose image is
//! widest (the side's width times its column of the Jacobian). A box at the resolution that is
//! neither dropped nor proved is [`SsiError::Unresolved`] where it lies: no fixed depth.
//!
//! Boxes are taken depth first, lower half first, so the work and the roots are the same
//! every run; the roots are returned sorted by their first unknown (an edge's parameter).
//! Each box costs one unit of the caller's budget, and the stop is polled after it.

use scx_algo::krawczyk::{Invert, ProvedRoot, contraction, k_operator, krawczyk_region, newton};
use scx_num::budget::{Budget, Stop};
use scx_num::interval::Ivl;
use scx_num::units::Length;
use scx_num::vector3d::Point3;

use crate::error::SsiError;

/// A system over a box: its values and Jacobian (by columns) enclosed, with the size and a
/// point of the region it maps to in space (for the resolution and for a refusal).
pub(crate) struct Enclosed<const N: usize> {
    pub f: [Ivl; N],
    pub columns: [[Ivl; N]; N],
    /// The longest side of the boxes holding the surface points the unknowns reach, in mm.
    pub size: f64,
    /// A point of that region.
    pub at: Point3,
}

/// A square system of `N` equations in `N` unknowns.
pub(crate) trait System<const N: usize> {
    /// The system over `x`, `None` where it cannot be enclosed.
    fn over(&self, x: &[Ivl; N]) -> Option<Enclosed<N>>;
}

/// The roots of `sys` in `start` (module docs).
///
/// # Errors
///
/// [`SsiError::Unresolved`] where a box at `floor` (mm) is undecided, or where the system
/// cannot be enclosed; [`SsiError::Budget`] and [`SsiError::Stopped`].
pub(crate) fn roots<S: System<N>, const N: usize>(
    sys: &S,
    start: [Ivl; N],
    floor: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Vec<ProvedRoot<N>>, SsiError>
where
    [[f64; N]; N]: Invert,
{
    let mut found: Vec<ProvedRoot<N>> = Vec::new();
    let mut stack = vec![start];
    while let Some(x) = stack.pop() {
        budget.charge(1)?;
        stop.check()?;
        if found.iter().any(|r| within(&x, &r.region)) {
            continue;
        }
        let Some(e) = sys.over(&x) else {
            return Err(refusal(sys, &x));
        };
        if e.f.iter().any(|f| f.lo > 0.0 || f.hi < 0.0) {
            continue;
        }
        let grown = grow(&x);
        match test(sys, &grown)? {
            Test::None => continue,
            Test::Unique(root) => {
                if meets(&root.root, &start) && !found.iter().any(|r| meets(&r.root, &root.root)) {
                    found.push(root);
                }
                if within(&x, &root.region) {
                    continue;
                }
            }
            Test::Open => {}
        }
        if e.size <= floor {
            return Err(SsiError::Unresolved {
                at: e.at,
                size: Length::from_mm(e.size),
            });
        }
        let Some([lo, hi]) = halve(&x, &e.columns) else {
            return Err(SsiError::Unresolved {
                at: e.at,
                size: Length::from_mm(e.size),
            });
        };
        stack.push(hi);
        stack.push(lo);
    }
    found.sort_by(|a, b| a.root[0].lo.total_cmp(&b.root[0].lo));
    Ok(found)
}

/// What Krawczyk's operator says of a box.
enum Test<const N: usize> {
    /// No root in it.
    None,
    /// Exactly one root in `region` (the box), held by `root`.
    Unique(ProvedRoot<N>),
    /// Neither.
    Open,
}

/// Krawczyk's test over `x` about its middle, with the inverse of the Jacobian there.
fn test<S: System<N>, const N: usize>(sys: &S, x: &[Ivl; N]) -> Result<Test<N>, SsiError>
where
    [[f64; N]; N]: Invert,
{
    let m = x.map(Ivl::mid);
    let (Some(at), Some(over)) = (sys.over(&m.map(Ivl::from)), sys.over(x)) else {
        return Ok(Test::Open);
    };
    let Some(c) = mid_columns(&at).inverse_of_columns() else {
        return Ok(Test::Open);
    };
    let (cm, _) = contraction(&c, &rows(&over.columns));
    let k = k_operator(&m, &c, &at.f, &cm, x);
    if k.iter().zip(x).any(|(a, b)| a.hi < b.lo || a.lo > b.hi) {
        return Ok(Test::None);
    }
    if !k.iter().zip(x).all(|(a, b)| a.lo > b.lo && a.hi < b.hi) {
        return Ok(Test::Open);
    }
    // Unique in `x`: refine, keeping the tight box only where it lies in `x`.
    let mut root = k;
    for (r, xi) in root.iter_mut().zip(x) {
        *r = meet(*r, *xi);
    }
    let coarse = ProvedRoot { root, region: *x };
    let point = |p: [f64; N]| -> Result<_, SsiError> {
        Ok(sys
            .over(&p.map(Ivl::from))
            .map(|e| (e.f.map(Ivl::mid), mid_columns(&e))))
    };
    let Some(r) = newton(m, point)? else {
        return Ok(Test::Unique(coarse));
    };
    let Some(cr) = sys
        .over(&r.map(Ivl::from))
        .and_then(|e| mid_columns(&e).inverse_of_columns())
    else {
        return Ok(Test::Unique(coarse));
    };
    let value = |y: [Ivl; N]| -> Result<_, SsiError> { Ok(sys.over(&y).map(|e| e.f)) };
    let jacobian = |y: [Ivl; N]| -> Result<_, SsiError> { Ok(sys.over(&y).map(|e| e.columns)) };
    Ok(Test::Unique(
        match krawczyk_region(r, cr, value, jacobian)? {
            Some(t) if within(&t.root, x) => ProvedRoot {
                root: t.root,
                region: *x,
            },
            _ => coarse,
        },
    ))
}

/// The point refusal for a box the system cannot be enclosed over.
fn refusal<S: System<N>, const N: usize>(sys: &S, x: &[Ivl; N]) -> SsiError {
    let m = x.map(Ivl::mid).map(Ivl::from);
    let at = sys.over(&m).map_or(Point3::new(0.0, 0.0, 0.0), |e| e.at);
    SsiError::Unresolved {
        at,
        size: Length::from_mm(f64::INFINITY),
    }
}

/// The Jacobian's columns at their middles.
fn mid_columns<const N: usize>(e: &Enclosed<N>) -> [[f64; N]; N] {
    e.columns.map(|c| c.map(Ivl::mid))
}

/// The Jacobian by rows from its columns.
fn rows<const N: usize>(columns: &[[Ivl; N]; N]) -> [[Ivl; N]; N] {
    let mut out = *columns;
    for (c, column) in columns.iter().enumerate() {
        for (row, v) in out.iter_mut().zip(column) {
            if let Some(slot) = row.get_mut(c) {
                *slot = *v;
            }
        }
    }
    out
}

/// `x` grown by an eighth of each side and a little more, so a root on a face is inside.
fn grow<const N: usize>(x: &[Ivl; N]) -> [Ivl; N] {
    x.map(|i| {
        let pad = 0.125 * (i.hi - i.lo) + 1e-15 * (1.0 + i.lo.abs().max(i.hi.abs()));
        (Ivl::from(i.lo) - Ivl::from(pad)).hull(Ivl::from(i.hi) + Ivl::from(pad))
    })
}

/// The two halves of `x` across the side whose image is widest: the side's width times the
/// largest magnitude in its column. `None` when that side cannot be split in `f64`.
fn halve<const N: usize>(x: &[Ivl; N], columns: &[[Ivl; N]; N]) -> Option<[[Ivl; N]; 2]> {
    // The first side of the largest reach.
    let mut best: Option<(usize, f64)> = None;
    for (i, (side, column)) in x.iter().zip(columns).enumerate() {
        let w = side.hi - side.lo;
        let s = column.iter().fold(0.0_f64, |m, c| m.max(c.mag()));
        // A column that is not bounded still gives the side's width as its measure.
        let reach = if s.is_finite() && s > 0.0 { w * s } else { w };
        if best.is_none_or(|(_, r)| reach > r) {
            best = Some((i, reach));
        }
    }
    let (best, _) = best?;
    let side = x.get(best)?;
    let (a, b) = (side.lo, side.hi);
    let m = 0.5 * (a + b);
    if !(m > a && m < b) {
        return None;
    }
    let mut lo = *x;
    let mut hi = *x;
    *lo.get_mut(best)? = Ivl::new(a, m);
    *hi.get_mut(best)? = Ivl::new(m, b);
    Some([lo, hi])
}

/// Whether `x` lies in `y`.
fn within<const N: usize>(x: &[Ivl; N], y: &[Ivl; N]) -> bool {
    x.iter().zip(y).all(|(a, b)| a.lo >= b.lo && a.hi <= b.hi)
}

/// Whether `x` and `y` meet.
pub(crate) fn meets<const N: usize>(x: &[Ivl; N], y: &[Ivl; N]) -> bool {
    x.iter().zip(y).all(|(a, b)| a.lo <= b.hi && b.lo <= a.hi)
}

/// The overlap of two intervals that meet (`a` where they do not).
fn meet(a: Ivl, b: Ivl) -> Ivl {
    let (lo, hi) = (a.lo.max(b.lo), a.hi.min(b.hi));
    if lo <= hi { Ivl::new(lo, hi) } else { a }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// `x^2 + y^2 = 1, x = y`: two roots, at `±(1/√2)(1, 1)`.
    struct Circle;

    impl System<2> for Circle {
        fn over(&self, x: &[Ivl; 2]) -> Option<Enclosed<2>> {
            let [a, b] = *x;
            let two = Ivl::from(2.0);
            Some(Enclosed {
                f: [a * a + b * b - Ivl::from(1.0), a - b],
                columns: [[two * a, Ivl::from(1.0)], [two * b, Ivl::from(-1.0)]],
                size: (a.hi - a.lo).max(b.hi - b.lo),
                at: Point3::new(a.mid(), b.mid(), 0.0),
            })
        }
    }

    /// `x^2 = 0`: a double root, which no box proves.
    struct Double;

    /// `x^2 = 0, y = 0`: a double root at the origin.
    impl System<2> for Double {
        fn over(&self, x: &[Ivl; 2]) -> Option<Enclosed<2>> {
            let [a, b] = *x;
            let (zero, one) = (Ivl::from(0.0), Ivl::from(1.0));
            Some(Enclosed {
                f: [a * a, b],
                columns: [[Ivl::from(2.0) * a, zero], [zero, one]],
                size: (a.hi - a.lo).max(b.hi - b.lo),
                at: Point3::new(a.mid(), 0.0, 0.0),
            })
        }
    }

    #[test]
    fn finds_every_simple_root_in_order() {
        let start = [Ivl::new(-2.0, 2.0), Ivl::new(-2.0, 2.0)];
        let mut budget = Budget::new(10_000);
        let rs = roots(&Circle, start, 1e-9, &mut budget, &Stop::new()).unwrap();
        assert_eq!(rs.len(), 2);
        let h = core::f64::consts::FRAC_1_SQRT_2;
        for (r, s) in rs.iter().zip([-h, h]) {
            for c in r.root {
                assert!(c.lo <= s && s <= c.hi && c.hi - c.lo < 1e-12, "{c:?}");
            }
        }
        // A root on the start box's face is found.
        let edge = [Ivl::new(-1.0, h), Ivl::new(0.0, 1.0)];
        let rs = roots(&Circle, edge, 1e-9, &mut Budget::new(10_000), &Stop::new()).unwrap();
        assert_eq!(rs.len(), 1);
    }

    #[test]
    fn a_double_root_is_unresolved_and_work_is_charged() {
        let r = roots(
            &Double,
            [Ivl::new(-1.0, 1.0); 2],
            1e-6,
            &mut Budget::new(10_000),
            &Stop::new(),
        );
        assert!(matches!(r, Err(SsiError::Unresolved { .. })), "{r:?}");
        let start = [Ivl::new(-2.0, 2.0), Ivl::new(-2.0, 2.0)];
        let r = roots(&Circle, start, 1e-9, &mut Budget::new(3), &Stop::new());
        assert!(matches!(r, Err(SsiError::Budget(_))), "{r:?}");
        let stop = Stop::new();
        stop.set();
        let r = roots(&Circle, start, 1e-9, &mut Budget::new(100), &stop);
        assert_eq!(r, Err(SsiError::Stopped));
    }
}
