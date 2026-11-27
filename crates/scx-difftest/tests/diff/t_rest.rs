// Written for solidcorex. Apache-2.0.
//! `certified.rs`, `limits.rs`, `matrix.rs`, the small-matrix helpers of
//! `geom_utility` and `transform.rs`.

use crate::diff;
use crate::facade::TfR;
use crate::harness::{List, MatR};

type V = [f64; 3];
type M = [[f64; 4]; 4];

// --- certified
diff!(
    unit_and_gamma,
    |n: usize| (scx_num::certified::UNIT, scx_num::certified::gamma(n)),
    (
        solidcore::nurbs::math::UNIT,
        solidcore::geom::certified::gamma(n)
    )
);
diff!(gamma_large, |n: f64| m::certified::gamma(
    (n.abs() * 1.0e6) as usize
));
// `next_up` and `next_down` are `pub(crate)` at the pin: no counterpart to call; they are
// covered through `distance`, `difference`, `dot` and the scx-num property test.
diff!(c_above, |a: f64, b: f64| m::certified::above(a, b));
diff!(c_underflow_root, || m::certified::UNDERFLOW_ROOT);
diff!(c_verdict, |a: f64, b: f64, t: f64| {
    let x = m::certified::difference(a, b);
    let (p, q) = (x.at_most(t), x.above(t));
    (p, q, p.and(q), q.and(p), p.is_yes(), q.is_yes())
});
diff!(c_distance, |a: V, b: V| m::certified::distance(
    m::p3(a),
    m::p3(b)
));
diff!(c_unit, pin_defect = crate::pin_defects::c_unit, |a: V| {
    m::certified::unit(m::v3(a))
});
diff!(c_errors, |a: f64, b: f64| (
    m::certified::unit_error(),
    m::certified::unit_error_of(a, b),
    m::certified::dot_error(a, b)
));
diff!(c_products, |a: V, b: V, ea: f64, eb: f64| {
    let (a, b) = (m::v3(a), m::v3(b));
    (
        m::certified::dot(a, b, ea, eb),
        m::certified::one_minus_dot(a, b, ea, eb),
        m::certified::cross_length(a, b, ea, eb),
    )
});
diff!(c_scaled, |a: f64, b: f64, s: f64| m::certified::scaled(
    m::certified::difference(a, b),
    s
));

// --- limits
diff!(l_consts, || (
    m::limits::MAX_ALLOC_BYTES,
    m::limits::MAX_COORDINATE_MAGNITUDE
));
diff!(l_exceeds_coordinate_bound, |x: f64| {
    m::limits::exceeds_coordinate_bound(x)
});
diff!(l_fits, |k: usize| {
    let n = [0usize, 1, 1000, 1 << 27, (1 << 27) + 1, 1 << 30, usize::MAX][k % 7];
    (
        m::limits::fits::<f64>(n),
        m::limits::fits::<u8>(n),
        m::limits::fits::<[f64; 4]>(n),
    )
});
diff!(l_try_filled, |k: usize, x: f64| {
    let n = [0usize, 1, 7, 1000, 1 << 28, usize::MAX][k % 6];
    m::limits::try_filled(n, x).map(|v| (v.len(), v.first().copied()))
});
diff!(l_try_reserve_total, |k: usize, pre: usize| {
    let n = [0usize, 1, 7, 1000, 1 << 28, usize::MAX][k % 6];
    let mut v = vec![0.0f64; pre];
    (m::limits::try_reserve_total(&mut v, n), v.len())
});

// --- matrix
diff!(mx_consts, || (
    m::matrix::MAX_MATRIX_SIZE,
    m::matrix::SVD_MAX_ITERATIONS
));
diff!(mx_new, |r: usize, c: usize| (
    m::matrix::Matrix::new(r, c),
    m::matrix::Matrix::try_new(r, c),
    m::matrix::Matrix::try_new(r * 20, c * 20).map(|x| x.num_rows())
));
diff!(mx_from_row_major, |r: usize, c: usize, d: List<f64>| {
    m::matrix::Matrix::from_row_major(r, c, d.0)
});
diff!(mx_at, |a: MatR, r: usize, c: usize, x: f64| {
    let mut p = m::mat(&a);
    let mut q = m::mat(&a);
    let s = p.set_at(r, c, x);
    let t = q.add_to(r, c, x);
    (m::mat(&a).at(r, c), (s, p), (t, q))
});
diff!(mx_row, |a: MatR, r: usize, x: f64| {
    let mut p = m::mat(&a);
    if let Some(row) = p.row_mut(r) {
        for v in row.iter_mut() {
            *v += x;
        }
    }
    (m::mat(&a).row(r).map(<[f64]>::to_vec), p)
});
diff!(mx_set_size, |a: MatR, r: usize, c: usize| {
    let mut p = m::mat(&a);
    (p.set_size(r, c), p)
});
diff!(mx_unary, |a: MatR| {
    let (mut p, mut q, mut t) = (m::mat(&a), m::mat(&a), m::mat(&a));
    p.clear();
    let i = q.make_identity();
    let tr = t.transpose();
    (m::mat(&a).is_symmetric(), p, (i, q), (tr, t))
});
diff!(mx_lu, |a: MatR, b: List<f64>| {
    let mut p = m::mat(&a);
    let lu = p.lu_decompose();
    let solved = lu.as_ref().ok().map(|d| p.solve_with_lu(&d.index, &b.0));
    (lu, p, solved)
});
diff!(
    mx_solve_with_lu_index,
    |a: MatR, b: List<f64>, idx: List<usize>| m::mat(&a).solve_with_lu(&idx.0, &b.0)
);
diff!(mx_solve_linear_system, |a: MatR, b: List<f64>| m::mat(&a)
    .solve_linear_system(&b.0));
