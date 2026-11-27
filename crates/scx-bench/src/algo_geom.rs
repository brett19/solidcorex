// Written for solidcorex. Apache-2.0.
//! scx-algo's geometry helpers, pseudo boxes, sort and point sets against the pin (part of
//! [`crate::algo`]'s cases).
//!
//! The cases: a line's closest parameter (`line_closest_point`), a point projected to a
//! plane (`point_project_to_plane`), a line through a plane (`line_plane_intersect`), two
//! planes' line (`intersect_two_planes`); a 16-point plane fit (`best_fit_plane`); sorting
//! 64 values (`sort`); a pseudo box bounding 16 points (`pbox_bound`), and a point's
//! classification and distance against one (`pbox_point`); a 16-point set built and its
//! tight box taken (`point_set`), and a 12-point loop's containment of a point
//! (`point_loop`). The four line and plane helpers have edge inputs too.

use crate::case::{Case, both, same};
use crate::{Rng, edges, seeded};

use scx_algo::geom::{fit as xfit, line as xl, plane as xpl};
use scx_algo::point_set as xs;
use scx_algo::pseudo_box as xp;
use scx_algo::sort as xsort;
use scx_num::fmath;
use scx_num::vector3d::{Point3 as XP, Vec3 as XV};
use solidcore::geom::geom_utility::{p1 as s1, p2 as s2};
use solidcore::geom::point_set as ss;
use solidcore::geom::pseudo_box as sp;
use solidcore::geom::vector3d::Vec3 as SV;

const NAN: f64 = f64::NAN;
const INF: f64 = f64::INFINITY;

type V = [f64; 3];

fn xv(a: V) -> XV {
    XV::new(a[0], a[1], a[2])
}
fn xpt(a: V) -> XP {
    XP::new(a[0], a[1], a[2])
}
fn sv(a: V) -> SV {
    SV::new(a[0], a[1], a[2])
}

/// A point in `[-10, 10]^3`.
fn point(r: &mut Rng) -> V {
    r.array(-10.0, 10.0)
}
/// A direction in `[-1, 1]^3`, scaled by up to 4.
fn dir(r: &mut Rng) -> V {
    let s = r.range(0.25, 4.0);
    r.array::<3>(-1.0, 1.0).map(|c| c * s)
}
/// A unit vector.
fn unit(r: &mut Rng) -> V {
    loop {
        let v: V = r.array(-1.0, 1.0);
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 0.1 {
            return v.map(|c| c / l);
        }
    }
}
/// `n` points about a random plane, off it by up to `noise`.
fn plane_points(r: &mut Rng, n: usize, noise: f64) -> Vec<V> {
    let o = point(r);
    let (a, b, c) = (unit(r), unit(r), unit(r));
    (0..n)
        .map(|_| {
            let (s, t, u) = (
                r.range(-5.0, 5.0),
                r.range(-5.0, 5.0),
                r.range(-noise, noise),
            );
            core::array::from_fn(|i| o[i] + s * a[i] + t * b[i] + u * c[i])
        })
        .collect()
}

/// Zero, tiny, huge and NaN vectors and points among ordinary ones.
fn vec_edges() -> Vec<V> {
    vec![
        [0.0; 3],
        [1e-300, 0.0, 0.0],
        [1e300, 1e300, 0.0],
        [NAN, 1.0, 0.0],
        [INF, 0.0, 0.0],
        [1.0, 2.0, 3.0],
        [0.0, 0.0, 1.0],
    ]
}
/// Every pairing of [`vec_edges`] taken `k` at a time, cycled.
fn edge_tuples<const K: usize>() -> Vec<[V; K]> {
    let e = vec_edges();
    let n = e.len();
    (0..n.pow(K as u32))
        .map(|mut i| {
            core::array::from_fn(|_| {
                let v = e[i % n];
                i /= n;
                v
            })
        })
        .collect()
}

