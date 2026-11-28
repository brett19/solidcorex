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
//!   tangent along the edge (BO22): past the edge the one bending further
//!   counter-clockwise (its normal curvature along `d`, with the use's sense) lies
//!   further counter-clockwise, so the bends order them, in closed form on the analytic
//!   kinds. Bends that tie are faces agreeing to second order, which may meet inside a
//!   face as well: [`BooleanUnsupported::NonManifold`]. An odd count is a defect in a
//!   regular result (a closed result's edge has as many fins of each sense); a selection
//!   that keeps laminas or sheets (BO25) may end one at the edge.
//! - **Links.** Shells meeting across a sector of such an edge are one shell: the sector is
//!   one piece of space. A sector between a material and a void shell is a defect.
//! - **Nesting.** Each shell gets its region from its volume's sign and the shells that
//!   enclose it: a positive material shell bounds its own solid region, a negative one (a
//!   cavity's wall) goes to the region of the innermost positive material shell holding
//!   it; a positive void shell (inside a cavity) bounds a new void region, a negative one
//!   (round a lump) goes to the innermost positive void shell's region holding it, else to
//!   the outside. Which shell holds which is a point-in-shell test per pair, run only
//!   where the boxes leave a choice; two shells sharing a face (across a wall) never hold
//!   each other, and a shell of laminas only (no volume) lies in the void round it.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;

