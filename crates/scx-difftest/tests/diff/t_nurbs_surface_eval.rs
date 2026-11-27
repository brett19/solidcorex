// Written for solidcorex. Apache-2.0.
//! `scx-nurbs-surface` evaluation against the pin's `nurbs::srf_nurb` (`Surface::eval` and
//! `Surface::derivs` with `upper`, the triangle `k + l <= order`), on the generated surfaces of
//! `t_nurbs_surface` (every knot and weight kind, degrees 0 to 32) at the corners of the
//! domain, the knots and a rounding error either side of them, points outside and random
//! ones, from each pair of sides of a knot in turn.
//!
//! Each side builds its surface as `t_nurbs_surface` does (the pin: `Surface::new`, then
//! `is_valid`), so a surface one side refuses evaluates to `None` there. The results are
//! compared as `Option`s; the pin defects are in `pin_defects.rs`.

use crate::diff;
use crate::harness::{Gen, SplitMix64};
use crate::pin_defects;
use crate::t_nurbs_surface::{Raw, surfaces};

/// A generated surface, a parameter `[u, v]` for it and the sides to evaluate from.
#[derive(Clone, Debug)]
pub struct At {
    pub raw: Raw,
    pub uv: [f64; 2],
    pub sides: Sides,
}

/// The side of a knot in each direction: `true` is the piece that starts at the knot (scx
/// `Approach::Above`, the pin's `Side::Left`).
#[derive(Clone, Copy, Debug)]
pub struct Sides(pub [bool; 2]);

/// The four, in the order [`At`]'s pool cycles through them.
const SIDES: [Sides; 4] = [
    Sides([true, true]),
    Sides([false, false]),
    Sides([true, false]),
    Sides([false, true]),
];

/// Parameters along one direction of domain `[a, b]`: the ends and a rounding error inside
/// them, two interior knots and their neighbours, two points outside and two random ones.
///
/// Inside an end at zero the rounding error is `eps b`, not the least subnormal: the exact
/// truth of a degree-31 basis at `5e-324` takes seconds.
fn along(knots: &[f64], degree: usize, rng: &mut SplitMix64) -> Vec<f64> {
    let n = knots.len().saturating_sub(degree + 1);
    let a = knots[degree.min(knots.len() - 1)];
    let b = knots[n.min(knots.len() - 1)];
    let inside = if a == 0.0 {
        f64::EPSILON * b
    } else {
        a.next_up()
    };
    let mut t = vec![
        a,
        b,
        inside,
        b.next_down(),
        a - 0.25 * (b - a),
        b + 0.25 * (b - a),
    ];
    let interior = knots.iter().copied().filter(|&k| k > a && k < b);
    for k in interior.take(2) {
        t.extend([k, k.next_down(), k.next_up()]);
    }
    for _ in 0..2 {
        t.push(rng.uniform(a, b));
    }
    t
}

impl Gen for At {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = Vec::new();
        for raw in surfaces(rng) {
            let us = along(&raw.knots_u, raw.degrees.0, rng);
            let vs = along(&raw.knots_v, raw.degrees.1, rng);
            // the diagonal: every value of each direction meets one of the other's, from one
            // pair of sides each, the four in turn
            let n = us.len().max(vs.len());
            for i in 0..n {
                let sides = SIDES[v.len() % SIDES.len()];
                v.push(At {
                    raw: raw.clone(),
                    uv: [us[i % us.len()], vs[i % vs.len()]],
                    sides,
                });
            }
        }
        v
    }
}

/// [`At`]'s pool cut to what a jet's exact truth can afford: every case of a surface of
/// degree `p + q <= FULL`, and of the higher ones only those at the corners of the domain.
/// The exact partials of a degree-31 basis at a random parameter take about a second each
/// (order 2) in rationals; at a clamped end, a millisecond.
#[derive(Clone, Debug)]
pub struct JetAt<const FULL: usize>(pub At);

