// Written for solidcorex. Apache-2.0.
//! The step-4 recipes: each operation's test case as data (research 72 §5.7, §8.1, task
//! T12). A [`Recipe`] is the inputs (placed primitives and a sequence of Boolean steps), the
//! expected [`Outcome`] (built, `Refused` or `Unsupported`, with the cause by name), the
//! truth of the result (volume and area from [`crate::solid`], with [`crate::csg`] as the
//! membership oracle), the topology stated by hand, and the lineage where 72 asks for it.
//!
//! No operation exists in this crate and no library type is used: the later tasks (MK, BO,
//! DT) turn each recipe's [`Part`]s into library calls and compare the answers. A recipe is
//! plain data; [`all`] lists them in a fixed order.
//!
//! **What a count means.**
//! - *Patch.* A maximal smooth patch (72 A12): faces joined across an edge where both sides
//!   lie on one surface record with one side. A cylinder's seam does not split a patch, and
//!   two coplanar faces the Boolean merges are one.
//! - *Cells.* [`Cells`] counts the patch complex. A vertex is where patches' edges end (a
//!   closed edge with no other end has one vertex), an edge is a curve between two patches
//!   (never a seam). The Euler-Poincare relation is then
//!   `V - E + sum over patches (2 - 2 g_f - L_f) = 2 (S - G)` with `L_f` a patch's loops
//!   and `g_f` its genus as a surface (the torus's wall is the only one above zero).
//! - *Pinches.* A vertex or edge shared by two sheets of the surface (two boxes touching at
//!   a corner or along a line) is counted once per sheet, so `S` is the shell count of the
//!   surface cut apart at its pinches.
//!
//! **Resolution.** The few recipes whose answer turns on the document's resolution `c`
//! (a size at the floor, a face a fraction of `c` off a plane) are given in units of the
//! `resolution` passed to [`all`]; every other length is fixed.
//!
//! **Dependencies on 72's proposals.** The outcomes follow 72 §12.4 as ruled: the analytic
//! SSI answers plane against all kinds, two spheres and coaxial pairs (`SsiPair` otherwise);
//! a touch at a vertex or along an edge is built (BO7, BR10), and a touch inside a face
//! too (BO25; D5 had it `Unsupported(NonManifold)`); a horn torus is
//! `Unsupported(HornTorus)` (D3).

// Fixed 3-arrays and loop counters only.
#![allow(clippy::indexing_slicing)]

mod boolean;
pub mod convex;
mod make;

use core::fmt;

use scx_num::fmath;

use crate::analytic::Frame;
use crate::csg::{Csg, CsgError, Primitive, Profile};
use crate::profile::Seg;
use crate::solid::{self, Solid, SolidError};

// ---------------------------------------------------------------------------------------
// Inputs

/// A surface kind, as the analytic SSI names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A plane.
    Plane,
    /// A cylinder.
    Cylinder,
    /// A cone.
    Cone,
    /// A sphere.
    Sphere,
    /// A torus.
    Torus,
}

/// A primitive at its standard placement: axis on `z`, base on `z = 0`; a block with its
/// corner on the origin and extents along the axes; a sphere and a torus centred on the
/// origin (the placement of [`crate::solid`] and [`crate::csg`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// `[0, a] x [0, b] x [0, c]`.
    Block {
        /// The extents along the frame's axes.
        size: [f64; 3],
    },
    /// A cylinder.
    Cylinder {
        /// Its radius.
        radius: f64,
        /// Its height.
        height: f64,
    },
    /// A cone frustum: a radius of zero is an apex.
    Cone {
        /// The radii at `z = 0` and at `z = height`.
        radii: [f64; 2],
        /// Its height.
        height: f64,
    },
    /// A sphere.
    Sphere {
        /// Its radius.
        radius: f64,
    },
    /// A torus.
    Torus {
        /// The radius of the tube's centre circle.
        major: f64,
        /// The tube's radius.
        minor: f64,
    },
    /// A profile swept along `z` from `0` to `height`. The outer loop is counter-clockwise
    /// and each hole clockwise, as [`crate::profile`] fixtures give them.
    Extrusion {
        /// The outer loop.
        outer: Vec<Seg>,
        /// The holes.
        holes: Vec<Vec<Seg>>,
        /// The height.
        height: f64,
    },
}

