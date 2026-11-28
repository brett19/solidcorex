// Written for solidcorex. Apache-2.0.
//! The Boolean algebra suite (BO10, 75 §8.1): the laws a Boolean must keep, asserted on the
//! testkit's placed primitives and on coaxial collars, and judged without the pin.
//!
//! An expression ([`E`]) over a pool of parts is built twice: as bodies, through the public
//! [`boolean`], and as a membership oracle (`scx_testkit::csg`). Two expressions are
//! equivalent ([`equiv`]) when, for each, the body
//! - passes `scx_check::contract` (an empty body excepted),
//! - has the same volume and area (`scx_mass`, to 1e-9 relative plus the Estimates' errors),
//! - has the same count of solid regions and of shells, and
//! - classifies [`POINTS`] seeded points as the oracle of its own expression does wherever
//!   the oracle is certain (outside the band, `2 C` from every boundary).
//!
//! A law is asserted only where every Boolean it uses is built: a case where one answers
//! `Unsupported` is counted and printed (run with `--no-capture`), so the suite measures
//! coverage as well. Any other error is a failure of the case.
//!
//! A case that fails today is listed in [`KNOWN`] with its cause; the sweep skips it and its
//! own `#[ignore = "BO10b: ..."]` test keeps it in the suite for `BO10b` to fix.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines
)]

use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::sync::Mutex;

use scx_bodyq::{PointClass, classify_point};
use scx_boolean::{BooleanError, BooleanOp, boolean};
use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document};
use scx_check::Contacts;
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::hash::mix;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_testkit::analytic::Frame as TFrame;
use scx_testkit::csg::{Csg, Side, box_points};
use scx_testkit::recipe::{Part, Shape, frame_at, frame_turned};

/// Seeded points per judged body.
const POINTS: usize = 1000;
/// The document's coincidence (the default resolution), as the points' band.
const C: f64 = 1e-5;
/// Relative agreement of volumes and areas.
const REL: f64 = 1e-9;

// ---------------------------------------------------------------------------------------
// Parts and expressions

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn frame(f: &TFrame) -> Frame {
    Frame::new(
        Point3::new(f.origin[0], f.origin[1], f.origin[2]),
        Vec3::new(f.z[0], f.z[1], f.z[2]),
        Vec3::new(f.x[0], f.x[1], f.x[2]),
    )
    .unwrap()
}

fn operand(p: &Part, doc: Document) -> Brep {
    let f = frame(&p.frame);
    match &p.shape {
        Shape::Block { size } => {
            scx_testkit::bodies::made::block(&f, size.map(mm), &doc).map(|m| m.body)
        }
        Shape::Cylinder { radius, height } => {
            scx_testkit::bodies::made::cylinder(&f, mm(*radius), mm(*height), &doc).map(|m| m.body)
        }
        Shape::Cone { radii, height } => {
            scx_testkit::bodies::made::cone(&f, radii.map(mm), mm(*height), &doc).map(|m| m.body)
        }
        Shape::Sphere { radius } => {
            scx_testkit::bodies::made::sphere(&f, mm(*radius), &doc).map(|m| m.body)
        }
        Shape::Torus { major, minor } => {
            scx_testkit::bodies::made::torus(&f, mm(*major), mm(*minor), &doc).map(|m| m.body)
        }
        Shape::Extrusion { .. } => panic!("no algebra case extrudes a profile"),
    }
    .unwrap()
}

fn block(o: [f64; 3], size: [f64; 3]) -> Part {
    Part {
        frame: frame_at(o),
        shape: Shape::Block { size },
    }
}

/// A cylinder on the x axis from `x0` to `x1` about `(y, z) = (0, 0)`: the frame's axes are
/// exact (`z` along x), so its faces meet a block's exactly.
fn collar(x0: f64, x1: f64, radius: f64) -> Part {
    Part {
        frame: TFrame {
            origin: [x0, 0.0, 0.0],
            x: [0.0, 1.0, 0.0],
            y: [0.0, 0.0, 1.0],
            z: [1.0, 0.0, 0.0],
        },
        shape: Shape::Cylinder {
            radius,
            height: x1 - x0,
        },
    }
}

/// A Boolean expression over parts.
#[derive(Clone, Debug)]
enum E {
    P(usize),
    U(Box<E>, Box<E>),
    I(Box<E>, Box<E>),
    D(Box<E>, Box<E>),
}

fn p(i: usize) -> E {
    E::P(i)
}
fn u(a: E, b: E) -> E {
    E::U(Box::new(a), Box::new(b))
}
fn i(a: E, b: E) -> E {
    E::I(Box::new(a), Box::new(b))
}
fn d(a: E, b: E) -> E {
    E::D(Box::new(a), Box::new(b))
}

