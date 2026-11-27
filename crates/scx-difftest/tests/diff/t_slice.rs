// Written for solidcorex. Apache-2.0.
//! The step-4 outcome difftest (research 72 §8.1, task DT1): every T12 recipe built in scx
//! (MK, then BO) and in the pin (its `api` primitives and Booleans), each side judged against
//! truth: the recipe's outcome, the closed-form volume and area of T9, the topology T12
//! states, the T10 classification oracle, validity, the lineage of the `issue_1462` boxes and
//! run-twice determinism.
//!
//! **scx.** Every check runs: the constructors (`scx_make`'s block, cylinder, cone, sphere,
//! torus and extrusion) and the Booleans (`scx_boolean::boolean`, folded over the recipe's
//! steps on one document). Each is judged on outcome (built, refused, or `Unsupported` with
//! its cause, by name), volume and area (`scx_mass` at 1e-9), topology, classification
//! (`scx_bodyq::classify_point`, judged as the pin is), validity and manifoldness
//! (`scx_check::contract`), the mesh (`scx_tess` at a chord of 1% of `cbrt(V)`, against T13's
//! oracle and truth's volume), determinism by the `scx-record` encoding, dangling vertices,
//! the lineage of the `issue_1462` boxes (from `Changes`) and the work (`Report::work`,
//! summed over the steps) against the committed baseline `slice_work.txt`. Every check scx
//! runs must pass.
//!
//! **Work.** `Report::work` is a public value, so the baseline is test data: one line per
//! built Boolean recipe. A rise of more than 10% fails; a fall passes and is printed.
//! `SCX_BLESS_WORK=1 cargo test ... t_slice` rewrites the file from the run.
//!
//! **A body with an edge of four fins** (the pinched recipes, BO7) is judged `Valid` by the
//! contract failing at exactly that breach: `scx_check::contract` refuses such an edge by
//! design (CK1's test 3), so the contract cannot say more of it.
//!
//! **The pin.** It runs every recipe and is judged against truth, never the other way. A
//! check where it contradicts truth is a finding; each finding must be covered by a
//! [`SliceDefect`] in `pin_defects.rs` whose predicate on the recipe's inputs selects it and
//! whose check confirms the recorded contradiction still holds. An entry that covers no
//! finding, or whose predicate selects a recipe the pin gets right, fails: the record stays
//! exact (a classification entry, being sampled, is counted there instead). It is a side
//! record, never filed upstream.
//!
//! **Classification.** 1,000 seeded points in the oracle's box grown 10%, plus a seeded
//! subset of at most 1,000 of T10's probes.
//! Points T10 calls certain must classify on that side; points in its band are counted, not
//! judged (72 §12 row 7). The recipes where the band holds more than 5% of the box points
//! are printed.
//!
//! **Counts printed, never compared.** The pin's face, edge and vertex counts record SMLib's
//! seam and merge choices (72 §8.1), so they are printed beside truth's patch counts.
//!
//! The test prints `difftest t_slice: <n> inputs ...` like the other difftests, then one
//! line per side.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};

use scx_num::fmath::{atan2, hypot};
use scx_num::hash::mix;
use scx_testkit::analytic::Frame as TFrame;
use scx_testkit::csg::{Csg, ProbeSpec, Side, box_points};
use scx_testkit::profile::Seg;
use scx_testkit::recipe::{
    self, BoxFace, Dimension, Invariant, Kind, Op, Outcome, Part, Recipe, Refusal, Shape, Topology,
    Unsupported as RecipeUnsupported,
};

use solidcore::boolean::api_brep::{boolean_difference, boolean_intersection, boolean_union};
use solidcore::curve::curve::Curve;
use solidcore::error::ErrorKind;
use solidcore::geom::attribute::{Attribute, Behavior};
use solidcore::geom::axis2_placement::Axis2Placement;
use solidcore::mass::integrator::properties::{brep_area, brep_volume_default, face_area};
use solidcore::math::{Point3, Vec3};
use solidcore::primitive::api_curves::{create_arc, create_line_segment};
use solidcore::primitive::api_primitives as ap;
use solidcore::query::brep::BrepClassifier;
use solidcore::sweep::api_brep::create_linear_sweep;
use solidcore::topology::Brep as PinBrep;
use solidcore::topology::brep::p2::is_manifold_solid;
use solidcore::topology::brep::p5::transform;
use solidcore::topology::integrity;
use solidcore::{EntityRef, Kernel, Point2};

use crate::pin_defects;

/// The document's resolution `c` in millimetres: scx's `Resolution::DEFAULT` and the pin's
/// system zone, so both sides and the recipes near it share one value.
pub const RES: f64 = 1e-5;
/// T10's band half-width for coincidence `c`: a point's box `p +- delta` reaches every point
/// within `c` of it (`sqrt(3) c`).
const DELTA: f64 = 1.732_050_807_568_877_2 * RES;
/// The pin's requested accuracy for volume and area (72 §8.1: `brep_volume(1e-6)`).
const PIN_ACCURACY: f64 = 1e-6;
/// The seed of every case: one fixed value, mixed with the recipe's name.
const SEED: u64 = 0x5_11CE;
/// Seeded points per case.
const POINTS: usize = 1_000;
/// The most boundary probes per case (a seeded subset when a recipe has more).
const PROBES: usize = 1_000;
/// How far a pin face's samples may lie from an operand's surface and still be on it.
const SHEET_TOL: f64 = 1e-6;
/// The first attribute id the lineage tags use (the pin's `issue_1462` tests' base).
const TAG_BASE: u64 = 10_007;

// ---------------------------------------------------------------------------------------
// What a check is, and what scx cannot run yet

/// A row of 72 §8.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Check {
    /// Built, refused or unsupported, with the cause.
    Outcome,
    /// T9's volume.
    Volume,
    /// T9's area.
    Area,
    /// Solid regions, shells, genus and the smooth patches.
    Topology,
    /// T10's certain points.
    Classification,
    /// The contract check (scx) or the integrity check (the pin).
    Valid,
    /// Two fins per edge, one fan per vertex.
    Manifold,
    /// T13's mesh invariants.
    Mesh,
    /// Run twice, equal in bits.
    Deterministic,
    /// The `issue_1462` boxes' face origins.
    Lineage,
    /// No edge names a vertex the body lacks.
    NoDanglingVertices,
    /// `Report::work`, against the committed baseline.
    Work,
}

impl Check {
    fn of(i: Invariant) -> Check {
        match i {
            Invariant::Valid => Check::Valid,
            Invariant::Manifold => Check::Manifold,
            Invariant::Volume => Check::Volume,
            Invariant::Area => Check::Area,
            Invariant::Topology => Check::Topology,
            Invariant::Classification => Check::Classification,
            Invariant::Mesh => Check::Mesh,
            Invariant::Deterministic => Check::Deterministic,
            Invariant::Lineage => Check::Lineage,
            Invariant::NoDanglingVertices => Check::NoDanglingVertices,
        }
    }
}

/// The checks a recipe asks for: the outcome always, the recipe's invariants where a body
/// is (or may be) built, and the work record for every Boolean that is built (72 §8.1's last
/// row; a constructor has no `Report`).
fn checks_of(r: &Recipe) -> BTreeSet<Check> {
    let mut c: BTreeSet<Check> = BTreeSet::from([Check::Outcome]);
    let built = matches!(r.outcome, Outcome::Built(_) | Outcome::Open(_));
    if built {
        c.extend(r.invariants.iter().map(|&i| Check::of(i)));
        if matches!(r.outcome, Outcome::Open(_)) {
            // No topology is claimed of a body whose existence is open.
            c.remove(&Check::Topology);
        }
        if !r.is_constructor() {
            c.insert(Check::Work);
        }
    }
    c
}

// ---------------------------------------------------------------------------------------
// The case's inputs: truth's points

/// The recipe's seed, from its name (adding a recipe changes no other case's points).
fn case_seed(r: &Recipe) -> u64 {
    let words: Vec<u64> = core::iter::once(SEED)
        .chain(r.name.bytes().map(u64::from))
        .collect();
    mix(&words)
}

/// The points one case classifies, with T10's side for each.
pub struct Points {
    /// The points: the box points first, then the probes.
    pub at: Vec<[f64; 3]>,
    /// T10's side of each.
    pub side: Vec<Side>,
    /// How many of `at` are box points.
    pub boxed: usize,
    /// The oracle, for how far a misclassified point lies from the boundary.
    pub csg: Option<Csg>,
}

fn points(csg: &Csg, seed: u64) -> Points {
    let mut at = match csg.bounds() {
        Some((lo, hi)) => box_points(lo, hi, 0.1, POINTS, mix(&[seed, 0])),
        None => Vec::new(),
    };
    let boxed = at.len();
    let spec = ProbeSpec {
        seed: mix(&[seed, 1]),
        per_face: 4,
        coincidence: RES,
        extent: 5.0,
    };
    // A seeded subset of at most `PROBES`: the probes number up to ~12k on the big plates, and
    // each costs the pin a classification on both runs.
    let mut probes: Vec<(u64, [f64; 3])> = csg
        .probes(&spec)
        .into_iter()
        .enumerate()
        .map(|(i, p)| (mix(&[seed, 2, i as u64]), p.point))
        .collect();
    probes.sort_by_key(|&(k, _)| k);
    at.extend(probes.into_iter().take(PROBES).map(|(_, p)| p));
    let side = at.iter().map(|&p| csg.side(p, DELTA)).collect();
    Points {
        at,
        side,
        boxed,
        csg: Some(csg.clone()),
    }
}

// ---------------------------------------------------------------------------------------
// The operands' surfaces: sheets, for patches and lineage on the pin's body

/// A part's boundary surface, in the part's own frame.
#[derive(Clone, Copy, Debug)]
enum SheetGeom {
    /// `n . l = d`, `n` unit.
    Plane { n: [f64; 3], d: f64 },
    /// The cylinder about the axis through `(c, z)` along `z`.
    Cylinder { c: [f64; 2], r: f64 },
    /// The cone through radius `r0` at `z = 0` and `r1` at `z = h`, about `z`.
    Cone { r0: f64, r1: f64, h: f64 },
    /// The sphere about the origin.
    Sphere { r: f64 },
    /// The torus about `z`.
    Torus { major: f64, minor: f64 },
}

#[derive(Clone, Copy, Debug)]
struct Sheet {
    frame: TFrame,
    geom: SheetGeom,
    kind: Kind,
    /// The part it bounds, `0` the base.
    part: usize,
    /// For a block, which face.
    role: Option<BoxFace>,
}

fn local(f: &TFrame, p: [f64; 3]) -> [f64; 3] {
    let q = [p[0] - f.origin[0], p[1] - f.origin[1], p[2] - f.origin[2]];
    let dot = |a: [f64; 3]| a[0] * q[0] + a[1] * q[1] + a[2] * q[2];
    [dot(f.x), dot(f.y), dot(f.z)]
}

