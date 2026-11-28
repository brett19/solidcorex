// Written for solidcorex. Apache-2.0.
//! Certified convex-hull ranges of a NURBS surface's Bézier pieces (research 73 §20).
//!
//! [`Surface::hulls`] extracts the Bézier net of every knot-span rectangle with the error
//! bounds [`crate::nurbs_invert`] carries (each homogeneous coordinate is the true one to
//! within its bound), divides each point by its weight in interval arithmetic, and keeps the
//! points as boxes. A rational Bézier patch with positive weights lies in the convex hull of
//! its Euclidean control points, so for any direction the least and greatest of the points'
//! interval dot products (rounded outward) bound the surface over the whole rectangle: a
//! range as cheap as the control points, and certified. Nothing is sampled.

use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::interval::Ivl;
use scx_num::vector3d::{Point3, Vec3};

use scx_nurbs_surface::NurbsSurface;

use crate::nurbs_invert::patches;

use crate::surface::Surface;

/// The Euclidean control points of one Bézier patch as boxes, with the rectangle it covers.
#[derive(Clone, Debug)]
pub struct Hull {
    uv: Extent2d,
    pts: Vec<[Ivl; 3]>,
    axis: Option<Vec3>,
}

impl Hull {
    /// The uv rectangle of the Bézier patch (its knot span).
    #[must_use]
    pub const fn uv(&self) -> Extent2d {
        self.uv
    }

    /// An interval holding `dir . S(u, v)` over the whole rectangle: the least and greatest
    /// of the control points' dot products with `dir`, each rounded outward.
    #[must_use]
    pub fn range_along(&self, dir: Vec3) -> Ivl {
        let d = [Ivl::from(dir.x), Ivl::from(dir.y), Ivl::from(dir.z)];
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for p in &self.pts {
            let x = d[0] * p[0] + d[1] * p[1] + d[2] * p[2];
            lo = lo.min(x.lo);
            hi = hi.max(x.hi);
        }
        Ivl::new(lo, hi)
    }

    /// A box holding the surface over the whole rectangle: the control points' box.
    #[must_use]
    pub fn bbox(&self) -> Option<Extent3d> {
        let mut m: Option<[Ivl; 3]> = None;
        for &[x, y, z] in &self.pts {
            m = Some(match m {
                None => [x, y, z],
                Some([a, b, c]) => [a.hull(x), b.hull(y), c.hull(z)],
            });
        }
        let [x, y, z] = m?;
        Extent3d::from_bounds(x.lo, y.lo, z.lo, x.hi, y.hi, z.hi)
    }

    /// A direction for slabs across the piece: the mean of the unit normals of its control
    /// net's quadrilaterals (an estimate, floating-point: any direction proves, a good one
    /// proves more). `None` for a net with no usable normal.
    #[must_use]
    pub const fn axis(&self) -> Option<Vec3> {
        self.axis
    }
}

/// The hulls of a NURBS surface's knot-span rectangles, found by the rectangle.
#[derive(Clone, Debug)]
pub struct Hulls {
    us: Vec<f64>,
    vs: Vec<f64>,
    /// `u` spans outer.
    cells: Vec<Hull>,
}

impl Hulls {
    /// The index and hull of the Bézier patch whose rectangle holds `uv`, if one does.
    #[must_use]
    pub fn containing(&self, uv: &Extent2d) -> Option<(usize, &Hull)> {
        let find = |t: &[f64], lo: f64, hi: f64| {
            let i = t.partition_point(|&x| x <= lo).checked_sub(1)?;
            (hi <= *t.get(i + 1)?).then_some(i)
        };
        let iu = find(&self.us, uv.min().x, uv.max().x)?;
        let iv = find(&self.vs, uv.min().y, uv.max().y)?;
        let k = iu * (self.vs.len() - 1) + iv;
        Some((k, self.cells.get(k)?))
    }

    /// The hull at index `k` (as [`Hulls::containing`] gave it).
    #[must_use]
    pub fn get(&self, k: usize) -> Option<&Hull> {
        self.cells.get(k)
    }
}

impl Surface {
    /// The certified hulls of a NURBS surface's Bézier patches ([`Hulls`]), and of a
    /// procedural surface's net's, each control point widened by the net's bound so they
    /// hold the definition over the net's box; `None` for the analytic kinds (their rational images round their data, so their ranges come from their
    /// own formulas), or where a weight of the extraction is not provably positive or a value
    /// is not finite.
    #[must_use]
    pub fn hulls(&self) -> Option<Hulls> {
        match self {
            Self::Nurbs(n) => net_hulls(n, 0.0),
            Self::Procedural(p) => net_hulls(p.net(), p.bound()),
            _ => None,
        }
    }
}

