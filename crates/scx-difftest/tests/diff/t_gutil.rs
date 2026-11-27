// Written for solidcorex. Apache-2.0.
//! `scx-algo`: `geom` (the `smgu_*` helpers of `geom_utility` p1/p2) and `sort`.
//!
//! Results are compared as plain numbers. A unit normal is made once, by scx's
//! `normalized`, and given to both sides (the pin's functions that want a unit vector take a
//! plain one). scx's angle is in radians and the pin's in degrees: the scx side converts with
//! the pin's own `rad_to_deg`, so the comparison is exact. Fits are compared by type name,
//! point, direction and gaps; the pin's zero direction is scx's `None`.

use crate::diff;
use crate::harness::{Gen, List, SplitMix64};

use scx_algo::geom::{fit as xfit, line as xl, plane as xpl, polygon as xpg};
use scx_algo::sort as xs;
use solidcore::geom::geom_utility::{p1 as s1, p2 as s2};

type V = [f64; 3];
type XV = scx_num::vector3d::Vec3;
type XP = scx_num::vector3d::Point3;
type XU = scx_num::vector3d::UnitVec3;
type SV = solidcore::geom::vector3d::Vec3;

pub(crate) fn xv(a: V) -> XV {
    XV::new(a[0], a[1], a[2])
}
pub(crate) fn xpt(a: V) -> XP {
    XP::new(a[0], a[1], a[2])
}
fn sv(a: V) -> SV {
    SV::new(a[0], a[1], a[2])
}
/// scx's unit vector of `a`, given to both sides.
pub(crate) fn unit(a: V) -> Option<XU> {
    xv(a).normalized()
}
fn su(u: XU) -> SV {
    SV::new(u.x, u.y, u.z)
}
fn xplane(p: V, n: V) -> Option<xpl::Plane> {
    Some(xpl::Plane::new(xpt(p), unit(n)?))
}

// ---- planes ------------------------------------------------------------------------------

diff!(
    gu_plane_point_distance,
    |p: V, n: V, q: V| xplane(p, n).map(|pl| pl.distance_to(xpt(q))),
    unit(n).and_then(|u| s1::plane_point_distance(sv(p), su(u), sv(q)).ok())
);
diff!(
    gu_intersect_three_planes,
    pin_defect = crate::pin_defects::gu_intersect_three_planes,
    |a: V, m: V, b: V, n: V, c: V, o: V| xplane(a, m)
        .zip(xplane(b, n))
        .zip(xplane(c, o))
        .map(|((x, y), z)| xpl::intersect_three_planes(&x, &y, &z)),
    unit(m)
        .zip(unit(n))
        .zip(unit(o))
        .map(
            |((m, n), o)| s1::intersect_three_planes(sv(a), su(m), sv(b), su(n), sv(c), su(o)).ok()
        )
);

fn xtwo(r: xpl::PlanesIntersection) -> (&'static str, Vec<f64>) {
    match r {
        xpl::PlanesIntersection::Line { point, direction } => (
            "Line",
            vec![
                point.x,
                point.y,
                point.z,
                direction.x,
                direction.y,
                direction.z,
            ],
        ),
        xpl::PlanesIntersection::Parallel { coincident } => {
            if coincident {
                ("Coincident", vec![])
            } else {
                ("Parallel", vec![])
            }
        }
    }
}
fn stwo(r: s2::PlanesIntersection) -> (&'static str, Vec<f64>) {
    match r {
        s2::PlanesIntersection::Line { point, direction } => (
            "Line",
            vec![
                point.x,
                point.y,
                point.z,
                direction.x,
                direction.y,
                direction.z,
            ],
        ),
        s2::PlanesIntersection::Parallel { coincident } => {
            if coincident {
                ("Coincident", vec![])
            } else {
                ("Parallel", vec![])
            }
        }
    }
}
diff!(
    gu_intersect_two_planes,
    pin_defect = crate::pin_defects::gu_intersect_two_planes,
    |a: V, m: V, b: V, n: V| xplane(a, m)
        .zip(xplane(b, n))
        .map(|(x, y)| xpl::intersect_two_planes(&x, &y).map(xtwo)),
    unit(m).zip(unit(n)).map(
        |(m, n)| s2::intersect_two_planes(sv(a), su(m), sv(b), su(n))
            .ok()
            .map(stwo)
    )
);
diff!(
    gu_point_project_to_plane,
    pin_defect = crate::pin_defects::gu_point_project_to_plane,
    |q: V, p: V, n: V| xpl::point_project_to_plane(xpt(q), xpt(p), xv(n)),
    Some(s1::point_project_to_plane(sv(q), sv(p), sv(n)))
);

// ---- lines and segments ------------------------------------------------------------------

