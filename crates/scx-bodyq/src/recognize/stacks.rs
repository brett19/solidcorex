// Written for solidcorex. Apache-2.0.
//! [`hole_stacks`]: the bores that line up through two or more touching bodies (research 74
//! §2.3, §33, BW6; the pin's `eval::hole_stacks::stacks`).
//!
//! Round holes whose axes sit within `axis_tolerance` of one line form one line of holes;
//! along it, runs of bores whose extents sit within `touching` of each other are stacks, one
//! layer per body's stretch (a counterbore and its through hole are one layer, of their
//! narrowest opening), and a stack needs two bodies. A slot joins the line of any round hole
//! that runs through it; slots that no round hole runs through stack on the axis through the
//! middle of the length they share.
//!
//! **The two lengths are the caller's** ([`Options`]): how far apart layers may be and how
//! far apart axes may be are the design's, not the kernel's. Everything else is the
//! coincidence of the bodies: two axes are parallel when they part by no more than
//! `axis_tolerance` over the bores' extent, so there is no `1 - 1e-5` on a dot product.
//!
//! **The work is the budget's.** The pin refused past 4,000,000 comparisons and 20,000
//! faces; here each comparison charges one unit of the caller's budget, and running out is
//! [`BodyqError::Exhausted`]. A grid of cells the width of `axis_tolerance` keeps a design of
//! many parallel holes to comparing neighbours.
//!
//! **Deterministic.** Bores are visited in `(body, bore)` order, cells in key order, groups
//! in root order; stacks are sorted by their start points.

use std::collections::{BTreeMap, BTreeSet};

use scx_num::budget::{Budget, Stop};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};

use scx_brep::Brep;

use super::bores::{Bore, Slot, bores, parallel};
use crate::error::{BodyqError, BodyqRefusal};

/// What shapes the stacks.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    touching: f64,
    axis_tolerance: f64,
    slots: bool,
    diameters: Vec<(f64, f64)>,
}

impl Options {
    /// The largest gap between the layers of one stack (`touching`, at least 0) and the
    /// largest offset between the axes of one line of holes (`axis_tolerance`, above 0).
    ///
    /// # Errors
    ///
    /// [`BodyqRefusal::BadOption`] for a length that is not finite, a `touching` below 0 or
    /// an `axis_tolerance` of 0 or less.
    pub fn new(touching: Length, axis_tolerance: Length) -> Result<Self, BodyqError> {
        let (t, a) = (touching.mm(), axis_tolerance.mm());
        if !(t.is_finite() && t >= 0.0) {
            return Err(BodyqError::Refused(BodyqRefusal::BadOption {
                name: "touching",
            }));
        }
        if !(a.is_finite() && a > 0.0) {
            return Err(BodyqError::Refused(BodyqRefusal::BadOption {
                name: "axis_tolerance",
            }));
        }
        Ok(Self {
            touching: t,
            axis_tolerance: a,
            slots: true,
            diameters: Vec::new(),
        })
    }

    /// Whether obround slots count (they do unless set off). [`hole_stacks`] only.
    #[must_use]
    pub fn with_slots(mut self, slots: bool) -> Self {
        self.slots = slots;
        self
    }

    /// Only bores whose diameter (a slot's width) lies in one of these ranges `(min, max)`
    /// count; none given takes every size. [`hole_stacks`] only.
    ///
    /// # Errors
    ///
    /// [`BodyqRefusal::BadOption`] for a range that is not finite or has `min > max`.
    pub fn with_diameters(mut self, ranges: &[(Length, Length)]) -> Result<Self, BodyqError> {
        let mut out = Vec::with_capacity(ranges.len());
        for &(lo, hi) in ranges {
            let (lo, hi) = (lo.mm(), hi.mm());
            if !(lo.is_finite() && hi.is_finite() && lo <= hi) {
                return Err(BodyqError::Refused(BodyqRefusal::BadOption {
                    name: "diameters",
                }));
            }
            out.push((lo, hi));
        }
        self.diameters = out;
        Ok(self)
    }

    fn keeps(&self, b: &Bore) -> bool {
        (self.slots || b.slot.is_none())
            && (self.diameters.is_empty()
                || self
                    .diameters
                    .iter()
                    .any(|&(lo, hi)| b.d >= lo && b.d <= hi))
    }
}

