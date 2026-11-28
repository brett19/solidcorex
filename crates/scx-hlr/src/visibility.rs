// Written for solidcorex. Apache-2.0.
//! Which parts of a scene's edges and silhouettes are seen (research 74 §5.3, task HL2,
//! §37).
//!
//! The method, in three steps:
//!
//! 1. **Every place a curve's visibility can change is a cut.** A curve's visibility changes
//!    only where its image crosses the image of an outline (a drawn edge or a silhouette:
//!    an occluder's outline is made of these) or where it passes through a face of another
//!    body. The first are found by [`scx_planar::crossings`] on the exact images (lines and
//!    conic arcs), certified complete; the second by `scx_surface::intersect_curve`, which is
//!    complete for analytic curves and surfaces. An extra cut changes nothing; only a missed
//!    one could, and none is missed.
//! 2. **One classification per span.** Between cuts the visibility is constant, so any
//!    point inside a span decides it: a ray from the point towards the eye, against every
//!    face whose image box holds the point and that lies nearer the eye. The faces are taken
//!    in turn, each against the spans whose points its box holds (a 2-D tree over the
//!    points), so one face's set-up serves every span behind it. A face hides the point when
//!    the ray meets its closure beyond the coincidence distance; the nearest such face is
//!    named.
//! 3. **What cannot be decided is named.** A span is [`Visibility::Uncertain`] only when the
//!    budget ran out before it was classified. A span whose cuts or rays wait on step 5
//!    (a NURBS face, a torus contour, an outline not found) is [`Visibility::Pending`] with
//!    the reason.

use std::collections::BTreeMap;

use scx_brep::op::{Cx, Event};
use scx_brep::{Brep, EdgeId, EdgeShape, Face, FaceId, UvSide};
use scx_curve::{Curve, Line};
use scx_num::budget::{Budget, Exhausted, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::frame::Frame;
use scx_num::invert::Inversion;
use scx_num::units::Resolution;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{CrossError, crossings};
use scx_surface::{CurveSurfaceHit, Plane, Surface, SurfaceError, intersect_curve};

use crate::error::HlrError;
use crate::project::{Lens, NoImage, Piece, meet, pieces};
use crate::scene::Scene;
use crate::silhouette::{OnFace, SilhouetteCurve, Undecided, Unsupported, of_body, rims};
use crate::tolerance::{ROUNDING, Tolerances};
use crate::view::View;

/// A scene's hidden-line drawing under a view.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawing {
    /// Every drawn curve, body by body in the scene's order, each body's edges in id order
    /// and then its silhouettes in face order.
    pub lines: Vec<DrawnLine>,
    /// The faces whose silhouettes are not found, by body: what they hide is not known, and
    /// the spans they may hide are [`Pending::Outline`].
    pub unsupported: Vec<(usize, Unsupported)>,
    /// The budget's exhaustion, when it ran out while spans were classified: the spans not
    /// yet classified are [`Visibility::Uncertain`].
    pub exhausted: Option<Exhausted>,
}

/// One drawn curve, cut into spans.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnLine {
    /// The body's position in the scene.
    pub body: usize,
    /// What the curve is.
    pub source: Source,
    /// The spans in increasing parameter order, adjacent spans of one visibility merged.
    pub spans: Vec<Span>,
}

/// What a drawn curve is.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// An edge of the body (its own curve and parameter).
    Edge(EdgeId),
    /// A silhouette of a face, in the body's coordinates (a silhouette held by several faces
    /// of one surface is drawn once, under the first).
    Silhouette {
        /// The face.
        face: FaceId,
        /// The curve.
        curve: SilhouetteCurve,
    },
}

/// A stretch of a drawn curve with one visibility.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    /// The curve's parameters.
    pub t: Extent1d,
    /// Whether it is seen.
    pub visibility: Visibility,
}

/// Whether a span is seen.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Visibility {
    /// No face lies between the span and the eye.
    Visible,
    /// This face (of the body at this position in the scene) lies between the span and the
    /// eye, the nearest such face found.
    Hidden {
        /// The occluder's body.
        body: usize,
        /// The occluder.
        face: FaceId,
    },
    /// The budget ran out before the span was classified.
    Uncertain,
    /// The span's visibility waits on what is named.
    Pending(Pending),
}

/// What a span's visibility waits on.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Pending {
    /// The curve itself has no exact image here: a NURBS curve, a parabola or a hyperbola
    /// (step 5's planar widening).
    Curve,
    /// The curve is a torus contour, whose crossings wait on step 5's general plane-curve
    /// intersections.
    TorusContour,
    /// The silhouette's part on its face is not decided (research 74 HL1).
    Silhouette(Undecided),
    /// The crossings of the span's image with another curve's could not be settled.
    Crossing(CrossError),
    /// Whether the curve passes through this face waits on step 5 (a NURBS face or a NURBS
    /// boundary edge).
    Piercing {
        /// The face's body.
        body: usize,
        /// The face.
        face: FaceId,
    },
    /// Whether this face hides the span waits on step 5: a ray against a NURBS face.
    Occluder {
        /// The face's body.
        body: usize,
        /// The face.
        face: FaceId,
    },
    /// This face's outline is not known exactly (a freeform face, a torus contour, a
    /// silhouette not decided, a NURBS or other pending edge), and its image may cross the
    /// span's: the span may not be one visibility.
    Outline {
        /// The face's body (or the edge's).
        body: usize,
        /// The face (or a face of the edge).
        face: FaceId,
    },
    /// No point of the span tried could be placed inside or outside this face
    /// (`Face::contains_uv` could not tell, or the face has no normal there).
    Unplaced {
        /// The face's body.
        body: usize,
        /// The face.
        face: FaceId,
    },
}

