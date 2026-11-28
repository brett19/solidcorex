// Written for solidcorex. Apache-2.0.
//! The Booleans' recipes (72 §5, BO1 to BO7): the pin's `boolean_api` cases, the near-coplanar
//! and lineage cases on two blocks, the two benchmark bodies (`plate_12`, `gear_60`) and a few
//! topology cases 72's §8.1 asks for (regions, shells, a cavity, a contact, an empty result).

use core::f64::consts::{PI, TAU};

use scx_num::fmath;

use super::convex::{Hull, overlap};
use super::{
    BoxFace, FaceOrigin, Invariant, Kind, Lineage, Op, Outcome, Part, Recipe, Shape, Truth,
    Unsupported, block_at, built, cells, cylinder_at, frame_at, recipe, sphere_at, step, torus_at,
    truth_of,
};
use crate::analytic::Frame;
use crate::solid::{self, Hole};

// The recipe's field is an `Option`, and most callers pass the answer straight in.
#[allow(clippy::unnecessary_wraps)]
fn truth(volume: f64, area: f64) -> Option<Truth> {
    Some(Truth {
        volume,
        area: Some(area),
        slack: 0.0,
    })
}

/// The truth of a disjoint pair: volumes and areas add.
fn sum_truth(a: &Part, b: &Part) -> Option<Truth> {
    let (a, b) = (a.solid().ok()?, b.solid().ok()?);
    truth(a.volume() + b.volume(), a.area + b.area)
}

fn face(area: f64, origins: &[(u8, BoxFace)]) -> FaceOrigin {
    FaceOrigin {
        area,
        origins: origins.to_vec(),
    }
}

/// `n` faces of one area and origin kind: the three faces of a block at `lo` (bottom, front,
/// left) or at `hi` (top, back, right) for operand `op`.
fn three(area: f64, op: u8, high: bool) -> Vec<FaceOrigin> {
    let faces = if high {
        [BoxFace::Top, BoxFace::Back, BoxFace::Right]
    } else {
        [BoxFace::Bottom, BoxFace::Front, BoxFace::Left]
    };
    faces.iter().map(|&f| face(area, &[(op, f)])).collect()
}

/// The elliptic overlap of `tpipe`'s branch with its main pipe:
/// `int 2 sqrt(r^2 - x^2) sqrt(R^2 - x^2) dx` over `|x| <= r`, with `r = 3/2`, `R = 2`. The
/// substitution `x = r sin t` gives the smooth `pi`-periodic integrand
/// `2 r^2 cos^2 t sqrt(R^2 - r^2 sin^2 t)`, so the plain periodic trapezoid rule converges
/// geometrically (the nearest singularity is at imaginary part `acosh(R / r)`); 1024 nodes
/// are far below rounding.
fn tpipe_overlap() -> f64 {
    let n = 1024_u32;
    let (r, big) = (1.5, 2.0);
    let sum: f64 = (0..n)
        .map(|j| {
            let t = -0.5 * PI + PI * f64::from(j) / f64::from(n);
            let (s, c) = fmath::sin_cos(t);
            2.0 * r * r * c * c * (big * big - r * r * s * s).sqrt()
        })
        .sum();
    sum * PI / f64::from(n)
}

/// B of the crease cases: a 10 x 10 x 20 block at `(0, 0, -10)` turned 45 degrees about `z`
/// then 45 degrees about `x`, then moved so its crease (the edge from `(0, 0, -10)` to
/// `(0, 0, 10)` before turning) crosses A's edge `y = z = 10` at depth `d` (the pin's
/// `boolean_no_dangling_vertex_refs` crease).
fn crease_block(d: f64) -> Part {
    let (s, c) = fmath::sin_cos(0.25 * PI);
    // Rx(45) Rz(45): the images of the world's axes.
    let x = [c, s * c, s * s];
    let y = [-s, c * c, c * s];
    let z = [0.0, -s, c];
    let shift = d / 2.0_f64.sqrt();
    let origin = [5.0, 10.0 - shift + 10.0 * s, 10.0 - shift - 10.0 * c];
    Part {
        frame: Frame { origin, x, y, z },
        shape: Shape::Block {
            size: [10.0, 10.0, 20.0],
        },
    }
}