/// The hole stacks among `bodies`: every set of coaxial bores through two or more touching
/// bodies, and the bores in none. Each body's [`bores`] are read first; a slot with
/// `with_slots(false)` and a bore outside every diameter range count as not there. The
/// indices in the answer are into the bores each body kept, in [`bores`] order.
///
/// One unit of `budget` per face read and per comparison made.
///
/// # Errors
///
/// As [`bores`] and [`stack_bores`].
pub fn hole_stacks(
    bodies: &[&Brep],
    o: &Options,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Stacks, BodyqError> {
    let mut all: Vec<Vec<Bore>> = Vec::with_capacity(bodies.len());
    for b in bodies {
        let mut kept = bores(b, budget, stop)?;
        kept.retain(|x| o.keeps(x));
        all.push(kept);
    }
    let slices: Vec<&[Bore]> = all.iter().map(Vec::as_slice).collect();
    stack_bores(&slices, o, budget, stop)
}

/// One layer of a stack: the bores of one body that run together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    /// The body, by its index into the `bodies` given.
    pub body: usize,
    /// The layer's narrowest bore, by its index into that body's bores.
    pub bore: usize,
    /// The layer's narrowest diameter.
    pub d: f64,
    /// The extent from the stack's start.
    pub t: [f64; 2],
    /// The narrowest bore's slot shape.
    pub slot: Option<Slot>,
}

/// Bores that line up through two or more touching bodies.
#[derive(Debug, Clone, PartialEq)]
pub struct Stack {
    /// Unit, from the start into the stack.
    pub axis: UnitVec3,
    /// The point on the axis the first layer starts at.
    pub start: Point3,
    /// The clamped thickness.
    pub grip: f64,
    /// The point on the axis the last layer ends at.
    pub end: Point3,
    /// The stack's layers, from its start along `axis`.
    pub layers: Vec<Layer>,
}

/// A bore in no stack, oriented the stack's way round: the largest component of `axis`
/// negative, `at` its entry point and `depth` its wall's extent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lone {
    /// The body, by its index into the `bodies` given.
    pub body: usize,
    /// The bore, by its index into that body's bores.
    pub bore: usize,
    /// The bore's entry point.
    pub at: Point3,
    /// Unit axis, from the entry point into the bore.
    pub axis: UnitVec3,
    /// The bore's diameter.
    pub d: f64,
    /// The wall's extent.
    pub depth: f64,
    /// The slot's shape.
    pub slot: Option<Slot>,
}

/// The stacks among bodies' bores, and the bores in none.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stacks {
    /// The stacks, ordered by their start points.
    pub stacks: Vec<Stack>,
    /// The bores in no stack.
    pub lone: Vec<Lone>,
}

/// One line of holes: its axis and a point on it, and the bores that sit on it.
struct Line {
    point: Vec3,
    axis: Vec3,
    members: Vec<usize>,
}

/// A bore's reference: body and index.
type Ref = (usize, usize);

/// What a pass charges and polls.
struct Meter<'a> {
    budget: &'a mut Budget,
    stop: &'a Stop,
    n: u64,
}

impl Meter<'_> {
    fn tick(&mut self) -> Result<(), BodyqError> {
        self.budget.charge(1)?;
        self.n += 1;
        if self.n.is_multiple_of(1024) {
            self.stop.check()?;
        }
        Ok(())
    }
}

