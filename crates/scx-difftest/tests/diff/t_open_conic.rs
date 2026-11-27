// Written for solidcorex. Apache-2.0.
//! `scx-curve`'s open conics against the pin's `curve::parabola::Parabola` and
//! `curve::hyperbola::Hyperbola`: point and jet, in 3-D and (the pin's dimension-2 curves at
//! z = 0) 2-D.
//!
//! Both sides take STEP's forms, `O + F t^2 X + 2 F t Y` and `C + a cosh t X + b sinh t Y`,
//! on scx's frame axes (`y` negated for `Sense::Reversed`, the pin's mirror; [`axes3`],
//! [`axes2`]), the focal length or semi-axes from the conic data's `a` and `b`. The pin's
//! `evaluate_step` (three derivatives, its most) clamps the parameter to its domain
//! (`+-1111.111` for a parabola, `[-50, 50]` here for a hyperbola), so the parameters stay
//! inside both; scx does not clamp. The two agree within the tolerance throughout, so no pin
//! defect is needed; scx-curve's own tests hold both to the double-double truth.

use crate::diff;
use crate::harness::{Gen, SplitMix64};
use crate::t_curve::{GoodConic, axes2, axes3, frame2, frame3, sense};

use scx_curve::{Hyperbola, Hyperbola2, Parabola, Parabola2};
use scx_num::units::Length;
use scx_nurbs::Approach;

/// An open conic's parameter, inside both sides' domains.
#[derive(Clone, Debug)]
pub struct OpenT(pub f64);

impl Gen for OpenT {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = vec![0.0, -0.0, 1.0, -1.0, 1e-300, -1e-9, 0.5, 20.0, -35.5, 45.0];
        v.extend((0..10).map(|_| rng.uniform(-6.0, 6.0)));
        v.into_iter().map(OpenT).collect()
    }
}

/// Which open conic of the input: a parabola (focal length `a`) or a hyperbola (`a`, `b`).
#[derive(Clone, Copy)]
pub enum Kind {
    Parabola,
    Hyperbola,
}

/// scx's side.
pub mod x {
    use super::*;

    fn len(x: f64) -> Length {
        Length::from_mm(x)
    }

    pub fn jet(c: &GoodConic, kind: Kind, t: f64) -> Vec<[f64; 3]> {
        let (c, f) = (&c.0, frame3(&c.0).unwrap());
        let j = match kind {
            Kind::Parabola => Parabola::new(f, len(c.a), sense(c))
                .unwrap()
                .jet::<3>(t, Approach::Above),
            Kind::Hyperbola => Hyperbola::new(f, len(c.a), len(c.b), sense(c))
                .unwrap()
                .jet::<3>(t, Approach::Above),
        };
        let mut v = vec![j.point.to_array()];
        v.extend(j.d.iter().map(|d| d.to_array()));
        v
    }

    pub fn point(c: &GoodConic, kind: Kind, t: f64) -> [f64; 3] {
        let (c, f) = (&c.0, frame3(&c.0).unwrap());
        match kind {
            Kind::Parabola => Parabola::new(f, len(c.a), sense(c)).unwrap().point(t),
            Kind::Hyperbola => Hyperbola::new(f, len(c.a), len(c.b), sense(c))
                .unwrap()
                .point(t),
        }
        .to_array()
    }

    pub fn jet2(c: &GoodConic, kind: Kind, t: f64) -> Vec<[f64; 2]> {
        let (c, f) = (&c.0, frame2(&c.0).unwrap());
        let j = match kind {
            Kind::Parabola => Parabola2::new(f, len(c.a), sense(c))
                .unwrap()
                .jet::<3>(t, Approach::Below),
            Kind::Hyperbola => Hyperbola2::new(f, len(c.a), len(c.b), sense(c))
                .unwrap()
                .jet::<3>(t, Approach::Below),
        };
        let mut v = vec![j.point.to_array()];
        v.extend(j.d.iter().map(|d| d.to_array()));
        v
    }
}

/// The pin's side.
pub mod s {
    use super::*;
    use solidcore::curve::hyperbola::Hyperbola as PinHyperbola;
    use solidcore::curve::parabola::Parabola as PinParabola;
    use solidcore::geom::extent1d::Extent1d;

    fn p3(a: [f64; 3]) -> solidcore::Point3 {
        solidcore::Point3::new(a[0], a[1], a[2])
    }

    fn v3(a: [f64; 3]) -> solidcore::Vec3 {
        solidcore::Vec3::new(a[0], a[1], a[2])
    }

    fn step(
        axes: ([f64; 3], [f64; 3], [f64; 3]),
        c: &GoodConic,
        kind: Kind,
        t: f64,
        dim: u64,
    ) -> Vec<[f64; 3]> {
        let (o, x, y) = axes;
        let d = match kind {
            Kind::Parabola => PinParabola::new(
                p3(o),
                v3(x),
                v3(y),
                Extent1d::from_min_max(-1e6, 1e6),
                c.0.a,
                dim,
            )
            .evaluate_step(t, 3),
            Kind::Hyperbola => PinHyperbola::new(
                p3(o),
                v3(x),
                v3(y),
                Extent1d::from_min_max(-50.0, 50.0),
                c.0.a,
                c.0.b,
                dim,
            )
            .evaluate_step(t, 3),
        };
        d.unwrap().into_iter().map(|v| [v.x, v.y, v.z]).collect()
    }

    pub fn jet(c: &GoodConic, kind: Kind, t: f64) -> Vec<[f64; 3]> {
        step(axes3(&c.0), c, kind, t, 3)
    }

    pub fn point(c: &GoodConic, kind: Kind, t: f64) -> [f64; 3] {
        jet(c, kind, t)[0]
    }

    pub fn jet2(c: &GoodConic, kind: Kind, t: f64) -> Vec<[f64; 2]> {
        step(axes2(&c.0), c, kind, t, 2)
            .into_iter()
            .map(|p| [p[0], p[1]])
            .collect()
    }
}

diff!(
    curve_parabola_point,
    |c: GoodConic, t: OpenT| x::point(&c, Kind::Parabola, t.0),
    s::point(&c, Kind::Parabola, t.0)
);
diff!(
    curve_parabola_jet,
    |c: GoodConic, t: OpenT| x::jet(&c, Kind::Parabola, t.0),
    s::jet(&c, Kind::Parabola, t.0)
);
diff!(
    curve_parabola2_jet,
    |c: GoodConic, t: OpenT| x::jet2(&c, Kind::Parabola, t.0),
    s::jet2(&c, Kind::Parabola, t.0)
);
diff!(
    curve_hyperbola_point,
    |c: GoodConic, t: OpenT| x::point(&c, Kind::Hyperbola, t.0),
    s::point(&c, Kind::Hyperbola, t.0)
);
diff!(
    curve_hyperbola_jet,
    |c: GoodConic, t: OpenT| x::jet(&c, Kind::Hyperbola, t.0),
    s::jet(&c, Kind::Hyperbola, t.0)
);
diff!(
    curve_hyperbola2_jet,
    |c: GoodConic, t: OpenT| x::jet2(&c, Kind::Hyperbola, t.0),
    s::jet2(&c, Kind::Hyperbola, t.0)
);
