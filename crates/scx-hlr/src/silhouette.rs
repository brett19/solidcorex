// Written for solidcorex. Apache-2.0.
//! A body's silhouettes under a view, face by face, in closed form.
//!
//! **The curves.** On a face whose (recognised) surface is analytic, the silhouette
//! `n · d = 0` is exact:
//!
//! - a plane has none;
//! - a cylinder or cone of axis `a`, radius `r` at `v = 0` and slope `k` (`0` for a
//!   cylinder): with `d = da a + dρ`, the generators at `ρ̂ = c û ± √(1−c²) ŵ`, where
//!   `c = k da / |dρ|`, `û = unit(dρ)` and `ŵ = a × û`; none when `|c| > 1` or the view is
//!   along the axis ([`crate::PARALLEL`]);
//! - a sphere: the great circle normal to `d`;
//! - a torus: [`crate::TorusContour`]'s two branches, or four circles (viewed across its
//!   axis) or its two equators (viewed along it).
//!
//! **The face's part of each curve** is found without sampling. The curve can enter or
//! leave the face only where it crosses one of the face's boundary edges, so each crossing
//! is found in closed form (the boundary edge against a plane that cuts the surface in the
//! silhouette, or, on a torus, the meridian or parallel the edge is), the curve is split
//! there, and one point inside each span is classified against the face. A seam (an edge the
//! face uses twice) is not a boundary of the face's region and is not a split. An extra split
//! changes nothing; only a missed crossing could, and none is missed: every crossing is a
//! root of a closed form or of `scx_surface::intersect_curve`, which is complete.

use core::f64::consts::{PI, TAU};
use std::collections::BTreeMap;

use scx_brep::op::{Cx, Event, Trace};
use scx_brep::{Brep, EdgeId, EdgeShape, Face, FaceId, UvSide};
use scx_curve::{Circle, Curve, Line};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::invert::Inversion;
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_surface::{CurveSurfaceHit, Plane, Surface, SurfaceError, intersect_curve, recognize};

use crate::contour::{self, TorusContour};
use crate::error::HlrError;
use crate::tolerance::{PARALLEL, ROUNDING, Tolerances};
use crate::view::{View, place_point, place_vec};

/// A body's silhouettes, and the faces whose silhouettes are not found yet.
#[derive(Debug, Clone, PartialEq)]
pub struct Silhouettes {
    /// The silhouette curves, face by face in id order.
    pub curves: Vec<Silhouette>,
    /// The faces left out, each with the reason. What they might hide is not known.
    pub unsupported: Vec<Unsupported>,
}

/// A face whose silhouette is not computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Unsupported {
    /// The face.
    pub face: FaceId,
    /// Why.
    pub why: Why,
}

/// Why a face's silhouette is not computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Why {
    /// A NURBS surface that is no analytic surface within the recognition accuracy: its
    /// silhouette is a curve traced on the surface (research 74 HL4, on step 5's marcher).
    Freeform,
    /// The face reaches infinity or has no box, so its curves have no finite extent.
    Unbounded,
}

/// One silhouette curve of one face.
#[derive(Debug, Clone, PartialEq)]
pub struct Silhouette {
    /// The face.
    pub face: FaceId,
    /// The whole curve on the face's surface: an infinite line is the generator, a closed
    /// curve is parametrised over `[0, 2π]`.
    pub curve: SilhouetteCurve,
    /// Whether the curve is the face's own surface's silhouette or a recognised surface's.
    pub accuracy: Exactness,
    /// The part of the curve on the face.
    pub on_face: OnFace,
}

/// A silhouette curve, in closed form.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SilhouetteCurve {
    /// A cylinder's or cone's generator, `t` the axial distance from the surface's origin.
    Line(Line),
    /// A sphere's great circle, or a torus's equator, top or bottom circle or meridian.
    Circle(Circle),
    /// One branch of a torus's contour.
    Torus(TorusContour),
}

impl SilhouetteCurve {
    /// The point at `t`.
    #[must_use]
    pub fn point(&self, t: f64) -> Point3 {
        match self {
            Self::Line(l) => l.point(t),
            Self::Circle(c) => c.point(t),
            Self::Torus(c) => c.point(t),
        }
    }
}