fn helpers(out: &mut Vec<Case>) {
    // [line point, line vector, test point]
    let lcp = seeded(41, |r| [point(r), dir(r), point(r)]);
    let lcp_e = edges(42, &edge_tuples::<3>());
    for (name, v) in [
        ("algo/line_closest_point", &lcp),
        ("algo/line_closest_point/edge", &lcp_e),
    ] {
        out.push(Case::map(
            name,
            same(v),
            |&[p, d, q]| xl::line_closest_point(xpt(p), xv(d), xpt(q)),
            |&[p, d, q]| s1::line_closest_point(sv(p), sv(d), sv(q)),
        ));
    }
    // [point, plane point, plane normal]
    let ppp = seeded(43, |r| [point(r), point(r), dir(r)]);
    let ppp_e = edges(44, &edge_tuples::<3>());
    for (name, v) in [
        ("algo/point_project_to_plane", &ppp),
        ("algo/point_project_to_plane/edge", &ppp_e),
    ] {
        out.push(Case::map(
            name,
            same(v),
            |&[q, p, n]| xpl::point_project_to_plane(xpt(q), xpt(p), xv(n)),
            |&[q, p, n]| s1::point_project_to_plane(sv(q), sv(p), sv(n)),
        ));
    }
    // [line point, line vector, plane point, plane normal]
    let lpi = seeded(45, |r| [point(r), dir(r), point(r), dir(r)]);
    let lpi_e = edges(46, &edge_tuples::<4>());
    for (name, v) in [
        ("algo/line_plane_intersect", &lpi),
        ("algo/line_plane_intersect/edge", &lpi_e),
    ] {
        out.push(Case::map(
            name,
            same(v),
            |&[p, d, q, n]| xl::line_plane_intersect(xpt(p), xv(d), xpt(q), xv(n)),
            |&[p, d, q, n]| s1::line_plane_intersect(sv(p), sv(d), sv(q), sv(n)),
        ));
    }
    // [plane point, unit normal] twice; each side takes its own unit type
    let itp = seeded(47, |r| [point(r), unit(r), point(r), unit(r)]);
    // only the tuples whose normals unitize: each plane needs one
    let unitizes = |v: &V| xv(*v).normalized().is_some();
    let itp_e: Vec<[V; 4]> = edge_tuples::<4>()
        .into_iter()
        .filter(|[_, n, _, m]| unitizes(n) && unitizes(m))
        .collect();
    let itp_e = edges(48, &itp_e);
    for (name, v) in [
        ("algo/intersect_two_planes", &itp),
        ("algo/intersect_two_planes/edge", &itp_e),
    ] {
        out.push(Case::map(
            name,
            both(
                v,
                |&[p, n, q, m]| {
                    Some((
                        xpl::Plane::new(xpt(p), xv(n).normalized()?),
                        xpl::Plane::new(xpt(q), xv(m).normalized()?),
                    ))
                },
                |&[p, n, q, m]| {
                    // the pin's normals are the same unit vectors scx holds
                    let (a, b) = (xv(n).normalized()?, xv(m).normalized()?);
                    Some((sv(p), sv(a.to_array()), sv(q), sv(b.to_array())))
                },
            ),
            |(a, b)| match xpl::intersect_two_planes(a, b) {
                Some(xpl::PlanesIntersection::Line { point, direction }) => {
                    Some((point.to_array(), Some(direction.to_array())))
                }
                Some(xpl::PlanesIntersection::Parallel { coincident }) => {
                    Some(([f64::from(u8::from(coincident)); 3], None))
                }
                None => None,
            },
            |&(p, n, q, m)| match s2::intersect_two_planes(p, n, q, m) {
                Ok(s2::PlanesIntersection::Line { point, direction }) => Some((
                    [point.x, point.y, point.z],
                    Some([direction.x, direction.y, direction.z]),
                )),
                Ok(s2::PlanesIntersection::Parallel { coincident }) => {
                    Some(([f64::from(u8::from(coincident)); 3], None))
                }
                Err(_) => None,
            },
        ));
    }
}

