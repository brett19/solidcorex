// Written for solidcorex. Apache-2.0.
//! `scx-nurbs-surface` enclosures against the pin's `geom::nurbs_enclosure`
//! (`nurbs_partials_over_piece`, `_piece_about`, `nurbs_partials_over_box` and `_box_about`)
//! on the generated surfaces of `t_nurbs_surface` that both sides build, over boxes of each
//! domain: points at random and on an interior knot, a narrow box, one across a knot, one
//! that ends on a knot, and the whole domain.
//!
//! A result is every interval `[lo, hi]` of the triangle `a + b <= N`, row by row, or `None`
//! where the enclosure is refused. On a rational surface scx takes the pin's arithmetic in
//! its order, so the bits agree; on a polynomial one scx skips the weight function and the
//! quotient (`pin_defects::nurbs_surface_enclose`): its intervals lie inside the pin's.
//!
//! The pin's `Shift::Middle` is crate-only, so [`About::Middle`] is checked by the unit
//! tests (against the exact partials), not here.
//!
//! [`About::Middle`]: scx_nurbs_surface::About::Middle

use crate::diff;
use crate::harness::{Gen, SplitMix64};
use crate::pin_defects;
use crate::t_nurbs_surface::{Raw, surfaces};

/// A generated surface, a box `[[u lo, u hi], [v lo, v hi]]` of its domain, the knot
/// continuity a crossed line needs, and the point to enclose about (`None`: the origin).
#[derive(Clone, Debug)]
pub struct BoxAt {
    pub raw: Raw,
    pub uv: [[f64; 2]; 2],
    pub continuity: usize,
    pub about: Option<[f64; 3]>,
}

/// Every interval of the triangle, `[x lo, x hi, y lo, y hi, z lo, z hi]` per partial.
pub type Enc = Vec<[f64; 6]>;

/// Ranges along one direction of the domain `[a, b]`.
fn ranges(knots: &[f64], degree: usize, rng: &mut SplitMix64) -> Vec<[f64; 2]> {
    let n = knots.len() - degree - 1;
    let (a, b) = (knots[degree], knots[n]);
    let t = rng.uniform(a, b);
    let mut out = vec![
        [t, t],
        [a, b],
        [t, (t + 1e-3 * (b - a)).min(b)],
        [a, a],
        [b, b],
    ];
    if let Some(&k) = knots.iter().find(|&&k| k > a && k < b) {
        let w = 0.1 * (b - a);
        out.extend([
            [k, k],
            [(k - w).max(a), (k + w).min(b)],
            [(k - w).max(a), k],
        ]);
    }
    out
}

impl Gen for BoxAt {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = Vec::new();
        for raw in surfaces(rng) {
            if x::surface(&raw).is_none() || s::nurbs(&raw).is_none() {
                continue;
            }
            let us = ranges(&raw.knots_u, raw.degrees.0, rng);
            let vs = ranges(&raw.knots_v, raw.degrees.1, rng);
            let n = us.len().max(vs.len());
            for i in 0..n {
                // the about point: the origin, a control point, or one far off
                let about = match v.len() % 3 {
                    0 => None,
                    1 => raw.points.first().copied(),
                    _ => Some([1e3, -2e3, 0.5]),
                };
                v.push(BoxAt {
                    raw: raw.clone(),
                    uv: [us[i % us.len()], vs[(i * 5 + 1) % vs.len()]],
                    continuity: [0, 1, 0, 2][v.len() % 4],
                    about,
                });
            }
        }
        v
    }
}

/// scx's side.
pub mod x {
    use super::{BoxAt, Enc, Raw};
    use scx_num::interval::Ivl;
    use scx_num::jet::SurfaceJet;
    use scx_nurbs::{Approach, KnotVector};
    use scx_nurbs_surface::{About, Net, NurbsSurface, ParamAxis};

    pub fn surface(r: &Raw) -> Option<NurbsSurface<3>> {
        let u = KnotVector::new(r.degrees.0, r.knots_u.clone()).ok()?;
        let v = KnotVector::new(r.degrees.1, r.knots_v.clone()).ok()?;
        let points = Net::new(r.rows(), r.cols, r.points.clone()).ok()?;
        let weights = match &r.weights {
            None => None,
            Some(w) => Some(Net::new(r.rows(), r.cols, w.clone()).ok()?),
        };
        NurbsSurface::new(u, v, points, weights).ok()
    }

    fn about(b: &BoxAt) -> About<3> {
        b.about.map_or(About::Origin, About::Point)
    }

    fn uv(b: &BoxAt) -> [Ivl; 2] {
        b.uv.map(|[lo, hi]| Ivl::new(lo, hi))
    }

