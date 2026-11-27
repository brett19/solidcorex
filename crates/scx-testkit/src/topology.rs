// Written for solidcorex. Apache-2.0.
//! Body structure invariants and the pole and seam rules as predicates on plain data
//! (research 63 T7): the oracle a body's `finish` check is tested against.
//!
//! A [`Body`] is incidence only: indices, senses and sides, no geometry. A test builds one
//! from the library's body by a thin adapter, so the library and the oracle share no code.
//! [`violations`] lists every invariant that fails, written for clarity, not speed: maps
//! and searches where the library uses sorted arrays and disjoint sets, so the two do not
//! share a bug.
//!
//! The pole and seam predicates ([`jumps`], [`dog_leg`], [`passes_pole`]) work on a
//! pcurve's dense uv samples, so they judge the curve itself rather than the library's
//! fixed sampling.
//!
//! [`euler`] gives a shell's Euler-Poincaré characteristic, and [`genus`] its genus where
//! the shell is closed.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A use of an edge by a face, along the edge or against it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Use {
    /// The edge's index.
    pub edge: usize,
    /// Along the edge's direction.
    pub forward: bool,
}

/// A face: its two sides' shells (front, then back) and its loops of uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Face {
    /// The shell on the front (where the surface normal points), then the back.
    pub sides: [usize; 2],
    /// Each loop's uses, in order round it.
    pub loops: Vec<Vec<Use>>,
}

/// An edge: its two ends, whether it is collapsed to a point, and its fins in radial
/// order (counter-clockwise about its direction) as (face, forward).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    /// Start and end vertex.
    pub ends: [usize; 2],
    /// A collapsed edge (a pole) has no length and no radial order.
    pub collapsed: bool,
    /// The fins, as (face index, forward).
    pub fins: Vec<(usize, bool)>,
}

/// A shell: its region, its wire edges and its lone vertices.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Shell {
    /// The region it bounds.
    pub region: usize,
    /// Edges in the shell with no face.
    pub wires: Vec<usize>,
    /// Vertices in the shell with no edge.
    pub lone: Vec<usize>,
}

/// A body's incidence. Region `0` is the outside. `None` in a table is a removed record.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Body {
    /// Regions; `true` is solid. Region 0 is the outside.
    pub regions: Vec<Option<bool>>,
    /// Shells.
    pub shells: Vec<Option<Shell>>,
    /// Faces.
    pub faces: Vec<Option<Face>>,
    /// Edges.
    pub edges: Vec<Option<Edge>>,
    /// Vertices: whether each slot is live.
    pub vertices: Vec<bool>,
}

/// One failed invariant.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Violation {
    /// The outside region (0) is gone or solid.
    Outside,
    /// A reference names no live record.
    Dangling,
    /// A wire edge has fins.
    WireWithFins {
        /// The edge.
        edge: usize,
    },
    /// A face has no loop.
    FaceWithoutLoop {
        /// The face.
        face: usize,
    },
    /// A loop has no use.
    EmptyLoop {
        /// The face.
        face: usize,
    },
    /// A use does not start where the one before it ends.
    LoopNotConnected {
        /// The face.
        face: usize,
    },
    /// An edge's uses by a face and its fins on that face are not the same multiset, each
    /// once (radial completeness).
    Radial {
        /// The edge.
        edge: usize,
    },
    /// A face uses an edge twice in one sense.
    RepeatedUse {
        /// The face.
        face: usize,
    },
    /// A vertex is on two loops of one face.
    VertexOnTwoLoops {
        /// The face.
        face: usize,
    },
    /// A collapsed edge with two different ends.
    CollapsedEnds {
        /// The edge.
        edge: usize,
    },
    /// An edge with neither a fin nor a shell.
    LooseEdge {
        /// The edge.
        edge: usize,
    },
    /// Two radially consecutive fins name different shells for the sector between them.
    SidesDisagree {
        /// The edge.
        edge: usize,
    },
    /// A vertex on no edge and in no shell.
    LooseVertex {
        /// The vertex.
        vertex: usize,
    },
    /// A shell with nothing in it.
    EmptyShell {
        /// The shell.
        shell: usize,
    },
    /// A shell in more than one piece.
    ShellNotConnected {
        /// The shell.
        shell: usize,
    },
    /// A region other than the outside with no shell, or the outside with none while some
    /// region has one.
    RegionWithoutShell {
        /// The region.
        region: usize,
    },
}

