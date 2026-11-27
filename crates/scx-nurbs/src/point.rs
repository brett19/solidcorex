// Written for solidcorex. Apache-2.0.
//! The control point arithmetic the curve algorithms share, for polynomial points
//! (`[f64; D]`) and homogeneous ones ([`Weighted`]).
//!
//! The combinations follow solidcore's `CPoint::combine2` and `CPoint::affine2` at the pin
//! (`nurbs::math`): `alpha a + beta b` per coordinate, and for an affine combination a
//! coordinate equal in both points is kept exactly, so a straight edge or a plane stays
//! exactly straight or plane through insertion and splitting. (Degree elevation sums
//! Bezier elevation products, as the pin does, and does not keep them exactly.)

/// A control point the curve algorithms combine.
pub(crate) trait Control: Copy {
    /// The origin.
    const ZERO: Self;

    /// `alpha a + beta b`.
    fn combine(alpha: f64, a: Self, beta: f64, b: Self) -> Self;

    /// `alpha a + (1 - alpha) b`, with every coordinate that is equal in `a` and `b` kept
    /// exactly.
    fn affine(alpha: f64, a: Self, b: Self) -> Self;

    /// `self + alpha a`.
    #[must_use]
    fn blend(self, alpha: f64, a: Self) -> Self;
}

/// `alpha a + (1 - alpha) b` for one coordinate, `a` where `a == b`.
fn affine1(alpha: f64, beta: f64, a: f64, b: f64) -> f64 {
    let r = alpha * a + beta * b;
    if a == b && alpha.is_finite() {
        a
    } else {
        r
    }
}

impl<const D: usize> Control for [f64; D] {
    const ZERO: Self = [0.0; D];

    fn combine(alpha: f64, a: Self, beta: f64, b: Self) -> Self {
        let mut r = a;
        for (r, &b) in r.iter_mut().zip(&b) {
            *r = alpha * *r + beta * b;
        }
        r
    }

    fn affine(alpha: f64, a: Self, b: Self) -> Self {
        let beta = 1.0 - alpha;
        let mut r = a;
        for (r, &b) in r.iter_mut().zip(&b) {
            *r = affine1(alpha, beta, *r, b);
        }
        r
    }

    fn blend(self, alpha: f64, a: Self) -> Self {
        let mut r = self;
        for (r, &a) in r.iter_mut().zip(&a) {
            *r += alpha * a;
        }
        r
    }
}

/// A homogeneous control point `(w P, w)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Weighted<const D: usize> {
    /// `w P`.
    pub(crate) point: [f64; D],
    /// `w`.
    pub(crate) weight: f64,
}

impl<const D: usize> Control for Weighted<D> {
    const ZERO: Self = Self {
        point: [0.0; D],
        weight: 0.0,
    };

    fn combine(alpha: f64, a: Self, beta: f64, b: Self) -> Self {
        Self {
            point: Control::combine(alpha, a.point, beta, b.point),
            weight: alpha * a.weight + beta * b.weight,
        }
    }

    fn affine(alpha: f64, a: Self, b: Self) -> Self {
        Self {
            point: Control::affine(alpha, a.point, b.point),
            weight: affine1(alpha, 1.0 - alpha, a.weight, b.weight),
        }
    }

    fn blend(self, alpha: f64, a: Self) -> Self {
        Self {
            point: self.point.blend(alpha, a.point),
            weight: self.weight + alpha * a.weight,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_keeps_equal_coordinates() {
        let a = [0.1, 3.0];
        let b = [0.7, 3.0];
        let r = <[f64; 2]>::affine(0.3, a, b);
        assert_eq!(r[1], 3.0);
        assert_eq!(r[0], 0.3 * 0.1 + (1.0 - 0.3) * 0.7);
        let w = Weighted::affine(
            0.3,
            Weighted {
                point: a,
                weight: 2.0,
            },
            Weighted {
                point: b,
                weight: 2.0,
            },
        );
        assert_eq!(w.weight, 2.0);
        assert_eq!(<[f64; 1]>::ZERO.blend(2.0, [3.0]), [6.0]);
    }
}