impl Shape {
    /// The surface kinds of the faces this shape's body has.
    #[must_use]
    pub fn kinds(&self) -> Vec<Kind> {
        match self {
            Shape::Block { .. } => vec![Kind::Plane],
            Shape::Cylinder { .. } => vec![Kind::Plane, Kind::Cylinder],
            Shape::Cone { .. } => vec![Kind::Plane, Kind::Cone],
            Shape::Sphere { .. } => vec![Kind::Sphere],
            Shape::Torus { .. } => vec![Kind::Torus],
            Shape::Extrusion { outer, holes, .. } => {
                let mut k = vec![Kind::Plane];
                if outer
                    .iter()
                    .chain(holes.iter().flatten())
                    .any(|s| matches!(s, Seg::Arc { .. }))
                {
                    k.push(Kind::Cylinder);
                }
                k
            }
        }
    }
}

/// A shape at a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// Where it stands.
    pub frame: Frame,
    /// What it is.
    pub shape: Shape,
}

impl Part {
    /// The closed-form volume, area and moments at the frame ([`crate::solid`]).
    ///
    /// # Errors
    /// [`SolidError`] when the shape has no solid (a size not above zero, a torus whose tube
    /// does not clear its axis, an open profile).
    pub fn solid(&self) -> Result<Solid, SolidError> {
        let local = match &self.shape {
            Shape::Block { size } => solid::block(size[0], size[1], size[2])?,
            Shape::Cylinder { radius, height } => solid::cylinder(*radius, *height)?,
            Shape::Cone { radii, height } => solid::cone(radii[0], radii[1], *height)?,
            Shape::Sphere { radius } => solid::sphere(*radius)?,
            Shape::Torus { major, minor } => solid::torus(*major, *minor)?,
            Shape::Extrusion {
                outer,
                holes,
                height,
            } => {
                let loops: Vec<Vec<Seg>> = core::iter::once(outer.clone())
                    .chain(holes.iter().cloned())
                    .collect();
                solid::extrusion(&loops, *height)?
            }
        };
        Ok(local.place(&self.frame))
    }

    /// The membership oracle for this part alone ([`crate::csg`]).
    ///
    /// # Errors
    /// [`CsgError`] for a size out of range or a frame that is not finite.
    pub fn csg(&self) -> Result<Csg, CsgError> {
        let primitive = match &self.shape {
            Shape::Block { size } => Primitive::Block { size: *size },
            Shape::Cylinder { radius, height } => Primitive::Cylinder {
                radius: *radius,
                height: *height,
            },
            Shape::Cone { radii, height } => Primitive::Frustum {
                radius0: radii[0],
                radius1: radii[1],
                height: *height,
            },
            Shape::Sphere { radius } => Primitive::Sphere { radius: *radius },
            Shape::Torus { major, minor } => Primitive::Torus {
                major: *major,
                minor: *minor,
            },
            Shape::Extrusion {
                outer,
                holes,
                height,
            } => {
                let loops: Vec<Vec<Seg>> = core::iter::once(outer.clone())
                    .chain(holes.iter().cloned())
                    .collect();
                Primitive::Extrusion {
                    profile: Profile::new(&loops)?,
                    height: *height,
                }
            }
        };
        Csg::leaf(&self.frame, primitive)
    }
}

/// A Boolean operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    /// `a` united with `b`.
    Union,
    /// `a` meet `b`.
    Intersection,
    /// `a` less `b`.
    Difference,
}

/// One Boolean step: the running result (operand 0) with `tool` (operand 1).
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    /// The operation.
    pub op: Op,
    /// The second operand.
    pub tool: Part,
}

// ---------------------------------------------------------------------------------------
// Outcomes

/// Which size a refusal names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimension {
    /// A block's extent along its first axis.
    X,
    /// A block's extent along its second axis.
    Y,
    /// A block's extent along its third axis.
    Z,
    /// A cylinder's or a sphere's radius.
    Radius,
    /// A height.
    Height,
    /// A torus's tube radius.
    Minor,
}

/// Why a constructor refuses its input (72 §4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A size that is not finite and strictly above the document's resolution.
    Size(Dimension),
    /// A cone with both radii zero.
    NoBody,
    /// A torus whose tube is wider than its axis circle (a spindle).
    SelfIntersecting,
    /// An extrusion whose two heights are equal.
    Span,
}

