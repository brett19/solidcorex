// Written for solidcorex. Apache-2.0.
//! Closed-form volumes, areas and moments of the recipes the step-4 slice builds (research
//! 72 §2.4, §8.1, task T9): the five primitives, a spherical cap, a sector of a torus (half a
//! torus among them), a block minus through-cylinders (`plate_12`), a gear (`gear_60`), and
//! extrusions of profiles made of lines and circular arcs.
//!
//! Each truth is a pure function of the recipe's parameters, on plain `f64` data and no
//! geometry type, so a kernel and its reference share no code.
//!
//! **Local placement.** Every function answers for the body at its *standard placement*:
//! its axis on `z`, its base on `z = 0` (a sphere, a cap and a torus centred on the origin;
//! a block with its corner on the origin and its extents along the axes). [`Solid::place`]
//! carries the answer to any frame: the volume and area are unchanged, the first moments
//! move as points do and the second as `R S R^T + ...` (a rigid motion). The frame's axes
//! are taken as orthonormal; a mirrored frame is a mirror image and has the same volume.
//!
//! **What is stated.** [`Moments`] holds the integrals over the body, at unit density, about
//! the placement's origin: `V = int 1`, `F = int x`, `S = int x x^T`. They add under union
//! of disjoint bodies and subtract under removal of a contained one, which is how the
//! Boolean recipes are built. The inertia tensor is a derived view
//! ([`Moments::inertia_about_centroid`]).
//!
//! **Error.** Each result is a short sum of products of the parameters, `pi` and (for arcs)
//! `sin` and `cos` of angles, in `f64` with [`scx_num::fmath`]. A result is within
//! [`REL_ERR`] of the exact value, relative to the largest term that went into it (so a
//! difference of two nearly equal volumes is judged against the volumes, not the remainder;
//! for an arc the terms are powers of the distance from its centre, so a small region far
//! from its arc's centre, like the gear's slot, has an error against its own size of `REL_ERR`
//! times `(distance / size)^3` at worst).
//! This is a bound by inspection of the short expressions, 64 ulps with room, and the tests
//! check it against double-double on the primitives.
//!
//! References: Pappus' centroid theorems; the principal moments of a cylinder, cone and
//! sphere as in any mechanics text; Green's theorem for planar regions (area `A = oint x dy`,
//! and the same with `x^p y^q` for the moments), with closed-form integrals of `cos^a sin^b`
//! along arcs.

// Indices are loop counters over fixed 3-arrays and 3x3 matrices, and the binomial and
// polynomial helpers' own bounds.
#![allow(clippy::indexing_slicing)]

use core::fmt;

use scx_num::fmath;

use crate::analytic::Frame;
use crate::profile::Seg;

/// The stated relative error of every result, against the largest term in it.
pub const REL_ERR: f64 = 1e-13;

const PI: f64 = core::f64::consts::PI;
const TAU: f64 = core::f64::consts::TAU;

/// Why a recipe has no solid.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum SolidError {
    /// A dimension that must be positive and finite is not.
    Dimension(&'static str),
    /// A parameter outside its range, named.
    Range(&'static str),
    /// A profile that is not closed, or whose arc has different radii at its ends.
    Profile(&'static str),
    /// Holes or slots that overlap each other or leave the body: the closed form assumes
    /// they are apart and inside.
    Apart,
}

impl fmt::Display for SolidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SolidError::Dimension(n) => write!(f, "dimension {n} must be positive and finite"),
            SolidError::Range(n) => write!(f, "parameter {n} is out of range"),
            SolidError::Profile(n) => write!(f, "bad profile: {n}"),
            SolidError::Apart => write!(f, "cuts overlap each other or leave the body"),
        }
    }
}

impl core::error::Error for SolidError {}

fn positive(v: f64, name: &'static str) -> Result<f64, SolidError> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(SolidError::Dimension(name))
    }
}

fn finite(v: f64, name: &'static str) -> Result<f64, SolidError> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(SolidError::Range(name))
    }
}