/// How many points of a span are tried before it is [`Pending::Unplaced`]: the midpoint,
/// then the quarter points, then the eighths, to six levels. Each point decides the whole
/// span, so a second point is only ever needed when `Face::contains_uv` cannot tell, which
/// happens within a few roundings of a boundary; a span on which 63 points spread over it
/// all fall there lies along that boundary.
const PROBES: u32 = 63;

impl Scene<'_> {
    /// The scene's hidden-line drawing under `view`: every edge (but seams and the edges
    /// between faces of one surface) and every silhouette, cut where its visibility can
    /// change, each span classified once.
    ///
    /// Emits the phases `"hlr.silhouettes"`, `"hlr.cuts"` and `"hlr.visibility"`. Charges
    /// the budget for the silhouettes (as [`crate::silhouettes`]), for each crossing and
    /// piercing search, and one unit for each ray besides what its intersection charges.
    ///
    /// # Errors
    ///
    /// [`HlrError::Stopped`]; [`HlrError::Exhausted`] when the budget runs out before the
    /// first span is classified (finding the cuts: nothing is decided yet); a surface or
    /// curve's refusal as [`crate::silhouettes`]; [`HlrError::BadPlacement`] when the view
    /// cannot be carried into a body's frame.
    pub fn hidden_lines(&self, view: &View, cx: &mut Cx<'_>) -> Result<Drawing, HlrError> {
        let mut budget = cx.budget();
        let stop = cx.stop;
        let origin = self.centre().unwrap_or(Point3::new(0.0, 0.0, 0.0));
        let mut lenses = Vec::with_capacity(self.bodies().len());
        for (i, (_, frame)) in self.bodies().iter().enumerate() {
            lenses.push(Lens::new(view, *frame, origin).ok_or(HlrError::BadPlacement { body: i })?);
        }
        let reach = self
            .bounds()
            .map_or(0.0, |b| b.max_distance_to_point(Point3::new(0.0, 0.0, 0.0)));
        let mut work = Work {
            bodies: self.bodies().iter().map(|(b, _)| *b).collect(),
            lenses,
            reach,
            res: finest(self.bodies()),
            items: Vec::new(),
            outlines: Vec::new(),
            unsupported: Vec::new(),
            budget: &mut budget,
            stop,
        };
        cx.trace.event(Event::Phase("hlr.silhouettes"));
        work.gather()?;
        cx.trace.event(Event::Phase("hlr.cuts"));
        work.image_cuts()?;
        work.piercings()?;
        cx.trace.event(Event::Phase("hlr.visibility"));
        let (spans, exhausted) = work.classify(cx)?;
        Ok(work.drawing(spans, exhausted))
    }
}

/// The finest resolution of the scene's bodies: crossings closer than it are one meet.
/// Element `i` of `v`: the indices are the work's own, so a miss is a defect.
fn at<T>(v: &[T], i: usize) -> Result<&T, HlrError> {
    v.get(i).ok_or(HlrError::Defect {
        reason: "an index of the drawing's work is out of range",
    })
}

/// [`at`], mutable.
fn at_mut<T>(v: &mut [T], i: usize) -> Result<&mut T, HlrError> {
    v.get_mut(i).ok_or(HlrError::Defect {
        reason: "an index of the drawing's work is out of range",
    })
}

fn finest(bodies: &[(&Brep, Frame)]) -> Resolution {
    bodies
        .iter()
        .map(|(b, _)| b.resolution())
        .min_by(|a, b| a.zone().mm().total_cmp(&b.zone().mm()))
        .unwrap_or_default()
}

/// A drawn curve while it is cut.
struct Item {
    body: usize,
    source: Source,
    /// The curve in its body's coordinates, when it has an exact image.
    curve: Option<Curve>,
    /// The parameter ranges drawn.
    ranges: Vec<[f64; 2]>,
    pieces: Vec<Piece>,
    cuts: Vec<f64>,
    /// Parameter ranges whose visibility waits, with the reason.
    pending: Vec<([f64; 2], Pending)>,
    /// How far the curve may lie from where it is drawn: its edge's recorded accuracy.
    accuracy: f64,
}

/// An outline whose image is not known: every span behind its box waits on it.
struct Outline {
    body: usize,
    face: FaceId,
    bounds: [f64; 4],
    near: f64,
}

/// A face as an occluder.
struct Occluder<'a> {
    body: usize,
    face: Face<'a>,
    bounds: [f64; 4],
    near: f64,
}

struct Work<'a, 'b> {
    bodies: Vec<&'a Brep>,
    lenses: Vec<Lens>,
    /// The scene's largest distance from the world's origin: the size of its coordinates.
    reach: f64,
    res: Resolution,
    items: Vec<Item>,
    outlines: Vec<Outline>,
    unsupported: Vec<(usize, Unsupported)>,
    budget: &'b mut Budget,
    stop: &'b Stop,
}

/// What a ray met on one face.
enum Ray {
    Miss,
    Hit(f64),
    Pending,
    Unplaced,
}

/// A span while it is classified.
struct Open {
    item: usize,
    t: [f64; 2],
    /// Points tried so far.
    probes: u32,
    best: Option<(f64, usize, FaceId)>,
    pending: Option<Pending>,
    unplaced: Option<(usize, FaceId)>,
    done: Option<Visibility>,
}