/// Why an operation says `Unsupported` (72 §5.2, §12.4).
#[derive(Clone, Debug, PartialEq)]
pub enum Unsupported {
    /// A horn torus (`minor == major`): its apex is a singular point (D3).
    HornTorus,
    /// A face pair whose intersection the analytic SSI has no closed form for (SS2 and the
    /// spiric sections). The two kinds are the pair of surfaces, in `Kind` order: the
    /// library names them in its own order, so compare as a set.
    SsiPair {
        /// The two kinds.
        kinds: [Kind; 2],
    },
    /// A contact the library does not build (no recipe states one since BO25 built the
    /// touch inside a face). The contact points are listed; the cause names one of them.
    NonManifold {
        /// Every such contact.
        at: Vec<[f64; 3]>,
    },
}

/// A face of a block, named by its plane: the order of `scx_make::BlockFaces` (bottom, top,
/// then the sides counter-clockwise from the face on `y = 0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BoxFace {
    /// `z = 0`.
    Bottom,
    /// `z` at its largest.
    Top,
    /// `y = 0`.
    Front,
    /// `x` at its largest.
    Right,
    /// `y` at its largest.
    Back,
    /// `x = 0`.
    Left,
}

/// A group of patches alike in kind, loops and genus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatchClass {
    /// The surface kind.
    pub kind: Kind,
    /// Boundary loops of each patch.
    pub loops: u32,
    /// Genus of each patch as a surface (zero but for a torus's whole wall).
    pub genus: u32,
    /// How many patches.
    pub count: u32,
}

/// The patch complex's counts (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cells {
    /// Vertices.
    pub vertices: u32,
    /// Edges.
    pub edges: u32,
    /// The patches, grouped.
    pub patches: Vec<PatchClass>,
    /// Every vertex has three edges (so `3 V = 2 E`).
    pub cubic: bool,
}

/// What the result's topology must be, stated by hand from the geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topology {
    /// Solid regions. Two bodies touching at a vertex or along a line are two.
    pub solid_regions: u32,
    /// Shells, outer and inner (a cavity adds one).
    pub shells: u32,
    /// Total genus (handles), over all shells.
    pub genus: u32,
    /// The patch complex, where the recipe states it.
    pub cells: Option<Cells>,
}

impl Topology {
    /// The number of patches, where stated.
    #[must_use]
    pub fn patch_count(&self) -> Option<u32> {
        self.cells
            .as_ref()
            .map(|c| c.patches.iter().map(|p| p.count).sum())
    }

    /// `V - E + sum (2 - 2 g_f - L_f) - 2 (S - G)`: zero when the counts are consistent.
    #[must_use]
    pub fn euler_defect(&self) -> Option<i64> {
        let c = self.cells.as_ref()?;
        let faces: i64 = c
            .patches
            .iter()
            .map(|p| i64::from(p.count) * (2 - 2 * i64::from(p.genus) - i64::from(p.loops)))
            .sum();
        Some(
            i64::from(c.vertices) - i64::from(c.edges) + faces
                - 2 * (i64::from(self.shells) - i64::from(self.genus)),
        )
    }
}

/// What a recipe's operation must answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// A body, with this topology.
    Built(Topology),
    /// The input is refused.
    Refused(Refusal),
    /// The operation does not cover the case, and says so by name.
    Unsupported(Unsupported),
    /// No claim: the case sits within the resolution of a degenerate one, where the merged
    /// points rule may settle it either way. If a body is built, the invariants of
    /// [`Invariant::BUILT`] hold; the reason says what is open.
    Open(&'static str),
}

/// The closed-form truth of the result (or, for a recipe that is not built, of the body the
/// step that lifts the limit will build).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Truth {
    /// The volume.
    pub volume: f64,
    /// The area of the whole boundary, where a closed form is stated.
    pub area: Option<f64>,
    /// An absolute allowance, beyond the requested accuracy, on the volume (and on the area
    /// times its own length scale): zero unless the recipe sits within the resolution of a
    /// merge, where the result may differ from the exact union by the merged gap.
    pub slack: f64,
}

/// What a check of the recipe's result is (72 §8.1's rows).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Invariant {
    /// The body passes the contract check and the testkit's structure oracle.
    Valid,
    /// Every edge has two fins and every vertex one fan: not for a pinch.
    Manifold,
    /// The volume is the truth's, within the requested accuracy.
    Volume,
    /// The area is the truth's.
    Area,
    /// Regions, shells, genus and (where stated) the patch complex.
    Topology,
    /// Points classify as [`crate::csg`] says; points in the band are counted, not judged.
    Classification,
    /// The mesh satisfies [`crate::mesh`]'s invariants and its volume is within chord * area.
    Mesh,
    /// Run twice, everything is equal in bits.
    Deterministic,
    /// Each result face's origins are those the recipe's [`Lineage`] lists.
    Lineage,
    /// No edge names a vertex the body does not have.
    NoDanglingVertices,
}

