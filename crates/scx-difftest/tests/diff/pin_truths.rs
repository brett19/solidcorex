// Written for solidcorex. Apache-2.0.
//! Pin defects judged by an oracle ([`TruthDefect`]), one per test: results whose true value
//! is a short formula over the inputs, recomputed here without scx.
//!
//! Nearly all are NaN operands. scx takes extremes with `f64::max` and `f64::min`, which
//! skip a NaN operand; the pin's `a > b ? a : b` answers NaN or not by the operands' order.
//! The truth here skips NaN explicitly ([`most`], [`least`]) and is NaN only when every
//! operand is. A NaN distance or factor moves no bound (scx's `d > 0` guards; the pin moves
//! by NaN). The rest: the pin's empty hull starts at `[1e20, -1e20]`, so a value of that
//! size or more is lost ([`hull`]); a negative scale leaves the pin's interval inverted,
//! where the true image is ordered; and a line crossing a large box at one point, whose
//! tolerance the pin's last check loses to rounding.
//!
//! Each truth applies where scx and the pin disagree (`Applies::Disagreeing`), and is a
//! total oracle: it gives the true value on every input of its test.

use crate::harness::{want, Applies, Out, TruthDefect};
use scx_num::math::{BIG_DOUBLE, EFF_ZERO, INFINITE_PARAMETER, INFINITE_PARAMETER_SQRT};

type I = [f64; 2];
type R = [f64; 4];
type B = [f64; 6];
type V = [f64; 3];
type P = [f64; 2];
type LineArgs = (R, P, P, Option<f64>, Option<f64>);

const NAN_OPERAND: &str = "the pin's a > b ? a : b answers by operand order on a NaN; scx skips it";
const NAN_MOVE: &str = "the pin moves bounds by a NaN distance or keeps a negative scale's \
                        inverted image; scx moves none and orders the image";
const NAN_PERIOD: &str = "the pin's scaled zero of a NaN period is NaN, so no tolerance \
                          holds; scx's is EFF_ZERO, the NaN skipped";
const HULL_START: &str = "the pin's empty hull [1e20, -1e20] loses values of that size; \
                          scx's hull skips NaN";

fn on_disagreement<A>(reason: &'static str, truth: fn(&A) -> Out) -> TruthDefect<A> {
    TruthDefect {
        reason,
        applies: Applies::Disagreeing,
        truth,
    }
}

/// The largest operand that is not NaN; NaN when all are.
fn most(v: &[f64]) -> f64 {
    v.iter()
        .copied()
        .filter(|x| !x.is_nan())
        .fold(f64::NAN, |m, x| if m.is_nan() || x > m { x } else { m })
}

/// The smallest operand that is not NaN; NaN when all are.
fn least(v: &[f64]) -> f64 {
    v.iter()
        .copied()
        .filter(|x| !x.is_nan())
        .fold(f64::NAN, |m, x| if m.is_nan() || x < m { x } else { m })
}

/// The smallest interval holding the values that are not NaN; `None` when none is.
fn hull(v: &[f64]) -> Option<I> {
    let (lo, hi) = (least(v), most(v));
    (!lo.is_nan()).then_some([lo, hi])
}

/// The interval `[a0, a1]` as scx holds it: `None` when inverted; NaN bounds are kept.
fn ivl(a: I) -> Option<I> {
    (a[0].partial_cmp(&a[1]) != Some(std::cmp::Ordering::Greater)).then_some(a)
}

/// The box with corners `a[..N]` and `a[N..]`, as one interval per axis.
fn axes<const N: usize, const M: usize>(a: [f64; M]) -> Option<[I; N]> {
    let mut e = [[0.0; 2]; N];
    for (i, x) in e.iter_mut().enumerate() {
        *x = ivl([a[i], a[i + N]])?;
    }
    Some(e)
}

/// An interval fingerprinted as an `Extent1d`: its min, then its max.
fn e1(e: I) -> (f64, f64) {
    (e[0], e[1])
}

/// A box fingerprinted as an `Extent2d`: its min corner, then its max corner.
fn e2(e: [I; 2]) -> ((f64, f64), (f64, f64)) {
    ((e[0][0], e[1][0]), (e[0][1], e[1][1]))
}

/// A box fingerprinted as an `Extent3d`.
fn e3(e: [I; 3]) -> ((f64, f64, f64), (f64, f64, f64)) {
    ((e[0][0], e[1][0], e[2][0]), (e[0][1], e[1][1], e[2][1]))
}

