// Written for solidcorex. Apache-2.0.
//! `stitch` with [`Want::Solid`] (74 §4.3 step 5, ST3): solids taken apart into one sheet
//! per face and stitched back, judged against closed-form volumes and the solid they came
//! from; faces flipped; a cavity made a void; permuted input giving the same bits; and the
//! refusals, each naming the sheet entity.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::f64::consts::PI;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, Entity, FaceId, FaceSide,
    LoopRecord, Operand, RegionId, ShellRecord, UseGeom, VertexId, VertexRecord,
};
use scx_curve::{Curve, Line};
use scx_mass::{RelAccuracy, outward, volume};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::frame::Frame;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_stitch::{
    SolidFault, StitchError, StitchRefusal, StitchUnsupported, Stitched, Want, stitch,
};
use scx_surface::{Plane, Surface};
use scx_testkit::bodies::{Kind, generate};

mod common;
use common::*;

#[test]
fn a_stitched_block_is_its_volume() {
    for n in [1, 2, 3] {
        let block = solid(Kind::Block, 0, n);
        let out = run(&refs(&explode(&block, |_| false))).unwrap();
        let b = &out.done.body;
        assert_solid(b, 1.0);
        assert_eq!(regions(b), (1, 0));
        assert_eq!(b.faces().count(), block.faces().count());
        assert!(out.report.left.is_empty());
    }
}

#[test]
fn a_stitched_random_block_is_the_blocks_volume() {
    for seed in 1..6 {
        let block = solid(Kind::Block, seed, 2);
        let out = run(&refs(&explode(&block, |_| false))).unwrap();
        assert_solid(&out.done.body, vol(&block));
    }
}

#[test]
fn a_stitched_cylinder_is_pi_r2_h() {
    // Seed 0: radius 1, height 2.
    for n in [3, 4, 6] {
        let cyl = solid(Kind::Cylinder, 0, n);
        let out = run(&refs(&explode(&cyl, |_| false))).unwrap();
        assert_solid(&out.done.body, 2.0 * PI);
        assert_eq!(regions(&out.done.body), (1, 0));
    }
}

#[test]
fn a_stitched_sphere_is_four_thirds_pi() {
    // Seed 0: radius 1; the poles are collapsed edges of one fin each.
    let sphere = solid(Kind::Sphere, 0, 4);
    let out = run(&refs(&explode(&sphere, |_| false))).unwrap();
    assert_solid(&out.done.body, 4.0 / 3.0 * PI);
}

#[test]
fn flipped_faces_are_oriented_by_parity() {
    let block = solid(Kind::Block, 0, 2);
    // Every other face flipped, then all of them: the material is still inside.
    let ids: Vec<FaceId> = block.faces().map(scx_brep::Face::id).collect();
    for pick in [
        Box::new(|f: FaceId| ids.iter().position(|&x| x == f).unwrap() % 2 == 1)
            as Box<dyn Fn(FaceId) -> bool>,
        Box::new(|_| true),
        Box::new(|f| ids.first() == Some(&f)),
    ] {
        let out = run(&refs(&explode(&block, &pick))).unwrap();
        assert_solid(&out.done.body, 1.0);
    }
}

#[test]
fn an_all_flipped_block_is_flipped_back_by_the_sign() {
    // Every face's front inside: parity alone would put the material outside; the
    // certified volume sign moves it in. The faces keep their (flipped) surfaces.
    let block = solid(Kind::Block, 0, 1);
    let out = run(&refs(&explode(&block, |_| true))).unwrap();
    let b = &out.done.body;
    assert_solid(b, 1.0);
    for f in b.faces() {
        let front = f.shell(FaceSide::Front).region();
        assert!(front.is_solid(), "{}: the material is in front", f.id());
    }
}

