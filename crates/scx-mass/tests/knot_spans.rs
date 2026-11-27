// Written for solidcorex. Apache-2.0.
//! The pin's #2091 (`issue_2091_precise_volume_spans`): where a NURBS surface's derivatives
//! jump at a knot, the integrals must split there and never take a piece's end from the
//! span beyond it. Here a block's top face is replaced by a NURBS surface with the same
//! boundary and a bump inside it, C0 at a double v knot (its v derivative jumps) and C1 at
//! a simple u knot; the volume holds accuracies 1e-9 to 1e-12 against its closed form and
//! the area against tensor Gauss-Legendre, knot span by knot span.
//!
//! The Kronrod rules never evaluate a piece's end (their nodes are interior), so the pin's
//! second check (a piece's end evaluated in its own span) has no counterpart here.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::sync::Arc;

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, FaceId, FaceSide};
use scx_mass::{MassSpec, RelAccuracy, area, face_area, mass, volume};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, KnotVector};
use scx_nurbs_surface::{Net, NurbsSurface};
use scx_surface::Surface;
use scx_testkit::bodies::{Kind, generate};

/// The bump's height.
const H: f64 = 0.4;
/// The u knots (degree 2, a simple knot at 0.4) and the bump's u coefficients.
const U: [f64; 7] = [0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0];
const CU: [f64; 4] = [0.0, 1.0, 0.6, 0.0];
/// The v knots (degree 2, a double knot at 0.5: C0) and the bump's v coefficients.
const V: [f64; 8] = [0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0];
const CV: [f64; 5] = [0.0, 1.0, 0.3, 0.8, 0.0];

/// The degree-2 Greville abscissae of `knots`, each basis function's integral, over the
/// face's `[lo, hi]` (the knots are on `[0, 1]`).
fn greville(knots: &[f64], lo: f64, hi: f64) -> (Vec<f64>, Vec<f64>) {
    let s = |t: f64| lo + t * (hi - lo);
    let n = knots.len() - 3;
    let xi = (0..n)
        .map(|i| s(0.5 * (knots[i + 1] + knots[i + 2])))
        .collect();
    let int = (0..n)
        .map(|i| (knots[i + 3] - knots[i]) * (hi - lo) / 3.0)
        .collect();
    (xi, int)
}

/// The seed-0 block with its top face bumped; the signed volume the bump adds.
fn bumped() -> (Brep, FaceId, f64) {
    let doc = Document::default();
    // The same seed twice: one finished to find the top face, the other bumped.
    let found = generate(Kind::Block, 0, 1).unwrap().draft;
    let found = found.finish(&doc).map_err(|u| u.cause).unwrap().0;
    let top = found
        .faces()
        .find(|f| f.bounding_box().is_some_and(|b| b.z_min() > 0.5))
        .unwrap();
    let id = top.id();
    let uv = top.uv_box().unwrap();
    let outward = top.shell(FaceSide::Back).region().is_solid();
    let mut d = generate(Kind::Block, 0, 1).unwrap().draft;
    let rec = d.face_mut(id).unwrap();
    let Surface::Plane(plane) = rec.surface.clone() else {
        panic!("the block's top is a plane");
    };
    let flat = Surface::Plane(plane);
    let at = |u: f64, v: f64| flat.point(Point2::new(u, v));
    let p0 = at(0.0, 0.0);
    let normal = (at(1.0, 0.0) - p0)
        .cross(at(0.0, 1.0) - p0)
        .normalized()
        .unwrap();
    let n: Vec3 = normal.into();
    let (xu, iu) = greville(&U, uv.x_min(), uv.x_max());
    let (xv, iv) = greville(&V, uv.y_min(), uv.y_max());
    let mut pts = Vec::new();
    for (i, &u) in xu.iter().enumerate() {
        for (j, &v) in xv.iter().enumerate() {
            let p = at(u, v) + n * (H * CU[i] * CV[j]);
            pts.push([p.x, p.y, p.z]);
        }
    }
    let scale = |k: &[f64], lo: f64, hi: f64| k.iter().map(|t| lo + t * (hi - lo)).collect();
    let ku = KnotVector::new(2, scale(&U, uv.x_min(), uv.x_max())).unwrap();
    let kv = KnotVector::new(2, scale(&V, uv.y_min(), uv.y_max())).unwrap();
    let net = Net::new(xu.len(), xv.len(), pts).unwrap();
    rec.surface = Surface::Nurbs(Arc::new(NurbsSurface::new(ku, kv, net, None).unwrap()));
    let added: f64 = H
        * iu.iter().zip(CU).map(|(a, c)| a * c).sum::<f64>()
        * iv.iter().zip(CV).map(|(a, c)| a * c).sum::<f64>();
    let b = d.finish(&doc).map_err(|u| u.cause).unwrap().0;
    let sign = if outward { 1.0 } else { -1.0 };
    (b, id, sign * added)
}

