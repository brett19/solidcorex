// Written for solidcorex. Apache-2.0.
//! `tangent_face_chain_ordered` (research 74 §52): the faces joined across edges proved
//! smooth at the caller's angle, on made bodies and extrusions whose dihedral angles are the
//! construction's.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

mod along_common;

use along_common::{deg, doc, extrude_polygon, stadium};
use scx_bodyq::{
    BodyqError, BodyqRefusal, BodyqUnsupported, Doubt, FaceChain, tangent_face_chain_ordered,
};
use scx_brep::{Brep, Entity, FaceId};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_testkit::bodies::made::{block, cylinder};

const SMOOTH: f64 = 0.5 * std::f64::consts::PI / 180.0;

fn chain(body: &Brep, seeds: &[FaceId], angle: f64) -> FaceChain {
    tangent_face_chain_ordered(body, seeds, angle, &mut Budget::unlimited(), &Stop::new()).unwrap()
}

#[test]
fn a_stadium_s_walls_chain_and_its_caps_do_not() {
    let (body, f) = stadium();
    let walls = &f.walls[0];
    assert_eq!(walls.len(), 4);
    let got = chain(&body, &[walls[0]], SMOOTH);
    assert!(got.undecided.is_empty());
    // the pick, then the walls in the order the chain reaches them: all four, no cap
    assert_eq!(got.faces[0], walls[0]);
    let mut all = got.faces.clone();
    all.sort();
    let mut want = walls.clone();
    want.sort();
    assert_eq!(all, want);
    // a cap chains to itself alone
    assert_eq!(chain(&body, &[f.start], SMOOTH).faces, vec![f.start]);
}

#[test]
fn a_box_and_a_can_chain_nowhere() {
    let b = block(
        &Frame::WORLD,
        [
            Length::from_mm(1.0),
            Length::from_mm(2.0),
            Length::from_mm(3.0),
        ],
        &doc(),
    )
    .unwrap();
    for f in b.body.faces().map(scx_brep::Face::id) {
        assert_eq!(chain(&b.body, &[f], SMOOTH).faces, vec![f]);
    }
    let c = cylinder(
        &Frame::WORLD,
        Length::from_mm(3.0),
        Length::from_mm(7.0),
        &doc(),
    )
    .unwrap();
    // the wall's seam is smooth and joins it to itself
    let got = chain(&c.body, &[c.faces.wall], SMOOTH);
    assert_eq!(got.faces, vec![c.faces.wall]);
    assert!(got.undecided.is_empty());
}

#[test]
fn picks_come_first_once_each_in_the_order_given() {
    let (body, f) = stadium();
    let w = &f.walls[0];
    let got = chain(&body, &[w[2], f.start, w[2]], SMOOTH);
    assert_eq!(&got.faces[..2], &[w[2], f.start]);
    assert_eq!(got.faces.len(), 5);
    assert!(chain(&body, &[], SMOOTH).faces.is_empty());
}

#[test]
fn the_angle_decides_a_kink() {
    // two walls 0.3 degrees apart
    let k = deg(0.3);
    let (body, f) = extrude_polygon(
        &[
            [0.0, 0.0],
            [10.0, 0.0],
            [20.0, 10.0 * fmath::tan(k)],
            [20.0, 10.0],
            [0.0, 10.0],
        ],
        2.0,
    );
    let w = &f.walls[0];
    assert_eq!(chain(&body, &[w[0]], SMOOTH).faces, vec![w[0], w[1]]);
    assert_eq!(chain(&body, &[w[1]], SMOOTH).faces, vec![w[1], w[0]]);
    let apart = chain(&body, &[w[0]], deg(0.2));
    assert_eq!(apart.faces, vec![w[0]]);
    assert!(apart.undecided.is_empty());
    // at the angle itself the proof cannot tell: the edge joins nothing and is named
    let at = chain(&body, &[w[0]], k);
    assert_eq!(at.faces, vec![w[0]]);
    assert_eq!(at.undecided.len(), 1);
    assert!(matches!(at.undecided[0].why, Doubt::Straddles { .. }));
    let e = at.undecided[0].edge;
    let fins: Vec<FaceId> = body.edge(e).unwrap().fins().map(|x| x.face).collect();
    assert!(fins.contains(&w[0]) && fins.contains(&w[1]));
}

#[test]
fn a_nurbs_face_is_named_and_the_chain_goes_on() {
    let (body, e, nurbs) = along_common::nurbs_meets_plane();
    let seed = body
        .edge(e)
        .unwrap()
        .fins()
        .map(|f| f.face)
        .find(|&f| f != nurbs)
        .unwrap();
    let got = chain(&body, &[seed], SMOOTH);
    assert_eq!(got.faces, vec![seed]);
    assert_eq!(got.undecided.len(), 1);
    assert_eq!(got.undecided[0].edge, e);
    assert_eq!(
        got.undecided[0].why,
        Doubt::Unsupported(BodyqUnsupported::Nurbs {
            at: Entity::Face(nurbs)
        })
    );
}

#[test]
fn refusals_work_and_determinism() {
    let (body, f) = stadium();
    let (mut b, s) = (Budget::unlimited(), Stop::new());
    for angle in [-0.1, f64::NAN, f64::INFINITY, std::f64::consts::PI, 4.0] {
        assert_eq!(
            tangent_face_chain_ordered(&body, &f.walls[0][..1], angle, &mut b, &s),
            Err(BodyqError::Refused(BodyqRefusal::Tolerance)),
            "{angle}"
        );
    }
    let ghost = FaceId::from_raw(9999);
    assert_eq!(
        tangent_face_chain_ordered(&body, &[ghost], SMOOTH, &mut b, &s),
        Err(BodyqError::Refused(BodyqRefusal::NoEntity {
            entity: Entity::Face(ghost)
        }))
    );
    let used = |budget: &mut Budget| {
        let got = tangent_face_chain_ordered(&body, &f.walls[0][..1], SMOOTH, budget, &s);
        (got, budget.used())
    };
    let mut one = Budget::unlimited();
    let (first, work) = used(&mut one);
    let mut two = Budget::unlimited();
    assert_eq!(used(&mut two), (first.clone(), work));
    assert!(work > 0);
    let mut tight = Budget::new(work - 1);
    assert!(matches!(
        used(&mut tight).0,
        Err(BodyqError::Exhausted { .. })
    ));
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        tangent_face_chain_ordered(
            &body,
            &f.walls[0][..1],
            SMOOTH,
            &mut Budget::unlimited(),
            &stop
        ),
        Err(BodyqError::Cancelled)
    );
}
