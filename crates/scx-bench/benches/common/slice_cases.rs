// Written for solidcorex. Apache-2.0.
//! The step-4 slice, end to end, against the pin (research 72 §14.39). It lives with the bench
//! targets, not in the library, because it reads the testkit's recipes (a dev-dependency
//! only, `layers.txt`); `tests/slice.rs` includes it for its unit tests: the T12 recipes built,
//! measured and queried on both sides. Each case is one whole operation on one recipe, and
//! both sides run it on a fresh kernel (the pin) or fresh scratch (scx), so no memo of an
//! earlier run speeds a later one.
//!
//! - `slice/make/<recipe>`: the constructors only (a base part, or one of its tools);
//! - `slice/build/<recipe>`: every part made and every Boolean step run, the pin's
//!   `boolean_*` against `scx_boolean::boolean`;
//! - `slice/parts/<recipe>`: every part made (base and tools), no Boolean; the Booleans' own
//!   cost is `build` less `parts` (the pin consumes its operands and has no copy, so a
//!   Boolean-only case cannot be rerun on it);
//! - `slice/mass/<recipe>`: volume and area at 1e-6 of the built body (`scx_mass::volume` and
//!   `area`; the pin's `brep_volume_default` and `brep_area`, whose B-rep clone is counted);
//! - `slice/mass_full/<recipe>`: scx's `mass` (volume, area, centroid, inertia) against the
//!   pin's volume and area: the pin does less here;
//! - `slice/classify/<recipe>`: [`POINTS`] seeded points through scx's `classify_point` and
//!   the pin's `BrepClassifier`;
//! - `slice/tess/<recipe>`: scx's `tessellate` (faces and edges, chord 1% of the cube root of
//!   the volume) against the pin's `tessellate_boundaries` (edges only; the pin has no
//!   face mesher), and `slice/wire/<recipe>` scx's `wireframe` (edges only) against the same.
//!
//! The setup that builds a case's bodies is not counted. A case whose scx side refuses
//! (a recipe scx does not build yet) is left out.

#![allow(dead_code)]

use scx_bench::Case;
use scx_bodyq::classify_point;
use scx_boolean::{BooleanOp, boolean};
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document};
use scx_num::budget::{Budget, Stop};
use scx_num::fmath::{atan2, hypot};
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2 as XP2;
use scx_num::vector3d::{Point3 as XP3, Vec3 as XV3};
use scx_testkit::csg::box_points;
use scx_testkit::profile::Seg;
use scx_testkit::recipe::{self, Op, Part, Recipe, Shape};
use std::sync::Mutex;

use solidcore::boolean::api_brep::{boolean_difference, boolean_intersection, boolean_union};
use solidcore::curve::curve::Curve;
use solidcore::geom::axis2_placement::Axis2Placement;
use solidcore::math::{Point3, Vec3};
use solidcore::primitive::api_curves::{create_arc, create_line_segment};
use solidcore::primitive::api_primitives as ap;
use solidcore::query::brep::BrepClassifier;
use solidcore::sweep::api_brep::create_linear_sweep;
use solidcore::topology::Brep as PinBrep;
use solidcore::topology::brep::p1::tessellate_boundaries;
use solidcore::topology::brep::p5::transform;
use solidcore::{Kernel, mass::integrator::properties as pin_mass};

/// Seeded query points per classify case.
pub const POINTS: usize = 200;
/// The accuracy of volume and area, both sides (72 §8.1).
const ACCURACY: f64 = 1e-6;
/// The pin's angle tolerance for its edge tessellation, in degrees.
const PIN_ANGLE: f64 = 5.0;
/// The seed of the classify points.
const SEED: u64 = 0x5_11CE;
/// The chord of a tess case, as a fraction of the cube root of the body's volume.
const CHORD: f64 = 0.01;

/// The Boolean recipes (BX1, BO6), the same as `scx-boolean`'s recipe tests run.
pub const BOOLEANS: [&str; 4] = ["box-cyl-hole", "plate-4", "plate-12", "gear-60"];
/// The constructor recipes.
pub const MAKES: [&str; 6] = [
    "block-3x4x5",
    "cylinder-r2-h10",
    "cone-frustum-3-1",
    "sphere-r5",
    "torus-5-1.5",
    "extrude-d",
];

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn recipe_named(name: &str) -> Option<Recipe> {
    recipe::all(Document::default().resolution().zone().mm())
        .into_iter()
        .find(|r| r.name == name)
}

