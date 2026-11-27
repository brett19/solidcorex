// Written for solidcorex. Apache-2.0.
//! `scx-mass`'s face memo (research 72 MA5) across Booleans that keep faces: the bench
//! recipes (BO6, BX1) run to their last step, measured, and then measured again after it
//! on the same `Scratch`. The warm result must be the cold one bit for bit, values and
//! stated errors, and charge the same work; most faces must hit, and those the last step
//! changed must miss.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use scx_boolean::{BooleanOp, boolean};
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document};
use scx_mass::{Mass, MassSpec, RelAccuracy, Sym3, mass, volume};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::recipe::{self, Op, Part, Recipe, Shape};

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn operand(p: &Part, doc: Document) -> Brep {
    let f = &p.frame;
    let f = Frame::new(
        Point3::new(f.origin[0], f.origin[1], f.origin[2]),
        Vec3::new(f.z[0], f.z[1], f.z[2]),
        Vec3::new(f.x[0], f.x[1], f.x[2]),
    )
    .unwrap();
    match &p.shape {
        Shape::Block { size } => {
            scx_testkit::bodies::made::block(&f, size.map(mm), &doc).map(|m| m.body)
        }
        Shape::Cylinder { radius, height } => {
            scx_testkit::bodies::made::cylinder(&f, mm(*radius), mm(*height), &doc).map(|m| m.body)
        }
        other => panic!("not a bench operand: {other:?}"),
    }
    .unwrap()
}

fn case(name: &str) -> Recipe {
    recipe::all(Document::default().resolution().zone().mm())
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no recipe named {name}"))
}

/// The recipe's result before its last step, and after it.
fn last_step(r: &Recipe) -> (Brep, Brep) {
    let doc = Document::default();
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut body = operand(&r.base, doc);
    let mut before = body.clone();
    for s in &r.steps {
        let tool = operand(&s.tool, doc);
        let op = match s.op {
            Op::Union => BooleanOp::Union,
            Op::Intersection => BooleanOp::Intersect,
            Op::Difference => BooleanOp::Subtract,
        };
        let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
        let next = boolean(&body, &tool, op, &mut cx).unwrap().body;
        before = std::mem::replace(&mut body, next);
    }
    (before, body)
}

/// Every number of a [`Mass`], as bits.
fn bits(m: &Mass) -> Vec<u64> {
    let v = |p: [f64; 3]| p.map(f64::to_bits);
    let s = |s: &Sym3| [s.xx, s.yy, s.zz, s.yz, s.zx, s.xy].map(f64::to_bits);
    let e = &m.error;
    let mut out = vec![m.density.to_bits(), m.volume.to_bits(), m.area.to_bits()];
    out.extend(v([m.about.x, m.about.y, m.about.z]));
    out.extend(v([m.first.x, m.first.y, m.first.z]));
    out.extend(s(&m.second));
    out.extend(v([m.area_first.x, m.area_first.y, m.area_first.z]));
    out.extend([e.volume.to_bits(), e.area.to_bits()]);
    out.extend(v([e.first.x, e.first.y, e.first.z]));
    out.extend(s(&e.second));
    out.extend(v([e.area_first.x, e.area_first.y, e.area_first.z]));
    out
}

const ACC: f64 = 1e-9;

fn measure(b: &Brep, scratch: &mut Scratch) -> (Vec<u64>, u64) {
    let mut budget = Budget::unlimited();
    let spec = MassSpec::new(RelAccuracy::new(ACC).unwrap());
    let m = mass(b, &spec, scratch, &mut budget, &Stop::new()).unwrap();
    (bits(&m), budget.used())
}

/// Measures `name`'s result cold and warm (after its body before the last step), and
/// returns the faces of the result and the entries the warm call added (its misses).
fn memo_across(name: &str) -> (usize, usize) {
    let (before, after) = last_step(&case(name));
    let cold = measure(&after, &mut Scratch::new());
    let mut warm = Scratch::new();
    measure(&before, &mut warm);
    let filled = warm.len();
    assert_eq!(
        measure(&after, &mut warm),
        cold,
        "{name}: warm against cold"
    );
    let misses = warm.len() - filled;
    // Determinism: a third call, all hits, and a memo filled in another order.
    assert_eq!(measure(&after, &mut warm), cold, "{name}: all hits");
    assert_eq!(warm.len(), filled + misses, "{name}: a hit adds nothing");
    let mut other = Scratch::new();
    let a = RelAccuracy::new(1e-6).unwrap();
    volume(
        &after,
        a,
        &mut other,
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    measure(&after, &mut other);
    measure(&before, &mut other);
    assert_eq!(
        measure(&after, &mut other),
        cold,
        "{name}: another fill order"
    );
    (after.faces().count(), misses)
}

#[test]
fn small_hole_cases_keep_their_faces_in_the_memo() {
    for name in ["box-cyl-hole", "plate-4"] {
        let (faces, misses) = memo_across(name);
        assert!(misses < faces, "{name}: {misses} misses of {faces}");
    }
}

#[test]
fn plate_12_hits_all_but_the_faces_its_last_hole_changed() {
    // The last hole changes the plate's top and bottom (one more loop each) and adds its
    // wall; every other wall is kept.
    let (faces, misses) = memo_across("plate-12");
    assert!(faces > 140, "{faces} faces");
    assert!(misses <= 4, "{misses} misses of {faces}");
}

#[test]
fn gear_60_hits_all_but_the_faces_its_last_slot_changed() {
    let (faces, misses) = memo_across("gear-60");
    assert!(faces > 60, "{faces} faces");
    assert!(misses <= 8, "{misses} misses of {faces}");
}
