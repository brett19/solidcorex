// Written for solidcorex. Apache-2.0.
//! `extent1d.rs`, `extent2d.rs`, `extent3d.rs`, `periodic_extent1d.rs`.
//!
//! The diff bodies call scx's API. [`crate::pin_extent`] gives the pin the same names, made of the
//! pin's own calls. An input box that is inverted, or has an end that is not finite or is the
//! pin's `SM_INFINITE_PARAMETER` marker, is `None` on both sides ([`crate::facade::plain_ends`]),
//! since scx holds no inverted extent and no end that is not a finite value. A result with an
//! end that is not finite is `None` on both sides too. A transform that only grows
//! (`expanded`) is given `|d|`; the shrinking direction is `shrunk`.

use crate::diff;
use crate::harness::List;

type I = [f64; 2];
type R = [f64; 4];
type B = [f64; 6];
type V = [f64; 3];
type P = [f64; 2];

// --- Extent1d
diff!(e1_constructors, |a: f64, b: f64| (
    m::extent1d::Extent1d::point(a),
    m::extent1d::Extent1d::spanning(a, b),
    m::e1([a, b]),
    m::extent1d::Hull1d::EMPTY.finish()
));
diff!(
    e1_hull,
    truth_defect = crate::pin_truths::e1_hull,
    |a: f64, b: f64| m::extent1d::Extent1d::hull([a, b])
);
diff!(e1_queries, |a: I| m::e1(a).map(|e| (e.mid(), e.length())));
diff!(e1_max_dimension, |a: I| m::e1(a).map(|e| e.max_dimension()));
diff!(e1_transforms, |a: I, v: f64| m::e1(a).map(|e| (
    (e.negated(), e.translated(v), e.scaled(v)),
    e.shrunk(v.abs()),
)));
diff!(
    e1_expanded,
    truth_defect = crate::pin_truths::e1_expanded,
    |a: I, v: f64| m::e1(a).map(|e| (e.expanded(v.abs()), e.expanded_relative(v.abs())))
);
diff!(e1_value_ops, |a: I, v: f64| m::e1(a).map(|e| (
    (
        e.t_left_eval(v),
        e.clamp_value(v),
        e.evaluate(v),
        e.distance_from(v),
    ),
    (e.inversion(v), e.periodic_wrap(v), e.is_closed(v)),
)));
diff!(
    e1_hull_value,
    truth_defect = crate::pin_truths::e1_hull_value,
    |a: I, v: f64| m::e1(a).map(|e| {
        let mut h = m::extent1d::Hull1d::EMPTY;
        h.add_extent(e);
        let u = h.union([v].into_iter().collect());
        h.add(v);
        (h.finish(), u.finish())
    })
);
diff!(e1_snap_value, |a: I, v: f64, t: f64| m::e1(a)
    .map(|e| e.snap_value(v, t)));
diff!(e1_with_bounds, |a: I, x: f64, y: f64| m::e1(a).map(|e| (
    m::e1([x, y]),
    e.with_min(x),
    e.with_max(y)
)));
diff!(e1_pair, |a: I, b: I| m::e1(a)
    .zip(m::e1(b))
    .map(|(e, f)| e.are_same(&f)));
diff!(e1_union_intersect, |a: I, b: I| m::e1(a)
    .zip(m::e1(b))
    .map(|(e, f)| (e.union(f), e.intersect(f))));
diff!(e1_pair_tol, |a: I, b: I, t: f64| m::e1(a)
    .zip(m::e1(b))
    .map(|(e, f)| (
        e.are_disjoint(&f, t),
        e.are_equal(&f, t),
        e.is_contained_by(&f, t),
    )));
diff!(e1_are_equal_bounds, |a: I, x: f64, y: f64, t: f64| m::e1(a)
    .map(|e| e.are_equal_bounds(x, y, t)));