impl Sheet {
    fn distance(&self, p: [f64; 3]) -> f64 {
        let l = local(&self.frame, p);
        let rho = hypot(l[0], l[1]);
        match self.geom {
            SheetGeom::Plane { n, d } => n[0] * l[0] + n[1] * l[1] + n[2] * l[2] - d,
            SheetGeom::Cylinder { c, r } => hypot(l[0] - c[0], l[1] - c[1]) - r,
            SheetGeom::Cone { r0, r1, h } => {
                let s = (r1 - r0) / h;
                // either nappe: the pin's cone surface runs past its apex
                (rho - (r0 + s * l[2]).abs()) / hypot(1.0, s)
            }
            SheetGeom::Sphere { r } => hypot(rho, l[2]) - r,
            SheetGeom::Torus { major, minor } => hypot(rho - major, l[2]) - minor,
        }
    }
}

/// Whether two operands have parallel planes at most `c` apart but not coincident (past
/// rounding, `1e-6 c`): the gap the
/// merged points rule closes (72 §6.2).
pub fn near_coplanar(r: &Recipe) -> bool {
    let planes: Vec<(usize, [f64; 3], [f64; 3])> = sheets(r)
        .iter()
        .filter_map(|s| match s.geom {
            SheetGeom::Plane { n, d } => {
                let f = &s.frame;
                let w = |i: usize| n[0] * f.x[i] + n[1] * f.y[i] + n[2] * f.z[i];
                let nw = [w(0), w(1), w(2)];
                let at = [0, 1, 2].map(|i| f.origin[i] + d * nw[i]);
                Some((s.part, nw, at))
            }
            _ => None,
        })
        .collect();
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    planes.iter().enumerate().any(|(i, &(pa, na, a))| {
        planes[i + 1..].iter().any(|&(pb, nb, b)| {
            let gap = dot(na, [b[0] - a[0], b[1] - a[1], b[2] - a[2]]).abs();
            pa != pb && dot(na, nb).abs() > 1.0 - 1e-12 && gap > 1e-6 * RES && gap <= RES
        })
    })
}

/// Every operand's sheets, base first, a block's in [`BoxFace`] order.
fn sheets(r: &Recipe) -> Vec<Sheet> {
    let mut v = Vec::new();
    for (part, p) in r.parts().enumerate() {
        let mut add = |geom, kind, role| {
            v.push(Sheet {
                frame: p.frame,
                geom,
                kind,
                part,
                role,
            });
        };
        let plane = |n: [f64; 3], d: f64| SheetGeom::Plane { n, d };
        match &p.shape {
            Shape::Block { size } => {
                let roles = [
                    (BoxFace::Bottom, [0.0, 0.0, 1.0], 0.0),
                    (BoxFace::Top, [0.0, 0.0, 1.0], size[2]),
                    (BoxFace::Front, [0.0, 1.0, 0.0], 0.0),
                    (BoxFace::Right, [1.0, 0.0, 0.0], size[0]),
                    (BoxFace::Back, [0.0, 1.0, 0.0], size[1]),
                    (BoxFace::Left, [1.0, 0.0, 0.0], 0.0),
                ];
                for (role, n, d) in roles {
                    add(plane(n, d), Kind::Plane, Some(role));
                }
            }
            Shape::Cylinder { radius, height } => {
                add(
                    SheetGeom::Cylinder {
                        c: [0.0, 0.0],
                        r: *radius,
                    },
                    Kind::Cylinder,
                    None,
                );
                add(plane([0.0, 0.0, 1.0], 0.0), Kind::Plane, None);
                add(plane([0.0, 0.0, 1.0], *height), Kind::Plane, None);
            }
            Shape::Cone { radii, height } => {
                add(
                    SheetGeom::Cone {
                        r0: radii[0],
                        r1: radii[1],
                        h: *height,
                    },
                    Kind::Cone,
                    None,
                );
                for (r, z) in [(radii[0], 0.0), (radii[1], *height)] {
                    if r > 0.0 {
                        add(plane([0.0, 0.0, 1.0], z), Kind::Plane, None);
                    }
                }
            }
            Shape::Sphere { radius } => add(SheetGeom::Sphere { r: *radius }, Kind::Sphere, None),
            Shape::Torus { major, minor } => add(
                SheetGeom::Torus {
                    major: *major,
                    minor: *minor,
                },
                Kind::Torus,
                None,
            ),
            Shape::Extrusion {
                outer,
                holes,
                height,
            } => {
                for s in outer.iter().chain(holes.iter().flatten()) {
                    match *s {
                        Seg::Line { start, end } => {
                            let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
                            let len = hypot(dx, dy);
                            let n = [dy / len, -dx / len, 0.0];
                            add(
                                plane(n, n[0] * start[0] + n[1] * start[1]),
                                Kind::Plane,
                                None,
                            );
                        }
                        Seg::Arc { centre, start, .. } => add(
                            SheetGeom::Cylinder {
                                c: centre,
                                r: hypot(start[0] - centre[0], start[1] - centre[1]),
                            },
                            Kind::Cylinder,
                            None,
                        ),
                    }
                }
                add(plane([0.0, 0.0, 1.0], 0.0), Kind::Plane, None);
                add(plane([0.0, 0.0, 1.0], *height), Kind::Plane, None);
            }
        }
    }
    v
}

/// The first sheet every sample lies on. Two operands' coincident surfaces always resolve
/// to the first of them, so faces on one surface share an index.
fn sheet_of(sheets: &[Sheet], samples: &[[f64; 3]]) -> Option<usize> {
    if samples.is_empty() {
        return None;
    }
    sheets
        .iter()
        .position(|s| samples.iter().all(|&p| s.distance(p).abs() <= SHEET_TOL))
}

// ---------------------------------------------------------------------------------------
// The pin's side

fn p3(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}

fn is_world(f: &TFrame) -> bool {
    f.x == [1.0, 0.0, 0.0] && f.y == [0.0, 1.0, 0.0] && f.z == [0.0, 0.0, 1.0]
}

/// The pin's extrusion: the profile's curves at `z = o.z`, swept along `z`. A clockwise arc
/// is the same arc written counter-clockwise from its end.
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

/// A part in the pin: built at its origin when its frame has the world's axes (as the pin's
/// own tests build), else at the world's origin and placed by `transform`.
fn pin_part(k: &mut Kernel, part: &Part) -> solidcore::Result<PinBrep> {
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
    }?;
    if !world {
        let f = &part.frame;
        let place = Axis2Placement::new(p3(f.origin), p3(f.x), p3(f.y));
        transform(k, &mut b, &place, None, false)?;
    }
    Ok(b)
}

/// How the pin answered.
#[derive(Clone, Debug, PartialEq)]
pub enum PinOutcome {
    /// A body.
    Body,
    /// An error, at the stage named (`make 0` for the base, `step 1` for the first Boolean).
    Error {
        /// Where.
        stage: String,
        /// The pin's kind.
        kind: ErrorKind,
        /// Its message.
        text: String,
    },
    /// A panic, at the stage named.
    Panic {
        /// Where.
        stage: String,
    },
}

impl PinOutcome {
    /// Whether the pin answered with an error of `kind`.
    pub fn is_error(&self, kind: ErrorKind) -> bool {
        matches!(self, PinOutcome::Error { kind: k, .. } if *k == kind)
    }
}

/// How the pin classifies one point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PinSide {
    /// Inside a solid region.
    In,
    /// In a void region.
    Out,
    /// On a face, edge or vertex.
    On,
    /// It failed.
    Failed,
}

/// The pin's topology, as truth counts it.
#[derive(Clone, Debug, PartialEq)]
pub struct PinTopo {
    /// Non-void regions other than the infinite one.
    pub solid_regions: u32,
    /// Face components joined across two-fin edges: the shells cut apart at pinches.
    pub shells: u32,
    /// Total genus over the components, or why it has none (an odd Euler characteristic).
    pub genus: Result<u32, String>,
    /// Faces, edges, vertices: printed only.
    pub fev: [usize; 3],
    /// Smooth patches by kind: faces on one operand surface joined across two-fin edges.
    pub patches: BTreeMap<Kind, u32>,
    /// Faces that lie on no operand's surface.
    pub off_sheet: u32,
}

/// One result face (patch) and its origins, for the lineage check.
pub type PinFace = (f64, Vec<(u8, BoxFace)>);

/// What the pin's body is.
#[derive(Clone, Debug, PartialEq)]
pub struct PinFacts {
    /// `brep_volume_default(1e-6)`.
    pub volume: Result<f64, String>,
    /// `brep_area(1e-6)`.
    pub area: Result<f64, String>,
    /// The counts.
    pub topo: Result<PinTopo, String>,
    /// `is_manifold_solid`.
    pub manifold: Result<bool, String>,
    /// `integrity::check`.
    pub integrity: Result<(), String>,
    /// Edges naming a vertex the body does not have.
    pub dangling: usize,
    /// Each point's side, in [`Points::at`]'s order.
    pub sides: Vec<PinSide>,
    /// The result's patches by area and origins, where the recipe states a lineage.
    pub lineage: Option<Result<Vec<PinFace>, String>>,
    /// Every vertex's point, in list order (for the run-twice comparison).
    pub vertices: Vec<[u64; 3]>,
}

/// One run of the pin on a recipe.
#[derive(Clone, Debug, PartialEq)]
pub struct PinRun {
    /// What it answered.
    pub outcome: PinOutcome,
    /// What its body is, when it built one.
    pub facts: Option<PinFacts>,
}

fn text(e: &solidcore::SolidError) -> String {
    let s = e.to_string();
    s.chars().take(160).collect()
}

/// Tags each face of a block operand with its [`BoxFace`], found from the part's sheets.
fn tag_box(b: &mut PinBrep, k: &Kernel, operand: u8, all: &[Sheet]) -> Result<(), String> {
    let mine: Vec<Sheet> = all
        .iter()
        .filter(|s| s.part == usize::from(operand))
        .copied()
        .collect();
    for f in b.face_list().to_vec() {
        let samples = face_samples(k, b, f);
        let role = sheet_of(&mine, &samples)
            .and_then(|i| mine[i].role)
            .ok_or_else(|| format!("operand {operand}: a face on no block plane"))?;
        let id = TAG_BASE + u64::from(operand) * 10 + role as u64;
        b.add_attribute(
            EntityRef::Face(f),
            Attribute::long(id, id as i64, Behavior::Copy),
        )
        .map_err(|e| text(&e))?;
    }
    Ok(())
}

const ROLES: [BoxFace; 6] = [
    BoxFace::Bottom,
    BoxFace::Top,
    BoxFace::Front,
    BoxFace::Right,
    BoxFace::Back,
    BoxFace::Left,
];

fn face_origins(b: &PinBrep, f: solidcore::topology::FaceId) -> Vec<(u8, BoxFace)> {
    b.entity_attributes(EntityRef::Face(f))
        .into_iter()
        .filter(|(_, a)| a.attribute_id() >= TAG_BASE && a.attribute_id() < TAG_BASE + 20)
        .map(|(_, a)| {
            let raw = a.attribute_id() - TAG_BASE;
            ((raw / 10) as u8, ROLES[(raw % 10) as usize])
        })
        .collect()
}