impl<'a> Work<'a, '_> {
    /// The drawn curves of every body and the outlines that are not known.
    fn gather(&mut self) -> Result<(), HlrError> {
        for (i, body) in self.bodies.clone().into_iter().enumerate() {
            let lens = *at(&self.lenses, i)?;
            self.edges(i, body, &lens);
            let found = of_body(
                body,
                &lens.local,
                self.budget,
                self.stop,
                &scx_brep::op::NoTrace,
            )?;
            let mut held: Vec<(SilhouetteCurve, FaceId, Vec<[f64; 2]>)> = Vec::new();
            for s in found.curves {
                let face = s.face;
                match s.on_face {
                    OnFace::Proved(ps) => {
                        let ranges = ps.iter().map(|p| [p.min(), p.max()]);
                        if let Some(h) = held.iter_mut().find(|h| h.0 == s.curve) {
                            h.2.extend(ranges);
                        } else {
                            held.push((s.curve, face, ranges.collect()));
                        }
                    }
                    OnFace::Undecided { over, why } => {
                        self.outline_of_face(i, &lens, body, face);
                        let range = [over.min(), over.max()];
                        self.items.push(Item {
                            body: i,
                            source: Source::Silhouette {
                                face,
                                curve: s.curve,
                            },
                            curve: None,
                            ranges: vec![range],
                            pieces: Vec::new(),
                            cuts: Vec::new(),
                            pending: vec![(range, Pending::Silhouette(why))],
                            accuracy: 0.0,
                        });
                    }
                }
            }
            for (curve, face, ranges) in held {
                let ranges = union(ranges);
                let as_curve = match &curve {
                    SilhouetteCurve::Line(l) => Some(Curve::Line(*l)),
                    SilhouetteCurve::Circle(c) => Some(Curve::Circle(*c)),
                    SilhouetteCurve::Torus(_) => None,
                };
                let mut item = Item {
                    body: i,
                    source: Source::Silhouette { face, curve },
                    curve: None,
                    ranges,
                    pieces: Vec::new(),
                    cuts: Vec::new(),
                    pending: Vec::new(),
                    accuracy: 0.0,
                };
                if let Some(c) = as_curve {
                    image(&mut item, c, &lens);
                } else {
                    self.outline_of_face(i, &lens, body, face);
                    item.pending = item
                        .ranges
                        .iter()
                        .map(|r| (*r, Pending::TorusContour))
                        .collect();
                }
                if !item.pieces.is_empty() || !item.pending.is_empty() {
                    self.items.push(item);
                }
            }
            for u in found.unsupported {
                self.outline_of_face(i, &lens, body, u.face);
                self.unsupported.push((i, u));
            }
        }
        Ok(())
    }

    /// The body's drawn edges: all but seams and edges between faces of one surface.
    fn edges(&mut self, i: usize, body: &'a Brep, lens: &Lens) {
        let mut faces: BTreeMap<EdgeId, Vec<Face<'a>>> = BTreeMap::new();
        for f in body.faces() {
            for u in f.uses() {
                faces.entry(u.edge().id()).or_default().push(f);
            }
        }
        for e in body.edges() {
            let EdgeShape::Curve { curve, interval } = e.shape() else {
                continue;
            };
            let around = faces.get(&e.id()).map_or(&[][..], Vec::as_slice);
            if let [first, rest @ ..] = around
                && !rest.is_empty()
                && rest.iter().all(|f| f.surface() == first.surface())
            {
                continue;
            }
            let mut item = Item {
                body: i,
                source: Source::Edge(e.id()),
                curve: None,
                ranges: vec![[interval.min(), interval.max()]],
                pieces: Vec::new(),
                cuts: Vec::new(),
                pending: Vec::new(),
                accuracy: e.accuracy().map_or(0.0, |r| r.acc.length().mm()),
            };
            if !image(&mut item, curve.clone(), lens) {
                // An edge with no exact image is an outline not known for its faces.
                if let Some(f) = around.first()
                    && let Some(b) = e.bounding_box()
                {
                    let (bounds, near) = box_image(lens, &b);
                    self.outlines.push(Outline {
                        body: i,
                        face: f.id(),
                        bounds,
                        near,
                    });
                }
            }
            if !item.pieces.is_empty() || !item.pending.is_empty() {
                self.items.push(item);
            }
        }
    }

    fn outline_of_face(&mut self, i: usize, lens: &Lens, body: &Brep, face: FaceId) {
        let Some(b) = body.face(face).and_then(Face::bounding_box) else {
            // A face with no box reaches everywhere.
            self.outlines.push(Outline {
                body: i,
                face,
                bounds: [
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                    f64::INFINITY,
                    f64::INFINITY,
                ],
                near: f64::NEG_INFINITY,
            });
            return;
        };
        let (bounds, near) = box_image(lens, &b);
        self.outlines.push(Outline {
            body: i,
            face,
            bounds,
            near,
        });
    }

    /// The coincidence distance between a point of a curve of accuracy `acc` and a face
    /// of body `body`: within it the ray's start is the point itself (its own faces meet
    /// it there). Twice the finest zone, plus the curve's and the face's recorded
    /// accuracies, plus the rounding of the scene's coordinates.
    fn coincidence(&self, acc: f64, face: Face<'_>) -> f64 {
        let face_acc = face.accuracy().map_or(0.0, |a| a.length().mm());
        let zone = self.res.zone().mm();
        2.0 * zone + (acc + face_acc) + 8.0 * ROUNDING * self.reach
    }

    /// Cuts every curve where its image meets another curve's image.
    fn image_cuts(&mut self) -> Result<(), HlrError> {
        let mut all: Vec<(usize, usize, [f64; 4])> = Vec::new();
        for (i, item) in self.items.iter().enumerate() {
            for (k, piece) in item.pieces.iter().enumerate() {
                all.push((i, k, piece.bounds));
            }
        }
        all.sort_by(|a, b| a.2[0].total_cmp(&b.2[0]).then((a.0, a.1).cmp(&(b.0, b.1))));
        let pad = self.res.zone().mm();
        let mut pairs = Vec::new();
        for (n, a) in all.iter().enumerate() {
            for b in all.iter().skip(n + 1) {
                if b.2[0] > a.2[2] + pad {
                    break;
                }
                if a.0 != b.0 && meet(&a.2, &b.2, pad) {
                    let (x, y) = ((a.0, a.1), (b.0, b.1));
                    pairs.push(if x < y { (x, y) } else { (y, x) });
                }
            }
        }
        pairs.sort_unstable();
        for ((i, k), (j, l)) in pairs {
            self.stop.check()?;
            let pa = at(&at(&self.items, i)?.pieces, k)?;
            let pb = at(&at(&self.items, j)?.pieces, l)?;
            match crossings(&pa.image, &pb.image, self.res, self.budget, self.stop) {
                Ok(found) => {
                    let (ma, mb) = (pa.map, pb.map);
                    let mut ca = Vec::new();
                    let mut cb = Vec::new();
                    for m in &found.meets {
                        ca.push(ma.t(0.5 * (m.a[0] + m.a[1])));
                        cb.push(mb.t(0.5 * (m.b[0] + m.b[1])));
                    }
                    for o in &found.overlaps {
                        ca.extend([ma.t(o.a[0]), ma.t(o.a[1])]);
                        cb.extend([mb.t(o.b[0]), mb.t(o.b[1])]);
                    }
                    at_mut(&mut self.items, i)?.cuts.append(&mut ca);
                    at_mut(&mut self.items, j)?.cuts.append(&mut cb);
                }
                Err(CrossError::Exhausted(e)) => return Err(HlrError::Exhausted(e)),
                Err(CrossError::Stopped(_)) => return Err(HlrError::Stopped),
                Err(e) => {
                    let (ta, tb) = (pa.t, pb.t);
                    at_mut(&mut self.items, i)?
                        .pending
                        .push((ta, Pending::Crossing(e)));
                    at_mut(&mut self.items, j)?
                        .pending
                        .push((tb, Pending::Crossing(e)));
                }
            }
        }
        Ok(())
    }

    /// Cuts every curve where it passes through a face of another body.
    fn piercings(&mut self) -> Result<(), HlrError> {
        for n in 0..self.items.len() {
            let (curve, a, ranges) = {
                let item = at(&self.items, n)?;
                let ranges: Vec<[f64; 2]> = item.pieces.iter().map(|p| p.t).collect();
                (item.curve.clone(), item.body, ranges)
            };
            let Some(curve) = curve else {
                continue;
            };
            let from = *at(&self.lenses, a)?;
            let mut cuts = Vec::new();
            let mut waits = Vec::new();
            for (b, body) in self.bodies.clone().into_iter().enumerate() {
                if b == a {
                    continue;
                }
                let to = *at(&self.lenses, b)?;
                let Some(moved) = transfer(&curve, &from, &to) else {
                    return Err(HlrError::BadPlacement { body: a });
                };
                for &t in &ranges {
                    let Some(ext) = Extent1d::new(t[0], t[1]) else {
                        continue;
                    };
                    let Some(cbox) = curve_box(&moved, t) else {
                        continue;
                    };
                    for face in body.faces_meeting(cbox) {
                        self.stop.check()?;
                        let Some(uv) = face.uv_box() else {
                            continue;
                        };
                        match intersect_curve(
                            &moved,
                            &ext,
                            face.surface(),
                            &uv,
                            body.resolution(),
                            self.budget,
                            self.stop,
                        ) {
                            Ok(hits) => {
                                for h in hits {
                                    match h {
                                        CurveSurfaceHit::Point { t, .. } => {
                                            cuts.push(t);
                                        }
                                        CurveSurfaceHit::Overlap { t } => {
                                            cuts.extend([t.min(), t.max()]);
                                        }
                                    }
                                }
                            }
                            Err(
                                SurfaceError::NurbsIntersection | SurfaceError::Undecided { .. },
                            ) => {
                                waits.push((
                                    t,
                                    Pending::Piercing {
                                        body: b,
                                        face: face.id(),
                                    },
                                ));
                            }
                            Err(e) => return Err(HlrError::surface(face.id(), e)),
                        }
                    }
                }
            }
            let item = at_mut(&mut self.items, n)?;
            item.cuts.append(&mut cuts);
            item.pending.append(&mut waits);
        }
        Ok(())
    }

    /// The spans of every curve, each classified (or waiting, or uncertain), and the
    /// budget's exhaustion if it ran out.
    fn classify(&mut self, cx: &Cx<'_>) -> Result<(Vec<Open>, Option<Exhausted>), HlrError> {
        let mut spans = self.spans();
        let occluders = self.occluders();
        let mut exhausted = None;
        let total = spans.len() as u64;
        loop {
            let open: Vec<usize> = spans
                .iter()
                .enumerate()
                .filter(|(_, s)| s.done.is_none())
                .map(|(s, _)| s)
                .collect();
            if open.is_empty() {
                break;
            }
            match self.round(&mut spans, &open, &occluders) {
                Ok(()) => {}
                Err(HlrError::Exhausted(e)) => {
                    exhausted = Some(e);
                    for &s in &open {
                        let sp = at_mut(&mut spans, s)?;
                        sp.done = Some(match sp.best {
                            Some((_, body, face)) => Visibility::Hidden { body, face },
                            None => Visibility::Uncertain,
                        });
                    }
                    break;
                }
                Err(e) => return Err(e),
            }
            let done = spans.iter().filter(|s| s.done.is_some()).count() as u64;
            cx.trace.event(Event::Progress { done, total });
        }
        Ok((spans, exhausted))
    }

    /// One point of each open span against every face in front of it.
    fn round(
        &mut self,
        spans: &mut [Open],
        open: &[usize],
        occluders: &[Occluder<'a>],
    ) -> Result<(), HlrError> {
        // Each open span's next point, world and screen.
        let mut points: Vec<(usize, Point3, Point2, f64)> = Vec::with_capacity(open.len());
        for &s in open {
            let sp = at_mut(spans, s)?;
            sp.probes += 1;
            let f = probe(sp.probes);
            let t = sp.t[0] + f * (sp.t[1] - sp.t[0]);
            let item = at(&self.items, sp.item)?;
            let Some(curve) = &item.curve else {
                return Err(HlrError::Defect {
                    reason: "a span to classify has no curve",
                });
            };
            let lens = at(&self.lenses, item.body)?;
            let p = curve.point(t);
            let (q, depth) = lens.image(p);
            points.push((s, lens.world(p), q, depth));
            sp.best = None;
            sp.pending = None;
            sp.unplaced = None;
        }
        let tree = Tree::new(points.iter().enumerate().map(|(n, a)| (a.2, n)).collect());
        let mut found = Vec::new();
        for occ in occluders {
            found.clear();
            tree.query(&occ.bounds, &mut found);
            found.sort_unstable();
            for &n in &found {
                let (s, world, _, depth) = *at(&points, n)?;
                let acc = at(&self.items, at(spans, s)?.item)?.accuracy;
                let c = self.coincidence(acc, occ.face);
                let reach = depth - occ.near + 2.0 * c;
                if reach <= c {
                    continue;
                }
                self.stop.check()?;
                self.budget.charge(1)?;
                match self.ray(world, reach, c, occ)? {
                    Ray::Miss => {}
                    Ray::Hit(t) => {
                        let sp = at_mut(spans, s)?;
                        if sp.best.is_none_or(|b| t < b.0) {
                            sp.best = Some((t, occ.body, occ.face.id()));
                        }
                    }
                    Ray::Pending => {
                        at_mut(spans, s)?
                            .pending
                            .get_or_insert_with(|| Pending::Occluder {
                                body: occ.body,
                                face: occ.face.id(),
                            });
                    }
                    Ray::Unplaced => {
                        at_mut(spans, s)?
                            .unplaced
                            .get_or_insert_with(|| (occ.body, occ.face.id()));
                    }
                }
            }
        }
        for &(s, ..) in &points {
            let sp = at_mut(spans, s)?;
            sp.done = if let Some((_, body, face)) = sp.best {
                Some(Visibility::Hidden { body, face })
            } else if let Some(p) = sp.pending {
                Some(Visibility::Pending(p))
            } else if let Some((body, face)) = sp.unplaced {
                (sp.probes >= PROBES)
                    .then_some(Visibility::Pending(Pending::Unplaced { body, face }))
            } else {
                Some(Visibility::Visible)
            };
        }
        Ok(())
    }

    /// The ray from the world point `world` towards the eye, over `(c, reach]`, against
    /// `occ`.
    fn ray(
        &mut self,
        world: Point3,
        reach: f64,
        c: f64,
        occ: &Occluder<'_>,
    ) -> Result<Ray, HlrError> {
        let lens = at(&self.lenses, occ.body)?;
        let face = occ.face;
        let q = lens.body(world);
        let dir = -*lens.local.d();
        let Ok(line) = Line::new(q, dir) else {
            return Err(HlrError::Defect {
                reason: "a ray has no direction",
            });
        };
        let Some(uv) = face.uv_box() else {
            return Ok(Ray::Miss);
        };
        let ext = span(0.0, reach)?;
        let res = at(&self.bodies, occ.body)?.resolution();
        let hits = match intersect_curve(
            &Curve::Line(line),
            &ext,
            face.surface(),
            &uv,
            res,
            self.budget,
            self.stop,
        ) {
            Ok(h) => h,
            Err(SurfaceError::NurbsIntersection | SurfaceError::Undecided { .. }) => {
                return Ok(Ray::Pending);
            }
            Err(e) => return Err(HlrError::surface(face.id(), e)),
        };
        let mut answer = Ray::Miss;
        let mut unplaced = false;
        for h in hits {
            let got = match h {
                CurveSurfaceHit::Point { t, uv, .. } => {
                    if t <= c {
                        continue;
                    }
                    match face.contains_uv(uv) {
                        Some(UvSide::Inside | UvSide::Near) => Ray::Hit(t),
                        Some(UvSide::Outside) => Ray::Miss,
                        None => Ray::Unplaced,
                    }
                }
                CurveSurfaceHit::Overlap { t } => {
                    let lo = t.min().max(c);
                    if t.max() <= lo {
                        continue;
                    }
                    self.along(face, res, q, dir, [lo, t.max()], c, &uv)?
                }
            };
            match got {
                Ray::Hit(t) => {
                    if let Ray::Hit(b) = answer {
                        answer = Ray::Hit(b.min(t));
                    } else {
                        answer = Ray::Hit(t);
                    }
                }
                Ray::Pending => {
                    if !matches!(answer, Ray::Hit(_)) {
                        answer = Ray::Pending;
                    }
                }
                Ray::Unplaced => unplaced = true,
                Ray::Miss => {}
            }
        }
        if unplaced && matches!(answer, Ray::Miss) {
            answer = Ray::Unplaced;
        }
        Ok(answer)
    }

    /// A ray lying in the face's surface over `range` (a face seen edge on, or a ruling
    /// along the sight): it meets the closed face where a boundary edge crosses it, or, if
    /// none does, where its middle lies inside the face.
    #[allow(clippy::too_many_arguments)]
    fn along(
        &mut self,
        face: Face<'_>,
        res: Resolution,
        q: Point3,
        dir: Vec3,
        range: [f64; 2],
        c: f64,
        uv: &Extent2d,
    ) -> Result<Ray, HlrError> {
        let id = face.id();
        let Some(fbox) = face.bounding_box() else {
            return Ok(Ray::Unplaced);
        };
        let Some(tol) = Tolerances::of(&fbox, res) else {
            return Err(HlrError::Defect {
                reason: "a resolution's approximation accuracy is not an accuracy",
            });
        };
        let surface = face.surface();
        let inv = surface
            .invert(q, None, Some(uv), self.budget, self.stop)
            .map_err(|e| HlrError::surface(id, e))?;
        let Some(n) = analytic_normal(surface, inv.foot().at) else {
            return Ok(Ray::Unplaced);
        };
        // The plane through the ray square to the surface's tangent plane along it (for a
        // plane face, square to the face; for a ruling, the tangent plane, constant along
        // it): it meets the face's boundary edges where they cross the ray.
        let Some(frame) = Frame::new(q, n.cross(dir), dir) else {
            return Ok(Ray::Unplaced);
        };
        let cut = Surface::Plane(Plane::new(frame));
        let half = 2.0 * fbox.max_distance_to_point(q) + res.zone().mm();
        let Some(se) = Extent2d::from_bounds(-half, -half, half, half) else {
            return Err(HlrError::Defect {
                reason: "a face box's span is not finite",
            });
        };
        let mut first: Option<f64> = None;
        for rim in rims(face, &tol) {
            let hits = match intersect_curve(
                rim.curve,
                &rim.interval,
                &cut,
                &se,
                res,
                self.budget,
                self.stop,
            ) {
                Ok(h) => h,
                Err(SurfaceError::NurbsIntersection | SurfaceError::Undecided { .. }) => {
                    return Ok(Ray::Pending);
                }
                Err(e) => return Err(HlrError::surface(id, e)),
            };
            let mut at = |p: Point3| {
                let r = p - q;
                let t = r.dot(dir);
                let off = (r - dir * t).length();
                if off <= rim.near + c && t > range[0] {
                    first = Some(first.map_or(t, |f: f64| f.min(t)));
                }
            };
            for h in hits {
                match h {
                    CurveSurfaceHit::Point { point, .. } => at(point),
                    CurveSurfaceHit::Overlap { t } => {
                        at(rim.curve.point(t.min()));
                        at(rim.curve.point(t.max()));
                    }
                }
            }
        }
        if let Some(t) = first
            && t <= range[1] + c
        {
            return Ok(Ray::Hit(t));
        }
        // No boundary crosses the ray between its start and `next`: all of that stretch is
        // inside the face or all outside. It is judged at a point well away from the start
        // (which may lie on the face's boundary, where `contains_uv` says `Near`), up to a
        // face's size along.
        let next = first.unwrap_or(f64::INFINITY);
        let size = 2.0 * fbox.max_distance_to_point(fbox.mid());
        let mid = 0.5 * (range[0] + next.min(range[0] + (range[1] - range[0]).max(size)));
        let p = q + dir * mid;
        let inv = surface
            .invert(p, None, Some(uv), self.budget, self.stop)
            .map_err(|e| HlrError::surface(id, e))?;
        Ok(match inv {
            Inversion::Outside(_) => Ray::Miss,
            Inversion::Inside(foot) => match face.contains_uv(foot.at) {
                Some(UvSide::Inside | UvSide::Near) => Ray::Hit(range[0]),
                Some(UvSide::Outside) => Ray::Miss,
                None => Ray::Unplaced,
            },
        })
    }

    /// Every face of every body as an occluder, with its image box and nearest depth.
    fn occluders(&self) -> Vec<Occluder<'a>> {
        let mut out = Vec::new();
        for (i, (body, lens)) in self.bodies.iter().zip(&self.lenses).enumerate() {
            for face in body.faces() {
                let Some(b) = face.bounding_box() else {
                    continue;
                };
                let (bounds, near) = box_image(lens, &b);
                let pad = self.coincidence(0.0, face);
                out.push(Occluder {
                    body: i,
                    face,
                    bounds: [
                        bounds[0] - pad,
                        bounds[1] - pad,
                        bounds[2] + pad,
                        bounds[3] + pad,
                    ],
                    near,
                });
            }
        }
        out
    }

    /// Every curve's spans between its cuts, those that wait decided at once.
    fn spans(&self) -> Vec<Open> {
        let mut out = Vec::new();
        for (n, item) in self.items.iter().enumerate() {
            for r in &item.ranges {
                let eps = ROUNDING * r[0].abs().max(r[1].abs()).max(1.0);
                let mut cuts: Vec<f64> = item
                    .cuts
                    .iter()
                    .copied()
                    .filter(|&c| r[0] + eps < c && c < r[1] - eps)
                    .collect();
                cuts.sort_by(f64::total_cmp);
                // Cuts closer together than the zone, or to an end, are one place: a span
                // shorter than the resolution is not drawn apart from its neighbour.
                let zone = self.res.zone().mm();
                let apart = |a: f64, b: f64| match &item.curve {
                    Some(c) => c.point(a).distance(c.point(b)) > zone,
                    None => b > a + eps,
                };
                let mut stops = vec![r[0]];
                for c in cuts {
                    if stops.last().is_some_and(|&l| c > l + eps && apart(l, c)) && apart(c, r[1]) {
                        stops.push(c);
                    }
                }
                stops.push(r[1]);
                for w in stops.windows(2) {
                    if let &[a, b] = w {
                        out.push(self.open(n, item, [a, b]));
                    }
                }
            }
        }
        out
    }

    fn open(&self, n: usize, item: &Item, t: [f64; 2]) -> Open {
        let mut bounds = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        let mut far = f64::NEG_INFINITY;
        for p in &item.pieces {
            if p.t[0] <= t[1] && t[0] <= p.t[1] {
                bounds[0] = bounds[0].min(p.bounds[0]);
                bounds[1] = bounds[1].min(p.bounds[1]);
                bounds[2] = bounds[2].max(p.bounds[2]);
                bounds[3] = bounds[3].max(p.bounds[3]);
                far = far.max(p.depth[1]);
            }
        }
        let waits = item
            .pending
            .iter()
            .find(|(r, _)| r[0] <= t[1] && t[0] <= r[1])
            .map(|(_, p)| *p)
            .or_else(|| {
                item.curve.as_ref()?;
                self.outlines
                    .iter()
                    .find(|o| {
                        !(o.body == item.body && matches!(item.source, Source::Silhouette { face, .. } if face == o.face))
                            && o.near < far
                            && meet(&o.bounds, &bounds, self.res.zone().mm())
                    })
                    .map(|o| Pending::Outline {
                        body: o.body,
                        face: o.face,
                    })
            });
        let done = match waits {
            Some(p) => Some(Visibility::Pending(p)),
            None if item.curve.is_none() => Some(Visibility::Pending(Pending::Curve)),
            None => None,
        };
        Open {
            item: n,
            t,
            probes: 0,
            best: None,
            pending: None,
            unplaced: None,
            done,
        }
    }

    /// The drawing: each curve's spans, adjacent spans of one visibility merged.
    fn drawing(self, spans: Vec<Open>, exhausted: Option<Exhausted>) -> Drawing {
        let mut lines: Vec<DrawnLine> = self
            .items
            .iter()
            .map(|item| DrawnLine {
                body: item.body,
                source: item.source.clone(),
                spans: Vec::new(),
            })
            .collect();
        for sp in spans {
            let Some(t) = Extent1d::new(sp.t[0], sp.t[1]) else {
                continue;
            };
            let visibility = sp.done.unwrap_or(Visibility::Uncertain);
            let Some(line) = lines.get_mut(sp.item) else {
                continue;
            };
            match line.spans.last_mut() {
                Some(last) if last.visibility == visibility && last.t.max() == t.min() => {
                    if let Some(m) = Extent1d::new(last.t.min(), t.max()) {
                        last.t = m;
                    }
                }
                _ => line.spans.push(Span { t, visibility }),
            }
        }
        Drawing {
            lines,
            unsupported: self.unsupported,
            exhausted,
        }
    }
}

