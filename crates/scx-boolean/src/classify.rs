// Written for solidcorex. Apache-2.0.
//! Stage 6, first half: classify (BO4, 72 §5.4 step 5, §14.29).
//!
//! Each face of the imprint's draft is a piece of one operand's face, and lies in the
//! other operand, out of it, or on it (on a face of it, within coincidence). The answer is
//! one per *component*: the pieces of one operand joined across edges where the operands do
//! not meet. Where they meet an edge has fins of both operands (a section, a lying piece,
//! an edge one shares along the other's), so the components stop there.
//!
//! - **Coincident pieces first.** A piece of a face stage 4 found coincident with a face of
//!   the other operand is classified by a point inside it: on that face (or another
//!   coincident with it), it is [`Class::On`], with whether the two materials lie on one
//!   side of it.
//! - **Seeds.** Each section's edge runs along its curve, so a piece using it in its own
//!   sense lies on the curve's left (as BQ2's [`Sides`](scx_bodyq::Sides) read it) and one
//!   using it against on the right: [`sides_along`] on the operand's face against the
//!   other's answers both, for each operand. A component two seeds answer differently is
//!   [`BooleanDefect::Seeds`].
//! - **The fallback.** A component no seed reached (no section bounds it, or every
//!   section's sides were [`NoClearSide`](scx_bodyq::BodyqUnsupported::NoClearSide)) is
//!   classified by [`classify_point`] at the middle of its first edge where the operands do
//!   not meet, or else at a point inside its first face.
//! - **The seeds' check.** Each seeded component is checked by a [`classify_point`] at one
//!   such edge middle, and one on the other side from its seed is
//!   [`BooleanDefect::Seeds`] (§14.35). A component with no free curve edge (a disk the
//!   sections alone bound) is not checked: a lone wrong piece leaves a section edge with
//!   one or three kept fins, which the build refuses, and a section whose sides are
//!   swapped flips its partner piece too, which has such an edge.
//!
//! A lying piece carries no contact, so it seeds nothing: the components on either side of
//! it are classified by a point (BQ2's first question, §14.29).

use std::collections::{BTreeMap, BTreeSet};

use scx_bodyq::{Along, BodyqError, BodyqUnsupported, PointClass, classify_point, sides_along};
use scx_brep::{
    Brep, Drafting, EdgeId, EdgeShape, Entity, Face, FaceId, FaceSide, RegionId, UvSide,
};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Sense;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;

use crate::error::{BooleanDefect, BooleanError, BooleanUnsupported};
use crate::faceface::FaceFace;
use crate::geom::{Operands, Side};
use crate::imprint::{Imprint, ring};

/// Where a piece of one operand lies against the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Class {
    /// In the other's material.
    In,
    /// Out of it.
    Out,
    /// On a face of it, within coincidence.
    On {
        /// Whether the two operands' materials lie on one side of the face.
        same: bool,
    },
}

/// What the classification found.
#[derive(Debug)]
pub(crate) struct Classes {
    /// Each draft face's operand face, by side.
    pub(crate) owner: BTreeMap<FaceId, (Side, FaceId)>,
    /// Each draft face's class.
    pub(crate) class: BTreeMap<FaceId, Class>,
}

/// How many edges' middles the fallback tries before a point inside the face.
const MIDDLES: usize = 4;