/// Nine points of the face's surface over its uv box (a sample the surface cannot evaluate
/// is skipped).
fn face_samples(k: &Kernel, b: &PinBrep, f: solidcore::topology::FaceId) -> Vec<[f64; 3]> {
    let Ok(face) = b.face(f) else {
        return Vec::new();
    };
    let Some(s) = face.surface.as_ref() else {
        return Vec::new();
    };
    let (u, v) = (face.uv_domain.u_interval(), face.uv_domain.v_interval());
    let mut out = Vec::new();
    for i in 0..3 {
        for j in 0..3 {
            let t = [0.1, 0.5, 0.9];
            let uv = Point2::new(u.min() + t[i] * u.length(), v.min() + t[j] * v.length());
            if let Ok(p) = s.evaluate_point(k, uv) {
                out.push([p.x, p.y, p.z]);
            }
        }
    }
    out
}

struct Uf(Vec<usize>);

impl Uf {
    fn new(n: usize) -> Uf {
        Uf((0..n).collect())
    }
    fn find(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }
    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

/// The pin body's faces, each with its sheet, its loops and the edges its loops use; and
/// for each edge, the faces that use it (once per use).
struct PinCells {
    faces: Vec<solidcore::topology::FaceId>,
    sheet: Vec<Option<usize>>,
    loops: Vec<usize>,
    edges: Vec<Vec<solidcore::topology::EdgeId>>,
    users: BTreeMap<solidcore::topology::EdgeId, Vec<usize>>,
}

fn pin_cells(k: &Kernel, b: &PinBrep, all: &[Sheet]) -> solidcore::Result<PinCells> {
    let faces: Vec<_> = b.faces().ids().collect();
    let mut cells = PinCells {
        sheet: Vec::new(),
        loops: Vec::new(),
        edges: Vec::new(),
        users: BTreeMap::new(),
        faces: faces.clone(),
    };
    for (i, &f) in faces.iter().enumerate() {
        let face = b.face(f)?;
        cells.sheet.push(sheet_of(all, &face_samples(k, b, f)));
        cells.loops.push(face.loops.len());
        let mut es = Vec::new();
        for &l in &face.loops {
            for le in &b.loop_(l)?.edges {
                es.push(le.edge);
                cells.users.entry(le.edge).or_default().push(i);
            }
        }
        cells.edges.push(es);
    }
    Ok(cells)
}

fn pin_topology(b: &PinBrep, cells: &PinCells, all: &[Sheet]) -> solidcore::Result<PinTopo> {
    let infinite = b.infinite_region();
    let solid_regions = b
        .regions()
        .iter()
        .filter(|(id, r)| r.in_brep && Some(*id) != infinite && r.is_void == 0)
        .count() as u32;
    let n = cells.faces.len();
    // components across two-fin edges, and patches across two-fin edges on one sheet
    let mut comp = Uf::new(n);
    let mut patch = Uf::new(n);
    for users in cells.users.values() {
        if let [a, c] = users.as_slice() {
            comp.join(*a, *c);
            if cells.sheet[*a].is_some() && cells.sheet[*a] == cells.sheet[*c] {
                patch.join(*a, *c);
            }
        }
    }
    let mut chi: BTreeMap<usize, (BTreeSet<_>, BTreeSet<_>, i64)> = BTreeMap::new();
    for i in 0..n {
        let root = comp.find(i);
        let e = chi.entry(root).or_default();
        let torus = cells.sheet[i].is_some_and(|s| all[s].kind == Kind::Torus);
        let g = i64::from(cells.loops[i] == 0 && torus);
        e.2 += 2 - 2 * g - cells.loops[i] as i64;
        for &edge in &cells.edges[i] {
            e.1.insert(edge);
            for v in b.edge(edge)?.vertices {
                e.0.insert(v);
            }
        }
    }
    let mut genus = Ok(0u32);
    for (vs, es, f) in chi.values() {
        let x = vs.len() as i64 - es.len() as i64 + f;
        genus = genus.and_then(|g| {
            if x % 2 != 0 || x > 2 {
                Err(format!("a component's Euler characteristic is {x}"))
            } else {
                Ok(g + ((2 - x) / 2) as u32)
            }
        });
    }
    let mut patches = BTreeMap::new();
    let mut off_sheet = 0;
    let mut seen = BTreeSet::new();
    for i in 0..n {
        match cells.sheet[i] {
            Some(s) => {
                if seen.insert(patch.find(i)) {
                    *patches.entry(all[s].kind).or_insert(0) += 1;
                }
            }
            None => off_sheet += 1,
        }
    }
    Ok(PinTopo {
        solid_regions,
        shells: chi.len() as u32,
        genus,
        fev: [n, b.edges().len(), b.vertices().len()],
        patches,
        off_sheet,
    })
}

fn pin_lineage(k: &Kernel, b: &mut PinBrep, cells: &PinCells) -> solidcore::Result<Vec<PinFace>> {
    let n = cells.faces.len();
    let mut patch = Uf::new(n);
    for users in cells.users.values() {
        if let [a, c] = users.as_slice()
            && cells.sheet[*a].is_some()
            && cells.sheet[*a] == cells.sheet[*c]
        {
            patch.join(*a, *c);
        }
    }
    let mut by: BTreeMap<usize, (f64, BTreeSet<(u8, BoxFace)>)> = BTreeMap::new();
    for i in 0..n {
        let f = cells.faces[i];
        let area = face_area(k, b, f, 1e-7)?;
        let e = by.entry(patch.find(i)).or_default();
        e.0 += area;
        e.1.extend(face_origins(b, f));
    }
    Ok(by
        .into_values()
        .map(|(a, o)| (a, o.into_iter().collect()))
        .collect())
}

fn pin_side(k: &Kernel, b: &PinBrep, c: &mut BrepClassifier<'_, '_>, p: [f64; 3]) -> PinSide {
    match c.point_3d_classify(k, p3(p), RES, false) {
        Ok(pc) => match pc.object() {
            Some(EntityRef::Region(r)) => match b.region(r) {
                Ok(reg) if reg.is_void() == 0 => PinSide::In,
                Ok(_) => PinSide::Out,
                Err(_) => PinSide::Failed,
            },
            Some(EntityRef::Face(_) | EntityRef::Edge(_) | EntityRef::Vertex(_)) => PinSide::On,
            _ => PinSide::Failed,
        },
        Err(_) => PinSide::Failed,
    }
}

fn pin_facts(k: &Kernel, mut b: PinBrep, r: &Recipe, pts: &Points) -> PinFacts {
    let all = sheets(r);
    let volume = brep_volume_default(k, &mut b, PIN_ACCURACY).map_err(|e| text(&e));
    let area = brep_area(k, &mut b, PIN_ACCURACY).map_err(|e| text(&e));
    let cells = pin_cells(k, &b, &all);
    let topo = cells
        .as_ref()
        .map_err(text)
        .and_then(|c| pin_topology(&b, c, &all).map_err(|e| text(&e)));
    let lineage = r.lineage.as_ref().map(|_| {
        cells
            .as_ref()
            .map_err(text)
            .and_then(|c| pin_lineage(k, &mut b, c).map_err(|e| text(&e)))
    });
    let manifold = is_manifold_solid(&b).map_err(|e| text(&e));
    let integrity = integrity::check(&b).map_err(|e| text(&e));
    let dangling = b
        .edges()
        .iter()
        .filter(|(_, e)| e.vertices.iter().any(|&v| b.vertex(v).is_err()))
        .count();
    let mut classifier = BrepClassifier::of_brep(&b);
    let sides = pts
        .at
        .iter()
        .map(|&p| pin_side(k, &b, &mut classifier, p))
        .collect();
    let vertices = b
        .vertex_list()
        .iter()
        .filter_map(|&v| b.vertex(v).ok())
        .map(|v| {
            [
                v.point.x.to_bits(),
                v.point.y.to_bits(),
                v.point.z.to_bits(),
            ]
        })
        .collect();
    PinFacts {
        volume,
        area,
        topo,
        manifold,
        integrity,
        dangling,
        sides,
        lineage,
        vertices,
    }
}

/// The recipe on a fresh kernel: the parts, then each Boolean step on the running result.
pub fn pin_run(r: &Recipe, pts: &Points) -> PinRun {
    let mut k = Kernel::default();
    let all = sheets(r);
    let built = catch_unwind(AssertUnwindSafe(|| -> Result<PinBrep, PinOutcome> {
        let fail = |stage: String| {
            move |e: solidcore::SolidError| PinOutcome::Error {
                stage,
                kind: e.kind(),
                text: text(&e),
            }
        };
        let mut acc = pin_part(&mut k, &r.base).map_err(fail("make 0".into()))?;
        for (i, s) in r.steps.iter().enumerate() {
            let mut tool = pin_part(&mut k, &s.tool).map_err(fail(format!("make {}", i + 1)))?;
            if i == 0 && r.lineage.is_some() {
                let tag = |b: &mut PinBrep, k: &Kernel, o| {
                    tag_box(b, k, o, &all).map_err(|text| PinOutcome::Error {
                        stage: "tag".into(),
                        kind: ErrorKind::Failed,
                        text,
                    })
                };
                tag(&mut acc, &k, 0)?;
                tag(&mut tool, &k, 1)?;
            }
            let step = match s.op {
                Op::Union => boolean_union(&mut k, acc, tool),
                Op::Intersection => boolean_intersection(&mut k, acc, tool),
                Op::Difference => boolean_difference(&mut k, acc, tool),
            };
            acc = step.map_err(fail(format!("step {}", i + 1)))?;
        }
        Ok(acc)
    }));
    match built {
        Ok(Ok(b)) => {
            let facts = catch_unwind(AssertUnwindSafe(|| pin_facts(&k, b, r, pts)));
            match facts {
                Ok(f) => PinRun {
                    outcome: PinOutcome::Body,
                    facts: Some(f),
                },
                Err(_) => PinRun {
                    outcome: PinOutcome::Panic {
                        stage: "facts".into(),
                    },
                    facts: None,
                },
            }
        }
        Ok(Err(o)) => PinRun {
            outcome: o,
            facts: None,
        },
        Err(_) => PinRun {
            outcome: PinOutcome::Panic {
                stage: "build".into(),
            },
            facts: None,
        },
    }
}

// ---------------------------------------------------------------------------------------
// Judging the pin against truth

/// Where the pin contradicts truth on one check.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// The check.
    pub check: Check,
    /// What truth says and the pin answers.
    pub what: String,
}

/// The pin's volume (or area) allowance: its requested accuracy, the recipe's slack and
/// one rounding of the closed form.
/// The length that turns the recipe's volume slack into an area slack.
fn area_scale(volume: f64) -> f64 {
    scx_num::fmath::cbrt(volume.abs()).max(f64::MIN_POSITIVE)
}

fn allowance(accuracy: f64, truth: f64, slack: f64) -> f64 {
    accuracy * truth.abs() + slack + 1e-13 * truth.abs()
}

/// The rungs, in units of `c`, at which [`reach`] measures a point's distance.
const RUNGS: [f64; 8] = [1.0, 1.5, 2.0, 2.5, 3.0, 10.0, 100.0, 1e4];