/// How exact a silhouette is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Exactness {
    /// The face's own analytic surface's silhouette, exact up to rounding.
    Exact,
    /// The silhouette of the analytic surface a NURBS face was recognised as, within
    /// `deviation` of the face's surface.
    Recognized {
        /// How far the recognised surface may stand from the face's.
        deviation: Length,
    },
}

/// The part of a silhouette curve that lies on its face.
#[derive(Debug, Clone, PartialEq)]
pub enum OnFace {
    /// The pieces, as parameter intervals in increasing order, proved. A piece of a closed
    /// curve that wraps past `2π` runs past it (`[5.9, 6.5]`); the whole curve is
    /// `[0, 2π]`. Never empty: a candidate wholly off the face is not listed.
    Proved(Vec<Extent1d>),
    /// The pieces are not decided; the curve over `over` holds them.
    Undecided {
        /// The parameters that hold the face's part of the curve.
        over: Extent1d,
        /// Why.
        why: Undecided,
    },
}

/// Why a silhouette's pieces on its face are not decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Undecided {
    /// A boundary edge is a NURBS curve, whose crossings with the silhouette need step 5's
    /// NURBS curve-surface intersection.
    NurbsEdge {
        /// The edge.
        edge: EdgeId,
    },
    /// A torus face's boundary edge is neither a meridian nor a parallel, whose crossings
    /// with the contour need a general curve-surface intersection.
    TorusEdge {
        /// The edge.
        edge: EdgeId,
    },
    /// A point on the curve could not be placed inside or outside the face
    /// (`Face::contains_uv` could not tell).
    Classification,
}

/// The silhouettes of `body`, in its own coordinates, under `view`.
///
/// Emits the phase `"hlr.silhouettes"` and one progress event per face. Charges one unit of
/// the budget per face and one per span classified, besides what a NURBS face's recognition
/// and inversion charge.
///
/// # Errors
///
/// [`HlrError::Exhausted`] and [`HlrError::Stopped`]; [`HlrError::Surface`] when a surface
/// refuses a query; [`HlrError::Curve`] when a curve's coordinates overflow.
pub fn silhouettes(body: &Brep, view: &View, cx: &mut Cx<'_>) -> Result<Silhouettes, HlrError> {
    let mut budget = cx.budget();
    of_body(body, view, &mut budget, cx.stop, cx.trace)
}

/// [`silhouettes`] on a budget shared with other bodies.
pub(crate) fn of_body(
    body: &Brep,
    view: &View,
    budget: &mut Budget,
    stop: &Stop,
    trace: &dyn Trace,
) -> Result<Silhouettes, HlrError> {
    trace.event(Event::Phase("hlr.silhouettes"));
    let total = body.faces().len() as u64;
    let mut out = Silhouettes {
        curves: Vec::new(),
        unsupported: Vec::new(),
    };
    for (done, face) in (1u64..).zip(body.faces()) {
        stop.check()?;
        budget.charge(1)?;
        let mut run = Run {
            face,
            res: body.resolution(),
            budget,
            stop,
        };
        match run.face(view)? {
            Ok(mut curves) => out.curves.append(&mut curves),
            Err(why) => out.unsupported.push(Unsupported {
                face: face.id(),
                why,
            }),
        }
        trace.event(Event::Progress { done, total });
    }
    Ok(out)
}

impl Silhouette {
    /// The same silhouette on the body placed by `frame`.
    pub(crate) fn placed(self, frame: &Frame) -> Result<Self, HlrError> {
        let face = self.face;
        let curve = match self.curve {
            SilhouetteCurve::Line(l) => SilhouetteCurve::Line(
                Line::new(
                    place_point(frame, l.origin()),
                    place_vec(frame, l.direction()),
                )
                .map_err(|error| HlrError::Curve { face, error })?,
            ),
            SilhouetteCurve::Circle(c) => {
                let f = c.frame();
                let placed = Frame::from_axes(
                    place_point(frame, f.origin()),
                    place_vec(frame, *f.x()),
                    place_vec(frame, *f.z()),
                )
                .map_err(|_| HlrError::Defect {
                    reason: "a placed circle's axes give no frame",
                })?;
                SilhouetteCurve::Circle(
                    Circle::new(placed, c.radius(), c.sense())
                        .map_err(|error| HlrError::Curve { face, error })?,
                )
            }
            SilhouetteCurve::Torus(t) => {
                SilhouetteCurve::Torus(t.placed(frame).ok_or(HlrError::Defect {
                    reason: "a placed torus's axes give no frame",
                })?)
            }
        };
        Ok(Self { curve, ..self })
    }
}