// ---------------------------------------------------------------------------------------
// scx

fn scx_frame(p: &Part) -> Option<Frame> {
    let f = &p.frame;
    Frame::new(
        XP3::new(f.origin[0], f.origin[1], f.origin[2]),
        XV3::new(f.z[0], f.z[1], f.z[2]),
        XV3::new(f.x[0], f.x[1], f.x[2]),
    )
}

fn scx_segment(s: &Seg) -> scx_planar::Segment {
    use scx_planar::Segment;
    match *s {
        Seg::Line { start, end } => Segment::line(XP2::from(start), XP2::from(end)),
        Seg::Arc {
            centre,
            start,
            end,
            ccw,
        } => {
            let sense = if ccw { Sense::Forward } else { Sense::Reversed };
            if start == end {
                let r = hypot(start[0] - centre[0], start[1] - centre[1]);
                Segment::circle(XP2::from(centre), r, sense)
            } else {
                Segment::arc(XP2::from(centre), XP2::from(start), XP2::from(end), sense)
            }
        }
    }
}

/// A part as a body of `doc`, through scx's constructors; `None` where they refuse.
fn scx_part(p: &Part, doc: Document) -> Option<Brep> {
    let f = scx_frame(p)?;
    match &p.shape {
        Shape::Block { size } => scx_make::block(&f, size.map(mm), &doc).ok().map(|m| m.body),
        Shape::Cylinder { radius, height } => {
            scx_make::cylinder(&f, mm(*radius), mm(*height), &doc)
                .ok()
                .map(|m| m.body)
        }
        Shape::Cone { radii, height } => scx_make::cone(&f, radii.map(mm), mm(*height), &doc)
            .ok()
            .map(|m| m.body),
        Shape::Sphere { radius } => scx_make::sphere(&f, mm(*radius), &doc).ok().map(|m| m.body),
        Shape::Torus { major, minor } => scx_make::torus(&f, mm(*major), mm(*minor), &doc)
            .ok()
            .map(|m| m.body),
        Shape::Extrusion {
            outer,
            holes,
            height,
        } => {
            let region = scx_planar::Region::new(
                outer.iter().map(scx_segment).collect(),
                holes
                    .iter()
                    .map(|h| h.iter().map(scx_segment).collect())
                    .collect(),
                doc.resolution(),
            )
            .ok()?;
            let span = scx_make::Span::new(mm(0.0), mm(*height));
            scx_make::extrude(&region, &f, span, &doc)
                .ok()
                .map(|m| m.body)
        }
    }
}

fn scx_op(o: Op) -> BooleanOp {
    match o {
        Op::Union => BooleanOp::Union,
        Op::Intersection => BooleanOp::Intersect,
        Op::Difference => BooleanOp::Subtract,
    }
}

/// One Boolean step: the running body less/with/against `tool`.
fn scx_step(body: &Brep, tool: &Brep, o: Op, doc: Document, scratch: &mut Scratch) -> Option<Brep> {
    let settings = Settings::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, scratch, &stop, &NoTrace);
    boolean(body, tool, scx_op(o), &mut cx).ok().map(|d| d.body)
}

/// The whole recipe in scx: every part, every step.
fn scx_build(r: &Recipe) -> Option<Brep> {
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let mut body = scx_part(&r.base, doc)?;
    for s in &r.steps {
        let tool = scx_part(&s.tool, doc)?;
        body = scx_step(&body, &tool, s.op, doc, &mut scratch)?;
    }
    Some(body)
}

fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------------------
// The pin

fn p3(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}

fn is_world(f: &scx_testkit::analytic::Frame) -> bool {
    f.x == [1.0, 0.0, 0.0] && f.y == [0.0, 1.0, 0.0] && f.z == [0.0, 0.0, 1.0]
}

