// Written for solidcorex. Apache-2.0.
//! Edits by value (04, 11-27): [`Curve::restrict`], [`Curve::reversed`] and
//! [`Curve::transformed`], with [`Curve2`]'s `restrict` and `reversed` (research 63 §3.9).
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `Trim` and `Copy` of a bounded line or a conic arc | none: a line and a conic carry no extent (an edge holds its interval), so a restriction is the curve itself |
//! | `Trim` and `Copy` of a NURBS curve | [`Curve::restrict`]: [`NurbsCurve::split`] at each end inside the domain |
//! | `ReverseParameterization`: a bounded line's `a + b - t`, a conic's y axis flipped and its angles mirrored, a NURBS curve's knots reflected about the domain | [`Curve::reversed`]: `C'(t) = C(-t)` for every kind, exact |
//! | `Transform`, a conic under a uniform scale only | [`Curve::transformed`]: any affine map, a circle to an ellipse where it is not a similarity, with the angle's [`ParamShift`] |
//!
//! The plane curves have no `transformed`: there is no plane affine map in `scx-num`, and
//! a body's transform keeps its pcurves (63 D14).

use std::borrow::Cow;
use std::sync::Arc;

use scx_num::extent1d::Extent1d;
use scx_num::math::Affine;
use scx_num::vector3d::Point3;
use scx_nurbs::NurbsCurve;

use crate::curve::{Curve, Curve2};
use crate::error::{CurveError, Degenerate, Part};

/// How a transformed curve's parameter relates to the original's: the image of the point
/// at `t` is at `t + by` on the image.
///
/// Zero for every kind but a conic under a map that is not a similarity, whose image is
/// placed on its principal axes, so its angle starts elsewhere (63 §3.2). An edge's
/// interval moves with it ([`ParamShift::extent`]).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ParamShift {
    by: f64,
}

impl ParamShift {
    /// No shift.
    pub const NONE: Self = Self { by: 0.0 };

    pub(crate) const fn new(by: f64) -> Self {
        Self { by }
    }

    /// The amount added to the parameter.
    #[must_use]
    pub const fn by(self) -> f64 {
        self.by
    }

    /// The image's parameter of the point at `t`: `t + by`, rounded once.
    #[must_use]
    pub fn apply(self, t: f64) -> f64 {
        t + self.by
    }

    /// An interval on the original moved onto the image; `None` where an end overflows.
    #[must_use]
    pub fn extent(self, e: Extent1d) -> Option<Extent1d> {
        e.translated(self.by)
    }
}

/// The NURBS curve over `e`, split at each end of `e` inside its domain: the same
/// parameter, the curve's own [`NurbsCurve::split`].
fn restrict_nurbs<const N: usize>(
    c: &Arc<NurbsCurve<N>>,
    e: Extent1d,
) -> crate::Result<Arc<NurbsCurve<N>>> {
    let domain = c.domain();
    if e.min() < domain.min() || e.max() > domain.max() {
        return Err(CurveError::OutsideDomain { extent: e, domain });
    }
    if e.min() == e.max() {
        return Err(CurveError::Degenerate(Degenerate::Extent { at: e.min() }));
    }
    if e == domain {
        return Ok(Arc::clone(c));
    }
    let above: Cow<'_, NurbsCurve<N>> = if e.min() > domain.min() {
        Cow::Owned(c.split(e.min())?.1)
    } else {
        Cow::Borrowed(c)
    };
    let piece = if e.max() < domain.max() {
        above.split(e.max())?.0
    } else {
        above.into_owned()
    };
    Ok(Arc::new(piece))
}

/// The NURBS curve mapped by `a`: each control point mapped, the knots and weights kept,
/// which is the image exactly up to one rounding of each point (a NURBS curve is an
/// affine combination of its points).
fn transform_nurbs(c: &NurbsCurve<3>, a: &Affine) -> crate::Result<NurbsCurve<3>> {
    if !a.is_finite() {
        return Err(CurveError::NotFinite(Part::Map));
    }
    let points = c
        .points()
        .iter()
        .map(|&p| a.apply(Point3::from(p)).to_array())
        .collect();
    let weights = c.weights().map(<[f64]>::to_vec);
    Ok(NurbsCurve::new(c.knot_vector().clone(), points, weights)?)
}