fn live<T>(table: &[Option<T>]) -> impl Iterator<Item = (usize, &T)> {
    table
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.as_ref().map(|r| (i, r)))
}

fn get<T>(table: &[Option<T>], i: usize) -> Option<&T> {
    table.get(i).and_then(Option::as_ref)
}

/// The vertex a use starts at, and the one it ends at.
fn ends(body: &Body, u: Use) -> Option<(usize, usize)> {
    let [a, b] = get(&body.edges, u.edge)?.ends;
    Some(if u.forward { (a, b) } else { (b, a) })
}

/// The face side (0 front, 1 back) that lies after a fin, counter-clockwise about its
/// edge: the front for a use along the edge.
const fn after(forward: bool) -> usize {
    if forward { 0 } else { 1 }
}

/// Whether every reference names a live record. The other predicates assume it.
#[must_use]
pub fn resolves(body: &Body) -> bool {
    let vertex = |v: usize| body.vertices.get(v).copied().unwrap_or(false);
    let shells_ok = live(&body.shells).all(|(_, s)| {
        get(&body.regions, s.region).is_some()
            && s.wires.iter().all(|&e| get(&body.edges, e).is_some())
            && s.lone.iter().all(|&v| vertex(v))
    });
    let faces_ok = live(&body.faces).all(|(_, f)| {
        f.sides.iter().all(|&s| get(&body.shells, s).is_some())
            && f.loops
                .iter()
                .flatten()
                .all(|u| get(&body.edges, u.edge).is_some())
    });
    let edges_ok = live(&body.edges).all(|(_, e)| {
        e.ends.iter().all(|&v| vertex(v))
            && e.fins.iter().all(|&(f, _)| get(&body.faces, f).is_some())
    });
    shells_ok && faces_ok && edges_ok
}

/// Every invariant that fails, sorted. Empty for a valid body.
#[must_use]
pub fn violations(body: &Body) -> Vec<Violation> {
    if get(&body.regions, 0).is_none_or(|&solid| solid) {
        return vec![Violation::Outside];
    }
    if !resolves(body) {
        return vec![Violation::Dangling];
    }
    let mut out = BTreeSet::new();
    let wires: BTreeSet<usize> = live(&body.shells)
        .flat_map(|(_, s)| s.wires.iter().copied())
        .collect();
    for &e in &wires {
        if get(&body.edges, e).is_some_and(|e| !e.fins.is_empty()) {
            out.insert(Violation::WireWithFins { edge: e });
        }
    }
    for (fi, face) in live(&body.faces) {
        face_violations(body, fi, face, &mut out);
    }
    for (ei, edge) in live(&body.edges) {
        edge_violations(body, ei, edge, &wires, &mut out);
    }
    let mut used: BTreeSet<usize> = live(&body.edges).flat_map(|(_, e)| e.ends).collect();
    used.extend(live(&body.shells).flat_map(|(_, s)| s.lone.iter().copied()));
    for (v, &is) in body.vertices.iter().enumerate() {
        if is && !used.contains(&v) {
            out.insert(Violation::LooseVertex { vertex: v });
        }
    }
    shell_violations(body, &mut out);
    let has_shell: BTreeSet<usize> = live(&body.shells).map(|(_, s)| s.region).collect();
    for (r, _) in live(&body.regions) {
        let bare = !has_shell.contains(&r);
        if bare && (r != 0 || !has_shell.is_empty()) {
            out.insert(Violation::RegionWithoutShell { region: r });
        }
    }
    out.into_iter().collect()
}