/// What building an expression came to.
enum Built {
    Body(Brep),
    /// A Boolean answered `Unsupported`, by name.
    Unsupported(&'static str),
    /// Any other error.
    Failed(String),
}

fn run_op(a: &Brep, b: &Brep, op: BooleanOp) -> Result<Brep, BooleanError> {
    let doc = Document::default();
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    boolean(a, b, op, &mut cx).map(|d| d.body)
}

fn eval(parts: &[Part], e: &E) -> Built {
    let (a, b, op) = match e {
        E::P(k) => return Built::Body(operand(&parts[*k], Document::default())),
        E::U(a, b) => (a, b, BooleanOp::Union),
        E::I(a, b) => (a, b, BooleanOp::Intersect),
        E::D(a, b) => (a, b, BooleanOp::Subtract),
    };
    let (x, y) = match (eval(parts, a), eval(parts, b)) {
        (Built::Body(x), Built::Body(y)) => (x, y),
        (Built::Failed(m), _) | (_, Built::Failed(m)) => return Built::Failed(m),
        (Built::Unsupported(n), _) | (_, Built::Unsupported(n)) => return Built::Unsupported(n),
    };
    match run_op(&x, &y, op) {
        Ok(body) => Built::Body(body),
        Err(BooleanError::Unsupported(_)) => {
            Built::Unsupported(run_op(&x, &y, op).err().map_or("", |e| e.name()))
        }
        Err(e) => Built::Failed(format!("{op:?}: {e:?}")),
    }
}

fn csg(parts: &[Part], e: &E) -> Csg {
    match e {
        E::P(k) => parts[*k].csg().unwrap(),
        E::U(a, b) => csg(parts, a).union(csg(parts, b)),
        E::I(a, b) => csg(parts, a).intersection(csg(parts, b)),
        E::D(a, b) => csg(parts, a).difference(csg(parts, b)),
    }
}

// ---------------------------------------------------------------------------------------
// Judging a body

/// What a judged body is, to compare with another.
#[derive(Debug, Clone, Copy)]
struct Measure {
    volume: f64,
    volume_err: f64,
    area: f64,
    area_err: f64,
    regions: usize,
    shells: usize,
}

fn estimate(
    b: &Brep,
    f: fn(
        &Brep,
        scx_mass::RelAccuracy,
        &mut Scratch,
        &mut Budget,
        &Stop,
    ) -> Result<scx_mass::Estimate, scx_mass::MassError>,
) -> Result<scx_mass::Estimate, String> {
    let a = scx_mass::RelAccuracy::new(REL).unwrap();
    f(
        b,
        a,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .map_err(|e| format!("mass: {e:?}"))
}

/// Judge `body` against the oracle `tree` alone: the contract, the measure and the points.
fn judge(body: &Brep, tree: &Csg, seed: u64) -> Result<Measure, String> {
    let doc = Document::default();
    let empty = body.faces().len() == 0;
    let (regions, shells) = {
        let solid: Vec<_> = body.regions().filter(|r| r.is_solid()).collect();
        (
            solid.len(),
            solid.iter().map(|r| r.shells().count()).sum::<usize>(),
        )
    };
    let m = if empty {
        Measure {
            volume: 0.0,
            volume_err: 0.0,
            area: 0.0,
            area_err: 0.0,
            regions,
            shells,
        }
    } else {
        let settings = Settings::new();
        let mut scratch = Scratch::new();
        let stop = Stop::new();
        let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
        scx_check::contract(body, &cx, Contacts::Radial).map_err(|e| format!("contract: {e:?}"))?;
        let v = estimate(body, scx_mass::volume)?;
        let a = estimate(body, scx_mass::area)?;
        Measure {
            volume: v.value,
            volume_err: v.error,
            area: a.value,
            area_err: a.error,
            regions,
            shells,
        }
    };
    if let Some((lo, hi)) = tree.bounds() {
        let (mut wrong, mut asked) = (None, 0);
        for q in box_points(lo, hi, 0.1, POINTS, seed) {
            let truth = tree.side(q, 2.0 * C);
            if truth == Side::Band {
                continue;
            }
            asked += 1;
            let solid = if empty {
                false
            } else {
                match classify_point(
                    body,
                    Point3::new(q[0], q[1], q[2]),
                    &mut Budget::unlimited(),
                    &Stop::new(),
                ) {
                    Ok(PointClass::In(r)) => body.region(r).is_some_and(scx_brep::Region::is_solid),
                    // Within coincidence of the result's boundary, where the oracle is
                    // certain 2 C clear of its own: a different boundary.
                    Ok(PointClass::On(_)) => {
                        wrong.get_or_insert_with(|| {
                            format!("{q:?}: on the result, oracle {truth:?}")
                        });
                        continue;
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        wrong.get_or_insert_with(|| format!("{q:?}: classify {e}"));
                        continue;
                    }
                }
            };
            if solid != (truth == Side::In) {
                wrong.get_or_insert_with(|| format!("{q:?}: body solid {solid}, oracle {truth:?}"));
            }
        }
        if let Some(w) = wrong {
            return Err(format!("points ({asked} certain): {w}"));
        }
    }
    Ok(m)
}

fn agree(a: f64, ea: f64, b: f64, eb: f64) -> bool {
    (a - b).abs() <= REL * a.abs().max(b.abs()) + ea + eb
}

fn same(a: &Measure, b: &Measure) -> Result<(), String> {
    if !agree(a.volume, a.volume_err, b.volume, b.volume_err) {
        return Err(format!("volume {} vs {}", a.volume, b.volume));
    }
    if !agree(a.area, a.area_err, b.area, b.area_err) {
        return Err(format!("area {} vs {}", a.area, b.area));
    }
    if (a.regions, a.shells) != (b.regions, b.shells) {
        return Err(format!(
            "regions/shells {:?} vs {:?}",
            (a.regions, a.shells),
            (b.regions, b.shells)
        ));
    }
    Ok(())
}

/// One case's result.
#[derive(Debug)]
enum Verdict {
    Pass,
    Skip(&'static str),
    Fail(String),
}

/// Build `e` and judge it against its own oracle.
fn built(parts: &[Part], e: &E, seed: u64) -> Result<Measure, Verdict> {
    match eval(parts, e) {
        Built::Body(b) => {
            judge(&b, &csg(parts, e), seed).map_err(|m| Verdict::Fail(format!("{e:?}: {m}")))
        }
        Built::Unsupported(n) => Err(Verdict::Skip(n)),
        Built::Failed(m) => Err(Verdict::Fail(format!("{e:?}: {m}"))),
    }
}

/// `l` and `r` are equivalent.
fn equiv(parts: &[Part], l: &E, r: &E, seed: u64) -> Verdict {
    let (ml, mr) = match (built(parts, l, seed), built(parts, r, mix(&[seed, 1]))) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(Verdict::Fail(m)), _) | (_, Err(Verdict::Fail(m))) => return Verdict::Fail(m),
        (Err(v), _) | (_, Err(v)) => return v,
    };
    match same(&ml, &mr) {
        Ok(()) => Verdict::Pass,
        Err(m) => Verdict::Fail(format!("{l:?} vs {r:?}: {m}")),
    }
}

// ---------------------------------------------------------------------------------------
// The pool

/// The pool: general position (no two faces coplanar or tangent) first, then the pairs
/// that share a plane. `GENERAL` is the count of the first kind.
fn pool() -> (Vec<(&'static str, Part)>, usize) {
    let mut v: Vec<(&'static str, Part)> = vec![
        ("boxA", block([0.0; 3], [10.0; 3])),
        ("boxB", block([5.0, 5.0, 5.0], [10.0; 3])),
        ("boxC", block([3.0, -2.0, 4.0], [8.0, 6.0, 5.0])),
        (
            "cyl",
            Part {
                frame: frame_at([5.0, 5.0, -1.0]),
                shape: Shape::Cylinder {
                    radius: 3.0,
                    height: 12.0,
                },
            },
        ),
        (
            "sphere",
            Part {
                frame: frame_at([11.5, 2.3, 6.2]),
                shape: Shape::Sphere { radius: 4.0 },
            },
        ),
        (
            "turned",
            Part {
                frame: frame_turned([2.0, 2.0, 2.0], [1.0, 1.0, 1.0], 0.5),
                shape: Shape::Block { size: [7.0; 3] },
            },
        ),
        (
            "cone",
            Part {
                frame: frame_at([6.0, 4.0, 1.5]),
                shape: Shape::Cone {
                    radii: [4.0, 1.0],
                    height: 9.0,
                },
            },
        ),
        (
            "torus",
            Part {
                frame: frame_at([5.0, 5.0, 5.0]),
                shape: Shape::Torus {
                    major: 5.0,
                    minor: 1.5,
                },
            },
        ),
    ];
    let general = v.len();
    // Shares A's z = 0 and z = 10 planes.
    v.push(("flush", block([5.0, 5.0, 0.0], [10.0; 3])));
    // Shares A's x = 10 face.
    v.push(("abut", block([10.0, 0.0, 0.0], [5.0, 10.0, 10.0])));
    // A sphere whose centre is on A's x = 10 plane: a great circle in a face, a real contact.
    v.push((
        "sphereOnFace",
        Part {
            frame: frame_at([10.0, 2.0, 6.0]),
            shape: Shape::Sphere { radius: 4.0 },
        },
    ));
    (v, general)
}

/// The parts, twice over: index `k + n` is a second body built the same as `k`.
fn doubled(pool: &[(&'static str, Part)]) -> Vec<Part> {
    pool.iter()
        .chain(pool.iter())
        .map(|(_, part)| part.clone())
        .collect()
}

// ---------------------------------------------------------------------------------------
// The sweep

/// Cases known to fail today: `(cause, law, label)`. The sweeps skip them; each cause has
/// an ignored test (`known_*`) that runs its cases and asserts they pass.
const KNOWN: &[(&str, &str, &str)] = &[
    ("build", "absorption-intersection", "torus/boxB"),
    ("loops", "absorption-intersection", "cone/boxC"),
    ("loops", "absorption-intersection", "sphereOnFace/sphere"),
    ("loops", "absorption-union", "cone/boxC"),
    ("loops", "absorption-union", "sphere/abut"),
    ("loops", "absorption-union", "sphereOnFace/sphere"),
    ("loops", "distributivity", "cone/boxA/boxC"),
    ("loops", "distributivity", "cone/boxC/turned"),
    ("loops", "distributivity", "cyl/boxA/turned"),
    ("loops", "distributivity", "cyl/boxB/turned"),
    ("loops", "distributivity", "cyl/boxC/turned"),
    ("loops", "partition", "cone/boxC"),
    ("loops", "partition", "cyl/boxB"),
    ("loops", "partition", "cyl/flush"),
    ("loops", "partition", "sphere/abut"),
    ("loops", "partition", "sphereOnFace/sphere"),
    ("loops", "partition", "torus/flush"),
    ("loosespan", "absorption-union", "cyl/boxB"),
    ("loosespan", "absorption-union", "torus/boxB"),
    ("loosespan", "absorption-union", "torus/flush"),
    ("loosespan", "associativity-union", "boxB/cyl/torus"),
    ("loosespan", "distributivity", "torus/boxB/cyl"),
    ("loosespan", "rigid-motion", "difference boxB/torus"),
    ("loosespan", "rigid-motion", "difference torus/boxB"),
    ("loosespan", "rigid-motion", "intersection boxB/torus"),
    ("loosespan", "rigid-motion", "union boxB/torus"),
    ("piece", "partition", "boxA/cone"),
    ("uvundecided", "absorption-intersection", "boxB/torus"),
    ("uvundecided", "absorption-intersection", "sphere/abut"),
    ("uvundecided", "distributivity", "boxA/boxB/cyl"),
    ("uvundecided", "distributivity", "boxB/boxA/cyl"),
    ("uvundecided", "distributivity", "cyl/boxA/boxB"),
    ("uvundecided", "partition", "torus/boxB"),
];

fn known_cause(c: &Case) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|&&(_, l, n)| l == c.law && n == c.label)
        .map(|&(cause, ..)| cause)
}

/// Run the known cases of `cause`: every one must pass (fails until `BO10b` fixes it).
fn run_cause(cause: &'static str) {
    let cases: Vec<Case> = ALL_LAWS
        .iter()
        .flat_map(|cases| cases())
        .filter(|c| known_cause(c) == Some(cause))
        .collect();
    sweep_all(cause, &cases);
}

struct Case {
    law: &'static str,
    label: String,
    run: Box<dyn Fn() -> Verdict + Send + Sync>,
}

fn case(
    law: &'static str,
    label: String,
    run: impl Fn() -> Verdict + Send + Sync + 'static,
) -> Case {
    Case {
        law,
        label,
        run: Box::new(run),
    }
}

/// Run the cases (known failures excepted) over every core, print the coverage, and assert
/// that none failed.
fn sweep(law: &str, cases: Vec<Case>) {
    let cases: Vec<Case> = cases
        .into_iter()
        .filter(|c| known_cause(c).is_none())
        .collect();
    sweep_all(law, &cases);
}

/// [`sweep`] over exactly `cases`.
fn sweep_all(law: &str, cases: &[Case]) {
    let next = Mutex::new(0usize);
    let out: Mutex<Vec<(usize, Verdict)>> = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism()
        .map_or(2, usize::from)
        .min(8);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let k = {
                        let mut n = next.lock().unwrap();
                        let k = *n;
                        *n += 1;
                        k
                    };
                    let Some(c) = cases.get(k) else { break };
                    let v = (c.run)();
                    out.lock().unwrap().push((k, v));
                }
            });
        }
    });
    let mut out = out.into_inner().unwrap();
    out.sort_by_key(|&(k, _)| k);
    let (mut pass, mut skips, mut fails) = (0, BTreeMap::<&str, usize>::new(), Vec::new());
    for (k, v) in out {
        match v {
            Verdict::Pass => pass += 1,
            Verdict::Skip(n) => *skips.entry(n).or_default() += 1,
            Verdict::Fail(m) => fails.push(format!("{} {}: {m}", cases[k].law, cases[k].label)),
        }
    }
    println!(
        "coverage {law}: {pass} asserted, {} unsupported {skips:?}, {} failed",
        skips.values().sum::<usize>(),
        fails.len()
    );
    assert!(
        fails.is_empty(),
        "{} failed:\n{}",
        fails.len(),
        fails.join("\n")
    );
}