diff!(
    gu_line_closest_point,
    pin_defect = crate::pin_defects::gu_line_closest_point,
    |p: V, v: V, q: V| xl::line_closest_point(xpt(p), xv(v), xpt(q)),
    Some(s1::line_closest_point(sv(p), sv(v), sv(q)))
);
diff!(
    gu_line_point_distance,
    pin_defect = crate::pin_defects::gu_line_point_distance,
    |p: V, v: V, q: V| xl::line_point_distance(xpt(p), xv(v), xpt(q)),
    Some(s1::line_point_distance(sv(p), sv(v), sv(q)))
);
diff!(
    gu_line_line_closest_point,
    pin_defect = crate::pin_defects::gu_line_line_closest_point,
    |p: V, v: V, q: V, w: V| xl::line_line_closest_point(xpt(p), xv(v), xpt(q), xv(w)),
    s1::line_line_closest_point(sv(p), sv(v), sv(q), sv(w)).ok()
);
diff!(
    gu_line_plane_intersect,
    pin_defect = crate::pin_defects::gu_line_plane_intersect,
    |p: V, v: V, q: V, n: V| xl::line_plane_intersect(xpt(p), xv(v), xpt(q), xv(n)),
    s1::line_plane_intersect(sv(p), sv(v), sv(q), sv(n)).ok()
);
diff!(
    gu_line_plane_intersect_uv,
    pin_defect = crate::pin_defects::gu_line_plane_intersect_uv,
    |p: V, v: V, q: V, a: V, b: V| xl::line_plane_intersect_uv(xpt(p), xv(v), xpt(q), xv(a), xv(b)),
    s1::line_plane_intersect_uv(sv(p), sv(v), sv(q), sv(a), sv(b)).ok()
);
diff!(
    gu_segment_point,
    pin_defect = crate::pin_defects::gu_segment_point,
    |a: V, b: V, q: V| {
        let (ang, t) = xl::segment_point_angle_to_perp(xpt(a), xpt(b), xpt(q));
        (
            xl::segment_point_distance(xpt(a), xpt(b), xpt(q)),
            (scx_num::math::rad_to_deg(ang), t),
        )
    },
    (
        s1::segment_point_distance(sv(a), sv(b), sv(q)),
        s1::segment_point_angle_deg_to_perp(sv(a), sv(b), sv(q)),
    )
);

// ---- polygons and triangles --------------------------------------------------------------

diff!(
    gu_polygon_area,
    pin_defect = crate::pin_defects::gu_polygon_area,
    |pts: List<V>, n: V| unit(n)
        .map(|u| xpg::polygon_area(&pts.0.iter().map(|&p| xpt(p)).collect::<Vec<_>>(), u)),
    unit(n).and_then(|u| s1::polygon_area(
        &pts.0.iter().map(|&p| sv(p)).collect::<Vec<_>>(),
        su(u)
    )
    .ok())
);
diff!(
    gu_triangle_area_squared,
    |a: V, b: V, c: V| xpg::triangle_area_squared(xpt(a), xpt(b), xpt(c)),
    s1::triangle_area_squared(sv(a), sv(b), sv(c))
);
diff!(
    gu_triangle_decomposition,
    pin_defect = crate::pin_defects::gu_triangle_decomposition,
    |b: f64, l: f64, r: f64| xpg::triangle_decomposition(b, l, r),
    s1::triangle_decomposition(b, l, r).ok()
);
diff!(
    gu_parallelogram_cross_vectors,
    pin_defect = crate::pin_defects::gu_parallelogram_cross_vectors,
    |a: V, b: V, h: f64| xpg::parallelogram_cross_vectors(xv(a), xv(b), h, 2.0 * h),
    s1::parallelogram_cross_vectors(sv(a), sv(b), h, 2.0 * h).ok()
);

// ---- best fits ---------------------------------------------------------------------------

/// A point set: empty, single, coincident, along a line or in a plane with and without
/// noise, bent to one side of its plane, and random clouds.
#[derive(Clone, Debug)]
pub struct Pts(pub Vec<V>);

impl Gen for Pts {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = vec![
            Pts(vec![]),
            Pts(vec![[1.0, 2.0, 3.0]]),
            Pts(vec![[1.0, 2.0, 3.0]; 4]),
            Pts(vec![[0.0; 3], [1.0, 0.0, 0.0]]),
            // a bent strip: every point but the ends below the chord plane
            Pts(vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, -0.3],
                [2.0, 0.0, -0.4],
                [3.0, 0.0, -0.3],
                [4.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [4.0, 1.0, 0.0],
            ]),
        ];
        let mut r = |s: f64| rng.uniform(-s, s);
        for k in 0..60 {
            let n = 2 + k % 9;
            let noise = [0.0, 1e-12, 1e-7, 1e-3, 0.1][k % 5];
            let o = [r(10.0), r(10.0), r(10.0)];
            let a = [r(1.0), r(1.0), r(1.0)];
            let b = [r(1.0), r(1.0), r(1.0)];
            let c = [r(1.0), r(1.0), r(1.0)];
            let dims = k % 4; // 0 point, 1 line, 2 plane, 3 cloud
            let pts = (0..n)
                .map(|_| {
                    let (s, t, u) = (r(5.0), r(5.0), r(5.0));
                    let mut p = o;
                    for i in 0..3 {
                        let along = match dims {
                            0 => 0.0,
                            1 => s * a[i],
                            2 => s * a[i] + t * b[i],
                            _ => s * a[i] + t * b[i] + u * c[i],
                        };
                        p[i] += along + r(noise);
                    }
                    p
                })
                .collect();
            v.push(Pts(pts));
        }
        // a bowl: a plane with every point but three lifted to one side
        for k in 0..12 {
            let lift = [1e-9, 1e-6, 1e-3, 0.5][k % 4];
            let mut pts = vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]];
            for _ in 0..(3 + k) {
                pts.push([r(5.0) + 5.0, r(5.0) + 5.0, lift * (1.0 + r(0.5))]);
            }
            v.push(Pts(pts));
        }
        v
    }
}

