// Written for solidcorex. Apache-2.0.
//! `extent1d.rs`, `extent2d.rs`, `extent3d.rs`, `periodic_extent1d.rs`.

use crate::diff;
use crate::harness::List;

type I = [f64; 2];
type R = [f64; 4];
type B = [f64; 6];
type V = [f64; 3];
type P = [f64; 2];

// --- Extent1d
diff!(e1_constructors, |a: f64, b: f64| (
    m::extent1d::Extent1d::new(),
    m::extent1d::Extent1d::from_value(a),
    m::extent1d::Extent1d::from_min_max(a, b),
    m::extent1d::Extent1d::from_raw(a, b),
    <m::extent1d::Extent1d as Default>::default()
));
diff!(e1_init_unbounded, |a: I| {
    let (mut p, mut q) = (m::e1(a), m::e1(a));
    p.init();
    q.set_unbounded();
    (p, q)
});
diff!(e1_queries, |a: I| {
    let e = m::e1(a);
    (
        (e.is_defined(), e.mid(), e.length(), e.max_dimension()),
        (
            e.has_negative_length(),
            e.is_init(),
            e.boundary_type(),
            e.is_bounded(),
        ),
    )
});
diff!(e1_negate_translate_scale, |a: I, v: f64| {
    let (mut p, mut q, mut r, mut s, mut t) = (m::e1(a), m::e1(a), m::e1(a), m::e1(a), m::e1(a));
    p.negate();
    q.translate(v);
    r.scale(v);
    s.expand_absolute(v);
    t.expand_relative(v);
    [p, q, r, s, t]
});
diff!(e1_value_ops, |a: I, v: f64| {
    let e = m::e1(a);
    let mut g = e;
    g.add_value(v);
    (
        (
            e.t_left_eval(v),
            e.clamp_value(v),
            e.evaluate(v),
            e.distance_from(v),
        ),
        (g, e.inversion(v), e.periodic_wrap(v), e.is_closed(v)),
    )
});
diff!(e1_snap_value, |a: I, v: f64, t: f64| m::e1(a)
    .snap_value(v, t));
diff!(e1_setters, |a: I, x: f64, y: f64| {
    let (mut p, mut q, mut r) = (m::e1(a), m::e1(a), m::e1(a));
    (
        (p.set_min_max(x, y), p),
        (q.set_min(x), q),
        (r.set_max(y), r),
    )
});
diff!(e1_pair, |a: I, b: I| {
    let (e, f) = (m::e1(a), m::e1(b));
    (e.union(&f), e.intersect(&f), e.are_same(&f))
});
diff!(e1_pair_tol, |a: I, b: I, t: f64| {
    let (e, f) = (m::e1(a), m::e1(b));
    (
        e.are_disjoint(&f, t),
        e.are_equal(&f, t),
        e.is_contained_by(&f, t),
    )
});
diff!(e1_are_equal_bounds, |a: I, x: f64, y: f64, t: f64| m::e1(a)
    .are_equal_bounds(x, y, t));
diff!(e1_value_tol, |a: I, v: f64, t: f64| {
    let e = m::e1(a);
    (
        e.contains_value(v, t),
        e.is_value_on_boundary(v, t),
        e.classify_point(v, t),
        e.is_degenerate(t),
    )
});
diff!(e1_approximate_unbounded, |a: I, c: f64, h: f64| {
    let e = m::e1(a);
    (
        e.approximate_unbounded(c, h),
        e.approximate_unbounded_expanded(c, h),
    )
});
diff!(e1_are_disjoint_periodic, |a: I, b: I, p: f64, t: f64| {
    m::e1(a).are_disjoint_periodic(&m::e1(b), p, t)
});
diff!(e1_intersect_periodic, |a: I, b: I, p: f64, d: bool| m::e1(
    a
)
.intersect_periodic(&m::e1(b), p, d));
diff!(e1_periodic_value, |a: I, v: f64, p: f64, t: f64| {
    let e = m::e1(a);
    (
        e.clamp_periodic_value(v, p, t),
        e.contains_periodic_value(v, p, t),
        e.is_value_on_periodic_boundary(v, p, t),
    )
});
diff!(e1_codes, |a: I, v: f64, t: f64| {
    let e = m::e1(a);
    (
        e.classify_point(v, t).code(),
        e.boundary_type().code(),
        e.inversion(v).is_success(),
    )
});
diff!(end_new_get, |d: usize, i: usize, a: I| {
    let mut n = m::extent1d::ExtentNd::new(d);
    if let Some(e) = n.get_mut(i) {
        *e = m::e1(a);
    }
    (n.get(i), n)
});
diff!(end_clamp_vector, |a: I,
                         b: I,
                         vals: List<f64>,
                         per: Option<bool>,
                         counts: bool| {
    let mut n = m::extent1d::ExtentNd::new(2);
    *n.get_mut(0).unwrap() = m::e1(a);
    *n.get_mut(1).unwrap() = m::e1(b);
    let flags = per.map(|p| vec![p; vals.0.len()]);
    let mut out = Vec::new();
    let mut c = vec![0u64; 2];
    let r = n.clamp_vector(
        &vals.0,
        flags.as_deref(),
        &mut out,
        counts.then_some(&mut c),
    );
    (r, out, c)
});