/// The hulls of `n`'s patches, each control point's box widened by `pad` (a procedural
/// surface's net bound, so the hulls hold its definition; zero for a NURBS surface).
fn net_hulls(n: &NurbsSurface<3>, pad: f64) -> Option<Hulls> {
    let patches = patches(n);
    let mut us: Vec<f64> = patches.iter().map(|p| p.lo.x).collect();
    let mut vs: Vec<f64> = patches.iter().map(|p| p.lo.y).collect();
    us.sort_by(f64::total_cmp);
    us.dedup();
    vs.sort_by(f64::total_cmp);
    vs.dedup();
    if us.len() * vs.len() != patches.len() {
        return None;
    }
    us.push(
        patches
            .iter()
            .map(|p| p.hi.x)
            .fold(f64::NEG_INFINITY, f64::max),
    );
    vs.push(
        patches
            .iter()
            .map(|p| p.hi.y)
            .fold(f64::NEG_INFINITY, f64::max),
    );
    let mut cells = Vec::with_capacity(patches.len());
    for p in &patches {
        let mut pts = Vec::with_capacity(p.net.len());
        for (h, e) in &p.net {
            let [hx, hy, hz, hw] = *h;
            let [ex, ey, ez, ew] = *e;
            let w = Ivl::from(hw) + Ivl::new(-ew, ew);
            if !(w.lo > 0.0 && w.hi.is_finite()) {
                return None;
            }
            let mut q = [(hx, ex), (hy, ey), (hz, ez)]
                .map(|(x, ex)| (Ivl::from(x) + Ivl::new(-ex, ex)) / w);
            if pad > 0.0 {
                q = q.map(|c| c + Ivl::new(-pad, pad));
            }
            if !q.iter().all(|r| r.is_finite()) {
                return None;
            }
            pts.push(q);
        }
        let uv = Extent2d::from_bounds(p.lo.x, p.lo.y, p.hi.x, p.hi.y)?;
        let axis = mean_normal(&pts, p.degrees);
        cells.push(Hull { uv, pts, axis });
    }
    Some(Hulls { us, vs, cells })
}