fn face_violations(body: &Body, fi: usize, face: &Face, out: &mut BTreeSet<Violation>) {
    if face.loops.is_empty() {
        out.insert(Violation::FaceWithoutLoop { face: fi });
    }
    let mut count: BTreeMap<Use, usize> = BTreeMap::new();
    let mut loops_at: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (li, lp) in face.loops.iter().enumerate() {
        if lp.is_empty() {
            out.insert(Violation::EmptyLoop { face: fi });
        }
        for (k, &u) in lp.iter().enumerate() {
            *count.entry(u).or_default() += 1;
            let next = lp.get((k + 1) % lp.len()).copied().unwrap_or(u);
            if let (Some((s, e)), Some((s2, _))) = (ends(body, u), ends(body, next)) {
                if e != s2 {
                    out.insert(Violation::LoopNotConnected { face: fi });
                }
                loops_at.entry(s).or_default().insert(li);
            }
        }
    }
    if count.values().any(|&n| n > 1) {
        out.insert(Violation::RepeatedUse { face: fi });
    }
    if loops_at.values().any(|l| l.len() > 1) {
        out.insert(Violation::VertexOnTwoLoops { face: fi });
    }
}

fn edge_violations(
    body: &Body,
    ei: usize,
    edge: &Edge,
    wires: &BTreeSet<usize>,
    out: &mut BTreeSet<Violation>,
) {
    if edge.collapsed && edge.ends[0] != edge.ends[1] {
        out.insert(Violation::CollapsedEnds { edge: ei });
    }
    if edge.fins.is_empty() && !wires.contains(&ei) {
        out.insert(Violation::LooseEdge { edge: ei });
    }
    // Radial completeness: the fins are exactly the uses of this edge, each once.
    let mut uses: Vec<(usize, bool)> = live(&body.faces)
        .flat_map(|(fi, f)| {
            f.loops
                .iter()
                .flatten()
                .filter(|u| u.edge == ei)
                .map(move |u| (fi, u.forward))
        })
        .collect();
    let mut fins = edge.fins.clone();
    uses.sort_unstable();
    fins.sort_unstable();
    if uses != fins {
        out.insert(Violation::Radial { edge: ei });
    }
    if edge.collapsed {
        return;
    }
    let n = edge.fins.len();
    for k in 0..n {
        let (Some(&(fa, sa)), Some(&(fb, sb))) = (edge.fins.get(k), edge.fins.get((k + 1) % n))
        else {
            continue;
        };
        let side = |f: usize, s: usize| get(&body.faces, f).and_then(|f| f.sides.get(s).copied());
        if side(fa, after(sa)) != side(fb, 1 - after(sb)) {
            out.insert(Violation::SidesDisagree { edge: ei });
        }
    }
}

/// A piece of a shell: a face side, a wire edge or a lone vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Piece {
    Side(usize, usize),
    Wire(usize),
    Lone(usize),
}

