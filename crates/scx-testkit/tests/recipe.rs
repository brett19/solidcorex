// Written for solidcorex. Apache-2.0.
//! The step-4 recipes (research 72 T12): each recipe's own consistency. No operation exists
//! yet; these tests check the data against independent routes:
//!
//! - the hand-stated topology satisfies Euler-Poincare, and a prism's counts follow from its
//!   plan polygon;
//! - the closed forms agree with textbook formulas written out again here, with the SMLib
//!   volumes recorded in the pin's `boolean_api` table, and (as a Monte Carlo at 5 sigma)
//!   with T10's membership oracle on the whole Boolean tree;
//! - the two-block recipes' volumes and areas agree with a polygon-clipping route;
//! - the contact points lie on both operands' boundaries;
//! - the lineage lists are whole (every operand face once; areas sum to the area).

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test helpers

use std::collections::BTreeSet;
use std::f64::consts::PI;

use scx_num::fmath;
use scx_num::hash::Rng;
use scx_testkit::analytic::Frame;
use scx_testkit::csg::{self, Side};
use scx_testkit::profile::{Seg, fixtures};
use scx_testkit::recipe::convex::{Hull, overlap};
use scx_testkit::recipe::{
    BoxFace, Dimension, Invariant, Kind, Outcome, Recipe, Refusal, Shape, Truth, Unsupported, all,
    frame_at, frame_turned,
};
use scx_testkit::sample;

const RES: [f64; 2] = [1e-5, 1e-3];

fn by_name(rs: &[Recipe], name: &str) -> Recipe {
    rs.iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no recipe {name}"))
        .clone()
}

fn close(a: f64, b: f64, rel: f64, what: &str) {
    let scale = a.abs().max(b.abs()).max(1e-300);
    assert!((a - b).abs() <= rel * scale, "{what}: {a} vs {b}");
}

// ---------------------------------------------------------------------------------------
// The list

#[test]
fn the_list_is_unique_stable_and_complete() {
    for res in RES {
        let rs = all(res);
        let names: BTreeSet<_> = rs.iter().map(|r| r.name).collect();
        assert_eq!(names.len(), rs.len(), "names are unique");
        // (Debug, not `==`: a NaN size is not equal to itself.)
        assert_eq!(
            format!("{rs:?}"),
            format!("{:?}", all(res)),
            "the list is a pure function of the resolution"
        );
        assert!(rs.len() >= 60, "{} recipes", rs.len());
    }
    // 72 §5.7: the `boolean_api` cases. 17 are named in the table, not the 15 §5.2 counts.
    let rs = all(1e-5);
    let api: Vec<_> = rs
        .iter()
        .filter(|r| {
            r.source.starts_with("boolean_api::cases")
                || r.source.starts_with("boolean_api::point_contacts")
        })
        .collect();
    assert_eq!(api.len(), 17);
    let built = api
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Built(_)))
        .count();
    let manifold = api
        .iter()
        .filter(|r| {
            matches!(
                &r.outcome,
                Outcome::Unsupported(Unsupported::NonManifold { .. })
            )
        })
        .count();
    let ssi = api
        .iter()
        .filter(|r| {
            matches!(
                &r.outcome,
                Outcome::Unsupported(Unsupported::SsiPair { .. })
            )
        })
        .count();
    assert_eq!((built, manifold, ssi), (5, 6, 6));
    // BO4's five and BO5's six are the ones the plan names.
    for n in [
        "box-cyl-hole",
        "box-box-union",
        "sphere-and-box",
        "coplanar-union",
        "torus-minus-box",
    ] {
        assert!(matches!(by_name(&rs, n).outcome, Outcome::Built(_)), "{n}");
    }
    for n in [
        "tpipe",
        "seam-cylinder-sphere",
        "off-seam-cylinder-sphere",
        "seam-torus-sphere",
        "seam-torus-box",
        "seam-vertex-torus-box",
    ] {
        assert!(
            matches!(
                by_name(&rs, n).outcome,
                Outcome::Unsupported(Unsupported::SsiPair { .. })
            ),
            "{n}"
        );
    }
}

