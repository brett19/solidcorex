// Derived from solidcore `solidcore/crates/solidcore-record/src/step/product.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8 (its own origin: SolidCraft's
// `crates/kernel/src/step_in/mod.rs`, MIT OR Apache-2.0).
// Upstream record (verbatim from that file): The product structure of a STEP file (ISO
// 10303-41 product definitions, ISO 10303-43 representations, ISO 10303-44 assembly
// structure, ISO 10303-46 presentation) as plain data: the products and their definitions,
// the shape representations with their items and contexts, the relationships between
// representations (with or without a transformation), the assembly occurrences, and the
// colours the presentation styles give items. Read from a parsed [`Exchange`] by [`read`].
// Changes: every refusal is a typed `P21Error` naming the instance at fault (was a `String`
// with the path of entities that led to it): `EntityCause::Missing`, `WrongType`, `BadParam`
// (with the parameter's position in its record), `TooDeep`, and the new `Cycle` and
// `TooManyInstances`; so `entity`, `reference`, `colour_number`, `optional_direction`,
// `optional_scale`, `refs`, `colour`, `surface_style`, `styled_item`, `index_of`, `frame`,
// `read`, `impl ProductStructure::instances` and `impl ProductStructure::expand` take the
// instance and the position instead of a label; new for that: `refuse`, `bad`, `not_a`,
// `representation_of`, `impl ProductStructure::entity_of` and `Expansion` (the root whose
// expansion is too large). `height` and `check_nesting` became `impl Graph<'_>::height` and
// `impl Graph<'_>::check_nesting`, the graph carrying each node's entity id (new:
// `impl Graph<'_>::refuse`). `link_params` returns where its attributes are (`LinkParams`)
// instead of the parameters; `operator_labels` is new, so `frame` reads the operator by its
// record positions and `operator_attributes` slices by it; `representation_params` reads
// `Instance` (the pin's `Entity`); `add_styles` lost its `nested` flag to the new
// `add_style`; the relationship transformation of `read` became `transformation`; the
// let-chains in `read` and `add_styles` are nested `if let`s (edition 2021); `in_id_order`
// is new: instances are read in id order across DATA sections, the first definition of an
// id winning, as `Exchange::get` finds it. The expansion node is `Node` (the pin's
// `Instance`, a name this crate gives the exchange's instances). Module doc without issue
// references. A refusal met through an occurrence, a mapped item, a representation map, a
// relationship, a transformation operator or a styled item is wrapped in
// `P21Error::Within` naming it (the pin prefixed the message: "occurrence #90: …");
// `read_frame` (new) is `frame`'s body, which wraps it.
//! The product structure of a STEP file as plain data: the products and their definitions
//! (ISO 10303-41), the shape representations with their items and contexts (ISO 10303-43),
//! the relationships between representations with or without a transformation and the
//! assembly occurrences (ISO 10303-44), and the colours the presentation styles give items
//! (ISO 10303-46). Read from a parsed [`Exchange`] by [`read`].
//!
//! Nothing is composed: an occurrence's placement is the two placements of its
//! `ITEM_DEFINED_TRANSFORMATION` (or the operator of its relationship) as the file gives them,
//! and a mapped item keeps its map's origin and its own target. Forming a placement's matrix,
//! inverting it and multiplying placements down the tree are the kernel's. Lengths are in
//! millimetres (each times its representation context's length unit, the only arithmetic
//! here); directions keep the ratios the file writes; colours keep the numbers the file writes
//! (not clamped, not converted between linear and sRGB), and a pre-defined colour's RGB comes
//! from ISO 10303-46's fixed table of names.
//!
//! Cross references are indices into the vectors of [`ProductStructure`], each vector in
//! entity id order. Bodies and the items a style colours are referred to by their entity ids:
//! their topology is read elsewhere.
//!
//! Input is hostile: every reference the structure follows must exist and be of the expected
//! kind, an assembly or a chain of mapped items may nest at most [`MAX_ASSEMBLY_DEPTH`] deep
//! and may not cycle, and expanding the assembly tree ([`ProductStructure::instances`]) stops
//! with an error past [`MAX_VISITS`] instances. Reading is linear in the file: a sub-assembly
//! shared at every level is read once, and only its expansion grows with its uses. Nothing is
//! pre-allocated from a count in the file, and every refusal is a typed [`P21Error`] naming
//! the instance at fault, never a panic.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::EntityCause;
use crate::exchange::{Exchange, Instance, Param};
use crate::part42::geom::{Placement, P3};
use crate::part42::read::{Reader, MAX_COORD};
use crate::units::{self, Context, Units};
use crate::P21Error;

type Result<T> = core::result::Result<T, P21Error>;

/// Deepest assembly nesting, and deepest chain of mapped items, read: the root is at depth 0,
/// so at most 33 levels.
pub const MAX_ASSEMBLY_DEPTH: usize = 32;
/// Most instances [`ProductStructure::instances`] expands (a sub-assembly used twice at every
/// level would otherwise grow exponentially).
pub const MAX_VISITS: usize = 100_000;

/// The product structure of a file. Every vector is in entity id order; cross references are
/// indices into them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProductStructure {
    /// Every `PRODUCT`.
    pub products: Vec<Product>,
    /// Every `PRODUCT_DEFINITION` (and `PRODUCT_DEFINITION_WITH_ASSOCIATED_DOCUMENTS`).
    pub definitions: Vec<Definition>,
    /// The representations the structure reaches: those a definition's shape uses, those a
    /// relationship or a representation map names, and every representation that lists a body.
    pub representations: Vec<Representation>,
    /// Every `REPRESENTATION_MAP` a mapped item uses.
    pub maps: Vec<RepresentationMap>,
    /// Every relationship between two representations.
    pub links: Vec<Link>,
    /// Every assembly occurrence.
    pub occurrences: Vec<Occurrence>,
    /// The definitions no occurrence uses as its component, in id order.
    pub roots: Vec<usize>,
    /// Every styled item.
    pub styles: Vec<StyledItem>,
}

/// `PRODUCT`.
#[derive(Clone, Debug, PartialEq)]
pub struct Product {
    /// The entity id.
    pub entity: u64,
    /// Its `id` attribute.
    pub id: String,
    /// Its `name`.
    pub name: String,
    /// Its `description` (empty when `$`).
    pub description: String,
}