impl<const FULL: usize> Gen for JetAt<FULL> {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let end = |k: &[f64], p: usize, t: f64| {
            let n = k.len().saturating_sub(p + 1);
            k.get(p) == Some(&t) || k.get(n) == Some(&t)
        };
        At::pool(rng)
            .into_iter()
            .filter(|a| {
                let r = &a.raw;
                let (p, q) = r.degrees;
                p + q <= FULL || (end(&r.knots_u, p, a.uv[0]) && end(&r.knots_v, q, a.uv[1]))
            })
            .map(JetAt)
            .collect()
    }
}

/// A short batch for `eval_many`: a grid row (repeated `u`), a column, and a repeat.
pub fn batch(a: &At) -> Vec<[f64; 2]> {
    let [u, v] = a.uv;
    let knot = |k: &[f64], t: usize| k[t.min(k.len() - 1)];
    vec![
        [u, v],
        [u, knot(&a.raw.knots_v, a.raw.degrees.1 + 1)],
        [u, v],
        [knot(&a.raw.knots_u, a.raw.degrees.0 + 1), v],
    ]
}

/// A small grid for `eval_grid`: `us` holds the pool's `u` and a knot, `vs` the pool's `v`, a
/// knot and `v` again.
pub fn grid(a: &At) -> (Vec<f64>, Vec<f64>) {
    let [u, v] = a.uv;
    let knot = |k: &[f64], t: usize| k[t.min(k.len() - 1)];
    (
        vec![u, knot(&a.raw.knots_u, a.raw.degrees.0 + 1)],
        vec![v, knot(&a.raw.knots_v, a.raw.degrees.1 + 1), v],
    )
}

/// The points of [`grid`] row by row (`u` outer).
pub fn grid_points(a: &At) -> Vec<[f64; 2]> {
    let (us, vs) = grid(a);
    us.iter()
        .flat_map(|&u| vs.iter().map(move |&v| [u, v]))
        .collect()
}

/// The triangle `k + l <= N` of partials, row by row (`k` outer).
pub type Tri = Vec<[f64; 3]>;

/// scx's side.
pub mod x {
    use super::{At, Sides, Tri};
    use scx_nurbs::{Approach, KnotVector};
    use scx_nurbs_surface::{Net, NurbsSurface};

    fn side(s: Sides) -> [Approach; 2] {
        s.0.map(|a| if a { Approach::Above } else { Approach::Below })
    }

    pub fn surface(a: &At) -> Option<NurbsSurface<3>> {
        let r = &a.raw;
        let u = KnotVector::new(r.degrees.0, r.knots_u.clone()).ok()?;
        let v = KnotVector::new(r.degrees.1, r.knots_v.clone()).ok()?;
        let points = Net::new(r.rows(), r.cols, r.points.clone()).ok()?;
        let weights = match &r.weights {
            None => None,
            Some(w) => Some(Net::new(r.rows(), r.cols, w.clone()).ok()?),
        };
        NurbsSurface::new(u, v, points, weights).ok()
    }

    pub fn point(a: &At) -> Option<[f64; 3]> {
        surface(a)?.point(a.uv, side(a.sides)).ok()
    }

    pub fn jet<const N: usize>(a: &At) -> Option<Tri> {
        let j = surface(a)?.jet::<N>(a.uv, side(a.sides)).ok()?;
        (0..=N)
            .flat_map(|k| (0..=N - k).map(move |l| (k, l)))
            .map(|(k, l)| j.get(k, l))
            .collect()
    }

    pub fn eval_many(a: &At) -> Option<Vec<[f64; 3]>> {
        let uv = super::batch(a);
        let mut out = vec![[0.0; 3]; uv.len()];
        surface(a)?.eval_many(&uv, side(a.sides), &mut out).ok()?;
        Some(out)
    }

    pub fn eval_grid(a: &At) -> Option<Vec<[f64; 3]>> {
        let (us, vs) = super::grid(a);
        let mut out = vec![[0.0; 3]; us.len() * vs.len()];
        surface(a)?
            .eval_grid(&us, &vs, side(a.sides), &mut out)
            .ok()?;
        Some(out)
    }
}

