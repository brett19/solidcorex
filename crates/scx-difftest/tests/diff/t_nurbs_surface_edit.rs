// Written for solidcorex. Apache-2.0.
//! `scx-nurbs-surface`'s by-value edits against the pin's: knot insertion
//! (`srf_tool::p1::insert_knot`), splitting (`srf_tool::p1::split`), degree elevation
//! (`srf_tool::p2::elevate_degree`), reversal (`Surface::reversed`), swapping the
//! directions (`Surface::swap_uv`) and transformation (`Surface::transform`), each along
//! either direction, on valid surfaces of every knot and weight kind, clamped and not.
//!
//! Each result is the edited surface read back as in `t_nurbs_surface` (degrees, knots, the
//! homogeneous net row-major), or `None` where it is refused. Where the two disagree, the
//! pin defects `nurbs_surface_*` in `pin_defects.rs` judge scx against the exact edit of
//! `scx_testkit::nurbs`.

use crate::diff;
use crate::harness::{Gen, SplitMix64};
use crate::pin_defects;
use crate::t_nurbs_surface::Raw;

use scx_num::hash::Rng;
use scx_nurbs_surface::ParamAxis;
use scx_testkit::sample::{self, Knots, Spec, Weights};

/// A valid surface (one scx builds), clamped or not.
#[derive(Clone, Debug)]
pub struct Surf(pub Raw);

/// An edit along `axis` at `t`, `times` times (insertion; split ignores `times`, elevation
/// and reversal `t`).
#[derive(Clone, Debug)]
pub struct Edit {
    pub r: Raw,
    pub axis: ParamAxis,
    pub t: f64,
    pub times: usize,
}

/// An [`Edit`] whose `t` is unused: one per axis and count.
#[derive(Clone, Debug)]
pub struct Along(pub Edit);

/// An [`Edit`] whose `times` is unused: one per axis and parameter.
#[derive(Clone, Debug)]
pub struct At(pub Edit);

/// A surface and an affine map, given as its axes' images and the origin's.
#[derive(Clone, Debug)]
pub struct Mapped {
    pub r: Raw,
    pub map: [[f64; 3]; 4],
}

fn surfaces(rng: &mut SplitMix64) -> Vec<Raw> {
    let mut r = Rng::new(rng.next_u64());
    let mut out = Vec::new();
    for (pu, pv) in [(1, 1), (2, 3), (3, 2), (5, 2), (9, 3), (15, 1)] {
        for knots in [
            Knots::Uniform,
            Knots::Random,
            Knots::Clustered,
            Knots::Multiple,
        ] {
            for weights in [Weights::Unit, Weights::Moderate, Weights::NearZero] {
                let spec = |degree, extra| Spec {
                    degree,
                    count: degree + extra,
                    knots,
                };
                let d = sample::surface(&mut r, (spec(pu, 3), spec(pv, 2)), weights);
                let cols = d.knots_v.len() - d.degrees.1 - 1;
                let raw = Raw {
                    edit: "as generated",
                    degrees: d.degrees,
                    knots_u: d.knots_u,
                    knots_v: d.knots_v,
                    cols,
                    points: d.points,
                    weights: d.weights,
                };
                out.push(unclamped(&raw));
                out.push(raw);
            }
        }
    }
    out.retain(|r| crate::t_nurbs_surface::x::new(r).is_some());
    out
}

/// `r` with its end knots moved outside the domain: along u at both ends, along v at the
/// start.
fn unclamped(r: &Raw) -> Raw {
    let (pu, pv) = r.degrees;
    let mut e = r.clone();
    e.edit = "unclamped";
    let m = e.knots_u.len() - 1;
    for i in 0..pu {
        let d = 0.1 * (pu - i) as f64;
        e.knots_u[i] = -d;
        e.knots_u[m - i] = 1.0 + 0.75 * d;
    }
    for i in 0..pv {
        e.knots_v[i] = -0.25 * (pv - i) as f64;
    }
    e
}

/// The domain of `knots` for degree `p`.
fn domain(p: usize, knots: &[f64]) -> (f64, f64) {
    (knots[p], knots[knots.len() - 1 - p])
}

/// The parameters to edit at along an axis: inside a span, the first interior knot, just
/// past it, and the domain's end.
fn parameters(p: usize, knots: &[f64]) -> Vec<f64> {
    let (a, b) = domain(p, knots);
    let mut out = vec![a + 0.37 * (b - a), b];
    if let Some(&k) = knots.iter().find(|&&k| a < k && k < b) {
        out.push(k);
        out.push(k + 1e-9 * (b - a));
    }
    out
}

