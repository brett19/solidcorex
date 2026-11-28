// Written for solidcorex (docs/research/74-step6-idiom-prep.md §6.3, §27). Ports the walk of
// the pin's `step_assembly.rs` (`placed_bodies`), with its guesses made typed.
//! The product structure as a tree of placed instances: the parts a file defines, every use of
//! a part in an assembly, and every body with the placement that puts it in the file's world
//! frame.
//!
//! [`scx_p21::product`] keeps placements exactly as the file writes them. This module forms
//! their maps and composes them down the tree; it computes no body geometry. Placements are
//! [`Affine`] maps from a part's own frame to the world, in millimetres (p21 has scaled every
//! length), column-vector convention: a child's world map is its parent's after its own,
//! `parent * local`.
//!
//! Nothing is guessed. Where the file does not say how a part sits (an occurrence with no
//! placement, a relationship that is not about the child, two different placements, a frame
//! with no direction), the instance's [`World`] is [`World::Unplaced`] with the cause, the
//! bodies below it are still listed (with the same cause), and the file is still read. A root
//! part is the world frame by definition.

use std::collections::{BTreeMap, BTreeSet};

use scx_num::math::Affine;
use scx_num::vector3d::{Point3, Vec3};
use scx_p21::P21Error;
use scx_p21::error::{EntityCause, Limit};
use scx_p21::part42::geom::Placement;
use scx_p21::product::{
    BodyKind, Definition, Frame, ItemKind, MAX_ASSEMBLY_DEPTH, MAX_VISITS, Occurrence, Operator,
    ProductStructure, Transformation,
};
use scx_p21::units::UnitsFault;

use crate::error::StepError;

/// A product (`PRODUCT_DEFINITION` with its `PRODUCT`): a part or an assembly, defined once.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// The `PRODUCT_DEFINITION` (entity id).
    pub definition: u64,
    /// Its `id` attribute.
    pub id: String,
    /// The `PRODUCT`'s name.
    pub name: String,
    /// The definition's description.
    pub description: String,
}

/// The assembly occurrence that places an [`Instance`] in its parent.
#[derive(Clone, Debug, PartialEq)]
pub struct Use {
    /// The occurrence (entity id).
    pub entity: u64,
    /// Its `id`.
    pub id: String,
    /// Its `name`.
    pub name: String,
    /// Its reference designator, when it has one.
    pub reference_designator: Option<String>,
}

/// One use of a part: a node of the expanded assembly tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    /// The part used (index into [`ProductTree::parts`]).
    pub part: usize,
    /// The occurrence that places it; `None` for a root.
    pub by: Option<Use>,
    /// The instance it is used in (index into [`ProductTree::instances`]); `None` for a root.
    pub parent: Option<usize>,
    /// Where it is in the file's world frame.
    pub world: World,
}

/// Where something sits in the file's world frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum World {
    /// A map from its own frame to the world, in millimetres. Not necessarily rigid: the file's
    /// transformation operators may scale, and a body listed in a representation whose length
    /// unit differs from the one it was read in carries the ratio of the two.
    Placed(Affine),
    /// The file does not give a placement, or gives one that cannot be formed.
    Unplaced(Unplaced),
}

impl World {
    /// The placement, when there is one.
    #[must_use]
    pub const fn placed(&self) -> Option<&Affine> {
        match self {
            Self::Placed(a) => Some(a),
            Self::Unplaced(_) => None,
        }
    }
}