diff!(e1_value_tol, |a: I, v: f64, t: f64| m::e1(a).map(|e| (
    e.contains_value(v, t),
    e.is_value_on_boundary(v, t),
    e.classify_point(v, t),
    e.is_degenerate(t),
)));
diff!(e1_are_disjoint_periodic, |a: I, b: I, p: f64, t: f64| {
    m::e1(a)
        .zip(m::e1(b))
        .map(|(e, f)| e.are_disjoint_periodic(&f, p, t))
});
diff!(
    e1_intersect_periodic,
    pin_defect = crate::pin_defects::e1_intersect_periodic,
    |a: I, b: I, p: f64, d: bool| {
        let at = if d {
            m::extent1d::AtSeam::Split
        } else {
            m::extent1d::AtSeam::Join
        };
        m::e1(a)
            .zip(m::e1(b))
            .map(|(e, f)| e.intersect_periodic(&f, p, at))
    }
);
diff!(e1_periodic_value, |a: I, v: f64, p: f64, t: f64| m::e1(a)
    .map(|e| (
        e.clamp_periodic_value(v, p, t),
        e.contains_periodic_value(v, p, t),
    )));
diff!(e1_periodic_boundary, |a: I, v: f64, p: f64, t: f64| m::e1(
    a
)
.map(|e| e.is_value_on_periodic_boundary(v, p, t)));
diff!(end_new_get, |d: usize, i: usize, a: I| m::e1(a).map(|e| {
    let mut n = m::end(vec![e; d]);
    if let Some(s) = n.get_mut(i) {
        *s = s.negated();
    }
    (n.get(i), n)
}));
diff!(end_clamp_vector, |a: I,
                         b: I,
                         vals: List<f64>,
                         per: Option<bool>,
                         counts: bool| {
    m::e1(a).zip(m::e1(b)).map(|(e, f)| {
        let n = m::end(vec![e, f]);
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
    })
});

// --- Extent2d
diff!(e2_constructors, |a: P, b: P| (
    m::extent2d::Extent2d::spanning(m::p2(a), m::p2(b)),
    m::extent2d::Extent2d::from_corners(m::p2(a), m::p2(b)),
    m::extent2d::Hull2d::EMPTY.finish()
));
// scx's `from_point` refuses a point that is not finite; the pin's stores any.
diff!(
    e2_from_point,
    |a: P| m::extent2d::Extent2d::from_point(m::p2(a)),
    crate::pin_extent::finite2(m::extent2d::Extent2d::from_point(m::p2(a)))
);
diff!(
    e2_hull,
    truth_defect = crate::pin_truths::e2_hull,
    |a: P, b: P| m::extent2d::Extent2d::hull([m::p2(a), m::p2(b)])
);
// scx's four bounds against the pin's `set_min_max_coords` (the pin's `from_bounds` stores
// any four numbers), refused where an end is not finite.
diff!(
    e2_from_bounds,
    |a: R| m::extent2d::Extent2d::from_bounds(a[0], a[1], a[2], a[3]),
    {
        let mut e = m::extent2d::Extent2d::new();
        e.set_min_max_coords(a[0], a[1], a[2], a[3])
            .ok()
            .map(|()| e)
            .and_then(crate::pin_extent::finite2)
    }
);
diff!(e2_from_extent3d, |b: B| m::e3(b).map(|e| (
    m::extent2d::Extent2d::from_extent3d(&e),
    m::extent2d::Extent2d::from(e),
)));
diff!(e2_transposed, |a: R| m::e2(a).map(|e| e.transposed()));
diff!(e2_mid, pin_defect = crate::pin_defects::e2_mid, |a: R| {
    m::e2(a).map(|e| e.mid())
});
diff!(e2_queries, |a: R| m::e2(a).map(|e| (
    (e.x_min(), e.x_max(), e.y_min(), e.y_max()),
    (e.x_interval(), e.y_interval(), e.size()),
    (e.x_length(), e.y_length()),
)));
diff!(e2_max_dimension, |a: R| m::e2(a).map(|e| e.max_dimension()));
diff!(
    e2_hull_point,
    truth_defect = crate::pin_truths::e2_hull_point,
    |a: R, p: P| m::e2(a).map(|e| {
        let mut h = m::extent2d::Hull2d::EMPTY;
        h.add_extent(e);
        let u = h.union([m::p2(p)].into_iter().collect());
        h.add(m::p2(p));
        (h.finish(), u.finish())
    })
);
diff!(e2_with_intervals, |a: R, i: I| m::e2(a).zip(m::e1(i)).map(
    |(e, i)| (
        e.with_x_interval(i),
        e.with_y_interval(i),
        m::extent2d::Extent2d::from_intervals(e.y_interval(), i),
    )
));
// One bound moved: the interval's `with_min`/`with_max`, set back on its axis.
diff!(e2_with_bounds, |a: R, x: f64| m::e2(a).map(|e| (
    (
        e.x_interval().with_min(x).map(|i| e.with_x_interval(i)),
        e.x_interval().with_max(x).map(|i| e.with_x_interval(i)),
    ),
    (
        e.y_interval().with_min(x).map(|i| e.with_y_interval(i)),
        e.y_interval().with_max(x).map(|i| e.with_y_interval(i)),
    ),
)));
diff!(
    e2_expand_scale,
    truth_defect = crate::pin_truths::e2_expand_scale,
    |a: R, d: f64, w: Option<f64>| m::e2(a).map(|e| (
        e.expanded(d.abs()),
        e.expanded_relative(d.abs()),
        w.map_or_else(|| e.scaled(d), |w| e.scaled_xy(d, w)),
    ))
);
diff!(e2_shrunk, |a: R, d: f64| m::e2(a)
    .map(|e| e.shrunk(d.abs())));
