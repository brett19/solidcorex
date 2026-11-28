// Written for solidcorex. Apache-2.0.
//! Certificates: proofs that a NURBS curve or surface is within a bound of an exact form
//! (research 75 §4, MK10's certificate module, ported from solidcore's
//! `nurbs/certificate.rs`). Every bound is a Bernstein convex-hull bound on homogeneous
//! forms with a running error bound ([`scx_algo::bernstein::Bounded`]); samples only refute.
//!
//! - [`on_analytic`]: a NURBS surface and a plane, cylinder, cone, sphere or torus
//!   ([`Analytic`]). SU7's recognition certificate, shared: [`crate::recognize`] proves its
//!   candidates with it.
//! - [`on_circle`]: a NURBS curve and a [`Circle`].
//! - [`turn_about_axis`]: whether a NURBS curve turns one way about an axis throughout.
//! - [`span_bound`]: the distance between a surface and a rail between two nodes, the loft's
//!   certificate (research 75 §4.3).
//!
//! The verdicts are typed ([`Proof`]): proved with its bound, refuted with a witness and a
//! proved lower bound of its distance, or undecided with the bound reached. Each piece a
//! certificate bounds is one unit of the budget and polls the stop; each has a fixed limit of
//! pieces and halvings (research 75 R6), past which it is undecided, never a refusal.

// `!(x <= bound)` is the test that refutes a NaN too.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

mod circle;
mod kind;
mod span;
#[cfg(test)]
mod tests;

use scx_algo::bernstein::{Bernstein2, Bounded};
use scx_num::certified::gamma;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::NurbsCurve;

pub use circle::{Turn, on_circle, turn_about_axis};
pub(crate) use kind::{Local, Placed, prove};
pub use kind::{Analytic, on_analytic};
pub use span::{Node, SpanBound, span_bound};

use crate::error::SurfaceError;
use crate::nurbs_invert::{Hb, spans, up};

const EPS: f64 = f64::EPSILON;
const DEFECT_BERNSTEIN: &str = "a certificate's Bernstein form could not be built";

/// What a certificate proves of a form and its exact counterpart, with `P` the parameter of
/// a witness (`f64` on a curve, `Point2` on a surface).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Proof<P> {
    /// Every point of the form is within `bound` of the exact one, and `bound` is within the
    /// tolerance.
    Proved {
        /// The proved bound of the distance.
        bound: Length,
    },
    /// The form's point at `at` is farther than the tolerance from the exact one: `by` is a
    /// proved lower bound of its distance, above the tolerance.
    Refuted {
        /// The witness's parameter.
        at: P,
        /// A proved lower bound of the witness's distance.
        by: Length,
    },
    /// Neither, within the certificate's fixed limits: the form comes to the tolerance's
    /// edge, or its bound does not tighten. `bound` is the bound reached where every piece
    /// has one (above the tolerance), `None` where a piece has none.
    Undecided {
        /// The bound reached, if one holds over the whole form.
        bound: Option<Length>,
    },
}

/// `net`'s coordinate `k` as a Bernstein form of degrees `(du, dv)`, its error the largest of
/// the coefficients'.
fn form(net: &[Hb], (du, dv): (usize, usize), k: usize) -> Result<Bounded, SurfaceError> {
    let c = net
        .iter()
        .map(|(h, _)| h.get(k).copied().unwrap_or(f64::NAN))
        .collect();
    let err = net.iter().fold(0.0, |m: f64, (_, e)| {
        m.max(e.get(k).copied().unwrap_or(f64::NAN))
    });
    defect(Bernstein2::new(du, dv, c)).map(|f| Bounded { f, err })
}

/// The four forms `(X, Y, Z, w)` of `net`.
fn forms(net: &[Hb], degrees: (usize, usize)) -> Result<[Bounded; 4], SurfaceError> {
    Ok([
        form(net, degrees, 0)?,
        form(net, degrees, 1)?,
        form(net, degrees, 2)?,
        form(net, degrees, 3)?,
    ])
}

/// The net in the frame `(origin, axes)`: `(X, Y, Z, w)` with `X = (w P - w O) . x` and so
/// on, each with its error bound (the inputs' carried, and the roundings of the shift and
/// the dot product).
fn local_net(net: &[Hb], origin: Point3, axes: [Vec3; 3]) -> Vec<Hb> {
    let o = [origin.x, origin.y, origin.z];
    net.iter()
        .map(|(h, e)| {
            let (w, ew) = (h[3], e[3]);
            let mut d = [0.0; 3];
            let mut ed = [0.0; 3];
            for k in 0..3 {
                let (hk, ok) = (
                    h.get(k).copied().unwrap_or(f64::NAN),
                    o.get(k).copied().unwrap_or(f64::NAN),
                );
                let ek = e.get(k).copied().unwrap_or(f64::NAN);
                if let (Some(dk), Some(edk)) = (d.get_mut(k), ed.get_mut(k)) {
                    *dk = hk - ok * w;
                    *edk = up(ek + ok.abs() * ew + 2.0 * EPS * (hk.abs() + (ok * w).abs()));
                }
            }
            let mut out = ([0.0, 0.0, 0.0, w], [0.0, 0.0, 0.0, ew]);
            for (j, a) in axes.iter().enumerate() {
                let av = [a.x, a.y, a.z];
                let mut v = 0.0;
                let mut size = 0.0;
                let mut err = 0.0;
                for ((dk, edk), ak) in d.iter().zip(&ed).zip(&av) {
                    v += dk * ak;
                    size += (dk * ak).abs();
                    err += ak.abs() * edk;
                }
                if let (Some(slot), Some(se)) = (out.0.get_mut(j), out.1.get_mut(j)) {
                    *slot = v;
                    *se = up(err + 4.0 * EPS * size);
                }
            }
            out
        })
        .collect()
}