impl Invariant {
    /// What every built body satisfies.
    pub const BUILT: [Invariant; 7] = [
        Invariant::Valid,
        Invariant::Manifold,
        Invariant::Volume,
        Invariant::Topology,
        Invariant::Classification,
        Invariant::Mesh,
        Invariant::Deterministic,
    ];
}

/// One result face's origins: the operand faces it comes from.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceOrigin {
    /// The face's area.
    pub area: f64,
    /// Its origins as `(operand, face)`, sorted: operand 0 is `a`, operand 1 is `b`.
    pub origins: Vec<(u8, BoxFace)>,
}

/// The lineage of a Boolean on two blocks (72 §5.6): the pin's `issue_1462` cases, as
/// outcomes of `Changes`.
#[derive(Clone, Debug, PartialEq)]
pub struct Lineage {
    /// Every result face (patch), by area and origins; the list is the whole result.
    pub faces: Vec<FaceOrigin>,
    /// Edges the operation made where the operands' faces cross, with no operand edge as
    /// their origin.
    pub new_edges: Option<u32>,
    /// Per operand, the edges the other operand crosses, each kept as the half outside it.
    /// Every operand edge appears exactly once in the result.
    pub halved_edges: Option<[u32; 2]>,
}

/// A step-4 test case.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    /// A name for failure messages, unique in [`all`].
    pub name: &'static str,
    /// Where the case comes from: a pin test, or `new` for one 72 adds.
    pub source: &'static str,
    /// The first operand, or the one primitive of a constructor recipe.
    pub base: Part,
    /// The Boolean steps, applied in order to the running result; none for a constructor.
    pub steps: Vec<Step>,
    /// What the operation answers.
    pub outcome: Outcome,
    /// The closed forms; `None` where the input is refused.
    pub truth: Option<Truth>,
    /// The origins of the result's faces, where the recipe states them.
    pub lineage: Option<Lineage>,
    /// What to check of the answer.
    pub invariants: Vec<Invariant>,
}

impl Recipe {
    /// The membership oracle of the whole recipe: the base with each step applied in order.
    ///
    /// # Errors
    /// [`CsgError`] when a part is out of range (a refused recipe).
    pub fn csg(&self) -> Result<Csg, CsgError> {
        let mut tree = self.base.csg()?;
        for s in &self.steps {
            let tool = s.tool.csg()?;
            tree = match s.op {
                Op::Union => tree.union(tool),
                Op::Intersection => tree.intersection(tool),
                Op::Difference => tree.difference(tool),
            };
        }
        Ok(tree)
    }

    /// Every part, base first.
    pub fn parts(&self) -> impl Iterator<Item = &Part> {
        core::iter::once(&self.base).chain(self.steps.iter().map(|s| &s.tool))
    }

    /// Whether the recipe is a constructor (no Boolean step).
    #[must_use]
    pub fn is_constructor(&self) -> bool {
        self.steps.is_empty()
    }
}

impl fmt::Display for Recipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.source)
    }
}

/// Every recipe, in a fixed order: the constructors, then the Booleans. `resolution` is the
/// document's resolution in millimetres: the recipes that sit near it are given in its
/// units (module docs).
#[must_use]
pub fn all(resolution: f64) -> Vec<Recipe> {
    let mut v = make::recipes(resolution);
    v.extend(boolean::recipes(resolution));
    v
}

// ---------------------------------------------------------------------------------------
// Helpers shared by the two lists

/// The frame at `origin` with the world's axes.
#[must_use]
pub fn frame_at(origin: [f64; 3]) -> Frame {
    Frame {
        origin,
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    }
}

/// The frame at `origin` turned `angle` radians about the unit `axis` (Rodrigues).
#[must_use]
pub fn frame_turned(origin: [f64; 3], axis: [f64; 3], angle: f64) -> Frame {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let k = axis.map(|v| v / n);
    let (s, c) = fmath::sin_cos(angle);
    let turn = |v: [f64; 3]| -> [f64; 3] {
        let kv = k[0] * v[0] + k[1] * v[1] + k[2] * v[2];
        let kxv = [
            k[1] * v[2] - k[2] * v[1],
            k[2] * v[0] - k[0] * v[2],
            k[0] * v[1] - k[1] * v[0],
        ];
        [
            v[0] * c + kxv[0] * s + k[0] * kv * (1.0 - c),
            v[1] * c + kxv[1] * s + k[1] * kv * (1.0 - c),
            v[2] * c + kxv[2] * s + k[2] * kv * (1.0 - c),
        ]
    };
    Frame {
        origin,
        x: turn([1.0, 0.0, 0.0]),
        y: turn([0.0, 1.0, 0.0]),
        z: turn([0.0, 0.0, 1.0]),
    }
}

