// Written for solidcorex. Apache-2.0.
//! `scx-nurbs-surface` construction against the pin's `nurbs::srf_basic::Surface`
//! (`Surface::new`, then `Surface::is_valid`: `counts_are_valid`, `weights_are_valid` and
//! `KnotVector::validate` in each direction), on valid and invalid surface data alike.
//!
//! The pin builds almost anything and checks it later, so its side is `new` followed by
//! `is_valid`; scx's is [`NurbsSurface::new`] (or [`NurbsSurface::from_homogeneous`]), which
//! checks everything once. The pin's kernel `BSplineSurface::from_data` adds no check of its
//! own (it only fixes up near knots, which scx does not do: the stored form is exact).
//!
//! The result is the surface read back through each side's accessors (degrees, knots, the
//! homogeneous net row-major), or `None` where it is refused. Where the two disagree on
//! acceptance, the pin defect `nurbs_surface_new` judges scx against the definition of a
//! NURBS surface (`pin_defects.rs`).

use crate::diff;
use crate::harness::{Gen, SplitMix64};
use crate::pin_defects;

use scx_num::hash::Rng;
use scx_testkit::data::SurfaceData;
use scx_testkit::sample::{self, Knots, Spec, Weights};

/// A surface read back: degrees, the u and v knots, and the homogeneous points `[w x, w y,
/// w z, w]` row-major (`w = 1` for a polynomial surface).
pub type Read = (usize, usize, Vec<f64>, Vec<f64>, Vec<[f64; 4]>);

/// Surface data, possibly broken by an edit. The net is `points` cut into rows of `cols`
/// (the last row short when `points.len()` is not a multiple: a ragged net for the pin).
#[derive(Clone, Debug)]
pub struct Raw {
    pub edit: &'static str,
    pub degrees: (usize, usize),
    pub knots_u: Vec<f64>,
    pub knots_v: Vec<f64>,
    pub cols: usize,
    pub points: Vec<[f64; 3]>,
    pub weights: Option<Vec<f64>>,
}

impl Raw {
    /// The rows of the net: `points.len() / cols`, rounded up.
    pub fn rows(&self) -> usize {
        self.points.len().div_ceil(self.cols.max(1))
    }

    /// The homogeneous points as given to both sides: `(w P, w)`, or `(P, 1)`.
    pub fn hom(&self) -> Vec<[f64; 4]> {
        match &self.weights {
            None => self
                .points
                .iter()
                .map(|p| [p[0], p[1], p[2], 1.0])
                .collect(),
            Some(w) => self
                .points
                .iter()
                .zip(w)
                .map(|(p, &w)| [w * p[0], w * p[1], w * p[2], w])
                .collect(),
        }
    }

    /// The data as the testkit's exact surface takes it.
    pub fn data(&self) -> SurfaceData<3> {
        SurfaceData {
            degrees: self.degrees,
            knots_u: self.knots_u.clone(),
            knots_v: self.knots_v.clone(),
            points: self.points.clone(),
            weights: self.weights.clone(),
        }
    }
}

/// A rational [`Raw`], built from homogeneous points.
#[derive(Clone, Debug)]
pub struct Homog(pub Raw);

fn base(d: SurfaceData<3>) -> Raw {
    let cols = d.knots_v.len() - d.degrees.1 - 1;
    Raw {
        edit: "as generated",
        degrees: d.degrees,
        knots_u: d.knots_u,
        knots_v: d.knots_v,
        cols,
        points: d.points,
        weights: d.weights,
    }
}

/// The generated surfaces: degree pairs from 1 to 9, every knot and weight kind, and degrees
/// 0, 17, 18, 31 and 32 (the pin's cap is 17, scx's 31).
pub fn surfaces(rng: &mut SplitMix64) -> Vec<Raw> {
    let mut r = Rng::new(rng.next_u64());
    let mut out = Vec::new();
    for (pu, pv) in [(1, 1), (2, 1), (2, 3), (3, 3), (5, 2), (9, 4)] {
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
                    (spec(pu, 3), spec(pv, 1)),
                    weights,
                )));
            }
        }
    }
    for (pu, pv) in [(0, 2), (17, 1), (18, 1), (31, 2), (32, 1)] {
        for weights in [Weights::Unit, Weights::Moderate] {
            let spec = |degree| Spec {
                degree,
                count: degree + 2,
                knots: Knots::Random,
            };
            out.push(base(sample::surface(&mut r, (spec(pu), spec(pv)), weights)));
        }
    }
    out
}