/// `PRODUCT_DEFINITION` (or `PRODUCT_DEFINITION_WITH_ASSOCIATED_DOCUMENTS`): a version of a
/// product, with its shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    /// The entity id.
    pub entity: u64,
    /// Its `id` attribute.
    pub id: String,
    /// Its `description` (empty when `$`).
    pub description: String,
    /// The `PRODUCT_DEFINITION_FORMATION` it defines (entity id).
    pub formation: u64,
    /// The product of that formation (index into [`ProductStructure::products`]).
    pub product: usize,
    /// The representations a `SHAPE_DEFINITION_REPRESENTATION` gives its shape (indices into
    /// [`ProductStructure::representations`]), in the order of those entities.
    pub shapes: Vec<usize>,
}

/// A representation (`SHAPE_REPRESENTATION`, `ADVANCED_BREP_SHAPE_REPRESENTATION`, …).
#[derive(Clone, Debug, PartialEq)]
pub struct Representation {
    /// The entity id.
    pub entity: u64,
    /// The entity type (the first record's, for a complex instance).
    pub kind: String,
    /// Its `name`.
    pub name: String,
    /// Its items, in the file's order.
    pub items: Vec<Item>,
    /// Its representation context (entity id), if the file gives one.
    pub context: Option<u64>,
    /// That context's units and uncertainty (millimetres and radians when it names none).
    pub units: Context,
}

/// An item of a representation.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The entity id.
    pub entity: u64,
    /// Its `name` (empty when it has none).
    pub name: String,
    /// What it is.
    pub kind: ItemKind,
}

/// What a representation item is.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ItemKind {
    /// A body, read by the topology reader from its entity id.
    Body(BodyKind),
    /// `MAPPED_ITEM`: the map's representation, placed.
    Mapped {
        /// The map (index into [`ProductStructure::maps`]).
        map: usize,
        /// Its `mapping_target`, in this representation's units: where the map's origin goes.
        target: Frame,
    },
    /// Anything else (a placement, a curve set, …): its entity type.
    Other(String),
}

/// The kinds of body a representation lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyKind {
    /// `MANIFOLD_SOLID_BREP`.
    ManifoldSolidBrep,
    /// `BREP_WITH_VOIDS`.
    BrepWithVoids,
    /// `FACETED_BREP`.
    FacetedBrep,
    /// `SHELL_BASED_SURFACE_MODEL`.
    ShellBasedSurfaceModel,
}

/// `REPRESENTATION_MAP`: a representation that mapped items place.
#[derive(Clone, Debug, PartialEq)]
pub struct RepresentationMap {
    /// The entity id.
    pub entity: u64,
    /// Its `mapping_origin`, in the mapped representation's units.
    pub origin: Frame,
    /// The mapped representation (index into [`ProductStructure::representations`]).
    pub representation: usize,
}

/// A placement as the file gives it.
///
/// ISO 10303-43 places a mapped item (and an `ITEM_DEFINED_TRANSFORMATION`) by the map that
/// takes the origin frame onto the target frame. Forming it is the kernel's.
#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    /// `AXIS2_PLACEMENT_3D`, or an `AXIS2_PLACEMENT_2D` (the placement in the XY plane it is).
    Placement(Placement),
    /// `CARTESIAN_TRANSFORMATION_OPERATOR_3D` or its `_NON_UNIFORM` subtype.
    Operator(Operator),
}

/// `CARTESIAN_TRANSFORMATION_OPERATOR_3D` (and `_NON_UNIFORM`), as the file writes it.
///
/// ISO 10303-42 makes the operator's axes from the three directions (an absent one from the
/// others and the global axes) and scales them by `scale` (absent: 1), `scale2` and `scale3`
/// (absent: `scale`). That construction is the kernel's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Operator {
    /// `axis1` as written (not normalised).
    pub axis1: Option<P3>,
    /// `axis2` as written.
    pub axis2: Option<P3>,
    /// `axis3` as written.
    pub axis3: Option<P3>,
    /// `local_origin`, in millimetres.
    pub origin: P3,
    /// `scale` as written (never zero).
    pub scale: Option<f64>,
    /// `scale2` of a non-uniform operator.
    pub scale2: Option<f64>,
    /// `scale3` of a non-uniform operator.
    pub scale3: Option<f64>,
}

/// A relationship between two representations: `SHAPE_REPRESENTATION_RELATIONSHIP` (or
/// `REPRESENTATION_RELATIONSHIP`), with or without `REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION`.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    /// The entity id.
    pub entity: u64,
    /// `rep_1` (index into [`ProductStructure::representations`]).
    pub rep_1: usize,
    /// `rep_2`.
    pub rep_2: usize,
    /// The transformation, when the relationship has one.
    pub transformation: Option<Transformation>,
}

/// The `transformation_operator` of a relationship with transformation.
#[derive(Clone, Debug, PartialEq)]
pub enum Transformation {
    /// `ITEM_DEFINED_TRANSFORMATION`: `item_1` (in `rep_1`'s units) is placed onto `item_2` (in
    /// `rep_2`'s units).
    ///
    /// `item_1` is always the item of `rep_1` and `item_2` that of `rep_2`. A file that lists
    /// `transform_item_1` in `rep_2`'s items and `transform_item_2` in `rep_1`'s (a
    /// non-conformant writing some exporters use) has its two items swapped on reading.
    Items {
        /// The entity id.
        entity: u64,
        /// The item that lives in `rep_1`, in `rep_1`'s units.
        item_1: Frame,
        /// The item that lives in `rep_2`, in `rep_2`'s units.
        item_2: Frame,
    },
    /// A placement or an operator, in `rep_2`'s units.
    Frame(Frame),
}

/// The kinds of assembly occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OccurrenceKind {
    /// `NEXT_ASSEMBLY_USAGE_OCCURRENCE`.
    NextAssemblyUsage,
    /// `ASSEMBLY_COMPONENT_USAGE`.
    AssemblyComponentUsage,
    /// `SPECIFIED_HIGHER_USAGE_OCCURRENCE`.
    SpecifiedHigherUsage,
}