// --- Extent2d
diff!(e2_constructors, |a: P, b: P| (
    m::extent2d::Extent2d::new(),
    m::extent2d::Extent2d::from_point(m::v2(a)),
    m::extent2d::Extent2d::from_points(m::v2(a), m::v2(b)),
    <m::extent2d::Extent2d as Default>::default()
));
diff!(e2_from_extent3d, |b: B| {
    let e = m::e3(b);
    (
        m::extent2d::Extent2d::from_extent3d(&e),
        m::extent2d::Extent2d::from(e),
    )
});
diff!(e2_init_transpose, |a: R| {
    let (mut p, mut q) = (m::e2(a), m::e2(a));
    p.init();
    q.transpose();
    (p, q)
});
diff!(e2_queries, |a: R| {
    let e = m::e2(a);
    (
        (e.mid(), e.u_min(), e.u_max(), e.v_min(), e.v_max()),
        (e.u_interval(), e.v_interval(), e.max_dimension(), e.size()),
        (
            e.x_length(),
            e.y_length(),
            e.has_negative_area(),
            e.is_init(),
        ),
        (e.boundary_types(), e.is_bounded()),
    )
});
diff!(e2_add_point, |a: R, p: P| {
    let mut e = m::e2(a);
    e.add_point(m::v2(p));
    e
});
diff!(e2_set_min_max, |a: R, p: P, q: P| {
    let (mut e, mut f) = (m::e2(a), m::e2(a));
    (
        (e.set_min_max(m::v2(p), m::v2(q)), e),
        (f.set_min_max_coords(p[0], p[1], q[0], q[1]), f),
    )
});
diff!(e2_set_intervals, |a: R, i: I| {
    let (mut e, mut f) = (m::e2(a), m::e2(a));
    e.set_u_interval(&m::e1(i));
    f.set_v_interval(&m::e1(i));
    (e, f)
});
diff!(e2_set_bounds, |a: R, x: f64| {
    let (mut p, mut q, mut r, mut s) = (m::e2(a), m::e2(a), m::e2(a), m::e2(a));
    (
        (p.set_u_min(x), p),
        (q.set_u_max(x), q),
        (r.set_v_min(x), r),
        (s.set_v_max(x), s),
    )
});
diff!(e2_expand_scale, |a: R, d: f64, w: Option<f64>| {
    let (mut p, mut q, mut r) = (m::e2(a), m::e2(a), m::e2(a));
    p.expand_absolute(d);
    q.expand_relative(d);
    r.scale(d, w);
    [p, q, r]
});
diff!(e2_expand_absolute_vec, |a: R, d: P| {
    let mut e = m::e2(a);
    e.expand_absolute_vec(m::v2(d));
    e
});
diff!(e2_pair, |a: R, b: R| {
    let (e, f) = (m::e2(a), m::e2(b));
    (
        e.are_same(&f),
        e.union(&f),
        e.intersect(&f),
        e.max_boundary_dist(&f),
    )
});
diff!(e2_pair_tol, |a: R, b: R, t: f64| {
    let (e, f) = (m::e2(a), m::e2(b));
    (
        e.are_disjoint(&f, t),
        e.are_equal(&f, t),
        e.is_contained_by(&f, t),
        e.is_degenerate(t),
    )
});
diff!(e2_point_tol, |a: R, p: P, t: f64| {
    let (e, p) = (m::e2(a), m::v2(p));
    (
        (
            e.contains_point(p, t),
            e.contains_point_dist(p, t),
            e.is_point_on_boundary(p, t),
        ),
        (e.point_boundaries(p, t), e.inversion(p, t)),
    )
});
diff!(e2_classify_point, |a: R, p: P, t: f64, w: Option<f64>| {
    m::e2(a).classify_point(m::v2(p), t, w)
});
diff!(
    e2_contains_periodic_point,
    |a: R, p: P, u: f64, v: f64, t: f64| m::e2(a).contains_periodic_point(m::v2(p), u, v, t)
);
diff!(e2_evaluate, |a: R, x: f64, y: f64| m::e2(a).evaluate(x, y));
diff!(e2_clamp_point, |a: R, p: P| m::e2(a).clamp_point(m::v2(p)));
diff!(e2_clip_line, |a: R, p: P, v: P, t: f64| m::e2(a).clip_line(
    m::v2(p),
    m::v2(v),
    t
));
diff!(
    e2_intersect_with_infinite_line,
    |a: R, p: P, v: P, tu: Option<f64>, tv: Option<f64>| {
        m::e2(a).intersect_with_infinite_line(m::v2(p), m::v2(v), tu, tv)
    }
);
diff!(e2_left_eval, |a: R, x: f64| (
    m::e2(a).u_left_eval(x),
    m::e2(a).v_left_eval(x)
));
diff!(e2_point_map_to_domain, |a: R, p: P, b: R, sw: bool| m::e2(
    a
)
.point_map_to_domain(m::v2(p), &m::e2(b), sw));
diff!(e2_consts, || [
    m::extent2d::SS_NONE,
    m::extent2d::SS_UMIN,
    m::extent2d::SS_VMIN,
    m::extent2d::SS_UMAX,
    m::extent2d::SS_VMAX
]);

