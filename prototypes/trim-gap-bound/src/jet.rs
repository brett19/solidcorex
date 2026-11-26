//! Jets: a value with its first three derivatives along one parameter (the third only when
//! switched on, so a second-order bound pays nothing for it).
//!
//! Every operation applies the chain rule exactly, so a jet whose scalars are intervals
//! encloses the value and the derivatives of the composed function at every parameter the
//! input interval holds (each identity below holds for the exact numbers, and interval
//! arithmetic encloses each identity's result). A jet over `f64` gives the same numbers
//! rounded, for sampling.

use core::cell::Cell;
use core::ops::{Add, Div, Mul, Sub};

use scx_num::ring::{Ring, TayScalar};

thread_local! {
    static THIRD: Cell<bool> = const { Cell::new(false) };
}

/// Whether jets carry the third derivative (off: it stays zero and costs nothing).
pub fn third() -> bool {
    THIRD.with(Cell::get)
}

/// Turns the third derivative on or off for the jets computed on this thread after it.
pub fn set_third(on: bool) {
    THIRD.with(|c| c.set(on));
}

/// `v` and its first three derivatives along `t` (the third only when [`third`] is on).
#[derive(Clone, Copy, Debug)]
pub struct Jet<T> {
    /// The value.
    pub v: T,
    /// The first derivative.
    pub d: T,
    /// The second derivative.
    pub dd: T,
    /// The third derivative.
    pub ddd: T,
}

impl<T: Ring> Jet<T> {
    /// A constant.
    pub fn cst(x: f64) -> Self {
        Self::of(T::lift(x))
    }

    /// A constant scalar (every derivative zero).
    pub fn of(v: T) -> Self {
        let z = T::lift(0.0);
        Self { v, d: z, dd: z, ddd: z }
    }

    /// The parameter itself (derivative one).
    pub fn var(v: T) -> Self {
        let z = T::lift(0.0);
        Self { v, d: T::lift(1.0), dd: z, ddd: z }
    }

    /// Times the constant `k`.
    #[must_use]
    pub fn scale(self, k: f64) -> Self {
        let k = T::lift(k);
        let ddd = if third() { self.ddd * k } else { self.ddd };
        Self { v: self.v * k, d: self.d * k, dd: self.dd * k, ddd }
    }
}

impl<T: Ring> Add for Jet<T> {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        let ddd = if third() { self.ddd + b.ddd } else { self.ddd };
        Self { v: self.v + b.v, d: self.d + b.d, dd: self.dd + b.dd, ddd }
    }
}

impl<T: Ring> Sub for Jet<T> {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        let ddd = if third() { self.ddd - b.ddd } else { self.ddd };
        Self { v: self.v - b.v, d: self.d - b.d, dd: self.dd - b.dd, ddd }
    }
}

impl<T: Ring> Mul for Jet<T> {
    type Output = Self;
    /// `(ab)'' = a''b + 2a'b' + ab''`; `(ab)''' = a'''b + 3(a''b' + a'b'') + ab'''`.
    fn mul(self, b: Self) -> Self {
        let cross = self.d * b.d;
        let ddd = if third() {
            self.ddd * b.v + T::lift(3.0) * (self.dd * b.d + self.d * b.dd) + self.v * b.ddd
        } else {
            self.ddd
        };
        Self {
            v: self.v * b.v,
            d: self.d * b.v + self.v * b.d,
            dd: self.dd * b.v + cross + cross + self.v * b.dd,
            ddd,
        }
    }
}

impl<T: Ring> Div for Jet<T> {
    type Output = Self;
    /// From `a = q b`: `q' = (a' - q b') / b`, `q'' = (a'' - 2 q' b' - q b'') / b`,
    /// `q''' = (a''' - 3(q'' b' + q' b'') - q b''') / b`.
    fn div(self, b: Self) -> Self {
        let v = self.v / b.v;
        let d = (self.d - v * b.d) / b.v;
        let two_db = d * b.d;
        let dd = (self.dd - two_db - two_db - v * b.dd) / b.v;
        let ddd = if third() {
            (self.ddd - T::lift(3.0) * (dd * b.d + d * b.dd) - v * b.ddd) / b.v
        } else {
            self.ddd
        };
        Self { v, d, dd, ddd }
    }
}

impl<T: TayScalar> Jet<T> {
    /// From `a = r^2`: `r' = a' / (2r)`, `r'' = (a'' - 2 r'^2) / (2r)`,
    /// `r''' = (a''' - 6 r' r'') / (2r)`.
    #[must_use]
    pub fn sqrt(self) -> Self {
        let v = self.v.sqrt_s();
        let two_v = v + v;
        let d = self.d / two_v;
        let dd = (self.dd - d * d - d * d) / two_v;
        let ddd = if third() { (self.ddd - T::lift(6.0) * d * dd) / two_v } else { self.ddd };
        Self { v, d, dd, ddd }
    }

    /// `(sin, cos)` of the jet: with `s, c` the sine and cosine of the value,
    /// `sin''' = c a''' - 3 s a' a'' - c a'^3` and `cos''' = s a'^3 - 3 c a' a'' - s a'''`.
    pub fn sin_cos(self) -> (Self, Self) {
        let (s, c) = self.v.sin_cos_s();
        let d2 = self.d * self.d;
        let zero = T::lift(0.0);
        let (ds, dc) = if third() {
            let (d3, dd1) = (d2 * self.d, T::lift(3.0) * self.d * self.dd);
            (c * self.ddd - s * dd1 - c * d3, s * d3 - c * dd1 - s * self.ddd)
        } else {
            (zero, zero)
        };
        (
            Self { v: s, d: c * self.d, dd: c * self.dd - s * d2, ddd: ds },
            Self { v: c, d: zero - s * self.d, dd: zero - c * d2 - s * self.dd, ddd: dc },
        )
    }
}

/// A point or vector in space whose coordinates are jets.
pub type V3<T> = [Jet<T>; 3];

/// `a + b`.
pub fn add3<T: Ring>(a: V3<T>, b: V3<T>) -> V3<T> {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `a - b`.
pub fn sub3<T: Ring>(a: V3<T>, b: V3<T>) -> V3<T> {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `k a` for a scalar jet `k`.
pub fn mul3<T: Ring>(k: Jet<T>, a: V3<T>) -> V3<T> {
    [k * a[0], k * a[1], k * a[2]]
}

/// `a × b`.
pub fn cross3<T: Ring>(a: V3<T>, b: V3<T>) -> V3<T> {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// `a · b`.
pub fn dot3<T: Ring>(a: V3<T>, b: V3<T>) -> Jet<T> {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl<T: Ring> Ring for Jet<T> {
    fn lift(d: f64) -> Self {
        Self::cst(d)
    }
}

/// Jets over jets: the outer derivatives along one parameter, the inner along another, so
/// every mixed partial up to the third order is carried.
impl<T: TayScalar> TayScalar for Jet<T> {
    fn sqrt_s(self) -> Self {
        self.sqrt()
    }
    /// Not used by the study: every component is NaN (no test passes it).
    fn acos_s(self) -> Self {
        Self::cst(f64::NAN)
    }
    fn sin_cos_s(self) -> (Self, Self) {
        self.sin_cos()
    }
}
