// Written for solidcorex. Apache-2.0.
//! `math.rs`: scalar helpers, the row-vector `Transform` and the 2D sketch helpers.

use crate::diff;
use crate::harness::List;

type V = [f64; 3];
type P = [f64; 2];
type M = [[f64; 4]; 4];

diff!(is_infinite_param, |a: f64| m::math::is_infinite_param(a));
diff!(max2, |a: f64, b: f64| m::math::max2(a, b));
diff!(min2, |a: f64, b: f64| m::math::min2(a, b));
diff!(max3, |a: f64, b: f64, c: f64| m::math::max3(a, b, c));
diff!(min3, |a: f64, b: f64, c: f64| m::math::min3(a, b, c));
diff!(max4, |a: f64, b: f64, c: f64, d: f64| m::math::max4(
    a, b, c, d
));
diff!(min4, |a: f64, b: f64, c: f64, d: f64| m::math::min4(
    a, b, c, d
));
diff!(rad_to_deg, |a: f64| m::math::rad_to_deg(a));
// `det3` is `pub(crate)` at the pin: no solidcore counterpart to call (see 52-num-move.md).

diff!(tr_translation, |d: V| m::math::Transform::translation(
    m::v3(d)
));
diff!(tr_default, || <m::math::Transform as Default>::default());
diff!(tr_from_axes, |a: V, b: V, c: V, d: V| {
    m::math::Transform::from_axes(m::v3(a), m::v3(b), m::v3(c), m::v3(d))
});
diff!(tr_axes, |t: M| {
    let t = m::mt(t);
    (t.x_axis(), t.y_axis(), t.z_axis(), t.translation_part())
});
diff!(tr_concat, |a: M, b: M| m::mt(a).concat(&m::mt(b)));
diff!(tr_transform_point, |t: M, p: V| m::mt(t)
    .transform_point(m::v3(p)));
diff!(tr_transform_points, |t: M, p: V, q: V| {
    let mut pts = [p, q];
    m::mt(t).transform_points(&mut pts);
    pts
});
diff!(tr_transformed_box, |t: M, a: V, b: V| m::mt(t)
    .transformed_box(m::v3(a), m::v3(b)));
diff!(tr_row_lengths, |t: M| m::mt(t).row_lengths());
diff!(tr_is_regular, |t: M, r: f64| m::mt(t).is_regular(r));
diff!(tr_is_rigid, |t: M, a: f64, b: f64| m::mt(t).is_rigid(a, b));
diff!(tr_plane_motion, |t: M,
                        o: V,
                        x: V,
                        y: V,
                        tol: f64,
                        id: f64| m::mt(t)
    .plane_motion(m::v3(o), m::v3(x), m::v3(y), tol, id)
    .map(|(r, d)| (r, d)));
diff!(tr_orthonormal_frame, |a: V, b: V| {
    m::math::Transform::orthonormal_frame(m::v3(a), m::v3(b))
});
diff!(tr_axis_squared_frame, |a: V, b: V, n: f64| {
    m::math::Transform::axis_squared_frame(m::v3(a), m::v3(b), n)
});
diff!(tr_rigid_plane_axes, |t: M, a: V, b: V| m::mt(t)
    .rigid_plane_axes(m::v3(a), m::v3(b)));
diff!(tr_rigid_plane_through, |t: M, a: V, b: V, c: V| m::mt(t)
    .rigid_plane_through(m::v3(a), m::v3(b), m::v3(c)));
diff!(tr_transform_vector, |t: M, v: V| m::mt(t)
    .transform_vector(m::v3(v)));
diff!(tr_transform_line, |t: M, p: V, v: V| m::mt(t)
    .transform_line(m::v3(p), m::v3(v)));
diff!(tr_transform_canonical_axis, |t: M, p: V, v: V| m::mt(t)
    .transform_canonical_axis(m::v3(p), m::v3(v)));
diff!(tr_pivot_displacement, |t: M, p: V| m::mt(t)
    .pivot_displacement(m::v3(p)));
