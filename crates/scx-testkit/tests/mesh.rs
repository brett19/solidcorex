// Written for solidcorex. Apache-2.0.
//! The mesh oracle passes sound meshes and names each deliberate break.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use scx_testkit::mesh::{Limits, Mesh, Violation, chord_bound, violations, volume_gap};

fn tetra() -> Mesh {
    // Right-hand windings outward; volume 1/6.
    Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        triangles: vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]],
    }
}

fn cube() -> Mesh {
    let vertices = (0..8)
        .map(|i| [(i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64])
        .collect();
    let q = |a: u32, b: u32, c: u32, d: u32| [[a, b, c], [a, c, d]];
    let faces = [
        q(0, 2, 3, 1), // z = 0, normal -z
        q(4, 5, 7, 6), // z = 1
        q(0, 1, 5, 4), // y = 0
        q(2, 6, 7, 3), // y = 1
        q(0, 4, 6, 2), // x = 0
        q(1, 3, 7, 5), // x = 1
    ];
    Mesh {
        vertices,
        triangles: faces.into_iter().flatten().collect(),
    }
}

/// An octahedron with its vertices on the sphere of radius `r`, subdivided `n` times with
/// midpoints pushed onto the sphere. Returns the mesh and the largest sagitta of any edge.
fn sphere(r: f64, n: usize) -> (Mesh, f64) {
    let mut vertices = vec![
        [r, 0., 0.],
        [-r, 0., 0.],
        [0., r, 0.],
        [0., -r, 0.],
        [0., 0., r],
        [0., 0., -r],
    ];
    let mut tris: Vec<[u32; 3]> = vec![
        [0, 2, 4],
        [2, 1, 4],
        [1, 3, 4],
        [3, 0, 4],
        [2, 0, 5],
        [1, 2, 5],
        [3, 1, 5],
        [0, 3, 5],
    ];
    let mut sag: f64 = r;
    for _ in 0..n {
        let mut mid = std::collections::BTreeMap::new();
        let mut next = Vec::new();
        sag = 0.0;
        let mut m = |a: u32, b: u32, vs: &mut Vec<[f64; 3]>| -> u32 {
            let key = (a.min(b), a.max(b));
            *mid.entry(key).or_insert_with(|| {
                let (p, q) = (vs[a as usize], vs[b as usize]);
                let c = [(p[0] + q[0]) / 2., (p[1] + q[1]) / 2., (p[2] + q[2]) / 2.];
                let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
                sag = sag.max(r - l);
                vs.push([c[0] * r / l, c[1] * r / l, c[2] * r / l]);
                (vs.len() - 1) as u32
            })
        };
        for t in &tris {
            let (a, b, c) = (t[0], t[1], t[2]);
            let (ab, bc, ca) = (
                m(a, b, &mut vertices),
                m(b, c, &mut vertices),
                m(c, a, &mut vertices),
            );
            next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
        }
        tris = next;
    }
    (
        Mesh {
            vertices,
            triangles: tris,
        },
        sag,
    )
}

fn has(v: &[Violation], f: impl Fn(&Violation) -> bool) -> bool {
    v.iter().any(f)
}

#[test]
fn sound_meshes_pass() {
    for m in [tetra(), cube(), sphere(2.0, 3).0] {
        assert_eq!(violations(&m, Limits::default()), vec![]);
    }
}

#[test]
fn volumes_of_hand_built_meshes() {
    assert!((tetra().signed_volume() - 1.0 / 6.0).abs() < 1e-15);
    assert!((cube().signed_volume() - 1.0).abs() < 1e-15);
    assert!((cube().area() - 6.0).abs() < 1e-14);
    // Far from the origin the shifted sum keeps its accuracy.
    let mut c = cube();
    for v in &mut c.vertices {
        v[0] += 1e7;
    }
    assert!((c.signed_volume() - 1.0).abs() < 1e-8);
}

#[test]
fn a_missing_triangle_leaves_a_boundary() {
    let mut m = cube();
    m.triangles.pop();
    let v = violations(&m, Limits::default());
    assert!(has(&v, |x| matches!(x, Violation::Boundary { .. })));
}

#[test]
fn a_flipped_triangle_breaks_the_orientation() {
    let mut m = cube();
    m.triangles[3].swap(1, 2);
    let v = violations(&m, Limits::default());
    // Its three edges now agree with their neighbours' direction.
    let n = v
        .iter()
        .filter(|x| matches!(x, Violation::Orientation { .. }))
        .count();
    assert_eq!(n, 3, "{v:?}");
}

#[test]
fn an_inward_mesh_is_consistent_but_its_volume_is_negative() {
    let mut m = cube();
    for t in &mut m.triangles {
        t.swap(1, 2);
    }
    assert_eq!(violations(&m, Limits::default()), vec![]);
    let g = volume_gap(&m, 1.0, 1e-9);
    assert!(!g.holds());
    assert!((g.mesh + 1.0).abs() < 1e-15);
}

#[test]
fn a_third_triangle_on_an_edge_is_non_manifold() {
    let mut m = tetra();
    m.vertices.push([1., 1., 1.]);
    m.triangles.push([0, 1, 4]);
    let v = violations(&m, Limits::default());
    assert!(has(&v, |x| matches!(
        x,
        Violation::NonManifoldEdge {
            edge: [0, 1],
            uses: 3
        }
    )));
}

#[test]
fn two_tetrahedra_sharing_a_vertex_pinch_it() {
    let a = tetra();
    let mut m = a.clone();
    let base = m.vertices.len() as u32;
    // The second copy, moved away, with its vertex 0 merged into the first copy's vertex 0.
    for v in &a.vertices[1..] {
        m.vertices.push([v[0] - 1.0, v[1] - 1.0, v[2] - 1.0]);
    }
    let map = |i: u32| if i == 0 { 0 } else { base + i - 1 };
    for t in &a.triangles {
        m.triangles.push([map(t[0]), map(t[1]), map(t[2])]);
    }
    let v = violations(&m, Limits::default());
    assert_eq!(v, vec![Violation::NonManifoldVertex { vertex: 0 }]);
}

#[test]
fn degenerate_triangles_are_found() {
    // A repeated vertex.
    let mut m = cube();
    m.triangles.push([0, 0, 1]);
    assert!(has(&violations(&m, Limits::default()), |x| matches!(
        x,
        Violation::Degenerate { triangle: 12 }
    )));
    // A zero-area triangle: a vertex on the middle of an edge, three collinear points.
    let m = Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [2., 0., 0.]],
        triangles: vec![[0, 1, 2]],
    };
    assert!(has(&violations(&m, Limits::default()), |x| matches!(
        x,
        Violation::Degenerate { triangle: 0 }
    )));
    // A sliver, flagged by the ratio and passed with ratio 0.
    let m = Mesh {
        vertices: vec![[0., 0., 0.], [1., 0., 0.], [0.5, 1e-12, 0.]],
        triangles: vec![[0, 1, 2]],
    };
    assert!(has(&violations(&m, Limits::default()), |x| matches!(
        x,
        Violation::Degenerate { .. }
    )));
    let lim = Limits {
        min_area_ratio: 0.0,
        ..Limits::default()
    };
    assert!(!has(&violations(&m, lim), |x| matches!(
        x,
        Violation::Degenerate { .. }
    )));
}

