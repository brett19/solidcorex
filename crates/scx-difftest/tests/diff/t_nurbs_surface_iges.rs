// Written for solidcorex. Apache-2.0.
//! `scx_nurbs_surface::iges::from_iges` against the pin's `nurbs::iges::iges_128_surface`
//! (`N_Iges128Srf`: weights and knots checked, clamped to the bounds, multiplicities
//! reduced), on well-formed and broken entity data alike.
//!
//! Both sides take the same entity: degrees, the knots, a net of points and weights, and the
//! bounds. The result is the surface read back through each side's accessors (degrees, knots,
//! the homogeneous net row-major), or `None` where it is refused. The pin's arrays may be
//! longer than its counts say (it reads a prefix); scx's must match exactly. Where the two
//! disagree on acceptance, the pin defect `iges_128_surface` judges scx against the
//! definition of the entity (`pin_defects.rs`).

use crate::diff;
use crate::harness::{Gen, SplitMix64};
use crate::pin_defects;
use crate::t_nurbs_surface::Read;

use scx_num::hash::Rng;
use scx_testkit::sample::{self, Knots, Spec, Weights};

/// IGES 128 data, possibly broken by an edit. The net is `points` and `weights` cut into rows
/// of `cols` (rows from `points.len()`).
#[derive(Clone, Debug)]
pub struct Ent {
    pub edit: &'static str,
    pub degrees: (usize, usize),
    pub knots_u: Vec<f64>,
    pub knots_v: Vec<f64>,
    pub cols: usize,
    pub points: Vec<[f64; 3]>,
    pub weights: Vec<f64>,
    pub u: [f64; 2],
    pub v: [f64; 2],
}

impl Ent {
    pub fn rows(&self) -> usize {
        self.points.len() / self.cols.max(1)
    }

    /// Whether the net is a full rectangle with one weight per point.
    pub fn net_is_whole(&self) -> bool {
        self.cols > 0
            && self.points.len().is_multiple_of(self.cols)
            && self.weights.len() == self.points.len()
    }
}

fn base(d: scx_testkit::data::SurfaceData<3>) -> Ent {
    let cols = d.knots_v.len() - d.degrees.1 - 1;
    let weights = d.weights.unwrap_or_else(|| vec![1.0; d.points.len()]);
    Ent {
        edit: "as generated",
        degrees: d.degrees,
        knots_u: d.knots_u,
        knots_v: d.knots_v,
        cols,
        points: d.points,
        weights,
        u: [0.0, 1.0],
        v: [0.0, 1.0],
    }
}

fn entities(rng: &mut SplitMix64) -> Vec<Ent> {
    let mut r = Rng::new(rng.next_u64());
    let mut out = Vec::new();
    for (pu, pv) in [(1, 1), (2, 3), (3, 2), (4, 4)] {
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
                out.push(base(sample::surface(
                    &mut r,
                    (spec(pu, 3), spec(pv, 2)),
                    weights,
                )));
            }
        }
    }
    for (pu, pv) in [(0, 2), (17, 1), (18, 1), (31, 2), (32, 1)] {
        let spec = |degree| Spec {
            degree,
            count: degree + 2,
            knots: Knots::Random,
        };
        out.push(base(sample::surface(
            &mut r,
            (spec(pu), spec(pv)),
            Weights::Moderate,
        )));
    }
    out
}