/// An assembly occurrence: one use of a definition (the child) in another (the parent).
#[derive(Clone, Debug, PartialEq)]
pub struct Occurrence {
    /// The entity id.
    pub entity: u64,
    /// The entity type.
    pub kind: OccurrenceKind,
    /// Its `id` attribute.
    pub id: String,
    /// Its `name`.
    pub name: String,
    /// Its `description` (empty when `$`).
    pub description: String,
    /// `relating_product_definition` (index into [`ProductStructure::definitions`]).
    pub parent: usize,
    /// `related_product_definition`.
    pub child: usize,
    /// Its `reference_designator`, when it has one.
    pub reference_designator: Option<String>,
    /// The relationships a `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION` gives it (indices into
    /// [`ProductStructure::links`]), in the order of those entities.
    pub placements: Vec<usize>,
}

/// One node of the expanded assembly tree ([`ProductStructure::instances`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    /// The definition placed (index into [`ProductStructure::definitions`]).
    pub definition: usize,
    /// The occurrence that places it (`None` for a root).
    pub occurrence: Option<usize>,
    /// The node of its parent (index into the expansion; `None` for a root).
    pub parent: Option<usize>,
}

/// `STYLED_ITEM` (or `OVER_RIDING_STYLED_ITEM`): the styles of one item.
#[derive(Clone, Debug, PartialEq)]
pub struct StyledItem {
    /// The entity id.
    pub entity: u64,
    /// Its `name`.
    pub name: String,
    /// The item it styles (entity id: a body, a face, a mapped item, …).
    pub item: u64,
    /// The styled item it overrides (entity id), for an `OVER_RIDING_STYLED_ITEM`.
    pub overrides: Option<u64>,
    /// Its surface styles (`SURFACE_STYLE_USAGE`), in the file's order.
    pub surface: Vec<SurfaceStyle>,
    /// The colours of its curve styles (`CURVE_STYLE`), in the file's order.
    pub curve: Vec<Colour>,
}

/// Which side of a surface a style is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// `.POSITIVE.`
    Positive,
    /// `.NEGATIVE.`
    Negative,
    /// `.BOTH.` (or anything else).
    Both,
}

/// `SURFACE_STYLE_USAGE` and what its `SURFACE_SIDE_STYLE` says about colour.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceStyle {
    /// The side.
    pub side: Side,
    /// The first `FILL_AREA_STYLE_COLOUR` of its `SURFACE_STYLE_FILL_AREA`.
    pub fill: Option<Colour>,
    /// The colour of its first `SURFACE_STYLE_RENDERING` (or `_WITH_PROPERTIES`).
    pub rendering: Option<Colour>,
    /// The first `SURFACE_STYLE_TRANSPARENT` of that rendering, as written (0 opaque, 1
    /// invisible).
    pub transparency: Option<f64>,
}

/// A colour as the file gives it.
#[derive(Clone, Debug, PartialEq)]
pub enum Colour {
    /// `COLOUR_RGB`: the three numbers as written (ISO 10303-46: each in 0..1; not clamped here).
    Rgb {
        /// Its `name`.
        name: String,
        /// Red, green and blue.
        rgb: [f64; 3],
    },
    /// `DRAUGHTING_PRE_DEFINED_COLOUR`: its name, and the RGB ISO 10303-46 gives that name
    /// (`None` for a name it does not define).
    PreDefined {
        /// Its `name`.
        name: String,
        /// Red, green and blue of the name.
        rgb: Option<[f64; 3]>,
    },
}

/// The RGB of a pre-defined colour name (ISO 10303-46, `pre_defined_colour`).
#[must_use]
pub fn pre_defined_rgb(name: &str) -> Option<[f64; 3]> {
    Some(match name {
        "red" => [1.0, 0.0, 0.0],
        "green" => [0.0, 1.0, 0.0],
        "blue" => [0.0, 0.0, 1.0],
        "yellow" => [1.0, 1.0, 0.0],
        "magenta" => [1.0, 0.0, 1.0],
        "cyan" => [0.0, 1.0, 1.0],
        "black" => [0.0, 0.0, 0.0],
        "white" => [1.0, 1.0, 1.0],
        _ => return None,
    })
}

/// What a shape definition representation or a context dependent shape representation names.
const PROPERTY: &str = "PRODUCT_DEFINITION_SHAPE or PROPERTY_DEFINITION";

fn refuse<T>(id: u64, cause: EntityCause) -> Result<T> {
    Err(P21Error::entity(id, cause))
}

fn bad(id: u64, index: usize, expected: &'static str) -> P21Error {
    P21Error::entity(id, EntityCause::BadParam { index, expected })
}

fn entity(ex: &Exchange, id: u64) -> Result<&Instance> {
    ex.get(id)
        .ok_or_else(|| P21Error::entity(id, EntityCause::Missing))
}

/// The refusal of `#id` where `what` was needed: missing, or of another type.
fn not_a(ex: &Exchange, id: u64, what: &'static str) -> P21Error {
    match ex.get(id) {
        None => P21Error::entity(id, EntityCause::Missing),
        Some(e) => P21Error::entity(
            id,
            EntityCause::WrongType {
                expected: what,
                found: e.name().to_owned(),
            },
        ),
    }
}

/// Parameter `index` of `#id`'s record `p`, a reference.
fn reference(id: u64, p: &[Param], index: usize) -> Result<u64> {
    p.get(index)
        .and_then(Param::as_ref_id)
        .ok_or_else(|| bad(id, index, "an entity reference"))
}

/// A string attribute; `$`, or anything that is not a string, is empty.
fn text(p: Option<&Param>) -> String {
    p.and_then(Param::as_str).unwrap_or("").to_string()
}

fn colour_number(id: u64, p: &[Param], index: usize) -> Result<f64> {
    p.get(index)
        .and_then(Param::as_f64)
        .filter(|x| x.abs() < MAX_COORD)
        .ok_or_else(|| bad(id, index, "a number smaller than 1e9 in size"))
}

/// The name, items and context of a representation entity: a simple `…REPRESENTATION` or
/// `DRAUGHTING_MODEL` (a representation subtype a relationship may name), or a complex
/// instance's `REPRESENTATION` record, else its first record named `…REPRESENTATION` with three
/// parameters.
fn representation_params(e: &Instance) -> Option<&[Param]> {
    if e.is_complex() {
        e.record("REPRESENTATION").or_else(|| {
            e.records()
                .iter()
                .find(|r| r.name.ends_with("REPRESENTATION") && r.params.len() >= 3)
                .map(|r| r.params.as_slice())
        })
    } else if e.name().ends_with("REPRESENTATION") || e.name() == "DRAUGHTING_MODEL" {
        Some(e.params())
    } else {
        None
    }
}

