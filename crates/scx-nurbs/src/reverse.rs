// Written for solidcorex. Apache-2.0.
//! Reversal: the same curve run the other way, with the parameter negated.
//!
//! The pin's `ReverseParameterization` reflects the knots about the domain (`a + b - t`),
//! which rounds each knot and can merge a knot with a neighbour. Negating them is exact, so
//! the reversal is total and a second reversal gives the curve back bit for bit (research 63
//! §3.9).

use crate::NurbsCurve;

impl<const D: usize> NurbsCurve<D> {
    /// The same curve run the other way, `C'(t) = C(-t)`, on the negated domain
    /// `[-b, -a]`: the knots negated in reverse order ([`KnotVector::negated`](crate::KnotVector::negated))
    /// and the control points, weights and homogeneous points reversed, each kept exactly.
    ///
    /// A side swaps with the parameter: `C'` at `-t` from [`Approach::Above`](crate::Approach::Above)
    /// is `C` at `t` from below, with the odd derivatives negated. The values agree to
    /// rounding (the basis sums run in the other order), not bit for bit.
    #[must_use]
    pub fn reversed(&self) -> Self {
        let rational = self
            .weights()
            .zip(self.weighted_points())
            .map(|(w, wp)| (backwards(w), backwards(wp)));
        Self::from_valid_parts(
            self.knot_vector().negated(),
            backwards(self.points()),
            rational,
        )
    }
}

fn backwards<T: Copy>(v: &[T]) -> Vec<T> {
    v.iter().rev().copied().collect()
}

#[cfg(test)]
mod tests {
    use crate::{Approach, KnotVector, NurbsCurve};

    fn curve(rational: bool) -> NurbsCurve<3> {
        let knots = vec![-1.0, -1.0, -1.0, -1.0, 0.25, 0.25, 1.5, 3.0, 3.0, 3.0, 3.0];
        let points = vec![
            [0.0, 0.0, 0.0],
            [1.0, 2.0, -1.0],
            [2.0, -1.0, 0.5],
            [3.0, 0.5, 2.0],
            [4.0, 3.0, -2.0],
            [5.0, 1.0, 1.0],
            [6.0, -2.0, 0.0],
        ];
        let weights = rational.then(|| vec![1.0, 0.5, 2.0, 0.75, 1.25, 3.0, 1.0]);
        NurbsCurve::new(KnotVector::new(3, knots).unwrap(), points, weights).unwrap()
    }

    #[test]
    fn reversed_runs_back_over_the_negated_domain() {
        for rational in [false, true] {
            let c = curve(rational);
            let r = c.reversed();
            assert_eq!(r.domain(), c.domain().negated());
            assert_eq!(
                r.reversed(),
                c,
                "a second reversal is the curve, bit for bit"
            );
            for i in 0..=64 {
                let t = -1.0 + 4.0 * f64::from(i) / 64.0;
                for (side, other) in [
                    (Approach::Above, Approach::Below),
                    (Approach::Below, Approach::Above),
                ] {
                    let [p, d1, d2] = c.derivs::<3>(t, side).unwrap();
                    let [q, e1, e2] = r.derivs::<3>(-t, other).unwrap();
                    for k in 0..3 {
                        let tol = 1e-13 * (1.0 + p[k].abs() + d1[k].abs() + d2[k].abs());
                        assert!((p[k] - q[k]).abs() <= tol, "{rational} {t} point");
                        assert!((d1[k] + e1[k]).abs() <= 8.0 * tol, "{rational} {t} d1");
                        assert!((d2[k] - e2[k]).abs() <= 64.0 * tol, "{rational} {t} d2");
                    }
                }
            }
        }
    }
}