/// The least rung `k` at which T10 puts `p` in the band of half-width `sqrt(3) k c`: an
/// upper bound on `p`'s distance from the boundary, in units of `c` (infinite past 1e4).
pub fn reach(csg: &Csg, p: [f64; 3]) -> f64 {
    RUNGS
        .iter()
        .copied()
        .find(|k| csg.side(p, DELTA * k) == Side::Band)
        .unwrap_or(f64::INFINITY)
}

/// T10's certain points the pin puts elsewhere.
#[derive(Clone, Debug, PartialEq)]
pub struct Misplaced {
    /// How many.
    pub count: usize,
    /// The certain points.
    pub certain: usize,
    /// The largest [`reach`] among them.
    pub reach: f64,
    /// The first, described.
    pub first: Option<String>,
}

/// The points T10 calls certain that the pin puts elsewhere.
pub fn misplaced(pts: &Points, sides: &[PinSide]) -> Misplaced {
    let mut m = Misplaced {
        count: 0,
        certain: 0,
        reach: 0.0,
        first: None,
    };
    for (i, (&t, &p)) in pts.side.iter().zip(sides).enumerate() {
        let ok = match t {
            Side::In => p == PinSide::In,
            Side::Out => p == PinSide::Out,
            Side::Band => continue,
        };
        m.certain += 1;
        if !ok {
            m.count += 1;
            if let Some(c) = &pts.csg {
                m.reach = m.reach.max(reach(c, pts.at[i]));
            }
            m.first
                .get_or_insert_with(|| format!("{:?}: truth {t:?}, pin {p:?}", pts.at[i]));
        }
    }
    m
}

fn judge_body(
    r: &Recipe,
    checks: &BTreeSet<Check>,
    topology: Option<&Topology>,
    f: &PinFacts,
    pts: &Points,
    out: &mut Vec<Finding>,
) {
    let mut find = |check, what: String| out.push(Finding { check, what });
    if let Some(t) = r.truth {
        if checks.contains(&Check::Volume) {
            match &f.volume {
                Ok(v) if (v - t.volume).abs() <= allowance(PIN_ACCURACY, t.volume, t.slack) => {}
                Ok(v) => find(
                    Check::Volume,
                    format!(
                        "truth {}, pin {v} (relative {:.1e})",
                        t.volume,
                        (v - t.volume) / t.volume
                    ),
                ),
                Err(e) => find(Check::Volume, format!("the pin's volume fails: {e}")),
            }
        }
        if let (true, Some(a)) = (checks.contains(&Check::Area), t.area) {
            let scale = area_scale(t.volume);
            match &f.area {
                Ok(v) if (v - a).abs() <= allowance(PIN_ACCURACY, a, t.slack / scale) => {}
                Ok(v) => find(
                    Check::Area,
                    format!("truth {a}, pin {v} (relative {:.1e})", (v - a) / a),
                ),
                Err(e) => find(Check::Area, format!("the pin's area fails: {e}")),
            }
        }
    }
    if let (true, Some(t)) = (checks.contains(&Check::Topology), topology) {
        match &f.topo {
            Ok(p) => {
                let mut bad = Vec::new();
                if p.solid_regions != t.solid_regions {
                    bad.push(format!(
                        "solid regions truth {}, pin {}",
                        t.solid_regions, p.solid_regions
                    ));
                }
                if p.shells != t.shells {
                    bad.push(format!("shells truth {}, pin {}", t.shells, p.shells));
                }
                match &p.genus {
                    Ok(g) if *g == t.genus => {}
                    Ok(g) => bad.push(format!("genus truth {}, pin {g}", t.genus)),
                    Err(e) => bad.push(e.clone()),
                }
                if p.off_sheet > 0 {
                    bad.push(format!("{} faces on no operand surface", p.off_sheet));
                }
                if let Some(c) = &t.cells {
                    let mut want: BTreeMap<Kind, u32> = BTreeMap::new();
                    for pc in &c.patches {
                        *want.entry(pc.kind).or_insert(0) += pc.count;
                    }
                    if want != p.patches {
                        bad.push(format!("patches truth {want:?}, pin {:?}", p.patches));
                    }
                }
                if !bad.is_empty() {
                    find(Check::Topology, bad.join("; "));
                }
            }
            Err(e) => find(Check::Topology, format!("the pin's tables fail: {e}")),
        }
    }
    if checks.contains(&Check::Classification) {
        let m = misplaced(pts, &f.sides);
        if m.count > 0 {
            find(
                Check::Classification,
                format!(
                    "{} of {} certain points, all within {} c; first {}",
                    m.count,
                    m.certain,
                    m.reach,
                    m.first.unwrap_or_default()
                ),
            );
        }
    }
    if checks.contains(&Check::Valid)
        && let Err(e) = &f.integrity
    {
        find(Check::Valid, format!("integrity: {e}"));
    }
    if checks.contains(&Check::Manifold) {
        match &f.manifold {
            Ok(true) => {}
            Ok(false) => find(Check::Manifold, "is_manifold_solid says no".into()),
            Err(e) => find(Check::Manifold, format!("is_manifold_solid fails: {e}")),
        }
    }
    if checks.contains(&Check::NoDanglingVertices) && f.dangling > 0 {
        find(
            Check::NoDanglingVertices,
            format!("{} edges name a missing vertex", f.dangling),
        );
    }
    if let (true, Some(want)) = (checks.contains(&Check::Lineage), &r.lineage) {
        match &f.lineage {
            Some(Ok(got)) => {
                if let Some(why) = lineage_differs(&want.faces, got) {
                    find(Check::Lineage, why);
                }
            }
            Some(Err(e)) => find(Check::Lineage, format!("the pin's face areas fail: {e}")),
            None => find(Check::Lineage, "no lineage read".into()),
        }
    }
}

/// Whether the result's patches are the stated ones: equal origins and areas within 1e-6
/// relative, as multisets.
fn lineage_differs(want: &[recipe::FaceOrigin], got: &[PinFace]) -> Option<String> {
    let mut left: Vec<&PinFace> = got.iter().collect();
    for w in want {
        let close = |(a, o): &&PinFace| {
            *o == w.origins && (a - w.area).abs() <= 1e-6 * w.area.abs().max(1.0)
        };
        match left.iter().position(close) {
            Some(i) => {
                left.swap_remove(i);
            }
            None => {
                return Some(format!(
                    "no result face of area {} from {:?}; pin's {:?}",
                    w.area, w.origins, got
                ));
            }
        }
    }
    (!left.is_empty()).then(|| format!("result faces truth does not list: {left:?}"))
}

/// What the pin's two runs contradict of truth. An error on a case truth leaves unsupported
/// or open is not a finding (the pin is not asked to build it).
pub fn judge_pin(r: &Recipe, pts: &Points, runs: &[PinRun; 2]) -> Vec<Finding> {
    let mut out = Vec::new();
    let run = &runs[0];
    let checks = checks_of(r);
    if runs[0] != runs[1] && checks.contains(&Check::Deterministic) {
        out.push(Finding {
            check: Check::Deterministic,
            what: "two runs differ".into(),
        });
    }
    match (&r.outcome, &run.outcome) {
        (Outcome::Refused(_), o) if o.is_error(ErrorKind::InvalidInput) => {}
        (Outcome::Refused(why), o) => out.push(Finding {
            check: Check::Outcome,
            what: format!("truth refuses ({why:?}); pin {}", describe(o)),
        }),
        (Outcome::Built(_), PinOutcome::Body) => {}
        (Outcome::Built(_), o) => out.push(Finding {
            check: Check::Outcome,
            what: format!("truth builds; pin {}", describe(o)),
        }),
        (Outcome::Unsupported(_) | Outcome::Open(_), _) => {}
    }
    if let Some(f) = &run.facts {
        match &r.outcome {
            Outcome::Built(t) => judge_body(r, &checks, Some(t), f, pts, &mut out),
            // A body on a case truth leaves open or unsupported: judged where truth speaks.
            Outcome::Open(_) | Outcome::Unsupported(_) => {
                let mut c: BTreeSet<Check> =
                    Invariant::BUILT.iter().map(|&i| Check::of(i)).collect();
                c.extend(checks.iter().copied());
                c.remove(&Check::Topology);
                if matches!(r.outcome, Outcome::Unsupported(_)) {
                    c.remove(&Check::Manifold);
                }
                judge_body(r, &c, None, f, pts, &mut out);
            }
            Outcome::Refused(_) => {}
        }
    }
    out
}

fn describe(o: &PinOutcome) -> String {
    match o {
        PinOutcome::Body => "builds a body".into(),
        PinOutcome::Error { stage, kind, text } => format!("errs at {stage}: {kind:?} ({text})"),
        PinOutcome::Panic { stage } => format!("panics at {stage}"),
    }
}

/// A recorded pin contradiction of truth (`pin_defects::slice`). `applies` selects the
/// recipes by their inputs; `holds` confirms the pin's answer is still the one recorded.
pub struct SliceDefect {
    /// What the pin gets wrong, briefly.
    pub reason: &'static str,
    /// The check it fails.
    pub check: Check,
    /// The recipes it covers, by their inputs.
    pub applies: fn(&Recipe) -> bool,
    /// The recorded contradiction, still there, on the recipe's points and first run.
    pub holds: fn(&Recipe, &Points, &PinRun) -> Result<(), String>,
}

// ---------------------------------------------------------------------------------------
// The scx side

/// What scx answers on one check.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// It agrees with truth.
    Pass,
    /// It does not.
    Fail(String),
}

/// A place where scx contradicts truth and is known to: recorded with the evidence, as
/// the pin's are in `pin_defects.rs`. Each of `checks` on `recipe` must still fail with
/// `shows` in its message; an entry that no longer holds fails the run, so the fix removes it.
struct ScxGap {
    recipe: &'static str,
    checks: &'static [Check],
    shows: &'static str,
    reason: &'static str,
}

/// scx's recorded shortfalls against truth (72 §14.38). They are scx's to fix, not truth's
/// to change.
fn scx_gaps() -> Vec<ScxGap> {
    vec![
        ScxGap {
            recipe: "coplanar-offset-0.1c",
            checks: &[Check::Topology],
            shows: "cells [12, 16]",
            reason: "the two fronts 0.1c apart merge to one face, but the four jog vertices and four jog edges of the step stay (12 vertices, 16 edges; truth 8, 12)",
        },
        ScxGap {
            recipe: "coplanar-offset-0.5c",
            checks: &[Check::Topology],
            shows: "cells [12, 16]",
            reason: "as coplanar-offset-0.1c, at 0.5c",
        },
        ScxGap {
            recipe: "crease-0.3c",
            checks: &[Check::Mesh],
            shows: "NonManifoldVertex",
            reason: "a crease 0.3c inside the cube's edge builds, and its mesh has a vertex where the surface meets itself; T12 leaves the recipe open but asks the mesh invariants of any body built",
        },
        ScxGap {
            recipe: "cylinder-minus-itself",
            checks: &[
                Check::Outcome,
                Check::Volume,
                Check::Area,
                Check::Topology,
                Check::Classification,
                Check::Valid,
                Check::Manifold,
                Check::Mesh,
                Check::Deterministic,
            ],
            shows: "Defect(Seeds",
            reason: "a cylinder less itself fails the seed cross-check (BO6) on its first face, where truth is the empty body (block-minus-itself builds it)",
        },
    ]
}