/// The `k`-th point of a span, as a fraction of it: 1/2, 1/4, 3/4, 1/8, 3/8, … (the van der
/// Corput sequence), so the points spread evenly and none repeats.
/// Sets `item`'s curve and image pieces; false (and the ranges pending) when it has no
/// exact image. A curve seen end on is a point and has neither.
fn image(item: &mut Item, curve: Curve, lens: &Lens) -> bool {
    for r in &item.ranges {
        match pieces(&curve, r[0], r[1], lens) {
            Ok(mut p) => item.pieces.append(&mut p),
            Err(NoImage::Point) => {}
            Err(NoImage::Pending) => {
                item.pending = item.ranges.iter().map(|r| (*r, Pending::Curve)).collect();
                item.pieces.clear();
                return false;
            }
        }
    }
    item.curve = Some(curve);
    true
}

/// The image box of a box under `lens`, and its nearest depth.
fn box_image(lens: &Lens, b: &scx_num::extent3d::Extent3d) -> ([f64; 4], f64) {
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut near = f64::INFINITY;
    for c in b.corners() {
        let (p, d) = lens.image(c);
        bounds[0] = bounds[0].min(p.x);
        bounds[1] = bounds[1].min(p.y);
        bounds[2] = bounds[2].max(p.x);
        bounds[3] = bounds[3].max(p.y);
        near = near.min(d);
    }
    (bounds, near)
}