#[test]
fn only_the_recipes_near_the_resolution_depend_on_it() {
    let (a, b) = (all(RES[0]), all(RES[1]));
    assert_eq!(a.len(), b.len());
    let mut moved: Vec<_> = a
        .iter()
        .zip(&b)
        .filter(|(x, y)| format!("{x:?}") != format!("{y:?}"))
        .map(|(x, _)| x.name)
        .collect();
    moved.sort_unstable();
    let mut want = vec![
        "block-size-twice-resolution",
        "block-small-at-1e4",
        "block-x-at-resolution",
        "block-x-below-resolution",
        "coplanar-offset-0",
        "coplanar-offset-0.1c",
        "coplanar-offset-0.5c",
        "coplanar-offset-10c",
        "coplanar-offset-4c",
        "crease-0.3c",
        "crease-10c",
        "crease-3c",
    ];
    want.sort_unstable();
    assert_eq!(moved, want);
}

#[test]
fn invariants_follow_the_outcome() {
    for r in all(1e-5) {
        let has = |i: Invariant| r.invariants.contains(&i);
        assert!(has(Invariant::Deterministic), "{r}");
        match &r.outcome {
            Outcome::Built(_) | Outcome::Open(_) => {
                for i in [
                    Invariant::Valid,
                    Invariant::Volume,
                    Invariant::Topology,
                    Invariant::Classification,
                ] {
                    assert!(has(i), "{r}: {i:?}");
                }
                assert_eq!(
                    has(Invariant::Area),
                    r.truth.is_some_and(|t| t.area.is_some()),
                    "{r}"
                );
                assert_eq!(has(Invariant::Lineage), r.lineage.is_some(), "{r}");
            }
            Outcome::Refused(_) | Outcome::Unsupported(_) => {
                assert_eq!(r.invariants, vec![Invariant::Deterministic], "{r}");
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Topology

#[test]
fn every_stated_topology_satisfies_euler_poincare() {
    let mut checked = 0;
    for res in RES {
        for r in all(res) {
            let Outcome::Built(t) = &r.outcome else {
                continue;
            };
            let Some(c) = &t.cells else { continue };
            assert_eq!(
                t.euler_defect(),
                Some(0),
                "{r}: V {} E {}",
                c.vertices,
                c.edges
            );
            assert_eq!(
                t.patch_count(),
                Some(c.patches.iter().map(|p| p.count).sum())
            );
            if c.cubic {
                assert_eq!(3 * c.vertices, 2 * c.edges, "{r}: a vertex has three edges");
            }
            assert!(t.shells >= t.solid_regions, "{r}: a region has a shell");
            // The lineage, where stated, names every patch.
            if let Some(l) = &r.lineage {
                assert_eq!(Some(l.faces.len() as u32), t.patch_count(), "{r}");
            }
            checked += 1;
        }
    }
    assert!(checked >= 80, "{checked}");
}

#[test]
fn a_prisms_cells_follow_from_its_plan() {
    // A prism over a simple n-gon has 2 n vertices, 3 n edges and n + 2 faces; a body that
    // is a prism of height 10 over the union of its operands' plans is one.
    let rs = all(1e-5);
    for (name, n) in [
        ("box-box-union-flush", 8),
        ("coplanar-union", 6),
        ("coplanar-offset-0", 4),
        ("coplanar-offset-0.5c", 4),
        ("coplanar-offset-4c", 8),
        ("coplanar-offset-10c", 8),
    ] {
        let r = by_name(&rs, name);
        let Outcome::Built(t) = &r.outcome else {
            panic!("{name}")
        };
        let c = t.cells.as_ref().expect("cells");
        assert_eq!(
            (c.vertices, c.edges, t.patch_count()),
            (2 * n, 3 * n, Some(n + 2)),
            "{name}"
        );
    }
    // The constructors: a block's, an extruded polygon's.
    for name in ["block-3x4x5", "block-far-turned", "extrude-square"] {
        let Outcome::Built(t) = by_name(&rs, name).outcome else {
            panic!("{name}")
        };
        assert_eq!(t.cells.map(|c| (c.vertices, c.edges)), Some((8, 12)));
    }
}

#[test]
fn a_mesh_of_the_stated_counts_is_a_valid_surface_when_the_body_is_a_polyhedron() {
    // Every all-planar recipe's cells must also satisfy the faces-only relation of a
    // polyhedron: sum over patches of the loops L >= F and E >= 3F/... is implied by cubic.
    // Here: for a cubic, all-planar body, F = 2 + E/3 + (L - F)... follows from Euler; check
    // the genus the other way, from the counts alone.
    for r in all(1e-5) {
        let Outcome::Built(t) = &r.outcome else {
            continue;
        };
        let Some(c) = &t.cells else { continue };
        if !c.cubic || c.patches.iter().any(|p| p.kind != Kind::Plane) {
            continue;
        }
        let f: i64 = c.patches.iter().map(|p| i64::from(p.count)).sum();
        let l: i64 = c
            .patches
            .iter()
            .map(|p| i64::from(p.count) * i64::from(p.loops))
            .sum();
        // V = 2 E / 3 and V - E + 2 F - L = 2 (S - G): E = 3 (2 F - L - 2 (S - G)).
        let e = 3 * (2 * f - l - 2 * (i64::from(t.shells) - i64::from(t.genus)));
        assert_eq!(e, i64::from(c.edges), "{r}");
    }
}

// ---------------------------------------------------------------------------------------
// Closed forms

#[test]
fn constructors_agree_with_textbook_formulas() {
    let rs = all(1e-5);
    let sq5 = 20.0_f64.sqrt();
    let want: Vec<(&str, f64, f64)> = vec![
        ("block-3x4x5", 60.0, 94.0),
        ("block-far-turned", 6000.0, 2200.0),
        ("cylinder-r2-h10", 40.0 * PI, 48.0 * PI),
        (
            "cylinder-turned",
            PI * 2.25 * 7.0,
            2.0 * PI * 2.25 + 2.0 * PI * 1.5 * 7.0,
        ),
        // frustum: V = pi h (R^2 + R r + r^2) / 3; A = pi (R^2 + r^2) + pi (R + r) slant
        (
            "cone-frustum-3-1",
            PI * 4.0 * 13.0 / 3.0,
            PI * 10.0 + PI * 4.0 * sq5,
        ),
        (
            "cone-frustum-1-3",
            PI * 4.0 * 13.0 / 3.0,
            PI * 10.0 + PI * 4.0 * sq5,
        ),
        (
            "cone-apex-up",
            PI * 9.0 * 4.0 / 3.0,
            PI * 9.0 + PI * 3.0 * 5.0,
        ),
        (
            "cone-apex-down",
            PI * 9.0 * 4.0 / 3.0,
            PI * 9.0 + PI * 3.0 * 5.0,
        ),
        ("sphere-r5", 500.0 * PI / 3.0, 100.0 * PI),
        (
            "torus-5-1.5",
            2.0 * PI * PI * 5.0 * 2.25,
            4.0 * PI * PI * 5.0 * 1.5,
        ),
        ("extrude-square", 48.0, 2.0 * 16.0 + 16.0 * 3.0),
        ("extrude-d", 25.0 * PI, 25.0 * PI + 2.0 * (10.0 + 5.0 * PI)),
        (
            "extrude-square-with-square-hole",
            91.0 * 4.0,
            2.0 * 91.0 + (40.0 + 12.0) * 4.0,
        ),
    ];
    for (name, v, a) in want {
        let t = by_name(&rs, name).truth.expect(name);
        close(t.volume, v, 1e-13, name);
        close(t.area.expect(name), a, 1e-13, name);
    }
    // The rounded top: a 6 x 4 rectangle and the circular segment of chord 6 at radius 5.
    let t = by_name(&rs, "extrude-rounded-top").truth.expect("rounded");
    let ang = 2.0 * fmath::atan2(3.0, 4.0);
    let area = 24.0 + 12.5 * (ang - 24.0 / 25.0);
    close(t.volume, 5.0 * area, 1e-13, "rounded top V");
    let perimeter = 6.0 + 8.0 + 5.0 * ang;
    close(
        t.area.expect("area"),
        2.0 * area + 5.0 * perimeter,
        1e-13,
        "rounded top A",
    );
}

#[test]
fn booleans_agree_with_textbook_formulas() {
    let rs = all(1e-5);
    let sphere = |r: f64| (4.0 * PI * r * r * r / 3.0, 4.0 * PI * r * r);
    let blk = |a: f64, b: f64, c: f64| (a * b * c, 2.0 * (a * b + b * c + a * c));
    let torus = (2.0 * PI * PI * 5.0 * 2.25, 4.0 * PI * PI * 5.0 * 1.5);
    let sum = |x: (f64, f64), y: (f64, f64)| (x.0 + y.0, x.1 + y.1);
    let want: Vec<(&str, (f64, f64))> = vec![
        ("box-cyl-hole", (1000.0 - 40.0 * PI, 600.0 + 32.0 * PI)),
        ("box-box-union", (1875.0, 1050.0)),
        ("box-box-difference", (875.0, 600.0)),
        ("box-box-intersection", (125.0, 150.0)),
        ("box-box-union-flush", (1750.0, 950.0)),
        // r = 5 below z = 2: the sphere less a cap of height 3 (`pi h^2 (3 r - h) / 3`), the
        // zone of 7 and the disc of radius sqrt 21.
        (
            "sphere-and-box",
            (500.0 * PI / 3.0 - 36.0 * PI, 70.0 * PI + 21.0 * PI),
        ),
        ("coplanar-union", (1500.0, 900.0)),
        (
            "tangent-sphere-box",
            sum(blk(10.0, 10.0, 10.0), sphere(3.0)),
        ),
        ("seam-sphere-box", sum(sphere(7.5), blk(5.0, 17.5, 15.0))),
        ("seam-box-sphere", sum(sphere(7.5), blk(5.0, 17.5, 15.0))),
        (
            "off-seam-sphere-box",
            sum(sphere(7.5), blk(5.0, 17.5, 15.0)),
        ),
        ("seam-sphere-sphere", sum(sphere(3.0), sphere(2.0))),
        (
            "seam-box-minus-sphere",
            (1000.0 - 500.0 * PI / 3.0, 600.0 + 100.0 * PI),
        ),
        ("seam-torus-sphere", sum(torus, sphere(2.0))),
        ("seam-torus-box", sum(torus, blk(6.0, 4.0, 6.0))),
        ("seam-vertex-torus-box", sum(torus, blk(4.0, 6.0, 6.0))),
        (
            "seam-cylinder-sphere",
            sum(
                (PI * 4.0 * 10.0, 2.0 * PI * 2.0 * 10.0 + 2.0 * PI * 4.0),
                sphere(3.0),
            ),
        ),
        (
            "off-seam-cylinder-sphere",
            sum(
                (PI * 4.0 * 10.0, 2.0 * PI * 2.0 * 10.0 + 2.0 * PI * 4.0),
                sphere(3.0),
            ),
        ),
        // half a torus: half the volume and area, and two discs of the tube's section
        (
            "torus-minus-box",
            (torus.0 / 2.0, torus.1 / 2.0 + 2.0 * PI * 2.25),
        ),
        ("corner-contact-union", (2000.0, 1200.0)),
        ("disjoint-union", (2000.0, 1200.0)),
        ("box-cavity", (936.0, 696.0)),
        ("block-minus-itself", (0.0, 0.0)),
        ("cylinder-minus-itself", (0.0, 0.0)),
        (
            "edge-contact-union",
            (1500.0, 600.0 + 100.0 + 200.0 * 2.0_f64.sqrt()),
        ),
    ];
    for (name, (v, a)) in want {
        let t = by_name(&rs, name).truth.expect(name);
        close(t.volume, v, 1e-13, name);
        if v == 0.0 {
            assert_eq!(t.volume, 0.0);
        }
        close(t.area.expect(name), a, 1e-13, name);
    }
    // plate_12 is 12 x 12 holes: 100000 - 144 * 40 pi (68's measured 81904.95 against
    // 81904.42), and its area is 2 (1e4 - 144 pi 4) + 4000 + 144 * 2 pi 2 * 10.
    let t = by_name(&rs, "plate-12").truth.expect("plate");
    close(t.volume, 100_000.0 - 5760.0 * PI, 1e-13, "plate_12 V");
    close(
        t.area.expect("a"),
        2.0 * (1e4 - 576.0 * PI) + 4000.0 + 5760.0 * PI,
        1e-13,
        "plate_12 A",
    );
    let t = by_name(&rs, "plate-4").truth.expect("plate");
    close(t.volume, 100_000.0 - 640.0 * PI, 1e-13, "plate_4 V");
    // The grids (research 72 §5.7, the pin's closed form): n * 2n * 3 - n^2, and the area
    // from the height steps of the unit cells.
    for (name, n) in [("interleaved-grids-4", 4.0), ("interleaved-grids-8", 8.0)] {
        let t = by_name(&rs, name).truth.expect(name);
        close(t.volume, n * 2.0 * n * 3.0 - n * n, 1e-13, name);
        close(t.area.expect(name), 14.0 * n * n + 6.0 * n, 1e-13, name);
    }
    // gear_60: the formula of research 72 §2.4, written out again.
    let gap = 2.0 * PI * 45.0 / 60.0 * 0.45;
    let half = gap / 2.0;
    let slot =
        half * (2500.0 - half * half).sqrt() + 2500.0 * fmath::asin(half / 50.0) - 45.0 * gap;
    let t = by_name(&rs, "gear-60").truth.expect("gear");
    close(
        t.volume,
        10.0 * (PI * 2500.0 - 60.0 * slot),
        1e-13,
        "gear V",
    );
}

#[test]
fn the_pins_recorded_volumes_agree_with_the_closed_forms() {
    // SMLib's volumes for the `boolean_api` cases (the pin's table), and the pin's own
    // numerical integral for the T-pipe: each within the pin's 1e-6 relative.
    let rs = all(1e-5);
    for (name, smlib) in [
        ("box-cyl-hole", 874.336289861),
        ("box-box-union", 1875.0),
        ("sphere-and-box", 410.501465112),
        ("tpipe", 308.956697417),
        ("coplanar-union", 1500.0),
        ("tangent-sphere-box", 1113.09733917),
        ("torus-minus-box", 111.033062089),
    ] {
        let t = by_name(&rs, name).truth.expect(name);
        assert!(
            (t.volume - smlib).abs() <= 1e-6 * smlib,
            "{name}: {} vs SMLib {smlib}",
            t.volume
        );
    }
    // The pin's numerical integral of the same T-pipe; the quadrature here is exact to
    // rounding, so the difference is the pin's.
    let t = by_name(&rs, "tpipe").truth.expect("tpipe");
    assert!((t.volume - 308.956664169885).abs() < 1e-9, "{}", t.volume);
}

// ---------------------------------------------------------------------------------------
// The membership oracle

fn monte_carlo_volume(r: &Recipe, seed: u64) -> (f64, f64) {
    let tree = r.csg().expect("csg");
    let (lo, hi) = tree.bounds().expect("bounded");
    let leaves = tree.leaves().len().max(1);
    let n = (200_000 / leaves).clamp(20_000, 100_000);
    let pts = csg::box_points(lo, hi, 0.0, n, seed);
    let c = tree.census(&pts, 1e-7);
    let vbox: f64 = (0..3).map(|i| hi[i] - lo[i]).product();
    let nf = n as f64;
    let p = (c.inside as f64 + 0.5 * c.band as f64) / nf;
    // (a floor of one point in n at either end, for a share of 0 or 1)
    let sigma = vbox * ((p * (1.0 - p)).max(1.0 / nf) / nf).sqrt();
    // Allowance: 5 sigma, the band's share of the box, and the box's room (1e-12 of the
    // largest coordinate, so far below 1e-9 of the box).
    (
        vbox * p,
        5.0 * sigma + vbox * (c.band as f64) / nf + 1e-9 * vbox,
    )
}

#[test]
fn every_recipes_volume_agrees_with_the_membership_oracle() {
    let rs = all(1e-5);
    let mut checked = 0;
    for (k, r) in rs.iter().enumerate() {
        let Some(t) = r.truth else { continue };
        let (est, allow) = monte_carlo_volume(r, 0x7112 ^ (k as u64) << 8);
        assert!(
            (est - t.volume).abs() <= allow + t.slack,
            "{r}: oracle {est} +- {allow}, closed form {}",
            t.volume
        );
        checked += 1;
    }
    assert!(checked >= 55, "{checked}");
}

#[test]
fn contact_points_lie_on_both_operands_boundaries() {
    let rs = all(1e-5);
    let mut points = 0;
    for r in &rs {
        let Outcome::Unsupported(Unsupported::NonManifold { at }) = &r.outcome else {
            continue;
        };
        let a = r.base.csg().expect("a");
        for s in &r.steps {
            let b = s.tool.csg().expect("b");
            for &p in at {
                // On the boundary: the enclosure over a tiny box holds zero.
                assert_eq!(a.side(p, 1e-9), Side::Band, "{r}: {p:?} not on a");
                assert_eq!(b.side(p, 1e-9), Side::Band, "{r}: {p:?} not on b");
                points += 1;
            }
        }
    }
    assert_eq!(points, 5 + 6);
}

#[test]
fn ssi_causes_name_the_surfaces_the_parts_have() {
    for r in all(1e-5) {
        let Outcome::Unsupported(Unsupported::SsiPair { kinds: [x, y] }) = &r.outcome else {
            continue;
        };
        let (a, b) = (r.base.shape.kinds(), r.steps[0].tool.shape.kinds());
        assert!(
            (a.contains(x) && b.contains(y)) || (a.contains(y) && b.contains(x)),
            "{r}: {x:?} {y:?} vs {a:?} {b:?}"
        );
        assert!(x <= y, "{r}: kinds in Kind order");
        // No plane pair is ever unsupported; the pair has a curved kind.
        assert_ne!(*y, Kind::Plane);
    }
}

// ---------------------------------------------------------------------------------------
// The two-block route

fn aligned_pair(rng: &mut Rng) -> ([f64; 3], [f64; 3], [f64; 3], [f64; 3]) {
    let mut v = || 1.0 + 9.0 * rng.next_f64();
    let (sa, sb) = ([v(), v(), v()], [v(), v(), v()]);
    let mut shift = || 6.0 * rng.next_signed_unit();
    let (oa, ob) = ([shift(), shift(), shift()], [shift(), shift(), shift()]);
    (oa, sa, ob, sb)
}

#[test]
fn clipping_gives_the_overlap_of_aligned_blocks_in_closed_form() {
    let mut rng = Rng::new(0xC11B);
    let mut hits = 0;
    for _ in 0..400 {
        let (oa, sa, ob, sb) = aligned_pair(&mut rng);
        let (a, b) = (
            Hull::block(&frame_at(oa), sa),
            Hull::block(&frame_at(ob), sb),
        );
        let Some(o) = overlap(&a, &b) else { continue };
        let d: Vec<f64> = (0..3)
            .map(|i| ((oa[i] + sa[i]).min(ob[i] + sb[i]) - oa[i].max(ob[i])).max(0.0))
            .collect();
        let vol = d[0] * d[1] * d[2];
        close(o.volume, vol, 1e-12, "intersection volume");
        if vol > 0.0 {
            hits += 1;
            let area = 2.0 * (d[0] * d[1] + d[1] * d[2] + d[0] * d[2]);
            close(o.a_in_b + o.b_in_a, area, 1e-12, "intersection area");
            let (uv, _) = o.union(&a, &b);
            close(uv, a.volume() + b.volume() - vol, 1e-12, "union volume");
        }
    }
    assert!(hits > 100, "{hits}");
}

#[test]
fn clipping_agrees_with_the_oracle_for_turned_blocks() {
    let mut rng = Rng::new(0xC11C);
    for k in 0..6 {
        let fa = sample::frame(&mut rng, 2.0);
        let fb = sample::frame(&mut rng, 2.0);
        let (sa, sb) = ([4.0, 5.0, 3.0], [3.5, 4.0, 6.0]);
        let (a, b) = (Hull::block(&fa, sa), Hull::block(&fb, sb));
        let o = overlap(&a, &b).expect("general position");
        let (uv, _) = o.union(&a, &b);
        let leaf = |f: &Frame, s: [f64; 3]| {
            csg::Csg::leaf(f, csg::Primitive::Block { size: s }).expect("leaf")
        };
        let tree = leaf(&fa, sa).union(leaf(&fb, sb));
        let (lo, hi) = tree.bounds().expect("bounds");
        let n = 200_000;
        let c = tree.census(&csg::box_points(lo, hi, 0.0, n, 40 + k), 1e-7);
        let vbox: f64 = (0..3).map(|i| hi[i] - lo[i]).product();
        let p = (c.inside as f64 + 0.5 * c.band as f64) / n as f64;
        let sigma = vbox * (p * (1.0 - p) / n as f64).sqrt();
        assert!(
            (vbox * p - uv).abs() < 5.0 * sigma + 1e-9,
            "{k}: {} vs {uv}",
            vbox * p
        );
    }
}

#[test]
fn the_crease_blocks_barely_overlap_the_cube() {
    // B's crease runs `d` into the cube's edge: the overlap is a sliver of volume about
    // `d^2` times the edge length, 1e-15 at 3 resolutions, far below the volume's rounding.
    // So the union is 1000 + 2000 and its area 600 + 1000, to rounding; the recipe's value
    // is the invariants (valid, no dangling vertex references), not the volume.
    let rs = all(1e-5);
    for n in ["crease-0.3c", "crease-3c", "crease-10c"] {
        let t = by_name(&rs, n).truth.expect(n);
        assert!((t.volume - 3000.0).abs() < 1e-9, "{n}: {}", t.volume);
        assert!(
            (t.area.expect(n) - 1600.0).abs() < 1e-7,
            "{n}: {:?}",
            t.area
        );
    }
}

// ---------------------------------------------------------------------------------------
// Lineage

#[test]
fn lineage_names_every_operand_face_once_and_sums_to_the_area() {
    let rs = all(1e-5);
    let mut seen = 0;
    for r in rs.iter().filter(|r| r.lineage.is_some()) {
        let l = r.lineage.as_ref().expect("lineage");
        let area: f64 = l.faces.iter().map(|f| f.area).sum();
        close(
            area,
            r.truth.expect("truth").area.expect("area"),
            1e-13,
            r.name,
        );
        let mut all_origins: Vec<(u8, BoxFace)> =
            l.faces.iter().flat_map(|f| f.origins.clone()).collect();
        for f in &l.faces {
            let mut s = f.origins.clone();
            s.sort_unstable();
            assert_eq!(s, f.origins, "{r}: origins are sorted");
        }
        let n = all_origins.len();
        all_origins.sort_unstable();
        all_origins.dedup();
        assert_eq!(
            n,
            all_origins.len(),
            "{r}: an operand face is the origin of one result face"
        );
        // Which operand faces survive: the union and the flush union keep all 12; the
        // difference all of a (6) and the three of b inside; the intersection three of each.
        let want = match r.name {
            "box-box-union" | "box-box-union-flush" => 12,
            "box-box-difference" => 9,
            "box-box-intersection" => 6,
            other => panic!("{other}"),
        };
        assert_eq!(n, want, "{r}");
        seen += 1;
    }
    assert_eq!(seen, 4);
}

#[test]
fn lineage_faces_are_the_clipped_areas_of_the_two_blocks() {
    // Each result face of the overlapping cubes has the area of its operand's face less the
    // part inside the other: the origin's own face, clipped (the closed form of the pin's
    // `check_faces`). Whole faces keep 100 and L-shaped ones 75; the pocket's are 25.
    let rs = all(1e-5);
    let l = by_name(&rs, "box-box-union").lineage.expect("lineage");
    let whole = l.faces.iter().filter(|f| f.area == 100.0).count();
    let bent = l.faces.iter().filter(|f| f.area == 75.0).count();
    assert_eq!((whole, bent), (6, 6));
    let cube = Hull::block(&frame_at([0.0; 3]), [10.0; 3]);
    let moved = Hull::block(&frame_at([5.0; 3]), [10.0; 3]);
    let o = overlap(&cube, &moved).expect("general position");
    close(o.a_in_b, 75.0, 1e-13, "a in b");
    close(o.b_in_a, 75.0, 1e-13, "b in a");
}

// ---------------------------------------------------------------------------------------
// Inputs

#[test]
fn refused_inputs_are_invalid_and_the_floor_is_the_resolution() {
    for res in RES {
        for r in all(res) {
            let Outcome::Refused(cause) = &r.outcome else {
                continue;
            };
            assert!(r.truth.is_none(), "{r}");
            assert!(r.steps.is_empty(), "{r}");
            match (cause, &r.base.shape) {
                (Refusal::Size(d), Shape::Block { size }) => {
                    let v = match d {
                        Dimension::X => size[0],
                        Dimension::Y => size[1],
                        _ => size[2],
                    };
                    // not finite, or not above the resolution
                    assert!(!v.is_finite() || v <= res, "{r}: {v} against {res}");
                }
                (Refusal::Size(_) | Refusal::Span | Refusal::NoBody, _) => {
                    assert!(r.base.solid().is_err() && r.base.csg().is_err(), "{r}");
                }
                (Refusal::SelfIntersecting, Shape::Torus { major, minor }) => {
                    assert!(minor > major);
                }
                other => panic!("{r}: {other:?}"),
            }
        }
    }
    // The horn torus is valid input the library builds (MK4b).
    let rs = all(1e-5);
    let horn = by_name(&rs, "torus-horn");
    let Shape::Torus { major, minor } = horn.base.shape else {
        panic!()
    };
    assert_eq!(major, minor);
    assert!(matches!(horn.outcome, Outcome::Built(_)));
}

#[test]
fn built_recipes_have_a_solid_an_oracle_and_a_truth() {
    for res in RES {
        for r in all(res) {
            if matches!(r.outcome, Outcome::Refused(_)) {
                continue;
            }
            for p in r.parts() {
                assert!(p.solid().is_ok(), "{r}");
                assert!(p.csg().is_ok(), "{r}");
            }
            assert!(r.csg().is_ok(), "{r}");
            assert!(r.truth.is_some(), "{r}");
            let t: Truth = r.truth.expect("truth");
            assert!(t.volume.is_finite() && t.volume >= 0.0, "{r}");
            assert!(t.slack >= 0.0);
        }
    }
}

#[test]
fn profiles_are_t11s_fixtures() {
    let fx = fixtures();
    let fixture = |n: &str| {
        fx.iter()
            .find(|f| f.name == n)
            .unwrap_or_else(|| panic!("{n}"))
            .clone()
    };
    let rs = all(1e-5);
    let profile = |name: &str| -> (Vec<Seg>, Vec<Vec<Seg>>) {
        match by_name(&rs, name).base.shape {
            Shape::Extrusion { outer, holes, .. } => (outer, holes),
            _ => panic!(),
        }
    };
    for (recipe, fix) in [
        ("extrude-square", "closed square"),
        ("extrude-d", "closed D of a line and an arc"),
        ("extrude-rounded-top", "closed rounded top"),
        (
            "extrude-square-with-square-hole",
            "square with a square hole",
        ),
    ] {
        let (outer, holes) = profile(recipe);
        let f = fixture(fix);
        // The square of the first recipe is 4 x 4 at the origin, the hole one's is 10.
        assert_eq!(
            (outer.clone(), holes.clone()),
            (f.outer.clone(), f.holes.clone()),
            "{recipe}"
        );
    }
}

#[test]
fn turned_frames_are_orthonormal_and_right_handed() {
    for (axis, angle) in [
        ([1.0, 2.0, 3.0], 0.7),
        ([0.0, 0.0, 1.0], 2.0),
        ([1.0, -1.0, 2.0], -0.4),
    ] {
        let f = frame_turned([1.0, 2.0, 3.0], axis, angle);
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        for (a, b, v) in [
            (f.x, f.x, 1.0),
            (f.y, f.y, 1.0),
            (f.z, f.z, 1.0),
            (f.x, f.y, 0.0),
            (f.y, f.z, 0.0),
            (f.x, f.z, 0.0),
        ] {
            assert!((dot(a, b) - v).abs() < 1e-15);
        }
        let cx = [
            f.x[1] * f.y[2] - f.x[2] * f.y[1],
            f.x[2] * f.y[0] - f.x[0] * f.y[2],
            f.x[0] * f.y[1] - f.x[1] * f.y[0],
        ];
        for i in 0..3 {
            assert!((cx[i] - f.z[i]).abs() < 1e-15);
        }
    }
    // The crease block's and the gear's frames are orthonormal too (the oracle refuses a
    // singular frame, and `Part::csg` passed above); the tpipe branch's axes are a rotation.
    let rs = all(1e-5);
    let b = by_name(&rs, "tpipe").steps[0].tool.frame;
    assert_eq!(
        (b.x, b.y, b.z),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
    );
}