/// Why a placement is not known.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Unplaced {
    /// The occurrence has no `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION`.
    NoRelationship {
        /// The occurrence (entity id).
        occurrence: u64,
    },
    /// None of the occurrence's relationships carries a transformation.
    NoTransformation {
        /// The first relationship (entity id).
        link: u64,
    },
    /// A relationship's two representations are not exactly one of the child's and something
    /// else, so which side the child is on is not decided.
    NotAboutChild {
        /// The relationship (entity id).
        link: u64,
    },
    /// The occurrence's relationships place the child differently.
    Several {
        /// The occurrence (entity id).
        occurrence: u64,
    },
    /// A placement or operator whose directions or scales do not make a regular map (a zero or
    /// parallel axis, a zero scale), or one whose inverse is not proved to exist.
    Degenerate {
        /// The entity (a relationship, its transformation, a representation map or a mapped
        /// item).
        entity: u64,
    },
    /// The length unit of the representation that lists the body is missing or contradictory,
    /// so the size of its numbers against the body's own is not known.
    Units {
        /// The representation (entity id).
        representation: u64,
        /// What its context lacks.
        fault: UnitsFault,
    },
}

/// A body listed by the product structure, and where it is.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedBody {
    /// The body (entity id): a `MANIFOLD_SOLID_BREP`, `SHELL_BASED_SURFACE_MODEL`, ...
    pub body: u64,
    /// What kind of body the representation lists it as.
    pub kind: BodyKind,
    /// The representation that lists it here (entity id).
    pub representation: u64,
    /// The instance whose shape it is part of (index into [`ProductTree::instances`]).
    pub instance: usize,
    /// The mapped items it is reached through, outermost first (entity ids): empty for a body
    /// the instance's own representation lists. Two placements of one body in one instance
    /// differ here.
    pub mapped_by: Vec<u64>,
    /// Where it is.
    pub world: World,
}

/// The product structure of a file: its parts, the assembly expanded into instances, and every
/// body placed.
///
/// The order is the file's, and deterministic: roots in definition order, each instance
/// followed by its occurrences' instances in occurrence order, bodies by instance, then by
/// representation id, then by item order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProductTree {
    /// Every definition, in definition entity order.
    pub parts: Vec<Part>,
    /// The expanded tree: a part used `n` times is `n` instances.
    pub instances: Vec<Instance>,
    /// Every body of every instance, with its placement. A body used `n` times is `n` entries.
    pub placed: Vec<PlacedBody>,
}

impl ProductTree {
    /// The placed bodies of the instance `instance`, in order.
    pub fn bodies_of(&self, instance: usize) -> impl Iterator<Item = &PlacedBody> {
        self.placed.iter().filter(move |p| p.instance == instance)
    }

    /// The placements of the body `body` (entity id), one per use, in order.
    pub fn placements_of(&self, body: u64) -> impl Iterator<Item = &PlacedBody> {
        self.placed.iter().filter(move |p| p.body == body)
    }
}

type Result<T> = core::result::Result<T, StepError>;

/// Length factors (millimetres per file unit) of the representations that list a body, or the
/// fault of the context that has none.
pub(crate) type Lengths = BTreeMap<u64, core::result::Result<f64, UnitsFault>>;

