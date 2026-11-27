// Written for solidcorex. Apache-2.0.
//! Stage 6, build: where the result meets itself, and where its shells nest (BO7, 72
//! §14.36; D5 in 04).
//!
//! - **Radial order.** An edge four or more kept faces bound is an edge where the result
//!   meets itself (two solids touching along a line, or one touching itself). Its fins are
//!   listed counter-clockwise about the edge's direction, as scx-brep's radial check wants:
//!   each face leaves the edge at its midpoint in the direction `d = σ N × T` (`T` the
//!   edge's tangent, `N` the face's normal at the use's lift, `σ` the use's sense), and
//!   the fins are sorted by the angle of `d`. Two faces leaving in one direction are
//!   tangent along the edge: the order there needs their bends, and the result meets
//!   itself inside a face as well, so it is [`BooleanUnsupported::NonManifold`]. An odd
//!   count is a defect: a closed result's edge has as many fins of each sense.
//! - **Links.** Shells meeting across a sector of such an edge are one shell: the sector is
//!   one piece of space. A sector between a material and a void shell is a defect.
//! - **Nesting.** Each shell gets its region from its volume's sign and the shells that
//!   enclose it: a positive material shell bounds its own solid region, a negative one (a
//!   cavity's wall) goes to the region of the innermost positive material shell holding
//!   it; a positive void shell (inside a cavity) bounds a new void region, a negative one
//!   (round a lump) goes to the innermost positive void shell's region holding it, else to
//!   the outside. Which shell holds which is a point-in-shell test per pair, run only
//!   where the boxes leave a choice.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;

use scx_bodyq::{BodyqError, PointClass, classify_point};
use scx_brep::{
    Brep, Document, Draft, EdgeId, EdgeShape, Entity, Fin, RegionId, ShellId, ShellRecord, UseGeom,
    VertexId,
};
use scx_mass::{MassError, Sign, volume_sign};
use scx_num::budget::{Budget, Stop};
use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::frame::Sense;
use scx_num::vector3d::Point3;
use scx_nurbs::Approach;

use crate::classify::inner_uv;
use crate::error::{BooleanDefect, BooleanError, BooleanUnsupported};

/// Two faces leaving an edge closer than this, in radians, leave it in one direction.
const TIED: f64 = 1e-9;

const fn defect() -> BooleanError {
    BooleanError::Defect(BooleanDefect::Build)
}

const fn non_manifold(at: Entity) -> BooleanError {
    BooleanError::Unsupported(BooleanUnsupported::NonManifold { at })
}

/// Lists the fins of `edge` (four or more) counter-clockwise about its direction.
pub(crate) fn order_fins(draft: &mut Draft, edge: EdgeId) -> Result<(), BooleanError> {
    let order = radial(draft, edge)?;
    draft.adjacency_mut(edge).ok_or_else(defect)?.fins = order;
    Ok(())
}

/// The fins of `edge` in radial order.
fn radial(draft: &Draft, edge: EdgeId) -> Result<Vec<Fin>, BooleanError> {
    let e = draft.edge(edge).ok_or_else(defect)?;
    if e.fins().len() % 2 == 1 {
        return Err(defect());
    }
    let Some(EdgeShape::Curve { curve, interval }) = e.shape() else {
        return Err(defect());
    };
    let s = 0.5 * (interval.min() + interval.max());
    let tan = curve
        .tangent_dir(s, Approach::Above)
        .ok_or_else(defect)?
        .as_vec();
    let mut leaving = Vec::with_capacity(e.fins().len());
    for u in e.uses() {
        let u = u.ok_or_else(defect)?;
        let UseGeom::Pcurve { curve: pc, .. } = u.geom() else {
            return Err(defect());
        };
        let n = u
            .face()
            .surface()
            .normal(pc.point(s), [Approach::Above; 2])
            .ok_or_else(|| non_manifold(Entity::Edge(edge)))?
            .as_vec();
        let d = (n.cross(tan) * u.sense().sign())
            .normalized()
            .ok_or_else(|| non_manifold(Entity::Edge(edge)))?
            .as_vec();
        leaving.push((u.fin(), d));
    }
    let Some(&(_, e1)) = leaving.first() else {
        return Err(defect());
    };
    let e2 = tan.cross(e1);
    let mut at: Vec<(f64, Fin)> = leaving
        .iter()
        .enumerate()
        .map(|(i, &(fin, d))| {
            let a = fmath::atan2(d.dot(e2), d.dot(e1));
            let a = if i == 0 {
                0.0
            } else if a < 0.0 {
                a + TAU
            } else {
                a
            };
            (a, fin)
        })
        .collect();
    at.sort_by(|x, y| x.0.total_cmp(&y.0));
    let tied = at.windows(2).any(|w| match w {
        [(a, _), (b, _)] => b - a <= TIED,
        _ => false,
    }) || at.last().is_some_and(|(last, _)| TAU - last <= TIED);
    if tied {
        return Err(non_manifold(Entity::Edge(edge)));
    }
    Ok(at.into_iter().map(|(_, f)| f).collect())
}