impl Curve {
    /// The curve over `e`, on the same parameter.
    ///
    /// A line and a conic carry no extent (an edge holds its interval, 63 §3.2), so they
    /// come back unchanged for any `e`. A NURBS curve is split at each end of `e` inside
    /// its domain; `e` equal to the domain gives the same `Arc`.
    ///
    /// # Errors
    ///
    /// For a NURBS curve: [`CurveError::OutsideDomain`] where `e` reaches past the domain,
    /// [`Degenerate::Extent`] for an `e` of no length, and the split's own cause
    /// ([`CurveError::Nurbs`]).
    pub fn restrict(&self, e: Extent1d) -> crate::Result<Self> {
        match self {
            Self::Line(_) | Self::Circle(_) | Self::Ellipse(_) => Ok(self.clone()),
            Self::Nurbs(n) => restrict_nurbs(n, e).map(Self::Nurbs),
        }
    }

    /// The same curve run the other way, `C'(t) = C(-t)`, for every kind: a line's
    /// direction negated, a conic's sense reversed, a NURBS curve's knots negated
    /// ([`NurbsCurve::reversed`]). Total; an analytic kind's point has the original's bits,
    /// and a domain `[a, b]` becomes `[-b, -a]`, so an edge's interval is negated exactly.
    #[must_use]
    pub fn reversed(&self) -> Self {
        match self {
            Self::Line(l) => Self::Line(l.reversed()),
            Self::Circle(c) => Self::Circle(c.reversed()),
            Self::Ellipse(e) => Self::Ellipse(e.reversed()),
            Self::Nurbs(n) => Self::Nurbs(Arc::new(n.reversed())),
        }
    }

    /// The image under `a`, and the shift of its parameter: with `(c, s)` the result,
    /// `c.point(s.apply(t))` is `a(self.point(t))` up to rounding.
    ///
    /// A line maps to a line ([`Line::transformed`](crate::Line::transformed)), a NURBS
    /// curve to a NURBS curve (its control points mapped), both with no shift. A circle
    /// maps to a circle under a similarity on its plane and to an ellipse otherwise
    /// ([`Circle::transformed`](crate::Circle::transformed)), an ellipse to an ellipse;
    /// these are exact images, so no kind becomes NURBS (63 §3.2).
    ///
    /// # Errors
    ///
    /// [`CurveError::NotFinite`] for a map that is not finite or an image that overflows,
    /// a degenerate size or [`Degenerate::Plane`] where `a` flattens the curve, and a NURBS
    /// curve's cause for its mapped points ([`CurveError::Nurbs`]).
    pub fn transformed(&self, a: &Affine) -> crate::Result<(Self, ParamShift)> {
        match self {
            Self::Line(l) => Ok((l.transformed(a)?.into(), ParamShift::NONE)),
            Self::Circle(c) => c.transformed(a),
            Self::Ellipse(e) => e.transformed(a).map(|(e, s)| (e.into(), s)),
            Self::Nurbs(n) => Ok((transform_nurbs(n, a)?.into(), ParamShift::NONE)),
        }
    }
}

impl Curve2 {
    /// The curve over `e`, as [`Curve::restrict`].
    ///
    /// # Errors
    ///
    /// As [`Curve::restrict`].
    pub fn restrict(&self, e: Extent1d) -> crate::Result<Self> {
        match self {
            Self::Line(_) | Self::Circle(_) | Self::Ellipse(_) => Ok(self.clone()),
            Self::Nurbs(n) => restrict_nurbs(n, e).map(Self::Nurbs),
        }
    }