fn mm(v: f64) -> scx_num::units::Length {
    scx_num::units::Length::from_mm(v)
}

fn scx_frame(f: &TFrame) -> Option<scx_num::frame::Frame> {
    use scx_num::vector3d::{Point3 as P, Vec3 as V};
    scx_num::frame::Frame::new(
        P::new(f.origin[0], f.origin[1], f.origin[2]),
        V::new(f.z[0], f.z[1], f.z[2]),
        V::new(f.x[0], f.x[1], f.x[2]),
    )
}

/// Truth's name for the size scx refuses; a cone's two radii are truth's `Radius`, and a
/// torus's major radius has no name in truth (no recipe refuses it).
fn scx_dimension(d: scx_make::Dimension) -> Option<Dimension> {
    use scx_make::Dimension as D;
    match d {
        D::X => Some(Dimension::X),
        D::Y => Some(Dimension::Y),
        D::Z => Some(Dimension::Z),
        D::Radius | D::BaseRadius | D::TopRadius => Some(Dimension::Radius),
        D::Height => Some(Dimension::Height),
        D::Minor => Some(Dimension::Minor),
        _ => None,
    }
}

/// Whether scx's error is the outcome truth states for a constructor.
fn scx_outcome_matches(e: &scx_make::MakeError, want: &Outcome) -> bool {
    use scx_make::{MakeError as E, Refusal as R, Unsupported as U};
    match (e, want) {
        (E::Refused(R::Size { dimension, .. }), Outcome::Refused(Refusal::Size(d))) => {
            scx_dimension(*dimension) == Some(*d)
        }
        (E::Refused(R::ZeroRadii), Outcome::Refused(Refusal::NoBody))
        | (E::Refused(R::SelfIntersecting { .. }), Outcome::Refused(Refusal::SelfIntersecting))
        | (E::Refused(R::Span { .. }), Outcome::Refused(Refusal::Span))
        | (
            E::Unsupported(U::HornTorus { .. }),
            Outcome::Unsupported(RecipeUnsupported::HornTorus),
        ) => true,
        _ => false,
    }
}

/// The one adapter from the testkit's segments to the planar layer's; equal arc ends are
/// the whole circle.
fn scx_segment(s: &Seg) -> scx_planar::Segment {
    use scx_num::frame::Sense;
    use scx_num::vector2d::Point2;
    use scx_planar::Segment;
    match *s {
        Seg::Line { start, end } => Segment::line(Point2::from(start), Point2::from(end)),
        Seg::Arc {
            centre,
            start,
            end,
            ccw,
        } => {
            let sense = if ccw { Sense::Forward } else { Sense::Reversed };
            if start == end {
                let r = hypot(start[0] - centre[0], start[1] - centre[1]);
                Segment::circle(Point2::from(centre), r, sense)
            } else {
                Segment::arc(
                    Point2::from(centre),
                    Point2::from(start),
                    Point2::from(end),
                    sense,
                )
            }
        }
    }
}

/// A part as scx builds it: the body, and a block's faces by role (the lineage maps them to
/// truth's `BoxFace`).
fn scx_build(
    frame: &scx_num::frame::Frame,
    p: &Part,
) -> Result<(scx_brep::Brep, Option<scx_make::BlockFaces>), scx_make::MakeError> {
    let doc = scx_brep::Document::default();
    let plain = |m: Result<scx_brep::Brep, scx_make::MakeError>| m.map(|b| (b, None));
    match &p.shape {
        Shape::Block { size } => {
            scx_make::block(frame, size.map(mm), &doc).map(|m| (m.body, Some(m.faces)))
        }
        Shape::Cylinder { radius, height } => {
            plain(scx_make::cylinder(frame, mm(*radius), mm(*height), &doc).map(|m| m.body))
        }
        Shape::Cone { radii, height } => {
            plain(scx_make::cone(frame, radii.map(mm), mm(*height), &doc).map(|m| m.body))
        }
        Shape::Sphere { radius } => {
            plain(scx_make::sphere(frame, mm(*radius), &doc).map(|m| m.body))
        }
        Shape::Torus { major, minor } => {
            plain(scx_make::torus(frame, mm(*major), mm(*minor), &doc).map(|m| m.body))
        }
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
            .map_err(|e| scx_make::MakeError::Refused(scx_make::Refusal::Profile(e)))?;
            let span = scx_make::Span::new(mm(0.0), mm(*height));
            plain(scx_make::extrude(&region, frame, span, &doc).map(|m| m.body))
        }
    }
}

/// Why scx's run of a recipe stopped.
#[derive(Debug)]
enum ScxError {
    /// A part's frame is not orthonormal.
    Frame,
    /// A part's constructor.
    Make(scx_make::MakeError),
    /// A Boolean step.
    Boolean(scx_boolean::BooleanError),
}

impl core::fmt::Display for ScxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ScxError::Frame => write!(f, "a frame that is not orthonormal"),
            ScxError::Make(e) => write!(f, "{e}"),
            ScxError::Boolean(e) => write!(f, "{e:?}"),
        }
    }
}

/// What a single-step Boolean says of its faces: the `Changes`, the two operands and the
/// faces of each, for the lineage check.
struct ScxLineage {
    changes: scx_brep::Changes,
    operands: [scx_brep::Brep; 2],
    faces: [scx_make::BlockFaces; 2],
}

/// scx's run of a recipe: the base, then each Boolean step on the running result.
struct ScxRun {
    body: scx_brep::Brep,
    /// `Report::work` summed over the steps.
    work: u64,
    /// For a one-step recipe on two blocks.
    lineage: Option<ScxLineage>,
}

fn scx_boolean_op(o: Op) -> scx_boolean::BooleanOp {
    match o {
        Op::Union => scx_boolean::BooleanOp::Union,
        Op::Intersection => scx_boolean::BooleanOp::Intersect,
        Op::Difference => scx_boolean::BooleanOp::Subtract,
    }
}

fn scx_operand(p: &Part) -> Result<(scx_brep::Brep, Option<scx_make::BlockFaces>), ScxError> {
    let frame = scx_frame(&p.frame).ok_or(ScxError::Frame)?;
    scx_build(&frame, p).map_err(ScxError::Make)
}

fn scx_run(r: &Recipe) -> Result<ScxRun, ScxError> {
    use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
    let doc = scx_brep::Document::default();
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = scx_num::budget::Stop::new();
    let (mut body, base_faces) = scx_operand(&r.base)?;
    let mut work = 0u64;
    let mut lineage = None;
    for step in &r.steps {
        let (tool, tool_faces) = scx_operand(&step.tool)?;
        let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
        let done = scx_boolean::boolean(&body, &tool, scx_boolean_op(step.op), &mut cx)
            .map_err(ScxError::Boolean)?;
        work += done.report.work;
        if let (1, Some(a), Some(b)) = (r.steps.len(), base_faces, tool_faces) {
            lineage = Some(ScxLineage {
                changes: done.changes,
                operands: [body.clone(), tool],
                faces: [a, b],
            });
        }
        body = done.body;
    }
    Ok(ScxRun {
        body,
        work,
        lineage,
    })
}

/// scx's topology, as truth counts it: solid regions, their shells, the genus from the
/// Euler-Poincare relation, and the patch complex (faces joined across two-fin edges whose
/// faces' surfaces are equal).
fn scx_topology(b: &scx_brep::Brep, t: &Topology) -> Verdict {
    use scx_surface::Surface;
    let solid: Vec<_> = b.regions().filter(|r| r.is_solid()).collect();
    let shells = solid.iter().map(|r| r.shells().count()).sum::<usize>() as u32;
    let faces: Vec<_> = b.faces().collect();
    let index: HashMap<_, _> = faces.iter().enumerate().map(|(i, f)| (f.id(), i)).collect();
    // Truth's complex has no seams and no poles (T12): drop the edges a face meets
    // twice and the collapsed ones, and count what is left. A vertex or an edge two solid
    // shells share (a pinch, BO7) counts once for each: T12 counts per sheet.
    let mut sheet = vec![usize::MAX; faces.len()];
    for (k, sh) in solid.iter().flat_map(|r| r.shells()).enumerate() {
        for (f, _) in sh.faces() {
            sheet[index[&f.id()]] = k;
        }
    }
    let mut patch = Uf::new(faces.len());
    let mut kept: Vec<Vec<[scx_brep::VertexId; 2]>> = vec![Vec::new(); faces.len()];
    let mut ends: BTreeSet<(usize, scx_brep::VertexId)> = BTreeSet::new();
    let mut edges = 0i64;
    for e in b.edges() {
        let fins: Vec<_> = e.fins().map(|f| index[&f.face]).collect();
        let seam = matches!(fins.as_slice(), [a, c] if a == c);
        if seam || matches!(e.shape(), scx_brep::EdgeShape::Collapsed) {
            continue;
        }
        let sheets: BTreeSet<usize> = fins.iter().map(|&f| sheet[f]).collect();
        edges += sheets.len() as i64;
        for &k in &sheets {
            ends.extend(e.ends().map(|v| (k, v)));
        }
        for &f in &fins {
            kept[f].push(e.ends());
        }
        if let [a, c] = fins.as_slice()
            && faces[*a].surface() == faces[*c].surface()
        {
            patch.join(*a, *c);
        }
    }
    // a face's boundaries: the connected pieces of its kept edges
    let bounds: Vec<i64> = kept
        .iter()
        .map(|es| {
            let at: BTreeMap<_, _> = es
                .iter()
                .flatten()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .enumerate()
                .map(|(i, v)| (*v, i))
                .collect();
            let mut uf = Uf::new(at.len());
            for [p, q] in es {
                uf.join(at[p], at[q]);
            }
            (0..at.len())
                .map(|i| uf.find(i))
                .collect::<BTreeSet<_>>()
                .len() as i64
        })
        .collect();
    let genus_f =
        |i: usize| i64::from(bounds[i] == 0 && matches!(faces[i].surface(), Surface::Torus(_)));
    let fsum: i64 = (0..faces.len())
        .map(|i| 2 - 2 * genus_f(i) - bounds[i])
        .sum();
    let chi = ends.len() as i64 - edges + fsum;
    let genus = i64::from(shells) - chi / 2;
    let mut bad = Vec::new();
    if solid.len() as u32 != t.solid_regions {
        bad.push(format!(
            "solid regions {} (truth {})",
            solid.len(),
            t.solid_regions
        ));
    }
    if shells != t.shells {
        bad.push(format!("shells {shells} (truth {})", t.shells));
    }
    if chi % 2 != 0 || genus != i64::from(t.genus) {
        bad.push(format!("genus {genus}, chi {chi} (truth {})", t.genus));
    }
    if let Some(c) = &t.cells {
        let roots: BTreeSet<usize> = (0..faces.len()).map(|i| patch.find(i)).collect();
        if roots.len() == faces.len() {
            // every patch one face: the complex is the body's own
            let kind = |s: &Surface| match s {
                Surface::Plane(_) => Some(Kind::Plane),
                Surface::Cylinder(_) => Some(Kind::Cylinder),
                Surface::Cone(_) => Some(Kind::Cone),
                Surface::Sphere(_) => Some(Kind::Sphere),
                Surface::Torus(_) => Some(Kind::Torus),
                _ => None,
            };
            let mut got: BTreeMap<(Option<Kind>, u32, u32), u32> = BTreeMap::new();
            for (i, f) in faces.iter().enumerate() {
                let key = (kind(f.surface()), bounds[i] as u32, genus_f(i) as u32);
                *got.entry(key).or_insert(0) += 1;
            }
            let mut want: BTreeMap<(Option<Kind>, u32, u32), u32> = BTreeMap::new();
            for p in &c.patches {
                *want.entry((Some(p.kind), p.loops, p.genus)).or_insert(0) += p.count;
            }
            let ve = [ends.len() as u32, edges as u32];
            if got != want || ve != [c.vertices, c.edges] {
                bad.push(format!(
                    "cells {ve:?} {got:?} (truth {:?} {want:?})",
                    [c.vertices, c.edges]
                ));
            }
        } else {
            bad.push("faces merge into patches: count the patch complex".into());
        }
    }
    if bad.is_empty() {
        Verdict::Pass
    } else {
        Verdict::Fail(bad.join("; "))
    }
}