fn nonneg(v: f64, name: &'static str) -> Result<f64, SolidError> {
    if v.is_finite() && v >= 0.0 {
        Ok(v)
    } else {
        Err(SolidError::Dimension(name))
    }
}

/// `x^n` by multiplication (the `powi` method differs between platforms).
fn ipow(x: f64, n: u32) -> f64 {
    (0..n).fold(1.0, |a, _| a * x)
}

type M3 = [[f64; 3]; 3];

/// The integrals of a body at unit density, about the origin of its placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moments {
    /// `int 1 dV`.
    pub volume: f64,
    /// `int x dV`, a vector.
    pub first: [f64; 3],
    /// `int x x^T dV`, symmetric.
    pub second: M3,
}

impl Moments {
    /// The empty body.
    pub const ZERO: Moments = Moments {
        volume: 0.0,
        first: [0.0; 3],
        second: [[0.0; 3]; 3],
    };

    /// Disjoint bodies together.
    #[must_use]
    pub fn plus(&self, o: &Moments) -> Moments {
        Moments {
            volume: self.volume + o.volume,
            first: core::array::from_fn(|i| self.first[i] + o.first[i]),
            second: core::array::from_fn(|i| {
                core::array::from_fn(|j| self.second[i][j] + o.second[i][j])
            }),
        }
    }

    /// A body contained in this one removed.
    #[must_use]
    pub fn minus(&self, o: &Moments) -> Moments {
        Moments {
            volume: self.volume - o.volume,
            first: core::array::from_fn(|i| self.first[i] - o.first[i]),
            second: core::array::from_fn(|i| {
                core::array::from_fn(|j| self.second[i][j] - o.second[i][j])
            }),
        }
    }

    /// The body moved by `d`: with `x -> x + d`, `F' = F + V d` and
    /// `S' = S + d F^T + F d^T + V d d^T`.
    #[must_use]
    pub fn translated(&self, d: [f64; 3]) -> Moments {
        Moments {
            volume: self.volume,
            first: core::array::from_fn(|i| self.first[i] + self.volume * d[i]),
            second: core::array::from_fn(|i| {
                core::array::from_fn(|j| {
                    self.second[i][j]
                        + d[i] * self.first[j]
                        + self.first[i] * d[j]
                        + self.volume * d[i] * d[j]
                })
            }),
        }
    }

    /// The body turned about the origin by the matrix whose columns are `axes`:
    /// `F' = R F`, `S' = R S R^T`.
    #[must_use]
    pub fn rotated(&self, axes: [[f64; 3]; 3]) -> Moments {
        // r[i][k] = axes[k][i]
        let r = |i: usize, k: usize| axes[k][i];
        let first = core::array::from_fn(|i| (0..3).map(|k| r(i, k) * self.first[k]).sum());
        let rs: M3 = core::array::from_fn(|i| {
            core::array::from_fn(|j| (0..3).map(|k| r(i, k) * self.second[k][j]).sum())
        });
        let second = core::array::from_fn(|i| {
            core::array::from_fn(|j| (0..3).map(|k| rs[i][k] * r(j, k)).sum())
        });
        Moments {
            volume: self.volume,
            first,
            second,
        }
    }

    /// The body carried from its standard placement to `frame`.
    #[must_use]
    pub fn placed(&self, frame: &Frame) -> Moments {
        self.rotated([frame.x, frame.y, frame.z])
            .translated(frame.origin)
    }

    /// The centre of volume.
    #[must_use]
    pub fn centroid(&self) -> [f64; 3] {
        self.first.map(|f| f / self.volume)
    }

    /// The inertia tensor about the centroid, for unit density:
    /// `I = tr(C) 1 - C` where `C = int (x - c)(x - c)^T dV = S - F F^T / V`.
    #[must_use]
    pub fn inertia_about_centroid(&self) -> M3 {
        let c: M3 = core::array::from_fn(|i| {
            core::array::from_fn(|j| {
                self.second[i][j] - self.first[i] * self.first[j] / self.volume
            })
        });
        let tr = c[0][0] + c[1][1] + c[2][2];
        core::array::from_fn(|i| {
            core::array::from_fn(|j| if i == j { tr - c[i][j] } else { -c[i][j] })
        })
    }
}