fn infinite_param(a: f64) -> bool {
    a.abs() == INFINITE_PARAMETER || a.abs() == INFINITE_PARAMETER_SQRT
}

/// Each bound that is not an infinite parameter moved out by `d` (in by `-d`).
fn moved_out(e: I, d: f64) -> I {
    let keep = |x: f64, m: f64| if infinite_param(x) { x } else { m };
    [keep(e[0], e[0] - d), keep(e[1], e[1] + d)]
}

/// The interval grown by `d`; a `d` that is not above zero moves nothing.
fn expanded(e: I, d: f64) -> I {
    if d > 0.0 {
        moved_out(e, d)
    } else {
        e
    }
}

/// The interval grown by `f` of its length; an `f` that is not above zero moves nothing.
fn expanded_relative(e: I, f: f64) -> I {
    if f > 0.0 {
        moved_out(e, (e[1] - e[0]) * f)
    } else {
        e
    }
}

/// The image of the interval under `x -> s x`, ordered; infinite parameters stay.
fn scaled(e: I, s: f64) -> I {
    let t = |x: f64| if infinite_param(x) { x } else { x * s };
    let (a, b) = (t(e[0]), t(e[1]));
    if a > b {
        [b, a]
    } else {
        [a, b]
    }
}

fn union(e: I, f: I) -> I {
    [least(&[e[0], f[0]]), most(&[e[1], f[1]])]
}

/// The overlap; `None` when the intervals are apart (touching is not apart).
fn intersect(e: I, f: I) -> Option<I> {
    (!(e[0] > f[1] || e[1] < f[0])).then(|| [most(&[e[0], f[0]]), least(&[e[1], f[1]])])
}

fn union_n<const N: usize>(e: [I; N], f: [I; N]) -> [I; N] {
    core::array::from_fn(|i| union(e[i], f[i]))
}

/// The overlap of boxes: `None` when apart on any axis.
fn intersect_n<const N: usize>(e: [I; N], f: [I; N]) -> Option<[I; N]> {
    let mut r = [[0.0; 2]; N];
    for (i, x) in r.iter_mut().enumerate() {
        *x = intersect(e[i], f[i])?;
    }
    Some(r)
}

/// The hull of an interval and a value, bound by bound: the least of its min and the value,
/// and the most of its max and the value, a NaN skipped; `None` when a bound is NaN either way.
fn hull_value(e: I, v: f64) -> Option<I> {
    let (lo, hi) = (least(&[e[0], v]), most(&[e[1], v]));
    (!lo.is_nan() && !hi.is_nan()).then_some([lo, hi])
}

/// The hull of a box and a point, axis by axis ([`hull_value`]).
fn hull_point<const N: usize>(e: [I; N], p: [f64; N]) -> Option<[I; N]> {
    let mut r = [[0.0; 2]; N];
    for (i, x) in r.iter_mut().enumerate() {
        *x = hull_value(e[i], p[i])?;
    }
    Some(r)
}

/// The hull of two points, axis by axis.
fn hull_two<const N: usize>(a: [f64; N], b: [f64; N]) -> Option<[I; N]> {
    let mut r = [[0.0; 2]; N];
    for (i, x) in r.iter_mut().enumerate() {
        *x = hull(&[a[i], b[i]])?;
    }
    Some(r)
}

/// The largest absolute bound.
fn max_dimension(e: &[I]) -> f64 {
    let v: Vec<f64> = e.iter().flatten().map(|x| x.abs()).collect();
    most(&v)
}

// --- math: the extremes of two to four numbers.
pub fn max2() -> TruthDefect<(f64, f64)> {
    on_disagreement(NAN_OPERAND, |&(a, b)| want(most(&[a, b])))
}
pub fn min2() -> TruthDefect<(f64, f64)> {
    on_disagreement(NAN_OPERAND, |&(a, b)| want(least(&[a, b])))
}
pub fn max3() -> TruthDefect<(f64, f64, f64)> {
    on_disagreement(NAN_OPERAND, |&(a, b, c)| want(most(&[a, b, c])))
}
pub fn min3() -> TruthDefect<(f64, f64, f64)> {
    on_disagreement(NAN_OPERAND, |&(a, b, c)| want(least(&[a, b, c])))
}
pub fn max4() -> TruthDefect<(f64, f64, f64, f64)> {
    on_disagreement(NAN_OPERAND, |&(a, b, c, d)| want(most(&[a, b, c, d])))
}
pub fn min4() -> TruthDefect<(f64, f64, f64, f64)> {
    on_disagreement(NAN_OPERAND, |&(a, b, c, d)| want(least(&[a, b, c, d])))
}

