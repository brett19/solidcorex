// Written for solidcorex. Apache-2.0.
//! Stage 6 over cells: which faces a selection keeps, and the shells of a result that is
//! not one manifold solid (BO25; 04 row 16, 75 §2.6).
//!
//! - **Cells.** The imprinted operands cut space into cells, each labelled by the operands
//!   it lies in (its [`Membership`]). A piece of an operand face lies between two cells:
//!   the one on its operand's material side, which holds that operand, and the one on the
//!   other side, which does not; the other operand is in both (the piece is inside it), in
//!   neither (outside), or in the one its own material faces (a coincident face, `On`).
//!   Where both operands have a piece on one place, `a`'s stands for the pair.
//! - **Selection.** A [`Rule`] says which cells are solid in the result; every face between
//!   a solid and a void cell stays (the regularised result's boundary). Its [`Faces`] may
//!   keep more: walls between two solid cells (each stays its own region), laminas and
//!   sheets between two void cells (a face with the void on both sides).
//! - **Shells.** A shell is a connected piece of the space round the kept faces: the face
//!   sides that meet across a sector of an edge are on one shell, turning about the edge
//!   in radial order; an edge one face bounds (a sheet's or a lamina's free edge) joins
//!   that face's two sides. The sides of one shell must agree on solid or void, else the
//!   selection and the radial order disagree, a defect. Each solid shell first bounds a
//!   region of its own and each void shell the outside; nesting ([`pinch::nest`]) then
//!   moves a cavity's wall to the region round it and a lump in a cavity to that void, as
//!   for a manifold result. A shell of laminas only bounds no volume: it lies in the void
//!   that holds it.

use std::collections::{BTreeMap, BTreeSet};

use scx_brep::{
    Document, EdgeGeom, EdgeShape, EdgeUse, Entity, FaceId, Fin, LoopRecord, Origin, RegionId,
    ShellId, ShellRecord, UseGeom, UvSide, VertexId, VertexRecord,
};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Sense;

use crate::boolean::BooleanOp;
use crate::build::unused;
use crate::classify::Class;
use crate::error::{BooleanDefect, BooleanError, BooleanUnsupported};
use crate::faceface::Touch;
use crate::geom::Side;
use crate::imprint::{Imprint, draft_defect};
use crate::pinch;

/// The operands a cell lies in: bit `i` is operand `i` (`a` is 0, `b` is 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Membership(u64);

impl Membership {
    /// In no operand: the void round them all.
    pub const NONE: Self = Self(0);
    /// In `a` only.
    pub const A: Self = Self(1);
    /// In `b` only.
    pub const B: Self = Self(2);
    /// In both.
    pub const BOTH: Self = Self(3);

    /// The cell of operand `i` alone, or `None` past the 64 operands a membership holds.
    #[must_use]
    pub const fn of(i: usize) -> Option<Self> {
        if i < 64 { Some(Self(1 << i)) } else { None }
    }

    /// Whether the cell lies in operand `i`.
    #[must_use]
    pub const fn contains(self, i: usize) -> bool {
        i < 64 && self.0 & (1 << i) != 0
    }

    /// The cell in every operand either is in.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The operands both are in.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// The operands, as bits.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// The membership of these bits.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }
}

/// The faces a selection keeps beyond its boundary (the faces between a solid and a void
/// cell, which every selection keeps).
#[derive(Clone, Copy)]
pub enum Faces<'r> {
    /// The boundary only: the regularised result, one manifold solid where the cells allow.
    Boundary,
    /// Also each face between two solid cells of different membership: every cell its
    /// own region, the faces between them two-sided walls (the cellular merge's body).
    Walls,
    /// Also each face between two void cells whose union is solid: the closure of the
    /// selection, its laminas and sheets kept (the non-regularised result).
    Closure,
    /// Also each face between two cells, both solid or both void, for which this holds
    /// (given the cells on the two sides, the material side of the piece's operand first).
    Where(&'r dyn Fn(Membership, Membership) -> bool),
}

impl core::fmt::Debug for Faces<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Boundary => "Boundary",
            Self::Walls => "Walls",
            Self::Closure => "Closure",
            Self::Where(_) => "Where(..)",
        })
    }
}