/// Where scx's `classify_point` puts a point, in the pin's terms: a solid region is `In`,
/// any other region `Out`, a face, edge or vertex `On`, and an error `Failed`.
fn scx_side(b: &scx_brep::Brep, p: [f64; 3]) -> PinSide {
    let at = scx_num::vector3d::Point3::new(p[0], p[1], p[2]);
    let found = scx_bodyq::classify_point(
        b,
        at,
        &mut scx_num::budget::Budget::unlimited(),
        &scx_num::budget::Stop::new(),
    );
    match found {
        Ok(scx_bodyq::PointClass::In(id)) if b.region(id).is_some_and(|r| r.is_solid()) => {
            PinSide::In
        }
        Ok(scx_bodyq::PointClass::In(_)) => PinSide::Out,
        Ok(_) => PinSide::On,
        Err(_) => PinSide::Failed,
    }
}

/// T10's certain points must classify on their side in scx, as in the pin; band points are
/// not judged. Every misplaced point is listed.
fn scx_classification(b: &scx_brep::Brep, pts: &Points) -> Verdict {
    let sides: Vec<PinSide> = pts.at.iter().map(|&p| scx_side(b, p)).collect();
    let m = misplaced(pts, &sides);
    if m.count == 0 {
        return Verdict::Pass;
    }
    let mut all = String::new();
    for ((p, t), s) in pts.at.iter().zip(&pts.side).zip(&sides) {
        let wrong = match t {
            Side::In => *s != PinSide::In,
            Side::Out => *s != PinSide::Out,
            Side::Band => false,
        };
        if wrong {
            let _ = write!(all, "; {p:?}: truth {t:?}, scx {s:?}");
        }
    }
    Verdict::Fail(format!(
        "{} of {} certain points, all within {} c{all}",
        m.count, m.certain, m.reach
    ))
}

/// The accuracy scx's mass properties are asked for and judged at (72 §8.1).
const SCX_ACCURACY: f64 = 1e-9;

fn scx_measure(
    b: &scx_brep::Brep,
    f: fn(
        &scx_brep::Brep,
        scx_mass::RelAccuracy,
        &mut scx_brep::op::Scratch,
        &mut scx_num::budget::Budget,
        &scx_num::budget::Stop,
    ) -> Result<scx_mass::Estimate, scx_mass::MassError>,
) -> Result<f64, String> {
    let accuracy = scx_mass::RelAccuracy::new(SCX_ACCURACY).map_err(|e| e.to_string())?;
    f(
        b,
        accuracy,
        &mut scx_brep::op::Scratch::new(),
        &mut scx_num::budget::Budget::unlimited(),
        &scx_num::budget::Stop::new(),
    )
    .map(|e| e.value)
    .map_err(|e| e.to_string())
}

fn scx_judge(what: &str, got: Result<f64, String>, truth: f64, slack: f64) -> Verdict {
    match got {
        Ok(v) if (v - truth).abs() <= allowance(SCX_ACCURACY, truth, slack) => Verdict::Pass,
        Ok(v) => Verdict::Fail(format!(
            "{what}: truth {truth}, scx {v} (relative {:.1e})",
            (v - truth) / truth
        )),
        Err(e) => Verdict::Fail(format!("{what}: scx fails: {e}")),
    }
}

fn scx_volume(b: &scx_brep::Brep, r: &Recipe) -> Verdict {
    match r.truth {
        Some(t) => scx_judge(
            "volume",
            scx_measure(b, scx_mass::volume),
            t.volume,
            t.slack,
        ),
        None => Verdict::Fail("volume asked with no truth".into()),
    }
}

fn scx_area(b: &scx_brep::Brep, r: &Recipe) -> Verdict {
    match r.truth {
        Some(t) => match t.area {
            Some(a) => scx_judge(
                "area",
                scx_measure(b, scx_mass::area),
                a,
                t.slack / area_scale(t.volume),
            ),
            None => Verdict::Fail("area asked with no stated area".into()),
        },
        None => Verdict::Fail("area asked with no truth".into()),
    }
}

/// The contract (`scx_check::contract`): closed, manifold and outward, or the first breach
/// named. It answers `Valid` and `Manifold` alike, as the pin's two checks both ask of a
/// body that every edge has two fins and every face bounds the solid.
fn scx_contract(b: &scx_brep::Brep) -> Verdict {
    use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
    let doc = scx_brep::Document::with_resolution(b.resolution());
    let mut scratch = Scratch::default();
    let settings = Settings::new();
    let stop = scx_num::budget::Stop::new();
    let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    match scx_check::contract(b, &cx) {
        Ok(_) => Verdict::Pass,
        Err(e) => Verdict::Fail(format!("contract: {e} ({e:?})")),
    }
}

/// The mesh's chord as a fraction of the body's size, `cbrt(V)`.
const MESH_CHORD: f64 = 0.01;

/// T13's mesh invariants on scx's mesh at the recipe's chord: no violation of the oracle
/// (closed, manifold, outward, unfolded, no degenerate triangle), every face's stated bound
/// within the chord, and the enclosed volume within chord times truth's area of truth's
/// volume (rounding, `1e-9` of the volume, added). The chord is `MESH_CHORD` of `cbrt(V)`,
/// never below ten resolutions.
fn scx_mesh(b: &scx_brep::Brep, r: &Recipe) -> Verdict {
    let Some(t) = r.truth else {
        return Verdict::Fail("mesh asked with no truth".into());
    };
    let chord = (MESH_CHORD * scx_num::fmath::cbrt(t.volume)).max(10.0 * RES);
    let spec = match scx_tess::TessSpec::new(mm(chord), b.resolution()) {
        Ok(s) => s,
        Err(e) => return Verdict::Fail(format!("spec at chord {chord}: {e}")),
    };
    let mesh = match scx_tess::tessellate(
        b,
        &spec,
        &mut scx_num::budget::Budget::unlimited(),
        &scx_num::budget::Stop::new(),
    ) {
        Ok(m) => m,
        Err(e) => return Verdict::Fail(format!("tessellate at chord {chord}: {e}")),
    };
    let m = scx_testkit::mesh::Mesh {
        vertices: mesh.positions.iter().map(|q| [q.x, q.y, q.z]).collect(),
        triangles: mesh.triangles.clone(),
    };
    let v = scx_testkit::mesh::violations(&m, scx_testkit::mesh::Limits::default());
    if !v.is_empty() {
        return Verdict::Fail(format!(
            "chord {chord}: {} violations, first {:?}",
            v.len(),
            v.first()
        ));
    }
    if let Some((f, bound)) = mesh
        .chord_bound
        .iter()
        .find(|(_, bound)| bound.mm() > chord * (1.0 + 1e-9))
    {
        return Verdict::Fail(format!(
            "face {f} states bound {} over chord {chord}",
            bound.mm()
        ));
    }
    let Some(area) = t.area else {
        return Verdict::Pass;
    };
    let slack = scx_testkit::mesh::chord_bound(chord, area) + 1e-9 * t.volume;
    let gap = scx_testkit::mesh::volume_gap(&m, t.volume, slack);
    if gap.holds() {
        Verdict::Pass
    } else {
        Verdict::Fail(format!("chord {chord}: volume {gap:?}"))
    }
}

/// Truth's kind for scx's surface kind (a NURBS face is no recipe's).
fn scx_kind(k: scx_surface::SurfaceKind) -> Option<Kind> {
    use scx_surface::SurfaceKind as K;
    match k {
        K::Plane => Some(Kind::Plane),
        K::Cylinder => Some(Kind::Cylinder),
        K::Cone => Some(Kind::Cone),
        K::Sphere => Some(Kind::Sphere),
        K::Torus => Some(Kind::Torus),
        _ => None,
    }
}

/// Whether scx's answer to the recipe is the outcome truth states, the `Unsupported` ones by
/// name (a touch inside a face is `NonManifold`, a pair with no closed form `SsiPair` of the
/// same two kinds), and an `Open` recipe either built or refused as unsupported.
fn scx_outcome(got: &Result<ScxRun, ScxError>, want: &Outcome) -> Verdict {
    use scx_boolean::{BooleanError as E, BooleanUnsupported as U};
    let ok = match (got, want) {
        (Ok(_), Outcome::Built(_) | Outcome::Open(_)) => true,
        (Err(ScxError::Make(e)), want) => scx_outcome_matches(e, want),
        (
            Err(ScxError::Boolean(E::Unsupported(U::NonManifold { .. }))),
            Outcome::Unsupported(RecipeUnsupported::NonManifold { .. }),
        ) => true,
        (
            Err(ScxError::Boolean(E::Unsupported(U::SsiPair { kinds, .. }))),
            Outcome::Unsupported(RecipeUnsupported::SsiPair { kinds: want }),
        ) => {
            let mut got = kinds.map(scx_kind);
            let mut want = want.map(Some);
            got.sort();
            want.sort();
            got == want
        }
        (Err(ScxError::Boolean(E::Unsupported(_) | E::Refused(_))), Outcome::Open(_)) => true,
        _ => false,
    };
    if ok {
        Verdict::Pass
    } else {
        Verdict::Fail(format!(
            "truth {want:?}; scx {}",
            match got {
                Ok(_) => "builds".to_owned(),
                Err(e) => e.to_string(),
            }
        ))
    }
}

/// The contract, for a recipe whose truth is an empty body (no solid region): the contract
/// refuses a body with no face, and that is the answer ("the body has no face"), so it is
/// passed there and only there.
fn scx_contract_empty(b: &scx_brep::Brep) -> Verdict {
    if b.faces().next().is_none() && b.regions().all(|r| !r.is_solid()) {
        Verdict::Pass
    } else {
        Verdict::Fail("an empty result has a face or a solid region".into())
    }
}