fn pin_extrusion(
    k: &mut Kernel,
    o: [f64; 3],
    loops: &[&Vec<Seg>],
    height: f64,
) -> solidcore::Result<PinBrep> {
    let at = |q: [f64; 2]| Point3::new(o[0] + q[0], o[1] + q[1], o[2]);
    let mut curves = Vec::new();
    for s in loops.iter().copied().flatten() {
        curves.push(match *s {
            Seg::Line { start, end } => Curve::from(create_line_segment(at(start), at(end))?),
            Seg::Arc {
                centre,
                start,
                end,
                ccw,
            } => {
                let deg = |q: [f64; 2]| atan2(q[1] - centre[1], q[0] - centre[0]).to_degrees();
                let r = hypot(start[0] - centre[0], start[1] - centre[1]);
                let (a, mut b) = if ccw {
                    (deg(start), deg(end))
                } else {
                    (deg(end), deg(start))
                };
                if b <= a {
                    b += 360.0;
                }
                Curve::from(create_arc(at(centre), r, a, b)?)
            }
        });
    }
    let refs: Vec<Option<&Curve>> = curves.iter().map(Some).collect();
    create_linear_sweep(k, &refs, Vec3::new(0.0, 0.0, 1.0), height, true)
}

/// A part in the pin: built at its origin when its frame has the world's axes, else at the
/// world's origin and placed by `transform`.
fn pin_part(k: &mut Kernel, part: &Part) -> Option<PinBrep> {
    let world = is_world(&part.frame);
    let o = if world {
        part.frame.origin
    } else {
        [0.0, 0.0, 0.0]
    };
    let mut b = match &part.shape {
        Shape::Block { size } => ap::create_box(k, p3(o), size[0], size[1], size[2]),
        Shape::Cylinder { radius, height } => ap::create_cylinder(k, p3(o), *radius, *height),
        Shape::Cone { radii, height } => ap::create_cone(k, p3(o), radii[0], radii[1], *height),
        Shape::Sphere { radius } => ap::create_sphere(k, p3(o), *radius),
        Shape::Torus { major, minor } => ap::create_torus(k, p3(o), *major, *minor),
        Shape::Extrusion {
            outer,
            holes,
            height,
        } => {
            let loops: Vec<&Vec<Seg>> = core::iter::once(outer).chain(holes).collect();
            pin_extrusion(k, o, &loops, *height)
        }
    }
    .ok()?;
    if !world {
        let f = &part.frame;
        let place = Axis2Placement::new(p3(f.origin), p3(f.x), p3(f.y));
        transform(k, &mut b, &place, None, false).ok()?;
    }
    Some(b)
}

fn pin_step(k: &mut Kernel, acc: PinBrep, tool: PinBrep, o: Op) -> Option<PinBrep> {
    match o {
        Op::Union => boolean_union(k, acc, tool),
        Op::Intersection => boolean_intersection(k, acc, tool),
        Op::Difference => boolean_difference(k, acc, tool),
    }
    .ok()
}

fn pin_build(r: &Recipe) -> Option<(Kernel, PinBrep)> {
    let mut k = Kernel::default();
    let mut acc = pin_part(&mut k, &r.base)?;
    for s in &r.steps {
        let tool = pin_part(&mut k, &s.tool)?;
        acc = pin_step(&mut k, acc, tool, s.op)?;
    }
    Some((k, acc))
}

// ---------------------------------------------------------------------------------------
// The cases

fn fold<A: 'static, B: 'static>(
    name: &str,
    a: A,
    b: B,
    scx: impl Fn(&A) -> u64 + 'static,
    pin: impl Fn(&B) -> u64 + 'static,
) -> Case {
    Case::fold(
        name,
        (vec![a], vec![b]),
        move |x: &[A]| x.first().map_or(0, &scx),
        move |x: &[B]| x.first().map_or(0, &pin),
    )
}

/// `slice/make/<recipe>`: one constructor.
fn make_case(r: &Recipe) -> Option<Case> {
    let doc = Document::default();
    // Both must be able to build it, else the case is dropped.
    scx_part(&r.base, doc)?;
    pin_part(&mut Kernel::default(), &r.base)?;
    let name = format!("slice/make/{}", r.name);
    Some(fold(
        &name,
        r.base.clone(),
        r.base.clone(),
        move |p| scx_part(p, Document::default()).map_or(0, |b| count(b.faces().count())),
        move |p| pin_part(&mut Kernel::default(), p).map_or(0, |b| count(b.face_list().len())),
    ))
}