/// [`representation_params`] of `#id`, or its refusal.
fn representation_of(id: u64, e: &Instance) -> Result<&[Param]> {
    representation_params(e).ok_or_else(|| {
        P21Error::entity(
            id,
            EntityCause::WrongType {
                expected: "a representation",
                found: e.name().to_owned(),
            },
        )
    })
}

fn body_kind(name: &str) -> Option<BodyKind> {
    match name {
        "MANIFOLD_SOLID_BREP" => Some(BodyKind::ManifoldSolidBrep),
        "BREP_WITH_VOIDS" => Some(BodyKind::BrepWithVoids),
        "FACETED_BREP" => Some(BodyKind::FacetedBrep),
        "SHELL_BASED_SURFACE_MODEL" => Some(BodyKind::ShellBasedSurfaceModel),
        _ => None,
    }
}

fn occurrence_kind(name: &str) -> Option<OccurrenceKind> {
    match name {
        "NEXT_ASSEMBLY_USAGE_OCCURRENCE" => Some(OccurrenceKind::NextAssemblyUsage),
        "ASSEMBLY_COMPONENT_USAGE" => Some(OccurrenceKind::AssemblyComponentUsage),
        "SPECIFIED_HIGHER_USAGE_OCCURRENCE" => Some(OccurrenceKind::SpecifiedHigherUsage),
        _ => None,
    }
}

/// Where a relationship's attributes are: the record holding `rep_1` (at 2) and `rep_2` (at
/// 3), and the record and position of its transformation operator, when it has one.
struct LinkParams<'a> {
    reps: &'a [Param],
    operator: Option<(&'a [Param], usize)>,
}

/// The [`LinkParams`] of a relationship between representations, or `None` when `e` is not
/// one.
fn link_params(e: &Instance) -> Option<LinkParams<'_>> {
    if e.is_complex() {
        let reps = e.record("REPRESENTATION_RELATIONSHIP")?;
        let operator = e
            .record("REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION")
            .filter(|r| !r.is_empty())
            .map(|r| (r, 0));
        return Some(LinkParams { reps, operator });
    }
    let p = e.params();
    match e.name() {
        "SHAPE_REPRESENTATION_RELATIONSHIP" | "REPRESENTATION_RELATIONSHIP" => Some(LinkParams {
            reps: p,
            operator: None,
        }),
        "REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION" => Some(LinkParams {
            reps: p,
            operator: p.get(4).map(|_| (p, 4)),
        }),
        _ => None,
    }
}

/// An optional direction of an operator: `$` or `*` is none.
fn optional_direction(
    reader: &Reader<'_>,
    id: u64,
    p: &[Param],
    index: usize,
) -> Result<Option<P3>> {
    match p.get(index) {
        None | Some(Param::Unset | Param::Derived) => Ok(None),
        Some(Param::Ref(d)) => Ok(Some(reader.direction(*d)?)),
        Some(_) => Err(bad(id, index, "a direction")),
    }
}

/// An optional scale of an operator: `$` or `*` is none; zero is refused.
fn optional_scale(id: u64, p: &[Param], index: usize) -> Result<Option<f64>> {
    match p.get(index) {
        None | Some(Param::Unset | Param::Derived) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|x| *x != 0.0 && x.abs() < MAX_COORD)
            .map(Some)
            .ok_or_else(|| {
                bad(
                    id,
                    index,
                    "a number other than zero, smaller than 1e9 in size",
                )
            }),
    }
}

/// How many labels a `CARTESIAN_TRANSFORMATION_OPERATOR_3D` (or `_NON_UNIFORM`) writes before
/// `axis1`; see [`operator_attributes`].
fn operator_labels(e: &Instance) -> usize {
    let p = e.params();
    let with_description = if e.name().ends_with("NON_UNIFORM") {
        9
    } else {
        7
    };
    match p.get(1) {
        Some(Param::Ref(_)) => 1,
        Some(Param::Unset | Param::Derived) if p.len() + 1 == with_description => 1,
        _ => 2,
    }
}

/// The attributes of a `CARTESIAN_TRANSFORMATION_OPERATOR_3D` (or `_NON_UNIFORM`) from `axis1`
/// on: `axis1, axis2, local_origin, scale, axis3` (then `scale2, scale3`).
///
/// Writers put one or two labels before them. The operator inherits a name from
/// `representation_item` and a name and an optional description from
/// `functionally_defined_transformation` (ISO 10303-43). Most files write a name and the
/// description (`'','',#1,...`); some write the name only (`'',#1,...`). The second parameter
/// says which: a reference is `axis1`, anything else the description. When it is unset (`$` or
/// `*`), the count says: the form without the description has one parameter fewer.
#[must_use]
pub fn operator_attributes(e: &Instance) -> &[Param] {
    e.params().get(operator_labels(e)..).unwrap_or_default()
}

/// A placement or a transformation operator, in `units`.
fn frame(ex: &Exchange, id: u64, units: Units) -> Result<Frame> {
    read_frame(ex, id, units).map_err(|e| e.within(id))
}

fn read_frame(ex: &Exchange, id: u64, units: Units) -> Result<Frame> {
    let e = entity(ex, id)?;
    let reader = Reader::new(ex, units);
    match e.name() {
        "CARTESIAN_TRANSFORMATION_OPERATOR_3D"
        | "CARTESIAN_TRANSFORMATION_OPERATOR_3D_NON_UNIFORM" => {
            // Positions are the record's: `axis1` is at `at`.
            let at = operator_labels(e);
            let p = e.params();
            let non_uniform = e.name().ends_with("NON_UNIFORM");
            let scale = |k| {
                if non_uniform {
                    optional_scale(id, p, at + k)
                } else {
                    Ok(None)
                }
            };
            Ok(Frame::Operator(Operator {
                axis1: optional_direction(&reader, id, p, at)?,
                axis2: optional_direction(&reader, id, p, at + 1)?,
                origin: reader.point(reference(id, p, at + 2)?)?,
                scale: optional_scale(id, p, at + 3)?,
                axis3: optional_direction(&reader, id, p, at + 4)?,
                scale2: scale(5)?,
                scale3: scale(6)?,
            }))
        }
        "AXIS2_PLACEMENT_3D" | "AXIS2_PLACEMENT_2D" => Ok(Frame::Placement(reader.frame(id)?)),
        n => refuse(
            id,
            EntityCause::WrongType {
                expected: "a placement or a cartesian transformation operator",
                found: n.to_owned(),
            },
        ),
    }
}

