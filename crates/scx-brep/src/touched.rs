// Written for solidcorex. Apache-2.0.
//! What a draft made from a finished body touched, so that `finish` checks those records
//! and the records whose rules read them, and nothing else (01 §2.3, research 63 §6.14).
//!
//! The base body passed every check. Each check is a rule applied to one record (an
//! *instance*) that reads that record and a few others; an instance none of whose inputs
//! changed passes again. So the draft marks, at the first write to a record the base had,
//! the instances that read it, found through what the record points at *then*: a face's
//! uses name the edges whose fins name it, an edge's fins name the faces that use it. A
//! valid base keeps those pointers two-way (fins and uses match one to one), so the marks
//! cover every reader. Records the draft created are past the base's slot counts and are
//! always checked.
//!
//! Three things have no pointer back and are found at `finish` instead: the edges at a
//! vertex (from the base body's stars when they were built, else one pass over the edges'
//! ends), whether a vertex the edit left without a known edge is loose, and whether the
//! shells are still one piece each ([`Touched::shells_unchanged`]).
//!
//! The whole body is checked when there is no base (a [`Draft::new`](crate::Draft::new)),
//! when the draft is finished under another resolution than the base's, and when a shell
//! the base had is removed (faces name their shells, and nothing names a shell's faces).

use std::sync::Arc;

use scx_num::units::Resolution;

use crate::body::{BodyCells, Seed};
use crate::check::Sets;
use crate::id::{EdgeId, FaceId, RegionId, ShellId, VertexId};
use crate::record::{FaceRecord, FaceSide, ShellRecord};
use crate::table::Id;
use crate::tables::Tables;

/// One bit per slot the base had.
#[derive(Debug, Clone, Default)]
struct Bits(Vec<u64>);

impl Bits {
    /// Sets `slot`'s bit; whether it was clear. Slots past `start` are never set.
    fn first(&mut self, slot: usize, start: usize) -> bool {
        if slot >= start {
            return false;
        }
        if self.0.is_empty() {
            self.0 = vec![0; start.div_ceil(64)];
        }
        let Some(word) = self.0.get_mut(slot / 64) else {
            return false;
        };
        let bit = 1u64 << (slot % 64);
        let clear = *word & bit == 0;
        *word |= bit;
        clear
    }
}

/// The slot counts of the base.
#[derive(Debug, Clone, Copy)]
struct Start {
    regions: usize,
    shells: usize,
    faces: usize,
    edges: usize,
    vertices: usize,
}

/// What the draft has touched of its base.
#[derive(Debug, Clone)]
struct Base {
    resolution: Resolution,
    /// The base's body cells: its vertex stars when built, and its boxes for the new body.
    cells: Arc<BodyCells>,
    start: Start,
    face_seen: Bits,
    geom_seen: Bits,
    adj_seen: Bits,
    shell_seen: Bits,
    vertex_seen: Bits,
    /// Base faces written or removed, with their records as the base had them.
    faces_was: Vec<(FaceId, Arc<FaceRecord>)>,
    /// Base edges whose geometry was written or that were removed, with their base ends.
    ends_was: Vec<(EdgeId, [VertexId; 2])>,
    /// Base edges whose fins were written or that were removed.
    adj_was: Vec<EdgeId>,
    /// Base shells written, as the base had them.
    shells_was: Vec<(ShellId, ShellRecord)>,
    /// Base vertices written.
    vertices: Vec<VertexId>,
    /// Base vertices removed.
    removed_vertices: Vec<VertexId>,
    /// Base faces and edges a touched record's pointers named: their structure instances
    /// read it.
    faces: Vec<FaceId>,
    edges: Vec<EdgeId>,
    /// A region, edge or vertex the base had is gone, or a wire's fins were written: every
    /// shell's instances run, as nothing points from those to the shells naming them.
    all_shells: bool,
    /// A wire the base had was written: its shell's pieces may have moved, and nothing
    /// names that shell from the edge.
    wire_touched: bool,
    /// A base shell is gone: the whole body is checked.
    whole: bool,
}

/// What a draft touched; nothing when it checks the whole body.
#[derive(Debug, Clone)]
pub(crate) struct Touched(Option<Box<Base>>);