/// The plate of `plate_holes_k`: a 100 x 100 x 10 block less `k * k` through-cylinders of
/// radius 2 at the centres of a `k * k` grid of cells, cut one at a time with cylinders 12
/// high from `z = -1` (68 §1; the runner's own pitch is not recorded, the volume and counts
/// do not depend on it).
fn plate(name: &'static str, k: u32) -> Recipe {
    let pitch = 100.0 / f64::from(k);
    let centres: Vec<[f64; 2]> = (0..k)
        .flat_map(|j| (0..k).map(move |i| [f64::from(i), f64::from(j)]))
        .map(|[i, j]| [(i + 0.5) * pitch, (j + 0.5) * pitch])
        .collect();
    let steps = centres
        .iter()
        .map(|c| step(Op::Difference, cylinder_at([c[0], c[1], -1.0], 2.0, 12.0)))
        .collect();
    let holes: Vec<Hole> = centres
        .iter()
        .map(|c| Hole {
            x: c[0],
            y: c[1],
            r: 2.0,
        })
        .collect();
    let truth = solid::plate(100.0, 100.0, 10.0, &holes)
        .ok()
        .as_ref()
        .map(truth_of);
    let n = k * k;
    // 8 corners and one vertex a circle at each end of each hole; 12 box edges and one
    // circle edge a hole end. Patches: 4 sides, 2 caps of 1 + n loops, n walls of 2 loops.
    recipe(
        name,
        "benchmark plate_holes (research 68)",
        block_at([0.0; 3], [100.0, 100.0, 10.0]),
        steps,
        built(
            1,
            1,
            n,
            cells(
                8 + 2 * n,
                12 + 2 * n,
                false,
                &[
                    (Kind::Plane, 1, 0, 4),
                    (Kind::Plane, 1 + n, 0, 2),
                    (Kind::Cylinder, 2, 0, n),
                ],
            ),
        ),
        truth,
    )
}

/// `gear_60` (research 68 §1): a cylinder of radius 50 and height 10 less 60 boxes
/// `10 x gap x 12` at `(45, -gap/2, -1)`, each turned about `z` by `0.1234 + 2 pi i / 60`.
fn gear_60() -> Recipe {
    let gap = 2.0 * PI * 45.0 / 60.0 * 0.45;
    let pitch = TAU / 60.0;
    let steps = (0..60_u32)
        .map(|i| {
            let (s, c) = fmath::sin_cos(0.1234 + pitch * f64::from(i));
            let (ox, oy) = (45.0, -gap / 2.0);
            step(
                Op::Difference,
                Part {
                    frame: Frame {
                        origin: [c * ox - s * oy, s * ox + c * oy, -1.0],
                        x: [c, s, 0.0],
                        y: [-s, c, 0.0],
                        z: [0.0, 0.0, 1.0],
                    },
                    shape: Shape::Block {
                        size: [10.0, gap, 12.0],
                    },
                },
            )
        })
        .collect();
    let truth = solid::gear(50.0, 10.0, 60, 45.0, gap, 0.1234)
        .ok()
        .as_ref()
        .map(truth_of);
    // Per slot: 4 corners on each cap; per cap 4 edges a slot (floor, two walls, the rim's
    // arc between slots); 4 verticals. Patches: 2 caps, 60 rim pieces, 180 slot faces.
    recipe(
        "gear-60",
        "benchmark gear_60 (research 68)",
        cylinder_at([0.0; 3], 50.0, 10.0),
        steps,
        built(
            1,
            1,
            0,
            cells(
                480,
                720,
                true,
                &[
                    (Kind::Plane, 1, 0, 2),
                    (Kind::Cylinder, 1, 0, 60),
                    (Kind::Plane, 1, 0, 180),
                ],
            ),
        ),
        truth,
    )
}

