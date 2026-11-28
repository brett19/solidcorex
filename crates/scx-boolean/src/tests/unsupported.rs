// Written for solidcorex. Apache-2.0.
//! The pin's Boolean cases that are out of scope for step 4, as expected-`Unsupported`
//! tests: each names the faces and the kinds of the pair whose surfaces meet in a curve
//! with no closed form here (72 §5.2), the quartics that step 5's marcher answers.
//!
//! Every case here is `SsiPair`, a real cause: none reaches `Stage(Imprint)`, so none
//! of these tests changes when BO3 removes that variant. The operands are the pin's
//! (`boolean_api.rs`, millimetres), rebuilt with the testkit's port of `scx-make`; the pin's cylinder rises
//! from its base point along z, which is the constructor's `z = 0` to `z = height` too.
//!
//! Stage 4 takes the pairs in order (a's face id, then b's) and stops at the first one
//! it cannot answer, so each case names that first pair, worked out by hand. A pair is
//! any two faces whose boxes meet; one with no closed form whose faces provably cannot
//! touch is dropped (72 §14.30), so the pair named is one that may touch.

use scx_brep::op::Settings;
use scx_brep::{Brep, Document, FaceId};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::Vec3;
use scx_ssi::SsiError;
use scx_surface::SurfaceKind;
use scx_testkit::bodies::made::{block, cylinder, torus};

use super::{call, pt};
use crate::{BooleanError, BooleanUnsupported};

fn frame(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    Frame::new(
        pt(o),
        Vec3::new(z[0], z[1], z[2]),
        Vec3::new(x[0], x[1], x[2]),
    )
    .unwrap()
}

const Z: [f64; 3] = [0.0, 0.0, 1.0];
const X: [f64; 3] = [1.0, 0.0, 0.0];

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

/// A pair that must answer `SsiPair` with these faces and kinds, in either operand order:
/// swapped, the faces and kinds swap with the operands (the first pair in `a`'s id order
/// is the same pair in each of these cases). The cause is an undecided region, the same
/// one either way: the proof that clears regions reads both surfaces alike.
fn expect_ssi_pair(a: &Brep, b: &Brep, faces: [FaceId; 2], kinds: [SurfaceKind; 2]) {
    let doc = Document::default();
    let settings = Settings::new();
    let pair = |e: BooleanError| {
        let BooleanError::Unsupported(BooleanUnsupported::SsiPair { a, b, kinds, cause }) = e
        else {
            panic!("{e:?}");
        };
        assert!(
            matches!(cause, SsiError::Unresolved { size, .. } if size.mm() > 0.0),
            "{cause:?}"
        );
        ([a, b], kinds, cause)
    };
    let e = call(a, b, doc, &settings);
    assert_eq!(e.name(), "ssi_pair");
    let (fwd, kf, cause) = pair(e);
    assert_eq!((fwd, kf), (faces, kinds));
    let (rev, kr, swapped) = pair(call(b, a, doc, &settings));
    assert_eq!((rev, kr), ([faces[1], faces[0]], [kinds[1], kinds[0]]));
    assert_eq!(swapped, cause);
}

/// The pin's `tpipe`: an R=2 pipe along z from -10 to 10 and an r=1.5 branch from the axis
/// out along +y to 10. Radii unlike, axes crossing at right angles: the walls meet in a
/// quartic (the saddle curve), closed form only for equal radii. The walls are both id 0,
/// so theirs is the first pair (the planar pairs after it are not reached).
#[test]
fn tpipe_walls_of_unlike_radii_meet_in_a_quartic() {
    let doc = Document::default();
    let a = cylinder(&frame([0.0, 0.0, -10.0], Z, X), mm(2.0), mm(20.0), &doc).unwrap();
    let b = cylinder(
        &frame([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        mm(1.5),
        mm(10.0),
        &doc,
    )
    .unwrap();
    expect_ssi_pair(
        &a.body,
        &b.body,
        [a.faces.wall, b.faces.wall],
        [SurfaceKind::Cylinder, SurfaceKind::Cylinder],
    );
}

/// The pin's `seam-torus-box` and `seam-vertex-torus-box`: the same torus and a block
/// touching its outer equator (a seam circle) at (0, 6.5, 0), or at (6.5, 0, 0) where its
/// two seams cross (the torus's one vertex). The block's side planes are parallel to the
/// torus's axis, and a plane parallel to the axis meets the torus in a spiric section, a
/// quartic. The block's bottom and top (z = -3 and 3) are clear of the tube (|z| <= 1.5).
/// In the first case the first side (`y = 6.5`) is the face the torus touches. In the
/// second the first side is on `y = -3`: its box meets the torus's, but the torus never
/// reaches it (at `y = -3` it has `x <= sqrt(6.5^2 - 9)`, about 5.77, and the face starts
/// at 6.5), so stage 4 drops it as provably apart (72 §14.30). `sides[1]` (`x =
/// 10.5`) is clear of the torus's box, and `sides[2]` (`y = 3`) is apart like `y = -3`:
/// the pair named is the one the torus touches, `sides[3]` on `x = 6.5`.
#[test]
fn torus_box_on_a_seam_circle_and_at_the_seam_vertex_meets_in_a_spiric_section() {
    let doc = Document::default();
    let t = torus(&frame([0.0; 3], Z, X), mm(5.0), mm(1.5), &doc).unwrap();
    let on_circle = block(
        &frame([-3.0, 6.5, -3.0], Z, X),
        [mm(6.0), mm(4.0), mm(6.0)],
        &doc,
    )
    .unwrap();
    let at_vertex = block(
        &frame([6.5, -3.0, -3.0], Z, X),
        [mm(4.0), mm(6.0), mm(6.0)],
        &doc,
    )
    .unwrap();
    for (b, side) in [(on_circle, 0), (at_vertex, 3)] {
        expect_ssi_pair(
            &t.body,
            &b.body,
            [t.faces.face, b.faces.sides[side]],
            [SurfaceKind::Torus, SurfaceKind::Plane],
        );
    }
}