/// Classifies every face of `m`'s draft against the other operand.
pub(crate) fn classify(
    ops: Operands<'_>,
    ff: &FaceFace,
    m: &Imprint,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Classes, BooleanError> {
    let mut owner = BTreeMap::new();
    for (&(side, of), pieces) in &m.pieces {
        for &p in pieces {
            owner.insert(p, (side, of));
        }
    }
    let side_of = |f: FaceId| owner.get(&f).map(|o| o.0);
    let cuts: BTreeSet<EdgeId> = m.cuts.iter().copied().collect();
    // Where the operands meet: an edge with fins of both, or a cut.
    let mut met = BTreeSet::new();
    let mut set = Joins::new(owner.keys().copied());
    for e in m.draft.edges() {
        let mut sides = e.fins().map(|f| side_of(f.face));
        let first = sides.next().flatten();
        let mixed = cuts.contains(&e.id()) || sides.any(|s| s != first);
        if mixed {
            met.insert(e.id());
            continue;
        }
        // A collapsed edge (a sphere's pole) is a point: the pieces round it meet there
        // alone, and a cut through the pole leaves pieces of either class round it.
        if matches!(e.shape(), Some(EdgeShape::Collapsed)) {
            continue;
        }
        let mut fins = e.fins();
        if let Some(f0) = fins.next() {
            for f in fins {
                set.join(f0.face, f.face);
            }
        }
    }
    // Each component's faces, by its root, ascending.
    let mut comps: BTreeMap<FaceId, Vec<FaceId>> = BTreeMap::new();
    for &f in owner.keys() {
        comps.entry(set.root(f)).or_default().push(f);
    }
    let mut answer: BTreeMap<FaceId, Class> = BTreeMap::new();
    // Coincident pieces.
    let mut coincident: BTreeSet<(FaceId, FaceId)> = BTreeSet::new();
    for c in &ff.coincident {
        coincident.insert((c.faces[0], c.faces[1]));
    }
    let mut on: BTreeMap<FaceId, Class> = BTreeMap::new();
    for c in &ff.coincident {
        for side in Side::BOTH {
            let (mine, theirs) = match side {
                Side::A => (c.faces[0], c.faces[1]),
                Side::B => (c.faces[1], c.faces[0]),
            };
            let Some(pieces) = m.pieces.get(&(side, mine)) else {
                continue;
            };
            for &p in pieces {
                if on.contains_key(&p) {
                    continue;
                }
                stop.check()?;
                let q = inside(ops, m, side, p)?;
                let other = ops.of(side.other());
                let PointClass::On(Entity::Face(g)) =
                    classify_point(other, q, budget, stop).map_err(|e| bodyq(e, side, p))?
                else {
                    continue;
                };
                let pair = match side {
                    Side::A => (mine, g),
                    Side::B => (g, mine),
                };
                if g != theirs && !coincident.contains(&pair) {
                    continue;
                }
                let normals = if g == theirs {
                    c.same
                } else {
                    ff.coincident
                        .iter()
                        .find(|k| (k.faces[0], k.faces[1]) == pair)
                        .is_some_and(|k| k.same)
                };
                let backs = [material_back(ops.of(side), mine)?, material_back(other, g)?];
                let same = normals == (backs[0] == backs[1]);
                on.insert(p, Class::On { same });
            }
        }
    }
    for (&p, &c) in &on {
        put(&mut answer, &owner, &mut set, p, c)?;
    }
    // A component holding a coincident piece is that piece alone.
    for faces in comps.values() {
        if let [first, _, ..] = faces.as_slice()
            && faces.iter().any(|f| on.contains_key(f))
        {
            return Err(seeds(&owner, *first));
        }
    }
    // Seeds.
    for (s, &edge) in ff.sections.iter().zip(&m.sections) {
        let Some(view) = m.draft.edge(edge) else {
            return Err(BooleanError::Defect(BooleanDefect::Draft));
        };
        let fins: Vec<_> = view.fins().collect();
        for side in Side::BOTH {
            let (mine, theirs) = match side {
                Side::A => (s.faces[0], s.faces[1]),
                Side::B => (s.faces[1], s.faces[0]),
            };
            let mine_fins: Vec<_> = fins
                .iter()
                .filter(|f| side_of(f.face) == Some(side) && !on.contains_key(&f.face))
                .collect();
            if mine_fins.is_empty() {
                continue;
            }
            let (Some(fm), Some(ft)) = (ops.of(side).face(mine), ops.of(side.other()).face(theirs))
            else {
                return Err(BooleanError::Defect(BooleanDefect::Draft));
            };
            stop.check()?;
            let coincidence = ops.coincidence(fm.accuracy(), ft.accuracy());
            let along = Along {
                curve: &s.curve,
                span: s.t,
                contact: s.contact,
                coincidence: Length::from_mm(coincidence),
            };
            let sides = match sides_along(fm, ft, &along, budget, stop) {
                Ok(x) => x,
                Err(BodyqError::Unsupported(BodyqUnsupported::NoClearSide { .. })) => continue,
                Err(e) => return Err(bodyq(e, side, mine)),
            };
            let other = ops.of(side.other());
            for f in mine_fins {
                let r = match f.sense {
                    Sense::Forward => sides.left,
                    Sense::Reversed => sides.right,
                };
                put(&mut answer, &owner, &mut set, f.face, class_of(other, r))?;
            }
        }
    }
    // Each seeded component checked by one point (§14.35): a seed that reads the wrong
    // side would otherwise spread over its whole component unseen.
    for (&root, faces) in &comps {
        let Some(&seed) = answer.get(&set.root(root)) else {
            continue;
        };
        if matches!(seed, Class::On { .. }) {
            continue;
        }
        let Some(&(side, _)) = owner.get(&root) else {
            continue;
        };
        stop.check()?;
        check_seed(ops, m, side, faces, &met, seed, budget, stop)?;
    }
    // The fallback.
    for (&root, faces) in &comps {
        if answer.contains_key(&set.root(root)) {
            continue;
        }
        let Some(&(side, _)) = owner.get(&root) else {
            continue;
        };
        stop.check()?;
        let c = by_point(ops, m, side, faces, &met, budget, stop)?;
        put(&mut answer, &owner, &mut set, root, c)?;
    }
    let mut class = BTreeMap::new();
    for &f in owner.keys() {
        let c = answer
            .get(&set.root(f))
            .copied()
            .ok_or_else(|| seeds(&owner, f))?;
        class.insert(f, c);
    }
    Ok(Classes { owner, class })
}

/// Gives the component of `f` the class `c`: [`BooleanDefect::Seeds`] if it has another.
fn put(
    answer: &mut BTreeMap<FaceId, Class>,
    owner: &BTreeMap<FaceId, (Side, FaceId)>,
    set: &mut Joins,
    f: FaceId,
    c: Class,
) -> Result<(), BooleanError> {
    let root = set.root(f);
    match answer.get(&root) {
        Some(&was) if was != c => Err(seeds(owner, root)),
        _ => {
            answer.insert(root, c);
            Ok(())
        }
    }
}

/// The class of a component of `side` with `faces`, by [`classify_point`] against the
/// other operand.
fn by_point(
    ops: Operands<'_>,
    m: &Imprint,
    side: Side,
    faces: &[FaceId],
    met: &BTreeSet<EdgeId>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Class, BooleanError> {
    let first = *faces
        .first()
        .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
    point_class(ops, m, side, faces, met, budget, stop)?.ok_or_else(|| {
        BooleanError::Defect(BooleanDefect::Classify {
            operand: side.operand(),
            face: first,
        })
    })
}

/// Checks a seeded component of `side` with `faces` by [`classify_point`] at the middle of
/// one of its first [`MIDDLES`] curve edges where the operands do not meet:
/// [`BooleanDefect::Seeds`] if that point lies on the other side from the seed's. A
/// component with no such edge, whose middles all fall on the other operand, or whose
/// point the query cannot place, is not checked: a point inside its face costs about ten
/// times as much (§14.35).
pub(crate) fn check_seed(
    ops: Operands<'_>,
    m: &Imprint,
    side: Side,
    faces: &[FaceId],
    met: &BTreeSet<EdgeId>,
    seed: Class,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<(), BooleanError> {
    let Some(&first) = faces.first() else {
        return Ok(());
    };
    match by_middles(ops, m, side, faces, met, budget, stop) {
        Ok(Some(c)) if c != seed => Err(BooleanError::Defect(BooleanDefect::Seeds {
            operand: side.operand(),
            face: first,
        })),
        // A point the query cannot place leaves the seed unchecked, as before the check.
        Ok(_) | Err(BooleanError::Defect(BooleanDefect::Classify { .. })) => Ok(()),
        Err(e) => Err(e),
    }
}

/// The class of a component of `side` with `faces` by one point in a region of the other
/// operand: by [`by_middles`], else a point inside its first face. `None` when every point
/// tried is on the other operand.
fn point_class(
    ops: Operands<'_>,
    m: &Imprint,
    side: Side,
    faces: &[FaceId],
    met: &BTreeSet<EdgeId>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<Class>, BooleanError> {
    if let Some(c) = by_middles(ops, m, side, faces, met, budget, stop)? {
        return Ok(Some(c));
    }
    let first = *faces
        .first()
        .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
    let other = ops.of(side.other());
    let q = inside(ops, m, side, first)?;
    match classify_point(other, q, budget, stop).map_err(|x| bodyq(x, side, first))? {
        PointClass::In(r) => Ok(Some(class_of(other, r))),
        _ => Ok(None),
    }
}

/// The class of a component of `side` with `faces` at the middle of one of its first
/// [`MIDDLES`] curve edges where the operands do not meet: the first in a region of the
/// other operand answers. `None` when there is no such edge or every middle tried is on
/// the other operand.
fn by_middles(
    ops: Operands<'_>,
    m: &Imprint,
    side: Side,
    faces: &[FaceId],
    met: &BTreeSet<EdgeId>,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Option<Class>, BooleanError> {
    let other = ops.of(side.other());
    let mut tried = 0;
    for &f in faces {
        let Some(view) = m.draft.face(f) else {
            continue;
        };
        for u in view.uses() {
            if tried >= MIDDLES {
                return Ok(None);
            }
            let e = u.record().edge;
            if met.contains(&e) {
                continue;
            }
            let Some(edge) = m.draft.edge(e) else {
                continue;
            };
            let Some(EdgeShape::Curve { curve, interval }) = edge.shape() else {
                continue;
            };
            tried += 1;
            let p = curve.point(interval.mid());
            if let PointClass::In(r) =
                classify_point(other, p, budget, stop).map_err(|x| bodyq(x, side, f))?
            {
                return Ok(Some(class_of(other, r)));
            }
        }
    }
    Ok(None)
}

/// In or out, by the other operand's region `r`.
fn class_of(other: &Brep, r: RegionId) -> Class {
    if other.region(r).is_some_and(scx_brep::Region::is_solid) {
        Class::In
    } else {
        Class::Out
    }
}

/// Whether the operand's face `f` has its material behind it (on its back).
pub(crate) fn material_back(body: &Brep, f: FaceId) -> Result<bool, BooleanError> {
    let face = body
        .face(f)
        .ok_or(BooleanError::Defect(BooleanDefect::Draft))?;
    Ok(face.shell(FaceSide::Back).region().is_solid())
}

/// The seeds' defect at the component of `f`.
fn seeds(owner: &BTreeMap<FaceId, (Side, FaceId)>, f: FaceId) -> BooleanError {
    let operand = owner.get(&f).map_or(Side::A, |o| o.0).operand();
    BooleanError::Defect(BooleanDefect::Seeds { operand, face: f })
}

/// A query's error at `side`'s draft face `f`.
fn bodyq(e: BodyqError, side: Side, f: FaceId) -> BooleanError {
    let operand = side.operand();
    match e {
        BodyqError::Exhausted { limit } => BooleanError::Exhausted { limit },
        BodyqError::Cancelled => BooleanError::Cancelled,
        BodyqError::Unsupported(BodyqUnsupported::Nurbs { at }) => {
            BooleanError::Unsupported(BooleanUnsupported::Nurbs {
                operand: side.other().operand(),
                at,
            })
        }
        BodyqError::Unsupported(BodyqUnsupported::NonManifold { at }) => {
            BooleanError::Unsupported(BooleanUnsupported::NonManifold { at })
        }
        _ => BooleanError::Defect(BooleanDefect::Classify { operand, face: f }),
    }
}

/// A point inside the draft face `f` of `side`, away from its boundary.
fn inside(ops: Operands<'_>, m: &Imprint, side: Side, f: FaceId) -> Result<Point3, BooleanError> {
    let _ = ops;
    let fail = BooleanError::Defect(BooleanDefect::Classify {
        operand: side.operand(),
        face: f,
    });
    let view = m.draft.face(f).ok_or(fail)?;
    let uv = inner_uv(view).ok_or(fail)?;
    Ok(view.surface().point(uv))
}

/// The scan lines' fractions of the box, in order: off the middle by an odd step, so a
/// line seldom runs through a vertex.
const SCANS: [f64; 5] = [0.513_791, 0.381_966, 0.618_034, 0.257_135, 0.742_865];

/// A uv point inside `face`'s region: the middle of the widest run of a scan line across
/// its loops' samples that the face's own test proves inside.
pub(crate) fn inner_uv(face: Face<'_, Drafting<'_>>) -> Option<Point2> {
    let rings: Vec<Vec<Point2>> = face.record().loops.iter().map(ring).collect();
    let mut lo = Point2::new(f64::INFINITY, f64::INFINITY);
    let mut hi = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in rings.iter().flatten() {
        lo = Point2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    if !(lo.x <= hi.x && lo.y <= hi.y) {
        return None;
    }
    for across in [false, true] {
        for f in SCANS {
            let mut hits = Vec::new();
            let at = if across {
                lo.x + f * (hi.x - lo.x)
            } else {
                lo.y + f * (hi.y - lo.y)
            };
            for r in &rings {
                let n = r.len();
                for i in 0..n {
                    let (Some(a), Some(b)) = (r.get(i), r.get((i + 1) % n)) else {
                        continue;
                    };
                    let (a0, a1, b0, b1) = if across {
                        (a.x, a.y, b.x, b.y)
                    } else {
                        (a.y, a.x, b.y, b.x)
                    };
                    if (a0 > at) != (b0 > at) {
                        hits.push(a1 + (at - a0) / (b0 - a0) * (b1 - a1));
                    }
                }
            }
            hits.sort_by(f64::total_cmp);
            let mut runs: Vec<(f64, f64)> = hits
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[s, e]| (s, e))
                .collect();
            runs.sort_by(|p, q| (q.1 - q.0).total_cmp(&(p.1 - p.0)));
            for (s, e) in runs {
                let mid = 0.5 * (s + e);
                let uv = if across {
                    Point2::new(at, mid)
                } else {
                    Point2::new(mid, at)
                };
                if face.contains_uv(uv) == Some(UvSide::Inside) {
                    return Some(uv);
                }
            }
        }
    }
    None
}

/// A union-find over faces.
pub(crate) struct Joins {
    up: BTreeMap<FaceId, FaceId>,
}

impl Joins {
    pub(crate) fn new(faces: impl Iterator<Item = FaceId>) -> Self {
        Self {
            up: faces.map(|f| (f, f)).collect(),
        }
    }

    /// The component's root: its lowest face.
    pub(crate) fn root(&mut self, f: FaceId) -> FaceId {
        let mut r = f;
        while let Some(&u) = self.up.get(&r) {
            if u == r {
                break;
            }
            r = u;
        }
        let mut x = f;
        while let Some(&u) = self.up.get(&x) {
            if u == x {
                break;
            }
            self.up.insert(x, r);
            x = u;
        }
        r
    }

    /// Joins the components of `f` and `g`; the lower root stays.
    pub(crate) fn join(&mut self, f: FaceId, g: FaceId) {
        let (rf, rg) = (self.root(f), self.root(g));
        if rf == rg || !self.up.contains_key(&rf) || !self.up.contains_key(&rg) {
            return;
        }
        let (lo, hi) = if rf < rg { (rf, rg) } else { (rg, rf) };
        self.up.insert(hi, lo);
    }
}