/// The pin's side: `Surface::new`, `is_valid`, then `eval` or `derivs`.
pub mod s {
    use super::{At, Sides, Tri};
    use solidcore::math::Vec3;
    use solidcore::nurbs::knots::Side;
    use solidcore::nurbs::math::{CPoint, NOW};
    use solidcore::nurbs::srf_basic::Surface;

    /// The pin's `Left` is the span starting at the parameter (scx `Above`).
    fn side(above: bool) -> Side {
        if above { Side::Left } else { Side::Right }
    }

    fn v3(v: Vec3) -> [f64; 3] {
        [v.x, v.y, v.z]
    }

    pub fn surface(a: &At) -> Option<Surface> {
        let r = &a.raw;
        let cpts: Vec<CPoint> = match &r.weights {
            None => r
                .points
                .iter()
                .map(|p| CPoint::new(p[0], p[1], p[2], NOW))
                .collect(),
            Some(_) => r
                .hom()
                .iter()
                .map(|h| CPoint::new(h[0], h[1], h[2], h[3]))
                .collect(),
        };
        let net: Vec<Vec<CPoint>> = cpts.chunks(r.cols.max(1)).map(<[CPoint]>::to_vec).collect();
        let (p, q) = r.degrees;
        let s = Surface::new(net, p, q, r.knots_u.clone(), r.knots_v.clone()).ok()?;
        s.is_valid().ok()?;
        Some(s)
    }

    fn eval(s: &Surface, [u, v]: [f64; 2], sides: Sides) -> Option<[f64; 3]> {
        let [su, sv] = sides.0;
        s.eval(u, v, side(su), side(sv)).ok().map(v3)
    }

    pub fn point(a: &At) -> Option<[f64; 3]> {
        eval(&surface(a)?, a.uv, a.sides)
    }

    pub fn jet<const N: usize>(a: &At) -> Option<Tri> {
        let [u, v] = a.uv;
        let [su, sv] = a.sides.0;
        let d = surface(a)?
            .derivs(u, v, side(su), side(sv), true, N, N)
            .ok()?;
        Some(
            (0..=N)
                .flat_map(|k| (0..=N - k).map(move |l| (k, l)))
                .map(|(k, l)| v3(d[k][l]))
                .collect(),
        )
    }

    pub fn eval_many(a: &At) -> Option<Vec<[f64; 3]>> {
        let s = surface(a)?;
        super::batch(a)
            .into_iter()
            .map(|uv| eval(&s, uv, a.sides))
            .collect()
    }

    /// The pin has no grid evaluation: `eval` at each grid point.
    pub fn eval_grid(a: &At) -> Option<Vec<[f64; 3]>> {
        let s = surface(a)?;
        super::grid_points(a)
            .into_iter()
            .map(|uv| eval(&s, uv, a.sides))
            .collect()
    }
}

diff!(
    nurbs_surface_point,
    pin_defect = pin_defects::nurbs_surface_point,
    |a: At| x::point(&a),
    s::point(&a)
);
diff!(
    nurbs_surface_jet,
    pin_defect = pin_defects::nurbs_surface_jet,
    |a: JetAt<7>| x::jet::<2>(&a.0),
    s::jet::<2>(&a.0)
);
diff!(
    nurbs_surface_jet4,
    pin_defect = pin_defects::nurbs_surface_jet4,
    |a: JetAt<4>| x::jet::<4>(&a.0),
    s::jet::<4>(&a.0)
);
diff!(
    nurbs_surface_eval_many,
    pin_defect = pin_defects::nurbs_surface_eval_many,
    |a: At| x::eval_many(&a),
    s::eval_many(&a)
);
diff!(
    nurbs_surface_eval_grid,
    pin_defect = pin_defects::nurbs_surface_eval_grid,
    |a: At| x::eval_grid(&a),
    s::eval_grid(&a)
);
