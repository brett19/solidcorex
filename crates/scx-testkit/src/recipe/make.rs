// Written for solidcorex. Apache-2.0.
//! The constructors' recipes (72 §4, MK1 to MK3): blocks, cylinders, cones, spheres, tori
//! and extrusions of line-and-arc profiles, and the inputs they refuse.

use crate::profile::{Seg, reversed};

use super::{
    Dimension, Kind, Outcome, Part, Recipe, Refusal, Shape, Topology, block_at, built, cells,
    cylinder_at, frame_at, frame_turned, recipe, sphere_at, torus_at, truth_part,
};

fn line(a: [f64; 2], b: [f64; 2]) -> Seg {
    Seg::Line { start: a, end: b }
}

fn arc(centre: [f64; 2], a: [f64; 2], b: [f64; 2], ccw: bool) -> Seg {
    Seg::Arc {
        centre,
        start: a,
        end: b,
        ccw,
    }
}

/// The square `[x0, x0 + s] x [y0, y0 + s]`, counter-clockwise: T11's "closed square" at
/// `(0, 0)`.
fn square(x0: f64, y0: f64, s: f64) -> Vec<Seg> {
    let p = [[x0, y0], [x0 + s, y0], [x0 + s, y0 + s], [x0, y0 + s]];
    (0..4).map(|i| line(p[i], p[(i + 1) % 4])).collect()
}

/// T11's "closed D of a line and an arc": the diameter, then the upper half circle of
/// radius 5.
fn d_profile() -> Vec<Seg> {
    vec![
        line([-5.0, 0.0], [5.0, 0.0]),
        arc([0.0, 0.0], [5.0, 0.0], [-5.0, 0.0], true),
    ]
}

/// T11's "closed rounded top": a 6 x 4 rectangle bulging to `(0, 5)` by an arc about the
/// origin through `(3, 4)` and `(-3, 4)`.
fn rounded_top() -> Vec<Seg> {
    vec![
        line([-3.0, 0.0], [3.0, 0.0]),
        line([3.0, 0.0], [3.0, 4.0]),
        arc([0.0, 0.0], [3.0, 4.0], [-3.0, 4.0], true),
        line([-3.0, 4.0], [-3.0, 0.0]),
    ]
}

fn extrusion(frame: super::Frame, outer: Vec<Seg>, holes: Vec<Vec<Seg>>, height: f64) -> Part {
    Part {
        frame,
        shape: Shape::Extrusion {
            outer,
            holes,
            height,
        },
    }
}