diff!(
    e2_expanded_vec,
    truth_defect = crate::pin_truths::e2_expanded_vec,
    |a: R, d: P| m::e2(a).map(|e| e.expanded_vec(m::v2([d[0].abs(), d[1].abs()])))
);
diff!(e2_pair, |a: R, b: R| m::e2(a)
    .zip(m::e2(b))
    .map(|(e, f)| (e.are_same(&f), e.max_boundary_dist(&f))));
diff!(e2_union_intersect, |a: R, b: R| m::e2(a)
    .zip(m::e2(b))
    .map(|(e, f)| (e.union(f), e.intersect(f))));
diff!(e2_pair_tol, |a: R, b: R, t: f64| m::e2(a)
    .zip(m::e2(b))
    .map(|(e, f)| (
        e.are_disjoint(&f, t),
        e.are_equal(&f, t),
        e.is_contained_by(&f, t),
        e.is_degenerate(t),
    )));
diff!(e2_point_tol, |a: R, p: P, t: f64| m::e2(a).map(|e| {
    let p = m::p2(p);
    (
        (
            e.contains_point(p, t),
            e.contains_point_dist(p, t),
            e.is_point_on_boundary(p, t),
        ),
        (e.point_boundaries(p, t), e.inversion(p, t)),
    )
}));
diff!(e2_classify_point, |a: R, p: P, t: f64, w: Option<f64>| {
    m::e2(a).map(|e| e.classify_point(m::p2(p), t, w))
});
diff!(
    e2_contains_periodic_point,
    |a: R, p: P, u: f64, v: f64, t: f64| m::e2(a).map(|e| e.contains_periodic_point(
        m::p2(p),
        u,
        v,
        t
    ))
);
diff!(e2_evaluate, |a: R, x: f64, y: f64| m::e2(a)
    .map(|e| e.evaluate(x, y)));
diff!(e2_clamp_point, |a: R, p: P| m::e2(a)
    .map(|e| e.clamp_point(m::p2(p))));
diff!(e2_clip_line, |a: R, p: P, v: P, t: f64| m::e2(a)
    .map(|e| e.clip_line(m::p2(p), m::v2(v), t)));
diff!(
    e2_intersect_with_infinite_line,
    truth_defect = crate::pin_truths::e2_intersect_with_infinite_line,
    |a: R, p: P, v: P, tu: Option<f64>, tv: Option<f64>| {
        m::e2(a).map(|e| e.intersect_with_infinite_line(m::p2(p), m::v2(v), tu, tv))
    }
);
diff!(e2_left_eval, |a: R, x: f64| m::e2(a)
    .map(|e| (e.x_left_eval(x), e.y_left_eval(x))));
