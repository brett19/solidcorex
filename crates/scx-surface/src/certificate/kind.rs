// Written for solidcorex. Apache-2.0.
//! The surface certificate: a NURBS surface within a bound of a plane, cylinder, cone, sphere
//! or torus (SU7's recognition certificate, moved here and shared; [`crate::recognize`]'s
//! module docs give the bounds).

use scx_num::budget::{Budget, Stop};
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs_surface::NurbsSurface;

use scx_algo::bernstein::Bounded;

use super::{EPS, Proof, defect, forms, local_net, max_ratio, sq, widen};
use crate::cone::Cone;
use crate::cylinder::Cylinder;
use crate::error::SurfaceError;
use crate::nurbs_invert::{Hb, halved, patches, up};
use crate::plane::Plane;
use crate::sphere::Sphere;
use crate::surface::Surface;
use crate::torus::Torus;

/// The most pieces [`on_analytic`] bounds in all (research 75 R6: a fixed amount; past it
/// the surface is undecided).
const MAX_PIECES: usize = 1 << 16;
/// A piece with a corner within this share of the accuracy is not halved further: the bound
/// tends to the piece's largest distance as it is halved, but too slowly there to end.
const NEAR: f64 = 1.0 / 64.0;

/// An exact analytic surface a certificate compares a NURBS surface with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Analytic {
    /// A plane.
    Plane(Plane),
    /// A cylinder.
    Cylinder(Cylinder),
    /// A cone (both nappes; a proof is of the nearer).
    Cone(Cone),
    /// A sphere.
    Sphere(Sphere),
    /// A torus.
    Torus(Torus),
}

impl Analytic {
    /// The analytic kind `s` is, or `None` for a NURBS surface.
    #[must_use]
    pub fn of(s: &Surface) -> Option<Self> {
        Some(match s {
            Surface::Plane(k) => Self::Plane(*k),
            Surface::Cylinder(k) => Self::Cylinder(*k),
            Surface::Cone(k) => Self::Cone(*k),
            Surface::Sphere(k) => Self::Sphere(*k),
            Surface::Torus(k) => Self::Torus(*k),
            Surface::Nurbs(_) => return None,
        })
    }

    fn placed(&self) -> Placed {
        let axes = |a: crate::common::Axes| (a.o, [a.x, a.y, a.z]);
        let (local, (origin, axes)) = match self {
            Self::Plane(k) => {
                let f = k.frame();
                (
                    Local::Plane,
                    (f.origin(), [f.x().as_vec(), f.y().as_vec(), f.z().as_vec()]),
                )
            }
            Self::Cylinder(k) => (Local::Cylinder { r: k.radius().mm() }, axes(k.axes())),
            Self::Sphere(k) => (Local::Sphere { r: k.radius().mm() }, axes(k.axes())),
            Self::Torus(k) => (
                Local::Torus {
                    big: k.major().mm(),
                    small: k.minor().mm(),
                },
                axes(k.axes()),
            ),
            Self::Cone(k) => {
                // `rho = r + k z`; a negative slope is the same cone about the opposite axis.
                let (o, [x, y, z]) = axes(k.axes());
                let (s, r) = (k.slope(), k.radius().mm());
                if s < 0.0 {
                    (Local::Cone { r, k: -s }, (o, [x, y, z * -1.0]))
                } else {
                    (Local::Cone { r, k: s }, (o, [x, y, z]))
                }
            }
        };
        Placed {
            local,
            origin,
            axes,
        }
    }
}