/// The structure instances to run, each list sorted; ids no longer live are skipped.
#[derive(Debug)]
pub(crate) struct Plan {
    /// `None` for every shell.
    pub(crate) shells: Option<Vec<ShellId>>,
    pub(crate) faces: Vec<FaceId>,
    pub(crate) edges: Vec<EdgeId>,
}

/// `ids` sorted without repeats.
fn sorted<I: Ord>(mut ids: Vec<I>) -> Vec<I> {
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The ids of the slots `from..to`.
fn range<I: Id>(from: usize, to: usize) -> impl Iterator<Item = I> {
    (from..to).filter_map(|s| u32::try_from(s).ok().map(I::from_slot))
}

impl Touched {
    /// Checks the whole body.
    pub(crate) const fn whole() -> Self {
        Self(None)
    }

    /// Tracks a draft of the finished body `t`, under `resolution`, with `cells`.
    pub(crate) fn of(t: &Tables, resolution: Resolution, cells: Arc<BodyCells>) -> Self {
        Self(Some(Box::new(Base {
            resolution,
            cells,
            start: Start {
                regions: t.regions.slots(),
                shells: t.shells.slots(),
                faces: t.faces.slots(),
                edges: t.edges.slots(),
                vertices: t.vertices.slots(),
            },
            face_seen: Bits::default(),
            geom_seen: Bits::default(),
            adj_seen: Bits::default(),
            shell_seen: Bits::default(),
            vertex_seen: Bits::default(),
            faces_was: Vec::new(),
            ends_was: Vec::new(),
            adj_was: Vec::new(),
            shells_was: Vec::new(),
            vertices: Vec::new(),
            removed_vertices: Vec::new(),
            faces: Vec::new(),
            edges: Vec::new(),
            all_shells: false,
            wire_touched: false,
            whole: false,
        })))
    }

    /// Whether finishing under `resolution` may check the touched records alone.
    pub(crate) fn applies(&self, resolution: Resolution) -> bool {
        self.0
            .as_ref()
            .is_some_and(|b| !b.whole && b.resolution == resolution)
    }

    fn base(&mut self) -> Option<&mut Base> {
        self.0.as_deref_mut()
    }

    // The marks, each made before the write it names.

    /// Before a write to face `id`'s record, or its removal.
    pub(crate) fn face(&mut self, t: &Tables, id: FaceId) {
        let Some(b) = self.base() else { return };
        if !b.face_seen.first(id.slot(), b.start.faces) {
            return;
        }
        if let Some(rec) = t.faces.get(id) {
            b.edges
                .extend(rec.loops.iter().flat_map(|l| &l.uses).map(|u| u.edge));
            b.faces_was.push((id, Arc::clone(rec)));
        }
    }

    /// Before a write to edge `id`'s fins, or their removal.
    pub(crate) fn fins(&mut self, t: &Tables, id: EdgeId) {
        let Some(b) = self.base() else { return };
        if !b.adj_seen.first(id.slot(), b.start.edges) {
            return;
        }
        if let Some(adj) = t.adjacency.get(id) {
            // A base edge without fins is a wire, and nothing names its shell.
            b.all_shells |= adj.fins.is_empty();
            b.wire_touched |= adj.fins.is_empty();
            b.faces.extend(adj.fins.iter().map(|f| f.face));
        }
        b.adj_was.push(id);
    }

    /// Before a write to edge `id`'s geometry part, or its removal.
    pub(crate) fn geom(&mut self, t: &Tables, id: EdgeId) {
        let Some(b) = self.base() else { return };
        if !b.geom_seen.first(id.slot(), b.start.edges) {
            return;
        }
        if let Some(g) = t.edges.get(id) {
            b.ends_was.push((id, g.ends));
        }
        // The fins as they are now: the base's, or marked when first written.
        if let Some(adj) = t.adjacency.get(id) {
            b.wire_touched |= adj.fins.is_empty();
            b.faces.extend(adj.fins.iter().map(|f| f.face));
        }
    }

    /// Before edge `id` is removed.
    pub(crate) fn edge_removed(&mut self, t: &Tables, id: EdgeId) {
        self.geom(t, id);
        self.fins(t, id);
        if let Some(b) = self.base()
            && id.slot() < b.start.edges
        {
            b.all_shells = true;
        }
    }

    /// Before a write to vertex `id`.
    pub(crate) fn vertex(&mut self, id: VertexId) {
        let Some(b) = self.base() else { return };
        if b.vertex_seen.first(id.slot(), b.start.vertices) {
            b.vertices.push(id);
        }
    }

    /// Vertex `id` was removed.
    pub(crate) fn vertex_removed(&mut self, id: VertexId) {
        let Some(b) = self.base() else { return };
        if id.slot() < b.start.vertices {
            b.removed_vertices.push(id);
            b.all_shells = true;
        }
    }

    /// Region `id` was removed.
    pub(crate) fn region_removed(&mut self, id: RegionId) {
        if let Some(b) = self.base() {
            // A new region is named only by shells the draft wrote or made.
            b.all_shells |= id.slot() < b.start.regions;
        }
    }

    /// Before a write to shell `id`.
    pub(crate) fn shell(&mut self, t: &Tables, id: ShellId) {
        let Some(b) = self.base() else { return };
        if !b.shell_seen.first(id.slot(), b.start.shells) {
            return;
        }
        if let Some(rec) = t.shells.get(id) {
            b.edges.extend(rec.wires.iter().copied());
            b.shells_was.push((id, rec.clone()));
        }
    }

    /// Shell `id` was removed.
    pub(crate) fn shell_removed(&mut self, id: ShellId) {
        if let Some(b) = self.base()
            && id.slot() < b.start.shells
        {
            b.whole = true;
        }
    }

    // What `finish` runs.

    /// The structure instances to run.
    pub(crate) fn plan(&self, t: &Tables) -> Plan {
        let Some(b) = self.0.as_deref() else {
            return Plan {
                shells: None,
                faces: t.faces.iter().map(|(id, _)| id).collect(),
                edges: t.edges.iter().map(|(id, _)| id).collect(),
            };
        };
        let shells = (!b.all_shells).then(|| {
            let mut s: Vec<ShellId> = b.shells_was.iter().map(|&(id, _)| id).collect();
            s.extend(range::<ShellId>(b.start.shells, t.shells.slots()));
            sorted(s)
        });
        let mut faces = b.faces.clone();
        faces.extend(b.faces_was.iter().map(|&(id, _)| id));
        faces.extend(range::<FaceId>(b.start.faces, t.faces.slots()));
        let mut edges = b.edges.clone();
        edges.extend(b.ends_was.iter().map(|&(id, _)| id));
        edges.extend(b.adj_was.iter().copied());
        edges.extend(range::<EdgeId>(b.start.edges, t.edges.slots()));
        // A loose-edge instance also reads which shells name the edge as a wire. A shell
        // the draft wrote or made names, beyond the base wires its mark added, only edges
        // the draft made, or edges whose fins it wrote (any other has fins, and that
        // shell's wire pass refuses first), and those are here already.
        edges.extend(self.star(t, &sorted(b.removed_vertices.clone())));
        Plan {
            shells,
            faces: sorted(faces),
            edges: sorted(edges),
        }
    }

    /// The live edges at any of `verts` (sorted), and perhaps others: from the base's
    /// stars when they were built, else one pass over every edge's ends.
    fn star(&self, t: &Tables, verts: &[VertexId]) -> Vec<EdgeId> {
        let Some(b) = self.0.as_deref() else {
            return Vec::new();
        };
        if verts.is_empty() {
            return Vec::new();
        }
        let at = |ends: [VertexId; 2]| ends.iter().any(|v| verts.binary_search(v).is_ok());
        let mut out = Vec::new();
        if let Some(adj) = b.cells.built_adjacency() {
            // The base's edges there, and every edge whose ends the draft wrote or made.
            for v in verts {
                out.extend(adj.stars.row(v.slot()).iter().copied());
            }
            let written = b.ends_was.iter().map(|&(id, _)| id);
            for e in written.chain(range(b.start.edges, t.edges.slots())) {
                if t.edges.get(e).is_some_and(|g| at(g.ends)) {
                    out.push(e);
                }
            }
        } else {
            out.extend(t.edges.iter().filter(|(_, g)| at(g.ends)).map(|(id, _)| id));
        }
        sorted(out)
    }

    /// The lowest loose vertex, given that every instance before it passed. Only a vertex
    /// the draft made, or one that was an end of an edge or a lone vertex of a shell the
    /// draft wrote, can be loose; each is cleared by a live edge ending there or a shell
    /// naming it, looked for first among the records the plan checked.
    pub(crate) fn first_loose(&self, t: &Tables, plan: &Plan) -> Option<VertexId> {
        let b = self.0.as_deref()?;
        let mut cand: Vec<VertexId> = b.ends_was.iter().flat_map(|&(_, ends)| ends).collect();
        for (_, was) in &b.shells_was {
            cand.extend(was.lone_vertices.iter().copied());
        }
        cand.extend(range::<VertexId>(b.start.vertices, t.vertices.slots()));
        cand.retain(|&v| t.vertices.get(v).is_some());
        let cand = sorted(cand);
        if cand.is_empty() {
            return None;
        }
        // Vertices a checked edge ends at, or a checked face's edge, or a shell names.
        let mut covered: Vec<VertexId> = plan
            .edges
            .iter()
            .filter_map(|&e| t.edges.get(e))
            .flat_map(|g| g.ends)
            .collect();
        for &f in &plan.faces {
            if let Some(rec) = t.faces.get(f) {
                let edges = rec.loops.iter().flat_map(|l| &l.uses);
                covered.extend(
                    edges
                        .filter_map(|u| t.edges.get(u.edge))
                        .flat_map(|g| g.ends),
                );
            }
        }
        for (_, s) in t.shells.iter() {
            covered.extend(s.lone_vertices.iter().copied());
        }
        let covered = sorted(covered);
        let open: Vec<VertexId> = cand
            .into_iter()
            .filter(|v| covered.binary_search(v).is_err())
            .collect();
        if open.is_empty() {
            return None;
        }
        let ended: Vec<VertexId> = sorted(
            self.star(t, &open)
                .into_iter()
                .filter_map(|e| t.edges.get(e))
                .flat_map(|g| g.ends)
                .collect(),
        );
        open.into_iter().find(|v| ended.binary_search(v).is_err())
    }

    /// Whether every shell is sure to be one piece and non-empty, as in the base, given
    /// that every instance before the shells pass held; `false` sends `finish` to the
    /// whole-body pass, which then finds the failure if there is one.
    ///
    /// A shell's pieces (face sides, wires, lone vertices) are joined at *hubs*: the shell
    /// and a vertex a piece is at (a face side is at its uses' start vertices; the radial
    /// joins follow from these once the sides agree and the loops close). Call the pieces
    /// whose hubs may have changed the changed pieces, as the base had them and as they are
    /// now. Every piece of the base shell that was not changed lies in a part that, in the
    /// base, met a changed piece at a hub that also holds an unchanged piece. So if every
    /// hub of the changed pieces as they were is a hub of a changed piece as it is now, and
    /// the changed pieces as they are now are joined among themselves, every part meets
    /// them and the shell is one piece. Where that does not show, the answer is `false`,
    /// never a refusal.
    pub(crate) fn shells_unchanged(&self, t: &Tables) -> bool {
        let Some(b) = self.0.as_deref() else {
            return false;
        };
        if b.wire_touched {
            return false;
        }
        let new_faces = range::<FaceId>(b.start.faces, t.faces.slots());
        let new_shells = range::<ShellId>(b.start.shells, t.shells.slots());
        let mut ends_was = b.ends_was.clone();
        ends_was.sort_unstable_by_key(|&(e, _)| e);
        let was_ends = |e: EdgeId| match ends_was.binary_search_by_key(&e, |&(id, _)| id) {
            Ok(i) => ends_was.get(i).map(|&(_, ends)| ends),
            Err(_) => t.edges.get(e).map(|g| g.ends),
        };
        // The faces whose pieces may have changed.
        let mut faces: Vec<FaceId> = Vec::new();
        for (id, was) in &b.faces_was {
            match t.faces.get(*id) {
                Some(now) if Arc::ptr_eq(now, was) || same_pieces(now, was) => {}
                _ => faces.push(*id),
            }
        }
        faces.extend(new_faces.filter(|&f| t.faces.get(f).is_some()));
        for &(e, ends) in &b.ends_was {
            match t.edges.get(e) {
                Some(g) if g.ends == ends => {}
                _ => {
                    if let Some(adj) = t.adjacency.get(e) {
                        faces.extend(adj.fins.iter().map(|f| f.face));
                    }
                }
            }
        }
        let mut shells: Vec<ShellId> = Vec::new();
        for (id, was) in &b.shells_was {
            match t.shells.get(*id) {
                Some(now) if now.wires == was.wires && now.lone_vertices == was.lone_vertices => {}
                _ => shells.push(*id),
            }
        }
        shells.extend(new_shells.filter(|&s| t.shells.get(s).is_some()));
        if faces.is_empty() && shells.is_empty() {
            return true;
        }
        let faces = sorted(faces);
        let shells = sorted(shells);
        let mut faces_was: Vec<(FaceId, &FaceRecord)> = b
            .faces_was
            .iter()
            .map(|(id, r)| (*id, r.as_ref()))
            .collect();
        faces_was.sort_unstable_by_key(|&(id, _)| id);
        let mut shells_was: Vec<(ShellId, &ShellRecord)> =
            b.shells_was.iter().map(|(id, r)| (*id, r)).collect();
        shells_was.sort_unstable_by_key(|&(id, _)| id);
        // Hubs of the changed pieces now, by piece; hubs as they were.
        let mut now: Vec<(ShellId, VertexId, usize)> = Vec::new();
        let mut was: Vec<(ShellId, VertexId)> = Vec::new();
        let mut piece = 0;
        let mut owner: Vec<ShellId> = Vec::new();
        let starts = |rec: &FaceRecord, ends: &dyn Fn(EdgeId) -> Option<[VertexId; 2]>| {
            let mut vs: Vec<VertexId> = rec
                .loops
                .iter()
                .flat_map(|l| &l.uses)
                .filter_map(|u| {
                    let [a, z] = ends(u.edge)?;
                    Some(if u.sense == scx_num::frame::Sense::Forward {
                        a
                    } else {
                        z
                    })
                })
                .collect();
            vs.sort_unstable();
            vs.dedup();
            vs
        };
        let ends_now = |e: EdgeId| t.edges.get(e).map(|g| g.ends);
        for &f in &faces {
            if let Some(rec) = t.faces.get(f) {
                let vs = starts(rec, &ends_now);
                for side in FaceSide::BOTH {
                    let s = rec.side(side);
                    now.extend(vs.iter().map(|&v| (s, v, piece)));
                    owner.push(s);
                    piece += 1;
                }
            }
            if f.slot() < b.start.faces {
                let base = match faces_was.binary_search_by_key(&f, |&(id, _)| id) {
                    Ok(i) => faces_was.get(i).map(|&(_, r)| r),
                    Err(_) => t.faces.get(f).map(AsRef::as_ref),
                };
                if let Some(rec) = base {
                    let vs = starts(rec, &was_ends);
                    for s in rec.sides {
                        was.extend(vs.iter().map(|&v| (s, v)));
                    }
                }
            }
        }
        for &s in &shells {
            if let Some(rec) = t.shells.get(s) {
                for &e in &rec.wires {
                    if let Some(g) = t.edges.get(e) {
                        now.extend(g.ends.iter().map(|&v| (s, v, piece)));
                    }
                    owner.push(s);
                    piece += 1;
                }
                for &v in &rec.lone_vertices {
                    now.push((s, v, piece));
                    owner.push(s);
                    piece += 1;
                }
            }
            let i = shells_was.binary_search_by_key(&s, |&(id, _)| id);
            if let Some(&(_, rec)) = i.ok().and_then(|i| shells_was.get(i)) {
                for &e in &rec.wires {
                    if let Some(ends) = was_ends(e) {
                        was.extend(ends.iter().map(|&v| (s, v)));
                    }
                }
                was.extend(rec.lone_vertices.iter().map(|&v| (s, v)));
            }
        }
        let mut sets = Sets::new(piece);
        now.sort_unstable();
        for w in now.windows(2) {
            if let [(s0, v0, a), (s1, v1, z)] = *w
                && (s0, v0) == (s1, v1)
            {
                sets.join(a, z);
            }
        }
        let was = sorted(was);
        // Each shell with a changed piece: its pieces now in one part, holding every hub
        // its pieces had.
        let mut pieces: Vec<(ShellId, usize)> =
            owner.iter().enumerate().map(|(p, &s)| (s, p)).collect();
        pieces.sort_unstable();
        let mut touched: Vec<ShellId> = owner;
        touched.extend(was.iter().map(|&(s, _)| s));
        touched.extend(shells.iter().copied());
        for s in sorted(touched) {
            let mut root = None;
            let lo = pieces.partition_point(|&(x, _)| x < s);
            let mine = pieces.get(lo..).unwrap_or(&[]);
            for &(_, p) in mine.iter().take_while(|&&(x, _)| x == s) {
                let r = sets.root(p);
                if *root.get_or_insert(r) != r {
                    return false;
                }
            }
            let Some(root) = root else {
                return false;
            };
            let lo = was.partition_point(|&(x, _)| x < s);
            let hubs = was.get(lo..).unwrap_or(&[]);
            let hubs = hubs.iter().take_while(|&&(x, _)| x == s);
            let mut any = false;
            for &(_, v) in hubs {
                any = true;
                let i = now.partition_point(|&(x, y, _)| (x, y) < (s, v));
                match now.get(i) {
                    Some(&(x, y, p)) if (x, y) == (s, v) && sets.root(p) == root => {}
                    _ => return false,
                }
            }
            // A base shell that only gained pieces: whether they meet the rest does not
            // show here.
            if !any && s.slot() < b.start.shells {
                return false;
            }
        }
        true
    }

    /// The geometry instances to run: the edges whose geometry or end vertices the draft
    /// wrote or made, then the faces it wrote or made and the faces using those edges.
    /// Run after the structure passes, so the fins and uses agree.
    pub(crate) fn geometry(&self, t: &Tables, plan: &Plan) -> (Vec<EdgeId>, Vec<FaceId>) {
        let Some(b) = self.0.as_deref() else {
            return (plan.edges.clone(), plan.faces.clone());
        };
        let mut edges: Vec<EdgeId> = b.ends_was.iter().map(|&(id, _)| id).collect();
        edges.extend(range::<EdgeId>(b.start.edges, t.edges.slots()));
        edges.extend(self.star(t, &sorted(b.vertices.clone())));
        edges.retain(|&e| t.edges.get(e).is_some());
        let edges = sorted(edges);
        let mut faces: Vec<FaceId> = b.faces_was.iter().map(|&(id, _)| id).collect();
        faces.extend(range::<FaceId>(b.start.faces, t.faces.slots()));
        for &e in &edges {
            if let Some(adj) = t.adjacency.get(e) {
                faces.extend(adj.fins.iter().map(|f| f.face));
            }
        }
        faces.retain(|&f| t.faces.get(f).is_some());
        (edges, sorted(faces))
    }

    /// What the new body's boxes can take from the base's, when the base had built them:
    /// `dirty` are the faces whose geometry check ran (sorted), which are every face whose
    /// box reads a record the draft wrote.
    pub(crate) fn seed(&self, dirty: Vec<FaceId>) -> Option<Seed> {
        let b = self.0.as_deref()?;
        Some(Seed {
            boxes: b.cells.built_boxes()?,
            dirty,
        })
    }
}

/// Whether two records of one face have the same pieces: the same sides, and the same
/// uses by edge and sense.
fn same_pieces(a: &FaceRecord, b: &FaceRecord) -> bool {
    let key = |r: &FaceRecord| {
        r.loops
            .iter()
            .flat_map(|l| &l.uses)
            .map(|u| (u.edge, u.sense))
            .collect::<Vec<_>>()
    };
    a.sides == b.sides && key(a) == key(b)
}