fn probe(k: u32) -> f64 {
    let mut f = 0.0;
    let mut scale = 0.5;
    let mut k = k;
    while k > 0 {
        if k & 1 == 1 {
            f += scale;
        }
        scale *= 0.5;
        k >>= 1;
    }
    f
}

/// An analytic surface's normal at `uv` (a NURBS face never reaches here: its ray is
/// pending first).
fn analytic_normal(surface: &Surface, uv: Point2) -> Option<scx_num::vector3d::UnitVec3> {
    match surface {
        Surface::Plane(k) => Some(k.normal(uv)),
        Surface::Cylinder(k) => Some(k.normal(uv)),
        Surface::Cone(k) => k.normal(uv),
        Surface::Sphere(k) => Some(k.normal(uv)),
        Surface::Torus(k) => Some(k.normal(uv)),
        _ => None,
    }
}

/// `[a, b]`.
fn span(a: f64, b: f64) -> Result<Extent1d, HlrError> {
    Extent1d::new(a, b).ok_or(HlrError::Defect {
        reason: "a span is not ordered and finite",
    })
}

/// Overlapping or touching ranges joined, in increasing order.
fn union(mut ranges: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    ranges.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(ranges.len());
    for r in ranges {
        match out.last_mut() {
            Some(l) if r[0] <= l[1] => l[1] = l[1].max(r[1]),
            _ => out.push(r),
        }
    }
    out
}