// --- Extent3d
diff!(e3_constructors, |a: V, b: V| (
    m::extent3d::Extent3d::new(),
    m::extent3d::Extent3d::from_points(m::v3(a), m::v3(b)),
    m::extent3d::Extent3d::from_point(m::v3(a)),
    <m::extent3d::Extent3d as Default>::default()
));
diff!(e3_init_unbounded, |a: B| {
    let (mut p, mut q) = (m::e3(a), m::e3(a));
    p.init();
    q.set_unbounded();
    (p, q)
});
diff!(e3_queries, |a: B| {
    let e = m::e3(a);
    (
        (
            e.is_defined(),
            e.mid(),
            e.u_min(),
            e.u_max(),
            e.v_min(),
            e.v_max(),
        ),
        (
            e.w_min(),
            e.w_max(),
            e.max_length(),
            e.u_interval(),
            e.v_interval(),
        ),
        (e.w_interval(), e.max_dimension(), e.size(), e.corners()),
        (e.edges(), e.planes(), e.x_length(), e.y_length()),
    )
});
diff!(e3_queries2, |a: B| {
    let e = m::e3(a);
    (
        (
            e.z_length(),
            e.sphere_bound(),
            e.has_negative_volume(),
            e.is_init(),
        ),
        (e.boundary_types(), e.is_bounded(), e.any_bounds()),
    )
});
diff!(e3_min_max_at, |a: B, i: usize| (
    m::e3(a).min_at(i),
    m::e3(a).max_at(i)
));
diff!(e3_set_intervals, |a: B, i: I| {
    let (mut e, mut f) = (m::e3(a), m::e3(a));
    e.set_u_interval(&m::e1(i));
    f.set_v_interval(&m::e1(i));
    (e, f)
});
diff!(e3_add_point, |a: B, p: V| {
    let mut e = m::e3(a);
    e.add_point(m::v3(p));
    e
});
diff!(e3_set_min_max, |a: B, p: V, q: V| {
    let (mut e, mut f) = (m::e3(a), m::e3(a));
    (
        (e.set_min_max(m::v3(p), m::v3(q)), e),
        (f.set_min_max_coords(p[0], p[1], p[2], q[0], q[1], q[2]), f),
    )
});
diff!(e3_set_bounds, |a: B, x: f64| {
    let (mut p, mut q, mut r, mut s) = (m::e3(a), m::e3(a), m::e3(a), m::e3(a));
    (
        (p.set_u_min(x), p),
        (q.set_u_max(x), q),
        (r.set_v_min(x), r),
        (s.set_v_max(x), s),
    )
});
diff!(e3_evaluate, |a: B, x: f64, y: f64, z: f64| m::e3(a)
    .evaluate(x, y, z));