fn pairs(n: usize) -> Vec<(usize, usize)> {
    (0..n)
        .flat_map(|a| (a + 1..n).map(move |b| (a, b)))
        .collect()
}

fn ordered(n: usize) -> Vec<(usize, usize)> {
    (0..n)
        .flat_map(|a| (0..n).filter(move |&b| a != b).map(move |b| (a, b)))
        .collect()
}

fn triples(n: usize) -> Vec<(usize, usize, usize)> {
    pairs(n)
        .into_iter()
        .flat_map(|(a, b)| (b + 1..n).map(move |c| (a, b, c)))
        .collect()
}

fn seed_of(label: &str) -> u64 {
    mix(&[
        label.len() as u64,
        label.bytes().map(u64::from).sum::<u64>(),
        0xb010,
    ])
}

/// A law over the ordered or unordered pairs of the whole pool.
fn pair_law(
    law: &'static str,
    ordered_pairs: bool,
    mk: impl Fn(usize, usize) -> (E, E) + Copy + Send + Sync + 'static,
) -> Vec<Case> {
    let (pool, _) = pool();
    let names: Vec<&str> = pool.iter().map(|&(n, _)| n).collect();
    let parts = std::sync::Arc::new(doubled(&pool));
    let list = if ordered_pairs {
        ordered(pool.len())
    } else {
        pairs(pool.len())
    };
    list.into_iter()
        .map(|(a, b)| {
            let parts = parts.clone();
            let label = format!("{}/{}", names[a], names[b]);
            let seed = seed_of(&label);
            case(law, label, move || {
                let (l, r) = mk(a, b);
                equiv(&parts, &l, &r, seed)
            })
        })
        .collect()
}