/// The union of the `n` x `n` interleaved strips of `boolean_coplanar_merge` (research 72
/// §5.7): strips `1` wide, `2` apart, `2 n` long, the ones along `x` `1` high and those
/// along `y` `2` high, the even half united with the odd half. The volume is `5 n^2`
/// (the pin's `n n L w (1 + 2) - n^2 w^2`), the area `14 n^2 + 6 n` (summing the height
/// steps of the unit cells), the patches `3 n^2 + 4 n + 2`, the genus `(n-1)^2` (the holes
/// of the bottom), and the vertices and edges are those of a simple polyhedron with that
/// many faces, loops and handles (`3 V = 2 E`).
fn grids(name: &'static str, n: u32, vertices: u32, edges: u32) -> Recipe {
    let len = f64::from(2 * n);
    let strip = |i: u32| block_at([0.0, 2.0 * f64::from(i), 0.0], [len, 1.0, 1.0]);
    let bar = |i: u32| block_at([2.0 * f64::from(i), 0.0, 0.0], [1.0, len, 2.0]);
    let mut steps = Vec::new();
    for parity in 0..2 {
        for i in (0..n).filter(|i| i % 2 == parity) {
            if i != 0 {
                steps.push(step(Op::Union, strip(i)));
            }
            steps.push(step(Op::Union, bar(i)));
        }
    }
    let nf = f64::from(n);
    let faces = 3 * n * n + 4 * n + 2;
    let handles = (n - 1) * (n - 1);
    recipe(
        name,
        "boolean_coplanar_merge::interleaved_grids_on_one_plane",
        strip(0),
        steps,
        built(
            1,
            1,
            handles,
            cells(
                vertices,
                edges,
                true,
                &[
                    (Kind::Plane, 1 + handles, 0, 1),
                    (Kind::Plane, 1, 0, faces - 1),
                ],
            ),
        ),
        truth(5.0 * nf * nf, 14.0 * nf * nf + 6.0 * nf),
    )
}