/// A colour entity: `COLOUR_RGB` or `DRAUGHTING_PRE_DEFINED_COLOUR`; any other kind is none.
fn colour(ex: &Exchange, id: u64) -> Result<Option<Colour>> {
    let e = entity(ex, id)?;
    let p = e.params();
    match e.name() {
        "COLOUR_RGB" => Ok(Some(Colour::Rgb {
            name: text(p.first()),
            rgb: [
                colour_number(id, p, 1)?,
                colour_number(id, p, 2)?,
                colour_number(id, p, 3)?,
            ],
        })),
        "DRAUGHTING_PRE_DEFINED_COLOUR" => {
            let name = text(p.first());
            let rgb = pre_defined_rgb(&name);
            Ok(Some(Colour::PreDefined { name, rgb }))
        }
        _ => Ok(None),
    }
}

/// The references of list parameter `index` of `#id`'s record `p`.
fn refs(id: u64, p: &[Param], index: usize) -> Result<impl Iterator<Item = u64> + '_> {
    let list = p
        .get(index)
        .and_then(Param::as_list)
        .ok_or_else(|| bad(id, index, "a list"))?;
    Ok(list.iter().filter_map(Param::as_ref_id))
}

/// A `SURFACE_STYLE_USAGE` `#id`'s side and its side style's colours.
fn surface_style(ex: &Exchange, id: u64, usage: &Instance) -> Result<SurfaceStyle> {
    let p = usage.params();
    let side = match p.first().and_then(Param::as_enum) {
        Some("POSITIVE") => Side::Positive,
        Some("NEGATIVE") => Side::Negative,
        _ => Side::Both,
    };
    let mut out = SurfaceStyle {
        side,
        fill: None,
        rendering: None,
        transparency: None,
    };
    let side_id = reference(id, p, 1)?;
    let side_style = entity(ex, side_id)?;
    if side_style.name() != "SURFACE_SIDE_STYLE" {
        return Ok(out);
    }
    for el in refs(side_id, side_style.params(), 1)? {
        let element = entity(ex, el)?;
        let ep = element.params();
        match element.name() {
            "SURFACE_STYLE_FILL_AREA" if out.fill.is_none() => {
                let fill_id = reference(el, ep, 0)?;
                let fill = entity(ex, fill_id)?;
                for c in refs(fill_id, fill.params(), 1)? {
                    let fc = entity(ex, c)?;
                    if fc.name() == "FILL_AREA_STYLE_COLOUR" {
                        out.fill = colour(ex, reference(c, fc.params(), 1)?)?;
                        if out.fill.is_some() {
                            break;
                        }
                    }
                }
            }
            "SURFACE_STYLE_RENDERING" | "SURFACE_STYLE_RENDERING_WITH_PROPERTIES"
                if out.rendering.is_none() =>
            {
                out.rendering = colour(ex, reference(el, ep, 1)?)?;
                if element.name() == "SURFACE_STYLE_RENDERING_WITH_PROPERTIES" {
                    for prop in refs(el, ep, 2)? {
                        let pe = entity(ex, prop)?;
                        if pe.name() == "SURFACE_STYLE_TRANSPARENT" {
                            out.transparency = Some(colour_number(prop, pe.params(), 0)?);
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The styles `#id` gives, added to `out`: a `PRESENTATION_STYLE_ASSIGNMENT` (or
/// `PRESENTATION_STYLE_BY_CONTEXT`) gives its styles, followed one level; a style the styled
/// item lists directly gives itself.
fn add_styles(ex: &Exchange, id: u64, out: &mut StyledItem) -> Result<()> {
    let e = entity(ex, id)?;
    match e.name() {
        "PRESENTATION_STYLE_ASSIGNMENT" | "PRESENTATION_STYLE_BY_CONTEXT" => {
            for s in refs(id, e.params(), 0)? {
                add_style(ex, s, entity(ex, s)?, out)?;
            }
            Ok(())
        }
        _ => add_style(ex, id, e, out),
    }
}

/// A surface style usage or curve style `#id`, added to `out`; anything else adds nothing.
fn add_style(ex: &Exchange, id: u64, e: &Instance, out: &mut StyledItem) -> Result<()> {
    match e.name() {
        "SURFACE_STYLE_USAGE" => out.surface.push(surface_style(ex, id, e)?),
        "CURVE_STYLE" => {
            if let Some(c) = e.params().get(3).and_then(Param::as_ref_id) {
                if let Some(c) = colour(ex, c)? {
                    out.curve.push(c);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn styled_item(ex: &Exchange, id: u64, e: &Instance) -> Result<Option<StyledItem>> {
    let (p, overrides) = match e.name() {
        "STYLED_ITEM" if !e.is_complex() => (e.params(), None),
        "OVER_RIDING_STYLED_ITEM" | "CONTEXT_DEPENDENT_OVER_RIDING_STYLED_ITEM" => {
            let p = e.params();
            (p, p.get(3).and_then(Param::as_ref_id))
        }
        _ => return Ok(None),
    };
    let item = reference(id, p, 2)?;
    entity(ex, item)?;
    if let Some(o) = overrides {
        entity(ex, o)?;
    }
    let mut out = StyledItem {
        entity: id,
        name: text(p.first()),
        item,
        overrides,
        surface: Vec::new(),
        curve: Vec::new(),
    };
    for s in refs(id, p, 1)? {
        add_styles(ex, s, &mut out)?;
    }
    Ok(Some(out))
}

/// Index of `#id` in an id → index map, or the refusal of `#id` where `what` was needed.
fn index_of(
    ex: &Exchange,
    map: &BTreeMap<u64, usize>,
    id: u64,
    what: &'static str,
) -> Result<usize> {
    map.get(&id).copied().ok_or_else(|| not_a(ex, id, what))
}

/// A graph whose nodes are entities: `children[n]` the nodes below node `n`, `entities[n]` its
/// entity id.
struct Graph<'a> {
    children: &'a [Vec<usize>],
    entities: &'a [u64],
}

impl Graph<'_> {
    fn refuse(&self, n: usize, cause: EntityCause) -> P21Error {
        P21Error::entity(self.entities.get(n).copied().unwrap_or(0), cause)
    }

    /// The height of node `n` (the longest path below it), checking that no path from a root
    /// is longer than [`MAX_ASSEMBLY_DEPTH`] and that the graph has no cycle. Recursion is
    /// bounded: `depth` grows by one per level and is refused past the bound. Each node's
    /// height is computed once, so the walk is linear in the graph.
    fn height(
        &self,
        n: usize,
        depth: usize,
        memo: &mut [Option<usize>],
        on_path: &mut [bool],
    ) -> Result<usize> {
        let too_deep = EntityCause::TooDeep {
            limit: MAX_ASSEMBLY_DEPTH,
        };
        if depth > MAX_ASSEMBLY_DEPTH {
            return Err(self.refuse(n, too_deep));
        }
        if on_path.get(n).copied().unwrap_or(false) {
            return Err(self.refuse(n, EntityCause::Cycle));
        }
        if let Some(Some(h)) = memo.get(n).copied() {
            if depth + h > MAX_ASSEMBLY_DEPTH {
                return Err(self.refuse(n, too_deep));
            }
            return Ok(h);
        }
        if let Some(f) = on_path.get_mut(n) {
            *f = true;
        }
        let mut h = 0;
        for &c in self.children.get(n).map_or(&[][..], Vec::as_slice) {
            h = h.max(self.height(c, depth + 1, memo, on_path)? + 1);
        }
        if let Some(f) = on_path.get_mut(n) {
            *f = false;
        }
        if let Some(m) = memo.get_mut(n) {
            *m = Some(h);
        }
        Ok(h)
    }

    /// Checks the graph for cycles and depth from every node.
    fn check_nesting(&self) -> Result<()> {
        let mut memo = vec![None; self.children.len()];
        let mut on_path = vec![false; self.children.len()];
        for n in 0..self.children.len() {
            self.height(n, 0, &mut memo, &mut on_path)?;
        }
        Ok(())
    }
}

/// Every instance of `ex` in id order (of an id two DATA sections define, the first, as
/// [`Exchange::get`] finds it).
fn in_id_order(ex: &Exchange) -> Vec<(u64, &Instance)> {
    if ex.sections.len() <= 1 {
        return ex.instances().collect();
    }
    let mut all: BTreeMap<u64, &Instance> = BTreeMap::new();
    for (id, e) in ex.instances() {
        all.entry(id).or_insert(e);
    }
    all.into_iter().collect()
}

/// The transformation of relationship `#id` between `rep_1` and `rep_2` (indices into
/// `out.representations`), whose operator is parameter `index` of record `p`.
fn transformation(
    ex: &Exchange,
    out: &ProductStructure,
    id: u64,
    (rep_1, rep_2): (usize, usize),
    operator: Option<(&[Param], usize)>,
) -> Result<Option<Transformation>> {
    let units_of = |r: usize| {
        out.representations
            .get(r)
            .map_or_else(Units::default, |r| r.units.units)
    };
    let Some((p, index)) = operator else {
        return Ok(None);
    };
    let op = reference(id, p, index)?;
    let oe = entity(ex, op)?;
    if oe.name() != "ITEM_DEFINED_TRANSFORMATION" {
        return Ok(Some(Transformation::Frame(frame(ex, op, units_of(rep_2))?)));
    }
    let p = oe.params();
    let written_1 = reference(op, p, 2)?;
    let written_2 = reference(op, p, 3)?;
    // Some exporters write the items the other way round from the representations
    // (`transform_item_1` listed in `rep_2`); where the items live decides.
    let holds = |r: usize, item: u64| {
        out.representations
            .get(r)
            .is_some_and(|r| r.items.iter().any(|i| i.entity == item))
    };
    let swapped = (holds(rep_2, written_1) && !holds(rep_1, written_1))
        || (holds(rep_1, written_2) && !holds(rep_2, written_2));
    let (item_1, item_2) = if swapped {
        (written_2, written_1)
    } else {
        (written_1, written_2)
    };
    Ok(Some(Transformation::Items {
        entity: op,
        item_1: frame(ex, item_1, units_of(rep_1)).map_err(|e| e.within(op))?,
        item_2: frame(ex, item_2, units_of(rep_2)).map_err(|e| e.within(op))?,
    }))
}

/// Reads the product structure of `ex`.
///
/// # Errors
///
/// [`P21Error`] naming the instance at fault: a reference the structure follows that is
/// missing or of the wrong kind ([`EntityCause::Missing`], [`EntityCause::WrongType`]), a
/// parameter of the wrong kind ([`EntityCause::BadParam`]), an assembly or chain of mapped
/// items that cycles ([`EntityCause::Cycle`]) or nests past [`MAX_ASSEMBLY_DEPTH`]
/// ([`EntityCause::TooDeep`]), or a placement the geometry reader refuses.
#[allow(clippy::too_many_lines)]
pub fn read(ex: &Exchange) -> Result<ProductStructure> {
    let all = in_id_order(ex);
    let mut out = ProductStructure::default();
    // Shape aspects of definitions: PRODUCT_DEFINITION_SHAPE / PROPERTY_DEFINITION → what it
    // defines.
    let mut pds: BTreeMap<u64, u64> = BTreeMap::new();
    let mut sdrs: Vec<(u64, u64)> = Vec::new(); // (PDS, rep)
    let mut cdsrs: Vec<(u64, u64, u64)> = Vec::new(); // (CDSR, relationship, PDS)
    let mut occurrence_ids: Vec<u64> = Vec::new();
    let mut definition_ids: Vec<u64> = Vec::new();
    let mut link_ids: Vec<u64> = Vec::new();
    // Representations to read: those listing a body here, more as they are reached.
    let mut pending: Vec<u64> = Vec::new();
    let mut products: BTreeMap<u64, usize> = BTreeMap::new();
    for &(id, e) in &all {
        let p = e.params();
        match e.name() {
            "PRODUCT" => {
                products.insert(id, out.products.len());
                out.products.push(Product {
                    entity: id,
                    id: text(p.first()),
                    name: text(p.get(1)),
                    description: text(p.get(2)),
                });
            }
            "PRODUCT_DEFINITION" | "PRODUCT_DEFINITION_WITH_ASSOCIATED_DOCUMENTS" => {
                definition_ids.push(id);
            }
            "PRODUCT_DEFINITION_SHAPE" | "PROPERTY_DEFINITION" => {
                pds.insert(id, reference(id, p, 2)?);
            }
            "SHAPE_DEFINITION_REPRESENTATION" => {
                sdrs.push((reference(id, p, 0)?, reference(id, p, 1)?));
            }
            "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION" => {
                cdsrs.push((id, reference(id, p, 0)?, reference(id, p, 1)?));
            }
            n if occurrence_kind(n).is_some() => occurrence_ids.push(id),
            _ => {}
        }
        if let Some(l) = link_params(e) {
            link_ids.push(id);
            pending.push(reference(id, l.reps, 2)?);
            pending.push(reference(id, l.reps, 3)?);
        }
        if let Some(items) = representation_params(e)
            .and_then(|rp| rp.get(1))
            .and_then(Param::as_list)
        {
            if items.iter().any(|i| {
                i.as_ref_id()
                    .and_then(|i| ex.get(i))
                    .is_some_and(|i| body_kind(i.name()).is_some())
            }) {
                pending.push(id);
            }
        }
    }
    // Definitions and their products.
    let mut definitions: BTreeMap<u64, usize> = BTreeMap::new();
    for id in definition_ids {
        let p = entity(ex, id)?.params();
        let formation = reference(id, p, 2)?;
        let of_product = reference(formation, entity(ex, formation)?.params(), 2)?;
        definitions.insert(id, out.definitions.len());
        out.definitions.push(Definition {
            entity: id,
            id: text(p.first()),
            description: text(p.get(1)),
            formation,
            product: index_of(ex, &products, of_product, "PRODUCT")?,
            shapes: Vec::new(),
        });
    }
    // Shapes of definitions: SHAPE_DEFINITION_REPRESENTATION of a PRODUCT_DEFINITION_SHAPE of a
    // PRODUCT_DEFINITION (one of anything else, a shape aspect's, is not the product's shape).
    let mut shapes: Vec<(usize, u64)> = Vec::new();
    for &(d, rep) in &sdrs {
        let of = *pds.get(&d).ok_or_else(|| not_a(ex, d, PROPERTY))?;
        if let Some(&def) = definitions.get(&of) {
            shapes.push((def, rep));
            pending.push(rep);
        }
    }
    // Every representation reached, and the maps its mapped items use.
    let mut reps: BTreeSet<u64> = BTreeSet::new();
    let mut maps: BTreeSet<u64> = BTreeSet::new();
    while let Some(r) = pending.pop() {
        if !reps.insert(r) {
            continue;
        }
        let rp = representation_of(r, entity(ex, r)?)?;
        for item in refs(r, rp, 1)? {
            let ie = entity(ex, item)?;
            if ie.name() == "MAPPED_ITEM" {
                let map = reference(item, ie.params(), 1)?;
                let me = entity(ex, map)?;
                if me.name() != "REPRESENTATION_MAP" {
                    return Err(not_a(ex, map, "REPRESENTATION_MAP"));
                }
                maps.insert(map);
                pending.push(reference(map, me.params(), 1)?);
            }
        }
    }
    let rep_index: BTreeMap<u64, usize> = reps.iter().enumerate().map(|(i, &r)| (r, i)).collect();
    let map_index: BTreeMap<u64, usize> = maps.iter().enumerate().map(|(i, &m)| (m, i)).collect();
    let mut contexts: BTreeMap<u64, Context> = BTreeMap::new();
    let mut context_of = |ctx: Option<u64>| -> Context {
        ctx.map_or_else(Context::default, |c| {
            *contexts.entry(c).or_insert_with(|| units::context(ex, c))
        })
    };
    // Representations, then maps (whose origins are in their representation's units).
    for &r in &reps {
        let e = entity(ex, r)?;
        let rp = representation_of(r, e)?;
        let context = rp.get(2).and_then(Param::as_ref_id);
        let ctx = context_of(context);
        let mut items = Vec::new();
        for item in refs(r, rp, 1)? {
            let ie = entity(ex, item)?;
            let kind = if let Some(b) = body_kind(ie.name()) {
                ItemKind::Body(b)
            } else if ie.name() == "MAPPED_ITEM" {
                let ip = ie.params();
                let map = reference(item, ip, 1)?;
                ItemKind::Mapped {
                    map: index_of(ex, &map_index, map, "REPRESENTATION_MAP")?,
                    target: frame(ex, reference(item, ip, 2)?, ctx.units)
                        .map_err(|e| e.within(item))?,
                }
            } else {
                ItemKind::Other(ie.name().to_string())
            };
            items.push(Item {
                entity: item,
                name: text(ie.params().first()),
                kind,
            });
        }
        out.representations.push(Representation {
            entity: r,
            kind: e.name().to_string(),
            name: text(rp.first()),
            items,
            context,
            units: ctx,
        });
    }
    for &m in &maps {
        let p = entity(ex, m)?.params();
        let rep = reference(m, p, 1)?;
        let representation = index_of(ex, &rep_index, rep, "a representation")?;
        let units = out
            .representations
            .get(representation)
            .map_or_else(Units::default, |r| r.units.units);
        out.maps.push(RepresentationMap {
            entity: m,
            origin: frame(ex, reference(m, p, 0)?, units).map_err(|e| e.within(m))?,
            representation,
        });
    }
    for &(def, rep) in &shapes {
        let r = index_of(ex, &rep_index, rep, "a representation")?;
        if let Some(d) = out.definitions.get_mut(def) {
            d.shapes.push(r);
        }
    }
    // Relationships between representations.
    let mut links: BTreeMap<u64, usize> = BTreeMap::new();
    for id in link_ids {
        let Some(l) = link_params(entity(ex, id)?) else {
            continue;
        };
        let rep_1 = index_of(
            ex,
            &rep_index,
            reference(id, l.reps, 2)?,
            "a representation",
        )?;
        let rep_2 = index_of(
            ex,
            &rep_index,
            reference(id, l.reps, 3)?,
            "a representation",
        )?;
        let transformation =
            transformation(ex, &out, id, (rep_1, rep_2), l.operator).map_err(|e| e.within(id))?;
        links.insert(id, out.links.len());
        out.links.push(Link {
            entity: id,
            rep_1,
            rep_2,
            transformation,
        });
    }
    // Assembly occurrences and their placements.
    let mut occurrences: BTreeMap<u64, usize> = BTreeMap::new();
    for id in occurrence_ids {
        let e = entity(ex, id)?;
        let p = e.params();
        let Some(kind) = occurrence_kind(e.name()) else {
            continue;
        };
        let parent = reference(id, p, 3)?;
        let child = reference(id, p, 4)?;
        occurrences.insert(id, out.occurrences.len());
        out.occurrences.push(Occurrence {
            entity: id,
            kind,
            id: text(p.first()),
            name: text(p.get(1)),
            description: text(p.get(2)),
            parent: index_of(ex, &definitions, parent, "PRODUCT_DEFINITION")
                .map_err(|e| e.within(id))?,
            child: index_of(ex, &definitions, child, "PRODUCT_DEFINITION")
                .map_err(|e| e.within(id))?,
            reference_designator: p.get(5).and_then(Param::as_str).map(str::to_string),
            placements: Vec::new(),
        });
    }
    for &(_, rel, d) in &cdsrs {
        let link = index_of(ex, &links, rel, "a representation relationship")?;
        let of = *pds.get(&d).ok_or_else(|| not_a(ex, d, PROPERTY))?;
        // One for anything but an occurrence (a file may give a part's own shape this way)
        // places nothing; its relationship is a link like any other.
        if let Some(o) = occurrences
            .get(&of)
            .and_then(|&o| out.occurrences.get_mut(o))
        {
            o.placements.push(link);
        }
    }
    // Nesting: the assembly, and chains of mapped items.
    let mut below = vec![Vec::new(); out.definitions.len()];
    let mut used = vec![false; out.definitions.len()];
    for o in &out.occurrences {
        if let Some(b) = below.get_mut(o.parent) {
            b.push(o.child);
        }
        if let Some(u) = used.get_mut(o.child) {
            *u = true;
        }
    }
    let definition_entities: Vec<u64> = out.definitions.iter().map(|d| d.entity).collect();
    Graph {
        children: &below,
        entities: &definition_entities,
    }
    .check_nesting()?;
    let mut mapped = vec![Vec::new(); out.representations.len()];
    for (r, rep) in out.representations.iter().enumerate() {
        for item in &rep.items {
            if let ItemKind::Mapped { map, .. } = item.kind {
                if let (Some(m), Some(b)) = (out.maps.get(map), mapped.get_mut(r)) {
                    b.push(m.representation);
                }
            }
        }
    }
    let rep_entities: Vec<u64> = out.representations.iter().map(|r| r.entity).collect();
    Graph {
        children: &mapped,
        entities: &rep_entities,
    }
    .check_nesting()?;
    out.roots = used
        .iter()
        .enumerate()
        .filter(|(_, u)| !**u)
        .map(|(i, _)| i)
        .collect();
    // Styles.
    for &(id, e) in &all {
        if let Some(s) = styled_item(ex, id, e).map_err(|e| e.within(id))? {
            out.styles.push(s);
        }
    }
    Ok(out)
}

impl ProductStructure {
    /// The assembly tree expanded: every root, then depth first each occurrence of the
    /// definition just placed, in occurrence order. A definition used `n` times is `n` nodes.
    ///
    /// # Errors
    ///
    /// [`EntityCause::TooManyInstances`] on the root whose expansion passes [`MAX_VISITS`]
    /// nodes, and [`EntityCause::TooDeep`] on a definition nested past
    /// [`MAX_ASSEMBLY_DEPTH`] (which [`read`] has checked; a structure built by hand may not
    /// be).
    pub fn instances(&self) -> Result<Vec<Node>> {
        let mut below: Vec<Vec<usize>> = vec![Vec::new(); self.definitions.len()];
        for (i, o) in self.occurrences.iter().enumerate() {
            if let Some(b) = below.get_mut(o.parent) {
                b.push(i);
            }
        }
        let mut out = Vec::new();
        for &root in &self.roots {
            let at = Expansion {
                below: &below,
                root: self.entity_of(root),
            };
            self.expand(&at, root, None, None, 0, &mut out)?;
        }
        Ok(out)
    }

    /// The entity id of definition `d` (0, no instance, for an index past the definitions).
    fn entity_of(&self, d: usize) -> u64 {
        self.definitions.get(d).map_or(0, |d| d.entity)
    }

    /// Adds the node of `definition` and, below it, its occurrences' nodes. Recursion is
    /// bounded by `depth` (refused past [`MAX_ASSEMBLY_DEPTH`]).
    fn expand(
        &self,
        at: &Expansion<'_>,
        definition: usize,
        occurrence: Option<usize>,
        parent: Option<usize>,
        depth: usize,
        out: &mut Vec<Node>,
    ) -> Result<()> {
        if depth > MAX_ASSEMBLY_DEPTH {
            return refuse(
                self.entity_of(definition),
                EntityCause::TooDeep {
                    limit: MAX_ASSEMBLY_DEPTH,
                },
            );
        }
        if out.len() >= MAX_VISITS {
            return refuse(at.root, EntityCause::TooManyInstances { limit: MAX_VISITS });
        }
        let me = out.len();
        out.push(Node {
            definition,
            occurrence,
            parent,
        });
        for &o in at.below.get(definition).map_or(&[][..], Vec::as_slice) {
            if let Some(occ) = self.occurrences.get(o) {
                self.expand(at, occ.child, Some(o), Some(me), depth + 1, out)?;
            }
        }
        Ok(())
    }
}

/// What one root's expansion carries down: each definition's occurrences (indices into
/// [`ProductStructure::occurrences`]) and the root's entity id.
struct Expansion<'a> {
    below: &'a [Vec<usize>],
    root: u64,
}