/// Expand `ps`. `lengths` has the length factor of each representation that lists a body: a
/// body is read in the units of the first representation (in id order) that lists it, so an
/// instance placed through another representation with a different length unit is scaled by
/// the ratio.
pub(crate) fn tree(ps: &ProductStructure, lengths: &Lengths) -> Result<ProductTree> {
    let nodes = ps.instances()?;
    let mut plain: Vec<Vec<usize>> = vec![Vec::new(); ps.representations.len()];
    // A relationship a CONTEXT_DEPENDENT_SHAPE_REPRESENTATION gives an occurrence places a
    // component; it does not say two representations are one shape, even without a
    // transformation.
    let placing: BTreeSet<usize> = ps
        .occurrences
        .iter()
        .flat_map(|o| o.placements.iter().copied())
        .collect();
    for (i, link) in ps.links.iter().enumerate() {
        if link.transformation.is_some() || placing.contains(&i) {
            continue;
        }
        if link.rep_1 < plain.len() && link.rep_2 < plain.len() {
            if let Some(l) = plain.get_mut(link.rep_1) {
                l.push(link.rep_2);
            }
            if let Some(l) = plain.get_mut(link.rep_2) {
                l.push(link.rep_1);
            }
        }
    }
    let mut read_in = BTreeMap::new();
    for rep in &ps.representations {
        for item in &rep.items {
            if matches!(item.kind, ItemKind::Body(_)) {
                read_in.entry(item.entity).or_insert(rep.entity);
            }
        }
    }
    let walk = Walk {
        ps,
        plain,
        lengths,
        read_in,
    };
    let parts = ps
        .definitions
        .iter()
        .map(|d| part(ps, d))
        .collect::<Vec<_>>();
    let mut instances: Vec<Instance> = Vec::with_capacity(nodes.len());
    let mut placed = Vec::new();
    for node in &nodes {
        let definition = ps
            .definitions
            .get(node.definition)
            .ok_or_else(|| missing(0))?;
        let used = node
            .occurrence
            .map(|o| {
                ps.occurrences
                    .get(o)
                    .ok_or_else(|| missing(definition.entity))
            })
            .transpose()?;
        let world = match (used, node.parent.map(|p| instances.get(p))) {
            (Some(o), Some(Some(parent))) => match parent.world {
                World::Placed(parent) => match walk.occurrence(o, definition) {
                    Ok(local) => World::Placed(parent * local),
                    Err(why) => World::Unplaced(why),
                },
                unplaced @ World::Unplaced(_) => unplaced,
            },
            (Some(o), _) => return Err(missing(o.entity)),
            _ => World::Placed(Affine::IDENTITY),
        };
        let instance = instances.len();
        let mut reps: Vec<usize> = definition
            .shapes
            .iter()
            .flat_map(|&s| walk.closure(s))
            .collect();
        reps.sort_unstable();
        reps.dedup();
        let from = placed.len();
        for rep in reps {
            walk.rep(rep, &world, &mut Vec::new(), 0, instance, &mut placed)?;
        }
        // One shape written in several representations lists its body in each: one placement.
        let mut seen = BTreeSet::new();
        let mut at = 0;
        placed.retain(|p| {
            let keep = at < from || seen.insert((p.body, p.mapped_by.clone()));
            at += 1;
            keep
        });
        instances.push(Instance {
            part: node.definition,
            by: used.map(|o| Use {
                entity: o.entity,
                id: o.id.clone(),
                name: o.name.clone(),
                reference_designator: o.reference_designator.clone(),
            }),
            parent: node.parent,
            world,
        });
    }
    Ok(ProductTree {
        parts,
        instances,
        placed,
    })
}

/// A reference the structure holds that points at nothing (a structure built by hand; `read`
/// never makes one).
fn missing(entity: u64) -> StepError {
    P21Error::entity(entity, EntityCause::Missing).into()
}

fn part(ps: &ProductStructure, d: &Definition) -> Part {
    Part {
        definition: d.entity,
        id: d.id.clone(),
        name: ps
            .products
            .get(d.product)
            .map_or_else(String::new, |p| p.name.clone()),
        description: d.description.clone(),
    }
}

struct Walk<'a> {
    ps: &'a ProductStructure,
    /// Representations related to each by a relationship without a transformation: the same
    /// shape written in another representation.
    plain: Vec<Vec<usize>>,
    lengths: &'a Lengths,
    /// The representation each body is read in (the first, in id order, that lists it).
    read_in: BTreeMap<u64, u64>,
}