/// The face side a fin's sector comes after, turning about the edge: a forward use's
/// front, a reversed use's back (as scx-brep's structure check reads it).
const fn after(sense: Sense) -> usize {
    match sense {
        Sense::Forward => 0,
        Sense::Reversed => 1,
    }
}

/// The root of `s` in the union-find `parent`.
fn root(parent: &BTreeMap<ShellId, ShellId>, mut s: ShellId) -> ShellId {
    while let Some(&p) = parent.get(&s) {
        if p == s {
            break;
        }
        s = p;
    }
    s
}

/// Joins the shells that meet across a sector of an edge with more than two fins: the
/// lowest id of each joined set stays, the others' faces are moved to it and they leave
/// `claimed`. Returns each moved shell with the shell it was joined to.
pub(crate) fn link(
    draft: &mut Draft,
    claimed: &mut BTreeSet<ShellId>,
) -> Result<BTreeMap<ShellId, ShellId>, BooleanError> {
    let mut parent: BTreeMap<ShellId, ShellId> = BTreeMap::new();
    let mut any = false;
    for e in draft.edges() {
        let fins: Vec<Fin> = e.fins().collect();
        if fins.len() <= 2 {
            continue;
        }
        any = true;
        let n = fins.len();
        for (i, &fin) in fins.iter().enumerate() {
            let next = fins.get((i + 1) % n).copied().ok_or_else(defect)?;
            let side = |f: Fin, k: usize| {
                draft
                    .face(f.face)
                    .and_then(|v| v.record().sides.get(k).copied())
            };
            let a = side(fin, after(fin.sense)).ok_or_else(defect)?;
            let b = side(next, 1 - after(next.sense)).ok_or_else(defect)?;
            let solid = |s: ShellId| {
                draft
                    .shell(s)
                    .and_then(scx_brep::Shell::region)
                    .map(scx_brep::Region::is_solid)
            };
            if solid(a).is_none() || solid(a) != solid(b) {
                return Err(defect());
            }
            let (ra, rb) = (root(&parent, a), root(&parent, b));
            if ra != rb {
                parent.insert(ra.max(rb), ra.min(rb));
            }
        }
    }
    let mut stands: BTreeMap<ShellId, ShellId> = BTreeMap::new();
    if !any {
        return Ok(stands);
    }
    let moved: BTreeMap<ShellId, ShellId> = parent
        .keys()
        .map(|&s| (s, root(&parent, s)))
        .filter(|(s, r)| s != r)
        .collect();
    if moved.is_empty() {
        return Ok(stands);
    }
    let faces: Vec<_> = draft
        .faces()
        .map(|f| (f.id(), f.record().sides))
        .filter(|(_, sides)| sides.iter().any(|s| moved.contains_key(s)))
        .collect();
    for (f, sides) in faces {
        let to = sides.map(|s| moved.get(&s).copied().unwrap_or(s));
        draft.face_mut(f).ok_or_else(defect)?.sides = to;
    }
    for (&s, &r) in &moved {
        claimed.remove(&s);
        stands.insert(s, r);
    }
    Ok(stands)
}

/// What one shell is, for nesting.
#[derive(Debug, Clone, Copy)]
struct Placed {
    shell: ShellId,
    material: bool,
    sign: Sign,
    bbox: Extent3d,
}

impl Placed {
    /// The shells of `all` that may hold `self`: positive, of `material`, their boxes
    /// holding its box.
    fn holders(&self, all: &[Self], material: bool) -> Vec<Self> {
        all.iter()
            .filter(|q| {
                q.material == material
                    && q.sign == Sign::Positive
                    && q.shell != self.shell
                    && self.bbox.is_contained_by(&q.bbox, 0.0)
            })
            .copied()
            .collect()
    }
}

/// Where a shell goes.
#[derive(Debug, Clone, Copy)]
enum Goes {
    /// It stays in its region.
    Stays,
    /// To the region of this shell.
    With(ShellId),
    /// To a new void region (a cavity's void).
    NewVoid,
    /// To the outside.
    Outside,
}

