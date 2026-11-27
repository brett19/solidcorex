// Written for solidcorex. Apache-2.0.
//! Where an integrand stops being smooth: a surface's break lines (an analytic kind's arc
//! joins, a NURBS surface's knots) and a trim's own knots, and where a trim crosses a break
//! line. The integrals are split there, as the pin splits them, so no Kronrod piece spans a
//! kink; a missed split costs only bisections, never accuracy.

use scx_curve::arcs::Arcs;
use scx_curve::{Curve, Curve2};
use scx_nurbs_surface::ParamAxis;
use scx_surface::Surface;

/// A surface's parameter direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dir {
    U,
    V,
}

/// The break lines of `s` across `dir` strictly inside `(lo, hi)`, in order.
pub(crate) fn surface_breaks(s: &Surface, dir: Dir, lo: f64, hi: f64) -> Vec<f64> {
    let domain = s.domain();
    let arcs = match (s, dir) {
        (
            Surface::Cylinder(_) | Surface::Cone(_) | Surface::Sphere(_) | Surface::Torus(_),
            Dir::U,
        ) => domain.u.period().map(Arcs::turn),
        (Surface::Sphere(_), Dir::V) => Some(Arcs::half()),
        (Surface::Torus(_), Dir::V) => domain.v.period().map(Arcs::turn),
        (Surface::Nurbs(n), _) => {
            let axis = match dir {
                Dir::U => ParamAxis::U,
                Dir::V => ParamAxis::V,
            };
            return inside(n.knots(axis), lo, hi);
        }
        _ => None,
    };
    arcs.and_then(|a| a.breaks(lo, hi)).unwrap_or_default()
}

/// A trim's own breaks strictly inside `(lo, hi)`: a NURBS curve's distinct knots. An angle
/// curve's arc joins lie on its surface's break lines and are found as crossings.
pub(crate) fn curve_breaks(c: &Curve2, lo: f64, hi: f64) -> Vec<f64> {
    match c {
        Curve2::Nurbs(n) => inside(n.knots(), lo, hi),
        // Its smoothness is its 3D curve's.
        Curve2::OnChart(o) => match o.curve() {
            Curve::Nurbs(n) => inside(n.knots(), lo, hi),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The distinct values of the sorted `knots` strictly inside `(lo, hi)`.
fn inside(knots: &[f64], lo: f64, hi: f64) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    for &k in knots {
        if lo < k && k < hi && out.last().is_none_or(|&l| l < k) {
            out.push(k);
        }
    }
    out
}

/// The parameters in `(lo, hi)` where `coord(t)` crosses one of `lines`, from a sign change
/// between `samples` (sorted, spanning `[lo, hi]`, each `(t, coord)`), bisected to the
/// last bit. A tangent touch shows no sign change and is not split.
pub(crate) fn crossings(
    samples: &[(f64, f64)],
    lines: &[f64],
    mut coord: impl FnMut(f64) -> f64,
) -> Vec<f64> {
    let mut out = Vec::new();
    for pair in samples.windows(2) {
        let [(t0, c0), (t1, c1)] = pair else {
            continue;
        };
        for &b in lines {
            let (f0, f1) = (c0 - b, c1 - b);
            if (f0 * f1).partial_cmp(&0.0) != Some(std::cmp::Ordering::Less) {
                continue;
            }
            let (mut a, mut z, fa) = (*t0, *t1, f0);
            for _ in 0..80 {
                let m = 0.5 * (a + z);
                if !(a < m && m < z) {
                    break;
                }
                if (coord(m) - b) * fa > 0.0 {
                    a = m;
                } else {
                    z = m;
                }
            }
            out.push(0.5 * (a + z));
        }
    }
    out.sort_by(f64::total_cmp);
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knots_inside_are_distinct() {
        assert_eq!(
            inside(&[0.0, 0.0, 0.5, 0.5, 1.0, 2.0, 2.0], 0.0, 2.0),
            vec![0.5, 1.0]
        );
    }

    #[test]
    fn a_crossing_is_bisected() {
        let samples: Vec<(f64, f64)> = (0..=4)
            .map(|i| (f64::from(i), f64::from(i) * 0.3))
            .collect();
        let t = crossings(&samples, &[0.45, 2.0], |t| t * 0.3);
        assert_eq!(t.len(), 1);
        assert!((t[0] - 1.5).abs() < 1e-15);
    }
}
