// Written for solidcorex. Apache-2.0.
//! `scx_algo::point_set` against the pin's `geom::point_set`: a set's type, centroid,
//! normal, axis-aligned box and tightened pseudo box (free, about a given `z`, and with a
//! given normal), and a closed loop's area and point containment. Every pin answer comes
//! from a fresh pin set, asked for its normal first, so its call-order-dependent cache
//! holds the fit's centroid (the one scx keeps).

use crate::diff;
use crate::t_gutil::{sdir, stype, sv, unit, xdir, xpt, xtype, Pts};
use crate::t_pbox::{sparts, xparts};

use scx_algo::point_set as xs;
use scx_num::vector3d::Point3;
use solidcore::geom::point_set as ss;

type V = [f64; 3];
type XV = scx_num::vector3d::Vec3;
type SV = solidcore::geom::vector3d::Vec3;
type XIv = scx_num::extent1d::Extent1d;
type SIv = solidcore::geom::extent1d::Extent1d;

/// A box: basis and intervals.
pub type XBox = ([XV; 3], [XIv; 3]);
/// An axis-aligned box: min and max.
pub type Aab = (V, V);
/// What a set answers: (type, centroid, normal, box, tight box).
pub type SetOut = (&'static str, Point3, Option<V>, Option<Aab>, Option<XBox>);

fn xset(pts: &Pts, tol: f64) -> Option<xs::PointSet> {
    xs::PointSet::new(pts.0.iter().map(|&p| xpt(p)).collect(), tol).ok()
}
fn sset(pts: &Pts, tol: f64, normal: Option<&mut SV>) -> ss::PointSet3d {
    ss::PointSet3d::new(pts.0.iter().map(|&p| sv(p)).collect(), tol, normal)
}

fn xaab(s: &xs::PointSet) -> Option<Aab> {
    s.bbox().map(|b| (b.min().to_array(), b.max().to_array()))
}
/// The pin's empty extent (no points) is scx's `None`.
fn saab(s: &mut ss::PointSet3d) -> Option<Option<Aab>> {
    let empty = s.points().is_empty();
    let b = s.bbox(None).ok()?;
    let mn = [b.min_at(0), b.min_at(1), b.min_at(2)];
    let mx = [b.max_at(0), b.max_at(1), b.max_at(2)];
    Some((!empty).then_some((mn, mx)))
}
type SBox = ([SV; 3], [SIv; 3]);
/// The pin's box of no points is scx's `None`.
fn stight(s: &mut ss::PointSet3d, z: Option<&mut SV>) -> Option<Option<SBox>> {
    let empty = s.points().is_empty();
    let b = s.tight_pbox(z).ok()?;
    Some((!empty).then(|| sparts(&b)))
}

fn xout(s: &xs::PointSet) -> SetOut {
    (
        xtype(s.set_type()),
        s.centroid(),
        xdir(Some(s.normal())),
        xaab(s),
        s.tight_pbox().as_ref().map(xparts),
    )
}

diff!(
    ps_set,
    pin_defect = crate::pin_defects::ps_set,
    |pts: Pts, tol: f64| xset(&pts, tol).map(|s| xout(&s)),
    {
        let mut s = sset(&pts, tol, None);
        (|| {
            let n = s.normal().ok()?;
            // the type is not public: the fit's, as the set's own `SetUpAxes` takes it
            // (at the tolerance's magnitude, as the set stores it)
            let p: Vec<SV> = pts.0.iter().map(|&q| sv(q)).collect();
            let kind = solidcore::geom::geom_utility::p2::best_fit_plane_to_points(&p, tol.abs())
                .ok()?
                .set_type;
            let c = s.centroid();
            let aab = saab(&mut s)?;
            let tight = stight(&mut s, None)?;
            Some((stype(kind), c, sdir(n), aab, tight))
        })()
    }
);

diff!(
    ps_tight_about,
    pin_defect = crate::pin_defects::ps_tight_about,
    |pts: Pts, tol: f64, z: V| {
        let z = unit(z)?;
        Some(xset(&pts, tol)?.tight_pbox_about(z).as_ref().map(xparts))
    },
    {
        unit(z)?;
        let mut zz = sv(z);
        let mut s = sset(&pts, tol, None);
        stight(&mut s, Some(&mut zz))
    }
);

diff!(
    ps_with_normal,
    pin_defect = crate::pin_defects::ps_with_normal,
    |pts: Pts, tol: f64, n: V| {
        let s = xs::PointSet::with_normal(pts.0.iter().map(|&p| xpt(p)).collect(), tol, unit(n)?);
        Some(xout(&s))
    },
    {
        unit(n)?;
        let mut nn = sv(n);
        let mut s = sset(&pts, tol, Some(&mut nn));
        let normal = s.normal().ok()?;
        let c = s.centroid();
        let aab = saab(&mut s)?;
        let tight = stight(&mut s, None)?;
        Some(("Planar", c, sdir(normal), aab, tight))
    }
);

/// The queries for a loop: each point, each segment's midpoint, the centroid and the
/// midpoints between it and each point.
fn queries(pts: &Pts) -> Vec<V> {
    let p = &pts.0;
    let n = p.len();
    if n == 0 {
        return vec![[0.0; 3]];
    }
    let mid = |a: V, b: V| {
        [
            0.5 * (a[0] + b[0]),
            0.5 * (a[1] + b[1]),
            0.5 * (a[2] + b[2]),
        ]
    };
    #[allow(clippy::cast_precision_loss)]
    let c = p.iter().fold([0.0; 3], |s, q| {
        [
            s[0] + q[0] / n as f64,
            s[1] + q[1] / n as f64,
            s[2] + q[2] / n as f64,
        ]
    });
    let mut out = vec![c];
    for i in 0..n {
        out.push(p[i]);
        out.push(mid(p[i], p[(i + 1) % n]));
        out.push(mid(c, p[i]));
    }
    out
}

fn xcont(c: xs::PointContainment) -> &'static str {
    match c {
        xs::PointContainment::Unknown => "Unknown",
        xs::PointContainment::Inside => "Inside",
        xs::PointContainment::Outside => "Outside",
        xs::PointContainment::OnBoundary => "OnBoundary",
    }
}
fn scont(c: ss::PointContainment) -> &'static str {
    match c {
        ss::PointContainment::Unknown => "Unknown",
        ss::PointContainment::Inside => "Inside",
        ss::PointContainment::Outside => "Outside",
        ss::PointContainment::OnBoundary => "OnBoundary",
    }
}

diff!(
    ps_loop,
    |pts: Pts, tol: f64| {
        let lp = xs::PointLoop::new(xset(&pts, tol)?);
        let cont: Vec<&str> = queries(&pts)
            .into_iter()
            .map(|q| xcont(lp.containment(xpt(q))))
            .collect();
        Some((lp.area(), cont))
    },
    {
        let mut lp =
            ss::PointSequence::new(pts.0.iter().map(|&p| sv(p)).collect(), tol, true, None);
        (|| {
            lp.normal().ok()?;
            // the pin's -1 for no points is scx's 0 (a deviation of the port)
            let area = if pts.0.is_empty() {
                0.0
            } else {
                lp.area().ok()?
            };
            let mut cont = Vec::new();
            for q in queries(&pts) {
                cont.push(scont(lp.point_containment(sv(q)).ok()?));
            }
            Some((area, cont))
        })()
    }
);
