// Written for solidcorex. Apache-2.0.
//! Broken bodies, one per invariant (63 §6.4, BR8): a valid body with one defect written
//! through the `Draft` API, so that [`violations`](super::check::violations) finds that
//! invariant and no other.

use scx_brep::{Draft, EdgeId, FaceId, Fin, RegionId, UseGeom, VertexRecord};
use scx_curve::{Circle2, Curve2};
use scx_num::fmath;
use scx_num::frame::{Frame2, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Vec2;
use scx_num::vector3d::Vec3;
use scx_surface::Surface;

use super::{GenError, Invariant, Kind, Result, generate, shifted};

/// The body for `seed` at `size` that breaks `invariant` alone; `None` for
/// [`Invariant::UvGap`], which no single defect breaks alone (a uv gap that is not a period
/// or a pole also moves a pcurve off its edge, or unchains the loop).
///
/// # Errors
///
/// [`GenError`] only on a bug.
pub fn broken(invariant: Invariant, seed: u64, size: usize) -> Result<Option<Draft>> {
    let base = |kind: Kind, size: usize| generate(kind, seed, size).map(|g| g.draft);
    let d = match invariant {
        Invariant::OutsideVoid => {
            let mut d = base(Kind::Block, size)?;
            let r = d.region_mut(RegionId::OUTSIDE).ok_or(missing("outside"))?;
            r.solid = true;
            d
        }
        Invariant::LiveReferences => {
            let mut d = base(Kind::Block, size)?;
            let e = first_edge(&d)?;
            d.remove_edge(e);
            d
        }
        Invariant::FinWithoutUse => {
            let mut d = base(Kind::Block, size)?;
            let e = first_edge(&d)?;
            // A face that does not use the edge.
            let face = d
                .faces()
                .find(|f| f.uses().all(|u| u.record().edge != e))
                .map(scx_brep::Face::id)
                .ok_or(missing("face off the edge"))?;
            adjacency(&mut d, e)?.push(Fin::new(face, Sense::Forward));
            d
        }
        Invariant::UseWithoutFin => {
            let mut d = base(Kind::Block, size)?;
            let e = first_edge(&d)?;
            adjacency(&mut d, e)?.pop();
            d
        }
        Invariant::RepeatedFin => {
            let mut d = base(Kind::Block, size)?;
            let e = first_edge(&d)?;
            let fins = adjacency(&mut d, e)?;
            let fin = *fins.first().ok_or(missing("fin"))?;
            fins.push(fin);
            d
        }
        Invariant::SameSenseTwice => {
            // The centre face of a sheet, its loop twice, with its fins.
            let n = size.max(3) | 1;
            let mut d = base(Kind::QuadSheet, n)?;
            let face = FaceId::from_raw(((n / 2) * n + n / 2) as u32);
            let f = d.face_mut(face).ok_or(missing("centre face"))?;
            let lp = f.loops.first().cloned().ok_or(missing("loop"))?;
            let passes: Vec<(EdgeId, Sense)> = lp.uses.iter().map(|u| (u.edge, u.sense)).collect();
            f.loops.push(lp);
            for (e, s) in passes {
                adjacency(&mut d, e)?.push(Fin::new(face, s));
            }
            d
        }
        Invariant::LoopChained => {
            let mut d = base(Kind::Block, size)?;
            let face = first_face(&d)?;
            let uses = &mut loop0(&mut d, face)?.uses;
            uses.swap(0, 1);
            d
        }
        Invariant::PeriodJump => {
            // A cylinder wall's top arc a turn along u: both its joins jump a period.
            let mut d = base(Kind::Cylinder, size)?;
            let face = d
                .faces()
                .find(|f| matches!(f.surface(), Surface::Cylinder(_)))
                .map(scx_brep::Face::id)
                .ok_or(missing("wall"))?;
            let f = d.face_mut(face).ok_or(missing("wall"))?;
            let period = f.surface.domain().u.period().ok_or(missing("period"))?;
            let u = f
                .loops
                .first_mut()
                .and_then(|l| l.uses.get_mut(2))
                .ok_or(missing("top arc"))?;
            if let UseGeom::Pcurve { curve, .. } = &mut u.geom {
                *curve = shifted(curve, [1.0, 0.0], [period, 0.0])?;
            }
            d
        }
        Invariant::PoleGap => {
            // A lune's north pole use and its collapsed edge gone: the meridians meet the
            // pole at different u.
            let mut d = base(Kind::Sphere, size.max(2))?;
            let face = first_face(&d)?;
            let uses = &mut loop0(&mut d, face)?.uses;
            if uses.len() < 4 {
                return Err(missing("lune"));
            }
            let pole = uses.remove(1).edge;
            d.remove_edge(pole);
            d
        }
        Invariant::UvGap => return Ok(None),
        Invariant::IntervalMismatch => {
            let mut d = base(Kind::Block, size)?;
            let face = first_face(&d)?;
            let u = loop0(&mut d, face)?
                .uses
                .first_mut()
                .ok_or(missing("use"))?;
            if let UseGeom::Pcurve { interval, .. } = &mut u.geom {
                *interval =
                    scx_num::extent1d::Extent1d::new(0.0, 0.5).ok_or(missing("interval"))?;
            }
            d
        }
        Invariant::PcurveOffEdge => {
            // A circular arc through the same ends: the joins hold, the middle leaves the
            // edge.
            let mut d = base(Kind::Block, size)?;
            let face = first_face(&d)?;
            let u = loop0(&mut d, face)?
                .uses
                .first_mut()
                .ok_or(missing("use"))?;
            if let UseGeom::Pcurve { curve, .. } = &mut u.geom {
                *curve = bowed(curve)?;
            }
            d
        }
        Invariant::VertexOffEdge => {
            let mut d = base(Kind::Block, size)?;
            let v = d
                .vertices()
                .next()
                .map(scx_brep::Vertex::id)
                .ok_or(missing("vertex"))?;
            let rec = d.vertex_mut(v).ok_or(missing("vertex"))?;
            rec.point += Vec3::new(0.0, 0.0, 0.1);
            d
        }
        Invariant::ShellOpen => {
            let mut d = base(Kind::Block, size)?;
            let face = first_face(&d)?;
            d.remove_face(face);
            d
        }
        Invariant::Misoriented => {
            // One face's loop run the other way: its front stays outward, so its uses
            // agree with its neighbours'.
            let mut d = base(Kind::Block, size)?;
            let face = first_face(&d)?;
            let uses = &mut loop0(&mut d, face)?.uses;
            uses.reverse();
            for u in uses.iter_mut() {
                u.sense = u.sense.reversed();
            }
            let edges: Vec<EdgeId> = uses.iter().map(|u| u.edge).collect();
            for e in edges {
                for fin in adjacency(&mut d, e)?.iter_mut() {
                    if fin.face == face {
                        fin.sense = fin.sense.reversed();
                    }
                }
            }
            d
        }
        Invariant::RadialOrder => {
            // Four fins whose sectors are the solid and the outside by turns: reversed, the
            // sides still agree, and only the geometry shows the order is wrong.
            let mut d = base(Kind::Touching, size)?;
            let e = d
                .edges()
                .find(|e| e.fins().len() >= 4)
                .map(scx_brep::Edge::id)
                .ok_or(missing("four-fin edge"))?;
            adjacency(&mut d, e)?.reverse();
            d
        }
        Invariant::OrphanVertex => {
            let mut d = base(Kind::Block, size)?;
            d.add_vertex(VertexRecord::new(scx_num::vector3d::Point3::new(
                9.0, 9.0, 9.0,
            )))?;
            d
        }
    };
    Ok(Some(d))
}

const fn missing(what: &'static str) -> GenError {
    GenError::Degenerate(what)
}

fn first_face(d: &Draft) -> Result<FaceId> {
    d.faces()
        .next()
        .map(scx_brep::Face::id)
        .ok_or(missing("face"))
}

fn first_edge(d: &Draft) -> Result<EdgeId> {
    d.edges()
        .next()
        .map(scx_brep::Edge::id)
        .ok_or(missing("edge"))
}

fn adjacency(d: &mut Draft, e: EdgeId) -> Result<&mut Vec<Fin>> {
    Ok(&mut d.adjacency_mut(e).ok_or(missing("adjacency"))?.fins)
}

fn loop0(d: &mut Draft, face: FaceId) -> Result<&mut scx_brep::LoopRecord> {
    d.face_mut(face)
        .and_then(|f| f.loops.first_mut())
        .ok_or(missing("loop"))
}

/// The arc of one radian from `curve`'s point at 0 to its point at 1, over `[0, 1]`.
fn bowed(curve: &Curve2) -> Result<Curve2> {
    let (a, b) = (curve.point(0.0), curve.point(1.0));
    let chord = b - a;
    let len = chord.length();
    let r = len / (2.0 * fmath::sin(0.5));
    let normal = Vec2::new(-chord.y, chord.x) * (1.0 / len);
    let centre = a + chord * 0.5 + normal * (r * fmath::cos(0.5));
    let frame = Frame2::new(centre, a - centre).ok_or(missing("arc frame"))?;
    for sense in [Sense::Forward, Sense::Reversed] {
        let c = Circle2::new(frame, Length::from_mm(r), sense)?;
        if (c.point(1.0) - b).length() < 1e-9 * (1.0 + len) {
            return Ok(Curve2::Circle(c));
        }
    }
    Err(missing("arc"))
}