#[test]
fn a_folded_pair_is_found() {
    // Push the cube's top-front-right vertex through the cube so two triangles on a shared
    // edge turn back on each other: vertex 7 moved to the near side of its neighbours.
    let mut m = cube();
    m.vertices[7] = [0.5, 0.5, -3.0];
    let v = violations(&m, Limits::default());
    assert!(has(&v, |x| matches!(x, Violation::Fold { .. })), "{v:?}");
    // A fold-free mesh is not flagged under the loosest limit.
    let lim = Limits {
        fold_cos: -1.0,
        ..Limits::default()
    };
    assert!(!has(&violations(&m, lim), |x| matches!(
        x,
        Violation::Fold { .. }
    )));
}

#[test]
fn bad_data_is_named() {
    let mut m = cube();
    m.triangles[0][0] = 99;
    assert_eq!(
        violations(&m, Limits::default()),
        vec![Violation::IndexOutOfRange { triangle: 0 }]
    );
    let mut m = cube();
    m.vertices[2][1] = f64::NAN;
    assert_eq!(
        violations(&m, Limits::default()),
        vec![Violation::NonFinite { vertex: 2 }]
    );
}

#[test]
fn a_sphere_mesh_volume_gap_is_within_chord_times_area() {
    let r = 2.0;
    let sphere_area = 4.0 * std::f64::consts::PI * r * r;
    let sphere_vol = 4.0 / 3.0 * std::f64::consts::PI * r * r * r;
    let mut last = f64::INFINITY;
    for n in 1..=4 {
        let (m, sag) = sphere(r, n);
        let g = volume_gap(&m, sphere_vol, chord_bound(sag, sphere_area));
        assert!(g.holds(), "n = {n}: gap {} bound {}", g.gap, g.bound);
        assert!(g.gap < 0.0, "an inscribed mesh is smaller");
        assert!(g.gap.abs() < last, "the gap shrinks as the mesh refines");
        last = g.gap.abs();
    }
    // A bound tighter than the truth fails: the check can fail.
    let (m, sag) = sphere(r, 2);
    assert!(!volume_gap(&m, sphere_vol, 0.01 * chord_bound(sag, sphere_area)).holds());
}
