// Written for solidcorex. Apache-2.0.
//! What the kinds share: the placement in effective axes, the separable form of a surface of
//! revolution, and its jet.
//!
//! Cylinder, cone, sphere and torus are `S(u, v) = O + f(v) D(u) + g(v) Z`, where
//! `D(u) = cos phi(u) X + sin phi(u) Y` and `Z` is the frame's z times the kind's sense
//! (research 63 D14: a mirror keeps the frame right-handed and reverses the sense, so the uv
//! map is unchanged). Then `d^(i+j) S / du^i dv^j = f^(j)(v) D^(i)(u)`, plus `g^(j)(v) Z`
//! when `i = 0`.

use scx_num::frame::{Frame, Sense};
use scx_num::ring::Ring;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};

use crate::arcs::Trig;
use scx_num::jet::SurfaceJet;

/// A frame's origin and axes, z times the sense.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Axes {
    pub(crate) o: Point3,
    pub(crate) x: Vec3,
    pub(crate) y: Vec3,
    pub(crate) z: Vec3,
}

impl Axes {
    pub(crate) fn new(frame: &Frame, sense: Sense) -> Self {
        Self {
            o: frame.origin(),
            x: *frame.x(),
            y: *frame.y(),
            z: *frame.z() * sense.sign(),
        }
    }

    /// `p`'s coordinates along `X`, `Y` and `Z`: the radial pair and the height.
    pub(crate) fn local(&self, p: Point3) -> (f64, f64, f64) {
        let d = p - self.o;
        (d.dot(self.x), d.dot(self.y), d.dot(self.z))
    }

    /// `cos X + sin Y`.
    pub(crate) fn radial(&self, (c, s): (f64, f64)) -> Vec3 {
        self.x * c + self.y * s
    }

    /// `O + f (c X + s Y) + g Z`: [`Self::at_in`] in `f64`, so `point` and `eval` agree to the
    /// bit.
    pub(crate) fn at(&self, cs: (f64, f64), f: f64, g: f64) -> Point3 {
        self.at_in(cs, f, g)
    }

    /// `O + f (c X + s Y) + g Z` in any ring.
    pub(crate) fn at_in<T: Ring>(&self, (c, s): (T, T), f: T, g: T) -> Point3<T> {
        let l = T::lift;
        let fc = f * c;
        let fs = f * s;
        Point3::new(
            l(self.o.x) + fc * l(self.x.x) + fs * l(self.y.x) + g * l(self.z.x),
            l(self.o.y) + fc * l(self.x.y) + fs * l(self.y.y) + g * l(self.z.y),
            l(self.o.z) + fc * l(self.x.z) + fs * l(self.y.z) + g * l(self.z.z),
        )
    }

    /// The jet of `O + f(v) D(u) + g(v) Z`, with `fg(j) = (f^(j)(v), g^(j)(v))`.
    pub(crate) fn jet<const D: usize>(
        &self,
        u: &Trig<D>,
        fg: impl Fn(usize) -> (f64, f64),
    ) -> SurfaceJet<Vec3, D> {
        SurfaceJet::from_fn(|i, j| {
            let (f, g) = fg(j);
            let mut d = self.radial(u.nth(i)) * f;
            if i == 0 {
                d += self.z * g;
            }
            if i == 0 && j == 0 {
                d += self.o.to_vec();
            }
            d
        })
    }
}

/// A combination of orthonormal axes with unit coefficients, unit within rounding.
pub(crate) fn unit(v: Vec3, fallback: UnitVec3) -> UnitVec3 {
    // `normalized` is `Some` for every vector of length near 1; the fallback is unreachable.
    v.normalized().unwrap_or(fallback)
}

/// The derivative `n` of `x` itself in `x`: `x`, `1`, then `0`.
pub(crate) fn linear(x: f64, n: usize) -> f64 {
    match n {
        0 => x,
        1 => 1.0,
        _ => 0.0,
    }
}

/// `eval_many`'s one check: as many outputs as parameters.
pub(crate) fn check_lengths(uv: &[scx_num::vector2d::Point2], out: &[Point3]) {
    assert_eq!(
        uv.len(),
        out.len(),
        "eval_many: uv and out differ in length"
    );
}