/// `curve` of the body seen through `from` in the coordinates of the body seen through
/// `to`, with the same parameter (both maps are rigid).
fn transfer(curve: &Curve, from: &Lens, to: &Lens) -> Option<Curve> {
    let point = |p: Point3| to.body(from.world(p));
    let vec = |v: Vec3| to.body_vec(crate::view::place_vec(&from.frame, v));
    let frame = |f: &Frame| Frame::from_axes(point(f.origin()), vec(*f.x()), vec(*f.z())).ok();
    Some(match curve {
        Curve::Line(l) => Curve::Line(Line::new(point(l.origin()), vec(l.direction())).ok()?),
        Curve::Circle(c) => {
            Curve::Circle(scx_curve::Circle::new(frame(c.frame())?, c.radius(), c.sense()).ok()?)
        }
        Curve::Ellipse(e) => Curve::Ellipse(
            scx_curve::Ellipse::new(frame(e.frame())?, e.semi_x(), e.semi_y(), e.sense()).ok()?,
        ),
        _ => return None,
    })
}

/// A box holding `curve` over `t`: a line's ends, or a conic's whole frame square.
fn curve_box(curve: &Curve, t: [f64; 2]) -> Option<scx_num::extent3d::Extent3d> {
    match curve {
        Curve::Line(l) => scx_num::extent3d::Extent3d::hull([l.point(t[0]), l.point(t[1])]),
        Curve::Circle(c) => square(c.frame(), c.radius().mm(), c.radius().mm()),
        Curve::Ellipse(e) => square(e.frame(), e.semi_x().mm(), e.semi_y().mm()),
        _ => None,
    }
}

