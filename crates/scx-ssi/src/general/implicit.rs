// Written for solidcorex. Apache-2.0.
//! The implicit forms of the analytic kinds (research 73 §4.5, row `SM6a`), and the field
//! `g = f(x)` of one of them over another surface's [`Net`].
//!
//! Each kind is `O + a X + b Y + c Z` for a canonical shape in `(a, b, c)`, its frame's axes
//! `X`, `Y` and `Z` (with `Z` its frame's z times its sense, as the kinds place points). In
//! the local coordinates `y = M (x - O)`, `M` the matrix of rows `X`, `Y`, `Z`:
//!
//! | kind | `f0(y)` | degree |
//! |---|---|---|
//! | plane | `y3` | 1 |
//! | cylinder | `y1^2 + y2^2 - r^2` | 2 |
//! | cone | `y1^2 + y2^2 - (r + k y3)^2`, `k = tan(half_angle)` (the kind's stored slope) | 2 |
//! | sphere | `abs(y)^2 - r^2` | 2 |
//! | torus | `(abs(y)^2 + R^2 - r^2)^2 - 4 R^2 (y1^2 + y2^2)` | 4 |
//!
//! Over a net `x = H / W`, the field is the polynomial `G = W^k f0(M (H - O W) / W)`, of
//! degree `k` times the net's in each parameter, built in [`Bounded`] arithmetic so its
//! coefficients carry a bound of their distance from the exact ones.
//!
//! The axes are unit and orthogonal only to rounding, so `M` is not exactly orthonormal and
//! the zero set of `f_used(x) = f0(M (x - O))` is not exactly the kind's surface. With
//! `eta = |M M^T - I|` (the largest entry, enclosed), the two lie within [`Implicit::shift`]
//! of each other in space ([`Implicit::shift`] has the argument).

use scx_algo::bernstein::Bounded;
use scx_num::certified::gamma;
use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::interval::Ivl;
use scx_num::vector3d::Vec3;
use scx_surface::Surface;

use super::net::{Net, scale_inexact, up};

/// The canonical shape, in local coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    Plane,
    Cylinder { r: f64 },
    Cone { r: f64, k: f64 },
    Sphere { r: f64 },
    Torus { big: f64, small: f64 },
}

/// The implicit form of an analytic surface: a canonical shape in the kind's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Implicit {
    origin: [f64; 3],
    /// The rows `X`, `Y`, `Z` of `M`.
    rows: [[f64; 3]; 3],
    shape: Shape,
    /// `|M M^T - I|`, the largest entry, an upper bound.
    eta: f64,
}

impl Implicit {
    /// The form of an analytic kind; `None` for a NURBS surface (no form is proved for one)
    /// or a frame too far from orthonormal for [`Self::shift`]'s bound (`eta` above
    /// `1e-6`, which no frame the kinds build reaches).
    pub(crate) fn of(s: &Surface) -> Option<Self> {
        let (frame, sign, shape) = match s {
            Surface::Plane(p) => (p.frame(), 1.0, Shape::Plane),
            Surface::Cylinder(c) => (
                c.frame(),
                c.sense().sign(),
                Shape::Cylinder { r: c.radius().mm() },
            ),
            Surface::Cone(c) => (
                c.frame(),
                c.sense().sign(),
                Shape::Cone {
                    r: c.radius().mm(),
                    k: fmath::tan(c.half_angle()),
                },
            ),
            Surface::Sphere(c) => (
                c.frame(),
                c.sense().sign(),
                Shape::Sphere { r: c.radius().mm() },
            ),
            Surface::Torus(t) => (
                t.frame(),
                t.sense().sign(),
                Shape::Torus {
                    big: t.major().mm(),
                    small: t.minor().mm(),
                },
            ),
            _ => return None,
        };
        let o = frame.origin();
        let (x, y, z) = (*frame.x(), *frame.y(), *frame.z() * sign);
        let rows = [[x.x, x.y, x.z], [y.x, y.y, y.z], [z.x, z.y, z.z]];
        let mut eta = 0.0_f64;
        for (i, a) in rows.iter().enumerate() {
            for (j, b) in rows.iter().enumerate() {
                let d = a.iter().zip(b).fold(Ivl::from(0.0), |s, (p, q)| {
                    s + Ivl::from(*p) * Ivl::from(*q)
                });
                let d = if i == j { d - Ivl::from(1.0) } else { d };
                eta = eta.max(d.mag());
            }
        }
        (eta <= 1e-6).then_some(Self {
            origin: [o.x, o.y, o.z],
            rows,
            shape,
            eta,
        })
    }

    /// The degree `k` of the form: 1 for a plane, 2 for a quadric, 4 for a torus.
    pub(crate) const fn degree(&self) -> usize {
        match self.shape {
            Shape::Plane => 1,
            Shape::Cylinder { .. } | Shape::Cone { .. } | Shape::Sphere { .. } => 2,
            Shape::Torus { .. } => 4,
        }
    }