const fn nesting() -> BooleanError {
    BooleanError::Unsupported(BooleanUnsupported::Nesting)
}

/// Gives every shell in `shells` its region (see the module doc). `mirrored` lists the
/// (material, void) pairs no link touched: the void's sign is the material's turned over.
/// `known` holds the signs already settled (a set that is an operand shell whole); the
/// rest are measured.
pub(crate) fn nest(
    draft: &mut Draft,
    shells: &[ShellId],
    mirrored: &[(ShellId, ShellId)],
    known: &BTreeMap<ShellId, Sign>,
    doc: Document,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(), BooleanError> {
    let (body, _) = draft.clone().finish(&doc).map_err(|_| defect())?;
    let mirror: BTreeMap<ShellId, ShellId> = mirrored.iter().map(|&(m, v)| (v, m)).collect();
    let mut sign: BTreeMap<ShellId, Sign> = BTreeMap::new();
    for &s in shells.iter().filter(|s| !mirror.contains_key(s)) {
        let v = match known.get(&s) {
            Some(&v) => v,
            None => volume_sign(&body, s, budget, stop).map_err(mass)?,
        };
        sign.insert(s, v);
    }
    for (&v, &m) in &mirror {
        let turned = match sign.get(&m).copied().ok_or_else(defect)? {
            Sign::Positive => Sign::Negative,
            Sign::Negative => Sign::Positive,
            Sign::Zero => Sign::Zero,
        };
        sign.insert(v, turned);
    }
    let mut placed = Vec::with_capacity(shells.len());
    for &s in shells {
        let sh = body.shell(s).ok_or_else(defect)?;
        let mut bbox: Option<Extent3d> = None;
        for (f, _) in sh.faces() {
            let b = f.bounding_box().ok_or_else(defect)?;
            bbox = Some(bbox.map_or(b, |x| x.union(b)));
        }
        let sign = sign.get(&s).copied().ok_or_else(defect)?;
        if sign == Sign::Zero {
            return Err(defect());
        }
        placed.push(Placed {
            shell: s,
            material: sh.region().is_solid(),
            sign,
            bbox: bbox.ok_or_else(defect)?,
        });
    }
    let mut tests = Tests::default();
    let mut goes: Vec<(ShellId, Goes)> = Vec::with_capacity(placed.len());
    for p in &placed {
        let g = match (p.material, p.sign) {
            (true, Sign::Positive) => Goes::Stays,
            (false, Sign::Positive) => Goes::NewVoid,
            // A cavity's wall: some positive material shell holds it.
            (true, _) => match p.holders(&placed, true).as_slice() {
                [] => return Err(nesting()),
                [one] => Goes::With(one.shell),
                many => {
                    let held = tests.holding(draft, &body, p, many, doc, budget, stop)?;
                    Goes::With(innermost(&held)?.ok_or_else(nesting)?.shell)
                }
            },
            // Round a lump: a cavity's void may hold it.
            (false, _) => {
                let cands = p.holders(&placed, false);
                if cands.is_empty() {
                    Goes::Outside
                } else {
                    let held = tests.holding(draft, &body, p, &cands, doc, budget, stop)?;
                    innermost(&held)?.map_or(Goes::Outside, |h| Goes::With(h.shell))
                }
            }
        };
        goes.push((p.shell, g));
    }
    // New void regions first, so the shells they hold can follow them.
    for &(s, g) in &goes {
        if let Goes::NewVoid = g {
            let r = draft.add_region(false).map_err(|_| defect())?;
            draft.shell_mut(s).ok_or_else(defect)?.region = r;
        }
    }
    for &(s, g) in &goes {
        let r = match g {
            Goes::Stays | Goes::NewVoid => continue,
            Goes::Outside => RegionId::OUTSIDE,
            Goes::With(h) => draft.shell(h).ok_or_else(defect)?.record().region,
        };
        draft.shell_mut(s).ok_or_else(defect)?.region = r;
    }
    Ok(())
}

/// The innermost of shells that all hold one shell: the one with the smallest box (shells
/// of a valid body do not cross, so holders of one shell nest, and an inner one's box is
/// inside an outer one's). Two with one box cannot be told apart: [`nesting`].
fn innermost(held: &[Placed]) -> Result<Option<Placed>, BooleanError> {
    let volume = |p: &Placed| {
        let s = p.bbox.size();
        s.x * s.y * s.z
    };
    let mut best: Option<Placed> = None;
    let mut tie = false;
    for &h in held {
        match best {
            None => best = Some(h),
            Some(b) => match volume(&h).total_cmp(&volume(&b)) {
                std::cmp::Ordering::Less => {
                    best = Some(h);
                    tie = false;
                }
                std::cmp::Ordering::Equal => tie = true,
                std::cmp::Ordering::Greater => {}
            },
        }
    }
    if tie {
        return Err(nesting());
    }
    Ok(best)
}

/// The point-in-shell tests run so far: a body per holder shell, its one shell's enclosed
/// side solid.
#[derive(Default)]
struct Tests {
    bodies: BTreeMap<ShellId, (Brep, RegionId)>,
}

impl Tests {
    /// The shells of `cands` that hold `p`: a point inside a face of `p` is inside them.
    #[allow(clippy::too_many_arguments)]
    fn holding(
        &mut self,
        draft: &Draft,
        body: &Brep,
        p: &Placed,
        cands: &[Placed],
        doc: Document,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Vec<Placed>, BooleanError> {
        let (face, _) = body
            .shell(p.shell)
            .and_then(|s| s.faces().next())
            .ok_or_else(defect)?;
        let view = draft.face(face.id()).ok_or_else(defect)?;
        let q: Point3 = view.surface().point(inner_uv(view).ok_or_else(defect)?);
        let mut held = Vec::new();
        for c in cands {
            stop.check()?;
            if let Entry::Vacant(v) = self.bodies.entry(c.shell) {
                v.insert(shell_body(draft, c.shell, doc)?);
            }
            let (b, solid) = self.bodies.get(&c.shell).ok_or_else(defect)?;
            match classify_point(b, q, budget, stop).map_err(bodyq)? {
                PointClass::In(r) if r == *solid => held.push(*c),
                PointClass::In(_) => {}
                _ => return Err(nesting()),
            }
        }
        Ok(held)
    }
}

/// A body of `draft`'s shell `p` alone, with the side it encloses solid, and that region.
fn shell_body(draft: &Draft, p: ShellId, doc: Document) -> Result<(Brep, RegionId), BooleanError> {
    let mut d = draft.clone();
    let solid = d.add_region(true).map_err(|_| defect())?;
    let out = d
        .add_shell(ShellRecord::new(RegionId::OUTSIDE))
        .map_err(|_| defect())?;
    d.shell_mut(p).ok_or_else(defect)?.region = solid;
    let faces: Vec<_> = d.faces().map(|f| (f.id(), f.record().sides)).collect();
    for (f, sides) in faces {
        match sides {
            [a, b] if a == p && b == p => return Err(defect()),
            [a, b] if a == p || b == p => {
                d.face_mut(f).ok_or_else(defect)?.sides =
                    sides.map(|s| if s == p { p } else { out });
            }
            _ => {
                d.remove_face(f);
            }
        }
    }
    let loose: Vec<EdgeId> = d
        .edges()
        .filter(|e| e.fins().len() == 0)
        .map(scx_brep::Edge::id)
        .collect();
    for e in loose {
        d.remove_edge(e);
    }
    let shells: Vec<ShellId> = d
        .shells()
        .map(scx_brep::Shell::id)
        .filter(|&s| s != p && s != out)
        .collect();
    for s in shells {
        d.remove_shell(s);
    }
    let mut used: BTreeSet<VertexId> = BTreeSet::new();
    for e in d.edges() {
        used.extend(e.ends());
    }
    for s in d.shells() {
        used.extend(s.record().lone_vertices.iter().copied());
    }
    let gone: Vec<VertexId> = d
        .vertices()
        .map(scx_brep::Vertex::id)
        .filter(|v| !used.contains(v))
        .collect();
    for v in gone {
        d.remove_vertex(v);
    }
    let regions: Vec<RegionId> = d
        .regions()
        .map(scx_brep::Region::id)
        .filter(|&r| r != RegionId::OUTSIDE && r != solid)
        .collect();
    for r in regions {
        d.remove_region(r);
    }
    let (b, _) = d.finish(&doc).map_err(|_| defect())?;
    Ok((b, solid))
}

/// A mass query's error, as the Boolean's.
fn mass(e: MassError) -> BooleanError {
    match e {
        MassError::Exhausted { limit } => BooleanError::Exhausted { limit },
        MassError::Cancelled => BooleanError::Cancelled,
        _ => defect(),
    }
}

/// A point query's error, as the Boolean's.
fn bodyq(e: BodyqError) -> BooleanError {
    match e {
        BodyqError::Exhausted { limit } => BooleanError::Exhausted { limit },
        BodyqError::Cancelled => BooleanError::Cancelled,
        _ => defect(),
    }
}