fn square(f: &Frame, a: f64, b: f64) -> Option<scx_num::extent3d::Extent3d> {
    let (x, y) = (*f.x() * a, *f.y() * b);
    let o = f.origin();
    scx_num::extent3d::Extent3d::hull([o + x + y, o + x - y, o - x + y, o - x - y])
}

/// A 2-D tree over points: the spans' points, queried by each occluder's image box.
struct Tree {
    points: Vec<(Point2, usize)>,
}

/// A leaf holds at most this many points.
const LEAF: usize = 8;

impl Tree {
    fn new(mut points: Vec<(Point2, usize)>) -> Self {
        build(&mut points, 0);
        Self { points }
    }

    /// The tags of the points inside `b`.
    fn query(&self, b: &[f64; 4], out: &mut Vec<usize>) {
        query(&self.points, 0, b, out);
    }
}

fn coord(p: Point2, axis: usize) -> f64 {
    if axis == 0 { p.x } else { p.y }
}

fn build(points: &mut [(Point2, usize)], axis: usize) {
    if points.len() <= LEAF {
        return;
    }
    // The median stays at the middle; the halves either side of it are built on the other
    // axis (so the median is never moved again).
    let mid = points.len() / 2;
    points.select_nth_unstable_by(mid, |a, b| {
        coord(a.0, axis)
            .total_cmp(&coord(b.0, axis))
            .then(a.1.cmp(&b.1))
    });
    let (lo, rest) = points.split_at_mut(mid);
    build(lo, 1 - axis);
    if let Some((_, hi)) = rest.split_first_mut() {
        build(hi, 1 - axis);
    }
}