/// 16 points about a plane, off it by up to 0 (exact), 1e-9 or 1e-3 (a third each).
fn fit_sets(seed: u64, n: usize) -> Vec<Vec<V>> {
    seeded(seed, |r| {
        let noise = [0.0, 1e-9, 1e-3][r.below(3)];
        plane_points(r, n, noise)
    })
}

/// The fit tolerance: between the two noisy levels.
const FIT_TOL: f64 = 1e-6;

fn fits(out: &mut Vec<Case>) {
    let pts = fit_sets(51, 16);
    out.push(Case::map(
        "algo/best_fit_plane",
        both(
            &pts,
            |p| Some(p.iter().map(|&q| xpt(q)).collect::<Vec<_>>()),
            |p| Some(p.iter().map(|&q| sv(q)).collect::<Vec<_>>()),
        ),
        |p| {
            xfit::best_fit_plane(p, FIT_TOL).ok().map(|f| {
                (
                    (f.point.to_array(), f.direction.map(|d| d.to_array())),
                    (f.max_plane_gap, f.max_centroid_gap),
                )
            })
        },
        |p| {
            s2::best_fit_plane_to_points(p, FIT_TOL).ok().map(|f| {
                (
                    (
                        [f.point.x, f.point.y, f.point.z],
                        Some([f.normal.x, f.normal.y, f.normal.z]),
                    ),
                    (f.max_plane_gap, f.max_centroid_gap),
                )
            })
        },
    ));
}

fn sorts(out: &mut Vec<Case>) {
    let vals = seeded(53, |r| {
        (0..64).map(|_| r.range(-1e3, 1e3)).collect::<Vec<f64>>()
    });
    out.push(Case::map(
        "algo/sort",
        same(&vals),
        |v| {
            let mut v = v.clone();
            xsort::sort(&mut v);
            v
        },
        |v| {
            let mut v = v.clone();
            s1::shell_sort_double_array(&mut v);
            v
        },
    ));
}

/// The pin's box over `points` in the frame of `b` (its statuses ignored, as the pin's
/// own callers do).
fn spbox(b: &[V; 3], points: &[SV]) -> sp::PseudoBox {
    let mut pb = sp::PseudoBox::new();
    let _ = pb.set_basis(sv(b[0]), sv(b[1]), sv(b[2]));
    pb.init();
    for &p in points {
        pb.add_point(p);
    }
    pb
}
/// A random orthonormal frame.
fn frame(r: &mut Rng) -> [V; 3] {
    let a = unit(r);
    let t = unit(r);
    let c = [
        a[1] * t[2] - a[2] * t[1],
        a[2] * t[0] - a[0] * t[2],
        a[0] * t[1] - a[1] * t[0],
    ];
    let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    let c = c.map(|x| x / l);
    let b = [
        c[1] * a[2] - c[2] * a[1],
        c[2] * a[0] - c[0] * a[2],
        c[0] * a[1] - c[1] * a[0],
    ];
    [a, b, c]
}

fn boxes(out: &mut Vec<Case>) {
    // [frame, 16 points]
    let bound = seeded(55, |r| {
        (frame(r), (0..16).map(|_| point(r)).collect::<Vec<V>>())
    });
    out.push(Case::map(
        "algo/pbox_bound",
        both(
            &bound,
            |(f, p)| {
                let basis = xp::BoxBasis::new(xv(f[0]), xv(f[1]), xv(f[2]))?;
                Some((basis, p.iter().map(|&q| xpt(q)).collect::<Vec<_>>()))
            },
            |(f, p)| Some((*f, p.iter().map(|&q| sv(q)).collect::<Vec<_>>())),
        ),
        |(basis, p)| xp::PseudoBox::bounding(*basis, p.iter().copied()).map(|b| *b.intervals()),
        |(f, p)| *spbox(f, p).intervals(),
    ));
    // [frame, 16 points, test point]: the box built outside the timing
    let pq = seeded(57, |r| {
        (
            frame(r),
            (0..16).map(|_| point(r)).collect::<Vec<V>>(),
            point(r),
        )
    });
    out.push(Case::map(
        "algo/pbox_point",
        both(
            &pq,
            |(f, p, q)| {
                let basis = xp::BoxBasis::new(xv(f[0]), xv(f[1]), xv(f[2]))?;
                let b = xp::PseudoBox::bounding(basis, p.iter().map(|&q| xpt(q)))?;
                Some((b, xpt(*q)))
            },
            |(f, p, q)| {
                let pts: Vec<SV> = p.iter().map(|&q| sv(q)).collect();
                Some((spbox(f, &pts), sv(*q)))
            },
        ),
        |(b, q)| {
            let c = b.classify_point(*q, 1e-9);
            (
                c.map(|t| t as u64),
                (b.contains_point(*q, 1e-9), b.distance_to_point(*q)),
            )
        },
        |(b, q)| {
            let c = b.classify_point(*q, 1e-9);
            (
                c.map(|t| t as u64),
                (b.contains_point(*q, 1e-9), b.distance_to_point(*q)),
            )
        },
    ));
}