/// Gauss-Legendre nodes and weights on `[-1, 1]` (Newton on `P_n`).
fn gauss_legendre(n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let mut x = fmath::cos(std::f64::consts::PI * (i as f64 + 0.75) / (n as f64 + 0.5));
            let mut dp = 0.0;
            for _ in 0..100 {
                let (mut p0, mut p1) = (1.0, x);
                for k in 2..=n {
                    let k = k as f64;
                    (p0, p1) = (p1, ((2.0 * k - 1.0) * x * p1 - (k - 1.0) * p0) / k);
                }
                dp = n as f64 * (x * p1 - p0) / (x * x - 1.0);
                let dx = p1 / dp;
                x -= dx;
                if dx.abs() < 1e-16 {
                    break;
                }
            }
            (x, 2.0 / ((1.0 - x * x) * dp * dp))
        })
        .collect()
}

/// The surface's area span by span, 24 points a span each way.
fn reference_area(s: &Surface, ku: &[f64], kv: &[f64]) -> f64 {
    let gl = gauss_legendre(24);
    let spans = |k: &[f64]| -> Vec<(f64, f64)> {
        k.windows(2)
            .filter(|w| w[0] < w[1])
            .map(|w| (w[0], w[1]))
            .collect()
    };
    let mut sum = 0.0;
    for (u0, u1) in spans(ku) {
        for (v0, v1) in spans(kv) {
            for &(x, wx) in &gl {
                for &(y, wy) in &gl {
                    let u = u0 + 0.5 * (x + 1.0) * (u1 - u0);
                    let v = v0 + 0.5 * (y + 1.0) * (v1 - v0);
                    let j = s.jet::<1>(Point2::new(u, v), [Approach::Above; 2]);
                    let c = j.get(1, 0).unwrap().cross(j.get(0, 1).unwrap()).length();
                    sum += wx * wy * c * 0.25 * (u1 - u0) * (v1 - v0);
                }
            }
        }
    }
    sum
}

#[test]
fn a_volume_across_a_c0_knot_holds_its_accuracy() {
    let (b, _, added) = bumped();
    let want = 1.0 + added;
    assert!(added.abs() > 0.01, "the bump moves the volume: {added}");
    for e in 9..=12 {
        let acc = fmath::powi(10.0, -e);
        let r = RelAccuracy::new(acc).unwrap();
        let v = volume(
            &b,
            r,
            &mut Scratch::new(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        let miss = (v.value - want).abs();
        assert!(
            miss <= acc,
            "1e-{e}: {} against {want}, miss {miss:e}",
            v.value
        );
        assert!(
            miss <= v.error.max(1e-14),
            "1e-{e}: miss {miss:e}, estimate {:e}",
            v.error
        );
        let spec = MassSpec::new(r);
        let m = mass(
            &b,
            &spec,
            &mut Scratch::new(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        assert!(
            (m.volume - want).abs() <= acc,
            "1e-{e}: mass volume {}",
            m.volume
        );
    }
}

#[test]
fn an_area_across_a_c0_knot_holds_its_accuracy() {
    let (b, id, _) = bumped();
    let face = b.faces().find(|f| f.id() == id).unwrap();
    let s = face.surface();
    let Surface::Nurbs(n) = s else {
        panic!("the top is the NURBS surface")
    };
    let want = reference_area(
        s,
        n.knots(scx_nurbs_surface::ParamAxis::U),
        n.knots(scx_nurbs_surface::ParamAxis::V),
    );
    assert!(want > 1.01, "the bump adds area: {want}");
    for e in 9..=12 {
        let acc = fmath::powi(10.0, -e);
        let r = RelAccuracy::new(acc).unwrap();
        let a = face_area(
            face,
            r,
            &mut Scratch::new(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        let miss = (a.value - want).abs();
        assert!(
            miss <= acc * want,
            "1e-{e}: {} against {want}, miss {miss:e}",
            a.value
        );
        assert!(
            miss <= a.error.max(1e-14),
            "1e-{e}: miss {miss:e}, estimate {:e}",
            a.error
        );
        let total = area(
            &b,
            r,
            &mut Scratch::new(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
        assert!(
            (total.value - (5.0 + want)).abs() <= 6.0 * acc,
            "1e-{e}: body area {}",
            total.value
        );
    }
}

#[test]
fn the_bumped_face_moves_the_centroid_upward() {
    let (b, _, added) = bumped();
    let spec =
        MassSpec::new(RelAccuracy::new(1e-10).unwrap()).with_about(Point3::new(0.0, 0.0, 0.0));
    let m = mass(
        &b,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    let c = m.centroid().unwrap();
    assert!(added > 0.0 && c.z > 0.5, "{c:?}");
}