fn edits(r: &Ent) -> Vec<Ent> {
    let (pu, pv) = r.degrees;
    let mut out = vec![r.clone()];
    let mut edit = |name: &'static str, f: &dyn Fn(&mut Ent)| {
        let mut e = r.clone();
        e.edit = name;
        f(&mut e);
        out.push(e);
    };
    edit("bounds inside", &|e| {
        e.u = [0.2, 0.9];
        e.v = [0.35, 0.6];
    });
    edit("bounds on one knot span", &|e| {
        e.u = [0.0, 0.5];
        e.v = [0.5, 1.0];
    });
    edit("junk first u knot", &|e| e.knots_u[0] = -5.0);
    edit("junk last v knot", &|e| {
        if let Some(k) = e.knots_v.last_mut() {
            *k = 9.0;
        }
    });
    edit("unclamped u", &|e| {
        let m = e.knots_u.len() - 1;
        for i in 0..pu {
            let d = 0.1 * (pu - i) as f64;
            e.knots_u[i] = -d;
            e.knots_u[m - i] = 1.0 + d;
        }
    });
    edit("unclamped v start", &|e| {
        for i in 0..pv {
            e.knots_v[i] = -0.25 * (pv - i) as f64;
        }
    });
    edit("unclamped, bounds inside", &|e| {
        let m = e.knots_u.len() - 1;
        for i in 0..pu {
            e.knots_u[i] = -0.1 * (pu - i) as f64;
            e.knots_u[m - i] = 1.0 + 0.1 * (pu - i) as f64;
        }
        e.u = [0.15, 0.85];
    });
    edit("u end multiplicity p + 2", &|e| {
        if let Some(k) = e.knots_u.get_mut(pu + 1) {
            *k = 0.0;
        }
    });
    edit("v end multiplicity p + 2", &|e| {
        let m = e.knots_v.len() - 1;
        if let Some(k) = e.knots_v.get_mut(m - pv - 1) {
            *k = 1.0;
        }
    });
    edit("u interior break", &|e| {
        let mid = e.knots_u.len() / 2;
        let k = e.knots_u[mid];
        for x in e.knots_u.iter_mut().skip(mid).take(pu + 1) {
            *x = k;
        }
    });
    edit("u knots decrease", &|e| e.knots_u[1] = 2.0);
    edit("NaN interior u knot", &|e| {
        let mid = e.knots_u.len() / 2;
        e.knots_u[mid] = f64::NAN;
    });
    edit("infinite last v knot", &|e| {
        if let Some(k) = e.knots_v.last_mut() {
            *k = f64::INFINITY;
        }
    });
    edit("v knot dropped", &|e| {
        e.knots_v.pop();
    });
    edit("extra u knot", &|e| e.knots_u.push(1.0));
    edit("empty u bounds", &|e| e.u = [0.5, 0.5]);
    edit("u bounds reversed", &|e| e.u = [0.9, 0.1]);
    edit("u bounds below", &|e| e.u = [-0.1, 1.0]);
    edit("v bounds above", &|e| e.v = [0.0, 1.1]);
    edit("NaN bound", &|e| e.u[0] = f64::NAN);
    edit("infinite bound", &|e| e.v[1] = f64::INFINITY);
    edit("u degree up", &|e| e.degrees.0 += 1);
    edit("v degree down", &|e| e.degrees.1 -= 1);
    edit("NaN point", &|e| {
        let mid = e.points.len() / 2;
        e.points[mid][1] = f64::NAN;
    });
    edit("infinite point", &|e| e.points[0][2] = f64::NEG_INFINITY);
    edit("ragged net", &|e| {
        e.points.pop();
        e.weights.pop();
    });
    edit("fewer weights", &|e| {
        e.weights.pop();
    });
    for (name, w) in [
        ("zero weight", 0.0),
        ("negative weight", -0.5),
        ("NaN weight", f64::NAN),
        ("infinite weight", f64::INFINITY),
        ("weight 1e-4", 1e-4),
        ("weight 1e3", 1e3),
        ("weight 1e300", 1e300),
    ] {
        edit(name, &|e| {
            if let Some(x) = e.weights.last_mut() {
                *x = w;
            }
        });
    }
    edit("all weights 1e-6", &|e| {
        e.weights.iter_mut().for_each(|w| *w *= 1e-6);
    });
    edit("all weights 2", &|e| {
        e.weights.iter_mut().for_each(|w| *w = 2.0)
    });
    edit("w P overflows", &|e| {
        if let (Some(w), Some(p)) = (e.weights.last_mut(), e.points.last_mut()) {
            *w = 1e300;
            *p = [1e10, 1.0, 1.0];
        }
    });
    out
}

impl Gen for Ent {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        entities(rng).iter().flat_map(edits).collect()
    }
}

/// scx's side.
pub mod x {
    use super::{Ent, Read};
    use crate::t_nurbs_surface::x::read;
    use scx_nurbs_surface::Net;
    use scx_nurbs_surface::iges::{Iges128, from_iges};