fn sets(out: &mut Vec<Case>) {
    let pts = fit_sets(61, 16);
    out.push(Case::map(
        "algo/point_set",
        both(
            &pts,
            |p| Some(p.iter().map(|&q| xpt(q)).collect::<Vec<_>>()),
            |p| Some(p.iter().map(|&q| sv(q)).collect::<Vec<_>>()),
        ),
        |p| {
            let s = xs::PointSet::new(p.clone(), FIT_TOL).ok()?;
            let b = s.tight_pbox()?;
            Some((
                (s.centroid().to_array(), s.normal().to_array()),
                (b.basis().vectors(), *b.intervals()),
            ))
        },
        |p| {
            let mut s = ss::PointSet3d::new(p.clone(), FIT_TOL, None);
            let n = s.normal().ok()?;
            let c = s.centroid();
            let b = s.tight_pbox(None).ok()?;
            Some((
                ([c.x, c.y, c.z], [n.x, n.y, n.z]),
                (*b.basis(), *b.intervals()),
            ))
        },
    ));
    // a 12-point star-shaped loop in a random plane, and a test point near it: the loop
    // built outside the timing (each side's fit and boxes), its containment timed
    let loops = seeded(63, |r| {
        let [a, b, _] = frame(r);
        let o = point(r);
        let pts: Vec<V> = (0..12)
            .map(|k| {
                let t = core::f64::consts::TAU * f64::from(k) / 12.0;
                let rad = r.range(2.0, 5.0);
                core::array::from_fn(|i| o[i] + rad * (fmath::cos(t) * a[i] + fmath::sin(t) * b[i]))
            })
            .collect();
        let (s, t) = (r.range(-5.0, 5.0), r.range(-5.0, 5.0));
        let q: V = core::array::from_fn(|i| o[i] + s * a[i] + t * b[i]);
        (pts, q)
    });
    out.push(Case::map(
        "algo/point_loop",
        both(
            &loops,
            |(p, q)| {
                let s = xs::PointSet::new(p.iter().map(|&v| xpt(v)).collect(), FIT_TOL).ok()?;
                Some((xs::PointLoop::new(s), xpt(*q)))
            },
            |(p, q)| {
                let mut s =
                    ss::PointSequence::new(p.iter().map(|&v| sv(v)).collect(), FIT_TOL, true, None);
                // its cached fit and box, as scx's are built
                s.bbox(None).ok()?;
                // the pin's query takes `&mut` (its fit cache, filled just above): an
                // uncontended lock per call, a few ns against a whole winding count
                Some((std::sync::Mutex::new(s), sv(*q)))
            },
        ),
        |(lp, q)| lp.containment(*q) as u64,
        |(lp, q)| {
            lp.lock()
                .ok()
                .and_then(|mut l| l.point_containment(*q).ok())
                .map_or(u64::MAX, |c| c as u64)
        },
    ));
}

/// The cases, in the order [`crate::algo::cases`] lists them.
pub fn cases(out: &mut Vec<Case>) {
    helpers(out);
    fits(out);
    sorts(out);
    boxes(out);
    sets(out);
}
