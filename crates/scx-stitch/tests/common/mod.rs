// Written for solidcorex. Apache-2.0.
//! Helpers shared by the stitch's solid tests: bodies exploded to one sheet per face, and
//! volume and solidity read through `scx-mass`.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{
    Brep, Document, Draft, EdgeId, EdgeUse, FaceId, LoopRecord, RegionId, ShellRecord, UseGeom,
    VertexId,
};
use scx_mass::{RelAccuracy, outward, volume};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_stitch::{StitchError, Stitched, Want, stitch};
use scx_surface::{Plane, Surface};
use scx_testkit::bodies::{Kind, generate};

pub fn finish(d: Draft) -> Brep {
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

pub fn solid(kind: Kind, seed: u64, size: usize) -> Brep {
    finish(generate(kind, seed, size).unwrap().draft)
}

/// The unit block scaled to `side` and moved to `lo` on each axis.
pub fn cube(lo: f64, side: f64) -> Brep {
    let mut d = generate(Kind::Block, 0, 1).unwrap().draft;
    let doc = Document::default();
    d.transform(
        &scx_num::math::Affine::uniform_scaling(Point3::ORIGIN, side).unwrap(),
        &doc,
    )
    .unwrap();
    d.transform(
        &scx_num::math::Affine::translation(Vec3::new(lo, lo, lo)),
        &doc,
    )
    .unwrap();
    finish(d)
}

pub fn run(sheets: &[&Brep]) -> Result<Stitched, StitchError> {
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let settings = Settings::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    stitch(sheets, Want::Solid, &mut cx)
}

/// `body`'s faces, each its own sheet (one shell in the outside on both sides), in face
/// order; a face `flip` names, if planar, has its plane's normal reversed and its loops run
/// the other way, so its front is the other side.
pub fn explode(body: &Brep, flip: impl Fn(FaceId) -> bool) -> Vec<Brep> {
    let doc = Document::default();
    body.faces()
        .map(|f| {
            let mut d = Draft::new();
            let shell = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
            let mut vs: BTreeMap<VertexId, VertexId> = BTreeMap::new();
            let mut es: BTreeMap<EdgeId, EdgeId> = BTreeMap::new();
            let mut rec = f.record().clone();
            for u in rec.loops.iter_mut().flat_map(|l| l.uses.iter_mut()) {
                let e = u.edge;
                if let Some(&x) = es.get(&e) {
                    u.edge = x;
                    continue;
                }
                let mut g = body.edge(e).unwrap().geometry().clone();
                for end in &mut g.ends {
                    let v = *end;
                    *end = *vs.entry(v).or_insert_with(|| {
                        d.add_vertex(body.vertex(v).unwrap().record().clone())
                            .unwrap()
                    });
                }
                let x = d.add_edge(g).unwrap();
                es.insert(e, x);
                u.edge = x;
            }
            rec.sides = [shell, shell];
            let flipped = match &rec.surface {
                Surface::Plane(p) if flip(f.id()) => {
                    let fr = p.frame();
                    Some(Surface::Plane(Plane::new(
                        Frame::new(fr.origin(), -Vec3::from(fr.z()), Vec3::from(fr.x())).unwrap(),
                    )))
                }
                _ => None,
            };
            if let Some(s) = flipped {
                rec.surface = s;
                rec.loops = rec
                    .loops
                    .iter()
                    .map(|l| {
                        LoopRecord::new(
                            l.uses
                                .iter()
                                .rev()
                                .map(|u| {
                                    EdgeUse::new(
                                        u.edge,
                                        u.sense.reversed(),
                                        UseGeom::Point(Point2::ORIGIN),
                                    )
                                })
                                .collect(),
                        )
                    })
                    .collect();
                let x = d.add_face(rec).unwrap();
                d.write_pcurves(x, &doc).unwrap();
            } else {
                d.add_face(rec).unwrap();
            }
            finish(d)
        })
        .collect()
}

pub fn refs(sheets: &[Brep]) -> Vec<&Brep> {
    sheets.iter().collect()
}

pub fn vol(b: &Brep) -> f64 {
    volume(
        b,
        RelAccuracy::new(1e-12).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

/// The body is outward (MA4's certificate: each region's shells' signs agree with it) and
/// its volume is `want` to 1e-9 relative.
pub fn assert_solid(b: &Brep, want: f64) {
    outward(b, &mut Budget::unlimited(), &Stop::new()).unwrap();
    let v = vol(b);
    assert!(
        (v - want).abs() <= 1e-9 * want.abs().max(1.0),
        "volume {v}, want {want}"
    );
}

/// The solid regions and the voids (not outside, not solid).
pub fn regions(b: &Brep) -> (usize, usize) {
    let solid = b.regions().filter(|r| r.is_solid()).count();
    let void = b
        .regions()
        .filter(|r| !r.is_solid() && !r.is_outside())
        .count();
    (solid, void)
}