/// One face's work.
struct Run<'a, 'b> {
    face: Face<'a>,
    res: Resolution,
    budget: &'b mut Budget,
    stop: &'b Stop,
}

/// A boundary edge of a face (used once by it) with a curve.
pub(crate) struct Rim<'a> {
    pub(crate) edge: EdgeId,
    pub(crate) curve: &'a Curve,
    pub(crate) interval: Extent1d,
    pub(crate) near: f64,
}

/// A silhouette before it is trimmed to its face: the curve, the parameters to trim over,
/// and how its boundary crossings are found.
pub(crate) struct Candidate {
    pub(crate) curve: SilhouetteCurve,
    pub(crate) range: Extent1d,
    pub(crate) closed: bool,
    pub(crate) splitter: Splitter,
}

/// How a candidate's crossings with the boundary edges are found.
pub(crate) enum Splitter {
    /// A plane cuts the surface in the candidate (and perhaps other curves, told apart by
    /// distance): each boundary edge's crossings with it near the candidate.
    Plane(Frame),
    /// The crossings are closed forms (a torus's), already found, or an edge that has none.
    Found(Result<Vec<f64>, Undecided>),
}

/// A silhouette's parameter at a point, and the point's distance from it.
fn locate(curve: &SilhouetteCurve, p: Point3) -> (f64, f64) {
    match curve {
        SilhouetteCurve::Line(l) => {
            let dir = l.direction();
            let t = (p - l.origin()).dot(dir) / dir.length_squared();
            (t, p.distance(l.point(t)))
        }
        SilhouetteCurve::Circle(c) => {
            let f = c.frame();
            let r = p - f.origin();
            let (x, y, z) = (r.dot(*f.x()), r.dot(*f.y()), r.dot(*f.z()));
            let t = fmath::atan2(y * c.sense().sign(), x).rem_euclid(TAU);
            (t, fmath::hypot(z, fmath::hypot(x, y) - c.radius().mm()))
        }
        // A torus contour is never split by a plane.
        SilhouetteCurve::Torus(c) => {
            let t = c.angle_of(p);
            (t, p.distance(c.point(t)))
        }
    }
}

impl Run<'_, '_> {
    /// The face's silhouettes, or why it has none computed.
    fn face(&mut self, view: &View) -> Result<Result<Vec<Silhouette>, Why>, HlrError> {
        let face = self.face;
        let id = face.id();
        if matches!(face.surface(), Surface::Plane(_)) {
            return Ok(Ok(Vec::new()));
        }
        let (Some(uv), Some(fbox)) = (face.uv_box(), face.bounding_box()) else {
            return Ok(Err(Why::Unbounded));
        };
        let tol = Tolerances::of(&fbox, self.res).ok_or(HlrError::Defect {
            reason: "a resolution's approximation accuracy is not an accuracy",
        })?;
        let (surface, accuracy) = match face.surface() {
            Surface::Nurbs(n) => {
                // The face's uv box is padded by its pcurves' rounding; recognition is
                // asked only over the surface's own domain.
                let Some(over) = uv.intersect(n.domain()) else {
                    return Ok(Err(Why::Freeform));
                };
                let found = recognize(
                    face.surface(),
                    &over,
                    tol.recognition(),
                    self.budget,
                    self.stop,
                )
                .map_err(|e| HlrError::surface(id, e))?;
                match found {
                    Some(r) => (
                        r.surface,
                        Exactness::Recognized {
                            deviation: r.deviation,
                        },
                    ),
                    None => return Ok(Err(Why::Freeform)),
                }
            }
            s => (s.clone(), Exactness::Exact),
        };
        let tol = match accuracy {
            Exactness::Exact => tol,
            Exactness::Recognized { deviation } => tol.with_deviation(deviation.mm()),
        };
        let rims = rims(face, &tol);
        let candidates = match &surface {
            Surface::Plane(_) => Vec::new(),
            Surface::Cylinder(c) => {
                let f = c.frame();
                ruled(id, f.origin(), f.z(), c.radius().mm(), 0.0, view, &fbox)?
            }
            Surface::Cone(c) => {
                let f = c.frame();
                let a = f.z() * c.sense().sign();
                let axis = a.normalized().ok_or(HlrError::Defect {
                    reason: "a cone's axis has no direction",
                })?;
                let k = fmath::tan(c.half_angle());
                ruled(id, f.origin(), axis, c.radius().mm(), k, view, &fbox)?
            }
            Surface::Sphere(s) => sphere(id, s.frame().origin(), s.radius().mm(), view)?,
            Surface::Torus(t) => contour::candidates(id, t, view, &rims)?,
            // A NURBS surface `recognize` left as one, or a kind added later.
            _ => return Ok(Err(Why::Freeform)),
        };
        let mut out = Vec::with_capacity(candidates.len());
        for cand in candidates {
            let on_face = self.trim(&cand, &rims, &uv, &fbox, tol.near(None))?;
            // A candidate wholly off the face is not one of its silhouettes.
            if matches!(&on_face, OnFace::Proved(p) if p.is_empty()) {
                continue;
            }
            out.push(Silhouette {
                face: id,
                curve: cand.curve,
                accuracy,
                on_face,
            });
        }
        Ok(Ok(out))
    }