/// The curve's Bezier spans: each its parameter range and its homogeneous control points
/// (with their errors) in the frame `(origin, axes)`.
fn curve_spans(c: &NurbsCurve<3>, origin: Point3, axes: [Vec3; 3]) -> Vec<(f64, f64, Vec<Hb>)> {
    // The weighted points are the curve's own (exact) data: no error yet.
    let hom: Vec<Hb> = match (c.weighted_points(), c.weights()) {
        (Some(wp), Some(w)) => wp
            .iter()
            .zip(w)
            .map(|(a, &w)| ([a[0], a[1], a[2], w], [0.0; 4]))
            .collect(),
        _ => c
            .points()
            .iter()
            .map(|a| ([a[0], a[1], a[2], 1.0], [0.0; 4]))
            .collect(),
    };
    spans(c.knots(), c.degree(), &hom)
        .into_iter()
        .map(|(a, b, net)| (a, b, local_net(&net, origin, axes)))
        .collect()
}

/// `max_k (|N_k| + err_N) / (D_k - err_D)`, rounded up: a bound of `|N / D|` for forms of
/// one degree whose `D` coefficients are all positive; `None` where one is not.
fn max_ratio(num: &Bounded, den: &Bounded) -> Option<f64> {
    if !(num.is_finite() && den.is_finite()) {
        return None;
    }
    let mut m = 0.0_f64;
    for (&nk, &dk) in num.f.coefficients().iter().zip(den.f.coefficients()) {
        let lo = (dk - den.err) * (1.0 - 2.0 * EPS);
        if !(lo > 0.0) {
            return None;
        }
        m = m.max(up(nk.abs() + num.err) / lo);
    }
    m.is_finite().then(|| up(m))
}

/// `f` restricted to `[u0, u1] x [v0, v1]` of its square, with the error the splits add: at
/// most two de Casteljau splits each way, each within `gamma_(du + dv + 2)` of the largest
/// coefficient (solidcore's `restricted`).
fn restricted(f: &Bounded, (u0, u1): (f64, f64), (v0, v1): (f64, f64)) -> Bounded {
    if u0 <= 0.0 && u1 >= 1.0 && v0 <= 0.0 && v1 >= 1.0 {
        return f.clone();
    }
    let (du, dv) = f.f.degrees();
    let add = 2.0 * gamma(2 * (du + dv) + 4) * (f.f.max_abs_coefficient() + f.err);
    Bounded {
        f: f.f.restrict(u0, u1, v0, v1),
        err: up(f.err + add),
    }
}

fn sq(f: &Bounded) -> Result<Bounded, SurfaceError> {
    defect(f.mul(f))
}

fn defect<T, E>(r: Result<T, E>) -> Result<T, SurfaceError> {
    r.map_err(|_| SurfaceError::Defect {
        reason: DEFECT_BERNSTEIN,
    })
}

/// `f` with its error raised by `extra`.
fn widen(mut f: Bounded, extra: f64) -> Bounded {
    f.err = up(f.err + extra);
    f
}

/// The point of the homogeneous forms `h` at `(u, v)` and a bound of its distance from the
/// exact point there (the forms' errors, de Casteljau's roundings and the division's), or
/// `None` where the weight there is not proved positive.
fn point_at(h: &[Bounded; 4], u: f64, v: f64) -> Option<(Vec3, f64)> {
    let [x, y, z, w] = h;
    let (du, dv) = w.f.degrees();
    let largest = h.iter().fold(0.0_f64, |m, f| m.max(f.f.max_abs_coefficient()));
    let worst = h.iter().fold(0.0_f64, |m, f| m.max(f.err));
    // de Casteljau rounds once per level each way: within gamma_(2(du + dv)) of the
    // largest coefficient.
    let eh = up(worst + gamma(2 * (du + dv) + 4) * (largest + worst));
    let wv = w.f.eval(u, v);
    if !(wv - eh > 0.0) {
        return None;
    }
    let p = Vec3::new(x.f.eval(u, v), y.f.eval(u, v), z.f.eval(u, v)) / wv;
    // Each coordinate moves by at most `(eh + |p_k| eh) / (w - eh)`, doubled for the
    // division's own rounding; their sum bounds the point's move.
    let sum = p.x.abs() + p.y.abs() + p.z.abs();
    let slack = up(2.0 * (3.0 * eh + sum * eh) / (wv - eh) + 8.0 * EPS * sum);
    (p.x.is_finite() && p.y.is_finite() && p.z.is_finite() && slack.is_finite())
        .then_some((p, slack))
}