    /// The field `G = W^k f0(M (H - O W) / W)` over `net`, with its rounding bound (module
    /// docs). `None` when a degree passes the Bernstein limit or a value is not finite.
    pub(crate) fn field(&self, net: &Net) -> Option<Bounded> {
        let w = Bounded::exact(net.w.clone());
        let mut d = Vec::with_capacity(3);
        for (h, o) in net.h.iter().zip(self.origin) {
            d.push(Bounded::exact(h.clone()).add_scaled(&w, -o).ok()?);
        }
        let [d0, d1, d2] = <[Bounded; 3]>::try_from(d).ok()?;
        let local = |m: &[f64; 3]| -> Option<Bounded> {
            let [a, b, c] = *m;
            d0.scale(a).add_scaled(&d1, b).ok()?.add_scaled(&d2, c).ok()
        };
        let [mx, my, mz] = &self.rows;
        let (lx, ly, lz) = (local(mx)?, local(my)?, local(mz)?);
        let sq = |b: &Bounded| b.mul(b).ok();
        let radial = || sq(&lx)?.add(&sq(&ly)?).ok();
        let g = match self.shape {
            Shape::Plane => lz,
            Shape::Cylinder { r } => {
                let r2 = r * r;
                radial()?
                    .sub(&scale_inexact(&sq(&w)?, r2, gamma(1) * r2))
                    .ok()?
            }
            Shape::Cone { r, k } => {
                let rho = w.scale(r).add_scaled(&lz, k).ok()?;
                radial()?.sub(&sq(&rho)?).ok()?
            }
            Shape::Sphere { r } => {
                let r2 = r * r;
                radial()?
                    .add(&sq(&lz)?)
                    .ok()?
                    .sub(&scale_inexact(&sq(&w)?, r2, gamma(1) * r2))
                    .ok()?
            }
            Shape::Torus { big, small } => {
                let w2 = sq(&w)?;
                let c = big * big - small * small;
                let c_err = gamma(2) * (big * big + small * small);
                let rad = radial()?;
                let s = rad
                    .add(&sq(&lz)?)
                    .ok()?
                    .add(&scale_inexact(&w2, c, c_err))
                    .ok()?;
                let four = 4.0 * big * big;
                let t = scale_inexact(&rad.mul(&w2).ok()?, four, gamma(1) * four);
                sq(&s)?.sub(&t).ok()?
            }
        };
        g.is_finite().then_some(g)
    }

    /// A bound of the distance, in space, between the zero set of `f_used` and the kind's
    /// surface, over points within `rho` of the origin: `4 eta rho`, rounded up.
    ///
    /// With `y = M (x - O)` and the kind's point `x = O + M^T y_c` (`f0(y_c) = 0`), `M M^T =
    /// I + E`, `|E|` at most `eta` per entry: the point `x' = O + M^-1 y_c` has `f_used(x') = 0`
    /// and `x' - x = M^T ((I + E)^-1 - I) y_c`, of length at most `|M| 3 eta / (1 - 3 eta)
    /// |y_c|`, and `|y_c|` is at most `|M^-1| rho`; both norms are below `1 + 2 eta`, so for
    /// `eta <= 1e-6` the length is below `4 eta rho` (the other way is the same argument).
    pub(crate) fn shift(&self, rho: f64) -> f64 {
        up(4.0 * self.eta * rho) * 3.0
    }

    /// The largest distance from the origin to a point of `b`, an upper bound.
    pub(crate) fn reach(&self, b: &Extent3d) -> f64 {
        let [ox, oy, oz] = self.origin;
        let far = |lo: f64, hi: f64, o: f64| {
            (Ivl::from(lo) - Ivl::from(o))
                .mag()
                .max((Ivl::from(hi) - Ivl::from(o)).mag())
        };
        let v = Vec3::new(
            Ivl::from(far(b.x_min(), b.x_max(), ox)),
            Ivl::from(far(b.y_min(), b.y_max(), oy)),
            Ivl::from(far(b.z_min(), b.z_max(), oz)),
        );
        v.norm().hi
    }

    /// Bounds of `|grad f_used|` over the box `b` (a world box), as `(sup, inf)`: the
    /// gradient of `f0` in interval arithmetic over the box's local coordinates, widened by
    /// [`Self::shift`], times `1 + 2 eta` (the norm of `M^T`) for the upper bound and
    /// divided by it for the lower. `None` when not finite.
    pub(crate) fn gradient(&self, b: &Extent3d) -> Option<(f64, f64)> {
        let [ox, oy, oz] = self.origin;
        let d = [
            Ivl::new(b.x_min(), b.x_max()) - Ivl::from(ox),
            Ivl::new(b.y_min(), b.y_max()) - Ivl::from(oy),
            Ivl::new(b.z_min(), b.z_max()) - Ivl::from(oz),
        ];
        let widen = self.shift(self.reach(b));
        let loc = |m: &[f64; 3]| {
            let s = d
                .iter()
                .zip(m)
                .fold(Ivl::from(0.0), |s, (x, c)| s + *x * Ivl::from(*c));
            s + Ivl::new(-widen, widen)
        };
        let [mx, my, mz] = &self.rows;
        let (y1, y2, y3) = (loc(mx), loc(my), loc(mz));
        let two = Ivl::from(2.0);
        let grad = match self.shape {
            Shape::Plane => Vec3::new(Ivl::from(0.0), Ivl::from(0.0), Ivl::from(1.0)),
            Shape::Cylinder { .. } => Vec3::new(two * y1, two * y2, Ivl::from(0.0)),
            Shape::Cone { r, k } => {
                let k = Ivl::from(k);
                Vec3::new(
                    two * y1,
                    two * y2,
                    Ivl::from(0.0) - two * k * (Ivl::from(r) + k * y3),
                )
            }
            Shape::Sphere { .. } => Vec3::new(two * y1, two * y2, two * y3),
            Shape::Torus { big, small } => {
                let (bi, si) = (Ivl::from(big), Ivl::from(small));
                let c = bi * bi - si * si;
                let s = y1 * y1 + y2 * y2 + y3 * y3 + c;
                let four = Ivl::from(4.0);
                let eight = Ivl::from(8.0) * bi * bi;
                Vec3::new(
                    four * s * y1 - eight * y1,
                    four * s * y2 - eight * y2,
                    four * s * y3,
                )
            }
        };
        let n = grad.norm();
        if !n.is_finite() {
            return None;
        }
        let m = 1.0 + 2.0 * self.eta;
        Some((up(n.hi * m), (n.lo / m) * (1.0 - gamma(4))))
    }
}