    /// The entity as scx takes it, or `None` where the net is not a whole rectangle (a type
    /// cannot say it).
    pub fn entity(r: &Ent) -> Option<Iges128> {
        if !r.net_is_whole() {
            return None;
        }
        let rows = r.rows();
        Some(Iges128 {
            m1: r.degrees.0,
            m2: r.degrees.1,
            s: r.knots_u.clone(),
            t: r.knots_v.clone(),
            weights: Net::new(rows, r.cols, r.weights.clone()).ok()?,
            points: Net::new(rows, r.cols, r.points.clone()).ok()?,
            u: r.u,
            v: r.v,
        })
    }

    pub fn from(r: &Ent) -> Option<Read> {
        Some(read(&from_iges(&entity(r)?).ok()?))
    }
}

/// The pin's side: `iges_128_surface`.
pub mod s {
    use super::{Ent, Read};
    use solidcore::math::Vec3;
    use solidcore::nurbs::iges::{Entity128, iges_128_surface};
    use solidcore::nurbs::math::NOW;

    pub fn from(r: &Ent) -> Option<Read> {
        let cols = r.cols.max(1);
        let w: Vec<Vec<f64>> = r.weights.chunks(cols).map(<[f64]>::to_vec).collect();
        let pts: Vec<Vec<Vec3>> = r
            .points
            .chunks(cols)
            .map(|row| row.iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect())
            .collect();
        // The pin's `K1` and `K2` are the net's last indices.
        let (k1, k2) = (pts.len().checked_sub(1)?, cols - 1);
        let e = Entity128 {
            k1,
            k2,
            m1: r.degrees.0,
            m2: r.degrees.1,
            s: &r.knots_u,
            t: &r.knots_v,
            w: &w,
            pts: &pts,
            u: r.u,
            v: r.v,
        };
        let s = iges_128_surface(&e).ok()?;
        let pts = s
            .rows()
            .flatten()
            .map(|c| [c.x, c.y, c.z, if c.w == NOW { 1.0 } else { c.w }])
            .collect();
        Some((
            s.p(),
            s.q(),
            s.u_knots().to_vec(),
            s.v_knots().to_vec(),
            pts,
        ))
    }
}

diff!(
    iges_128,
    pin_defect = pin_defects::iges_128_surface,
    |r: Ent| x::from(&r),
    s::from(&r)
);

/// The entity of a surface and back: exact, whenever the surface is a surface.
#[test]
fn iges_128_round_trip() {
    use scx_nurbs::KnotVector;
    use scx_nurbs_surface::iges::{from_iges, to_iges};
    use scx_nurbs_surface::{Net, NurbsSurface};

    let mut rng = SplitMix64(crate::harness::seed_of("iges_128_round_trip"));
    let mut n = 0;
    for r in Ent::pool(&mut rng)
        .iter()
        .filter(|r| r.edit == "as generated")
    {
        let rows = r.rows();
        let (Ok(u), Ok(v)) = (
            KnotVector::new(r.degrees.0, r.knots_u.clone()),
            KnotVector::new(r.degrees.1, r.knots_v.clone()),
        ) else {
            continue;
        };
        let (Ok(p), Ok(w)) = (
            Net::new(rows, r.cols, r.points.clone()),
            Net::new(rows, r.cols, r.weights.clone()),
        ) else {
            continue;
        };
        let rational = r.weights.iter().any(|&x| x != r.weights[0]);
        let Ok(s) = NurbsSurface::new(u, v, p, rational.then_some(w)) else {
            continue;
        };
        assert_eq!(from_iges(&to_iges(&s)).as_ref(), Ok(&s), "{r:?}");
        n += 1;
    }
    assert!(n > 20, "{n}");
}

/// Whatever the data, scx refuses it as the caller's: no index ever falls outside an array.
#[test]
fn iges_128_never_defects() {
    use scx_num::error::ErrorKind;
    let mut rng = SplitMix64(crate::harness::seed_of("iges_128_never_defects"));
    for r in Ent::pool(&mut rng) {
        if let Some(e) = x::entity(&r)
            && let Err(err) = scx_nurbs_surface::iges::from_iges(&e)
        {
            assert_ne!(err.kind(), ErrorKind::Defect, "{r:?}");
        }
    }
}