impl Gen for Surf {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        surfaces(rng).into_iter().map(Surf).collect()
    }
}

impl Gen for Edit {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut out = Vec::new();
        for r in surfaces(rng) {
            for axis in [ParamAxis::U, ParamAxis::V] {
                let (p, knots) = match axis {
                    ParamAxis::U => (r.degrees.0, &r.knots_u),
                    ParamAxis::V => (r.degrees.1, &r.knots_v),
                };
                for t in parameters(p, knots) {
                    for times in 0..4 {
                        out.push(Edit {
                            r: r.clone(),
                            axis,
                            t,
                            times,
                        });
                    }
                }
            }
        }
        out
    }
}

impl Gen for Along {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        Edit::pool(rng)
            .into_iter()
            .filter(|e| e.t == first_t(e))
            .map(Along)
            .collect()
    }
}

/// An [`Along`] whose result degree is at most 17, the pin's cap. Above it the pin refuses
/// and scx answers. A5.9's knot removal was then up to `1e77` from the exact elevation on
/// the degree-15 surfaces here (63 §2.6, NS3); `scx-nurbs` now removes knots the stable way
/// and is checked against the exact curve there by its own truth tests. The pool still
/// stops at the cap: past it there is no pin answer to compare, and the exact elevation
/// takes seconds a line.
#[derive(Clone, Debug)]
pub struct Raise(pub Edit);

impl Gen for Raise {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        Along::pool(rng)
            .into_iter()
            .filter(|a| {
                let p = match a.0.axis {
                    ParamAxis::U => a.0.r.degrees.0,
                    ParamAxis::V => a.0.r.degrees.1,
                };
                p + a.0.times <= 17
            })
            .map(|a| Raise(a.0))
            .collect()
    }
}

/// The first parameter [`Edit`] generates along its axis.
fn first_t(e: &Edit) -> f64 {
    let (p, knots) = match e.axis {
        ParamAxis::U => (e.r.degrees.0, &e.r.knots_u),
        ParamAxis::V => (e.r.degrees.1, &e.r.knots_v),
    };
    parameters(p, knots)[0]
}

impl Gen for At {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        Edit::pool(rng)
            .into_iter()
            .filter(|e| e.times == 1)
            .map(At)
            .collect()
    }
}

/// The maps: the identity, a shear with a translation, a turn and scale, and a far
/// translation.
const MAPS: [[[f64; 3]; 4]; 4] = [
    [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 0.0],
    ],
    [
        [2.0, 0.0, 0.0],
        [0.5, 1.0, 0.0],
        [0.0, -0.25, 3.0],
        [1.0, 2.0, 3.0],
    ],
    [
        [0.0, 3.0, 0.0],
        [-3.0, 0.0, 0.0],
        [0.0, 0.0, 3.0],
        [-0.5, 0.0, 0.25],
    ],
    [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1e6, -3e7, 0.1],
    ],
];

impl Gen for Mapped {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        surfaces(rng)
            .into_iter()
            .flat_map(|r| MAPS.map(|map| Mapped { r: r.clone(), map }))
            .collect()
    }
}

/// scx's side: [`NurbsSurface::new`] from the points and weights, then the edit.
pub mod x {
    use super::{Edit, Mapped};
    use crate::t_nurbs_surface::{Raw, Read, x::read};
    use scx_num::math::Affine;
    use scx_num::vector3d::{Point3, Vec3};
    use scx_nurbs::KnotVector;
    use scx_nurbs_surface::{Net, NurbsSurface};

    pub fn build(r: &Raw) -> NurbsSurface<3> {
        let u = KnotVector::new(r.degrees.0, r.knots_u.clone()).unwrap();
        let v = KnotVector::new(r.degrees.1, r.knots_v.clone()).unwrap();
        let (rows, cols) = (r.rows(), r.cols);
        let points = Net::new(rows, cols, r.points.clone()).unwrap();
        let weights = r.weights.clone().map(|w| Net::new(rows, cols, w).unwrap());
        NurbsSurface::new(u, v, points, weights).unwrap()
    }

    pub fn insert(e: &Edit) -> Option<Read> {
        Some(read(&build(&e.r).insert_knot(e.axis, e.t, e.times).ok()?))
    }

    pub fn split(e: &Edit) -> Option<(Read, Read)> {
        let (l, r) = build(&e.r).split(e.axis, e.t).ok()?;
        Some((read(&l), read(&r)))
    }

    pub fn elevate(e: &Edit) -> Option<Read> {
        Some(read(&build(&e.r).elevate(e.axis, e.times).ok()?))
    }

    pub fn reverse(e: &Edit) -> Option<Read> {
        Some(read(&build(&e.r).reversed(e.axis).ok()?))
    }