/// The constructors' recipes; `res` is the document's resolution.
pub(super) fn recipes(res: f64) -> Vec<Recipe> {
    let tilt = |o: [f64; 3]| frame_turned(o, [1.0, 2.0, 3.0], 0.7);
    let mut v = Vec::new();

    // --- blocks ----------------------------------------------------------------------
    let part = block_at([0.0; 3], [3.0, 4.0, 5.0]);
    let truth = truth_part(&part);
    v.push(recipe(
        "block-3x4x5",
        "primitives_solid",
        part,
        vec![],
        built(1, 1, 0, cells(8, 12, true, &[(Kind::Plane, 1, 0, 6)])),
        truth,
    ));
    // 72 §4.2: built at the frame, so the vertices round once. Far from the origin and
    // turned, as `boxes_far_from_the_origin_build_closed_and_valid`.
    let part = Part {
        frame: tilt([3.0e5, -2.5e5, 4.0e5]),
        shape: Shape::Block {
            size: [10.0, 20.0, 30.0],
        },
    };
    let truth = truth_part(&part);
    v.push(recipe(
        "block-far-turned",
        "primitives_solid::boxes_far_from_the_origin_build_closed_and_valid",
        part,
        vec![],
        built(1, 1, 0, cells(8, 12, true, &[(Kind::Plane, 1, 0, 6)])),
        truth,
    ));
    // `small_boxes_at_1e4_answer_as_at_the_origin`, in units of the resolution.
    let part = block_at(
        [1.0e4, 1.0e4, 1.0e4],
        [100.0 * res, 200.0 * res, 300.0 * res],
    );
    let truth = truth_part(&part);
    v.push(recipe(
        "block-small-at-1e4",
        "primitives_solid::small_boxes_at_1e4_answer_as_at_the_origin",
        part,
        vec![],
        built(1, 1, 0, cells(8, 12, true, &[(Kind::Plane, 1, 0, 6)])),
        truth,
    ));
    // The size floor is the resolution, strictly above (MK1).
    let just_above = block_at([0.0; 3], [2.0 * res, 3.0, 4.0]);
    let truth = truth_part(&just_above);
    v.push(recipe(
        "block-size-twice-resolution",
        "new",
        just_above,
        vec![],
        built(1, 1, 0, cells(8, 12, true, &[(Kind::Plane, 1, 0, 6)])),
        truth,
    ));
    for (name, size, dim) in [
        ("block-x-zero", [0.0, 1.0, 1.0], Dimension::X),
        ("block-y-negative", [1.0, -1.0, 1.0], Dimension::Y),
        ("block-z-nan", [1.0, 1.0, f64::NAN], Dimension::Z),
        ("block-x-at-resolution", [res, 1.0, 1.0], Dimension::X),
        (
            "block-x-below-resolution",
            [0.5 * res, 1.0, 1.0],
            Dimension::X,
        ),
    ] {
        v.push(recipe(
            name,
            "api_primitives (size refusals)",
            block_at([0.0; 3], size),
            vec![],
            Outcome::Refused(Refusal::Size(dim)),
            None,
        ));
    }

    // --- cylinder, cone ---------------------------------------------------------------
    let part = cylinder_at([1.0, 2.0, 3.0], 2.0, 10.0);
    let truth = truth_part(&part);
    v.push(recipe(
        "cylinder-r2-h10",
        "primitives_are_closed_solids_with_analytic_properties",
        part,
        vec![],
        built(
            1,
            1,
            0,
            cells(
                2,
                2,
                false,
                &[(Kind::Plane, 1, 0, 2), (Kind::Cylinder, 2, 0, 1)],
            ),
        ),
        truth,
    ));
    let part = Part {
        frame: tilt([-4.0, 5.0, 6.0]),
        shape: Shape::Cylinder {
            radius: 1.5,
            height: 7.0,
        },
    };
    let truth = truth_part(&part);
    v.push(recipe(
        "cylinder-turned",
        "primitives_are_closed_solids_with_analytic_properties",
        part,
        vec![],
        built(
            1,
            1,
            0,
            cells(
                2,
                2,
                false,
                &[(Kind::Plane, 1, 0, 2), (Kind::Cylinder, 2, 0, 1)],
            ),
        ),
        truth,
    ));
    for (name, radii) in [
        ("cone-frustum-3-1", [3.0, 1.0]),
        ("cone-frustum-1-3", [1.0, 3.0]),
    ] {
        let part = Part {
            frame: frame_at([0.5, -1.0, 2.0]),
            shape: Shape::Cone { radii, height: 4.0 },
        };
        let truth = truth_part(&part);
        v.push(recipe(
            name,
            "primitives_are_closed_solids_with_analytic_properties",
            part,
            vec![],
            built(
                1,
                1,
                0,
                cells(
                    2,
                    2,
                    false,
                    &[(Kind::Plane, 1, 0, 2), (Kind::Cone, 2, 0, 1)],
                ),
            ),
            truth,
        ));
    }
    // An apex: the wall is one patch with one loop (the base circle) and the cap is the
    // only other.
    for (name, radii) in [("cone-apex-up", [3.0, 0.0]), ("cone-apex-down", [0.0, 3.0])] {
        let part = Part {
            frame: tilt([1.0, 1.0, -2.0]),
            shape: Shape::Cone { radii, height: 4.0 },
        };
        let truth = truth_part(&part);
        v.push(recipe(
            name,
            "primitives_are_closed_solids_with_analytic_properties",
            part,
            vec![],
            built(
                1,
                1,
                0,
                cells(
                    1,
                    1,
                    false,
                    &[(Kind::Plane, 1, 0, 1), (Kind::Cone, 1, 0, 1)],
                ),
            ),
            truth,
        ));
    }

    // --- sphere, torus -----------------------------------------------------------------
    let part = sphere_at([1.0, 2.0, 3.0], 5.0);
    let truth = truth_part(&part);
    v.push(recipe(
        "sphere-r5",
        "precise_sphere_properties_scale_and_translate",
        part,
        vec![],
        built(1, 1, 0, cells(0, 0, false, &[(Kind::Sphere, 0, 0, 1)])),
        truth,
    ));
    let part = torus_at([0.0; 3], 5.0, 1.5);
    let truth = truth_part(&part);
    v.push(recipe(
        "torus-5-1.5",
        "complete_spheres_and_tori_have_analytic_properties",
        part,
        vec![],
        built(1, 1, 1, cells(0, 0, false, &[(Kind::Torus, 0, 1, 1)])),
        truth,
    ));
    // The horn torus: the tube meets the axis at one point. Built since MK4b (the pole
    // is a vertex). The patch complex is not stated: a pinched torus is no patch class here.
    let part = torus_at([0.0; 3], 3.0, 3.0);
    let truth = truth_part(&part);
    v.push(
        recipe(
            "torus-horn",
            "new (72 D3, built by MK4b)",
            part,
            vec![],
            Outcome::Built(Topology {
                solid_regions: 1,
                shells: 1,
                genus: 1,
                cells: None,
            }),
            truth,
        )
        .pinched(),
    );
    v.push(recipe(
        "torus-spindle",
        "new (72 §4.2)",
        torus_at([0.0; 3], 3.0, 4.0),
        vec![],
        Outcome::Refused(Refusal::SelfIntersecting),
        None,
    ));

    // --- refused sizes of the other primitives -----------------------------------------
    v.push(recipe(
        "cylinder-radius-zero",
        "api_primitives (size refusals)",
        cylinder_at([0.0; 3], 0.0, 1.0),
        vec![],
        Outcome::Refused(Refusal::Size(Dimension::Radius)),
        None,
    ));
    v.push(recipe(
        "cylinder-height-infinite",
        "api_primitives (size refusals)",
        cylinder_at([0.0; 3], 1.0, f64::INFINITY),
        vec![],
        Outcome::Refused(Refusal::Size(Dimension::Height)),
        None,
    ));
    v.push(recipe(
        "sphere-radius-negative",
        "api_primitives (size refusals)",
        sphere_at([0.0; 3], -1.0),
        vec![],
        Outcome::Refused(Refusal::Size(Dimension::Radius)),
        None,
    ));
    v.push(recipe(
        "torus-minor-zero",
        "api_primitives (size refusals)",
        torus_at([0.0; 3], 3.0, 0.0),
        vec![],
        Outcome::Refused(Refusal::Size(Dimension::Minor)),
        None,
    ));
    v.push(recipe(
        "cone-both-radii-zero",
        "new (72 §4.2)",
        Part {
            frame: frame_at([0.0; 3]),
            shape: Shape::Cone {
                radii: [0.0, 0.0],
                height: 1.0,
            },
        },
        vec![],
        Outcome::Refused(Refusal::NoBody),
        None,
    ));

    // --- extrusions of line-and-arc profiles (MK3) ---------------------------------------
    let part = extrusion(
        frame_at([1.0, 2.0, 3.0]),
        square(0.0, 0.0, 4.0),
        vec![],
        3.0,
    );
    let truth = truth_part(&part);
    v.push(recipe(
        "extrude-square",
        "sweep/extrude.rs",
        part,
        vec![],
        built(1, 1, 0, cells(8, 12, true, &[(Kind::Plane, 1, 0, 6)])),
        truth,
    ));
    // A D: the line's wall and the two caps are planes, the arc's wall a cylinder. Its
    // four vertices are the line's ends at each cap.
    let part = extrusion(tilt([0.0, 0.0, 1.0]), d_profile(), vec![], 2.0);
    let truth = truth_part(&part);
    v.push(recipe(
        "extrude-d",
        "sweep/extrude.rs",
        part,
        vec![],
        built(
            1,
            1,
            0,
            cells(
                4,
                6,
                true,
                &[(Kind::Plane, 1, 0, 3), (Kind::Cylinder, 1, 0, 1)],
            ),
        ),
        truth,
    ));
    let part = extrusion(frame_at([0.0; 3]), rounded_top(), vec![], 5.0);
    let truth = truth_part(&part);
    v.push(recipe(
        "extrude-rounded-top",
        "sweep/extrude.rs",
        part,
        vec![],
        built(
            1,
            1,
            0,
            cells(
                8,
                12,
                true,
                &[(Kind::Plane, 1, 0, 5), (Kind::Cylinder, 1, 0, 1)],
            ),
        ),
        truth,
    ));
    // A square with a square hole: a tube of square section, genus 1. The two caps have two
    // loops each.
    let part = extrusion(
        frame_at([0.0; 3]),
        square(0.0, 0.0, 10.0),
        vec![reversed(&square(3.0, 3.0, 3.0))],
        4.0,
    );
    let truth = truth_part(&part);
    v.push(recipe(
        "extrude-square-with-square-hole",
        "sweep/extrude.rs",
        part,
        vec![],
        built(
            1,
            1,
            1,
            cells(
                16,
                24,
                true,
                &[(Kind::Plane, 2, 0, 2), (Kind::Plane, 1, 0, 8)],
            ),
        ),
        truth,
    ));
    // The span's two heights are equal: the solid has no thickness. The recipe's frame
    // carries the lower height and `height` the span.
    v.push(recipe(
        "extrude-span-zero",
        "new (72 §4.2)",
        extrusion(frame_at([0.0; 3]), square(0.0, 0.0, 4.0), vec![], 0.0),
        vec![],
        Outcome::Refused(Refusal::Span),
        None,
    ));
    v
}