/// Whether every point of the NURBS surface `n` lies within `tol` of `kind` (research 75
/// §6.5's `rebuild_analytic`; SU7's certificate). The caller restricts `n` to the face's
/// extent first. Over each Bezier patch, in the kind's frame, the kind's implicit form in
/// the patch's homogeneous coordinates is a Bernstein form with a running error bound, and
/// the distance is bounded by its coefficients ([`crate::recognize`]'s module docs). A piece
/// whose bound is above `tol` is halved; a corner (a point of the surface) farther than
/// `tol` past its rounding refutes, with its parameters as the witness.
///
/// Each piece is one unit of `budget` and polls `stop`; past `2^16` pieces the surface is
/// [`Proof::Undecided`].
///
/// # Errors
///
/// [`SurfaceError::Exhausted`] when `budget` runs out, [`SurfaceError::Stopped`] when `stop`
/// is set, and [`SurfaceError::Defect`] where a Bernstein form cannot be built (a degree
/// past [`scx_algo::bernstein::MAX_DEGREE`]).
pub fn on_analytic(
    n: &NurbsSurface<3>,
    kind: &Analytic,
    tol: Accuracy,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Proof<Point2>, SurfaceError> {
    prove(n, &kind.placed(), tol.length().mm(), budget, stop)
}

/// The kind in its frame, as the certificate reads it (world units).
#[derive(Debug, Clone, Copy)]
pub(crate) enum Local {
    Plane,
    Sphere { r: f64 },
    Cylinder { r: f64 },
    Cone { r: f64, k: f64 },
    Torus { big: f64, small: f64 },
}

/// A kind and its frame: `origin` and the axes `x`, `y` and `z` (the kind's `z` its axis).
pub(crate) struct Placed {
    pub(crate) local: Local,
    pub(crate) origin: Point3,
    pub(crate) axes: [Vec3; 3],
}

/// [`on_analytic`] for a placed kind and an accuracy `acc` at least 0.
pub(crate) fn prove(
    n: &NurbsSurface<3>,
    placed: &Placed,
    acc: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Proof<Point2>, SurfaceError> {
    let mut worst = 0.0_f64;
    let mut reach = 0.0_f64;
    // Over the pieces left unproved: the largest bound, `None` where one has none.
    let mut open: Option<Option<f64>> = None;
    let mut pieces = 0_usize;
    for patch in patches(n) {
        let net = local_net(&patch.net, placed.origin, placed.axes);
        for (h, _) in &net {
            reach = reach.max((h[0].abs() + h[1].abs() + h[2].abs()) / h[3]);
        }
        let mut stack = vec![(patch.lo, patch.hi, net)];
        while let Some((lo, hi, net)) = stack.pop() {
            budget.charge(1).map_err(SurfaceError::Exhausted)?;
            stop.check().map_err(|_| SurfaceError::Stopped)?;
            pieces += 1;
            let (b, halve) = match bound(&placed.local, &net, patch.degrees, acc)? {
                Verdict::Refuted { corner, by } => {
                    let at = match corner {
                        0 => lo,
                        1 => Point2::new(hi.x, lo.y),
                        2 => Point2::new(lo.x, hi.y),
                        _ => hi,
                    };
                    return Ok(Proof::Refuted {
                        at,
                        by: Length::from_mm(by),
                    });
                }
                Verdict::Bounded { bound, halve } => (bound, halve),
            };
            if let Some(b) = b
                && b <= acc
            {
                worst = worst.max(b);
                continue;
            }
            let m = lo.midpoint(hi);
            let su = lo.x < m.x && m.x < hi.x;
            let sv = lo.y < m.y && m.y < hi.y;
            if !(halve && (su || sv) && pieces < MAX_PIECES) {
                open = Some(match (open, b) {
                    (None, b) => b,
                    (Some(Some(l)), Some(b)) => Some(l.max(b)),
                    (Some(_), _) => None,
                });
                continue;
            }
            let mut parts = vec![(lo, hi, net)];
            if su {
                parts = parts
                    .into_iter()
                    .flat_map(|(lo, hi, net)| {
                        let (a, b) = halved(&net, patch.degrees, true);
                        [
                            (lo, Point2::new(m.x, hi.y), a),
                            (Point2::new(m.x, lo.y), hi, b),
                        ]
                    })
                    .collect();
            }
            if sv {
                parts = parts
                    .into_iter()
                    .flat_map(|(lo, hi, net)| {
                        let (a, b) = halved(&net, patch.degrees, false);
                        [
                            (lo, Point2::new(hi.x, m.y), a),
                            (Point2::new(lo.x, m.y), hi, b),
                        ]
                    })
                    .collect();
            }
            // Last pushed is first taken: the lowest corner first.
            stack.extend(parts.into_iter().rev());
        }
    }
    // The rounding of the frame's axes: `16 eps` of the frame's size.
    let sizes = match placed.local {
        Local::Plane => 0.0,
        Local::Sphere { r } | Local::Cylinder { r } | Local::Cone { r, .. } => r,
        Local::Torus { big, small } => big + small,
    };
    let total = |b: f64| Length::from_mm(up(b + 16.0 * EPS * (reach + sizes)));
    Ok(match open {
        Some(b) => Proof::Undecided {
            bound: b.map(|b| total(b.max(worst))),
        },
        None if total(worst).mm() <= acc => Proof::Proved {
            bound: total(worst),
        },
        None => Proof::Undecided {
            bound: Some(total(worst)),
        },
    })
}

/// The piece's homogeneous forms in the kind's frame.
struct Forms {
    x: Bounded,
    y: Bounded,
    z: Bounded,
    w: Bounded,
}

enum Verdict {
    /// The corner (`0` the lowest, `1` past it in `u`, `2` in `v`, `3` the highest) is at
    /// least `by` from the kind, `by` above the accuracy.
    Refuted { corner: usize, by: f64 },
    /// The piece's bound (`None` where the bound's factor does not hold), and whether
    /// halving may still prove it.
    Bounded { bound: Option<f64>, halve: bool },
}

/// The least and largest of `Z / w` over the piece: of its coefficients' quotients, each at
/// the end of its error that widens it.
fn ratio_range(net: &[Hb], k: usize) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (h, e) in net {
        let (z, ez) = (h.get(k).copied()?, e.get(k).copied()?);
        let (wl, wh) = (h[3] - e[3], h[3] + e[3]);
        if !(wl > 0.0) {
            return None;
        }
        let (nl, nh) = (z - ez, z + ez);
        let l = if nl >= 0.0 { nl / wh } else { nl / wl };
        let u = if nh >= 0.0 { nh / wl } else { nh / wh };
        lo = lo.min(l - up(l.abs() * 2.0 * EPS));
        hi = hi.max(u + up(u.abs() * 2.0 * EPS));
    }
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// A lower bound of `rho = |(x, y)|` over the piece (0 where none is found): the piece is
/// inside its control points' hull, so `rho >= e . p` for the unit `e` along their mean,
/// at the least of the points.
fn radial_floor(net: &[Hb]) -> f64 {
    let pts: Vec<(f64, f64, f64)> = net
        .iter()
        .map(|(h, e)| {
            let wl = h[3] - e[3];
            let slack = if wl > 0.0 {
                (e[0] + e[1]) / wl + 4.0 * EPS * (h[0].abs() + h[1].abs()) / wl
            } else {
                f64::INFINITY
            };
            (h[0] / h[3], h[1] / h[3], slack)
        })
        .collect();
    let (mx, my) = pts
        .iter()
        .fold((0.0, 0.0), |(a, b), &(x, y, _)| (a + x, b + y));
    let len = (mx * mx + my * my).sqrt();
    if !(len > 0.0) {
        return 0.0;
    }
    let (ex, ey) = (mx / len, my / len);
    let lo = pts.iter().fold(f64::INFINITY, |m: f64, &(x, y, sl)| {
        m.min(ex * x + ey * y - sl - 4.0 * EPS * (x.abs() + y.abs()))
    });
    if lo.is_finite() { lo.max(0.0) } else { 0.0 }
}
/// The distance of the local point `(x, y, z)` from the kind, in floating point, and its
/// radial part `delta` (the quantity whose branch the bound needs).
fn distance(local: &Local, p: Vec3) -> (f64, f64) {
    let rho = (p.x * p.x + p.y * p.y).sqrt();
    match *local {
        Local::Plane => (p.z.abs(), 0.0),
        Local::Sphere { r } => {
            let d = (p.length() - r).abs();
            (d, d)
        }
        Local::Cylinder { r } => {
            let d = (rho - r).abs();
            (d, d)
        }
        Local::Cone { r, k } => {
            let s = r + k * p.z;
            let c = 1.0 / (1.0 + k * k).sqrt();
            // The foot on the cone's line lies on the nappe where `s + k^2 rho >= 0`.
            if s + k * k * rho >= 0.0 {
                ((rho - s).abs() * c, (rho - s).abs())
            } else {
                let apex = -r / k;
                let d = (rho * rho + (p.z - apex) * (p.z - apex)).sqrt();
                (d, f64::INFINITY)
            }
        }
        Local::Torus { big, small } => {
            let d = ((rho - big) * (rho - big) + p.z * p.z).sqrt();
            ((d - small).abs(), (d - small).abs())
        }
    }
}

fn bound(
    local: &Local,
    net: &[Hb],
    degrees: (usize, usize),
    acc: f64,
) -> Result<Verdict, SurfaceError> {
    let (du, dv) = degrees;
    let at = |i: usize, j: usize| {
        net.get(i * (dv + 1) + j)
            .map_or(Vec3::splat(f64::NAN), |(h, _)| {
                Vec3::new(h[0] / h[3], h[1] / h[3], h[2] / h[3])
            })
    };
    let corners = [at(0, 0), at(du, 0), at(0, dv), at(du, dv)];
    let scale = corners
        .iter()
        .fold(0.0_f64, |m, c| m.max(c.max_dimension()));
    let margin = 64.0 * EPS * scale;
    let mut first_delta = f64::INFINITY;
    let mut seen = 0.0_f64;
    let mut beyond = false;
    for (i, (c, idx)) in corners
        .iter()
        .zip([0, du * (dv + 1), dv, du * (dv + 1) + dv])
        .enumerate()
    {
        let (d, delta) = distance(local, *c);
        if !(d <= acc + margin) {
            let by = floor_distance(local, *c, scale) - corner_slack(net.get(idx), *c);
            if by > acc {
                return Ok(Verdict::Refuted { corner: i, by });
            }
            beyond = true;
        }
        seen = seen.max(d);
        if i == 0 {
            first_delta = delta;
        }
    }
    if beyond {
        // A corner past the accuracy that its rounding leaves unrefuted: no bound proves
        // the piece, and halving keeps the corner.
        return Ok(Verdict::Bounded {
            bound: None,
            halve: false,
        });
    }
    let [x, y, z, w] = forms(net, degrees)?;
    let f = Forms { x, y, z, w };
    // The radial part's bound, the distance's share of it, and the factor's reach
    // `delta_max`: no more than twice the accuracy's radial reach, so the factor loses at
    // most that much.
    let (bd, share, delta_max) = match *local {
        Local::Plane => (max_ratio(&f.z, &f.w), 1.0, f64::INFINITY),
        Local::Sphere { r } | Local::Cylinder { r } => {
            let mut q = defect(sq(&f.x)?.add(&sq(&f.y)?))?;
            if matches!(local, Local::Sphere { .. }) {
                q = defect(q.add(&sq(&f.z)?))?;
            }
            let w2 = sq(&f.w)?;
            let r2 = r * r;
            let q = widen(
                defect(q.add_scaled(&w2, -r2))?,
                2.0 * EPS * r2 * w2.f.max_abs_coefficient(),
            );
            let dm = (r / 8.0).min(2.0 * acc);
            (
                max_ratio(&q, &w2).map(|b| up(b / ((2.0 * r - dm) * (1.0 - 2.0 * EPS)))),
                1.0,
                dm,
            )
        }
        Local::Cone { r, k } => {
            let Some((zl, zh)) = ratio_range(net, 2) else {
                return Ok(Verdict::Bounded {
                    bound: None,
                    halve: true,
                });
            };
            let s_min = (r + (k * zl).min(k * zh)) * (1.0 - 4.0 * EPS)
                - 4.0 * EPS * (k * zl).abs().max((k * zh).abs());
            if !(s_min > 0.0) {
                return Ok(Verdict::Bounded {
                    bound: None,
                    halve: true,
                });
            }
            let w2 = sq(&f.w)?;
            let g = defect(f.w.scale(r).add_scaled(&f.z, k))?;
            let q = defect(defect(sq(&f.x)?.add(&sq(&f.y)?))?.sub(&sq(&g)?))?;
            let c = 1.0 / (1.0 + k * k).sqrt();
            let dm = (s_min / 8.0).min(2.0 * acc / c);
            (
                max_ratio(&q, &w2).map(|b| up(b / ((2.0 * s_min - dm) * (1.0 - 2.0 * EPS)))),
                c * (1.0 + 4.0 * EPS),
                dm,
            )
        }
        Local::Torus { big, small } => {
            let p = defect(sq(&f.x)?.add(&sq(&f.y)?))?;
            let w2 = sq(&f.w)?;
            let shift = big * big - small * small;
            let s = defect(defect(p.add(&sq(&f.z)?))?.add_scaled(&w2, shift))?;
            let s = widen(
                s,
                4.0 * EPS * (big * big + small * small) * w2.f.max_abs_coefficient(),
            );
            let pw = defect(p.mul(&w2))?;
            let four = 4.0 * big * big;
            let n = defect(sq(&s)?.add_scaled(&pw, -four))?;
            let n = widen(n, 4.0 * EPS * four * pw.f.max_abs_coefficient());
            let w4 = sq(&w2)?;
            let dm = (small.min(big - small) / 8.0).min(2.0 * acc);
            // `d_t'^2 >= (rho + R)^2`, with `rho` at least `R - r - delta` and at least the
            // piece's least reach from the axis along its control points' mean direction.
            let far = (2.0 * big - small - dm).max(radial_floor(net) + big);
            let lead = (2.0 * small - dm) * (far * far - small * small);
            (
                max_ratio(&n, &w4).map(|b| up(b / (lead * (1.0 - 8.0 * EPS)))),
                1.0,
                dm,
            )
        }
    };
    // The bound holds only where `delta` starts below its reach.
    if !(first_delta < delta_max) {
        return Ok(Verdict::Bounded {
            bound: None,
            halve: false,
        });
    }
    let bound = bd.filter(|&bd| bd < delta_max).map(|bd| up(bd * share));
    // A corner this close to the accuracy leaves the bound too little room to close.
    let halve = !bound.is_some_and(|b| b <= acc) && !(seen >= acc * (1.0 - NEAR));
    Ok(Verdict::Bounded { bound, halve })
}

/// A lower bound of the distance of the local point `p` from the whole kind (the cone's
/// both nappes), less the rounding of its evaluation (`64 eps` of the sizes involved).
fn floor_distance(local: &Local, p: Vec3, scale: f64) -> f64 {
    let rho = (p.x * p.x + p.y * p.y).sqrt();
    let (d, size) = match *local {
        Local::Plane => (p.z.abs(), scale),
        Local::Sphere { r } => ((p.length() - r).abs(), scale + r),
        Local::Cylinder { r } => ((rho - r).abs(), scale + r),
        Local::Cone { r, k } => {
            // Each nappe lies on its line in the meridian half-plane, `rho = s` or
            // `rho = -s`, so the distance is at least the nearer line's.
            let s = r + k * p.z;
            let c = (1.0 / (1.0 + k * k).sqrt()) * (1.0 - 4.0 * EPS);
            ((rho - s.abs()).abs() * c, scale * (1.0 + k) + r)
        }
        Local::Torus { big, small } => {
            let d = ((rho - big) * (rho - big) + p.z * p.z).sqrt();
            ((d - small).abs(), scale + big + small)
        }
    };
    d - 64.0 * EPS * size
}

/// A bound of the distance of the corner `p`, as computed from the net's point `h`, from
/// the exact corner: each coordinate within `(e_k + |p_k| e_w) / (w - e_w)` and its
/// division's rounding; infinite where the weight is not proved positive.
fn corner_slack(h: Option<&Hb>, p: Vec3) -> f64 {
    let Some((h, e)) = h else {
        return f64::INFINITY;
    };
    let wl = h[3] - e[3];
    if !(wl > 0.0) {
        return f64::INFINITY;
    }
    let sum = p.x.abs() + p.y.abs() + p.z.abs();
    up((e[0] + e[1] + e[2] + sum * e[3]) / wl + 4.0 * EPS * sum)
}