/// The contract, for a recipe whose body has an edge of four fins: the contract refuses such
/// an edge by design (CK1's test 3), so it passes when it fails there and nowhere else
/// (`scx_check::contract` stops at the first breach, which is the structure test).
fn scx_contract_pinched(b: &scx_brep::Brep) -> Verdict {
    use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
    let doc = scx_brep::Document::with_resolution(b.resolution());
    let mut scratch = Scratch::default();
    let settings = Settings::new();
    let stop = scx_num::budget::Stop::new();
    let cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    match scx_check::contract(b, &cx) {
        Ok(_)
        | Err(scx_check::ContractError::Broken(scx_check::Breach::NonManifoldEdge {
            fins: 4,
            ..
        })) => Verdict::Pass,
        Err(e) => Verdict::Fail(format!("contract: {e} ({e:?})")),
    }
}

/// The face's box origin of an operand face.
fn box_face(f: &scx_make::BlockFaces, id: scx_brep::FaceId) -> Option<BoxFace> {
    let by_role = [
        (f.bottom, BoxFace::Bottom),
        (f.top, BoxFace::Top),
        (f.sides[0], BoxFace::Front),
        (f.sides[1], BoxFace::Right),
        (f.sides[2], BoxFace::Back),
        (f.sides[3], BoxFace::Left),
    ];
    by_role.iter().find(|(g, _)| *g == id).map(|&(_, b)| b)
}