fn triple_law(
    law: &'static str,
    mk: impl Fn(usize, usize, usize) -> (E, E) + Copy + Send + Sync + 'static,
    permute: bool,
) -> Vec<Case> {
    let (pool, general) = pool();
    let names: Vec<&str> = pool.iter().map(|&(n, _)| n).collect();
    let parts = std::sync::Arc::new(doubled(&pool));
    let mut out = Vec::new();
    for (a, b, c) in triples(general) {
        let rolls: &[(usize, usize, usize)] = if permute {
            &[(a, b, c), (b, a, c), (c, a, b)]
        } else {
            &[(a, b, c)]
        };
        for &(x, y, z) in rolls {
            let parts = parts.clone();
            let label = format!("{}/{}/{}", names[x], names[y], names[z]);
            let seed = seed_of(&label);
            out.push(case(law, label, move || {
                let (l, r) = mk(x, y, z);
                equiv(&parts, &l, &r, seed)
            }));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// The laws

fn cases_commutativity_of_union() -> Vec<Case> {
    pair_law("commutativity-union", false, |a, b| {
        (u(p(a), p(b)), u(p(b), p(a)))
    })
}

#[test]
fn commutativity_of_union() {
    sweep("commutativity-union", cases_commutativity_of_union());
}

fn cases_commutativity_of_intersection() -> Vec<Case> {
    pair_law("commutativity-intersection", false, |a, b| {
        (i(p(a), p(b)), i(p(b), p(a)))
    })
}

#[test]
fn commutativity_of_intersection() {
    sweep(
        "commutativity-intersection",
        cases_commutativity_of_intersection(),
    );
}

fn cases_idempotence_of_union_and_intersection() -> Vec<Case> {
    let (pool, _) = pool();
    let n = pool.len();
    let names: Vec<&str> = pool.iter().map(|&(n, _)| n).collect();
    let parts = std::sync::Arc::new(doubled(&pool));
    let mut cases = Vec::new();
    for (k, name) in names.iter().enumerate() {
        for (law, which) in [
            ("idempotence-union", true),
            ("idempotence-intersection", false),
        ] {
            let parts = parts.clone();
            let label = (*name).to_string();
            let seed = seed_of(&label);
            cases.push(case(law, label, move || {
                let l = if which {
                    u(p(k), p(k + n))
                } else {
                    i(p(k), p(k + n))
                };
                equiv(&parts, &l, &p(k), seed)
            }));
        }
    }
    cases
}

#[test]
fn idempotence_of_union_and_intersection() {
    sweep("idempotence", cases_idempotence_of_union_and_intersection());
}

fn cases_self_difference_is_empty() -> Vec<Case> {
    let (pool, _) = pool();
    let n = pool.len();
    let parts = std::sync::Arc::new(doubled(&pool));
    (0..n)
        .map(|k| {
            let parts = parts.clone();
            let label = pool[k].0.to_string();
            case("self-difference", label, move || {
                match eval(&parts, &d(p(k), p(k + n))) {
                    Built::Body(b) => {
                        let solid = b.regions().filter(|r| r.is_solid()).count();
                        if solid == 0 && b.faces().len() == 0 {
                            Verdict::Pass
                        } else {
                            Verdict::Fail(format!(
                                "{solid} solid regions, {} faces",
                                b.faces().len()
                            ))
                        }
                    }
                    Built::Unsupported(n) => Verdict::Skip(n),
                    Built::Failed(m) => Verdict::Fail(m),
                }
            })
        })
        .collect()
}

#[test]
fn self_difference_is_empty() {
    sweep("self-difference", cases_self_difference_is_empty());
}

fn cases_absorption() -> Vec<Case> {
    let mut cases = pair_law("absorption-union", true, |a, b| {
        (u(p(a), i(p(a), p(b))), p(a))
    });
    cases.extend(pair_law("absorption-intersection", true, |a, b| {
        (i(p(a), u(p(a), p(b))), p(a))
    }));
    cases
}

#[test]
fn absorption() {
    sweep("absorption", cases_absorption());
}

fn cases_partition_as_a_union() -> Vec<Case> {
    pair_law("partition", true, |a, b| {
        (u(d(p(a), p(b)), i(p(a), p(b))), p(a))
    })
}

#[test]
fn partition_as_a_union() {
    sweep("partition", cases_partition_as_a_union());
}

/// A volume relation: `parts of e` sum to `want` over the bodies of `terms`.
fn volume_of(parts: &[Part], e: &E, seed: u64) -> Result<Measure, Verdict> {
    built(parts, e, seed)
}

fn cases_partition_and_inclusion_exclusion_in_volume() -> Vec<Case> {
    let (pool, _) = pool();
    let names: Vec<&str> = pool.iter().map(|&(n, _)| n).collect();
    let parts = std::sync::Arc::new(doubled(&pool));
    let mut cases = Vec::new();
    for (a, b) in ordered(pool.len()) {
        let parts = parts.clone();
        let label = format!("{}/{}", names[a], names[b]);
        let seed = seed_of(&label);
        cases.push(case("volume-relations", label, move || {
            // vol(A - B) + vol(A and B) = vol(A); vol(A or B) = vol(A) + vol(B) - vol(A and B).
            let ms: Vec<Measure> = match [p(a), p(b), d(p(a), p(b)), i(p(a), p(b)), u(p(a), p(b))]
                .iter()
                .map(|e| volume_of(&parts, e, seed))
                .collect::<Result<_, _>>()
            {
                Ok(v) => v,
                Err(v) => return v,
            };
            let [va, vb, vd, vi, vu] = [0, 1, 2, 3, 4].map(|k| &ms[k]);
            let err = vd.volume_err + vi.volume_err + va.volume_err;
            if (vd.volume + vi.volume - va.volume).abs() > REL * va.volume + err {
                return Verdict::Fail(format!(
                    "partition: {} + {} != {}",
                    vd.volume, vi.volume, va.volume
                ));
            }
            let err = vu.volume_err + va.volume_err + vb.volume_err + vi.volume_err;
            let want = va.volume + vb.volume - vi.volume;
            if (vu.volume - want).abs() > REL * want + err {
                return Verdict::Fail(format!("inclusion-exclusion: {} != {want}", vu.volume));
            }
            Verdict::Pass
        }));
    }
    cases
}

#[test]
fn partition_and_inclusion_exclusion_in_volume() {
    sweep(
        "volume-relations",
        cases_partition_and_inclusion_exclusion_in_volume(),
    );
}

/// A box about every pool member: a body to take complements in.
fn with_box(mut parts: Vec<Part>) -> (Vec<Part>, usize) {
    parts.push(block([-5.0; 3], [30.0; 3]));
    let x = parts.len() - 1;
    (parts, x)
}

fn cases_de_morgan_in_a_box() -> Vec<Case> {
    let (pool, _) = pool();
    let names: Vec<&str> = pool.iter().map(|&(n, _)| n).collect();
    let (parts, x) = with_box(doubled(&pool));
    let parts = std::sync::Arc::new(parts);
    let mut cases = Vec::new();
    for (a, b) in pairs(pool.len()) {
        let parts = parts.clone();
        let label = format!("{}/{}", names[a], names[b]);
        let seed = seed_of(&label);
        cases.push(case("de-morgan", label, move || {
            let v = equiv(
                &parts,
                &d(p(x), u(p(a), p(b))),
                &i(d(p(x), p(a)), d(p(x), p(b))),
                seed,
            );
            if !matches!(v, Verdict::Pass) {
                return v;
            }
            equiv(
                &parts,
                &d(p(x), i(p(a), p(b))),
                &u(d(p(x), p(a)), d(p(x), p(b))),
                mix(&[seed, 2]),
            )
        }));
    }
    cases
}

#[test]
fn de_morgan_in_a_box() {
    sweep("de-morgan", cases_de_morgan_in_a_box());
}

fn cases_associativity_of_union() -> Vec<Case> {
    triple_law(
        "associativity-union",
        |a, b, c| (u(u(p(a), p(b)), p(c)), u(p(a), u(p(b), p(c)))),
        false,
    )
}

#[test]
fn associativity_of_union() {
    sweep("associativity-union", cases_associativity_of_union());
}

fn cases_associativity_of_intersection() -> Vec<Case> {
    triple_law(
        "associativity-intersection",
        |a, b, c| (i(i(p(a), p(b)), p(c)), i(p(a), i(p(b), p(c)))),
        false,
    )
}

#[test]
fn associativity_of_intersection() {
    sweep(
        "associativity-intersection",
        cases_associativity_of_intersection(),
    );
}

fn cases_distributivity_of_intersection_over_union() -> Vec<Case> {
    triple_law(
        "distributivity",
        |a, b, c| (i(p(a), u(p(b), p(c))), u(i(p(a), p(b)), i(p(a), p(c)))),
        true,
    )
}

#[test]
fn distributivity_of_intersection_over_union() {
    sweep(
        "distributivity",
        cases_distributivity_of_intersection_over_union(),
    );
}

// ---------------------------------------------------------------------------------------
// Coaxial collars (the radial order at a shared circular edge, R2; solidcraft#20408)

/// Two collars of radius `r` on the x axis, each on a block: `A` has its block over
/// `x in [-6, 0]` and its collar over `[0, a1]`; `B` has its collar over `[b0, 8]` and its
/// block over `[8, 14]`. The collars overlap over `a1 - b0` (`0` when they meet in a ring).
/// With `blocks` false the operands are the bare collars. Parts: 0 and 1 the blocks, 2 and 3
/// the collars of A and B.
struct Collars {
    parts: Vec<Part>,
    a: E,
    b: E,
    /// The overlap length.
    ell: f64,
    r: f64,
}

fn collars(a1: f64, b0: f64, blocks: bool) -> Collars {
    let r = 1.3;
    let parts = vec![
        block([-6.0, -3.0, -3.0], [6.0, 6.0, 6.0]),
        block([8.0, -3.0, -3.0], [6.0, 6.0, 6.0]),
        collar(0.0, a1, r),
        collar(b0, 8.0, r),
    ];
    let (a, b) = if blocks {
        (u(p(0), p(2)), u(p(1), p(3)))
    } else {
        (p(2), p(3))
    };
    Collars {
        parts,
        a,
        b,
        ell: a1 - b0,
        r,
    }
}

fn collar_cases() -> Vec<(&'static str, Collars)> {
    vec![
        ("bare overlap 2", collars(5.0, 3.0, false)),
        ("bare ring", collars(4.0, 4.0, false)),
        ("blocks overlap 2", collars(5.0, 3.0, true)),
        ("blocks ring", collars(4.0, 4.0, true)),
    ]
}

/// One collar case: both orders of union, intersection and the differences, each judged
/// against the oracle, with `vol(A or B) = vol(A) + vol(B) - pi r^2 l`.
fn collar_case(c: &Collars, seed: u64) -> Verdict {
    let (a, b) = (c.a.clone(), c.b.clone());
    let v = equiv(
        &c.parts,
        &u(a.clone(), b.clone()),
        &u(b.clone(), a.clone()),
        seed,
    );
    if !matches!(v, Verdict::Pass) {
        return v;
    }
    let v = equiv(
        &c.parts,
        &i(a.clone(), b.clone()),
        &i(b.clone(), a.clone()),
        mix(&[seed, 1]),
    );
    if !matches!(v, Verdict::Pass) {
        return v;
    }
    let tube = PI * c.r * c.r * c.ell;
    // The operands' own volumes by measure, and the union's by the closed form.
    let m = |e: &E, s: u64| built(&c.parts, e, s);
    let (ma, mb, mu, mi) = match (
        m(&a, mix(&[seed, 2])),
        m(&b, mix(&[seed, 3])),
        m(&u(a.clone(), b.clone()), mix(&[seed, 4])),
        m(&i(a.clone(), b.clone()), mix(&[seed, 5])),
    ) {
        (Ok(x), Ok(y), Ok(z), Ok(w)) => (x, y, z, w),
        (Err(v), ..) | (_, Err(v), ..) | (_, _, Err(v), _) | (.., Err(v)) => return v,
    };
    let want = ma.volume + mb.volume - tube;
    let err = ma.volume_err + mb.volume_err + mu.volume_err;
    if (mu.volume - want).abs() > REL * want + err {
        return Verdict::Fail(format!(
            "union volume {} != {want} (overlap {tube})",
            mu.volume
        ));
    }
    if (mi.volume - tube).abs() > REL * tube.max(1.0) + mi.volume_err {
        return Verdict::Fail(format!("intersection volume {} != {tube}", mi.volume));
    }
    for (x, y, vx) in [(&a, &b, ma.volume), (&b, &a, mb.volume)] {
        let Ok(md) = m(&d(x.clone(), y.clone()), mix(&[seed, 6])) else {
            return Verdict::Fail("difference not built".into());
        };
        if (md.volume - (vx - tube)).abs() > REL * vx + md.volume_err + 1e-9 {
            return Verdict::Fail(format!("difference volume {} != {}", md.volume, vx - tube));
        }
    }
    // The union is one solid region of one shell.
    if (mu.regions, mu.shells) != (1, 1) {
        return Verdict::Fail(format!(
            "union regions/shells {:?}",
            (mu.regions, mu.shells)
        ));
    }
    Verdict::Pass
}

fn cases_collars_in_both_operand_orders() -> Vec<Case> {
    collar_cases()
        .into_iter()
        .map(|(name, c)| {
            let seed = seed_of(name);
            case("collars", name.to_string(), move || collar_case(&c, seed))
        })
        .collect()
}

#[test]
fn collars_in_both_operand_orders() {
    sweep("collars", cases_collars_in_both_operand_orders());
}

/// solidcraft#20408 as its geometry: a hoop and a back, each a swept part ending in a collar
/// of radius 1.3 mm, the two collars coaxial and overlapping over exactly 2 mm; the union
/// is one solid of the operands' volumes less `pi 1.3^2 2`. (The pin's two records are
/// DT4's, once they can be read.)
fn cases_issue_20408_collars_union() -> Vec<Case> {
    collar_cases()
        .into_iter()
        .filter(|(n, _)| n.contains("blocks overlap"))
        .map(|(name, c)| {
            let seed = seed_of(name);
            case("issue-20408", name.to_string(), move || {
                collar_case(&c, seed)
            })
        })
        .collect()
}

#[test]
fn issue_20408_collars_union() {
    sweep("issue-20408", cases_issue_20408_collars_union());
}

// ---------------------------------------------------------------------------------------
// Rigid motion and determinism

/// The rotation by `angle` about `axis` of `v`, and the part moved by it and `t`.
fn moved(part: &Part, t: [f64; 3]) -> Part {
    let axis = [1.0, 2.0, 3.0];
    let n = (14.0_f64).sqrt();
    let k = axis.map(|v| v / n);
    let (s, c) = scx_num::fmath::sin_cos(0.7);
    let rot = |v: [f64; 3]| -> [f64; 3] {
        let kv = k[0] * v[0] + k[1] * v[1] + k[2] * v[2];
        let kxv = [
            k[1] * v[2] - k[2] * v[1],
            k[2] * v[0] - k[0] * v[2],
            k[0] * v[1] - k[1] * v[0],
        ];
        [0, 1, 2].map(|j| v[j] * c + kxv[j] * s + k[j] * kv * (1.0 - c))
    };
    let o = rot(part.frame.origin);
    Part {
        frame: TFrame {
            origin: [o[0] + t[0], o[1] + t[1], o[2] + t[2]],
            x: rot(part.frame.x),
            y: rot(part.frame.y),
            z: rot(part.frame.z),
        },
        shape: part.shape.clone(),
    }
}

fn cases_rigid_motion() -> Vec<Case> {
    let (pool, general) = pool();
    let names: Vec<&str> = pool.iter().map(|&(n, _)| n).collect();
    let plain = std::sync::Arc::new(doubled(&pool));
    let t = [1.0e6, -1.0e6, 1.0e6];
    let far = std::sync::Arc::new(plain.iter().map(|q| moved(q, t)).collect::<Vec<_>>());
    let mut cases = Vec::new();
    for (a, b) in ordered(general) {
        for (law, which) in [("union", 0), ("intersection", 1), ("difference", 2)] {
            if which < 2 && a > b {
                continue;
            }
            let (plain, far) = (plain.clone(), far.clone());
            let label = format!("{law} {}/{}", names[a], names[b]);
            let seed = seed_of(&label);
            cases.push(case("rigid-motion", label, move || {
                let e = match which {
                    0 => u(p(a), p(b)),
                    1 => i(p(a), p(b)),
                    _ => d(p(a), p(b)),
                };
                let (m0, m1) = match (built(&plain, &e, seed), built(&far, &e, seed)) {
                    (Ok(x), Ok(y)) => (x, y),
                    (Err(Verdict::Fail(m)), _) | (_, Err(Verdict::Fail(m))) => {
                        return Verdict::Fail(m);
                    }
                    (Err(v), _) | (_, Err(v)) => return v,
                };
                match same(&m0, &m1) {
                    Ok(()) => Verdict::Pass,
                    Err(m) => Verdict::Fail(format!("moved: {m}")),
                }
            }));
        }
    }
    cases
}

#[test]
fn rigid_motion() {
    sweep("rigid-motion", cases_rigid_motion());
}

fn cases_determinism_of_encodings() -> Vec<Case> {
    let (pool, _) = pool();
    let names: Vec<&str> = pool.iter().map(|&(n, _)| n).collect();
    let parts = std::sync::Arc::new(doubled(&pool));
    let mut cases = Vec::new();
    for (a, b) in pairs(pool.len()) {
        for (law, which) in [("union", 0), ("intersection", 1), ("difference", 2)] {
            let parts = parts.clone();
            let label = format!("{law} {}/{}", names[a], names[b]);
            cases.push(case("determinism", label, move || {
                let e = match which {
                    0 => u(p(a), p(b)),
                    1 => i(p(a), p(b)),
                    _ => d(p(a), p(b)),
                };
                let enc = |e: &E| match eval(&parts, e) {
                    Built::Body(b) => Ok(scx_record::encode(&b).map_err(|e| format!("{e:?}"))),
                    Built::Unsupported(n) => Err(Verdict::Skip(n)),
                    Built::Failed(m) => Err(Verdict::Fail(m)),
                };
                match (enc(&e), enc(&e)) {
                    (Ok(x), Ok(y)) => match (x, y) {
                        (Ok(x), Ok(y)) if x == y => Verdict::Pass,
                        (Ok(_), Ok(_)) => Verdict::Fail("encodings differ between two runs".into()),
                        (Err(m), _) | (_, Err(m)) => Verdict::Fail(m),
                    },
                    (Err(v), _) | (_, Err(v)) => v,
                }
            }));
        }
    }
    cases
}

#[test]
fn determinism_of_encodings() {
    sweep("determinism", cases_determinism_of_encodings());
}

// ---------------------------------------------------------------------------------------
// Known failures, for BO10b

/// Every law's test, to collect their cases.
const ALL_LAWS: &[fn() -> Vec<Case>] = &[
    cases_commutativity_of_union,
    cases_commutativity_of_intersection,
    cases_idempotence_of_union_and_intersection,
    cases_self_difference_is_empty,
    cases_absorption,
    cases_partition_as_a_union,
    cases_partition_and_inclusion_exclusion_in_volume,
    cases_de_morgan_in_a_box,
    cases_associativity_of_union,
    cases_associativity_of_intersection,
    cases_distributivity_of_intersection_over_union,
    cases_collars_in_both_operand_orders,
    cases_issue_20408_collars_union,
    cases_rigid_motion,
    cases_determinism_of_encodings,
];

#[test]
#[ignore = "BO10b: Defect(Build) on a torus against a box's union with it"]
fn known_build() {
    run_cause("build");
}

#[test]
#[ignore = "BO10b: Defect(Loops) when an operand is an earlier Boolean result (X op (X op Y)) with a cylinder, cone or sphere"]
fn known_loops() {
    run_cause("loops");
}

#[test]
#[ignore = "BO10b: Defect(LooseSpan) on torus or cylinder against a box, incl. X u (X n Y)"]
fn known_loosespan() {
    run_cause("loosespan");
}

#[test]
#[ignore = "BO10b: Defect(Piece) in (box - cone) u (box n cone)"]
fn known_piece() {
    run_cause("piece");
}

#[test]
#[ignore = "BO10b: Defect(UvUndecided) on a box meeting a cylinder, sphere or torus"]
fn known_uvundecided() {
    run_cause("uvundecided");
}
