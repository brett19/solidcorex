// Written for solidcorex. Apache-2.0.
//! A horn torus (MK4b) against the contract: its tube meets the axis at a pole, a vertex
//! whose collapsed edge is the inner equator. The body is a closed outward solid; the pole
//! is a pinch of the surface (two lobes meet there) that the structure pass names or not
//! as the contract's `Contacts` allow.

#![allow(missing_docs, clippy::pedantic, clippy::unwrap_used, clippy::panic)]

use scx_brep::Document;
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_check::{Class, Contacts, Scope, check, contract};
use scx_num::budget::Stop;
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::bodies::made::torus;

fn frame() -> Frame {
    Frame::new(
        Point3::new(3.0, -2.0, 1.0),
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(-3.0, 0.5, 1.0),
    )
    .unwrap()
}

#[test]
fn a_horn_torus_is_a_checked_solid() {
    let doc = Document::default();
    let body = torus(&frame(), Length::from_mm(4.0), Length::from_mm(4.0), &doc)
        .unwrap()
        .body;
    let (settings, stop) = (Settings::new(), Stop::new());
    let mut scratch = Scratch::default();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let found = check(&body, &mut cx, Scope::Import).unwrap();
    assert_eq!(found.class, Class::Solid, "{:?}", found.list);
    println!("findings: {:?}", found.list);
    for contacts in [Contacts::Refused, Contacts::Radial, Contacts::Manifold] {
        let r = contract(&body, &cx, contacts);
        println!("{contacts:?}: {:?}", r.as_ref().map(|_| ()));
    }
}