    /// The part of `cand` on the face. A point of `cand` on the face lies within `near` of
    /// the face's surface; one farther off is off the face, even where the surface's foot
    /// lies in its domain (a NURBS patch's foot is clamped to its edge).
    fn trim(
        &mut self,
        cand: &Candidate,
        rims: &[Rim<'_>],
        uv: &Extent2d,
        fbox: &Extent3d,
        near: f64,
    ) -> Result<OnFace, HlrError> {
        let splits = match &cand.splitter {
            Splitter::Found(found) => found.clone(),
            Splitter::Plane(frame) => self.plane_splits(&cand.curve, frame, rims, fbox)?,
        };
        let splits = match splits {
            Ok(s) => s,
            Err(why) => {
                return Ok(OnFace::Undecided {
                    over: cand.range,
                    why,
                });
            }
        };
        let face = self.face;
        let id = face.id();
        let mut keep = |t: f64| -> Result<Option<bool>, HlrError> {
            self.budget.charge(1)?;
            let p = cand.curve.point(t);
            let inv = face
                .surface()
                .invert(p, None, Some(uv), self.budget, self.stop)
                .map_err(|e| HlrError::surface(id, e))?;
            Ok(match inv {
                Inversion::Outside(_) => Some(false),
                Inversion::Inside(foot) if foot.distance.is_nan() => None,
                Inversion::Inside(foot) if foot.distance > near => Some(false),
                Inversion::Inside(foot) => match face.contains_uv(foot.at) {
                    Some(UvSide::Inside | UvSide::Near) => Some(true),
                    Some(UvSide::Outside) => Some(false),
                    None => None,
                },
            })
        };
        let pieces = if cand.closed {
            closed_pieces(splits, &mut keep)?
        } else {
            open_pieces(cand.range, splits, &mut keep)?
        };
        Ok(match pieces {
            Some(p) => OnFace::Proved(p),
            None => OnFace::Undecided {
                over: cand.range,
                why: Undecided::Classification,
            },
        })
    }

    /// The parameters on `curve` where the boundary edges cross the plane of `frame` near
    /// it.
    fn plane_splits(
        &mut self,
        curve: &SilhouetteCurve,
        frame: &Frame,
        rims: &[Rim<'_>],
        fbox: &Extent3d,
    ) -> Result<Result<Vec<f64>, Undecided>, HlrError> {
        let id = self.face.id();
        let plane = Surface::Plane(Plane::new(*frame));
        // The plane's uv box: wide enough to hold every point of the face, whose boundary
        // edges lie in its box.
        let half = 2.0 * fbox.max_distance_to_point(frame.origin()) + self.res.zone().mm();
        let se = Extent2d::from_bounds(-half, -half, half, half).ok_or(HlrError::Defect {
            reason: "a face box's span is not finite",
        })?;
        let mut splits = Vec::new();
        for rim in rims {
            let hits = match intersect_curve(
                rim.curve,
                &rim.interval,
                &plane,
                &se,
                self.res,
                self.budget,
                self.stop,
            ) {
                Ok(h) => h,
                Err(SurfaceError::NurbsIntersection | SurfaceError::Undecided { .. }) => {
                    return Ok(Err(Undecided::NurbsEdge { edge: rim.edge }));
                }
                Err(e) => return Err(HlrError::surface(id, e)),
            };
            let mut at = |p: Point3| {
                let (t, dist) = locate(curve, p);
                if dist <= rim.near {
                    splits.push(t);
                }
            };
            for hit in hits {
                match hit {
                    CurveSurfaceHit::Point { point, .. } => at(point),
                    CurveSurfaceHit::Overlap { t } => {
                        at(rim.curve.point(t.min()));
                        at(rim.curve.point(t.max()));
                    }
                }
            }
        }
        Ok(Ok(splits))
    }
}

/// The face's boundary edges with curves: those it uses once, in id order.
pub(crate) fn rims<'a>(face: Face<'a>, tol: &Tolerances) -> Vec<Rim<'a>> {
    let mut count: BTreeMap<EdgeId, (usize, scx_brep::Edge<'a>)> = BTreeMap::new();
    for u in face.uses() {
        let e = u.edge();
        count.entry(e.id()).or_insert((0, e)).0 += 1;
    }
    count
        .into_values()
        .filter(|(n, _)| *n == 1)
        .filter_map(|(_, e)| match e.shape() {
            EdgeShape::Curve { curve, interval } => Some(Rim {
                edge: e.id(),
                curve,
                interval: *interval,
                near: tol.near(e.accuracy().map(|r| r.acc)),
            }),
            EdgeShape::Collapsed => None,
        })
        .collect()
}

/// The silhouette generators of a cylinder or cone of origin `o`, axis `a`, radius `r` at
/// `v = 0` and slope `k`, over the axial span of `fbox`.
fn ruled(
    face: FaceId,
    o: Point3,
    a: UnitVec3,
    r: f64,
    k: f64,
    view: &View,
    fbox: &Extent3d,
) -> Result<Vec<Candidate>, HlrError> {
    let d = *view.d();
    let da = d.dot(*a);
    let d_rho = d - *a * da;
    let Some((m, u)) = d_rho.length_and_unit() else {
        return Ok(Vec::new());
    };
    if m <= PARALLEL {
        return Ok(Vec::new());
    }
    let c = k * da / m;
    // A NaN `c` (no finite slope) has no silhouette either.
    if c.is_nan() || c.abs() > 1.0 {
        return Ok(Vec::new());
    }
    let w = a.cross(*u);
    let s = (1.0 - c * c).sqrt();
    let dirs: Vec<Vec3> = if s == 0.0 {
        vec![*u * c]
    } else {
        vec![*u * c + w * s, *u * c - w * s]
    };
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in fbox.corners() {
        let v = (p - o).dot(*a);
        lo = lo.min(v);
        hi = hi.max(v);
    }
    // A cone's surface is the one nappe where the radius `r + k v` is not negative: the
    // generator ends at the apex `v = -r / k`, never runs on through it.
    if k > 0.0 {
        lo = lo.max(-r / k);
    } else if k < 0.0 {
        hi = hi.min(-r / k);
    }
    let Some(range) = Extent1d::new(lo, hi) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(dirs.len());
    for rho in dirs {
        let rho = rho.normalized().ok_or(HlrError::Defect {
            reason: "a generator's radial direction has no direction",
        })?;
        let line = Line::new(o + *rho * r, *a + *rho * k)
            .map_err(|error| HlrError::Curve { face, error })?;
        let normal = a.cross(*rho);
        let frame = Frame::new(o, normal, *rho).ok_or(HlrError::Defect {
            reason: "a generator's splitting plane has no frame",
        })?;
        out.push(Candidate {
            curve: SilhouetteCurve::Line(line),
            range,
            closed: false,
            splitter: Splitter::Plane(frame),
        });
    }
    Ok(out)
}

/// A sphere's silhouette: its great circle normal to the view, split by that circle's plane.
fn sphere(face: FaceId, centre: Point3, r: f64, view: &View) -> Result<Vec<Candidate>, HlrError> {
    let frame = Frame::new(centre, *view.d(), *view.x()).ok_or(HlrError::Defect {
        reason: "a view's axes give no frame",
    })?;
    let circle = Circle::new(frame, Length::from_mm(r), Sense::Forward)
        .map_err(|error| HlrError::Curve { face, error })?;
    Ok(vec![Candidate {
        curve: SilhouetteCurve::Circle(circle),
        range: whole_turn()?,
        closed: true,
        splitter: Splitter::Plane(frame),
    }])
}

/// `[0, 2π]`.
pub(crate) fn whole_turn() -> Result<Extent1d, HlrError> {
    span(0.0, TAU)
}

/// `[a, b]`, for `a <= b` finite.
pub(crate) fn span(a: f64, b: f64) -> Result<Extent1d, HlrError> {
    Extent1d::new(a, b).ok_or(HlrError::Defect {
        reason: "a silhouette's span is not ordered and finite",
    })
}

/// The kept spans of an open curve over `range`, cut at `splits`; `None` when one cannot be
/// classified.
fn open_pieces(
    range: Extent1d,
    mut splits: Vec<f64>,
    keep: &mut impl FnMut(f64) -> Result<Option<bool>, HlrError>,
) -> Result<Option<Vec<Extent1d>>, HlrError> {
    let (lo, hi) = (range.min(), range.max());
    let eps = ROUNDING * lo.abs().max(hi.abs());
    splits.retain(|&s| lo + eps < s && s < hi - eps);
    let mut cuts = Vec::with_capacity(splits.len() + 2);
    cuts.push(lo);
    cuts.extend(settled(splits, eps));
    cuts.push(hi);
    kept(&cuts, keep)
}

/// `cuts` in increasing order, each within `eps` of the one before dropped: they are the
/// same crossing rounded twice (every ruling of a cone meets the splitting plane at the
/// apex, which also ends the generator), and the sliver between them is no piece.
fn settled(mut cuts: Vec<f64>, eps: f64) -> Vec<f64> {
    cuts.sort_by(f64::total_cmp);
    let mut out: Vec<f64> = Vec::with_capacity(cuts.len());
    for c in cuts {
        if out.last().is_none_or(|&l| c > l + eps) {
            out.push(c);
        }
    }
    out
}

/// The kept spans of a closed curve of period `2π`, cut at `splits`; `None` when one cannot
/// be classified.
fn closed_pieces(
    splits: Vec<f64>,
    keep: &mut impl FnMut(f64) -> Result<Option<bool>, HlrError>,
) -> Result<Option<Vec<Extent1d>>, HlrError> {
    let mut cuts = settled(
        splits.into_iter().map(|s| s.rem_euclid(TAU)).collect(),
        ROUNDING * TAU,
    );
    // The last and the first may be one crossing either side of `2π`.
    if let (Some(&first), Some(&last)) = (cuts.first(), cuts.last())
        && cuts.len() > 1
        && first + TAU <= last + ROUNDING * TAU
    {
        cuts.pop();
    }
    let Some(&first) = cuts.first() else {
        return Ok(match keep(PI)? {
            None => None,
            Some(true) => Some(vec![whole_turn()?]),
            Some(false) => Some(Vec::new()),
        });
    };
    cuts.push(first + TAU);
    let Some(mut pieces) = kept(&cuts, keep)? else {
        return Ok(None);
    };
    let end = first + TAU;
    if let [only] = pieces.as_slice()
        && only.min() == first
        && only.max() == end
    {
        return Ok(Some(vec![whole_turn()?]));
    }
    if pieces.len() > 1
        && pieces.first().is_some_and(|p| p.min() == first)
        && pieces.last().is_some_and(|p| p.max() == end)
    {
        let head = pieces.remove(0);
        if let Some(last) = pieces.last_mut() {
            *last = span(last.min(), head.max() + TAU)?;
        }
    }
    Ok(Some(pieces))
}

/// The spans between consecutive `cuts` whose midpoints `keep` keeps, adjacent ones merged.
fn kept(
    cuts: &[f64],
    keep: &mut impl FnMut(f64) -> Result<Option<bool>, HlrError>,
) -> Result<Option<Vec<Extent1d>>, HlrError> {
    let mut out: Vec<Extent1d> = Vec::new();
    for (&a, &b) in cuts.iter().zip(cuts.iter().skip(1)) {
        if b <= a {
            continue;
        }
        match keep(0.5 * (a + b))? {
            None => return Ok(None),
            Some(false) => {}
            Some(true) => match out.last_mut() {
                Some(last) if last.max() == a => *last = span(last.min(), b)?,
                _ => out.push(span(a, b)?),
            },
        }
    }
    Ok(Some(out))
}