/// A block part.
fn block_at(origin: [f64; 3], size: [f64; 3]) -> Part {
    Part {
        frame: frame_at(origin),
        shape: Shape::Block { size },
    }
}

/// A cylinder part on `z`.
fn cylinder_at(origin: [f64; 3], radius: f64, height: f64) -> Part {
    Part {
        frame: frame_at(origin),
        shape: Shape::Cylinder { radius, height },
    }
}

/// A sphere part.
fn sphere_at(centre: [f64; 3], radius: f64) -> Part {
    Part {
        frame: frame_at(centre),
        shape: Shape::Sphere { radius },
    }
}

/// A torus part about `z`.
fn torus_at(centre: [f64; 3], major: f64, minor: f64) -> Part {
    Part {
        frame: frame_at(centre),
        shape: Shape::Torus { major, minor },
    }
}

fn step(op: Op, tool: Part) -> Step {
    Step { op, tool }
}

/// The patch complex in one line.
fn cells(vertices: u32, edges: u32, cubic: bool, patches: &[(Kind, u32, u32, u32)]) -> Cells {
    Cells {
        vertices,
        edges,
        cubic,
        patches: patches
            .iter()
            .map(|&(kind, loops, genus, count)| PatchClass {
                kind,
                loops,
                genus,
                count,
            })
            .collect(),
    }
}

/// A topology with its cells stated.
fn built(regions: u32, shells: u32, genus: u32, cells: Cells) -> Outcome {
    Outcome::Built(Topology {
        solid_regions: regions,
        shells,
        genus,
        cells: Some(cells),
    })
}

/// The invariants a recipe's outcome calls for.
fn invariants_for(
    outcome: &Outcome,
    truth: Option<Truth>,
    lineage: bool,
    pinch: bool,
) -> Vec<Invariant> {
    match outcome {
        Outcome::Built(_) | Outcome::Open(_) => {
            let mut v: Vec<Invariant> = Invariant::BUILT
                .into_iter()
                .filter(|i| !(pinch && matches!(i, Invariant::Manifold | Invariant::Mesh)))
                .collect();
            if truth.is_some_and(|t| t.area.is_some()) {
                v.push(Invariant::Area);
            }
            if lineage {
                v.push(Invariant::Lineage);
            }
            v
        }
        Outcome::Refused(_) | Outcome::Unsupported(_) => vec![Invariant::Deterministic],
    }
}

/// A recipe with the invariants its outcome calls for.
fn recipe(
    name: &'static str,
    source: &'static str,
    base: Part,
    steps: Vec<Step>,
    outcome: Outcome,
    truth: Option<Truth>,
) -> Recipe {
    let invariants = invariants_for(&outcome, truth, false, false);
    Recipe {
        name,
        source,
        base,
        steps,
        outcome,
        truth,
        lineage: None,
        invariants,
    }
}

impl Recipe {
    /// With a lineage (and the invariant that checks it).
    #[must_use]
    fn with_lineage(mut self, lineage: Lineage) -> Recipe {
        self.lineage = Some(lineage);
        self.invariants.push(Invariant::Lineage);
        self
    }

    /// With one more invariant.
    #[must_use]
    fn also(mut self, invariant: Invariant) -> Recipe {
        self.invariants.push(invariant);
        self
    }

    /// For a body with a pinch: the manifold and mesh checks do not apply (the T13 oracle
    /// reports a non-manifold vertex there).
    #[must_use]
    fn pinched(mut self) -> Recipe {
        self.invariants
            .retain(|i| !matches!(i, Invariant::Manifold | Invariant::Mesh));
        self
    }
}

/// `Truth` from a [`Solid`].
fn truth_of(s: &Solid) -> Truth {
    Truth {
        volume: s.volume(),
        area: Some(s.area),
        slack: 0.0,
    }
}

/// The truth of a recipe's one part.
fn truth_part(p: &Part) -> Option<Truth> {
    p.solid().ok().as_ref().map(truth_of)
}