/// A selection over the cells of two operands.
#[derive(Clone, Copy)]
pub struct Rule<'r> {
    /// Whether a cell is solid in the result.
    pub solid: &'r dyn Fn(Membership) -> bool,
    /// The faces kept beyond the boundary.
    pub faces: Faces<'r>,
}

impl core::fmt::Debug for Rule<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Rule")
            .field("faces", &self.faces)
            .finish_non_exhaustive()
    }
}

fn union(m: Membership) -> bool {
    m != Membership::NONE
}

fn intersect(m: Membership) -> bool {
    m == Membership::BOTH
}

fn subtract(m: Membership) -> bool {
    m == Membership::A
}

impl Rule<'static> {
    /// The regularised Boolean: union keeps a cell in either operand, intersection one in
    /// both, subtraction one in `a` only.
    #[must_use]
    pub const fn of(op: BooleanOp) -> Self {
        Self::with(op, Faces::Boundary)
    }

    /// The Boolean's cells with `faces` kept.
    #[must_use]
    pub const fn with(op: BooleanOp, faces: Faces<'static>) -> Self {
        let solid: &'static dyn Fn(Membership) -> bool = match op {
            BooleanOp::Union => &union,
            BooleanOp::Intersect => &intersect,
            BooleanOp::Subtract => &subtract,
        };
        Self { solid, faces }
    }
}

impl Rule<'_> {
    /// Whether a face between cells `m` and `o`, both solid or both void (`solid`), stays.
    fn more(&self, m: Membership, o: Membership, solid: bool) -> bool {
        match self.faces {
            Faces::Boundary => false,
            Faces::Walls => solid && m != o,
            Faces::Closure => !solid && (self.solid)(m.union(o)),
            Faces::Where(f) => f(m, o),
        }
    }
}

/// What a kept piece is: whether its operand's material side goes void (it is turned
/// over), and whether each of its two sides is solid (front, back) once kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pick {
    pub(crate) flip: bool,
    /// Both sides of one kind: `Some(solid)` for a wall (`true`) or a lamina (`false`);
    /// `None` for a boundary face.
    pub(crate) both: Option<bool>,
}

/// Whether `rule` keeps a piece of `side` of class `c`, and how.
pub(crate) fn pick(rule: &Rule<'_>, side: Side, c: Class) -> Option<Pick> {
    let (own, other) = match side {
        Side::A => (Membership::A, Membership::B),
        Side::B => (Membership::B, Membership::A),
    };
    // Whether the other operand holds the material side, and the other side.
    let (in_m, in_o) = match c {
        Class::In => (true, true),
        Class::Out => (false, false),
        // `a`'s piece stands for a coincident pair.
        Class::On { .. } if side == Side::B => return None,
        Class::On { same } => (same, !same),
    };
    let m = if in_m { own.union(other) } else { own };
    let o = if in_o { other } else { Membership::NONE };
    let (sm, so) = ((rule.solid)(m), (rule.solid)(o));
    if sm != so {
        Some(Pick {
            flip: so,
            both: None,
        })
    } else if rule.more(m, o, sm) {
        Some(Pick {
            flip: false,
            both: Some(sm),
        })
    } else {
        None
    }
}

const fn defect() -> BooleanError {
    BooleanError::Defect(BooleanDefect::Build)
}

/// A face side: the face, and 0 for its front, 1 for its back.
type FaceSide = (FaceId, usize);

/// The face side a fin's sector comes after, turning about the edge (as in [`pinch`]).
const fn after(sense: Sense) -> usize {
    match sense {
        Sense::Forward => 0,
        Sense::Reversed => 1,
    }
}

/// A union-find over face sides; a set's root is its least side.
struct Sides(BTreeMap<FaceSide, FaceSide>);

impl Sides {
    fn find(&self, mut s: FaceSide) -> FaceSide {
        while let Some(&p) = self.0.get(&s) {
            if p == s {
                break;
            }
            s = p;
        }
        s
    }