diff!(e3_pair, |a: B, b: B| {
    let (e, f) = (m::e3(a), m::e3(b));
    (
        (e.union(&f), e.intersect(&f), e.max_distance_squared(&f)),
        (
            e.min_distance_squared(&f),
            e.max_distance(&f),
            e.min_distance(&f),
        ),
        e.cone_bound_to(&f),
    )
});
diff!(e3_pair_tol, |a: B, b: B, t: f64| {
    let (e, f) = (m::e3(a), m::e3(b));
    (
        e.are_disjoint(&f, t),
        e.is_contained_by(&f, t),
        e.is_point_sized(t),
    )
});
diff!(e3_point, |a: B, p: V| m::e3(a)
    .max_distance_to_point(m::v3(p)));
diff!(e3_point_tol, |a: B, p: V, t: f64| {
    let e = m::e3(a);
    (
        e.contains_point(m::v3(p), t),
        e.is_point_on_boundary(m::v3(p), t),
    )
});
diff!(e3_line, |a: B, p: V, d: V| {
    let e = m::e3(a);
    (
        e.span_along(m::v3(p), m::v3(d)),
        e.distance_to_plane(m::v3(p), m::v3(d)),
        e.intersect_line(m::v3(p), m::v3(d)),
    )
});
diff!(e3_line_hit_count, |a: B, p: V, d: V| m::e3(a)
    .intersect_line(m::v3(p), m::v3(d))
    .map(m::extent3d::LineHit::count));
diff!(e3_line_tol, |a: B, p: V, d: V, t: f64| {
    let e = m::e3(a);
    (
        e.contains_line_seg(m::v3(p), m::v3(d), t),
        e.meets_ray(m::v3(p), m::v3(d), t),
    )
});
diff!(e3_expand, |a: B, d: f64| {
    let (mut p, mut q) = (m::e3(a), m::e3(a));
    p.expand_relative(d);
    q.expand_absolute(d);
    (p, q)
});
diff!(e3_approximate_unbounded, |a: B, c: Option<V>, h: f64| {
    let e = m::e3(a);
    (
        e.approximate_unbounded(c.map(m::v3), h),
        e.approximate_unbounded_expanded(c.map(m::v3), h),
    )
});

// --- PeriodicExtent1d
diff!(pe_constructors, |a: V, f: bool| (
    m::periodic_extent1d::PeriodicExtent1d::full(a[2]),
    m::pe(a, f)
));
diff!(pe_queries, |a: V, f: bool| {
    let e = m::pe(a, f);
    (e.is_empty(), e.is_full_period(), e.crosses_seam(), e.mid())
});
diff!(pe_set_full_complement, |a: V, f: bool, g: bool| {
    let (mut p, mut q) = (m::pe(a, f), m::pe(a, f));
    p.set_full_period(g);
    q.complement();
    (p, q)
});
diff!(pe_values, |a: V, f: bool, v: f64| {
    let e = m::pe(a, f);
    (e.map_value(v), e.normalize_value(v), e.contains_value(v))
});
diff!(pe_add_interval, |a: V,
                        f: bool,
                        x: f64,
                        y: f64,
                        fw: bool| {
    let mut e = m::pe(a, f);
    (e.add_interval(x, y, fw), e)
});
/// Spans disjoint from the extent, in each of the four seam cases (the random draws
/// rarely give one): `(min, max, period, from, to)`.
const PE_DISJOINT: &[[f64; 5]] = &[
    [2.0, 3.0, core::f64::consts::TAU, 5.0, 1.0],
    [5.0, 1.0, core::f64::consts::TAU, 2.0, 3.0],
    [2.0, 3.0, core::f64::consts::TAU, 4.0, 5.0],
    [5.0, 1.0, core::f64::consts::TAU, 6.0, 4.0],
    [1.0e20, -1.0e20, core::f64::consts::TAU, 1.0, 2.0],
];
diff!(pe_add_interval_disjoint, |i: usize, fw: bool| {
    PE_DISJOINT.get(i).map(|&[a, b, p, x, y]| {
        let mut e = m::pe([a, b, p], false);
        (e.add_interval(x, y, fw), e)
    })
});
diff!(pe_set_min_max, |a: V,
                       f: bool,
                       x: f64,
                       y: f64,
                       p: Option<f64>,
                       sp: bool| {
    let mut e = m::pe(a, f);
    (e.set_min_max(x, y, p, sp), e)
});
diff!(pe_pair, |a: V, f: bool, b: V, g: bool| {
    let (e, h) = (m::pe(a, f), m::pe(b, g));
    (e.is_contained_by(&h), e.are_disjoint(&h))
});
