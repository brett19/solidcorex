// Written for solidcorex. Apache-2.0.
//! Stage 4 (72 §14.30): a pair with no closed form is dropped when its faces provably cannot
//! touch, and refused when they may. Each case is worked by hand.
//!
//! The torus is the pin's R=5 r=1.5 about z. On the plane `y = -3` it reaches
//! `rho = sqrt(x^2 + 9) <= 6.5`, so `x <= sqrt(6.5^2 - 9) = sqrt(33.25)`. A block on
//! `[x0, 8] x [-4, -3] x [-1, 1]` is clear of it by `sqrt(x0^2 + 9) - 6.5` (at its edge
//! `x = x0, y = -3, z = 0`), and its faces' boxes meet the torus's for any `x0` below 6.5.
//! Its faces: bottom and top (`z = -1, 1`, planes square to the axis: circles, a closed
//! form), `sides[0]` on `y = -4`, `sides[1]` on `x = 8`,
//! `sides[2]` on `y = -3` and `sides[3]` on `x = x0`. The plane sides are parallel to the
//! axis: spiric sections, no closed form. The coincidence here is the resolution, 1e-5.

use scx_brep::{Document, FaceId};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::Vec3;
use scx_ssi::SsiError;
use scx_surface::SurfaceKind;
use scx_testkit::bodies::made::{BlockFaces, TorusFaces, block, cylinder, sphere, torus};

use super::{pt, run};
use crate::faceface::{FaceFace, face_face};
use crate::geom::Operands;
use crate::{BooleanError, BooleanUnsupported};

const Z: [f64; 3] = [0.0, 0.0, 1.0];
const X: [f64; 3] = [1.0, 0.0, 0.0];

fn frame(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    Frame::new(
        pt(o),
        Vec3::new(z[0], z[1], z[2]),
        Vec3::new(x[0], x[1], x[2]),
    )
    .unwrap()
}

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

/// The torus's and the block's faces, stage 1's pairs and stage 4's answer.
struct Case {
    t: TorusFaces,
    b: BlockFaces,
    pairs: Vec<(FaceId, FaceId)>,
    f: Result<FaceFace, BooleanError>,
}

/// The torus and the block clear of it by `gap`, with stage 4's answer on them.
fn torus_and_block(gap: f64) -> Case {
    let doc = Document::default();
    let t = torus(&frame([0.0; 3], Z, X), mm(5.0), mm(1.5), &doc).unwrap();
    let x0 = ((6.5 + gap) * (6.5 + gap) - 9.0).sqrt();
    let b = block(
        &frame([x0, -4.0, -1.0], Z, X),
        [mm(8.0 - x0), mm(1.0), mm(2.0)],
        &doc,
    )
    .unwrap();
    let (ta, bb) = (&t.body, &b.body);
    let i = run(ta, bb).unwrap();
    let f = face_face(
        Operands { a: ta, b: bb },
        &i,
        &mut Budget::unlimited(),
        &Stop::new(),
    );
    Case {
        t: t.faces,
        b: b.faces,
        pairs: i.pairs,
        f,
    }
}

/// Ten coincidences clear: every side pair has no closed form, but no side can touch the
/// torus, so all are dropped; the caps' circles miss the block. Stage 4 answers nothing.
#[test]
fn a_block_just_clear_of_a_torus_is_dropped_pair_by_pair() {
    let Case { t, b, pairs, f } = torus_and_block(1e-4);
    // The pairs stage 1 found: the torus with every face of the block but the one on
    // `x = 8` (the torus's box is exact now, `|x| <= 6.5`, so that face's box is clear of
    // it; it was loose enough to reach `x = 8` before).
    let mut want: Vec<_> = [b.bottom, b.top, b.sides[0], b.sides[2], b.sides[3]]
        .map(|s| (t.face, s))
        .to_vec();
    want.sort_unstable();
    assert_eq!(pairs, want);
    assert_eq!(f.unwrap(), FaceFace::default());
}