// --- tol: the scaled zero `EFF_ZERO (1 + m)`, `m` the largest magnitude of the operands
// (the absolute value of a number, the largest absolute coordinate of a vector or box). A
// NaN coordinate gives no magnitude, so with no other the scaled zero is `EFF_ZERO`.

/// The scaled zero of the numbers `v`, with its square and square root.
fn scaled_zero(v: &[f64]) -> (f64, f64, f64) {
    let m: Vec<f64> = v.iter().map(|x| x.abs()).collect();
    let z = EFF_ZERO * (1.0 + most(&m).max(0.0));
    (z, z * z, z.sqrt())
}

pub fn scaled_zero_of_each() -> TruthDefect<(f64, P, V, I)> {
    on_disagreement(NAN_OPERAND, |&(a, b, c, d)| {
        want(ivl(d).map(|d| {
            let z = |v: &[f64]| scaled_zero(v);
            (
                [z(&[a]).0, z(&b).0, z(&c).0, z(&d).0],
                [z(&[a]).1, z(&c).2],
                [z(&b).1, z(&d).2],
            )
        }))
    })
}

pub fn scaled_zero_of_boxes() -> TruthDefect<(B, R)> {
    on_disagreement(NAN_OPERAND, |&(e, f)| {
        want(axes::<3, 6>(e).zip(axes::<2, 4>(f)).map(|_| {
            let (ze, zf) = (scaled_zero(&e), scaled_zero(&f));
            [ze.0, zf.0, ze.1, zf.2]
        }))
    })
}

pub fn scaled_zero_n() -> TruthDefect<(f64, V, I, B)> {
    on_disagreement(NAN_OPERAND, |&(a, b, c, d)| {
        want(ivl(c).zip(axes::<3, 6>(d)).map(|_| {
            let z = |parts: &[&[f64]]| scaled_zero(&parts.concat());
            let (a, b, c, d) = (&[a][..], &b[..], &c[..], &d[..]);
            (
                [z(&[a, b]).0, z(&[b, c]).1, z(&[c, d]).2],
                [z(&[a, b, c]).0, z(&[b, c, d]).1, z(&[a, c, d]).2],
                [z(&[a, b, c, d]).0, z(&[a, b, c, d]).1, z(&[a, b, c, d]).2],
            )
        }))
    })
}

// --- Extent1d
pub fn e1_hull() -> TruthDefect<(f64, f64)> {
    on_disagreement(HULL_START, |&(a, b)| want(hull(&[a, b]).map(e1)))
}
pub fn e1_max_dimension() -> TruthDefect<(I,)> {
    on_disagreement(NAN_OPERAND, |&(a,)| {
        want(ivl(a).map(|e| max_dimension(&[e])))
    })
}
pub fn e1_expanded() -> TruthDefect<(I, f64)> {
    on_disagreement(NAN_MOVE, |&(a, v)| {
        want(ivl(a).map(|e| (e1(expanded(e, v.abs())), e1(expanded_relative(e, v.abs())))))
    })
}
pub fn e1_hull_value() -> TruthDefect<(I, f64)> {
    on_disagreement(HULL_START, |&(a, v)| {
        want(ivl(a).map(|e| {
            let h = hull_value(e, v).map(e1);
            (h, h)
        }))
    })
}
pub fn e1_union_intersect() -> TruthDefect<(I, I)> {
    on_disagreement(NAN_OPERAND, |&(a, b)| {
        want(
            ivl(a)
                .zip(ivl(b))
                .map(|(e, f)| (e1(union(e, f)), intersect(e, f).map(e1))),
        )
    })
}
/// Whether `v` is on a boundary of the interval, periodic of length `p`, to within `t`, and
/// its distance to the nearer end: the smaller of its distances to the two ends (the pin's
/// own, compared in `e1_periodic_value`), a NaN one skipped.
pub fn e1_periodic_boundary() -> TruthDefect<(I, f64, f64, f64)> {
    on_disagreement(NAN_OPERAND, |&(a, v, p, t)| {
        #[allow(unused_imports)]
        use crate::facade::s::ext::*;
        want(crate::facade::s::e1(a).map(|e| {
            let c = e.contains_periodic_value(v, p, t);
            let on = c.dist_to_min < t || c.dist_to_max < t;
            (on, least(&[c.dist_to_min, c.dist_to_max]))
        }))
    })
}