use scx_bodyq::{BodyqError, PointClass, classify_point};
use scx_brep::{
    Brep, Document, Draft, EdgeId, EdgeShape, Entity, FaceId, Fin, RegionId, ShellId, ShellRecord,
    UseGeom, VertexId,
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

/// The relative rounding of a curvature from an analytic surface's second partials.
const CURVATURE_SLACK: f64 = 1e-9;

const fn defect() -> BooleanError {
    BooleanError::Defect(BooleanDefect::Build)
}

const fn non_manifold(at: Entity) -> BooleanError {
    BooleanError::Unsupported(BooleanUnsupported::NonManifold { at })
}

/// Lists the fins of `edge` (three or more) counter-clockwise about its direction. An odd
/// count is a defect unless `free` (a lamina or a sheet may end at the edge); `zone` is the
/// body's resolution, how far a lift may stand from the edge where none is recorded.
pub(crate) fn order_fins(
    draft: &mut Draft,
    edge: EdgeId,
    free: bool,
    zone: f64,
) -> Result<(), BooleanError> {
    let order = radial(draft, edge, free, zone)?;
    draft.adjacency_mut(edge).ok_or_else(defect)?.fins = order;
    Ok(())
}

/// How a fin's face leaves the edge at its middle.
#[derive(Debug, Clone, Copy)]
struct Leaving {
    fin: Fin,
    /// The counter-clockwise angle of `d = σ N × T` from the first fin's.
    angle: f64,
    /// The face's bend toward the counter-clockwise side past `d`: its normal curvature
    /// along `d`, with the use's sense. Larger is further counter-clockwise.
    bend: f64,
    /// The face's largest principal curvature, which bounds how far `bend` can move.
    kmax: f64,
}

/// The fins of `edge` in radial order. Faces leaving in one direction (tangent along the
/// edge, BO22) are ordered by their bends: past the edge the one bending further
/// counter-clockwise lies further counter-clockwise, as scx-brep's radial check reads it.
/// Bends that tie within what the lift's distance from the edge can move them are faces
/// that agree to second order across the edge, so the result may meet itself inside a
/// face: [`BooleanUnsupported::NonManifold`].
fn radial(draft: &Draft, edge: EdgeId, free: bool, zone: f64) -> Result<Vec<Fin>, BooleanError> {
    let e = draft.edge(edge).ok_or_else(defect)?;
    if !free && e.fins().len() % 2 == 1 {
        return Err(defect());
    }
    let Some(EdgeShape::Curve { curve, interval }) = e.shape() else {
        return Err(defect());
    };
    let rho = e.accuracy().map_or(zone, |r| r.acc.length().mm().max(zone));
    let s = 0.5 * (interval.min() + interval.max());
    let tan = curve
        .tangent_dir(s, Approach::Above)
        .ok_or_else(defect)?
        .as_vec();
    let refuse = || non_manifold(Entity::Edge(edge));
    let mut leaving: Vec<Leaving> = Vec::with_capacity(e.fins().len());
    let mut e1 = None;
    for u in e.uses() {
        let u = u.ok_or_else(defect)?;
        let UseGeom::Pcurve { curve: pc, .. } = u.geom() else {
            return Err(defect());
        };
        let uv = pc.point(s);
        let surface = u.face().surface();
        let n = surface
            .normal(uv, [Approach::Above; 2])
            .ok_or_else(refuse)?
            .as_vec();
        let d = (n.cross(tan) * u.sense().sign())
            .normalized()
            .ok_or_else(refuse)?
            .as_vec();
        let bend = surface
            .normal_curvature(uv, [Approach::Above; 2], d)
            .ok_or_else(refuse)?;
        let e1 = *e1.get_or_insert(d);
        let a = fmath::atan2(d.dot(tan.cross(e1)), d.dot(e1));
        let angle = if a < 0.0 { a + TAU } else { a };
        leaving.push(Leaving {
            fin: u.fin(),
            angle,
            bend: bend.along * u.sense().sign(),
            kmax: bend.largest,
        });
    }
    // Start past the widest gap, so no run of tied directions wraps round the start.
    leaving.sort_by(|x, y| x.angle.total_cmp(&y.angle));
    let n = leaving.len();
    let gap = |i: usize| -> f64 {
        let (Some(a), Some(b)) = (leaving.get(i), leaving.get((i + 1) % n)) else {
            return 0.0;
        };
        let g = b.angle - a.angle;
        if i + 1 == n { g + TAU } else { g }
    };
    let widest = (0..n)
        .max_by(|&i, &j| gap(i).total_cmp(&gap(j)))
        .ok_or_else(defect)?;
    if gap(widest) <= TIED {
        return Err(refuse());
    }
    leaving.rotate_left((widest + 1) % n);
    let base = leaving.first().map_or(0.0, |l| l.angle);
    for l in &mut leaving {
        let a = l.angle - base;
        l.angle = if a < 0.0 { a + TAU } else { a };
    }
    // Runs of tied directions, each ordered by bend.
    let mut order = Vec::with_capacity(n);
    let mut run: Vec<Leaving> = Vec::new();
    for (i, l) in leaving.iter().enumerate() {
        run.push(*l);
        if leaving
            .get(i + 1)
            .is_some_and(|m| m.angle - l.angle <= TIED)
        {
            continue;
        }
        run.sort_by(|x, y| x.bend.total_cmp(&y.bend));
        for w in run.windows(2) {
            if let [x, y] = w {
                // Moving the sample by `rho` turns `d` by `kmax rho` and moves the bend by
                // about twice `kmax^2 rho`; doubled again for the change of curvature
                // along the surface, plus the curvatures' rounding.
                let k = x.kmax.max(y.kmax).max(x.bend.abs()).max(y.bend.abs());
                let slack = 4.0 * (x.kmax * x.kmax + y.kmax * y.kmax) * rho + CURVATURE_SLACK * k;
                if y.bend - x.bend <= slack {
                    return Err(refuse());
                }
            }
        }
        order.extend(run.drain(..).map(|l| l.fin));
    }
    Ok(order)
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
#[derive(Debug, Clone)]
struct Placed {
    shell: ShellId,
    material: bool,
    sign: Sign,
    bbox: Extent3d,
    /// Its faces, in id order.
    faces: Vec<FaceId>,
}

impl Placed {
    /// The shells of `all` that may hold `self`: positive, of `material`, their boxes
    /// holding its box, and no face of theirs one of its own (a shell across a wall from
    /// it, which lies outside it there).
    fn holders(&self, all: &[Self], material: bool) -> Vec<Self> {
        all.iter()
            .filter(|q| {
                q.material == material
                    && q.sign == Sign::Positive
                    && q.shell != self.shell
                    && self.bbox.is_contained_by(&q.bbox, 0.0)
                    && !shares(&self.faces, &q.faces)
            })
            .cloned()
            .collect()
    }
}

impl Placed {
    /// The positive material shells whose boxes meet `self`'s, no face shared: the
    /// candidates when no box holds `self`'s. A face's box is an enclosure, not the
    /// tightest box (a sphere's reaches past its poles), so a cavity's wall can outgrow the
    /// box of the shell round it; the point test then decides (the box less its inscribed
    /// ball, whose wall touches each face at a point).
    fn holders_loose(&self, all: &[Self]) -> Vec<Self> {
        all.iter()
            .filter(|q| {
                q.material
                    && q.sign == Sign::Positive
                    && q.shell != self.shell
                    && !self.bbox.are_disjoint(&q.bbox, 0.0)
                    && !shares(&self.faces, &q.faces)
            })
            .cloned()
            .collect()
    }
}

/// Whether two id-ordered lists share an id.
fn shares(p: &[FaceId], q: &[FaceId]) -> bool {
    let (mut i, mut j) = (0, 0);
    while let (Some(a), Some(b)) = (p.get(i), q.get(j)) {
        match a.cmp(b) {
            core::cmp::Ordering::Less => i += 1,
            core::cmp::Ordering::Greater => j += 1,
            core::cmp::Ordering::Equal => return true,
        }
    }
    false
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
        let material = sh.region().is_solid();
        // A shell of laminas or sheets only bounds no volume: a void one lies in the void
        // round it, as a negative void shell does; a solid one cannot be.
        let sign = match sign {
            Sign::Zero if material => return Err(defect()),
            Sign::Zero => Sign::Negative,
            s => s,
        };
        placed.push(Placed {
            shell: s,
            material,
            sign,
            bbox: bbox.ok_or_else(defect)?,
            faces: sh.faces().map(|(f, _)| f.id()).collect(),
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
                [] => match p.holders_loose(&placed).as_slice() {
                    [] => return Err(nesting()),
                    [one] => Goes::With(one.shell),
                    many => {
                        let held = tests.holding(draft, &body, p, many, doc, budget, stop)?;
                        Goes::With(innermost(&held)?.ok_or_else(nesting)?)
                    }
                },
                [one] => Goes::With(one.shell),
                many => {
                    let held = tests.holding(draft, &body, p, many, doc, budget, stop)?;
                    Goes::With(innermost(&held)?.ok_or_else(nesting)?)
                }
            },
            // Round a lump: a cavity's void may hold it.
            (false, _) => {
                let cands = p.holders(&placed, false);
                if cands.is_empty() {
                    Goes::Outside
                } else {
                    let held = tests.holding(draft, &body, p, &cands, doc, budget, stop)?;
                    innermost(&held)?.map_or(Goes::Outside, Goes::With)
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
fn innermost(held: &[Placed]) -> Result<Option<ShellId>, BooleanError> {
    let volume = |p: &Placed| {
        let s = p.bbox.size();
        s.x * s.y * s.z
    };
    let mut best: Option<&Placed> = None;
    let mut tie = false;
    for h in held {
        match best {
            None => best = Some(h),
            Some(b) => match volume(h).total_cmp(&volume(b)) {
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
    Ok(best.map(|b| b.shell))
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
                PointClass::In(r) if r == *solid => held.push(c.clone()),
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
            // A lamina inside it: no part of what it encloses.
            [a, b] if a == p && b == p => {
                d.remove_face(f);
            }
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