/// The stacks among `bodies`' bores (each body's, from [`bores`](super::bores)), and the bores
/// in none.
///
/// # Errors
///
/// [`BodyqError::Exhausted`] when the comparisons outrun `budget`, [`BodyqError::Cancelled`]
/// when `stop` is set.
pub fn stack_bores(
    bodies: &[&[Bore]],
    o: &Options,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Stacks, BodyqError> {
    let refs: Vec<Ref> = bodies
        .iter()
        .enumerate()
        .flat_map(|(b, bs)| (0..bs.len()).map(move |i| (b, i)))
        .collect();
    let bore = |r: usize| -> Option<&Bore> {
        refs.get(r)
            .and_then(|&(b, i)| bodies.get(b).and_then(|bs| bs.get(i)))
    };
    let mut meter = Meter { budget, stop, n: 0 };
    let mut parent: Vec<usize> = (0..refs.len()).collect();
    let (rounds, slots): (Vec<usize>, Vec<usize>) =
        (0..refs.len()).partition(|&i| bore(i).is_some_and(|b| b.slot.is_none()));
    // Round holes join through buckets of their lines' feet (each line's nearest point to the
    // origin): lines within `axis_tolerance` have feet in the same or a neighbouring bucket,
    // so only those are compared, exactly.
    let cell = o.axis_tolerance;
    let mut cells: BTreeMap<[i64; 3], Vec<usize>> = BTreeMap::new();
    for &r in &rounds {
        meter.tick()?;
        if let Some(b) = bore(r)
            && let Some(c) = foot(b, cell)
        {
            cells.entry(c).or_default().push(r);
        }
    }
    for (here, members) in &cells {
        for dx in -1i64..=1 {
            for dy in -1i64..=1 {
                for dz in -1i64..=1 {
                    // A bore far enough out to saturate its bucket has no representable
                    // neighbour past the edge: checked adds skip that cell.
                    let (Some(nx), Some(ny), Some(nz)) = (
                        here[0].checked_add(dx),
                        here[1].checked_add(dy),
                        here[2].checked_add(dz),
                    ) else {
                        continue;
                    };
                    let Some(others) = cells.get(&[nx, ny, nz]) else {
                        continue;
                    };
                    for &i in members {
                        for &j in others {
                            if i >= j {
                                continue; // each pair once, over both buckets
                            }
                            meter.tick()?;
                            let (Some(a), Some(b)) = (bore(i), bore(j)) else {
                                continue;
                            };
                            if axes_parallel(a, b, o.axis_tolerance)
                                && apart(a, b) <= o.axis_tolerance
                            {
                                join(&mut parent, i, j);
                            }
                        }
                    }
                }
            }
        }
    }
    // Each line of round holes, and the slots its axis runs through.
    let mut threaded = vec![false; refs.len()];
    let mut lines: Vec<Line> = Vec::new();
    for members in groups(&mut parent, &rounds) {
        let Some(first) = members.first().and_then(|&i| bore(i)) else {
            continue;
        };
        let (p, axis) = (first.point.to_vec(), canonical(*first.axis));
        let mut members = members;
        for &s in &slots {
            meter.tick()?;
            if let Some(sl) = bore(s)
                && parallel(*sl.axis, axis, reach(sl), o.axis_tolerance)
                && offset(p, axis, sl) <= o.axis_tolerance
            {
                // The slot joins the line only where a round bore could actually run with
                // it: a bore far along the axis never reaches the slot's stretch.
                let st = extent(sl, p, axis);
                let near = members.iter().any(|&m| {
                    bore(m).is_some_and(|b| {
                        let (m0, m1) = extent(b, p, axis);
                        m0 <= st.1 + o.touching && st.0 <= m1 + o.touching
                    })
                });
                if near {
                    members.push(s);
                    if let Some(x) = threaded.get_mut(s) {
                        *x = true;
                    }
                }
            }
        }
        lines.push(Line {
            point: p,
            axis,
            members,
        });
    }
    // Slots that no round hole runs through, on one centre line: the axis through the middle
    // of the length they share.
    let free: Vec<usize> = slots
        .iter()
        .copied()
        .filter(|&s| !threaded.get(s).copied().unwrap_or(true))
        .collect();
    for (k, &i) in free.iter().enumerate() {
        for &j in free.iter().skip(k + 1) {
            meter.tick()?;
            if let (Some(a), Some(b)) = (bore(i), bore(j))
                && same_slot_line(a, b, o.axis_tolerance)
            {
                join(&mut parent, i, j);
            }
        }
    }
    for members in groups(&mut parent, &free) {
        let Some(first) = members.first().and_then(|&i| bore(i)) else {
            continue;
        };
        let Some(sa) = first.slot else { continue };
        // A chain of slots can run past the end of where it began (slots centred at 0, 5,
        // 10 and 15 mm): every pair meets its neighbour, but no one axis runs through all.
        // Cut the chain into runs that share a centre interval, earliest first, and run
        // each run's axis through the middle of what it shares.
        let mut spans: Vec<(f64, f64, usize)> = members
            .iter()
            .filter_map(|&i| bore(i).map(|h| (i, h)))
            .filter_map(|(i, h)| {
                let s = h.slot?;
                let c = (h.point - first.point).dot(*sa.along);
                let half = ((s.length - h.d) / 2.0).max(0.0);
                Some((c, half, i))
            })
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
        let mut run: Vec<usize> = Vec::new();
        let mut lo = f64::NEG_INFINITY;
        let mut hi = f64::INFINITY;
        for (c, half, i) in spans {
            if lo.max(c - half) > hi.min(c + half) && !run.is_empty() {
                let mid = lo / 2.0 + hi / 2.0;
                lines.push(Line {
                    point: first.point.to_vec() + *sa.along * mid,
                    axis: canonical(*first.axis),
                    members: std::mem::take(&mut run),
                });
                lo = f64::NEG_INFINITY;
                hi = f64::INFINITY;
            }
            run.push(i);
            lo = lo.max(c - half);
            hi = hi.min(c + half);
        }
        if !run.is_empty() {
            let mid = lo / 2.0 + hi / 2.0;
            lines.push(Line {
                point: first.point.to_vec() + *sa.along * mid,
                axis: canonical(*first.axis),
                members: run,
            });
        }
    }
    // Along each line, runs of extents that sit within `touching`, one layer per body's
    // stretch: a clevis around its lug gives two layers of one body.
    let mut in_stack = vec![false; refs.len()];
    let mut out = Vec::new();
    for line in &lines {
        let mut pieces: Vec<(f64, f64, usize)> = Vec::new();
        for &m in &line.members {
            if let Some(h) = bore(m) {
                let (t0, t1) = extent(h, line.point, line.axis);
                pieces.push((t0, t1, m));
            }
        }
        pieces.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.cmp(&b.2))
        });
        let mut runs: Vec<Vec<(f64, f64, usize)>> = Vec::new();
        let mut reach_end = f64::NEG_INFINITY;
        for piece in pieces {
            match runs.last_mut() {
                Some(run) if piece.0 <= reach_end + o.touching => {
                    run.push(piece);
                    reach_end = reach_end.max(piece.1);
                }
                _ => {
                    runs.push(vec![piece]);
                    reach_end = piece.1;
                }
            }
        }
        for run in runs {
            let mut layers: Vec<(Layer, Vec<usize>)> = Vec::new();
            for (t0, t1, m) in &run {
                let (Some(h), Some(&(body, bi))) = (bore(*m), refs.get(*m)) else {
                    continue;
                };
                let layer = Layer {
                    body,
                    bore: bi,
                    d: h.d,
                    t: [*t0, *t1],
                    slot: h.slot,
                };
                match layers.last_mut() {
                    Some((l, ids)) if l.body == body => {
                        l.t[0] = l.t[0].min(layer.t[0]);
                        l.t[1] = l.t[1].max(layer.t[1]);
                        if layer.d < l.d {
                            l.d = layer.d;
                            l.slot = layer.slot;
                            l.bore = layer.bore;
                        }
                        ids.push(*m);
                    }
                    _ => layers.push((layer, vec![*m])),
                }
            }
            let distinct: BTreeSet<usize> = layers.iter().map(|(l, _)| l.body).collect();
            if distinct.len() < 2 {
                continue;
            }
            let t_start = layers
                .iter()
                .map(|(l, _)| l.t[0])
                .fold(f64::INFINITY, f64::min);
            let t_end = layers
                .iter()
                .map(|(l, _)| l.t[1])
                .fold(f64::NEG_INFINITY, f64::max);
            for (_, ids) in &layers {
                for i in ids {
                    if let Some(x) = in_stack.get_mut(*i) {
                        *x = true;
                    }
                }
            }
            let layers = layers
                .into_iter()
                .map(|(mut l, _)| {
                    l.t[0] -= t_start;
                    l.t[1] -= t_start;
                    l
                })
                .collect();
            let Some(axis) = line.axis.normalized() else {
                continue;
            };
            out.push(Stack {
                axis,
                start: (line.point + line.axis * t_start).to_point(),
                grip: t_end - t_start,
                end: (line.point + line.axis * t_end).to_point(),
                layers,
            });
        }
    }
    out.sort_by(|a, b| {
        a.start
            .x
            .total_cmp(&b.start.x)
            .then(a.start.y.total_cmp(&b.start.y))
            .then(a.start.z.total_cmp(&b.start.z))
    });
    let lone = (0..refs.len())
        .filter(|i| !in_stack.get(*i).copied().unwrap_or(true))
        .filter_map(|i| {
            let (body, index) = refs.get(i).copied()?;
            let h = bodies.get(body)?.get(index)?;
            // The stack's way round: the largest component negative, so a lone hole reports
            // from its top, and its entry point and depth come with it.
            let axis = canonical(*h.axis);
            let axis = axis.normalized()?;
            let t = if axis.dot(*h.axis) >= 0.0 {
                h.t
            } else {
                [-h.t[1], -h.t[0]]
            };
            Some(Lone {
                body,
                bore: index,
                at: (h.point.to_vec() + *axis * t[0]).to_point(),
                axis,
                d: h.d,
                depth: t[1] - t[0],
                slot: h.slot,
            })
        })
        .collect();
    Ok(Stacks { stacks: out, lone })
}