// --- Extent2d
pub fn e2_hull() -> TruthDefect<(P, P)> {
    on_disagreement(HULL_START, |&(a, b)| want(hull_two(a, b).map(e2)))
}
pub fn e2_max_dimension() -> TruthDefect<(R,)> {
    on_disagreement(NAN_OPERAND, |&(a,)| {
        want(axes::<2, 4>(a).map(|e| max_dimension(&e)))
    })
}
pub fn e2_hull_point() -> TruthDefect<(R, P)> {
    on_disagreement(HULL_START, |&(a, p)| {
        want(axes::<2, 4>(a).map(|e| {
            let h = hull_point(e, p).map(e2);
            (h, h)
        }))
    })
}
pub fn e2_expand_scale() -> TruthDefect<(R, f64, Option<f64>)> {
    on_disagreement(NAN_MOVE, |&(a, d, w)| {
        want(axes::<2, 4>(a).map(|e| {
            let (sx, sy) = (d, w.unwrap_or(d));
            (
                e2(e.map(|x| expanded(x, d.abs()))),
                e2(e.map(|x| expanded_relative(x, d.abs()))),
                e2([scaled(e[0], sx), scaled(e[1], sy)]),
            )
        }))
    })
}
pub fn e2_expanded_vec() -> TruthDefect<(R, P)> {
    on_disagreement(NAN_MOVE, |&(a, d)| {
        want(axes::<2, 4>(a).map(|e| e2([expanded(e[0], d[0].abs()), expanded(e[1], d[1].abs())])))
    })
}
pub fn e2_union_intersect() -> TruthDefect<(R, R)> {
    on_disagreement(NAN_OPERAND, |&(a, b)| {
        want(
            axes::<2, 4>(a)
                .zip(axes::<2, 4>(b))
                .map(|(e, f)| (e2(union_n(e, f)), intersect_n(e, f).map(e2))),
        )
    })
}
/// The parameters of the line `p + s v` inside the box grown by `tu` across x and `tv`
/// across y (defaults `100 EFF_ZERO` and `tu`): where it crosses each side's line (when
/// not parallel to it to `EFF_ZERO`) within the grown other axis, and then all between.
/// `None` when it crosses none, or the crossing is not longer than `-max(tu, tv)` (only a
/// negative tolerance refuses a crossing).
pub fn e2_intersect_with_infinite_line() -> TruthDefect<LineArgs> {
    on_disagreement(
        "the pin's empty hull [1e20, -1e20] loses crossings of that size, and its \
         max > min - tol check loses tol to rounding at a single crossing of a large box",
        |&(a, p, v, tu, tv)| {
            want(axes::<2, 4>(a).map(|e| {
                let tu = tu.unwrap_or(EFF_ZERO * 100.0);
                let tol = [tu, tv.unwrap_or(tu)];
                let mut prm = Vec::new();
                for (i, j) in [(0, 1), (1, 0)] {
                    if v[i].abs() > EFF_ZERO {
                        for bound in e[i] {
                            let s = (bound - p[i]) / v[i];
                            let at = p[j] + s * v[j];
                            if at >= e[j][0] - tol[j] && at <= e[j][1] + tol[j] {
                                prm.push(s);
                            }
                        }
                    }
                }
                hull(&prm)
                    .filter(|h| h[1] - h[0] > -tol[0].max(tol[1]))
                    .map(e1)
            }))
        },
    )
}

// --- Extent3d
pub fn e3_hull() -> TruthDefect<(V, V)> {
    on_disagreement(HULL_START, |&(a, b)| want(hull_two(a, b).map(e3)))
}
/// The largest side length.
pub fn e3_max_length() -> TruthDefect<(B,)> {
    on_disagreement(NAN_OPERAND, |&(a,)| {
        want(axes::<3, 6>(a).map(|e| most(&e.map(|x| x[1] - x[0]))))
    })
}
/// The bounding sphere's smallest half side: from the centre (each axis at `min + 0.5
/// (max - min)`) to the max corner.
pub fn e3_sphere_min_radius() -> TruthDefect<(B,)> {
    on_disagreement(NAN_OPERAND, |&(a,)| {
        want(axes::<3, 6>(a).map(|e| least(&e.map(|x| (x[0] + 0.5 * (x[1] - x[0]) - x[1]).abs()))))
    })
}
pub fn e3_hull_point() -> TruthDefect<(B, V)> {
    on_disagreement(HULL_START, |&(a, p)| {
        want(axes::<3, 6>(a).map(|e| {
            let h = hull_point(e, p).map(e3);
            (h, h)
        }))
    })
}
/// The union, the overlap, and the largest distance between points of the two boxes: the
/// union's diagonal (and its square).
pub fn e3_union_intersect_far() -> TruthDefect<(B, B)> {
    on_disagreement(NAN_OPERAND, |&(a, b)| {
        want(axes::<3, 6>(a).zip(axes::<3, 6>(b)).map(|(e, f)| {
            let u = union_n(e, f);
            let sq: f64 = u.iter().map(|x| (x[1] - x[0]) * (x[1] - x[0])).sum();
            (e3(u), intersect_n(e, f).map(e3), sq, sq.sqrt())
        }))
    })
}
pub fn e3_expand() -> TruthDefect<(B, f64)> {
    on_disagreement(NAN_MOVE, |&(a, d)| {
        want(axes::<3, 6>(a).map(|e| {
            (
                e3(e.map(|x| expanded_relative(x, d.abs()))),
                e3(e.map(|x| expanded(x, d.abs()))),
            )
        }))
    })
}