/// The operand entities (operand, entity) a result entity carries the lineage of: its
/// origins, or itself where `Changes` is silent (an untouched face of `a`), with what it
/// merged in.
fn lineage_of<I: scx_brep::Tracked>(
    changes: &scx_brep::EntityChanges<I>,
    id: I,
    own: impl Fn(scx_brep::Origin) -> Option<(u32, I)>,
) -> Vec<(u32, I)> {
    let mut out = Vec::new();
    let mut add = |o: scx_brep::Origin| out.extend(own(o));
    match changes.origins(id) {
        Some(os) => os.iter().for_each(|&o| add(o)),
        None => add(I::origin(id)),
    }
    for m in changes.merged.iter().filter(|m| m.survivor == id) {
        for &g in &m.absorbed {
            add(g.origin());
        }
        m.taken.iter().for_each(|&o| add(o));
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn scx_lineage(run: &ScxRun, want: &recipe::Lineage) -> Verdict {
    use scx_brep::{Entity, Origin};
    let Some(l) = &run.lineage else {
        return Verdict::Fail(
            "lineage asked of a recipe that is not one step on two blocks".into(),
        );
    };
    let acc = match scx_mass::RelAccuracy::new(1e-10) {
        Ok(a) => a,
        Err(e) => return Verdict::Fail(e.to_string()),
    };
    let face_own = |o: Origin| match o {
        Origin::Face(g) => Some((0, g)),
        Origin::Operand(op, Entity::Face(g)) => Some((op.raw(), g)),
        _ => None,
    };
    let edge_own = |o: Origin| match o {
        Origin::Edge(g) => Some((0, g)),
        Origin::Operand(op, Entity::Edge(g)) => Some((op.raw(), g)),
        _ => None,
    };
    let mut got: Vec<PinFace> = Vec::new();
    for f in run.body.faces() {
        let mut origins = Vec::new();
        for (k, g) in lineage_of(&l.changes.faces, f.id(), face_own) {
            match box_face(&l.faces[k as usize], g) {
                Some(b) => origins.push((k as u8, b)),
                None => return Verdict::Fail(format!("{:?} names {g:?}, no block face", f.id())),
            }
        }
        let area = match scx_mass::face_area(
            f,
            acc,
            &mut scx_brep::op::Scratch::new(),
            &mut scx_num::budget::Budget::unlimited(),
            &scx_num::budget::Stop::new(),
        ) {
            Ok(a) => a.value,
            Err(e) => return Verdict::Fail(format!("{:?} area: {e}", f.id())),
        };
        origins.sort_unstable();
        got.push((area, origins));
    }
    if let Some(why) = lineage_differs(&want.faces, &got) {
        return Verdict::Fail(why);
    }
    let length = |b: &scx_brep::Brep, e: scx_brep::EdgeId| -> Option<f64> {
        scx_mass::edge_length(
            b.edge(e)?,
            acc,
            &mut scx_brep::op::Scratch::new(),
            &mut scx_num::budget::Budget::unlimited(),
            &scx_num::budget::Stop::new(),
        )
        .ok()
        .map(|e| e.value)
    };
    let (mut fresh, mut halved, mut seen) = (0u32, [0u32; 2], BTreeSet::new());
    for e in run.body.edges() {
        let from = lineage_of(&l.changes.edges, e.id(), edge_own);
        match from.as_slice() {
            [] => fresh += 1,
            [(k, g)] => {
                seen.insert((*k, *g));
                let (Some(was), Some(is)) = (
                    length(&l.operands[*k as usize], *g),
                    length(&run.body, e.id()),
                ) else {
                    return Verdict::Fail(format!("{:?}: no length", e.id()));
                };
                if is < was - 1e-9 * was.max(1.0) {
                    halved[*k as usize] += 1;
                }
            }
            _ => return Verdict::Fail(format!("{:?} from {from:?}", e.id())),
        }
    }
    if let Some(n) = want.new_edges
        && n != fresh
    {
        return Verdict::Fail(format!("new edges: truth {n}, scx {fresh}"));
    }
    if let Some(h) = want.halved_edges
        && h != halved
    {
        return Verdict::Fail(format!("halved edges: truth {h:?}, scx {halved:?}"));
    }
    Verdict::Pass
}

// ---------------------------------------------------------------------------------------
// The work baseline

/// The committed `Report::work` of each built Boolean recipe (test data: the value is public).
const WORK_BASELINE: &str = include_str!("slice_work.txt");
/// The file's path, for `SCX_BLESS_WORK`.
const WORK_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/diff/slice_work.txt");
/// A rise over this share of the baseline fails (01 §9.4).
const WORK_RISE: f64 = 0.10;

fn work_baseline(name: &str) -> Option<u64> {
    WORK_BASELINE
        .lines()
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| {
            let (n, w) = l.split_once(' ')?;
            (n == name).then(|| w.trim().parse().ok())?
        })
}

fn scx_work(r: &Recipe, got: Option<u64>) -> Verdict {
    let Some(work) = got else {
        // An open recipe scx refuses has no report.
        return Verdict::Pass;
    };
    match work_baseline(r.name) {
        None => Verdict::Fail(format!(
            "work {work}: no baseline (SCX_BLESS_WORK=1 writes slice_work.txt)"
        )),
        Some(base) if work as f64 > base as f64 * (1.0 + WORK_RISE) => Verdict::Fail(format!(
            "work {work} against baseline {base}: a rise of {:.0}%",
            100.0 * (work as f64 / base as f64 - 1.0)
        )),
        Some(_) => Verdict::Pass,
    }
}

/// Every check of the recipe on scx's side, the work of the run (none if it did not build)
/// and whether it built a body.
pub fn scx_checks(r: &Recipe, pts: &Points) -> (BTreeMap<Check, Verdict>, Option<u64>, bool) {
    let checks = checks_of(r);
    let first = scx_run(r);
    let second = scx_run(r);
    let work = first.as_ref().ok().map(|b| b.work);
    let pinched = r.invariants.contains(&Invariant::Valid)
        && !r.invariants.contains(&Invariant::Manifold)
        && matches!(r.outcome, Outcome::Built(_));
    let mut out = BTreeMap::new();
    for c in checks {
        let v = match (c, &first, &r.outcome) {
            (Check::Outcome, got, want) => scx_outcome(got, want),
            (Check::Work, _, _) => scx_work(r, work),
            (Check::Topology, Ok(b), Outcome::Built(t)) => scx_topology(&b.body, t),
            (Check::Deterministic, Ok(a), _) => match (&second, scx_record::encode(&a.body)) {
                (Ok(b), Ok(ea)) => match scx_record::encode(&b.body) {
                    Ok(eb) if ea == eb => Verdict::Pass,
                    Ok(_) => Verdict::Fail("two runs encode differently".into()),
                    Err(e) => Verdict::Fail(format!("encode: {e}")),
                },
                (Err(e), _) => Verdict::Fail(format!("the second run fails: {e}")),
                (_, Err(e)) => Verdict::Fail(format!("encode: {e}")),
            },
            (Check::NoDanglingVertices, Ok(run), _) => {
                let b = &run.body;
                let vs: BTreeSet<_> = b.vertices().map(|v| v.id()).collect();
                let dangling = b
                    .edges()
                    .filter(|e| !vs.contains(&e.start().id()) || !vs.contains(&e.end().id()))
                    .count();
                if dangling == 0 {
                    Verdict::Pass
                } else {
                    Verdict::Fail(format!("{dangling} edges name a missing vertex"))
                }
            }
            (Check::Volume, Ok(b), _) => scx_volume(&b.body, r),
            (Check::Area, Ok(b), _) => scx_area(&b.body, r),
            (Check::Classification, Ok(b), _) => scx_classification(&b.body, pts),
            (Check::Valid | Check::Manifold, Ok(b), Outcome::Built(t)) if t.solid_regions == 0 => {
                scx_contract_empty(&b.body)
            }
            (Check::Valid, Ok(b), _) if pinched => scx_contract_pinched(&b.body),
            (Check::Valid | Check::Manifold, Ok(b), _) => scx_contract(&b.body),
            (Check::Mesh, Ok(b), _) => scx_mesh(&b.body, r),
            (Check::Lineage, Ok(b), _) => match &r.lineage {
                Some(want) => scx_lineage(b, want),
                None => Verdict::Fail("lineage asked with none stated".into()),
            },
            (c, Err(e), _) => Verdict::Fail(format!("{c:?}: no body ({e})")),
            (c, Ok(_), o) => Verdict::Fail(format!("{c:?} asked of {o:?}")),
        };
        out.insert(c, v);
    }
    (out, work, first.is_ok())
}

// ---------------------------------------------------------------------------------------
// The run

/// One case's answers on both sides.
struct Case {
    points: Points,
    band: f64,
    runs: [PinRun; 2],
    findings: Vec<Finding>,
    scx: BTreeMap<Check, Verdict>,
    /// `Report::work` of scx's run, where it built a Boolean.
    work: Option<u64>,
    /// Whether scx built a body.
    scx_built: bool,
}

fn case(r: &Recipe) -> Case {
    let points = match r.csg() {
        Ok(c) => points(&c, case_seed(r)),
        Err(_) => Points {
            at: Vec::new(),
            side: Vec::new(),
            boxed: 0,
            csg: None,
        },
    };
    let boxed = &points.side[..points.boxed];
    let band = if boxed.is_empty() {
        1.0
    } else {
        boxed.iter().filter(|s| **s != Side::Band).count() as f64 / boxed.len() as f64
    };
    let runs = [pin_run(r, &points), pin_run(r, &points)];
    let findings = judge_pin(r, &points, &runs);
    let (scx, work, scx_built) = scx_checks(r, &points);
    Case {
        points,
        band,
        runs,
        findings,
        scx,
        work,
        scx_built,
    }
}

/// The summary of the whole run; the test asserts on it.
struct Summary {
    lines: Vec<String>,
    failures: Vec<String>,
}

fn summarise(
    recipes: &[Recipe],
    cases: &[Case],
    defects: &[SliceDefect],
    gaps: &[ScxGap],
) -> Summary {
    let mut lines = Vec::new();
    let mut failures = Vec::new();
    let points: usize = cases.iter().map(|c| c.points.at.len()).sum();
    let mut used = vec![0usize; defects.len()];
    let mut unsampled = 0usize;
    let (mut pin_defects, mut pin_ok, mut pin_listed) = (0, 0, 0);
    let mut verdicts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut gap_hits: BTreeSet<(usize, Check)> = BTreeSet::new();
    for (r, c) in recipes.iter().zip(cases) {
        let mut line = format!(
            "  {:<28} pin {:<10} fev {:<14} findings {}",
            r.name,
            match &c.runs[0].outcome {
                PinOutcome::Body => "body".to_owned(),
                PinOutcome::Error { kind, .. } => format!("{kind:?}"),
                PinOutcome::Panic { .. } => "panic".to_owned(),
            },
            c.runs[0]
                .facts
                .as_ref()
                .and_then(|f| f.topo.as_ref().ok())
                .map_or_else(|| "-".to_owned(), |t| format!("{:?}", t.fev)),
            c.findings.len()
        );
        if !matches!(r.outcome, Outcome::Built(_) | Outcome::Refused(_))
            && !matches!(c.runs[0].outcome, PinOutcome::Body)
        {
            pin_listed += 1;
        }
        if c.findings.is_empty() {
            pin_ok += 1;
        }
        for f in &c.findings {
            let covering: Vec<usize> = defects
                .iter()
                .enumerate()
                .filter(|(_, d)| d.check == f.check && (d.applies)(r))
                .map(|(i, _)| i)
                .collect();
            match covering.as_slice() {
                [i] => match (defects[*i].holds)(r, &c.points, &c.runs[0]) {
                    Ok(()) => {
                        used[*i] += 1;
                        pin_defects += 1;
                    }
                    Err(why) => failures.push(format!(
                        "{r}: pin defect ({}) no longer holds: {why}; finding: {:?} {}",
                        defects[*i].reason, f.check, f.what
                    )),
                },
                [] => failures.push(format!("{r}: pin {:?}: {}", f.check, f.what)),
                _ => failures.push(format!(
                    "{r}: {} pin defects cover {:?}",
                    covering.len(),
                    f.check
                )),
            }
        }
        // An entry whose predicate selects a recipe the pin gets right is too wide. A sampled
        // check (classification) may miss the contradiction on one recipe's points and find it
        // on the next, so there it is counted, not failed.
        for (i, d) in defects.iter().enumerate() {
            if (d.applies)(r) && !c.findings.iter().any(|f| f.check == d.check) {
                if d.check == Check::Classification {
                    unsampled += 1;
                } else {
                    failures.push(format!(
                        "{r}: pin defect ({}) applies but the pin passes {:?}",
                        d.reason, d.check
                    ));
                    used[i] += 1;
                }
            }
        }
        for (check, v) in &c.scx {
            match v {
                Verdict::Pass => *verdicts.entry("pass").or_insert(0) += 1,
                Verdict::Fail(why) => {
                    let known = gaps.iter().position(|g| {
                        g.recipe == r.name && g.checks.contains(check) && why.contains(g.shows)
                    });
                    if let Some(i) = known {
                        gap_hits.insert((i, *check));
                        *verdicts.entry("known gap").or_insert(0) += 1;
                    } else {
                        *verdicts.entry("fail").or_insert(0) += 1;
                        failures.push(format!("{r}: scx {check:?}: {why}"));
                    }
                }
            }
        }
        if c.band < 0.95 {
            let _ = write!(line, " certain {:.3}", c.band);
        }
        for f in &c.findings {
            let _ = write!(line, "\n      {:?}: {}", f.check, f.what);
        }
        lines.push(line);
    }
    for (i, d) in defects.iter().enumerate() {
        if used[i] == 0 {
            failures.push(format!("pin defect ({}) applies to no recipe", d.reason));
        }
    }
    for (i, g) in gaps.iter().enumerate() {
        for c in g.checks {
            if !gap_hits.contains(&(i, *c)) {
                failures.push(format!(
                    "{}: scx gap ({}) no longer holds on {c:?}",
                    g.recipe, g.reason
                ));
            }
        }
    }
    let unstable = cases.iter().filter(|c| c.runs[0] != c.runs[1]).count();
    lines.insert(
        0,
        format!(
            "difftest t_slice: {} inputs ({} recipes, {points} points), {} pin defects, {} failures, {unstable} pin nondeterministic",
            recipes.len() + points,
            recipes.len(),
            pin_defects,
            failures.len()
        ),
    );
    lines.insert(
        1,
        format!(
            "difftest t_slice pin: {pin_ok} recipes agree with truth, {} with findings ({pin_defects} recorded), {pin_listed} unsupported or open and not built, {unsampled} sampled entries not met on their points",
            recipes.len() - pin_ok
        ),
    );
    let count = |k: &str| *verdicts.get(k).unwrap_or(&0);
    let built = cases.iter().filter(|c| c.work.is_some()).count();
    lines.insert(
        2,
        format!(
            "difftest t_slice scx: {} pass, {} fail, {} known gaps; {built} recipes built, {} refused or unsupported as truth says",
            count("pass"),
            count("fail"),
            count("known gap"),
            recipes.len() - built
        ),
    );
    // Each side is judged against truth; where the two disagree with each other, say so.
    let differ: Vec<String> = recipes
        .iter()
        .zip(cases)
        .filter(|(_, c)| (c.runs[0].outcome == PinOutcome::Body) != c.scx_built)
        .map(|(r, c)| {
            format!(
                "{} (pin {}, scx {})",
                r.name,
                if c.runs[0].outcome == PinOutcome::Body {
                    "builds"
                } else {
                    "refuses"
                },
                if c.scx_built { "builds" } else { "refuses" }
            )
        })
        .collect();
    lines.insert(
        3,
        format!(
            "difftest t_slice outcome, pin against scx: {} of {} recipes differ{}{}",
            differ.len(),
            recipes.len(),
            if differ.is_empty() { "" } else { ": " },
            differ.join(", ")
        ),
    );
    for (r, c) in recipes.iter().zip(cases) {
        if let (Some(w), Some(base)) = (c.work, work_baseline(r.name))
            && w < base
        {
            lines.push(format!(
                "difftest t_slice work: {} fell from {base} to {w}",
                r.name
            ));
        }
    }
    Summary { lines, failures }
}

#[test]
fn t_slice() {
    let recipes = recipe::all(RES);
    let cases: Vec<Case> = std::thread::scope(|s| {
        let handles: Vec<_> = recipes.iter().map(|r| s.spawn(|| case(r))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    if std::env::var_os("SCX_BLESS_WORK").is_some() {
        let mut text = String::from(
            "# Report::work of scx's Boolean on each built Boolean recipe (steps summed), 72 section 14.38.\n# A rise of more than 10% fails t_slice. SCX_BLESS_WORK=1 rewrites this file.\n",
        );
        for (r, c) in recipes.iter().zip(&cases) {
            if let Some(w) = c.work.filter(|_| !r.is_constructor()) {
                let _ = writeln!(text, "{} {w}", r.name);
            }
        }
        std::fs::write(WORK_FILE, text).expect("write the work baseline");
        return;
    }
    let summary = summarise(&recipes, &cases, &pin_defects::slice(), &scx_gaps());
    for l in &summary.lines {
        println!("{l}");
    }
    assert!(
        summary.failures.is_empty(),
        "t_slice: {} failures:\n{}",
        summary.failures.len(),
        summary.failures.join("\n")
    );
}

/// The harness catches what it exists to catch: a wrong truth, an unrecorded contradiction and
/// a stale entry, on one block's case.
#[test]
fn t_slice_controls() {
    let all = recipe::all(RES);
    let r = all.iter().find(|r| r.name == "block-far-turned").unwrap();
    let c = case(r);
    assert!(c.findings.is_empty(), "{:?}", c.findings);
    // a truth one part in 1e4 off is a volume finding
    let mut wrong = r.clone();
    if let Some(t) = &mut wrong.truth {
        t.volume *= 1.0 + 1e-4;
    }
    let f = judge_pin(&wrong, &c.points, &c.runs);
    assert!(f.iter().any(|f| f.check == Check::Volume), "{f:?}");
    // so the summary fails it, unrecorded
    let w = Case {
        findings: f,
        ..case(&wrong)
    };
    let s = summarise(
        std::slice::from_ref(&wrong),
        std::slice::from_ref(&w),
        &[],
        &[],
    );
    assert!(
        s.failures.iter().any(|l| l.contains("pin Volume")),
        "{:?}",
        s.failures
    );
    // an entry that meets nothing is stale
    let stale = SliceDefect {
        reason: "control",
        check: Check::Volume,
        applies: |_| false,
        holds: |_, _, _| Ok(()),
    };
    let s = summarise(
        std::slice::from_ref(r),
        std::slice::from_ref(&c),
        &[stale],
        &[],
    );
    assert!(
        s.failures
            .iter()
            .any(|l| l.contains("applies to no recipe")),
        "{:?}",
        s.failures
    );
    // a recorded scx gap that the check no longer shows is stale
    let gap = ScxGap {
        recipe: r.name,
        checks: &[Check::Volume],
        shows: "volume",
        reason: "control",
    };
    let s = summarise(
        std::slice::from_ref(r),
        std::slice::from_ref(&c),
        &[],
        &[gap],
    );
    assert!(
        s.failures.iter().any(|l| l.contains("no longer holds")),
        "{:?}",
        s.failures
    );
    // an unrecorded scx failure fails the run, and a recorded one does not
    let mut w = case(r);
    w.scx
        .insert(Check::Volume, Verdict::Fail("volume: control".into()));
    let s = summarise(std::slice::from_ref(r), std::slice::from_ref(&w), &[], &[]);
    assert!(
        s.failures.iter().any(|l| l.contains("scx Volume")),
        "{:?}",
        s.failures
    );
    let gap = ScxGap {
        recipe: r.name,
        checks: &[Check::Volume],
        shows: "volume",
        reason: "control",
    };
    let s = summarise(
        std::slice::from_ref(r),
        std::slice::from_ref(&w),
        &[],
        &[gap],
    );
    assert!(s.failures.is_empty(), "{:?}", s.failures);
    // scx's classification passes the true sides and fails the swapped ones
    let b = scx_build(&scx_frame(&r.base.frame).unwrap(), &r.base)
        .unwrap()
        .0;
    assert_eq!(scx_classification(&b, &c.points), Verdict::Pass);
    let swapped = Points {
        at: c.points.at.clone(),
        side: c
            .points
            .side
            .iter()
            .map(|s| match s {
                Side::In => Side::Out,
                Side::Out => Side::In,
                Side::Band => Side::Band,
            })
            .collect(),
        boxed: c.points.boxed,
        csg: None,
    };
    assert!(
        matches!(scx_classification(&b, &swapped), Verdict::Fail(_)),
        "swapped sides pass"
    );
}