diff!(e2_point_map_to_domain, |a: R, p: P, b: R, sw: bool| {
    let order = if sw {
        m::extent2d::ParamOrder::Swapped
    } else {
        m::extent2d::ParamOrder::Kept
    };
    m::e2(a)
        .zip(m::e2(b))
        .map(|(e, f)| e.point_map_to_domain(m::p2(p), &f, order))
});
diff!(e2_consts, || [
    m::extent2d::SS_NONE,
    m::extent2d::SS_UMIN,
    m::extent2d::SS_VMIN,
    m::extent2d::SS_UMAX,
    m::extent2d::SS_VMAX
]);

// --- Extent3d
diff!(e3_constructors, |a: V, b: V| (
    m::extent3d::Extent3d::spanning(m::p3(a), m::p3(b)),
    m::extent3d::Extent3d::from_corners(m::p3(a), m::p3(b)),
    m::extent3d::Hull3d::EMPTY.finish()
));
// scx's `from_point` refuses a point that is not finite; the pin's stores any.
diff!(
    e3_from_point,
    |a: V| m::extent3d::Extent3d::from_point(m::p3(a)),
    crate::pin_extent::finite3(m::extent3d::Extent3d::from_point(m::p3(a)))
);
diff!(
    e3_hull,
    truth_defect = crate::pin_truths::e3_hull,
    |a: V, b: V| m::extent3d::Extent3d::hull([m::p3(a), m::p3(b)])
);
// scx's six bounds against the pin's `set_min_max_coords` (the pin's `from_bounds` stores
// any six numbers), refused where an end is not finite.
diff!(
    e3_from_bounds,
    |a: B| m::extent3d::Extent3d::from_bounds(a[0], a[1], a[2], a[3], a[4], a[5]),
    {
        let mut e = m::extent3d::Extent3d::new();
        e.set_min_max_coords(a[0], a[1], a[2], a[3], a[4], a[5])
            .ok()
            .map(|()| e)
            .and_then(crate::pin_extent::finite3)
    }
);
diff!(e3_queries, |a: B| m::e3(a).map(|e| (
    (e.mid(), e.x_min(), e.x_max(), e.y_min(), e.y_max()),
    (e.z_min(), e.z_max(), e.x_interval(), e.y_interval()),
    (e.z_interval(), e.max_dimension(), e.size(), e.corners()),
    (e.edges(), e.planes(), e.x_length(), e.y_length()),
)));
diff!(e3_max_length, |a: B| m::e3(a).map(|e| e.max_length()));
diff!(e3_queries2, |a: B| m::e3(a).map(|e| {
    let b = e.sphere_bound();
    (e.z_length(), b.center, b.radius)
}));
diff!(e3_sphere_min_radius, |a: B| m::e3(a)
    .map(|e| e.sphere_bound().min_radius));
diff!(e3_min_max_at, |a: B, i: usize| m::e3(a)
    .map(|e| m::e3_at(&e, i)));
diff!(e3_with_intervals, |a: B, i: I| m::e3(a).zip(m::e1(i)).map(
    |(e, i)| (
        e.with_x_interval(i),
        e.with_y_interval(i),
        m::extent3d::Extent3d::from_intervals(e.z_interval(), i, e.x_interval()),
    )
));
diff!(
    e3_hull_point,
    truth_defect = crate::pin_truths::e3_hull_point,
    |a: B, p: V| m::e3(a).map(|e| {
        let mut h = m::extent3d::Hull3d::EMPTY;
        h.add_extent(e);
        let u = h.union([m::p3(p)].into_iter().collect());
        h.add(m::p3(p));
        (h.finish(), u.finish())
    })
);
// One bound moved: the interval's `with_min`/`with_max`, set back on its axis.
diff!(e3_with_bounds, |a: B, x: f64| m::e3(a).map(|e| (
    (
        e.x_interval().with_min(x).map(|i| e.with_x_interval(i)),
        e.x_interval().with_max(x).map(|i| e.with_x_interval(i)),
    ),
    (
        e.y_interval().with_min(x).map(|i| e.with_y_interval(i)),
        e.y_interval().with_max(x).map(|i| e.with_y_interval(i)),
    ),
)));
diff!(e3_evaluate, |a: B, x: f64, y: f64, z: f64| m::e3(a)
    .map(|e| e.evaluate(x, y, z)));
