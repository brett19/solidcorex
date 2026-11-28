// Written for solidcorex. Apache-2.0.
//! The curve certificates: a NURBS curve on a circle ([`on_circle`]) and a NURBS curve's
//! turn about an axis ([`turn_about_axis`]), solidcore's `on_circle` and `turn_about_axis`.

use scx_algo::bernstein::Bounded;
use scx_curve::Circle;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::units::{Accuracy, Length};
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::NurbsCurve;

use super::{EPS, Proof, curve_spans, defect, forms, max_ratio, point_at, restricted, sq, widen};
use crate::error::SurfaceError;
use crate::nurbs_invert::{halved, up};

/// The most pieces [`on_circle`] bounds in all (research 75 R6; past it: undecided).
const MAX_CIRCLE_PIECES: usize = 1 << 12;
/// The most halvings of one Bezier span in [`on_circle`].
const MAX_CIRCLE_HALVINGS: usize = 40;

/// Whether every point of the NURBS curve `c` lies within `tol` of `circle` (research 75
/// §6.5: a NURBS face's rim proved a circle before the face is rebuilt as its analytic
/// surface).
///
/// **How.** On each Bezier span, in homogeneous form `(X, Y, Z, W)` in the circle's frame,
/// `rho^2 - r^2 = F / W^2` with `F = X^2 + Y^2 - r^2 W^2` and the height off the circle's
/// plane is `Z / W`. Their coefficients' quotients bound them (positive weights): `|F / W^2|
/// <= f` and `|Z / W| <= z`, so the radial part `|rho - r| = |rho^2 - r^2| / (rho + r)` is at
/// most `f / (r + sqrt(max(r^2 - f, 0)))`, and the distance to the circle, `hypot(rho - r, z)`,
/// at most the hypot of the two. For a rational arc of the circle `F` and `Z` vanish
/// identically, so an exact arc is proved at once, its bound its rounding.
///
/// A span whose bound is above `tol` is read at its ends and middle: a point farther than
/// `tol` past its evaluation's rounding is a witness ([`Proof::Refuted`], its parameter on
/// `c`). Otherwise it is halved (de Casteljau at `1/2`), at most 40 times deep and `2^12`
/// pieces in all; past them, [`Proof::Undecided`]. Each piece is one unit of `budget` and
/// polls `stop`.
///
/// Solidcore bounded the distance from the circle's centre and from its plane apart (two
/// tests); this bounds the distance from the circle itself.
///
/// # Errors
///
/// [`SurfaceError::Exhausted`] when `budget` runs out, [`SurfaceError::Stopped`] when `stop`
/// is set, and [`SurfaceError::Defect`] where a Bernstein form cannot be built.
pub fn on_circle(
    c: &NurbsCurve<3>,
    circle: &Circle,
    tol: Accuracy,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Proof<f64>, SurfaceError> {
    let f = circle.frame();
    let r = circle.radius().mm();
    let tol = tol.length().mm();
    let axes = [f.x().as_vec(), f.y().as_vec(), f.z().as_vec()];
    let d = c.degree();
    let spans = curve_spans(c, f.origin(), axes);
    let mut reach = 0.0_f64;
    for (_, _, net) in &spans {
        for (h, _) in net {
            reach = reach.max((h[0].abs() + h[1].abs() + h[2].abs()) / h[3]);
        }
    }
    let mut work: Vec<_> = spans
        .into_iter()
        .rev()
        .map(|(a, b, net)| (a, b, net, 0_usize))
        .collect();
    let mut worst = 0.0_f64;
    // Over the pieces left unproved: the largest bound, `None` where one has none.
    let mut open: Option<Option<f64>> = None;
    let mut pieces = 0_usize;
    while let Some((a, b, net, depth)) = work.pop() {
        budget.charge(1).map_err(SurfaceError::Exhausted)?;
        stop.check().map_err(|_| SurfaceError::Stopped)?;
        pieces += 1;
        let h = forms(&net, (d, 0))?;
        let bound = circle_bound(&h, r)?;
        if let Some(bd) = bound
            && bd <= tol
        {
            worst = worst.max(bd);
            continue;
        }
        for s in [0.0, 0.5, 1.0] {
            if let Some((p, slack)) = point_at(&h, s, 0.0) {
                let rho = fmath::hypot(p.x, p.y);
                let dist = fmath::hypot(rho - r, p.z);
                let size = p.x.abs() + p.y.abs() + p.z.abs() + r;
                let by = dist - slack - 8.0 * EPS * size;
                if by > tol {
                    return Ok(Proof::Refuted {
                        at: a + s * (b - a),
                        by: Length::from_mm(by),
                    });
                }
            }
        }
        if depth >= MAX_CIRCLE_HALVINGS || pieces >= MAX_CIRCLE_PIECES {
            open = Some(match (open, bound) {
                (None, b) => b,
                (Some(Some(l)), Some(b)) => Some(l.max(b)),
                (Some(_), _) => None,
            });
            continue;
        }
        let (lower, upper) = halved(&net, (d, 0), true);
        let m = 0.5 * (a + b);
        work.push((m, b, upper, depth + 1));
        work.push((a, m, lower, depth + 1));
    }
    // The rounding of the frame's axes: `16 eps` of the frame's size.
    let total = |b: f64| Length::from_mm(up(b + 16.0 * EPS * (reach + r)));
    Ok(match open {
        Some(b) => Proof::Undecided {
            bound: b.map(|b| total(b.max(worst))),
        },
        None if total(worst).mm() <= tol => Proof::Proved {
            bound: total(worst),
        },
        None => Proof::Undecided {
            bound: Some(total(worst)),
        },
    })
}

/// The bound of the distance of the piece `h` (in the circle's frame) from the circle of
/// radius `r` about its origin, or `None` where its weight is not proved positive.
fn circle_bound(h: &[Bounded; 4], r: f64) -> Result<Option<f64>, SurfaceError> {
    let [x, y, z, w] = h;
    let w2 = sq(w)?;
    let r2 = r * r;
    let q = defect(sq(x)?.add(&sq(y)?))?;
    let f = widen(
        defect(q.add_scaled(&w2, -r2))?,
        2.0 * EPS * r2 * w2.f.max_abs_coefficient(),
    );
    let (Some(fm), Some(plane)) = (max_ratio(&f, &w2), max_ratio(z, w)) else {
        return Ok(None);
    };
    // The square root's argument rounded down, so a cancellation cannot raise the
    // denominator.
    let foot = (r2 * (1.0 - 4.0 * EPS) - fm).max(0.0).sqrt() * (1.0 - 2.0 * EPS);
    let radial = up(fm / (r + foot) * (1.0 + 4.0 * EPS));
    let b = up(fmath::hypot(radial, plane) * (1.0 + 4.0 * EPS));
    Ok(b.is_finite().then_some(b))
}

/// Which way a curve turns about an axis ([`turn_about_axis`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    /// Counter-clockwise seen from the axis's head, throughout: proved.
    Positive,
    /// Clockwise, throughout: proved.
    Negative,
    /// Neither is proved: the curve stands still about the axis somewhere, turns back, or
    /// its bound does not separate it from zero.
    Undecided,
}