fn shell_violations(body: &Body, out: &mut BTreeSet<Violation>) {
    // Pieces of each shell, and their vertices.
    let mut pieces: BTreeMap<usize, Vec<(Piece, BTreeSet<usize>)>> = BTreeMap::new();
    for (fi, f) in live(&body.faces) {
        let vs: BTreeSet<usize> = f
            .loops
            .iter()
            .flatten()
            .filter_map(|&u| ends(body, u).map(|(s, _)| s))
            .collect();
        for (side, &s) in f.sides.iter().enumerate() {
            pieces
                .entry(s)
                .or_default()
                .push((Piece::Side(fi, side), vs.clone()));
        }
    }
    for (si, s) in live(&body.shells) {
        for &e in &s.wires {
            let vs = get(&body.edges, e).map(|e| e.ends.into_iter().collect());
            pieces
                .entry(si)
                .or_default()
                .push((Piece::Wire(e), vs.unwrap_or_default()));
        }
        for &v in &s.lone {
            pieces
                .entry(si)
                .or_default()
                .push((Piece::Lone(v), BTreeSet::from([v])));
        }
    }
    // Face sides joined across edges, by the sector rule.
    let mut across: BTreeMap<Piece, Vec<Piece>> = BTreeMap::new();
    for (_, e) in live(&body.edges) {
        if e.collapsed {
            continue;
        }
        let n = e.fins.len();
        for k in 0..n {
            if let (Some(&(fa, sa)), Some(&(fb, sb))) = (e.fins.get(k), e.fins.get((k + 1) % n)) {
                let a = Piece::Side(fa, after(sa));
                let b = Piece::Side(fb, 1 - after(sb));
                across.entry(a).or_default().push(b);
                across.entry(b).or_default().push(a);
            }
        }
    }
    for (si, _) in live(&body.shells) {
        let Some(ps) = pieces.get(&si).filter(|p| !p.is_empty()) else {
            out.insert(Violation::EmptyShell { shell: si });
            continue;
        };
        // Breadth-first from the first piece, over shared vertices and across edges.
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::new();
        if let Some((p, _)) = ps.first() {
            seen.insert(*p);
            queue.push_back(*p);
        }
        while let Some(p) = queue.pop_front() {
            let vs = ps
                .iter()
                .find(|(q, _)| *q == p)
                .map_or_else(BTreeSet::new, |(_, v)| v.clone());
            let near = ps
                .iter()
                .filter(|(_, w)| !w.is_disjoint(&vs))
                .map(|(q, _)| *q)
                .chain(across.get(&p).into_iter().flatten().copied())
                .filter(|q| ps.iter().any(|(r, _)| r == q))
                .collect::<Vec<_>>();
            for q in near {
                if seen.insert(q) {
                    queue.push_back(q);
                }
            }
        }
        if ps.iter().any(|(p, _)| !seen.contains(p)) {
            out.insert(Violation::ShellNotConnected { shell: si });
        }
    }
}

/// The Euler-Poincaré characteristic `V - E + F - (L - F)` of the faces with `shell` on a
/// side, their edges and vertices; a collapsed edge is its vertex, not an edge. For a
/// closed orientable shell it is `2 - 2 genus`.
#[must_use]
pub fn euler(body: &Body, shell: usize) -> i64 {
    let faces: Vec<&Face> = live(&body.faces)
        .filter(|(_, f)| f.sides.contains(&shell))
        .map(|(_, f)| f)
        .collect();
    let edges: BTreeSet<usize> = faces
        .iter()
        .flat_map(|f| f.loops.iter().flatten())
        .map(|u| u.edge)
        .collect();
    let vertices: BTreeSet<usize> = edges
        .iter()
        .filter_map(|&e| get(&body.edges, e))
        .flat_map(|e| e.ends)
        .collect();
    let lines = edges
        .iter()
        .filter(|&&e| get(&body.edges, e).is_some_and(|e| !e.collapsed))
        .count();
    let rings: usize = faces.iter().map(|f| f.loops.len().saturating_sub(1)).sum();
    let count = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    count(vertices.len()) - count(lines) + count(faces.len()) - count(rings)
}

/// The genus of a closed shell: every non-collapsed edge of its faces has exactly two fins.
/// `None` for an open shell or an odd characteristic.
#[must_use]
pub fn genus(body: &Body, shell: usize) -> Option<i64> {
    let closed = live(&body.faces)
        .filter(|(_, f)| f.sides.contains(&shell))
        .flat_map(|(_, f)| f.loops.iter().flatten())
        .filter_map(|u| get(&body.edges, u.edge))
        .all(|e| e.collapsed || e.fins.len() == 2);
    let chi = euler(body, shell);
    (closed && chi % 2 == 0).then_some(1 - chi / 2)
}

/// The first `k` for which samples `k` and `k + 1` differ by at least half a period in a
/// closed direction: there the pcurve runs on a different copy of the surface's domain, a
/// jump across the seam (Edgeuse 7). `periods` are the `u` and `v` periods, `None` where
/// open. Dense samples make the half-period threshold the truth: a continuous curve
/// moves less than half a period between neighbours.
#[must_use]
pub fn jumps(uv: &[[f64; 2]], periods: [Option<f64>; 2]) -> Option<usize> {
    uv.windows(2).position(|w| match w {
        [a, b] => periods
            .iter()
            .zip(a.iter().zip(b))
            .any(|(p, (x, y))| p.is_some_and(|p| (y - x).abs() >= p / 2.0)),
        _ => false,
    })
}