diff!(e3_pair, |a: B, b: B| m::e3(a).zip(m::e3(b)).map(
    |(e, f)| (
        (e.min_distance_squared(&f), e.min_distance(&f)),
        e.cone_bound_to(&f),
    )
));
diff!(e3_union_intersect_far, |a: B, b: B| m::e3(a)
    .zip(m::e3(b))
    .map(|(e, f)| (
        e.union(f),
        e.intersect(f),
        e.max_distance_squared(&f),
        e.max_distance(&f),
    )));
diff!(e3_pair_tol, |a: B, b: B, t: f64| m::e3(a)
    .zip(m::e3(b))
    .map(|(e, f)| (
        e.are_disjoint(&f, t),
        e.is_contained_by(&f, t),
        e.is_point_sized(t),
    )));
diff!(e3_point, |a: B, p: V| m::e3(a)
    .map(|e| e.max_distance_to_point(m::p3(p))));
diff!(e3_point_tol, |a: B, p: V, t: f64| m::e3(a).map(|e| (
    e.contains_point(m::p3(p), t),
    e.is_point_on_boundary(m::p3(p), t),
)));
/// A box the pin's `is_defined` refuses: a minimum at `SM_UNDEF_DOUBLE` or a maximum at its
/// negation. scx has no such sentinel, so `intersect_line` is compared on other boxes only.
fn pin_undefined(a: B) -> bool {
    let u = solidcore::math::UNDEF_DOUBLE;
    a[..3].contains(&u) || a[3..].contains(&-u)
}
diff!(e3_line, |a: B, p: V, d: V| m::e3(a).map(|e| (
    e.span_along(m::p3(p), m::v3(d)),
    e.distance_to_plane(m::p3(p), m::v3(d)),
)));
diff!(
    e3_intersect_line,
    pin_defect = crate::pin_defects::e3_intersect_line,
    |a: B, p: V, d: V| {
        m::e3(a)
            .filter(|_| !pin_undefined(a))
            .map(|e| e.intersect_line(m::p3(p), m::v3(d)))
    }
);
diff!(
    e3_line_hit_count,
    pin_defect = crate::pin_defects::e3_line_hit_count,
    |a: B, p: V, d: V| m::e3(a).filter(|_| !pin_undefined(a)).map(|e| e
        .intersect_line(m::p3(p), m::v3(d))
        .map(m::extent3d::LineHit::count))
);
diff!(e3_line_tol, |a: B, p: V, d: V, t: f64| m::e3(a).map(|e| (
    e.contains_line_seg(m::p3(p), m::p3(d), t),
    e.meets_ray(m::p3(p), m::v3(d), t),
)));
diff!(
    e3_expand,
    truth_defect = crate::pin_truths::e3_expand,
    |a: B, d: f64| m::e3(a).map(|e| (e.expanded_relative(d.abs()), e.expanded(d.abs()),))
);
diff!(e3_shrunk, |a: B, d: f64| m::e3(a)
    .map(|e| e.shrunk(d.abs())));

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
diff!(
    pe_values,
    truth_defect = crate::pin_truths::pe_values,
    |a: V, f: bool, v: f64| {
        let e = m::pe(a, f);
        (e.map_value(v), e.normalize_value(v), e.contains_value(v))
    }
);
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
diff!(
    pe_set_min_max,
    truth_defect = crate::pin_truths::pe_set_min_max,
    |a: V, f: bool, x: f64, y: f64, p: Option<f64>, sp: bool| {
        let mut e = m::pe(a, f);
        (e.set_min_max(x, y, p, sp), e)
    }
);
diff!(
    pe_pair,
    truth_defect = crate::pin_truths::pe_pair,
    |a: V, f: bool, b: V, g: bool| {
        let (e, h) = (m::pe(a, f), m::pe(b, g));
        (e.is_contained_by(&h), e.are_disjoint(&h))
    }
);