    pub fn swap(r: &Raw) -> Option<Read> {
        Some(read(&build(r).swapped()))
    }

    pub fn affine(m: &[[f64; 3]; 4]) -> Affine {
        let v = |a: [f64; 3]| Vec3::new(a[0], a[1], a[2]);
        Affine::from_axes(
            v(m[0]),
            v(m[1]),
            v(m[2]),
            Point3::new(m[3][0], m[3][1], m[3][2]),
        )
    }

    pub fn transform(m: &Mapped) -> Option<Read> {
        Some(read(&build(&m.r).transformed(&affine(&m.map)).ok()?))
    }
}

/// The pin's side: `Surface::new` from the homogeneous points, then the edit.
pub mod s {
    use super::{Edit, Mapped};
    use crate::t_nurbs_surface::{Raw, Read};
    use scx_nurbs_surface::ParamAxis;
    use solidcore::nurbs::math::{CPoint, MatrixType, NOW, RealMatrix};
    use solidcore::nurbs::srf_basic::{Direction, Surface};
    use solidcore::nurbs::srf_tool::{p1, p2};

    pub fn build(r: &Raw) -> Option<Surface> {
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
        let net: Vec<Vec<CPoint>> = cpts.chunks(r.cols).map(<[CPoint]>::to_vec).collect();
        let (p, q) = r.degrees;
        Surface::new(net, p, q, r.knots_u.clone(), r.knots_v.clone()).ok()
    }

    pub fn read(s: &Surface) -> Read {
        let pts = s
            .rows()
            .flatten()
            .map(|c| [c.x, c.y, c.z, if c.w == NOW { 1.0 } else { c.w }])
            .collect();
        (
            s.p(),
            s.q(),
            s.u_knots().to_vec(),
            s.v_knots().to_vec(),
            pts,
        )
    }

    fn dir(a: ParamAxis) -> Direction {
        match a {
            ParamAxis::U => Direction::U,
            ParamAxis::V => Direction::V,
        }
    }

    pub fn insert(e: &Edit) -> Option<Read> {
        let s = build(&e.r)?;
        Some(read(&p1::insert_knot(&s, e.t, e.times, dir(e.axis)).ok()?))
    }

    pub fn split(e: &Edit) -> Option<(Read, Read)> {
        let (l, r) = p1::split(&build(&e.r)?, e.t, dir(e.axis)).ok()?;
        Some((read(&l), read(&r)))
    }

    pub fn elevate(e: &Edit) -> Option<Read> {
        let s = build(&e.r)?;
        Some(read(&p2::elevate_degree(&s, e.times, dir(e.axis)).ok()?))
    }

    pub fn reverse(e: &Edit) -> Option<Read> {
        Some(read(&build(&e.r)?.reversed(dir(e.axis)).ok()?))
    }

    pub fn swap(r: &Raw) -> Option<Read> {
        let mut s = build(r)?;
        s.swap_uv();
        Some(read(&s))
    }

    /// The pin's 4x4 matrix of the map: column-vector form, row `i` `[X_i, Y_i, Z_i,
    /// shift_i]`, row 3 `[0, 0, 0, 1]`.
    pub fn transform(m: &Mapped) -> Option<Read> {
        let a = &m.map;
        let mut rows: Vec<Vec<f64>> = (0..3)
            .map(|i| vec![a[0][i], a[1][i], a[2][i], a[3][i]])
            .collect();
        rows.push(vec![0.0, 0.0, 0.0, 1.0]);
        let rma = RealMatrix::from_rows(3, 3, rows, MatrixType::Full, 3).ok()?;
        let mut s = build(&m.r)?;
        s.transform(&rma).ok()?;
        Some(read(&s))
    }
}

diff!(
    nurbs_surface_insert_knot,
    |e: Edit| x::insert(&e),
    s::insert(&e)
);
diff!(nurbs_surface_split, |a: At| x::split(&a.0), s::split(&a.0));
diff!(
    nurbs_surface_elevate,
    pin_defect = pin_defects::nurbs_surface_elevate,
    |a: Raise| x::elevate(&a.0),
    s::elevate(&a.0)
);
diff!(
    nurbs_surface_reversed,
    pin_defect = pin_defects::nurbs_surface_reversed,
    |a: Along| x::reverse(&a.0),
    s::reverse(&a.0)
);
diff!(
    nurbs_surface_swapped,
    |r: Surf| x::swap(&r.0),
    s::swap(&r.0)
);
diff!(
    nurbs_surface_transformed,
    |m: Mapped| x::transform(&m),
    s::transform(&m)
);
