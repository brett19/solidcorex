// Written for solidcorex. Apache-2.0.
//! [`tessellate`]: a body's faces triangulated on its [`wireframe`](crate::wireframe)
//! (research 72 TS2, TS3).
//!
//! A planar face is a constrained Delaunay triangulation ([`crate::cdt`]) of its loops'
//! edge polylines in the plane's `uv`, with no point added. A cylinder, cone, sphere or
//! torus face is triangulated in its own `uv` with interior points, refined until every
//! triangle is within the chord ([`crate::curved`]). Either way every triangle side on the
//! boundary is a polyline segment, so two faces meeting at an edge use the same indices
//! and a closed body gives a closed mesh by construction. NURBS faces are unsupported.

use scx_brep::{Brep, Face, FaceId, FaceSide};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Sense;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Vec3;
use scx_surface::{Plane, Surface, SurfaceKind};

use crate::cdt::{Fault, triangulate};
use crate::curved::{Lines, Points, Rules, curved_face};
use crate::error::{TessDefect, TessError, TessRefusal, TessUnsupported};
use crate::mesh::Mesh;
use crate::spec::TessSpec;
use crate::wire::wire;

/// The mesh of every face of `body` under `spec`: [`wireframe`](crate::wireframe)'s points
/// and polylines, then each face's triangles in face id order.
///
/// # Errors
///
/// As [`tessellate_faces`] over every face.
pub fn tessellate(
    body: &Brep,
    spec: &TessSpec,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Mesh, TessError> {
    let faces: Vec<FaceId> = body.faces().map(Face::id).collect();
    tessellate_faces(body, spec, &faces, budget, stop)
}

/// The mesh of the listed faces of `body`: the whole [`wireframe`](crate::wireframe), then
/// the triangles of each listed face in face id order, whatever the list's order or repeats.
///
/// Each triangle is counter-clockwise seen from outside the solid (from the face's front
/// when neither or both sides are solid). A plane's normals are the plane's on that side; a
/// curved face's are the surface's at each corner (at the triangle's middle for a corner on
/// a pole or apex). One unit of `budget` per point inserted and per flip, on top of the
/// wireframe's; a curved face pays again for each refinement round's triangulation.
///
/// # Errors
///
/// [`TessRefusal::NoFace`] for an id that is not a live face; [`TessUnsupported::FaceKind`]
/// for a listed NURBS face; [`TessRefusal::FaceTooFine`] when a curved face would need more
/// points than a mesh indexes; [`TessDefect::Crossing`] when two of a face's polyline
/// segments cross (two edges closer than their chords allow); [`TessDefect::Duplicate`]
/// when two of a face's points coincide in `uv`; [`TessDefect::Refinement`] when a curved
/// face still has a triangle outside its bound after the most rounds; otherwise as
/// [`wireframe`](crate::wireframe).
pub fn tessellate_faces(
    body: &Brep,
    spec: &TessSpec,
    faces: &[FaceId],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Mesh, TessError> {
    let mut ids = faces.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let mut jobs = Vec::with_capacity(ids.len());
    for &id in &ids {
        let face = body
            .face(id)
            .ok_or(TessError::Refused(TessRefusal::NoFace { face: id }))?;
        let surface = face.surface();
        if let Surface::Plane(plane) = surface {
            jobs.push((face, Work::Plane(plane)));
        } else if let Some(rules) = Rules::of(surface, spec) {
            jobs.push((face, Work::Curved(rules)));
        } else {
            return Err(TessError::Unsupported(TessUnsupported::FaceKind {
                face: id,
                kind: SurfaceKind::of(surface),
            }));
        }
    }
    let (mut mesh, params) = wire(body, spec, budget, stop)?;
    let mut line_of = vec![usize::MAX; body.edge_slots()];
    for (i, line) in mesh.edges.iter().enumerate() {
        if let Some(slot) = line_of.get_mut(line.edge.index()) {
            *slot = i;
        }
    }
    let mut scratch = FaceScratch::default();
    let mut points = Points::default();
    for (face, work) in jobs {
        stop.check()?;
        match work {
            Work::Plane(plane) => {
                plane_face(&mut mesh, &line_of, face, plane, &mut scratch, budget)?;
            }
            Work::Curved(rules) => {
                let lines = Lines {
                    line_of: &line_of,
                    params: &params,
                };
                curved_face(&mut mesh, &lines, face, &rules, &mut points, budget, stop)?;
            }
        }
    }
    Ok(mesh)
}

/// How one face is triangulated.
enum Work<'b> {
    Plane(&'b Plane),
    Curved(Rules),
}

/// Buffers reused from face to face.
#[derive(Default)]
struct FaceScratch {
    /// Each loop's ring of mesh indices, one after another.
    ring: Vec<u32>,
    /// Where each loop's ring starts in `ring`.
    starts: Vec<usize>,
    /// The face's distinct mesh indices, sorted: a local index is a place in it.
    locals: Vec<u32>,
    uv: Vec<Point2>,
    segments: Vec<[u32; 2]>,
}

fn plane_face(
    mesh: &mut Mesh,
    line_of: &[usize],
    face: Face<'_>,
    plane: &Plane,
    s: &mut FaceScratch,
    budget: &mut Budget,
) -> Result<(), TessError> {
    let id = face.id();
    let defect = |d: TessDefect| TessError::Defect(d);
    s.ring.clear();
    s.starts.clear();
    let mut chord = Length::ZERO;
    for lp in face.loops() {
        s.starts.push(s.ring.len());
        for u in lp.uses() {
            let edge = u.edge().id();
            let line = line_of
                .get(edge.index())
                .and_then(|&i| mesh.edges.get(i))
                .ok_or_else(|| defect(TessDefect::Unindexed { edge }))?;
            chord = chord.max(line.chord);
            // Each use's points but its last, which is the next use's first.
            let pts = line.points.get(..line.points.len().saturating_sub(1));
            match u.sense() {
                Sense::Forward => s.ring.extend_from_slice(pts.unwrap_or_default()),
                Sense::Reversed => s.ring.extend(line.points.iter().skip(1).rev().copied()),
            }
        }
    }
    s.starts.push(s.ring.len());

    s.locals.clear();
    s.locals.extend_from_slice(&s.ring);
    s.locals.sort_unstable();
    s.locals.dedup();
    let frame = plane.frame();
    let (o, x, y) = (frame.origin(), frame.x().as_vec(), frame.y().as_vec());
    s.uv.clear();
    for &i in &s.locals {
        let d = *mesh
            .positions
            .get(i as usize)
            .ok_or_else(|| defect(TessDefect::Triangulation { face: id }))?
            - o;
        s.uv.push(Point2::new(d.dot(x), d.dot(y)));
    }
    let local = |i: u32| s.locals.binary_search(&i).map(|l| l as u32);
    s.segments.clear();
    for w in s.starts.windows(2) {
        let ring = match *w {
            [from, to] => s.ring.get(from..to).unwrap_or_default(),
            _ => &[],
        };
        // Each point to the next, the last back to the first.
        let next = ring.iter().cycle().skip(1);
        for (&a, &b) in ring.iter().zip(next) {
            if let (Ok(a), Ok(b)) = (local(a), local(b)) {
                s.segments.push([a, b]);
            }
        }
    }

    let tris = triangulate(&s.uv, &s.segments, budget).map_err(|f| match f {
        Fault::Duplicate => defect(TessDefect::Duplicate { face: id }),
        Fault::Crossing => defect(TessDefect::Crossing { face: id }),
        Fault::Lost | Fault::Parity => defect(TessDefect::Triangulation { face: id }),
        Fault::Exhausted(e) => e.into(),
    })?;

    let solid = |side| face.shell(side).region().is_solid();
    let flip = solid(FaceSide::Front) && !solid(FaceSide::Back);
    let n: Vec3 = plane.normal(Point2::new(0.0, 0.0)).as_vec();
    let n = if flip { -n } else { n };
    mesh.triangles.reserve(tris.len());
    for tri in tris {
        let mut corners = [0; 3];
        for (m, l) in corners.iter_mut().zip(tri) {
            *m = *s
                .locals
                .get(l as usize)
                .ok_or_else(|| defect(TessDefect::Triangulation { face: id }))?;
        }
        let [a, b, c] = corners;
        mesh.triangles
            .push(if flip { [a, c, b] } else { [a, b, c] });
        mesh.normals.push([n; 3]);
        mesh.face_of.push(id);
    }
    mesh.chord_bound.push((id, chord));
    Ok(())
}