/// `slice/build/<recipe>`.
fn build_case(r: &Recipe) -> Option<Case> {
    scx_build(r)?;
    pin_build(r)?;
    Some(fold(
        &format!("slice/build/{}", r.name),
        r.clone(),
        r.clone(),
        |r| scx_build(r).map_or(0, |b| count(b.faces().count())),
        |r| pin_build(r).map_or(0, |(_, b)| count(b.face_list().len())),
    ))
}

/// `slice/parts/<recipe>`: every part of the recipe made (base and tools), no Boolean. The
/// Booleans' own cost is `build` less `parts`.
fn parts_case(r: &Recipe) -> Option<Case> {
    let doc = Document::default();
    let scx = |r: &Recipe| {
        let doc = Document::default();
        let mut n = scx_part(&r.base, doc).map_or(0, |b| count(b.faces().count()));
        for s in &r.steps {
            n += scx_part(&s.tool, doc).map_or(0, |b| count(b.faces().count()));
        }
        n
    };
    let pin = |r: &Recipe| {
        let mut k = Kernel::default();
        let mut n = pin_part(&mut k, &r.base).map_or(0, |b| count(b.face_list().len()));
        for s in &r.steps {
            n += pin_part(&mut k, &s.tool).map_or(0, |b| count(b.face_list().len()));
        }
        n
    };
    scx_part(&r.base, doc)?;
    pin_part(&mut Kernel::default(), &r.base)?;
    Some(fold(
        &format!("slice/parts/{}", r.name),
        r.clone(),
        r.clone(),
        scx,
        pin,
    ))
}

fn estimate(v: Result<scx_mass::Estimate, scx_mass::MassError>) -> f64 {
    v.map_or(f64::NAN, |e| e.value)
}

fn mass_case(full: bool, r: &Recipe, sb: &Brep, pb: PinBrep) -> Option<Case> {
    let acc = scx_mass::RelAccuracy::new(ACCURACY).ok()?;
    let scx_vol_area = move |b: &Brep| {
        let mut scratch = Scratch::new();
        let mut budget = Budget::unlimited();
        let stop = Stop::new();
        let v = estimate(scx_mass::volume(b, acc, &mut scratch, &mut budget, &stop));
        let a = estimate(scx_mass::area(b, acc, &mut scratch, &mut budget, &stop));
        v.to_bits() ^ a.to_bits().rotate_left(13)
    };
    let scx_full = move |b: &Brep| {
        let spec = scx_mass::MassSpec::new(acc);
        let m = scx_mass::mass(
            b,
            &spec,
            &mut Scratch::new(),
            &mut Budget::unlimited(),
            &Stop::new(),
        );
        m.map_or(0, |m| m.mass().to_bits())
    };
    // A fresh kernel and body per run: the pin's calls memoise on both.
    let pin = |b: &Mutex<PinBrep>| {
        let k = Kernel::default();
        let Ok(mut b) = b.lock() else { return 0 };
        let v = pin_mass::brep_volume_default(&k, &mut b, ACCURACY).unwrap_or(f64::NAN);
        let a = pin_mass::brep_area(&k, &mut b, ACCURACY).unwrap_or(f64::NAN);
        v.to_bits() ^ a.to_bits().rotate_left(13)
    };
    let phase = if full { "mass_full" } else { "mass" };
    let name = format!("slice/{phase}/{}", r.name);
    Some(if full {
        fold(&name, sb.clone(), Mutex::new(pb), scx_full, pin)
    } else {
        fold(&name, sb.clone(), Mutex::new(pb), scx_vol_area, pin)
    })
}

/// The truth's box of a recipe, else the box of a block base.
fn bounds(r: &Recipe) -> Option<([f64; 3], [f64; 3])> {
    r.csg().ok()?.bounds()
}