pub(super) fn recipes(res: f64) -> Vec<Recipe> {
    let mut v = Vec::new();
    let cube = block_at([0.0; 3], [10.0, 10.0, 10.0]);
    let cube_solid = cube.solid().ok();
    let (cube_v, cube_a) = cube_solid.map_or((f64::NAN, f64::NAN), |s| (s.volume(), s.area));

    // --- boolean_api: the five that are built -------------------------------------------
    // 10^3 box less an r = 2 through-hole of length 10. The patches are 4 sides, 2 annular
    // caps and the bore; the 8 corners and a vertex on each circle; a genus-1 solid.
    let truth_hole = solid::plate(
        10.0,
        10.0,
        10.0,
        &[Hole {
            x: 5.0,
            y: 5.0,
            r: 2.0,
        }],
    )
    .ok()
    .as_ref()
    .map(truth_of);
    v.push(recipe(
        "box-cyl-hole",
        "boolean_api::cases",
        cube.clone(),
        vec![step(
            Op::Difference,
            cylinder_at([5.0, 5.0, -1.0], 2.0, 12.0),
        )],
        built(
            1,
            1,
            1,
            cells(
                10,
                14,
                false,
                &[
                    (Kind::Plane, 1, 0, 4),
                    (Kind::Plane, 2, 0, 2),
                    (Kind::Cylinder, 2, 0, 1),
                ],
            ),
        ),
        truth_hole,
    ));

    // Two 10^3 boxes overlapping in a 5^3 cube (also the `issue_1462` union): the union has
    // the 6 L-shaped faces and 6 whole ones; the volume is 2000 - 125, the area 1200 less
    // the 75 of each cube's surface that is inside the other, twice.
    let ab = || block_at([5.0, 5.0, 5.0], [10.0, 10.0, 10.0]);
    let mut union_faces = three(100.0, 0, false);
    union_faces.extend(three(75.0, 0, true));
    union_faces.extend(three(100.0, 1, true));
    union_faces.extend(three(75.0, 1, false));
    v.push(
        recipe(
            "box-box-union",
            "boolean_api::cases; issue_1462_boolean_attributes (union, edges, attributes)",
            cube.clone(),
            vec![step(Op::Union, ab())],
            built(1, 1, 0, cells(20, 30, true, &[(Kind::Plane, 1, 0, 12)])),
            truth(2.0 * cube_v - 125.0, 2.0 * cube_a - 150.0),
        )
        .with_lineage(Lineage {
            faces: union_faces,
            new_edges: Some(6),
            halved_edges: Some([3, 3]),
        }),
    );
    let mut diff_faces = three(100.0, 0, false);
    diff_faces.extend(three(75.0, 0, true));
    diff_faces.extend(three(25.0, 1, false));
    v.push(
        recipe(
            "box-box-difference",
            "issue_1462_boolean_attributes::difference_of_overlapping_boxes",
            cube.clone(),
            vec![step(Op::Difference, ab())],
            built(1, 1, 0, cells(14, 21, true, &[(Kind::Plane, 1, 0, 9)])),
            truth(cube_v - 125.0, cube_a - 75.0 + 75.0),
        )
        .with_lineage(Lineage {
            faces: diff_faces,
            new_edges: Some(6),
            halved_edges: None,
        }),
    );
    let mut meet_faces = three(25.0, 0, true);
    meet_faces.extend(three(25.0, 1, false));
    v.push(
        recipe(
            "box-box-intersection",
            "issue_1462_boolean_attributes::intersection_of_overlapping_boxes",
            cube.clone(),
            vec![step(Op::Intersection, ab())],
            built(1, 1, 0, cells(8, 12, true, &[(Kind::Plane, 1, 0, 6)])),
            truth(125.0, 150.0),
        )
        .with_lineage(Lineage {
            faces: meet_faces,
            new_edges: Some(6),
            halved_edges: None,
        }),
    );
    // B shares A's z = 0 and z = 10 planes: the two L-shaped caps are each one face made of
    // a face of each box (the merge keeps both origins). The plan is an 8-gon of area 175
    // and perimeter 60, a prism of height 10.
    let mut flange_faces = vec![
        face(175.0, &[(0, BoxFace::Bottom), (1, BoxFace::Bottom)]),
        face(175.0, &[(0, BoxFace::Top), (1, BoxFace::Top)]),
        face(100.0, &[(0, BoxFace::Front)]),
        face(50.0, &[(0, BoxFace::Right)]),
        face(50.0, &[(0, BoxFace::Back)]),
        face(100.0, &[(0, BoxFace::Left)]),
        face(50.0, &[(1, BoxFace::Front)]),
        face(100.0, &[(1, BoxFace::Right)]),
        face(100.0, &[(1, BoxFace::Back)]),
        face(50.0, &[(1, BoxFace::Left)]),
    ];
    flange_faces.sort_by(|a, b| a.origins.cmp(&b.origins));
    v.push(
        recipe(
            "box-box-union-flush",
            "issue_1462_boolean_attributes::coplanar_faces_merged_by_the_cleanup_keep_both_origins",
            cube.clone(),
            vec![step(
                Op::Union,
                block_at([5.0, 5.0, 0.0], [10.0, 10.0, 10.0]),
            )],
            built(1, 1, 0, cells(16, 24, true, &[(Kind::Plane, 1, 0, 10)])),
            truth(2.0 * cube_v - 250.0, 2.0 * 175.0 + 60.0 * 10.0),
        )
        .with_lineage(Lineage {
            faces: flange_faces,
            new_edges: None,
            halved_edges: None,
        }),
    );

    // r = 5 sphere less the cap above z = 2, as a box z <= 2 cuts it: the sphere's patch
    // with one loop and the flat disc. Zone `2 pi r (r + 2)` plus the disc of `r^2 - 4`.
    v.push(recipe(
        "sphere-and-box",
        "boolean_api::cases",
        sphere_at([0.0; 3], 5.0),
        vec![step(
            Op::Intersection,
            block_at([-10.0, -10.0, -10.0], [20.0, 20.0, 12.0]),
        )],
        built(
            1,
            1,
            0,
            cells(
                1,
                1,
                false,
                &[(Kind::Sphere, 1, 0, 1), (Kind::Plane, 1, 0, 1)],
            ),
        ),
        solid::sphere(5.0)
            .ok()
            .zip(solid::sphere_cap(5.0, 3.0).ok())
            .and_then(|(s, cap)| {
                truth(
                    s.volume() - cap.volume(),
                    2.0 * PI * 5.0 * 7.0 + PI * (25.0 - 4.0),
                )
            }),
    ));

    // 10^3 box plus a 10 x 5 x 10 box sharing the x = 10 plane: the plan is a 6-gon of area
    // 150 and perimeter 60, a prism of height 10.
    v.push(recipe(
        "coplanar-union",
        "boolean_api::cases",
        cube.clone(),
        vec![step(
            Op::Union,
            block_at([10.0, 0.0, 0.0], [10.0, 5.0, 10.0]),
        )],
        built(1, 1, 0, cells(12, 18, true, &[(Kind::Plane, 1, 0, 8)])),
        truth(1500.0, 2.0 * 150.0 + 60.0 * 10.0),
    ));

    // Half of an R = 5, r = 1.5 torus: the tube's wall with two loops (the two meridian
    // circles in x = 0) and the two discs.
    let half = solid::torus_sector(5.0, 1.5, PI).ok();
    v.push(recipe(
        "torus-minus-box",
        "boolean_api::cases",
        torus_at([0.0; 3], 5.0, 1.5),
        vec![step(
            Op::Difference,
            block_at([0.0, -10.0, -10.0], [20.0, 20.0, 20.0]),
        )],
        built(
            1,
            1,
            0,
            cells(
                2,
                2,
                false,
                &[(Kind::Torus, 2, 0, 1), (Kind::Plane, 1, 0, 2)],
            ),
        ),
        half.as_ref().map(truth_of),
    ));

    // --- boolean_api: a touch inside a face (BO25: built; D5 had it NonManifold) ----------
    // Two lumps touching at a point are two regions of one shell each; the box less its
    // inscribed ball is one region of two shells. A vertex there has two fans: a pinch.
    // Counted per sheet, the touch is a vertex of each, and a point loop of the patch it
    // lies in: a block touched in a face's inside has 9 vertices and that face 2 loops.
    let ball_on_block = || {
        built(
            2,
            2,
            0,
            cells(
                10,
                12,
                false,
                &[
                    (Kind::Plane, 1, 0, 5),
                    (Kind::Plane, 2, 0, 1),
                    (Kind::Sphere, 1, 0, 1),
                ],
            ),
        )
    };
    let tangent = sphere_at([5.0, 5.0, 13.0], 3.0);
    let t = sum_truth(&cube, &tangent);
    v.push(
        recipe(
            "tangent-sphere-box",
            "boolean_api::cases",
            cube.clone(),
            vec![step(Op::Union, tangent)],
            ball_on_block(),
            t,
        )
        .pinched(),
    );
    let flat = || block_at([10.0, 0.0, 0.0], [5.0, 17.5, 15.0]);
    let seam_ball = || sphere_at([2.5, 15.0, 12.5], 7.5);
    let t = sum_truth(&seam_ball(), &flat());
    v.push(
        recipe(
            "seam-sphere-box",
            "boolean_api::point_contacts",
            seam_ball(),
            vec![step(Op::Union, flat())],
            ball_on_block(),
            t,
        )
        .pinched(),
    );
    v.push(
        recipe(
            "seam-box-sphere",
            "boolean_api::point_contacts",
            flat(),
            vec![step(Op::Union, seam_ball())],
            ball_on_block(),
            t,
        )
        .pinched(),
    );
    let off_ball = sphere_at([12.5, -7.5, 7.5], 7.5);
    let t = sum_truth(&off_ball, &flat());
    v.push(
        recipe(
            "off-seam-sphere-box",
            "boolean_api::point_contacts",
            off_ball,
            vec![step(Op::Union, flat())],
            ball_on_block(),
            t,
        )
        .pinched(),
    );
    let (s30, c30) = fmath::sin_cos(PI / 6.0);
    let big = sphere_at([0.0; 3], 3.0);
    let small = sphere_at([5.0 * c30, 0.0, 5.0 * s30], 2.0);
    let t = sum_truth(&big, &small);
    v.push(
        recipe(
            "seam-sphere-sphere",
            "boolean_api::point_contacts",
            big,
            vec![step(Op::Union, small)],
            built(2, 2, 0, cells(2, 0, false, &[(Kind::Sphere, 1, 0, 2)])),
            t,
        )
        .pinched(),
    );
    // The box less its inscribed r = 5 sphere, which touches every face at its centre.
    v.push(
        recipe(
            "seam-box-minus-sphere",
            "boolean_api::point_contacts",
            cube.clone(),
            vec![step(Op::Difference, sphere_at([5.0; 3], 5.0))],
            built(
                1,
                2,
                0,
                cells(
                    20,
                    12,
                    false,
                    &[(Kind::Plane, 2, 0, 6), (Kind::Sphere, 6, 0, 1)],
                ),
            ),
            truth(cube_v - 500.0 * PI / 3.0, cube_a + 100.0 * PI),
        )
        .pinched(),
    );

    // --- boolean_api: pairs the analytic SSI has no closed form for (SS2, spiric) ---------
    let ssi = |a: Kind, b: Kind| Outcome::Unsupported(Unsupported::SsiPair { kinds: [a, b] });
    // r = 2, h = 20 pipe plus an r = 1.5, h = 10 branch along +y from the axis; the branch's
    // overlap with the pipe is the quadrature above.
    let pipe = cylinder_at([0.0, 0.0, -10.0], 2.0, 20.0);
    let branch = Part {
        frame: Frame {
            origin: [0.0; 3],
            x: [0.0, 0.0, 1.0],
            y: [1.0, 0.0, 0.0],
            z: [0.0, 1.0, 0.0],
        },
        shape: Shape::Cylinder {
            radius: 1.5,
            height: 10.0,
        },
    };
    let tpipe_truth = Some(Truth {
        volume: 102.5 * PI - tpipe_overlap(),
        area: None,
        slack: 0.0,
    });
    v.push(recipe(
        "tpipe",
        "boolean_api::cases",
        pipe,
        vec![step(Op::Union, branch)],
        ssi(Kind::Cylinder, Kind::Cylinder),
        tpipe_truth,
    ));
    // A sphere touching a cylinder or ring torus off its spine: BO22's closed form gives the
    // one point, so the union is two lumps touching there (BO25's point loops), pinched. The
    // cells are not stated (the touch lies on a seam).
    let touching = |genus: u32| {
        Outcome::Built(super::Topology {
            solid_regions: 2,
            shells: 2,
            genus,
            cells: None,
        })
    };
    let post = cylinder_at([0.0; 3], 2.0, 10.0);
    for (name, centre) in [
        ("seam-cylinder-sphere", [5.0, 0.0, 5.0]),
        ("off-seam-cylinder-sphere", [0.0, 5.0, 5.0]),
    ] {
        let ball = sphere_at(centre, 3.0);
        let t = sum_truth(&post, &ball);
        v.push(
            recipe(
                name,
                "boolean_api::point_contacts",
                post.clone(),
                vec![step(Op::Union, ball)],
                touching(0),
                t,
            )
            .pinched(),
        );
    }
    let (s45, c45) = fmath::sin_cos(0.25 * PI);
    let ring = torus_at([0.0; 3], 5.0, 1.5);
    let ball = sphere_at([5.0 + 3.5 * c45, 0.0, 3.5 * s45], 2.0);
    let t = sum_truth(&ring, &ball);
    v.push(
        recipe(
            "seam-torus-sphere",
            "boolean_api::point_contacts",
            ring.clone(),
            vec![step(Op::Union, ball)],
            touching(1),
            t,
        )
        .pinched(),
    );
    for (name, origin, size) in [
        ("seam-torus-box", [-3.0, 6.5, -3.0], [6.0, 4.0, 6.0]),
        ("seam-vertex-torus-box", [6.5, -3.0, -3.0], [4.0, 6.0, 6.0]),
    ] {
        let slab = block_at(origin, size);
        let t = sum_truth(&ring, &slab);
        v.push(recipe(
            name,
            "boolean_api::point_contacts",
            ring.clone(),
            vec![step(Op::Union, slab)],
            ssi(Kind::Plane, Kind::Torus),
            t,
        ));
    }

    // --- near-coplanar fronts (boolean_coplanar_merge) --------------------------------------
    // A 10^3 cube united with a 10^3 block at x = 5 moved `dy` along y. Within the
    // resolution the fronts are one plane and the union is the 15 x 10 x 10 block: the exact
    // union differs from it by the gap times the area, which is the truth's slack. Past it
    // the fronts stay two faces: the plan is an 8-gon of area 150 + 5 dy and perimeter
    // 50 + 2 dy, a prism of height 10.
    for (name, k) in [
        ("coplanar-offset-0", 0.0),
        ("coplanar-offset-0.1c", 0.1),
        ("coplanar-offset-0.5c", 0.5),
        ("coplanar-offset-4c", 4.0),
        ("coplanar-offset-10c", 10.0),
    ] {
        let dy = k * res;
        let merged = k <= 0.5;
        let outcome = if merged {
            built(1, 1, 0, cells(8, 12, true, &[(Kind::Plane, 1, 0, 6)]))
        } else {
            built(1, 1, 0, cells(16, 24, true, &[(Kind::Plane, 1, 0, 10)]))
        };
        let t = Truth {
            volume: 1500.0 + 50.0 * dy,
            area: Some(800.0 + 30.0 * dy),
            slack: if merged { 1000.0 * res } else { 0.0 },
        };
        v.push(recipe(
            name,
            "boolean_coplanar_merge::near_coplanar_fronts",
            cube.clone(),
            vec![step(
                Op::Union,
                block_at([5.0, dy, 0.0], [10.0, 10.0, 10.0]),
            )],
            outcome,
            Some(t),
        ));
    }

    // --- crossing creases (boolean_no_dangling_vertex_refs) ---------------------------------
    // A 10^3 cube united with a 10 x 10 x 20 block turned 45 degrees twice, whose crease
    // passes `d` into the cube's edge y = z = 10. Both are convex and no faces are coplanar,
    // so volume and area come from clipping (`convex`). Below the resolution the crease
    // touches the edge: the merged points rule may settle it either way. At 0.3c the overlap
    // is a tetrahedron about `d` across, every corner of it within the resolution of one
    // point, so a body that keeps the overlap meets itself there: two lumps at one vertex,
    // a pinch (D5, 72 §14.42), and the mesh's non-manifold vertex is the truth.
    for (name, k) in [
        ("crease-0.3c", 0.3),
        ("crease-3c", 3.0),
        ("crease-10c", 10.0),
    ] {
        let b = crease_block(k * res);
        let t = overlap(
            &Hull::block(&frame_at([0.0; 3]), [10.0, 10.0, 10.0]),
            &Hull::block(&b.frame, [10.0, 10.0, 20.0]),
        )
        .map(|o| {
            let (vol, area) = o.union(
                &Hull::block(&frame_at([0.0; 3]), [10.0, 10.0, 10.0]),
                &Hull::block(&b.frame, [10.0, 10.0, 20.0]),
            );
            Truth {
                volume: vol,
                area: Some(area),
                slack: 0.0,
            }
        });
        let outcome = if k < 1.0 {
            Outcome::Open("the crease is within the resolution of the cube's edge")
        } else {
            Outcome::Built(super::Topology {
                solid_regions: 1,
                shells: 1,
                genus: 0,
                cells: None,
            })
        };
        v.push(
            recipe(
                name,
                "boolean_no_dangling_vertex_refs",
                cube.clone(),
                vec![step(Op::Union, b)],
                outcome,
                t,
            )
            .also(Invariant::NoDanglingVertices),
        );
        if k < 1.0
            && let Some(r) = v.pop()
        {
            v.push(r.pinched());
        }
    }

    // --- contacts, regions, shells, empty results (72 §8.1) --------------------------------
    // Two cubes sharing the corner (10, 10, 10): two solid regions touching at a vertex
    // (D5: built). Counted per sheet, each cube has its own 8 vertices.
    let t = truth(2.0 * cube_v, 2.0 * cube_a);
    v.push(
        recipe(
            "corner-contact-union",
            "new (72 BO7)",
            cube.clone(),
            vec![step(
                Op::Union,
                block_at([10.0, 10.0, 10.0], [10.0, 10.0, 10.0]),
            )],
            built(2, 2, 0, cells(16, 24, true, &[(Kind::Plane, 1, 0, 12)])),
            t,
        )
        .pinched(),
    );
    // A diamond prism (a square turned 45 degrees about z, half-diagonal 5) whose vertical
    // edge lies on the cube's face x = 10 at y = 5: the two touch along a line. The cube's
    // face is split by the contact edge (7 patches), the diamond has 6; the contact edge
    // and the cube's top and bottom edges through it give 2 more vertices on the cube.
    let h = 0.5_f64.sqrt();
    let side = 5.0 * 2.0_f64.sqrt();
    let diamond = Part {
        frame: Frame {
            origin: [15.0, 0.0, 0.0],
            x: [h, h, 0.0],
            y: [-h, h, 0.0],
            z: [0.0, 0.0, 1.0],
        },
        shape: Shape::Block {
            size: [side, side, 10.0],
        },
    };
    let t = truth(cube_v + 500.0, cube_a + 100.0 + 200.0 * 2.0_f64.sqrt());
    v.push(
        recipe(
            "edge-contact-union",
            "new (72 BO7)",
            cube.clone(),
            vec![step(Op::Union, diamond)],
            built(2, 2, 0, cells(18, 27, true, &[(Kind::Plane, 1, 0, 13)])),
            t,
        )
        .pinched(),
    );
    v.push(recipe(
        "disjoint-union",
        "new (72 §8.1)",
        cube.clone(),
        vec![step(
            Op::Union,
            block_at([20.0, 0.0, 0.0], [10.0, 10.0, 10.0]),
        )],
        built(2, 2, 0, cells(16, 24, true, &[(Kind::Plane, 1, 0, 12)])),
        truth(2.0 * cube_v, 2.0 * cube_a),
    ));
    // A cavity: one solid region with two shells. The inner shell's 6 faces are the 4^3
    // block's, facing inward.
    v.push(recipe(
        "box-cavity",
        "new (72 §8.1)",
        cube.clone(),
        vec![step(
            Op::Difference,
            block_at([3.0, 3.0, 3.0], [4.0, 4.0, 4.0]),
        )],
        built(1, 2, 0, cells(16, 24, true, &[(Kind::Plane, 1, 0, 12)])),
        truth(cube_v - 64.0, cube_a + 96.0),
    ));
    // A body cut by an exact copy of itself leaves nothing (`a_part_with_step_tolerances_
    // minus_itself_is_empty`). How an empty result is represented is BO4's; this recipe says
    // there is no region, shell or patch.
    let empty = || built(0, 0, 0, cells(0, 0, true, &[]));
    v.push(recipe(
        "block-minus-itself",
        "boolean_api::a_part_with_step_tolerances_minus_itself_is_empty",
        cube.clone(),
        vec![step(Op::Difference, cube.clone())],
        empty(),
        truth(0.0, 0.0),
    ));
    let post_10 = cylinder_at([0.0; 3], 2.0, 10.0);
    v.push(recipe(
        "cylinder-minus-itself",
        "boolean_api::a_part_with_step_tolerances_minus_itself_is_empty",
        post_10.clone(),
        vec![step(Op::Difference, post_10)],
        empty(),
        truth(0.0, 0.0),
    ));

    // --- the benchmark bodies -----------------------------------------------------------------
    v.push(plate("plate-4", 4));
    v.push(plate("plate-12", 12));
    v.push(gear_60());
    v.push(grids("interleaved-grids-4", 4, 146, 219));
    v.push(grids("interleaved-grids-8", 8, 546, 819));
    v
}