/// The mean of the unit normals over the quadrilaterals of the net (index `i (q + 1) + j`),
/// at the points' middles; a unit vector, or `None` when they cancel or none exists.
fn mean_normal(pts: &[[Ivl; 3]], (p, q): (usize, usize)) -> Option<Vec3> {
    let at = |i: usize, j: usize| {
        let c = pts.get(i * (q + 1) + j)?;
        Some(Point3::new(c[0].mid(), c[1].mid(), c[2].mid()))
    };
    let mut sum = Vec3::new(0.0, 0.0, 0.0);
    for i in 0..p {
        for j in 0..q {
            let (a, b, c, d) = (at(i, j)?, at(i + 1, j)?, at(i, j + 1)?, at(i + 1, j + 1)?);
            // The diagonals' cross is the quadrilateral's area normal.
            let n = (d - a).cross(c - b);
            let len = n.length();
            if len > 0.0 && len.is_finite() {
                sum += n * (1.0 / len);
            }
        }
    }
    let len = sum.length();
    (len > 1e-9 && len.is_finite()).then(|| sum * (1.0 / len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Sphere, Torus};
    use scx_num::frame::{Frame, Sense};
    use scx_num::units::Length;
    use scx_num::vector2d::Point2;
    use scx_nurbs::KnotVector;
    use scx_nurbs_surface::{Net, NurbsSurface};
    use std::sync::Arc;

    fn frame() -> Frame {
        Frame::new(
            Point3::new(1.0, 2.0, -3.0),
            Vec3::new(0.3, -0.2, 0.9),
            Vec3::new(1.0, 0.1, 0.0),
        )
        .unwrap_or_else(|| panic!("frame"))
    }

    fn whole() -> Extent2d {
        Extent2d::from_bounds(0.0, 0.0, 1.0, 1.0).unwrap_or_else(|| panic!("box"))
    }

    fn torus_nurbs() -> Surface {
        let t = Torus::new(
            frame(),
            Length::from_mm(3.0),
            Length::from_mm(1.0),
            Sense::Forward,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        Surface::Nurbs(Arc::new(
            t.to_nurbs(&whole()).unwrap_or_else(|e| panic!("{e}")),
        ))
    }

    const DIRS: [[f64; 3]; 4] = [
        [1.0, 0.0, 0.0],
        [0.3, -0.5, 0.8],
        [-2.0, 0.25, 0.1],
        [0.0, 0.0, -1.0],
    ];

    /// Every dense sample of every patch lies in its hull's range along each direction and in
    /// its box; the rectangles are found by `containing`.
    #[test]
    fn hulls_hold_dense_samples() {
        let sphere = Sphere::new(frame(), Length::from_mm(2.0), Sense::Forward)
            .unwrap_or_else(|e| panic!("{e}"));
        let ns: Surface = Surface::Nurbs(Arc::new(
            sphere.to_nurbs(&whole()).unwrap_or_else(|e| panic!("{e}")),
        ));
        for s in [torus_nurbs(), ns] {
            let hulls = s.hulls().unwrap_or_else(|| panic!("hulls"));
            let n = 24;
            let mut seen = 0;
            for k in 0.. {
                let Some(h) = hulls.get(k) else { break };
                seen += 1;
                let (lo, hi) = (h.uv().min(), h.uv().max());
                let (found, again) = hulls.containing(&h.uv()).unwrap_or_else(|| panic!("find"));
                assert_eq!(found, k);
                assert_eq!(again.uv(), h.uv());
                let bb = h.bbox().unwrap_or_else(|| panic!("bbox"));
                for i in 0..=n {
                    for j in 0..=n {
                        let t = |a: f64, b: f64, m: usize| a + (b - a) * (m as f64 / n as f64);
                        let p = s.point(Point2::new(t(lo.x, hi.x, i), t(lo.y, hi.y, j)));
                        assert!(
                            bb.expanded(1e-12).unwrap_or(bb).contains_point(p, 0.0),
                            "{p:?} outside {bb:?}"
                        );
                        for d in DIRS {
                            let d = Vec3::new(d[0], d[1], d[2]);
                            let r = h.range_along(d);
                            let x = d.x * p.x + d.y * p.y + d.z * p.z;
                            assert!(r.lo - 1e-12 <= x && x <= r.hi + 1e-12, "{x} outside {r:?}");
                        }
                    }
                }
            }
            assert!(seen > 1);
        }
    }

    /// A bilinear patch is its own control hull: the range along a direction is exactly the
    /// range of its four corners (to rounding), and a rectangle is found by any inner part.
    #[test]
    fn a_bilinear_patch_has_its_exact_range() {
        let k = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap_or_else(|e| panic!("{e}"));
        let corners = [
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.25],
            [1.0, 0.0, -0.5],
            [1.0, 1.0, 0.1],
        ];
        let pts = Net::new(2, 2, corners.to_vec()).unwrap_or_else(|e| panic!("{e}"));
        let s = Surface::Nurbs(Arc::new(
            NurbsSurface::new(k.clone(), k, pts, None).unwrap_or_else(|e| panic!("{e}")),
        ));
        let hulls = s.hulls().unwrap_or_else(|| panic!("hulls"));
        let inner = Extent2d::from_bounds(0.25, 0.25, 0.5, 0.75).unwrap_or_else(|| panic!("box"));
        let (_, h) = hulls.containing(&inner).unwrap_or_else(|| panic!("find"));
        for d in DIRS {
            let r = h.range_along(Vec3::new(d[0], d[1], d[2]));
            let dots: Vec<f64> = corners
                .iter()
                .map(|c| d[0] * c[0] + d[1] * c[1] + d[2] * c[2])
                .collect();
            let (lo, hi) = (
                dots.iter().copied().fold(f64::INFINITY, f64::min),
                dots.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            );
            assert!(r.lo <= lo && hi <= r.hi, "{r:?} must hold [{lo}, {hi}]");
            assert!(
                lo - r.lo <= 1e-14 && r.hi - hi <= 1e-14,
                "{r:?} tight to [{lo}, {hi}]"
            );
        }
        // Outside the rectangles nothing is found.
        let past = Extent2d::from_bounds(0.5, 0.5, 1.5, 0.75).unwrap_or_else(|| panic!("box"));
        assert!(hulls.containing(&past).is_none());
        // The axis of a planar quadrilateral net is its normal.
        let a = h.axis().unwrap_or_else(|| panic!("axis"));
        assert!((a.length() - 1.0).abs() < 1e-12);
    }

    /// Only a NURBS surface has hulls: the other kinds' rational images round their data.
    #[test]
    fn analytic_kinds_have_none() {
        let sphere = Sphere::new(frame(), Length::from_mm(2.0), Sense::Forward)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(Surface::from(sphere).hulls().is_none());
    }

    /// The same bits on every call.
    #[test]
    fn hulls_are_deterministic() {
        let s = torus_nurbs();
        let (a, b) = (
            s.hulls().unwrap_or_else(|| panic!("a")),
            s.hulls().unwrap_or_else(|| panic!("b")),
        );
        for k in 0.. {
            let (Some(x), Some(y)) = (a.get(k), b.get(k)) else {
                break;
            };
            let d = Vec3::new(0.3, -0.5, 0.8);
            assert_eq!(x.range_along(d), y.range_along(d));
            assert_eq!(x.axis(), y.axis());
        }
    }
}