fn classify_case(r: &Recipe, sb: &Brep, pb: PinBrep) -> Option<Case> {
    let (lo, hi) = bounds(r)?;
    let pts = box_points(lo, hi, 0.1, POINTS, SEED);
    let res = Document::default().resolution().zone().mm();
    let scx_pts = pts.clone();
    let scx = move |b: &Brep| {
        let mut sum = 0_u64;
        for p in &scx_pts {
            let c = classify_point(
                b,
                XP3::new(p[0], p[1], p[2]),
                &mut Budget::unlimited(),
                &Stop::new(),
            );
            sum = sum.rotate_left(3)
                ^ c.map_or(9, |c| u64::from(matches!(c, scx_bodyq::PointClass::In(_))));
        }
        sum
    };
    let pin = move |(b, pts): &(PinBrep, Vec<[f64; 3]>)| {
        let k = Kernel::default();
        let mut classifier = BrepClassifier::of_brep(b);
        let mut sum = 0_u64;
        for p in pts {
            let c = classifier.point_3d_classify(&k, p3(*p), res, false);
            sum = sum.rotate_left(3) ^ c.map_or(9, |c| u64::from(c.object().is_some()));
        }
        sum
    };
    Some(fold(
        &format!("slice/classify/{}", r.name),
        sb.clone(),
        (pb, pts),
        scx,
        pin,
    ))
}

fn tess_case(wire_only: bool, r: &Recipe, sb: &Brep, pb: PinBrep) -> Option<Case> {
    let t = r.truth?;
    let chord = (CHORD * scx_num::fmath::cbrt(t.volume)).max(1e-4);
    let spec = scx_tess::TessSpec::new(mm(chord), sb.resolution()).ok()?;
    let full = move |b: &Brep| {
        scx_tess::tessellate(b, &spec, &mut Budget::unlimited(), &Stop::new())
            .map_or(0, |m| count(m.triangles.len()))
    };
    let wire = move |b: &Brep| {
        scx_tess::wireframe(b, &spec, &mut Budget::unlimited(), &Stop::new())
            .map_or(0, |m| count(m.positions.len()))
    };
    let pin = |b: &PinBrep| {
        let k = Kernel::default();
        tessellate_boundaries(&k, b, PIN_ANGLE).map_or(0, |t| count(t.points.len()))
    };
    let phase = if wire_only { "wire" } else { "tess" };
    let name = format!("slice/{phase}/{}", r.name);
    Some(if wire_only {
        fold(&name, sb.clone(), pb, wire, pin)
    } else {
        fold(&name, sb.clone(), pb, full, pin)
    })
}

/// The phases a Boolean recipe has a case for.
const PHASES: [&str; 7] = [
    "build",
    "parts",
    "mass",
    "mass_full",
    "classify",
    "tess",
    "wire",
];

/// The case `name` (`slice/<phase>/<recipe>`), its bodies built; `None` for a name that is
/// not one, or whose recipe either side cannot build.
pub fn case(name: &str) -> Option<Case> {
    let rest = name.strip_prefix("slice/")?;
    let (phase, recipe) = rest.split_once('/')?;
    let r = recipe_named(recipe)?;
    match phase {
        "make" => make_case(&r),
        "build" => build_case(&r),
        "parts" => parts_case(&r),
        _ => {
            let sb = scx_build(&r)?;
            let (_, pb) = pin_build(&r)?;
            match phase {
                "mass" => mass_case(false, &r, &sb, pb),
                "mass_full" => mass_case(true, &r, &sb, pb),
                "classify" => classify_case(&r, &sb, pb),
                "tess" => tess_case(false, &r, &sb, pb),
                "wire" => tess_case(true, &r, &sb, pb),
                _ => None,
            }
        }
    }
}

/// The names of every case: the constructors' `make`, and each phase of each Boolean recipe.
pub fn names() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    out.extend(MAKES.iter().map(|n| format!("slice/make/{n}")));
    for r in BOOLEANS {
        out.extend(PHASES.iter().map(|p| format!("slice/{p}/{r}")));
    }
    out
}

/// Every case of the slice, in [`names`] order; a name either side cannot build is left out.
/// (The instruction-count target builds one case at a time with [`case`].)
pub fn cases() -> Vec<Case> {
    names().iter().filter_map(|n| case(n)).collect()
}

/// The cases whose name contains `filter` (all of them when it is empty): a criterion run
/// of a few cases need not build the others.
pub fn cases_matching(filter: &str) -> Vec<Case> {
    names()
        .iter()
        .filter(|n| n.contains(filter))
        .filter_map(|n| case(n))
        .collect()
}