fn inside(b: &[f64; 4], p: Point2) -> bool {
    b[0] <= p.x && p.x <= b[2] && b[1] <= p.y && p.y <= b[3]
}

fn query(points: &[(Point2, usize)], axis: usize, b: &[f64; 4], out: &mut Vec<usize>) {
    if points.len() <= LEAF {
        out.extend(
            points
                .iter()
                .filter(|(p, _)| inside(b, *p))
                .map(|(_, tag)| *tag),
        );
        return;
    }
    let mid = points.len() / 2;
    let (lo, rest) = points.split_at(mid);
    let Some(((median, tag), hi)) = rest.split_first() else {
        return;
    };
    if inside(b, *median) {
        out.push(*tag);
    }
    let split = coord(*median, axis);
    let (low, high) = if axis == 0 {
        (b[0], b[2])
    } else {
        (b[1], b[3])
    };
    if low <= split {
        query(lo, 1 - axis, b, out);
    }
    if split <= high {
        query(hi, 1 - axis, b, out);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn the_tree_finds_exactly_the_points_in_a_box() {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        let pts: Vec<(Point2, usize)> = (0..2000)
            .map(|n| (Point2::new(next(), next()), n))
            .collect();
        let tree = Tree::new(pts.clone());
        for _ in 0..300 {
            let (x0, y0) = (next(), next());
            let b = [x0, y0, x0 + 0.3 * next(), y0 + 0.3 * next()];
            let mut got = Vec::new();
            tree.query(&b, &mut got);
            got.sort_unstable();
            let want: Vec<usize> = pts
                .iter()
                .filter(|(p, _)| inside(&b, *p))
                .map(|(_, n)| *n)
                .collect();
            assert_eq!(got, want);
        }
    }

    #[test]
    fn probes_spread_and_never_repeat() {
        let ps: Vec<f64> = (1..=PROBES).map(probe).collect();
        assert_eq!(&ps[..3], &[0.5, 0.25, 0.75]);
        let mut sorted = ps.clone();
        sorted.sort_by(f64::total_cmp);
        sorted.dedup();
        assert_eq!(sorted.len(), ps.len());
        assert!(ps.iter().all(|&p| p > 0.0 && p < 1.0));
    }
}