/// Which way the NURBS curve `c` turns about the axis through `origin` along `axis`, over
/// the parameters `over`: the sign of `axis . (P - origin) x P'` throughout, proved
/// ([`Turn::Positive`] or [`Turn::Negative`]), or [`Turn::Undecided`] (solidcore's
/// `turn_about_axis`, research 75 §6.5).
///
/// On each Bezier span, in homogeneous form `H = (X, Y, Z)` about `origin` and `W`,
/// `(P - o) x P' = H x H' / W^2`, so the sign is that of the polynomial `axis . H x H'`, read
/// from its Bernstein coefficients with their running error bound. A span `over` covers in
/// part is restricted to it first. One pass over the spans, no halving: a curve that the
/// coefficients do not separate from zero is undecided.
///
/// # Errors
///
/// [`SurfaceError::Defect`] where a Bernstein form cannot be built.
pub fn turn_about_axis(
    c: &NurbsCurve<3>,
    origin: Point3,
    axis: UnitVec3,
    over: Extent1d,
) -> Result<Turn, SurfaceError> {
    let (t0, t1) = (over.min(), over.max());
    let unit = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let a = axis.as_vec();
    let d = c.degree();
    let mut sign = None;
    for (sa, sb, net) in curve_spans(c, origin, unit) {
        let (lo, hi) = (t0.max(sa), t1.min(sb));
        if !(lo < hi) {
            continue;
        }
        let range = ((lo - sa) / (sb - sa), (hi - sa) / (sb - sa));
        let [hx, hy, hz, _] = forms(&net, (d, 0))?.map(|f| restricted(&f, range, (0.0, 1.0)));
        let (dx, dy, dz) = (hx.deriv_u(), hy.deriv_u(), hz.deriv_u());
        // a component of H x H': p dq - q dp
        let comp = |p: &Bounded, dq: &Bounded, q: &Bounded, dp: &Bounded| {
            defect(defect(p.mul(dq))?.sub(&defect(q.mul(dp))?))
        };
        let cx = comp(&hy, &dz, &hz, &dy)?;
        let cy = comp(&hz, &dx, &hx, &dz)?;
        let cz = comp(&hx, &dy, &hy, &dx)?;
        let q = defect(defect(cx.scale(a.x).add(&cy.scale(a.y)))?.add(&cz.scale(a.z)))?;
        if !q.is_finite() {
            return Ok(Turn::Undecided);
        }
        let here = if q.f.min_coefficient() > q.err {
            Turn::Positive
        } else if q.f.max_coefficient() < -q.err {
            Turn::Negative
        } else {
            return Ok(Turn::Undecided);
        };
        match sign {
            Some(s) if s != here => return Ok(Turn::Undecided),
            _ => sign = Some(here),
        }
    }
    Ok(sign.unwrap_or(Turn::Undecided))
}