/// A body's moments at its placement and its boundary area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Solid {
    /// The integrals over the body.
    pub moments: Moments,
    /// The area of the whole boundary.
    pub area: f64,
}

impl Solid {
    /// The volume.
    #[must_use]
    pub fn volume(&self) -> f64 {
        self.moments.volume
    }

    /// The same body at `frame` (a rigid motion: the area is unchanged).
    #[must_use]
    pub fn place(&self, frame: &Frame) -> Solid {
        Solid {
            moments: self.moments.placed(frame),
            area: self.area,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Primitives

/// A block with its corner on the origin and extents `dx, dy, dz` along the axes.
/// Derivation: `int x^2 dV = dx^3 dy dz / 3`, `int x y dV = dx^2 dy^2 dz / 4`.
pub fn block(dx: f64, dy: f64, dz: f64) -> Result<Solid, SolidError> {
    let d = [
        positive(dx, "dx")?,
        positive(dy, "dy")?,
        positive(dz, "dz")?,
    ];
    let volume = d[0] * d[1] * d[2];
    let first = d.map(|e| volume * e / 2.0);
    let second = core::array::from_fn(|i| {
        core::array::from_fn(|j| {
            if i == j {
                volume * d[i] * d[i] / 3.0
            } else {
                volume * d[i] * d[j] / 4.0
            }
        })
    });
    Ok(Solid {
        moments: Moments {
            volume,
            first,
            second,
        },
        area: 2.0 * (d[0] * d[1] + d[1] * d[2] + d[0] * d[2]),
    })
}

/// A body of revolution about `z`, `z` in `[z0, z1]`, whose slice at `z` is the disk of
/// squared radius `rho2(z)`, a polynomial with ascending coefficients.
///
/// A disk of radius `rho` has `int dA = pi rho^2`, `int x dA = 0`, `int x^2 dA = pi rho^4 / 4`,
/// so `V = pi int rho^2`, `F_z = pi int z rho^2`, `S_zz = pi int z^2 rho^2`,
/// `S_xx = S_yy = pi/4 int rho^4`, the rest zero. The polynomials are integrated exactly
/// (their antiderivative at the ends).
fn revolved(rho2: &[f64], z0: f64, z1: f64) -> Moments {
    let sq = poly_mul(rho2, rho2);
    let zr = poly_mul(&[0.0, 1.0], rho2);
    let zzr = poly_mul(&[0.0, 0.0, 1.0], rho2);
    let int = |p: &[f64]| poly_int(p, z1) - poly_int(p, z0);
    let xx = PI / 4.0 * int(&sq);
    let mut second = [[0.0; 3]; 3];
    second[0][0] = xx;
    second[1][1] = xx;
    second[2][2] = PI * int(&zzr);
    Moments {
        volume: PI * int(rho2),
        first: [0.0, 0.0, PI * int(&zr)],
        second,
    }
}

fn poly_mul(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    out
}

/// The antiderivative of a polynomial (ascending coefficients) at `z`.
fn poly_int(p: &[f64], z: f64) -> f64 {
    // Horner on sum c_k z^(k+1) / (k+1)
    let mut acc = 0.0;
    for (k, c) in p.iter().enumerate().rev() {
        acc = (acc + c / (k as f64 + 1.0)) * z;
    }
    acc
}

/// A cylinder of radius `r` and height `h`, base on `z = 0`.
pub fn cylinder(r: f64, h: f64) -> Result<Solid, SolidError> {
    let (r, h) = (positive(r, "r")?, positive(h, "h")?);
    Ok(Solid {
        moments: revolved(&[r * r], 0.0, h),
        area: 2.0 * PI * r * r + 2.0 * PI * r * h,
    })
}

/// A cone or frustum: radius `r0` on `z = 0` and `r1` on `z = h` (`r1 = 0` for a cone;
/// either may be zero, not both). The volume is `pi h (r0^2 + r0 r1 + r1^2) / 3`; the side
/// area is `pi (r0 + r1) s` with the slant `s = sqrt(h^2 + (r0 - r1)^2)`.
pub fn cone(r0: f64, r1: f64, h: f64) -> Result<Solid, SolidError> {
    let (r0, r1, h) = (nonneg(r0, "r0")?, nonneg(r1, "r1")?, positive(h, "h")?);
    if r0 == 0.0 && r1 == 0.0 {
        return Err(SolidError::Dimension("r0 + r1"));
    }
    let k = (r1 - r0) / h;
    let slant = (h * h + (r0 - r1) * (r0 - r1)).sqrt();
    Ok(Solid {
        moments: revolved(&[r0 * r0, 2.0 * r0 * k, k * k], 0.0, h),
        area: PI * (r0 + r1) * slant + PI * r0 * r0 + PI * r1 * r1,
    })
}

/// A sphere of radius `r` about the origin: `V = 4 pi r^3 / 3`, `A = 4 pi r^2`, and
/// `int x^2 dV = 4 pi r^5 / 15` (the 2/5 m r^2 inertia).
pub fn sphere(r: f64) -> Result<Solid, SolidError> {
    let r = positive(r, "r")?;
    Ok(Solid {
        moments: revolved(&[r * r, 0.0, -1.0], -r, r),
        area: 4.0 * PI * r * r,
    })
}

/// The cap of the sphere of radius `r` about the origin on its `+z` pole, of thickness `h`
/// (`0 < h <= 2 r`; `h = 2 r` is the sphere): the part with `z >= r - h`.
/// `V = pi h^2 (3 r - h) / 3`; the boundary is the spherical zone `2 pi r h` and the flat disk
/// of radius `sqrt(h (2 r - h))` at `z = r - h`.
pub fn sphere_cap(r: f64, h: f64) -> Result<Solid, SolidError> {
    let (r, h) = (positive(r, "r")?, positive(h, "h")?);
    if h > 2.0 * r {
        return Err(SolidError::Range("h"));
    }
    Ok(Solid {
        moments: revolved(&[r * r, 0.0, -1.0], r - h, r),
        area: 2.0 * PI * r * h + PI * h * (2.0 * r - h),
    })
}

/// A sector of a torus about the origin, axis `z`, of major radius `big`, minor radius
/// `small < big`, from the `+x` axis counter-clockwise through `sweep` (`0 < sweep <= 2 pi`;
/// `pi` is half a torus, `2 pi` all of it).
///
/// With a point `((R + s) cos t, (R + s) sin t, u)`, `(s, u)` in the disk of radius `r`,
/// `dV = (R + s) ds du dt`, and over the disk `int (R+s) = pi r^2 R`, `int (R+s)^3 =
/// pi r^2 R^3 + 3 R pi r^4 / 4`, `int u^2 (R+s) = pi r^4 R / 4`. Then `V = sweep pi r^2 R`;
/// `S_xx = int (R+s)^3 * int cos^2 t dt`, and likewise `S_yy`, `S_xy`; `S_zz = sweep *
/// pi r^4 R / 4`; `F = int (R+s)^2 * (sin sweep, 1 - cos sweep, 0)`, with `int (R+s)^2 =
/// pi r^2 (R^2 + r^2/4)`. A sector short of the whole has two flat end disks `pi r^2`
/// each; the side is `sweep 2 pi r R`.
pub fn torus_sector(big: f64, small: f64, sweep: f64) -> Result<Solid, SolidError> {
    let (big, small) = (positive(big, "R")?, positive(small, "r")?);
    if small >= big {
        return Err(SolidError::Range("r"));
    }
    if !(sweep.is_finite() && sweep > 0.0 && sweep <= TAU) {
        return Err(SolidError::Range("sweep"));
    }
    let disk = PI * small * small;
    let c1 = disk * big; // int (R+s)
    let c2 = disk * (big * big + small * small / 4.0); // int (R+s)^2
    let c3 = disk * big * big * big + 3.0 * big * PI * ipow(small, 4) / 4.0; // int (R+s)^3
    let (sn, cs) = fmath::sin_cos(sweep);
    let (s2, c2a) = fmath::sin_cos(2.0 * sweep);
    // int_0^w cos^2 = w/2 + sin 2w / 4; sin^2 = w/2 - sin 2w / 4; sin cos = (1 - cos 2w)/4
    let cc = sweep / 2.0 + s2 / 4.0;
    let ss = sweep / 2.0 - s2 / 4.0;
    let sc = (1.0 - c2a) / 4.0;
    let mut second = [[0.0; 3]; 3];
    second[0][0] = c3 * cc;
    second[1][1] = c3 * ss;
    second[0][1] = c3 * sc;
    second[1][0] = c3 * sc;
    second[2][2] = sweep * PI * ipow(small, 4) * big / 4.0;
    let ends = if sweep < TAU { 2.0 * disk } else { 0.0 };
    Ok(Solid {
        moments: Moments {
            volume: sweep * c1,
            first: [c2 * sn, c2 * (1.0 - cs), 0.0],
            second,
        },
        area: sweep * 2.0 * PI * small * big + ends,
    })
}

/// A whole torus: [`torus_sector`] through `2 pi`.
pub fn torus(big: f64, small: f64) -> Result<Solid, SolidError> {
    torus_sector(big, small, TAU)
}

// ---------------------------------------------------------------------------------------
// Profiles of lines and arcs

/// The planar integrals `A`, `int x`, `int y`, `int x^2`, `int x y`, `int y^2`, and the
/// length of the boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Planar {
    area: f64,
    qx: f64,
    qy: f64,
    xx: f64,
    xy: f64,
    yy: f64,
    length: f64,
}

/// `int_{t0}^{t1} cos^a sin^b` in closed form by the reduction formulas.
fn trig(a: u32, b: u32, t0: f64, t1: f64) -> f64 {
    let (s0, c0) = fmath::sin_cos(t0);
    let (s1, c1) = fmath::sin_cos(t1);
    let at = |s: f64, c: f64, ca: u32, sb: u32| ipow(c, ca) * ipow(s, sb);
    if a >= 2 {
        let n = f64::from(a + b);
        (at(s1, c1, a - 1, b + 1) - at(s0, c0, a - 1, b + 1)) / n
            + f64::from(a - 1) / n * trig(a - 2, b, t0, t1)
    } else if b >= 2 {
        let n = f64::from(a + b);
        -(at(s1, c1, a + 1, b - 1) - at(s0, c0, a + 1, b - 1)) / n
            + f64::from(b - 1) / n * trig(a, b - 2, t0, t1)
    } else {
        match (a, b) {
            (0, 0) => t1 - t0,
            (1, 0) => s1 - s0,
            (0, 1) => c0 - c1,
            _ => (s1 * s1 - s0 * s0) / 2.0,
        }
    }
}

/// `oint x^p y^q dy` along one segment from `p0`, for `p + q <= 3`.
fn dy_integral(seg: &Seg, start_sweep: Option<(f64, f64, f64)>, p: u32, q: u32) -> f64 {
    match *seg {
        Seg::Line { start: p0, end: to } => {
            // x = x0 + t dx, y = y0 + t dy: the integrand x^p y^q dy is a polynomial of
            // degree <= 3 in t, integrated exactly by 3-point Gauss-Legendre (exact to 5).
            let (dx, dy) = (to[0] - p0[0], to[1] - p0[1]);
            let w = 5.0 / 18.0;
            let off = 0.5 * (0.6f64).sqrt();
            let nodes = [(0.5 - off, w), (0.5, 8.0 / 18.0), (0.5 + off, w)];
            nodes
                .iter()
                .map(|&(t, wt)| wt * ipow(p0[0] + t * dx, p) * ipow(p0[1] + t * dy, q) * dy)
                .sum()
        }
        Seg::Arc { centre: center, .. } => {
            // x = cx + r cos t, y = cy + r sin t, dy = r cos t dt; expand by the binomial
            // theorem into terms cos^a sin^b.
            let (r, t0, t1) = start_sweep.unwrap_or((0.0, 0.0, 0.0));
            let mut total = 0.0;
            for i in 0..=p {
                for j in 0..=q {
                    let coef = binom(p, i)
                        * binom(q, j)
                        * ipow(center[0], p - i)
                        * ipow(r, i)
                        * ipow(center[1], q - j)
                        * ipow(r, j)
                        * r;
                    total += coef * trig(i + 1, j, t0, t1);
                }
            }
            total
        }
    }
}

fn binom(n: u32, k: u32) -> f64 {
    let mut b = 1.0;
    for i in 0..k {
        b = b * f64::from(n - i) / f64::from(i + 1);
    }
    b
}

fn planar_raw(loops: &[Vec<Seg>]) -> Result<Planar, SolidError> {
    let mut out = Planar {
        area: 0.0,
        qx: 0.0,
        qy: 0.0,
        xx: 0.0,
        xy: 0.0,
        yy: 0.0,
        length: 0.0,
    };
    for lp in loops {
        let Some(first) = lp.first() else {
            return Err(SolidError::Profile("empty loop"));
        };
        let origin = first.start();
        if !(origin[0].is_finite() && origin[1].is_finite()) {
            return Err(SolidError::Profile("empty loop"));
        }
        let mut p0 = origin;
        for seg in lp {
            // Each piece starts where the one before ends.
            let start = seg.start();
            if (start[0] - p0[0]).abs() > 1e-9 * (1.0 + p0[0].abs())
                || (start[1] - p0[1]).abs() > 1e-9 * (1.0 + p0[1].abs())
            {
                return Err(SolidError::Profile("loop is not closed"));
            }
            p0 = start;
            let to = seg.end();
            let sweep = match *seg {
                Seg::Line { .. } => {
                    out.length += ((to[0] - p0[0]) * (to[0] - p0[0])
                        + (to[1] - p0[1]) * (to[1] - p0[1]))
                        .sqrt();
                    None
                }
                Seg::Arc { centre, ccw, .. } => {
                    let (a, b) = (
                        [p0[0] - centre[0], p0[1] - centre[1]],
                        [to[0] - centre[0], to[1] - centre[1]],
                    );
                    let (ra, rb) = (
                        (a[0] * a[0] + a[1] * a[1]).sqrt(),
                        (b[0] * b[0] + b[1] * b[1]).sqrt(),
                    );
                    if !(ra > 0.0 && (ra - rb).abs() <= 1e-9 * ra) {
                        return Err(SolidError::Profile("arc radii differ"));
                    }
                    let t0 = fmath::atan2(a[1], a[0]);
                    let mut t1 = fmath::atan2(b[1], b[0]);
                    if ccw {
                        while t1 <= t0 {
                            t1 += TAU;
                        }
                        if t1 - t0 > TAU {
                            t1 -= TAU;
                        }
                    } else {
                        while t1 >= t0 {
                            t1 -= TAU;
                        }
                        if t0 - t1 > TAU {
                            t1 += TAU;
                        }
                    }
                    out.length += ra * (t1 - t0).abs();
                    Some((ra, t0, t1))
                }
            };
            let i = |p, q| dy_integral(seg, sweep, p, q);
            out.area += i(1, 0);
            out.qx += i(2, 0) / 2.0;
            out.qy += i(1, 1);
            out.xx += i(3, 0) / 3.0;
            out.xy += i(2, 1) / 2.0;
            out.yy += i(1, 2);
            p0 = to;
        }
        if (p0[0] - origin[0]).abs() > 1e-9 * (1.0 + origin[0].abs())
            || (p0[1] - origin[1]).abs() > 1e-9 * (1.0 + origin[1].abs())
        {
            return Err(SolidError::Profile("loop is not closed"));
        }
    }
    if out.area > 0.0 {
        Ok(out)
    } else {
        Err(SolidError::Profile("area is not positive"))
    }
}

/// The planar integrals, computed about the first loop's start (the boundary integrals
/// cancel badly for a small region far from the origin) and moved back: with the region
/// shifted by `o`, `Q' = Q + A o`, `I_xx' = I_xx + 2 o_x Q_x + A o_x^2`, and so on.
fn planar(loops: &[Vec<Seg>]) -> Result<Planar, SolidError> {
    let o = loops
        .first()
        .and_then(|l| l.first())
        .map_or([0.0; 2], Seg::start);
    let sh = |p: [f64; 2]| [p[0] - o[0], p[1] - o[1]];
    let shifted: Vec<Vec<Seg>> = loops
        .iter()
        .map(|l| {
            l.iter()
                .map(|s| match *s {
                    Seg::Line { start, end } => Seg::Line {
                        start: sh(start),
                        end: sh(end),
                    },
                    Seg::Arc {
                        centre,
                        start,
                        end,
                        ccw,
                    } => Seg::Arc {
                        centre: sh(centre),
                        start: sh(start),
                        end: sh(end),
                        ccw,
                    },
                })
                .collect()
        })
        .collect();
    let p = planar_raw(&shifted)?;
    Ok(Planar {
        area: p.area,
        qx: p.qx + p.area * o[0],
        qy: p.qy + p.area * o[1],
        xx: p.xx + 2.0 * o[0] * p.qx + p.area * o[0] * o[0],
        xy: p.xy + o[0] * p.qy + o[1] * p.qx + p.area * o[0] * o[1],
        yy: p.yy + 2.0 * o[1] * p.qy + p.area * o[1] * o[1],
        length: p.length,
    })
}

/// The planar area of a region bounded by `loops` (outer counter-clockwise, holes clockwise),
/// by Green's theorem.
pub fn profile_area(loops: &[Vec<Seg>]) -> Result<f64, SolidError> {
    Ok(planar(loops)?.area)
}

/// The prism of `loops` from `z = 0` to `z = h`: `V = A h`, `F = (Q_x h, Q_y h, A h^2 / 2)`,
/// `S_xx = I_xx h`, `S_zz = A h^3 / 3`, `S_xz = Q_x h^2 / 2`, ...; the area is `2 A + P h`
/// with `P` the length of all the loops.
pub fn extrusion(loops: &[Vec<Seg>], h: f64) -> Result<Solid, SolidError> {
    let h = positive(h, "h")?;
    let p = planar(loops)?;
    let second = [
        [p.xx * h, p.xy * h, p.qx * h * h / 2.0],
        [p.xy * h, p.yy * h, p.qy * h * h / 2.0],
        [
            p.qx * h * h / 2.0,
            p.qy * h * h / 2.0,
            p.area * h * h * h / 3.0,
        ],
    ];
    Ok(Solid {
        moments: Moments {
            volume: p.area * h,
            first: [p.qx * h, p.qy * h, p.area * h * h / 2.0],
            second,
        },
        area: 2.0 * p.area + p.length * h,
    })
}

// ---------------------------------------------------------------------------------------
// Boolean recipes

/// A through-hole of a [`plate`]: a cylinder along `z` at `(x, y)` of radius `r`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hole {
    /// Axis position.
    pub x: f64,
    /// Axis position.
    pub y: f64,
    /// Radius.
    pub r: f64,
}

/// A block `dx` x `dy` x `dz` (corner on the origin) minus cylinders through its `z` extent.
/// The holes must lie inside the plate and apart (a hole touching a wall or another is
/// [`SolidError::Apart`]). `V = dx dy dz - dz sum pi r^2`; `A = 2 (dx dy - sum pi r^2) +
/// 2 (dx + dy) dz + dz sum 2 pi r`. (Research 72 §2.4 prints `plate_12` as `100000 - 5760 pi`;
/// twelve holes of radius 2 through 10 are `12 * pi * 4 * 10 = 480 pi`.)
pub fn plate(dx: f64, dy: f64, dz: f64, holes: &[Hole]) -> Result<Solid, SolidError> {
    let b = block(dx, dy, dz)?;
    let mut moments = b.moments;
    let mut area = b.area;
    for (i, h) in holes.iter().enumerate() {
        let r = positive(h.r, "hole r")?;
        finite(h.x, "hole x")?;
        finite(h.y, "hole y")?;
        if !(h.x - r > 0.0 && h.x + r < dx && h.y - r > 0.0 && h.y + r < dy) {
            return Err(SolidError::Apart);
        }
        for o in &holes[..i] {
            let d = (h.x - o.x) * (h.x - o.x) + (h.y - o.y) * (h.y - o.y);
            if d.sqrt() <= h.r + o.r {
                return Err(SolidError::Apart);
            }
        }
        let c = cylinder(r, dz)?;
        moments = moments.minus(&c.moments.translated([h.x, h.y, 0.0]));
        // the two end faces lose a disk each; the bore adds the cylinder's side
        area += 2.0 * PI * r * dz - 2.0 * PI * r * r;
    }
    Ok(Solid { moments, area })
}

/// The gear recipe of research 68 / 72 (`gear_60`): a cylinder of radius `radius` and height
/// `h` on `z = 0`, less `count` slots. Slot `i` is the box `x >= root`, `|y| <= gap / 2`
/// turned about `z` by `phase + 2 pi i / count`, cut to the disk (the pin's boxes pass
/// through the whole height and out of the rim, so the slot is the part of the box inside the
/// cylinder).
///
/// One slot's cross-section is the region `root <= x <= sqrt(radius^2 - y^2)` with
/// `|y| <= g / 2`, of area
///
/// `I = [y sqrt(R^2 - y^2) / 2 + R^2 asin(y / R) / 2]` from `-g/2` to `g/2`, less `root g`,
///
/// so `V = h (pi R^2 - count I)`. The slot's moments come from the same region as a loop of
/// lines and one arc ([`extrusion`]). The area is `2 (pi R^2 - count I)` for the two ends,
/// `h (2 pi R - count 2 R theta)` for the rim with `theta = asin(g / 2R)`, and per slot a
/// floor `g h` and two walls of `h (sqrt(R^2 - g^2 / 4) - root)` each.
pub fn gear(
    radius: f64,
    h: f64,
    count: u32,
    root: f64,
    gap: f64,
    phase: f64,
) -> Result<Solid, SolidError> {
    let (radius, h, gap) = (
        positive(radius, "radius")?,
        positive(h, "h")?,
        positive(gap, "gap")?,
    );
    let root = positive(root, "root")?;
    finite(phase, "phase")?;
    if count == 0 {
        return Err(SolidError::Range("count"));
    }
    let half = gap / 2.0;
    if !(half < root && root < radius) {
        return Err(SolidError::Range("root"));
    }
    let rim_x = (radius * radius - half * half).sqrt();
    if rim_x <= root {
        return Err(SolidError::Range("gap"));
    }
    // Neighbouring slots apart: the angular half-width of a slot, widest at its root corner,
    // must fit the pitch.
    let pitch = TAU / f64::from(count);
    if count > 1 && 2.0 * fmath::atan2(half, root) >= pitch {
        return Err(SolidError::Apart);
    }
    let slot = vec![
        Seg::Line {
            start: [root, -half],
            end: [rim_x, -half],
        },
        Seg::Arc {
            centre: [0.0, 0.0],
            start: [rim_x, -half],
            end: [rim_x, half],
            ccw: true,
        },
        Seg::Line {
            start: [rim_x, half],
            end: [root, half],
        },
        Seg::Line {
            start: [root, half],
            end: [root, -half],
        },
    ];
    let slot = extrusion(&[slot], h)?;
    let mut moments = cylinder(radius, h)?.moments;
    for i in 0..count {
        let a = phase + pitch * f64::from(i);
        let (s, c) = fmath::sin_cos(a);
        let frame = Frame {
            origin: [0.0; 3],
            x: [c, s, 0.0],
            y: [-s, c, 0.0],
            z: [0.0, 0.0, 1.0],
        };
        moments = moments.minus(&slot.moments.placed(&frame));
    }
    let n = f64::from(count);
    let slot_area = (half * rim_x + radius * radius * fmath::asin(half / radius)) - root * gap;
    let theta = fmath::asin(half / radius);
    let area = 2.0 * (PI * radius * radius - n * slot_area)
        + h * (TAU * radius - n * 2.0 * radius * theta)
        + n * (gap * h + 2.0 * h * (rim_x - root));
    Ok(Solid { moments, area })
}