/// The pin's `stitch_faces_leaves_a_closed_cavity_void` and
/// `stitched_solid_cavity_is_a_void`: a block with a block-shaped void, its faces stitched
/// back, is one solid with a cavity: 3³ - 1³ (seed 0).
#[test]
fn a_stitched_cavity_is_a_void() {
    let cavity = solid(Kind::Cavity, 0, 1);
    let sheets = explode(&cavity, |_| false);
    let out = run(&refs(&sheets)).unwrap();
    let b = &out.done.body;
    assert_solid(b, 26.0);
    assert_eq!(regions(b), (1, 1));
    // The void's shell's outer side is a shell of the solid.
    let solid_region = b.regions().find(|r| r.is_solid()).unwrap();
    assert_eq!(solid_region.shells().count(), 2);
    // The inner box's faces given first, and flipped: the same answer.
    let mut rev = sheets.clone();
    rev.reverse();
    let out = run(&refs(&rev)).unwrap();
    assert_solid(&out.done.body, 26.0);
    let flipped = explode(&cavity, |_| true);
    let out = run(&refs(&flipped)).unwrap();
    assert_solid(&out.done.body, 26.0);
    assert_eq!(regions(&out.done.body), (1, 1));
}

/// The pin's `stitch_faces_leaves_a_closed_cavity_void` (#21540): a cube of side 10 with a
/// cavity of side 4 as twelve loose faces, the cavity's faces turned either way and given
/// before or after the cube's. Both shells close; the material region holds two shells,
/// the cavity's region is a void of one, and the material's outer shell is across from the
/// outside's. 10³ - 4³ = 936.
#[test]
fn the_pins_cube_with_a_cavity() {
    let outer = explode(&cube(0.0, 10.0), |_| false);
    for (inward, cavity_first) in [(true, false), (false, false), (true, true), (false, true)] {
        let case = format!("inward {inward}, cavity first {cavity_first}");
        let inner = explode(&cube(3.0, 4.0), |_| inward);
        let sheets: Vec<&Brep> = if cavity_first {
            inner.iter().chain(&outer).collect()
        } else {
            outer.iter().chain(&inner).collect()
        };
        let out = run(&sheets).unwrap();
        let b = &out.done.body;
        assert_eq!(out.report.joined.len(), 24, "{case}");
        assert_eq!(b.faces().count(), 12, "{case}");
        assert_solid(b, 936.0);
        let mut rs: Vec<(bool, bool, usize)> = b
            .regions()
            .map(|r| (r.is_outside(), r.is_solid(), r.shells().count()))
            .collect();
        rs.sort_unstable();
        assert_eq!(
            rs,
            vec![(false, false, 1), (false, true, 2), (true, false, 1)],
            "{case}"
        );
        // Every face lies between the material and a void.
        for f in b.faces() {
            let solid = |s| f.shell(s).region().is_solid();
            assert_ne!(solid(FaceSide::Front), solid(FaceSide::Back), "{case}");
        }
    }
}

#[test]
fn two_apart_blocks_are_two_solids_and_nested_ones_are_solid_void_solid() {
    let a = solid(Kind::Block, 0, 1);
    let mut far = generate(Kind::Block, 0, 1).unwrap().draft;
    far.transform(
        &scx_num::math::Affine::translation(Vec3::new(3.0, 0.0, 0.0)),
        &Document::default(),
    )
    .unwrap();
    let b = finish(far);
    let mut sheets = explode(&a, |_| false);
    sheets.extend(explode(&b, |_| false));
    let out = run(&refs(&sheets)).unwrap();
    assert_solid(&out.done.body, 2.0);
    assert_eq!(regions(&out.done.body), (2, 0));

    // A block inside the cavity's void: solid, void, solid; 26 + 1/8.
    let cavity = solid(Kind::Cavity, 0, 1);
    let inner = cube(1.25, 0.5);
    let mut sheets = explode(&cavity, |_| false);
    sheets.extend(explode(&inner, |_| false));
    let out = run(&refs(&sheets)).unwrap();
    assert_solid(&out.done.body, 26.125);
    assert_eq!(regions(&out.done.body), (2, 1));
}

/// Each face by its vertices' points' bits, with whether the material is behind it, and the
/// volume's bits.
type Fingerprint = (Vec<(Vec<[u64; 3]>, bool, bool)>, u64);

