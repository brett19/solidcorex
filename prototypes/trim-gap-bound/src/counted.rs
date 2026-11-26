//! An interval scalar that counts its operations, so the cost of a bound can be reported in
//! interval operations (one per `+ - * /`, square root, and sine-cosine pair).

use core::cell::Cell;
use core::ops::{Add, Div, Mul, Sub};

use scx_num::interval::Ivl;
use scx_num::ring::{Ring, TayScalar};

thread_local! {
    static OPS: Cell<u64> = const { Cell::new(0) };
}

fn tick() {
    OPS.with(|c| c.set(c.get() + 1));
}

/// The interval operations counted on this thread so far.
pub fn ops() -> u64 {
    OPS.with(Cell::get)
}

/// An [`Ivl`] whose operations are counted.
#[derive(Clone, Copy, Debug)]
pub struct Ci(pub Ivl);

impl Ci {
    /// The point interval `[x, x]`.
    pub fn point(x: f64) -> Self {
        Self(Ivl::from(x))
    }

    /// `[lo, hi]`.
    pub fn span(lo: f64, hi: f64) -> Self {
        Self(Ivl::new(lo, hi))
    }

    /// The smallest interval holding both.
    #[must_use]
    pub fn hull(self, b: Self) -> Self {
        Self(self.0.hull(b.0))
    }
}

impl Add for Ci {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        tick();
        Self(self.0 + b.0)
    }
}

impl Sub for Ci {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        tick();
        Self(self.0 - b.0)
    }
}

impl Mul for Ci {
    type Output = Self;
    fn mul(self, b: Self) -> Self {
        tick();
        Self(self.0 * b.0)
    }
}

impl Div for Ci {
    type Output = Self;
    fn div(self, b: Self) -> Self {
        tick();
        Self(self.0 / b.0)
    }
}

impl Ring for Ci {
    fn lift(d: f64) -> Self {
        Self::point(d)
    }
}

impl TayScalar for Ci {
    fn sqrt_s(self) -> Self {
        tick();
        Self(self.0.sqrt())
    }
    fn acos_s(self) -> Self {
        tick();
        Self(self.0.acos_s())
    }
    fn sin_cos_s(self) -> (Self, Self) {
        tick();
        let (s, c) = self.0.sin_cos_s();
        (Self(s), Self(c))
    }
}