    fn join(&mut self, a: FaceSide, b: FaceSide) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0.insert(ra.max(rb), ra.min(rb));
        }
    }
}

/// Gives the kept faces their shells and regions where some face is a wall or a lamina:
/// `solid` holds each live face's (front, back) kinds. The fins of every edge are already
/// in radial order.
pub(crate) fn shells(
    m: &mut Imprint,
    solid: &BTreeMap<FaceId, [bool; 2]>,
    doc: Document,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(), BooleanError> {
    let mut sides = Sides(BTreeMap::new());
    for e in m.draft.edges() {
        if matches!(e.shape(), Some(EdgeShape::Collapsed)) {
            continue;
        }
        let fins: Vec<Fin> = e.fins().collect();
        let n = fins.len();
        for (i, &fin) in fins.iter().enumerate() {
            let next = fins.get((i + 1) % n).copied().ok_or_else(defect)?;
            let a = (fin.face, after(fin.sense));
            let b = (next.face, 1 - after(next.sense));
            let kind = |(f, k): FaceSide| solid.get(&f).and_then(|s| s.get(k).copied());
            if kind(a).is_none() || kind(a) != kind(b) {
                return Err(defect());
            }
            sides.join(a, b);
        }
    }
    // Each set of sides, by its root, with its kind.
    let mut sets: BTreeMap<FaceSide, (bool, Vec<FaceSide>)> = BTreeMap::new();
    for (&f, kinds) in solid {
        for (k, &s) in kinds.iter().enumerate() {
            let r = sides.find((f, k));
            sets.entry(r).or_insert((s, Vec::new())).1.push((f, k));
        }
    }
    // A shell per set: the shell its root side has now where no set took it first and it
    // is of the set's kind, else a new one; a solid shell in a region of its own.
    let mut claimed: BTreeSet<ShellId> = BTreeSet::new();
    let mut regions: BTreeSet<RegionId> = BTreeSet::new();
    let mut all: Vec<ShellId> = Vec::with_capacity(sets.len());
    for (&(f, k), (is_solid, members)) in &sets {
        let now = m
            .draft
            .face(f)
            .and_then(|v| v.record().sides.get(k).copied())
            .ok_or_else(defect)?;
        let region = m.draft.shell(now).map(|s| s.record().region);
        let reuse = !claimed.contains(&now)
            && region.is_some_and(|r| {
                m.draft.region(r).is_some_and(|g| g.is_solid() == *is_solid)
                    && (!*is_solid || !regions.contains(&r))
            });
        let shell = if reuse {
            if let Some(r) = region.filter(|_| *is_solid) {
                regions.insert(r);
            }
            now
        } else {
            let r = if *is_solid {
                let r = m.draft.add_region(true).map_err(|_| defect())?;
                regions.insert(r);
                r
            } else {
                RegionId::OUTSIDE
            };
            m.draft
                .add_shell(ShellRecord::new(r))
                .map_err(|_| defect())?
        };
        claimed.insert(shell);
        all.push(shell);
        for &(g, j) in members {
            let face = m.draft.face_mut(g).ok_or_else(defect)?;
            if let Some(s) = face.sides.get_mut(j) {
                *s = shell;
            }
        }
    }
    // A void shell starts in the outside; a reused one may be a cavity's.
    for &s in &all {
        let record = m.draft.shell(s).ok_or_else(defect)?.record();
        let void = m.draft.region(record.region).is_some_and(|r| !r.is_solid());
        if void && record.region != RegionId::OUTSIDE {
            m.draft.shell_mut(s).ok_or_else(defect)?.region = RegionId::OUTSIDE;
        }
    }
    unused(m, &claimed);
    if all.len() > 1 {
        pinch::nest(&mut m.draft, &all, &[], &BTreeMap::new(), doc, budget, stop)?;
        unused(m, &claimed);
    }
    Ok(())
}

/// Writes each touch inside a face where both faces keep a piece (the result meeting
/// itself at a point, D5): a vertex there (the merged point's, where the touch has one)
/// and, in each kept piece whose inside holds the touch, a loop of one point use on a
/// collapsed edge at it, scx-brep's lone vertex in a face (01, the entity table). A face
/// whose boundary already passes through the vertex needs no loop. `kept` lists the live
/// kept faces with their operand faces.
///
/// # Errors
///
/// [`BooleanUnsupported::NonManifold`] at a face holding the touch whose region test
/// cannot place it inside one piece (on a piece's boundary, so no lone point). A touch
/// that no kept piece of one of its faces reaches is not in the result and is skipped.
pub(crate) fn touches(
    m: &mut Imprint,
    touches: &[Touch],
    kept: &[(FaceId, Side, FaceId)],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(), BooleanError> {
    for t in touches {
        let pieces = |side: Side, of: FaceId| {
            kept.iter()
                .filter(move |k| (k.1, k.2) == (side, of))
                .map(|k| k.0)
        };
        let sides = [Side::A, Side::B];
        if !sides
            .into_iter()
            .zip(t.faces)
            .all(|(side, of)| pieces(side, of).next().is_some())
        {
            continue;
        }
        stop.check()?;
        let merged = match t.point {
            Some(i) => Some(m.points.get(i).copied().ok_or_else(defect)?),
            None => None,
        };
        let mut lone: Vec<(FaceId, scx_num::vector2d::Point2)> = Vec::new();
        let mut unsure = None;
        let mut absent = false;
        for (side, of) in sides.into_iter().zip(t.faces) {
            let on_boundary =
                merged.is_some_and(|v| pieces(side, of).any(|f| uses_vertex(m, f, v)));
            if on_boundary {
                continue;
            }
            let (mut inside, mut near) = (None, false);
            for f in pieces(side, of) {
                let view = m.draft.face(f).ok_or_else(defect)?;
                let domain = view.uv_box();
                let uv = view
                    .surface()
                    .invert(t.at, None, domain.as_ref(), budget, stop)
                    .map_err(|_| defect())?
                    .foot()
                    .at;
                match view.contains_uv(uv) {
                    Some(UvSide::Inside) => {
                        inside = Some((f, uv));
                        break;
                    }
                    Some(UvSide::Outside) => {}
                    Some(UvSide::Near) | None => near = true,
                }
            }
            match (inside, near) {
                (Some(at), _) => lone.push(at),
                (None, true) => unsure = Some(of),
                // No kept piece of this face reaches the touch: the result is the other
                // face alone there, smooth, and holds no touch.
                (None, false) => absent = true,
            }
        }
        if absent {
            continue;
        }
        if let Some(of) = unsure {
            return Err(BooleanError::Unsupported(BooleanUnsupported::NonManifold {
                at: Entity::Face(of),
            }));
        }
        if lone.is_empty() {
            continue;
        }
        let v = if let Some(v) = merged {
            v
        } else {
            let v = m
                .draft
                .add_vertex(VertexRecord::new(t.at))
                .map_err(draft_defect)?;
            m.draft
                .derive(v, Origin::Face(t.faces[0]))
                .map_err(draft_defect)?;
            m.draft
                .derive(
                    v,
                    Origin::Operand(Side::B.operand(), Entity::Face(t.faces[1])),
                )
                .map_err(draft_defect)?;
            v
        };
        for (f, uv) in lone {
            let e = m
                .draft
                .add_edge(EdgeGeom::new(EdgeShape::Collapsed, [v, v]))
                .map_err(draft_defect)?;
            m.draft.derive(e, Origin::Face(f)).map_err(draft_defect)?;
            let point = EdgeUse::new(e, Sense::Forward, UseGeom::Point(uv));
            m.draft
                .edit_face(f, |r| r.loops.push(LoopRecord::new(vec![point])))
                .ok_or_else(defect)?;
        }
    }
    Ok(())
}

/// Whether a loop of draft face `f` passes through vertex `v`.
fn uses_vertex(m: &Imprint, f: FaceId, v: VertexId) -> bool {
    m.draft.face(f).is_some_and(|face| {
        face.record()
            .loops
            .iter()
            .flat_map(|l| &l.uses)
            .any(|u| m.draft.edge(u.edge).is_some_and(|e| e.ends().contains(&v)))
    })
}