    /// The same curve run the other way, `C'(t) = C(-t)`, as [`Curve::reversed`].
    #[must_use]
    pub fn reversed(&self) -> Self {
        match self {
            Self::Line(l) => Self::Line(l.reversed()),
            Self::Circle(c) => Self::Circle(c.reversed()),
            Self::Ellipse(e) => Self::Ellipse(e.reversed()),
            Self::Nurbs(n) => Self::Nurbs(Arc::new(n.reversed())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::tests::{bits2, bits3, curves, curves2, params};
    use crate::{Circle, Ellipse, Line};
    use scx_num::frame::{Frame, Sense};
    use scx_num::hash::Rng;
    use scx_num::jet::CurveJet;
    use scx_num::units::Length;
    use scx_num::vector3d::Vec3;
    use scx_nurbs::Approach;

    const EPS: f64 = f64::EPSILON;

    const SIDES: [(Approach, Approach); 2] = [
        (Approach::Above, Approach::Below),
        (Approach::Below, Approach::Above),
    ];

    /// `x == y`, or both NaN (a derivative's zero keeps no sign here: `-0` is `0`).
    fn same(x: f64, y: f64) -> bool {
        x == y || (x.is_nan() && y.is_nan())
    }

    fn max_abs(v: [f64; 3]) -> f64 {
        v.iter().fold(0.0_f64, |m, x| m.max(x.abs()))
    }

    fn near3(got: Vec3, want: Vec3, scale: f64, rel: f64) -> bool {
        max_abs((got - want).to_array()) <= rel * scale
    }

    fn analytic(c: &Curve) -> bool {
        !matches!(c, Curve::Nurbs(_))
    }

    #[test]
    fn reversed_analytic_kinds_run_back_bit_for_bit() {
        let mut rng = Rng::new(31);
        let mut n = 0;
        for c in curves(5).iter().filter(|c| analytic(c)) {
            let r = c.reversed();
            assert_eq!(r.reversed(), *c);
            assert_eq!(r.domain(), c.domain());
            for t in params(&mut rng) {
                assert_eq!(bits3(r.point(-t)), bits3(c.point(t)), "{c:?} {t}");
                let (j, k): (CurveJet<4>, CurveJet<4>) =
                    (c.jet(t, Approach::Above), r.jet(-t, Approach::Below));
                assert_eq!(bits3(k.point), bits3(j.point));
                for (i, (d, e)) in j.d.iter().zip(&k.d).enumerate() {
                    let sign = if i % 2 == 0 { -1.0 } else { 1.0 };
                    let want = (*d * sign).to_array();
                    assert!(
                        want.iter().zip(e.to_array()).all(|(&w, g)| same(w, g)),
                        "{c:?} {t} d{}",
                        i + 1
                    );
                }
                n += 1;
            }
        }
        assert!(n > 4000, "{n}");
        for c in curves2(5).iter().filter(|c| !matches!(c, Curve2::Nurbs(_))) {
            let r = c.reversed();
            assert_eq!(r.reversed(), *c);
            for t in params(&mut rng) {
                assert_eq!(bits2(r.point(-t)), bits2(c.point(t)), "{c:?} {t}");
            }
        }
    }

    #[test]
    fn reversed_nurbs_runs_back_over_the_negated_domain() {
        let mut rng = Rng::new(32);
        let mut n = 0;
        for c in curves(6).iter().filter(|c| !analytic(c)) {
            let r = c.reversed();
            assert_eq!(r.reversed(), *c, "a second reversal gives the curve back");
            let (Some(d), Some(e)) = (bounded(c), bounded(&r)) else {
                panic!("a NURBS curve is bounded");
            };
            assert_eq!(e, d.negated());
            for _ in 0..16 {
                let t = d.min() + d.length() * rng.next_f64();
                for (side, other) in SIDES {
                    let (j, k): (CurveJet<2>, CurveJet<2>) = (c.jet(t, side), r.jet(-t, other));
                    let scale = max_abs(j.point.to_array()).max(1.0);
                    assert!(near3(k.point.to_vec(), j.point.to_vec(), scale, 1e-12));
                    let scale = max_abs(j.d[0].to_array()).max(scale);
                    assert!(near3(-k.d[0], j.d[0], scale, 1e-10), "{c:?} {t}");
                }
                n += 1;
            }
        }
        assert!(n > 400, "{n}");
        for c in curves2(6).iter().filter(|c| matches!(c, Curve2::Nurbs(_))) {
            assert_eq!(c.reversed().reversed(), *c);
        }
    }

    fn bounded(c: &Curve) -> Option<Extent1d> {
        match c.domain() {
            scx_num::domain::ParamDomain::Bounded(e) => Some(e),
            _ => None,
        }
    }

    #[test]
    fn restrict_splits_nurbs_and_keeps_the_rest() {
        let mut rng = Rng::new(33);
        for c in curves(7) {
            let Some(d) = bounded(&c) else {
                // Unbounded and periodic kinds carry no extent.
                let e = Extent1d::new(-2.0, 3.0).unwrap();
                assert_eq!(c.restrict(e).unwrap(), c);
                continue;
            };
            let Curve::Nurbs(n) = &c else { unreachable!() };
            let Curve::Nurbs(whole) = c.restrict(d).unwrap() else {
                unreachable!()
            };
            assert!(
                Arc::ptr_eq(n, &whole),
                "the domain itself is the same curve"
            );
            let at = |f: f64| d.min() + f * d.length();
            for (lo, hi) in [(0.25, 0.75), (0.0, 0.5), (0.5, 1.0), (0.1, 0.1 + 1e-9)] {
                let e = Extent1d::new(at(lo), at(hi)).unwrap();
                let r = c.restrict(e).unwrap();
                assert_eq!(bounded(&r), Some(e), "{c:?} {e:?}");
                for i in 0..=8 {
                    let t = e.min() + e.length() * f64::from(i) / 8.0;
                    let t = t.min(e.max());
                    let want = c.point(t);
                    let scale = max_abs(want.to_array()).max(1.0);
                    assert!(near3(r.point(t).to_vec(), want.to_vec(), scale, 1e-11));
                }
            }
            let past = Extent1d::new(at(0.5), at(1.0) + 1.0).unwrap();
            assert_eq!(
                c.restrict(past),
                Err(CurveError::OutsideDomain {
                    extent: past,
                    domain: d
                })
            );
            let point = Extent1d::point(at(rng.next_f64())).unwrap();
            assert_eq!(
                c.restrict(point),
                Err(CurveError::Degenerate(Degenerate::Extent {
                    at: point.min()
                }))
            );
        }
        for c in curves2(7) {
            if let Curve2::Nurbs(n) = &c {
                let d = n.domain();
                let e = Extent1d::new(d.min() + 0.25 * d.length(), d.max()).unwrap();
                assert_eq!(
                    c.restrict(e).unwrap().domain(),
                    Curve2::from(n.split(e.min()).unwrap().1).domain()
                );
            } else {
                assert_eq!(c.restrict(Extent1d::new(0.0, 1.0).unwrap()).unwrap(), c);
            }
        }
    }

    /// Random affine maps of every size, with translations, mirrors among them.
    fn maps(rng: &mut Rng) -> Vec<Affine> {
        let mut out = Vec::new();
        for scale in [1.0, 1e-3, 1e3] {
            for _ in 0..12 {
                let v = |rng: &mut Rng| {
                    Vec3::from(core::array::from_fn(|_| scale * rng.next_signed_unit()))
                };
                let (x, y, z, o) = (v(rng), v(rng), v(rng), v(rng));
                out.push(Affine::from_axes(x, y, z, o.to_point()));
            }
        }
        out
    }

    /// The bound on a mapped point: the map's largest column times the curve's reach.
    fn reach(a: &Affine, c: &Curve, t: f64) -> f64 {
        let col = [a.x_axis(), a.y_axis(), a.z_axis()]
            .map(Vec3::length)
            .into_iter()
            .fold(0.0, f64::max);
        let p = c.point(t);
        let size = match c {
            Curve::Line(l) => l.direction().length() * (1.0 + t.abs()),
            Curve::Circle(k) => k.radius().mm(),
            Curve::Ellipse(e) => e.semi_x().mm().max(e.semi_y().mm()),
            _ => 0.0,
        };
        max_abs(a.translation_part().to_array()) + 3.0 * col * (max_abs(p.to_array()) + size)
    }

    #[test]
    fn transformed_maps_every_point_and_tangent() {
        let mut rng = Rng::new(34);
        let maps = maps(&mut rng);
        let mut kinds = [0; 4];
        for c in curves(8) {
            for a in &maps {
                let (image, shift) = c.transformed(a).unwrap();
                kinds[match image {
                    Curve::Line(_) => 0,
                    Curve::Circle(_) => 1,
                    Curve::Ellipse(_) => 2,
                    _ => 3,
                }] += 1;
                if !matches!(c, Curve::Circle(_) | Curve::Ellipse(_)) {
                    assert_eq!(shift, ParamShift::NONE);
                }
                let ts: Vec<f64> = match bounded(&c) {
                    Some(d) => (0..=8)
                        .map(|i| d.min() + d.length() * f64::from(i) / 8.0)
                        .collect(),
                    None => (0..16).map(|_| 10.0 * rng.next_signed_unit()).collect(),
                };
                for t in ts {
                    let (j, k): (CurveJet<1>, CurveJet<1>) = (
                        c.jet(t, Approach::Above),
                        image.jet(shift.apply(t), Approach::Above),
                    );
                    let scale = reach(a, &c, t);
                    let want = a.apply(j.point);
                    assert!(
                        near3(k.point.to_vec(), want.to_vec(), scale, 1e-13),
                        "{c:?} {a:?} {t}: {:?} {want:?}",
                        k.point
                    );
                    let dw = a.apply_vec(j.d[0]);
                    let scale = scale.max(3.0 * max_abs(dw.to_array()));
                    assert!(
                        near3(k.d[0], dw, scale, 1e-12),
                        "{c:?} {t} {:?} {dw:?}",
                        k.d[0]
                    );
                }
            }
        }
        // Lines, circles under a random map (never a similarity) become ellipses, and NURBS.
        assert_eq!(kinds[1], 0, "{kinds:?}");
        assert!(kinds.iter().filter(|&&k| k > 0).count() == 3, "{kinds:?}");
    }

    fn unit_circle(sense: Sense) -> Circle {
        Circle::new(Frame::WORLD, Length::from_mm(2.0), sense).unwrap()
    }

    fn linear(x: [f64; 3], y: [f64; 3], z: [f64; 3]) -> Affine {
        Affine::from_axes(Vec3::from(x), Vec3::from(y), Vec3::from(z), Point3::ORIGIN)
    }

    #[test]
    fn transformed_conics_against_closed_forms() {
        for sense in [Sense::Forward, Sense::Reversed] {
            let c = unit_circle(sense);
            // A rotation and a uniform scale keep a circle, with no shift.
            let turn = Affine::rotation(Point3::new(1.0, 2.0, 3.0), Vec3::new(1.0, -2.0, 0.5), 0.7)
                .unwrap();
            let scaled = Affine::uniform_scaling(Point3::new(-1.0, 0.0, 4.0), 2.5).unwrap() * turn;
            for (a, r) in [(turn, 2.0), (scaled, 5.0)] {
                let (Curve::Circle(k), shift) = c.transformed(&a).unwrap() else {
                    panic!("a similarity keeps a circle");
                };
                assert_eq!(shift, ParamShift::NONE);
                assert!((k.radius().mm() - r).abs() <= 8.0 * f64::EPSILON * r);
                assert_eq!(k.sense(), sense);
            }
            // A mirror keeps the images of x and y and the sense: z is their normal.
            let mirror = Affine::reflection(Point3::ORIGIN, Vec3::X).unwrap();
            let (Curve::Circle(k), shift) = c.transformed(&mirror).unwrap() else {
                panic!("a mirror keeps a circle");
            };
            assert_eq!(shift, ParamShift::NONE);
            assert_eq!(k.sense(), sense);
            assert_eq!(k.frame().x().as_vec(), -Vec3::X);
            assert_eq!(k.frame().z().as_vec(), -Vec3::Z);
            for t in [0.0, 0.5, 2.0, -1.25] {
                assert!(near3(
                    k.point(t).to_vec(),
                    mirror.apply(c.point(t)).to_vec(),
                    2.0,
                    4e-16
                ));
            }
            // A scale along the frame's axes: semi-axes 3r and 2r (or 2r and 3r), no shift.
            for (sx, sy) in [(3.0, 2.0), (2.0, 3.0)] {
                let a = linear([sx, 0.0, 0.0], [0.0, sy, 0.0], [0.0, 0.0, 1.0]);
                let (Curve::Ellipse(e), shift) = c.transformed(&a).unwrap() else {
                    panic!("a non-uniform scale makes an ellipse");
                };
                assert_eq!(shift, ParamShift::NONE);
                assert_eq!((e.semi_x().mm(), e.semi_y().mm()), (2.0 * sx, 2.0 * sy));
                assert_eq!(
                    (e.frame().x(), e.frame().z(), e.sense()),
                    (Frame::WORLD.x(), Frame::WORLD.z(), sense)
                );
            }
            // A shear: the semi-axes are r times the golden ratio and its inverse, their
            // product r^2 (the shear keeps area).
            let shear = linear([1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
            let (Curve::Ellipse(e), shift) = c.transformed(&shear).unwrap() else {
                panic!("a shear makes an ellipse");
            };
            let phi = 0.5 * (1.0 + 5.0_f64.sqrt());
            let (a, b) = (e.semi_x().mm(), e.semi_y().mm());
            let (big, small) = (a.max(b), a.min(b));
            assert!(
                (big - 2.0 * phi).abs() <= 1e-15 * 8.0 && (small - 2.0 / phi).abs() <= 1e-15 * 8.0
            );
            assert_eq!(e.frame().z(), Frame::WORLD.z());
            assert!(shift.by().abs() <= core::f64::consts::FRAC_PI_4);
            assert_eq!(e.sense(), sense);
            // A projection along the plane's normal keeps the circle (z's image is not used).
            let flat_z = linear([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]);
            assert!(matches!(c.transformed(&flat_z), Ok((Curve::Circle(_), _))));
            // A map that flattens the plane is refused.
            let flat_y = linear([1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
            assert_eq!(
                c.transformed(&flat_y).unwrap_err(),
                CurveError::Degenerate(Degenerate::SemiAxis {
                    axis: crate::SemiAxis::Y,
                    length: 0.0
                })
            );
            let nan = linear([f64::NAN, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
            assert_eq!(
                c.transformed(&nan).unwrap_err(),
                CurveError::NotFinite(Part::Map)
            );
        }
    }

    #[test]
    fn transformed_near_similarities_and_degenerate_images() {
        // Inside the similarity test (16 roundings) the image is the snapped one, within
        // twice the misfit; just past it the principal axes are ill-conditioned but the
        // image is not, and every point maps to a few roundings.
        let e = Ellipse::new(
            Frame::WORLD,
            Length::from_mm(1.0),
            Length::from_mm(1.0),
            Sense::Forward,
        )
        .unwrap();
        let c = Curve::from(e);
        for (eps, tol) in [
            (1e-15, 2.0 * 16.0 * EPS),
            (1e-14, 2.0 * 16.0 * EPS),
            (1e-13, 4.0 * EPS),
            (1e-10, 4.0 * EPS),
        ] {
            let a = linear([1.0 + eps, 0.0, 0.0], [eps, 1.0, 0.0], [0.0, 0.0, 1.0]);
            let (image, shift) = c.transformed(&a).unwrap();
            for i in 0..32 {
                let t = f64::from(i) * 0.2;
                let want = a.apply(c.point(t));
                let got = image.point(shift.apply(t));
                assert!(
                    near3(got.to_vec(), want.to_vec(), 1.0, tol),
                    "{eps} {t} {:?}",
                    got - want
                );
            }
        }
        // A line taken to a point is refused.
        let l = Line::new(Point3::ORIGIN, Vec3::X).unwrap();
        let flat = linear([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
        assert_eq!(
            Curve::from(l).transformed(&flat).unwrap_err(),
            CurveError::Degenerate(Degenerate::Direction { length: 0.0 })
        );
        // A NURBS curve's mapped points overflowing are the curve's own refusal.
        let big = linear([1e308, 0.0, 0.0], [0.0, 1e308, 0.0], [0.0, 0.0, 1e308]);
        for c in curves(9).iter().filter(|c| !analytic(c)).take(3) {
            let c = c
                .transformed(&Affine::uniform_scaling(Point3::ORIGIN, 1e3).unwrap())
                .unwrap()
                .0;
            assert!(matches!(c.transformed(&big), Err(CurveError::Nurbs(_))));
        }
    }
}