impl Walk<'_> {
    /// `rep` and every representation reachable from it through plain relationships.
    fn closure(&self, rep: usize) -> Vec<usize> {
        let mut seen = vec![rep];
        let mut at = 0;
        while let Some(&r) = seen.get(at) {
            at += 1;
            for &next in self.plain.get(r).map_or(&[][..], Vec::as_slice) {
                if !seen.contains(&next) {
                    seen.push(next);
                }
            }
        }
        seen
    }

    /// The map that places the child of `occurrence` in its parent.
    fn occurrence(
        &self,
        occurrence: &Occurrence,
        child: &Definition,
    ) -> core::result::Result<Affine, Unplaced> {
        if occurrence.placements.is_empty() {
            return Err(Unplaced::NoRelationship {
                occurrence: occurrence.entity,
            });
        }
        let child_reps: Vec<usize> = child.shapes.iter().flat_map(|&s| self.closure(s)).collect();
        let links: Vec<_> = occurrence
            .placements
            .iter()
            .filter_map(|&l| self.ps.links.get(l))
            .filter(|l| l.transformation.is_some())
            .collect();
        if links.is_empty() {
            return Err(Unplaced::NoTransformation {
                link: occurrence
                    .placements
                    .first()
                    .and_then(|&l| self.ps.links.get(l))
                    .map_or(occurrence.entity, |l| l.entity),
            });
        }
        let mut maps = Vec::with_capacity(links.len());
        for link in &links {
            let (first_is_child, second_is_child) = (
                child_reps.contains(&link.rep_1),
                child_reps.contains(&link.rep_2),
            );
            if first_is_child == second_is_child {
                return Err(Unplaced::NotAboutChild { link: link.entity });
            }
            let degenerate = || Unplaced::Degenerate {
                entity: link.entity,
            };
            let map = match &link.transformation {
                Some(Transformation::Frame(f)) => frame(f).ok_or_else(degenerate)?,
                Some(Transformation::Items {
                    item_1,
                    item_2,
                    entity,
                }) => {
                    let (from, to) = if first_is_child {
                        (item_1, item_2)
                    } else {
                        (item_2, item_1)
                    };
                    let bad = || Unplaced::Degenerate { entity: *entity };
                    let from = frame(from).ok_or_else(bad)?;
                    let to = frame(to).ok_or_else(bad)?;
                    to * from.inverse().ok_or_else(bad)?
                }
                None => continue,
            };
            maps.push(map);
        }
        let (Some(&one), true) = (maps.first(), maps.iter().all(|m| Some(m) == maps.first()))
        else {
            return Err(Unplaced::Several {
                occurrence: occurrence.entity,
            });
        };
        Ok(one)
    }

    /// Lists the bodies of representation `rep`, placed by `world`, and those of the
    /// representations its mapped items place.
    fn rep(
        &self,
        rep: usize,
        world: &World,
        via: &mut Vec<u64>,
        depth: usize,
        instance: usize,
        out: &mut Vec<PlacedBody>,
    ) -> Result<()> {
        let r = self.ps.representations.get(rep).ok_or_else(|| missing(0))?;
        if depth > MAX_ASSEMBLY_DEPTH {
            return Err(P21Error::from(Limit::TooDeep {
                id: r.entity,
                limit: MAX_ASSEMBLY_DEPTH,
            })
            .into());
        }
        for item in &r.items {
            if out.len() >= MAX_VISITS {
                return Err(P21Error::from(Limit::TooManyInstances {
                    id: r.entity,
                    limit: MAX_VISITS,
                })
                .into());
            }
            match &item.kind {
                ItemKind::Body(kind) => out.push(PlacedBody {
                    body: item.entity,
                    kind: *kind,
                    representation: r.entity,
                    instance,
                    mapped_by: via.clone(),
                    world: self.sized(item.entity, r.entity, world),
                }),
                ItemKind::Mapped { map, target } => {
                    let Some(m) = self.ps.maps.get(*map) else {
                        return Err(P21Error::entity(item.entity, EntityCause::Missing).into());
                    };
                    let placed = match world {
                        World::Placed(w) => {
                            let local = frame(target)
                                .zip(frame(&m.origin))
                                .and_then(|(target, origin)| Some(target * origin.inverse()?));
                            local.map_or(
                                World::Unplaced(Unplaced::Degenerate {
                                    entity: item.entity,
                                }),
                                |local| World::Placed(*w * local),
                            )
                        }
                        unplaced @ World::Unplaced(_) => *unplaced,
                    };
                    via.push(item.entity);
                    for next in self.closure(m.representation) {
                        self.rep(next, &placed, via, depth + 1, instance, out)?;
                    }
                    via.pop();
                }
                // Other items (placements, curve sets, ...) hold no body.
                _ => {}
            }
        }
        Ok(())
    }

    /// `world` for a body listed by `rep`, scaled when the body was read in a representation
    /// with another length unit.
    fn sized(&self, body: u64, rep: u64, world: &World) -> World {
        let World::Placed(w) = world else {
            return *world;
        };
        let Some(&first) = self.read_in.get(&body) else {
            return *world;
        };
        if first == rep {
            return *world;
        }
        match (self.lengths.get(&rep), self.lengths.get(&first)) {
            (Some(Err(fault)), _) => World::Unplaced(Unplaced::Units {
                representation: rep,
                fault: *fault,
            }),
            (Some(Ok(here)), Some(Ok(there))) if here != there => {
                match Affine::uniform_scaling(Point3::ORIGIN, here / there) {
                    Ok(s) => World::Placed(*w * s),
                    Err(_) => World::Unplaced(Unplaced::Degenerate { entity: rep }),
                }
            }
            _ => *world,
        }
    }
}

