// Written for solidcorex. Apache-2.0.
//! `vector3d.rs` and `vector2d.rs`: every public function, constant and operator.

use crate::diff;

type V = [f64; 3];
type P = [f64; 2];

diff!(sqrt, |a: f64| m::vector3d::sqrt(a));
diff!(cbrt, |a: f64| m::vector3d::cbrt(a));
diff!(acos, |a: f64| m::vector3d::acos(a));
diff!(asin, |a: f64| m::vector3d::asin(a));
diff!(
    v3_noz_now,
    || [scx_num::vector3d::NOZ, scx_num::vector3d::NOW],
    [solidcore::nurbs::math::NOZ, solidcore::nurbs::math::NOW]
);
diff!(v3_consts, || [
    m::vector3d::Vec3::ZERO,
    m::vector3d::Vec3::X,
    m::vector3d::Vec3::Y,
    m::vector3d::Vec3::Z
]);
diff!(v3_canonical_axis_direction, |a: V| m::v3(a)
    .canonical_axis_direction());
diff!(v3_unit_axis_direction, |a: V, t: f64| m::v3(a)
    .unit_axis_direction(t));
diff!(v3_from_array, |a: V| m::vector3d::Vec3::from_array(a)
    .to_array());
diff!(v3_from_nl, |a: [f64; 4]| m::vector3d::Vec3::from_nl(a));
diff!(
    v3_between,
    |a: V, b: V| m::p3(b) - m::p3(a),
    m::vector3d::Vec3::between(m::v3(a), m::v3(b))
);
/// A vector holding the pin's `SM_UNDEF_DOUBLE` sentinel, which its `IsUndef` and
/// `operator==` test for (scx: no sentinel). Those two are compared on other vectors only.
fn pin_unset(a: &[f64]) -> bool {
    a.contains(&solidcore::math::UNDEF_DOUBLE)
}
diff!(
    v3_predicates,
    |a: V| {
        let v = m::v3(a);
        ((!pin_unset(&a)).then(|| v.is_invalid()), v.is_2d())
    },
    {
        let v = m::v3(a);
        ((!pin_unset(&a)).then(|| v.is_undef()), v.is_2d())
    }
);
diff!(v3_is_zero, |a: V, t: f64| m::v3(a).is_zero(t));
diff!(v3_lengths, |a: V| {
    let v = m::v3(a);
    (
        v.length_squared(),
        v.length(),
        v.max_dimension(),
        v.min_dimension(),
    )
});
diff!(v3_dot, |a: V, b: V| m::v3(a).dot(m::v3(b)));
diff!(v3_distance_along, |a: V, b: V, c: V| m::p3(a)
    .distance_along(m::p3(b), m::v3(c)));
diff!(v3_cross, |a: V, b: V| m::v3(a).cross(m::v3(b)));
diff!(
    v3_unitize,
    pin_defect = crate::pin_defects::v3_normalized,
    |a: V| m::v3(a).normalized(),
    m::v3(a).unitize().ok()
);
diff!(v3_length_and_unit, |a: V| m::v3(a).length_and_unit());
diff!(v3_triple_product, |a: V, b: V, c: V| m::v3(a)
    .triple_product(m::v3(b), m::v3(c)));
diff!(v3_normal_in_plane, |a: V, b: V| m::v3(a)
    .normal_in_plane(m::v3(b)));
diff!(v3_is_parallel_to, |a: V, b: V, t: f64| m::v3(a)
    .is_parallel_to(m::v3(b), t));
diff!(v3_parallel_sense, |a: V, b: V, t: f64| m::v3(a)
    .parallel_sense(m::v3(b), t));
diff!(v3_with_sense_of, |a: V, b: V| m::v3(a)
    .with_sense_of(m::v3(b)));
diff!(v3_is_perpendicular_to, |a: V, b: V, t: f64| m::v3(a)
    .is_perpendicular_to(m::v3(b), t));
diff!(v3_project_to_plane, |a: V, b: V| m::v3(a)
    .project_to_plane(m::v3(b)));