// --- PeriodicExtent1d with a NaN period. Its scaled zero is `EFF_ZERO` ([`scaled_zero`]:
// the NaN gives no magnitude), the same as period 0's. `contains_value`, `is_contained_by`
// and `are_disjoint` use the period only through that scaled zero, so their truth is the
// pin's own answer with 0 for the NaN period. `normalize_value` and `set_min_max` also use
// the period itself; their truth is their definition, written out here.

/// The extent `a` (min, max, period) with 0 for a NaN period.
fn zero_nan_period(a: V) -> V {
    [a[0], a[1], if a[2].is_nan() { 0.0 } else { a[2] }]
}

/// `value` mapped into `[0, period]` by at most one period, after snapping values within
/// the period's scaled zero of `0` or `period` onto them; `0` for the empty extent.
fn normalize(a: V, v: f64) -> f64 {
    let (p, sz) = (a[2], scaled_zero(&[a[2]]).0);
    let v = if v.abs() < sz {
        0.0
    } else if (v - p).abs() < sz {
        p
    } else {
        v
    };
    if a[0] == BIG_DOUBLE && a[1] == -BIG_DOUBLE {
        0.0
    } else if v < 0.0 {
        v + p
    } else if v > p {
        v - p
    } else {
        v
    }
}

pub fn pe_values() -> TruthDefect<(V, bool, f64)> {
    on_disagreement(NAN_PERIOD, |&(a, f, v)| {
        #[allow(unused_imports)]
        use crate::facade::s::ext::*;
        let n = normalize(a, v);
        want((
            n,
            n,
            crate::facade::s::pe(zero_nan_period(a), f).contains_value(v),
        ))
    })
}

/// `[x, y]` with period `p` (the extent's own when `None`): the full period when the ends
/// agree to its scaled zero and not `sp`, else `[x, y]`, full when its length is the
/// period's to the scaled zero; refused (but set) when an end is outside `[0, period]`.
pub fn pe_set_min_max() -> TruthDefect<(V, bool, f64, f64, Option<f64>, bool)> {
    on_disagreement(NAN_PERIOD, |&(a, _, x, y, p, sp)| {
        use scx_num::periodic_extent1d::PeriodicExtent1d;
        let p = p.unwrap_or(a[2]);
        let sz = scaled_zero(&[p]).0;
        let e = if (y - x).abs() < sz {
            if sp {
                PeriodicExtent1d::from_parts(x, y, p, false)
            } else {
                PeriodicExtent1d::from_parts(0.0, p, p, true)
            }
        } else {
            PeriodicExtent1d::from_parts(x, y, p, p - (y - x).abs() < sz)
        };
        let r = if x < 0.0 || x > p || y < 0.0 || y > p {
            Err(scx_num::NumError::InvalidInput {
                operation: "geom::periodic_extent1d::PeriodicExtent1d::set_min_max",
                reason: scx_num::error::Invalid::OutsidePeriod,
            })
        } else {
            Ok(())
        };
        want((r, e))
    })
}

pub fn pe_pair() -> TruthDefect<(V, bool, V, bool)> {
    on_disagreement(NAN_PERIOD, |&(a, f, b, g)| {
        #[allow(unused_imports)]
        use crate::facade::s::ext::*;
        let (e, h) = (
            crate::facade::s::pe(zero_nan_period(a), f),
            crate::facade::s::pe(zero_nan_period(b), g),
        );
        want((e.is_contained_by(&h), e.are_disjoint(&h)))
    })
}