diff!(tr_rotation, |p: V, a: V, ang: f64| {
    m::math::Transform::rotation(m::v3(p), m::v3(a), ang)
});
diff!(tr_uniform_scaling, |p: V, f: f64| {
    m::math::Transform::uniform_scaling(m::v3(p), f)
});
diff!(tr_reflection, |p: V, n: V| m::math::Transform::reflection(
    m::v3(p),
    m::v3(n)
));
diff!(tr_into_plane_frame, |p: V, x: V, y: V| {
    m::math::Transform::into_plane_frame(m::v3(p), m::v3(x), m::v3(y))
});
diff!(tr_inverse, |t: M| m::mt(t).inverse());
diff!(tr_eq, |t: M, u: M| {
    let (a, b) = (m::mt(t), m::mt(u));
    (a == b, a)
});

diff!(rect_of_aspect, |a: V,
                       b: V,
                       c: Option<V>,
                       n: V,
                       asp: f64| {
    m::math::rect_of_aspect(m::v3(a), m::v3(b), c.map(m::v3), m::v3(n), asp)
});
diff!(
    fit_aspect,
    |a: V, b: V, c: V, asp: f64| m::math::fit_aspect(m::v3(a), m::v3(b), m::v3(c), asp)
);
diff!(offset_quad, |a: V, b: V, c: V, d: V, l: f64| {
    m::math::offset_quad([m::v3(a), m::v3(b), m::v3(c), m::v3(d)], l)
});
diff!(parallelogram_point, |c: V, u: V, v: V, s: f64, t: f64| {
    m::math::parallelogram_point(m::v3(c), m::v3(u), m::v3(v), s, t)
});
diff!(box_extent_along, |a: V, b: V, t: M, ax: V| {
    m::math::box_extent_along(m::v3(a), m::v3(b), &m::mt(t), m::v3(ax))
});
diff!(axis_along_own_axes, |t: M, ax: V| {
    m::math::axis_along_own_axes(&m::mt(t), m::v3(ax))
});
diff!(plane_offset_square_to, |t: M, o: V, n: V, ax: V| {
    m::math::plane_offset_square_to(&m::mt(t), m::v3(o), m::v3(n), m::v3(ax))
});
diff!(centroid_and_mean_radius, |l: List<P>| {
    let pts: Vec<_> = l.0.iter().map(|&p| m::v2(p)).collect();
    m::math::centroid_and_mean_radius(&pts)
});
diff!(unit_2d, |v: P| m::math::unit_2d(v));
diff!(unit_between_2d, |a: P, b: P| m::math::unit_between_2d(a, b));
diff!(left_normal_between_2d, |a: P, b: P| {
    m::math::left_normal_between_2d(a, b)
});
diff!(tangent_arc_centre_2d, |a: P, b: P, d: P| {
    m::math::tangent_arc_centre_2d(a, b, d)
});
diff!(bisector_centre_2d, |a: P, b: P, p: P| {
    m::math::bisector_centre_2d(a, b, p)
});
diff!(is_square_to_radius_2d, |c: P, p: P, d: P| {
    m::math::is_square_to_radius_2d(c, p, d)
});
diff!(
    circumcentre_2d,
    |a: P, b: P, c: P| m::math::circumcentre_2d(a, b, c)
);
diff!(three_point_circle_2d, |a: P, b: P, c: P| {
    m::math::three_point_circle_2d(a, b, c)
});
diff!(
    arc_third_point_2d,
    |a: P, b: P, h: P, r: Option<f64>, sw: Option<f64>| {
        m::math::arc_third_point_2d(a, b, h, r, sw)
    }
);

diff!(constants, || [
    m::math::PI,
    m::math::HALF_PI,
    m::math::THREE_HALF_PI,
    m::math::TWO_PI,
    m::math::SQRT3,
    m::math::EFF_ZERO,
    m::math::EFF_ZERO_SQ,
    m::math::EFF_ZERO_SQRT,
    m::math::EFF_ZERO_PARAM,
    m::math::EFF_ZERO_DEG,
    m::math::EFF_ZERO_RAD,
    m::math::UNDEF_DOUBLE,
    m::math::BIG_DOUBLE,
    m::math::INFINITE_PARAMETER,
    m::math::INFINITE_PARAMETER_SQRT,
    m::math::BOUNDED_INFINITE_PARAM,
]);