/// The extent a bore's wall reaches along its axis.
fn reach(h: &Bore) -> f64 {
    h.t[1] - h.t[0]
}

fn axes_parallel(a: &Bore, b: &Bore, tol: f64) -> bool {
    parallel(*a.axis, *b.axis, reach(a).max(reach(b)), tol)
}

/// A line's foot: its nearest point to the origin, as bucket indices of side `cell`.
fn foot(b: &Bore, cell: f64) -> Option<[i64; 3]> {
    let q = b.point.to_vec();
    let p = q - *b.axis * q.dot(*b.axis);
    p.is_finite()
        .then(|| [bucket(p.x, cell), bucket(p.y, cell), bucket(p.z, cell)])
}

/// The bucket a coordinate falls in. The cast saturates, so a coordinate beyond any bucket
/// only collides with the other coordinates out there, which the exact tests then reject.
fn bucket(x: f64, cell: f64) -> i64 {
    (x / cell).floor() as i64
}

/// Union-find root.
fn find(p: &[usize], mut i: usize) -> usize {
    for _ in 0..p.len() {
        match p.get(i) {
            Some(up) if *up != i => i = *up,
            _ => break,
        }
    }
    i
}

fn join(p: &mut [usize], i: usize, j: usize) {
    let (ri, rj) = (find(p, i), find(p, j));
    if let Some(x) = p.get_mut(ri.max(rj)) {
        *x = ri.min(rj);
    }
}