/// The first `k` at which the samples run along a pole's line `v = pole_v`: two
/// consecutive samples within `eps_v` of it whose `u` differ by more than `eps_u` (the
/// dog-leg, Edgeuse 20). A pcurve only arrives at or leaves a pole; moving in `u` there
/// is a run along the line the surface collapses to a point.
#[must_use]
pub fn dog_leg(uv: &[[f64; 2]], pole_v: f64, eps_v: f64, eps_u: f64) -> Option<usize> {
    let at = |p: &[f64; 2]| (p[1] - pole_v).abs() <= eps_v;
    uv.windows(2).position(|w| match w {
        [a, b] => at(a) && at(b) && (b[0] - a[0]).abs() > eps_u,
        _ => false,
    })
}

/// The first interior sample within `eps_v` of the pole's line where neither end is: the
/// use passes through the pole inside itself, so the pole is not a vertex (Face 24).
#[must_use]
pub fn passes_pole(uv: &[[f64; 2]], pole_v: f64, eps_v: f64) -> Option<usize> {
    let at = |p: &[f64; 2]| (p[1] - pole_v).abs() <= eps_v;
    let (Some(first), Some(last)) = (uv.first(), uv.last()) else {
        return None;
    };
    if at(first) || at(last) {
        return None;
    }
    uv.iter().position(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(edge: usize, forward: bool) -> Use {
        Use { edge, forward }
    }

    /// A closed cylinder: two circles, a seam, three faces; shells 0 (outside) and 1.
    fn cylinder() -> Body {
        let wall = Face {
            sides: [0, 1],
            loops: vec![vec![u(0, true), u(2, true), u(1, false), u(2, false)]],
        };
        let bottom = Face {
            sides: [0, 1],
            loops: vec![vec![u(0, false)]],
        };
        let top = Face {
            sides: [0, 1],
            loops: vec![vec![u(1, true)]],
        };
        let edge = |ends, fins| {
            Some(Edge {
                ends,
                collapsed: false,
                fins,
            })
        };
        Body {
            regions: vec![Some(false), Some(true)],
            shells: vec![
                Some(Shell::default()),
                Some(Shell {
                    region: 1,
                    ..Shell::default()
                }),
            ],
            faces: vec![Some(wall), Some(bottom), Some(top)],
            edges: vec![
                edge([0, 0], vec![(0, true), (1, false)]),
                edge([1, 1], vec![(0, false), (2, true)]),
                edge([0, 1], vec![(0, true), (0, false)]),
            ],
            vertices: vec![true, true],
        }
    }

    /// A sphere: one face, a seam and two collapsed pole edges.
    fn sphere() -> Body {
        let face = Face {
            sides: [0, 1],
            loops: vec![vec![u(1, true), u(0, true), u(2, false), u(0, false)]],
        };
        let edge = |ends, collapsed, fins| {
            Some(Edge {
                ends,
                collapsed,
                fins,
            })
        };
        Body {
            regions: vec![Some(false), Some(true)],
            shells: vec![
                Some(Shell::default()),
                Some(Shell {
                    region: 1,
                    ..Shell::default()
                }),
            ],
            faces: vec![Some(face)],
            edges: vec![
                edge([0, 1], false, vec![(0, true), (0, false)]),
                edge([0, 0], true, vec![(0, true)]),
                edge([1, 1], true, vec![(0, false)]),
            ],
            vertices: vec![true, true],
        }
    }

    #[test]
    fn valid_bodies_have_no_violation_and_genus_zero() {
        for body in [cylinder(), sphere()] {
            assert_eq!(violations(&body), vec![]);
            assert_eq!(euler(&body, 1), 2);
            assert_eq!(genus(&body, 1), Some(0));
            assert_eq!(genus(&body, 0), Some(0));
        }
    }

    #[test]
    fn each_broken_invariant_is_named() {
        let mut b = cylinder();
        if let Some(Some(f)) = b.faces.get_mut(1) {
            f.sides = [1, 0];
        }
        assert!(violations(&b).contains(&Violation::SidesDisagree { edge: 0 }));

        let mut b = cylinder();
        if let Some(Some(e)) = b.edges.get_mut(2) {
            e.fins.pop();
        }
        assert!(violations(&b).contains(&Violation::Radial { edge: 2 }));

        let mut b = cylinder();
        if let Some(Some(f)) = b.faces.get_mut(0)
            && let Some(lp) = f.loops.first_mut()
        {
            lp.swap(0, 1);
        }
        assert!(violations(&b).contains(&Violation::LoopNotConnected { face: 0 }));

        let mut b = cylinder();
        b.vertices.push(true);
        b.shells.push(Some(Shell::default()));
        b.regions.push(Some(true));
        assert_eq!(
            violations(&b),
            vec![
                Violation::LooseVertex { vertex: 2 },
                Violation::EmptyShell { shell: 2 },
                Violation::RegionWithoutShell { region: 2 },
            ]
        );

        let mut b = cylinder();
        if let Some(Some(e)) = b.edges.get_mut(1) {
            e.ends = [3, 3];
        }
        assert_eq!(violations(&b), vec![Violation::Dangling]);
    }

    #[test]
    fn two_bodies_in_one_shell_pair_split_both_shells() {
        let one = cylinder();
        let mut two = one.clone();
        let shift = |n: usize| move |i: usize| i + n;
        for f in one.faces.iter().flatten() {
            let mut f = f.clone();
            for u in f.loops.iter_mut().flatten() {
                u.edge = shift(3)(u.edge);
            }
            two.faces.push(Some(f));
        }
        for e in one.edges.iter().flatten() {
            let mut e = e.clone();
            e.ends = e.ends.map(shift(2));
            e.fins = e.fins.iter().map(|&(f, s)| (f + 3, s)).collect();
            two.edges.push(Some(e));
        }
        two.vertices.extend([true, true]);
        assert_eq!(
            violations(&two),
            vec![
                Violation::ShellNotConnected { shell: 0 },
                Violation::ShellNotConnected { shell: 1 },
            ]
        );
        assert_eq!(euler(&two, 1), 4);
    }

    #[test]
    fn pole_and_seam_predicates_judge_dense_samples() {
        let line = |a: [f64; 2], b: [f64; 2], n: usize| -> Vec<[f64; 2]> {
            (0..=n)
                .map(|k| {
                    let s = k as f64 / n as f64;
                    [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s]
                })
                .collect()
        };
        // Up a meridian to the north pole: arrives, no leg, no pass.
        let up = line([0.25, 0.5], [0.25, 1.0], 100);
        assert_eq!(dog_leg(&up, 1.0, 1e-9, 1e-9), None);
        assert_eq!(passes_pole(&up, 1.0, 1e-9), None);
        // Then along the pole's line: a dog-leg where the leg starts.
        let mut leg = up.clone();
        leg.extend(line([0.25, 1.0], [0.75, 1.0], 10).into_iter().skip(1));
        assert_eq!(dog_leg(&leg, 1.0, 1e-9, 1e-9), Some(100));
        // Through the pole and down the far side, u flipped: a pass inside the use.
        let mut through = up;
        through.extend(line([0.75, 1.0], [0.75, 0.5], 100));
        assert_eq!(passes_pole(&through, 1.0, 1e-9), Some(100));
        // A seam crossing written as a jump from u near 1 to near 0.
        let jump = [[0.9, 0.5], [0.95, 0.5], [0.0, 0.5], [0.05, 0.5]];
        assert_eq!(jumps(&jump, [Some(1.0), None]), Some(1));
        assert_eq!(jumps(&jump, [None, None]), None);
    }
}