/// The [`Fingerprint`] of `b`.
fn fingerprint(b: &Brep) -> Fingerprint {
    let mut faces: Vec<_> = b
        .faces()
        .map(|f| {
            let mut ps: Vec<[u64; 3]> = f
                .uses()
                .map(|u| {
                    let p = u.start().point();
                    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
                })
                .collect();
            ps.sort_unstable();
            ps.dedup();
            let side = |s| f.shell(s).region().is_solid();
            (ps, side(FaceSide::Back), side(FaceSide::Front))
        })
        .collect();
    faces.sort();
    (faces, vol(b).to_bits())
}

#[test]
fn permuted_sheets_give_the_same_bits() {
    let cavity = solid(Kind::Cavity, 3, 2);
    let sheets = explode(&cavity, |f| f.index() % 3 == 0);
    let want = fingerprint(&run(&refs(&sheets)).unwrap().done.body);
    let n = sheets.len();
    for step in [1, 5, 7] {
        let order: Vec<&Brep> = (0..n).map(|i| &sheets[(i * step + 3) % n]).collect();
        let got = fingerprint(&run(&order).unwrap().done.body);
        assert_eq!(got, want, "step {step}");
    }
    // And again, bit for bit.
    let again = fingerprint(&run(&refs(&sheets)).unwrap().done.body);
    assert_eq!(again, want);
}

#[test]
fn an_open_box_is_not_a_solid() {
    let block = solid(Kind::Block, 0, 1);
    let mut sheets = explode(&block, |_| false);
    sheets.pop();
    let e = run(&refs(&sheets)).unwrap_err();
    let StitchError::Refused(StitchRefusal::NotSolid { at, why, .. }) = e else {
        panic!("{e:?}");
    };
    assert_eq!(why, SolidFault::Open);
    assert!(matches!(at, Entity::Edge(_)));
}

#[test]
fn one_sheet_is_not_a_solid() {
    let d = generate(Kind::QuadSheet, 0, 1).unwrap().draft;
    let e = run(&[&finish(d)]).unwrap_err();
    assert!(
        matches!(
            e,
            StitchError::Refused(StitchRefusal::NotSolid {
                operand,
                why: SolidFault::Open,
                ..
            }) if operand == Operand::from_raw(0)
        ),
        "{e:?}"
    );
}

#[test]
fn a_wire_beside_a_closed_box_is_refused() {
    let block = solid(Kind::Block, 0, 1);
    let mut sheets = explode(&block, |_| false);
    let mut d = Draft::new();
    let shell = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
    let (p, q) = (Point3::new(5.0, 0.0, 0.0), Point3::new(6.0, 0.0, 0.0));
    let a = d.add_vertex(VertexRecord::new(p)).unwrap();
    let b = d.add_vertex(VertexRecord::new(q)).unwrap();
    let e = d
        .add_edge(EdgeGeom::new(
            EdgeShape::Curve {
                curve: Curve::Line(Line::new(p, q - p).unwrap()),
                interval: Extent1d::new(0.0, 1.0).unwrap(),
            },
            [a, b],
        ))
        .unwrap();
    d.shell_mut(shell).unwrap().wires.push(e);
    sheets.push(finish(d));
    let err = run(&refs(&sheets)).unwrap_err();
    assert_eq!(
        err,
        StitchError::Refused(StitchRefusal::NotSolid {
            operand: Operand::from_raw(6),
            at: Entity::Edge(e),
            why: SolidFault::Wire,
        })
    );
}

#[test]
fn a_solid_operand_is_unsupported() {
    let block = solid(Kind::Block, 0, 1);
    let e = run(&[&block]).unwrap_err();
    assert_eq!(
        e,
        StitchError::Unsupported(StitchUnsupported::SolidOperand {
            operand: Operand::from_raw(0)
        })
    );
}

#[test]
fn sheets_keep_their_sides_without_want_solid() {
    let block = solid(Kind::Block, 0, 1);
    let sheets = explode(&block, |_| false);
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let out = stitch(&refs(&sheets), Want::Sheets, &mut cx).unwrap();
    assert_eq!(regions(&out.done.body), (0, 0));
    assert_eq!(out.done.body.shells().count(), 1);
}