diff!(mx_eigen, |a: MatR| {
    let (mut p, mut q) = (m::mat(&a), m::mat(&a));
    (
        (p.eigen_system_jacobi(), p),
        (q.compute_3x3_eigen_vectors(), q),
    )
});
diff!(mx_svd, |a: MatR| {
    let mut p = m::mat(&a);
    (p.sv_decompose(), p)
});

// --- small matrices (geom_utility)
diff!(sm_vec_triple_product, |a: V, b: V, c: V| {
    m::small_matrix::vec_triple_product(&a, &b, &c)
});
diff!(sm_determinate_3x3, |a: V, b: V, c: V| {
    m::small_matrix::determinate_3x3(a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2])
});
diff!(sm_inverse_3x3, |a: V, b: V, c: V| {
    m::small_matrix::inverse_3x3(&a, &b, &c)
});
diff!(sm_determinate_4x4, |a: M| m::small_matrix::determinate_4x4(
    &a
));
diff!(sm_mult_4x4, |a: M, b: M| m::small_matrix::mult_4x4(&a, &b));
diff!(sm_inverse_4x4, |a: M| m::small_matrix::inverse_4x4(&a));

// --- transform (`SmTransform`, the matrix part)
diff!(tf_consts, || (
    m::transform::MAX_DERIV,
    m::transform::Transform::IDENTITY,
    <m::transform::Transform as Default>::default()
));
diff!(tf_new, |r: TfR| m::tf(&r));
diff!(tf_queries, |r: TfR| {
    let t = m::tf(&r);
    (
        (t.to_origin(), t.to_x_axis(), t.to_y_axis()),
        (
            t.to_z_axis(),
            t.to_disp(),
            t.is_bounded_simple(),
            t.is_curve_in_param_domain_simple(),
        ),
    )
});
diff!(tf_approx_eq, |a: TfR, b: TfR| m::tf(&a)
    .approx_eq(&m::tf(&b)));
diff!(tf_init, |r: TfR| {
    let (mut p, mut q) = (m::tf(&r), m::tf(&r));
    p.init_ext();
    q.init();
    (p, q)
});
diff!(tf_setters, |r: TfR, v: V| {
    let mut t = [m::tf(&r), m::tf(&r), m::tf(&r), m::tf(&r), m::tf(&r)];
    t[0].set_to_origin(m::p3(v));
    t[1].set_to_x_axis(m::v3(v));
    t[2].set_to_y_axis(m::v3(v));
    t[3].set_to_z_axis(m::v3(v));
    t[4].set_to_disp(m::v3(v));
    t
});
diff!(tf_set_canonical, |r: TfR, s: TfR| {
    let mut t = m::tf(&r);
    t.set_canonical(m::p3(s.0), m::v3(s.1), m::v3(s.2), s.3.map(m::v3));
    t
});
diff!(tf_transform_vector, |r: TfR, v: V| {
    let t = m::tf(&r);
    (
        t.transform_vector(m::v3(v)),
        t.inv_transform_point(m::p3(v)),
        t.inv_transform_vector(m::v3(v)),
    )
});
diff!(tf_concat_transform, |a: TfR, b: TfR| {
    let mut t = m::tf(&a);
    t.concat_transform(&m::tf(&b));
    t
});
diff!(tf_scale_simple, |r: TfR, s: V, c: Option<V>| {
    let mut t = m::tf(&r);
    t.scale_simple(m::v3(s), c.map(m::p3));
    t
});
diff!(
    tf_mirror_simple,
    pin_defect = crate::pin_defects::tf_mirror_simple,
    |r: TfR, p: V, n: V| {
        let mut t = m::tf(&r);
        (t.mirror_simple(m::p3(p), m::v3(n)), t)
    }
);
diff!(tf_invert_simple, |r: TfR| {
    let mut t = m::tf(&r);
    (t.invert_simple(), t)
});
diff!(tf_evaluate_simple, |r: TfR, p: V, d: usize| m::tf(&r)
    .evaluate_simple(m::p3(p), d));
diff!(tf_inv_evaluate_guess_point_simple, |r: TfR, p: V| m::tf(&r)
    .inv_evaluate_guess_point_simple(m::p3(p)));