diff!(v3_project_point_to_plane, |a: V, b: V, c: V| m::p3(a)
    .project_point_to_plane(m::p3(b), m::v3(c)));
diff!(v3_rotate_project_to_plane, |a: V, b: V, c: V| m::v3(a)
    .rotate_project_to_plane(m::v3(b), m::v3(c)));
diff!(
    v3_rotate_project_point_to_plane,
    |a: V, b: V, c: V, d: V| m::p3(a).rotate_project_point_to_plane(m::p3(b), m::v3(c), m::v3(d))
);
diff!(
    v3_perspective_project_to_plane,
    |a: V, b: V, c: V, d: V, e: V| m::v3(a).perspective_project_to_plane(
        m::p3(b),
        m::p3(c),
        m::v3(d),
        m::p3(e)
    )
);
diff!(
    v3_perspective_project_point_to_plane,
    |a: V, b: V, c: V, d: V| m::p3(a).perspective_project_point_to_plane(
        m::p3(b),
        m::v3(c),
        m::p3(d)
    )
);
diff!(v3_distance, |a: V, b: V| (
    m::p3(a).distance(m::p3(b)),
    m::p3(a).distance_squared(m::p3(b))
));
diff!(v3_closer_than, |a: V, t: f64, b: V| m::p3(a)
    .closer_than(t, m::p3(b)));
diff!(v3_multiply, |a: V, b: V| m::v3(a).multiply(m::v3(b)));
diff!(v3_derivative_of_cross_product, |a: V, b: V, c: V, d: V| {
    m::v3(a).derivative_of_cross_product(m::v3(b), m::v3(c), m::v3(d))
});
diff!(v3_solve_two_linear_equations, |a: V, b: V| m::v3(a)
    .solve_two_linear_equations(m::v3(b)));
diff!(v3_component, |a: V, i: usize| m::v3(a).component(i));
diff!(v3_component_mut, |a: V, i: usize, x: f64| {
    let mut v = m::v3(a);
    let r = v.component_mut(i).map(|c| *c = x);
    (r, v)
});
diff!(v3_approx_eq, |a: V, b: V| (!pin_unset(&a)
    && !pin_unset(&b))
.then(|| m::v3(a).approx_eq(m::v3(b))));
diff!(v3_is_inside_sector, |a: V, b: V, c: V, d: V| m::v3(a)
    .is_inside_sector(m::v3(b), m::v3(c), m::v3(d)));
diff!(
    v3_make_unit_ortho_vectors,
    pin_defect = crate::pin_defects::v3_make_unit_ortho_vectors,
    |a: V, b: Option<V>| m::v3(a).make_unit_ortho_vectors(b.map(m::v3))
);
diff!(v3_rotate_vec_about_axis, |a: V, b: V, t: f64| m::v3(a)
    .rotate_vec_about_axis(m::v3(b), t));
diff!(v3_unitized_derivative, |a: V, b: V| m::v3(a)
    .unitized_derivative(m::v3(b)));
diff!(v3_unitized_derivative2, |a: V, b: V, c: V| m::v3(a)
    .unitized_derivative2(m::v3(b), m::v3(c)));
diff!(v3_unitized_derivative3, |a: V, b: V, c: V, d: V| m::v3(a)
    .unitized_derivative3(m::v3(b), m::v3(c), m::v3(d)));
diff!(v3_angle_between, |a: V, b: V| m::v3(a)
    .angle_between(m::v3(b)));
diff!(v3_signed_unit_angle, |a: V, b: V, c: V| m::v3(a)
    .signed_unit_angle(m::v3(b), m::v3(c)));