fn xtype(t: xfit::PointSetType) -> &'static str {
    match t {
        xfit::PointSetType::Void => "Void",
        xfit::PointSetType::PointSized => "PointSized",
        xfit::PointSetType::Linear => "Linear",
        xfit::PointSetType::Planar => "Planar",
        xfit::PointSetType::Scattered => "Scattered",
    }
}
fn stype(t: s1::PointSetType) -> &'static str {
    match t {
        s1::PointSetType::Unknown => "Unknown",
        s1::PointSetType::Void => "Void",
        s1::PointSetType::PointSized => "PointSized",
        s1::PointSetType::Linear => "Linear",
        s1::PointSetType::Planar => "Planar",
        s1::PointSetType::Scattered => "Scattered",
    }
}
/// The pin's zero direction ("none fitted") is scx's `None`.
fn sdir(v: SV) -> Option<[f64; 3]> {
    (v != SV::ZERO).then_some([v.x, v.y, v.z])
}
fn xdir(u: Option<XU>) -> Option<[f64; 3]> {
    u.map(|u| [u.x, u.y, u.z])
}

/// What a fit answers: (type, point, direction, the gap to the fitted thing, the gap to
/// the centroid).
pub type FitOut = (&'static str, XP, Option<[f64; 3]>, f64, f64);

diff!(
    gu_best_fit_point_line,
    |pts: Pts, tol: f64| {
        let p: Vec<XP> = pts.0.iter().map(|&q| xpt(q)).collect();
        let f = xfit::best_fit_point(&p, tol);
        (
            (xtype(f.set_type), f.center, f.max_gap),
            xfit::best_fit_line(&p, tol).ok().map(|l| {
                (
                    xtype(l.set_type),
                    l.point,
                    xdir(l.direction),
                    l.max_line_gap,
                    l.max_centroid_gap,
                )
            }),
        )
    },
    {
        let p: Vec<SV> = pts.0.iter().map(|&q| sv(q)).collect();
        let f = s1::best_fit_point_to_points(&p, tol);
        (
            (stype(f.set_type), f.center, f.max_gap),
            s1::best_fit_line_to_points(&p, tol).ok().map(|l| {
                (
                    stype(l.set_type),
                    l.line_pt,
                    sdir(l.line_vec),
                    l.max_line_gap,
                    l.max_centroid_gap,
                )
            }),
        )
    }
);
diff!(
    gu_best_fit_plane,
    pin_defect = crate::pin_defects::gu_best_fit_plane,
    |pts: Pts, tol: f64| {
        let p: Vec<XP> = pts.0.iter().map(|&q| xpt(q)).collect();
        xfit::best_fit_plane(&p, tol).ok().map(|f| -> FitOut {
            (
                xtype(f.set_type),
                f.point,
                xdir(f.direction),
                f.max_plane_gap,
                f.max_centroid_gap,
            )
        })
    },
    {
        let p: Vec<SV> = pts.0.iter().map(|&q| sv(q)).collect();
        s2::best_fit_plane_to_points(&p, tol).ok().map(|f| {
            (
                stype(f.set_type),
                f.point,
                sdir(f.normal),
                f.max_plane_gap,
                f.max_centroid_gap,
            )
        })
    }
);

// ---- sorting -----------------------------------------------------------------------------

diff!(
    gu_sort,
    pin_defect = crate::pin_defects::gu_sort,
    |vals: List<f64>| {
        let mut v = vals.0.clone();
        xs::sort(&mut v);
        v
    },
    {
        let mut v = vals.0.clone();
        s1::shell_sort_double_array(&mut v);
        v
    }
);
diff!(
    gu_insert_sorted,
    |vals: List<f64>, up: bool| {
        let order = if up {
            xs::Order::Increasing
        } else {
            xs::Order::Decreasing
        };
        let mut s = Vec::new();
        for (i, &v) in vals.0.iter().enumerate() {
            xs::insert_sorted(&mut s, v, i as u64, order);
        }
        s.into_iter().unzip::<f64, u64, Vec<f64>, Vec<u64>>()
    },
    {
        let (mut v, mut idx) = (Vec::new(), Vec::new());
        for (i, &x) in vals.0.iter().enumerate() {
            s1::add_sorted(x, i as u64, up, &mut v, Some(&mut idx));
        }
        (v, idx)
    }
);