/// The map from a file's frame to its parent: a placement's, or an operator's.
fn frame(f: &Frame) -> Option<Affine> {
    match f {
        Frame::Placement(p) => placement(p),
        Frame::Operator(o) => operator(o),
    }
}

fn vec(v: [f64; 3]) -> Vec3 {
    Vec3::new(v[0], v[1], v[2])
}

/// `AXIS2_PLACEMENT_3D`: Z from `axis` (+Z when absent), X the reference direction made square
/// to it (ISO 10303-42's default when absent). `None` for a direction with no length or a
/// reference along the axis.
fn placement(p: &Placement) -> Option<Affine> {
    let origin = Point3::new(p.origin[0], p.origin[1], p.origin[2]);
    let z = p
        .axis
        .map_or(Some(Vec3::Z), |a| Some(*vec(a).normalized()?))?;
    let hint = p
        .ref_direction
        .map_or_else(|| crate::geom::default_ref(z), vec);
    Some(scx_num::frame::Frame::new(origin, z, hint)?.to_affine())
}

/// `CARTESIAN_TRANSFORMATION_OPERATOR_3D` (and `_NON_UNIFORM`): ISO 10303-42's `base_axis`
/// (Z from `axis3`; X from `axis1` made square to Z; Y from `axis2` made square to both, so a
/// mirrored operator stays one), each scaled: `scale` (1 when absent), `scale2` and `scale3`
/// (`scale` when absent). `None` for a direction with no length, a scale of zero or not
/// finite.
fn operator(o: &Operator) -> Option<Affine> {
    let z = o
        .axis3
        .map_or(Some(Vec3::Z), |a| Some(*vec(a).normalized()?))?;
    let square = |v: Vec3, to: &[Vec3]| {
        let v = to.iter().fold(v, |v, &t| v - t * v.dot(t));
        v.normalized().map(|u| *u)
    };
    let x = square(
        o.axis1.map_or_else(|| crate::geom::default_ref(z), vec),
        &[z],
    )?;
    let y = square(o.axis2.map_or(Vec3::new(0.0, 1.0, 0.0), vec), &[z, x])?;
    let s = o.scale.unwrap_or(1.0);
    let (sx, sy, sz) = (s, o.scale2.unwrap_or(s), o.scale3.unwrap_or(s));
    if ![sx, sy, sz].iter().all(|v| v.is_finite() && *v != 0.0) {
        return None;
    }
    let origin = Point3::new(o.origin[0], o.origin[1], o.origin[2]);
    Some(Affine::from_axes(x * sx, y * sy, z * sz, origin))
}