/// Unit directions perpendicular to a unit normal (random draws almost never are), at the
/// half-turn, near it, and at the 1e-9 unit and perpendicular limits.
const UNIT_TRIPLES: &[[V; 3]] = &[
    [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]],
    [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [-1.0, 1.0e-13, 0.0]],
    [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [-1.0, -1.0e-11, 0.0]],
    [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.6, -0.8, 0.0]],
    [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
    [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0e-9], [0.0, 1.0, 0.0]],
    [
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.000_000_000_000_001e-9],
        [0.0, 1.0, 0.0],
    ],
    [[0.0, 0.0, 1.0 + 5.0e-10], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    [[0.0, 0.0, 1.0 + 6.0e-10], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
];
diff!(v3_signed_unit_angle_units, |i: usize| UNIT_TRIPLES
    .get(i)
    .map(
        |&[a, b, c]| m::v3(a).signed_unit_angle(m::v3(b), m::v3(c))
    ));
diff!(v3_ccw_angle_between, |a: V, b: V, c: V| m::v3(a)
    .ccw_angle_between(m::v3(b), m::v3(c)));
diff!(v3_square_turn, |a: V, b: V, c: V| m::v3(a)
    .square_turn(m::v3(b), m::v3(c)));
diff!(v3_is_colinear_with, |a: V, b: V, c: V, d: V| m::v3(a)
    .is_colinear_with(m::v3(b), m::p3(c), m::p3(d)));
diff!(v3_from_conversions, |a: V, b: P| (
    m::vector3d::Vec3::from(a),
    m::vector3d::Vec3::from(m::v2(b))
));
diff!(v3_ops, |a: V, b: V| {
    let (x, y) = (m::v3(a), m::v3(b));
    (-x, x + y, x - y, x == y)
});
diff!(v3_scalar_ops, |a: V, s: f64| {
    let x = m::v3(a);
    let (mut p, mut q, mut r, mut t) = (x, x, x, x);
    p += x;
    q -= x;
    r *= s;
    t /= s;
    (x * s, s * x, x / s, [p, q, r, t])
});

diff!(v2_consts, || [m::vector2d::Vec2::ZERO]);
diff!(v2_from_to_array, |a: P| m::vector2d::Vec2::from_array(a)
    .to_array());
diff!(v2_from_xy, |a: V| m::vector2d::Vec2::from_xy(m::v3(a)));
diff!(v2_is_zero, |a: P, t: f64| m::v2(a).is_zero(t));
diff!(v2_lengths, |a: P| {
    let v = m::v2(a);
    (
        v.length_squared(),
        v.length(),
        v.max_dimension(),
        v.swap_xy(),
    )
});
diff!(v2_dot, |a: P, b: P| m::v2(a).dot(m::v2(b)));
diff!(
    v2_unitize,
    pin_defect = crate::pin_defects::v2_normalized,
    |a: P| m::v2(a).normalized(),
    m::v2(a).unitize().ok()
);
diff!(v2_distance, |a: P, b: P| (
    m::p2(a).distance(m::p2(b)),
    m::p2(a).distance_squared(m::p2(b))
));
diff!(v2_closer_than, |a: P, t: f64, b: P| m::p2(a)
    .closer_than(t, m::p2(b)));
diff!(v2_rotate, |a: P, t: f64| m::v2(a).rotate(t));
diff!(v2_approx, |a: P, b: P| (
    m::v2(a).approx_eq(m::v2(b)),
    m::v2(a).approx_ne(m::v2(b))
));
diff!(v2_component, |a: P, i: usize| m::v2(a).component(i));
diff!(v2_component_mut, |a: P, i: usize, x: f64| {
    let mut v = m::v2(a);
    let r = v.component_mut(i).map(|c| *c = x);
    (r, v)
});
diff!(v2_angle_between, |a: P, b: P| m::v2(a)
    .angle_between(m::v2(b)));
diff!(v2_ccw_angle_between, |a: P, b: P| m::v2(a)
    .ccw_angle_between(m::v2(b)));
diff!(v2_ops, |a: P, b: P| {
    let (x, y) = (m::v2(a), m::v2(b));
    let (mut p, mut q) = (x, x);
    p += y;
    q -= y;
    (-x, x + y, x - y, [p, q], x == y, m::vector2d::Vec2::from(a))
});
diff!(v2_scalar_ops, |a: P, s: f64| {
    let x = m::v2(a);
    let mut r = x;
    r *= s;
    (x * s, x / s, r)
});