    pub fn flat<const N: usize>(j: &SurfaceJet<[Ivl; 3], N>) -> Enc {
        (0..=N)
            .flat_map(|a| (0..=N - a).map(move |b| (a, b)))
            .filter_map(|(a, b)| j.get(a, b))
            .map(|p| [p[0].lo, p[0].hi, p[1].lo, p[1].hi, p[2].lo, p[2].hi])
            .collect()
    }

    pub fn over<const N: usize>(b: &BoxAt) -> Option<Enc> {
        let s = surface(&b.raw)?;
        let j = s.partials_over::<N>(uv(b), b.continuity, about(b)).ok()?;
        Some(flat(&j))
    }

    /// The piece the box's lower corner starts.
    pub fn span(b: &BoxAt) -> Option<[usize; 2]> {
        let s = surface(&b.raw)?;
        let k = |axis, t| s.knot_vector(axis).span(t, Approach::Above).ok();
        Some([k(ParamAxis::U, b.uv[0][0])?, k(ParamAxis::V, b.uv[1][0])?])
    }

    pub fn piece<const N: usize>(b: &BoxAt) -> Option<Enc> {
        let s = surface(&b.raw)?;
        let j = s.partials_over_piece::<N>(span(b)?, uv(b), about(b)).ok()?;
        Some(flat(&j))
    }
}

/// The pin's side: its `Nurbs` from the same data (`pw` the homogeneous points u-major,
/// as `t_nurbs_surface`'s `Surface::new` reads them).
pub mod s {
    use super::{BoxAt, Enc, Raw};
    use solidcore::blend::fillet_section::interval::Ivl;
    use solidcore::blend::fillet_section::station::{Nurbs, Partials};
    use solidcore::geom::nurbs_enclosure as ne;

    pub fn nurbs(r: &Raw) -> Option<Nurbs> {
        // only what both sides build (t_nurbs_surface compares the building)
        crate::t_nurbs_surface_eval::s::surface(&crate::t_nurbs_surface_eval::At {
            raw: r.clone(),
            uv: [0.0, 0.0],
            sides: crate::t_nurbs_surface_eval::Sides([true, true]),
        })?;
        Some(Nurbs {
            p: r.degrees.0,
            q: r.degrees.1,
            nu: r.rows(),
            nv: r.cols,
            u: r.knots_u.clone(),
            v: r.knots_v.clone(),
            pw: r.hom().into_iter().flatten().collect(),
        })
    }

    fn flat(p: &Partials<Ivl>, n: usize) -> Enc {
        let i = |x: Ivl| [x.lo, x.hi];
        (0..=n)
            .flat_map(|a| (0..=n - a).map(move |b| (a, b)))
            .map(|(a, b)| {
                let v = p.at(a, b);
                let [x, y, z] = [i(v.x), i(v.y), i(v.z)];
                [x[0], x[1], y[0], y[1], z[0], z[1]]
            })
            .collect()
    }

    pub fn over<const N: usize>(b: &BoxAt) -> Option<Enc> {
        let c = nurbs(&b.raw)?;
        let [[ua, ub], [va, vb]] = b.uv;
        let p = match b.about {
            None => ne::nurbs_partials_over_box(&c, ua, ub, va, vb, N, b.continuity),
            Some(o) => ne::nurbs_partials_over_box_about(&c, ua, ub, va, vb, N, b.continuity, o),
        }
        .ok()?;
        Some(flat(&p, N))
    }

    pub fn piece<const N: usize>(b: &BoxAt) -> Option<Enc> {
        let c = nurbs(&b.raw)?;
        let [ku, kv] = super::x::span(b)?;
        let [[ua, ub], [va, vb]] = b.uv;
        let p = match b.about {
            None => ne::nurbs_partials_over_piece(&c, ku, kv, ua, ub, va, vb, N),
            Some(o) => ne::nurbs_partials_over_piece_about(&c, ku, kv, ua, ub, va, vb, N, o),
        }
        .ok()?;
        Some(flat(&p, N))
    }
}

diff!(
    nurbs_surface_enclose_over,
    pin_defect = pin_defects::nurbs_surface_enclose_over,
    |b: BoxAt| x::over::<2>(&b),
    s::over::<2>(&b)
);
diff!(
    nurbs_surface_enclose_over4,
    pin_defect = pin_defects::nurbs_surface_enclose_over4,
    |b: BoxAt| x::over::<4>(&b),
    s::over::<4>(&b)
);
diff!(
    nurbs_surface_enclose_point,
    pin_defect = pin_defects::nurbs_surface_enclose_point,
    |b: BoxAt| x::over::<0>(&b),
    s::over::<0>(&b)
);
diff!(
    nurbs_surface_enclose_piece,
    pin_defect = pin_defects::nurbs_surface_enclose_piece,
    |b: BoxAt| x::piece::<1>(&b),
    s::piece::<1>(&b)
);