/// The groups of a union-find, each with its members in id order, the groups in root order.
fn groups(parent: &mut [usize], ids: &[usize]) -> Vec<Vec<usize>> {
    let mut by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &i in ids {
        let root = find(parent, i);
        by_root.entry(root).or_default().push(i);
    }
    by_root.into_values().collect()
}

/// The distance, across `axis`, from the axis line through `p` to a bore's centre line (a
/// point for a round hole, a segment for a slot).
fn offset(p: Vec3, axis: Vec3, h: &Bore) -> f64 {
    let flat = |v: Vec3| v - axis * v.dot(axis);
    let w = flat(h.point.to_vec() - p);
    match h.slot {
        None => w.length(),
        Some(s) => {
            let half = ((s.length - h.d) / 2.0).max(0.0);
            let along = w.dot(*s.along).clamp(-half, half);
            (w - *s.along * along).length()
        }
    }
}

/// A bore's extent along `axis`, measured from `p`: its own extent, turned round when its
/// axis points the other way.
fn extent(h: &Bore, p: Vec3, axis: Vec3) -> (f64, f64) {
    let base = (h.point.to_vec() - p).dot(axis);
    let d = h.axis.dot(axis);
    // A bore's ends land on the line at its own axial step times how much of its axis points
    // along the line's, either way round.
    let (t0, t1) = (h.t[0] * d.abs(), h.t[1] * d.abs());
    if d >= 0.0 {
        (base + t0, base + t1)
    } else {
        (base - t1, base - t0)
    }
}

/// How far apart two round bores' lines sit over the stretch the bores themselves reach: the
/// perpendicular distance from `a`'s line to `b`'s, at each bore's two ends. Skewed lines
/// drift apart along the axis, so one measurement at a frame origin can read tight while the
/// bores sit apart; exactly parallel lines measure the same at every end.
fn apart(a: &Bore, b: &Bore) -> f64 {
    let (ap, bp) = (a.point.to_vec(), b.point.to_vec());
    let (b0, b1) = extent(b, ap, *a.axis);
    let at = |s: f64| {
        let w = ap + *a.axis * s - bp;
        (w - *b.axis * w.dot(*b.axis)).length()
    };
    at(a.t[0]).max(at(a.t[1])).max(at(b0)).max(at(b1))
}

/// Two slots on one axis line: parallel, the same way across, one centre line, overlapping.
fn same_slot_line(a: &Bore, b: &Bore, tol: f64) -> bool {
    let (Some(sa), Some(sb)) = (a.slot, b.slot) else {
        return false;
    };
    let across = sa.length.max(sb.length);
    if !axes_parallel(a, b, tol) || !parallel(*sa.along, *sb.along, across, tol) {
        return false;
    }
    let w = b.point.to_vec() - a.point.to_vec();
    let flat = w - *a.axis * w.dot(*a.axis);
    let along = flat.dot(*sa.along);
    let off = (flat - *sa.along * along).length();
    let (ha, hb) = (
        ((sa.length - a.d) / 2.0).max(0.0),
        ((sb.length - b.d) / 2.0).max(0.0),
    );
    off <= tol && along.abs() <= ha + hb + tol
}

/// The stack's way along an axis: the largest component negative (-z for a vertical axis), so
/// a stack of plates lying on the XY plane starts at its top.
fn canonical(a: Vec3) -> Vec3 {
    let big = if a.x.abs() >= a.y.abs() && a.x.abs() >= a.z.abs() {
        a.x
    } else if a.y.abs() >= a.z.abs() {
        a.y
    } else {
        a.z
    };
    if big > 0.0 { -a } else { a }
}