/// `r` edited every way that breaks (or, for the pin, seems to break) a surface.
fn edits(r: &Raw) -> Vec<Raw> {
    let (pu, pv) = r.degrees;
    let mut out = vec![r.clone()];
    let mut edit = |name: &'static str, f: &dyn Fn(&mut Raw)| {
        let mut e = r.clone();
        e.edit = name;
        f(&mut e);
        out.push(e);
    };
    edit("extra row", &|e| {
        let row: Vec<_> = e.points.iter().take(e.cols).copied().collect();
        e.points.extend(row);
        if let Some(w) = &mut e.weights {
            let row: Vec<_> = w.iter().take(e.cols).copied().collect();
            w.extend(row);
        }
    });
    edit("short rows", &|e| {
        let cols = e.cols;
        let keep = |k: &usize| k % cols != cols - 1;
        e.points = e
            .points
            .iter()
            .enumerate()
            .filter(|(k, _)| keep(k))
            .map(|(_, p)| *p)
            .collect();
        if let Some(w) = &mut e.weights {
            *w = w
                .iter()
                .enumerate()
                .filter(|(k, _)| keep(k))
                .map(|(_, w)| *w)
                .collect();
        }
        e.cols -= 1;
    });
    edit("ragged", &|e| {
        e.points.pop();
        if let Some(w) = &mut e.weights {
            w.pop();
        }
    });
    edit("u degree up", &|e| e.degrees.0 += 1);
    edit("v knot dropped", &|e| {
        e.knots_v.pop();
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
    edit("u end multiplicity p + 2", &|e| {
        if let Some(k) = e.knots_u.get_mut(pu + 1) {
            *k = 0.0;
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
    edit("empty u domain", &|e| {
        e.knots_u.iter_mut().for_each(|k| *k = 0.5)
    });
    edit("NaN point", &|e| {
        let mid = e.points.len() / 2;
        e.points[mid][1] = f64::NAN;
    });
    edit("infinite point", &|e| e.points[0][2] = f64::NEG_INFINITY);
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
            let n = e.points.len();
            let ws = e.weights.get_or_insert_with(|| vec![1.0; n]);
            if let Some(x) = ws.last_mut() {
                *x = w;
            }
        });
    }
    edit("all weights 1e-6", &|e| {
        let n = e.points.len();
        let ws = e.weights.get_or_insert_with(|| vec![1.0; n]);
        ws.iter_mut().for_each(|w| *w *= 1e-6);
    });
    edit("w P overflows", &|e| {
        let n = e.points.len();
        let ws = e.weights.get_or_insert_with(|| vec![1.0; n]);
        if let (Some(w), Some(p)) = (ws.last_mut(), e.points.last_mut()) {
            *w = 1e300;
            *p = [1e10, 1.0, 1.0];
        }
    });
    out
}

impl Gen for Raw {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        surfaces(rng).iter().flat_map(edits).collect()
    }
}

impl Gen for Homog {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        Raw::pool(rng)
            .into_iter()
            .filter(|r| r.weights.is_some())
            .map(Homog)
            .collect()
    }
}

/// scx's side.
pub mod x {
    use super::{Raw, Read};
    use scx_nurbs::KnotVector;
    use scx_nurbs_surface::{Net, NurbsSurface, ParamAxis};

    pub fn read(s: &NurbsSurface<3>) -> Read {
        let pts = match (s.weighted_points(), s.weights()) {
            (Some(p), Some(w)) => p
                .points()
                .iter()
                .zip(w.points())
                .map(|(p, &w)| [p[0], p[1], p[2], w])
                .collect(),
            _ => s
                .points()
                .points()
                .iter()
                .map(|p| [p[0], p[1], p[2], 1.0])
                .collect(),
        };
        (
            s.degree(ParamAxis::U),
            s.degree(ParamAxis::V),
            s.knots(ParamAxis::U).to_vec(),
            s.knots(ParamAxis::V).to_vec(),
            pts,
        )
    }

    fn knots(r: &Raw) -> Option<(KnotVector, KnotVector)> {
        Some((
            KnotVector::new(r.degrees.0, r.knots_u.clone()).ok()?,
            KnotVector::new(r.degrees.1, r.knots_v.clone()).ok()?,
        ))
    }

    pub fn new(r: &Raw) -> Option<Read> {
        let (u, v) = knots(r)?;
        let (rows, cols) = (r.rows(), r.cols);
        let points = Net::new(rows, cols, r.points.clone()).ok()?;
        let weights = match &r.weights {
            None => None,
            Some(w) => Some(Net::new(rows, cols, w.clone()).ok()?),
        };
        Some(read(&NurbsSurface::new(u, v, points, weights).ok()?))
    }

    pub fn from_homogeneous(r: &Raw) -> Option<Read> {
        let (u, v) = knots(r)?;
        let (rows, cols) = (r.rows(), r.cols);
        let hom = r.hom();
        let weighted = Net::new(rows, cols, hom.iter().map(|h| [h[0], h[1], h[2]]).collect());
        let weights = Net::new(rows, cols, hom.iter().map(|h| h[3]).collect());
        let s = NurbsSurface::from_homogeneous(u, v, weighted.ok()?, weights.ok()?).ok()?;
        Some(read(&s))
    }
}

/// The pin's side: `Surface::new`, then `is_valid`.
pub mod s {
    use super::{Raw, Read};
    use solidcore::nurbs::math::{CPoint, NOW};
    use solidcore::nurbs::srf_basic::Surface;

    pub fn new(r: &Raw) -> Option<Read> {
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
    nurbs_surface_new,
    pin_defect = pin_defects::nurbs_surface_new,
    |r: Raw| x::new(&r),
    s::new(&r)
);
diff!(
    nurbs_surface_from_homogeneous,
    pin_defect = pin_defects::nurbs_surface_from_homogeneous,
    |h: Homog| x::from_homogeneous(&h.0),
    s::new(&h.0)
);