/// Touching at `(x0, -3, 0)` (a point of the outer equator), or clear by half the
/// coincidence, which is touching too: `sides[0]` is clear by `sqrt(x0^2 + 16) - 6.5`,
/// about 0.53, and dropped; `sides[2]` (`y = -3`) holds the touch on its edge and is the
/// first pair kept, refused for its spiric section. The refusal names the region the
/// bisection could not clear: the touch lies in it.
#[test]
fn a_block_touching_a_torus_keeps_the_touching_pair() {
    for gap in [0.0, 5e-6] {
        let Case { t, b, f, .. } = torus_and_block(gap);
        let e = f.unwrap_err();
        let BooleanError::Unsupported(BooleanUnsupported::SsiPair {
            a,
            b: fb,
            kinds,
            cause: SsiError::Unresolved { at, size },
        }) = e
        else {
            panic!("gap {gap}: {e:?}");
        };
        assert_eq!(
            (a, fb, kinds),
            (t.face, b.sides[2], [SurfaceKind::Torus, SurfaceKind::Plane]),
            "gap {gap}"
        );
        let x0 = ((6.5 + gap) * (6.5 + gap) - 9.0).sqrt();
        let touch = pt([x0, -3.0, 0.0]);
        assert!(
            at.distance(touch) <= size.mm(),
            "gap {gap}: {at:?} {size:?}"
        );
        // Eight halvings of the overlap: a piece well under the block's 1 mm depth.
        assert!(size.mm() < 0.25, "gap {gap}: {size:?}");
        assert_eq!(e.name(), "ssi_pair");
        assert!(
            e.to_string()
                .ends_with("is neither proved apart nor loop-free"),
            "{e}"
        );
    }
}

/// A sphere of radius 3 off a cylinder's axis by `5 + gap` along the diagonal `x = y`, so
/// its box meets the r=2 cylinder's (x and y from about 0.54) while the shared box is not
/// clear of either surface alone. Before BO22 this pair had no closed form and needed the
/// bisection (proved apart at depth 8 clear by 0.8, refused by 0.5 or less); now the sphere's
/// nearest approach to the axis decides it (`scx_ssi::analytic`'s touch), at any gap.
fn cylinder_and_sphere(gap: f64) -> Result<FaceFace, BooleanError> {
    let doc = Document::default();
    let c = cylinder(&frame([0.0; 3], Z, X), mm(2.0), mm(10.0), &doc).unwrap();
    let d = (5.0 + gap) / 2.0_f64.sqrt();
    let s = sphere(&frame([d, d, 5.0], Z, X), mm(3.0), &doc).unwrap();
    let i = run(&c.body, &s.body).unwrap();
    assert_eq!(i.pairs, vec![(c.faces.wall, s.faces.face)]);
    face_face(
        Operands {
            a: &c.body,
            b: &s.body,
        },
        &i,
        &mut Budget::unlimited(),
        &Stop::new(),
    )
}

#[test]
fn a_sphere_clear_of_a_cylinder_is_apart_by_the_closed_form() {
    // Clear by more than the resolution (1e-5), however little: no bisection needed.
    for gap in [0.8, 0.3, 1e-3, 2e-5] {
        assert_eq!(
            cylinder_and_sphere(gap).unwrap(),
            FaceFace::default(),
            "{gap}"
        );
    }
}

#[test]
fn a_sphere_touching_a_cylinder_touches_it_at_one_point() {
    // At the wall's point nearest the centre, (sqrt 2, sqrt 2, 5); a miss or press within the
    // resolution is the same touch.
    let h = 2.0_f64.sqrt();
    for gap in [0.0, 5e-6, -5e-6] {
        let f = cylinder_and_sphere(gap).unwrap();
        assert!(f.sections.is_empty() && f.lying.is_empty() && f.coincident.is_empty());
        let [t] = f.touches[..] else {
            panic!("{gap}: {:?}", f.touches);
        };
        assert!(t.at.distance(pt([h, h, 5.0])) < 1e-12, "{gap}: {:?}", t.at);
    }
}
