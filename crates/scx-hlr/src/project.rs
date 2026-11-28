// Written for solidcorex. Apache-2.0.
//! A curve's image on the screen, exactly: a parallel view is an affine map, so a line's
//! image is a line and a conic arc's image is the conic arc of the projected control points
//! with the same weights (research 74 §37).
//!
//! Each piece carries the map from its image's Bézier parameter `s` in `[0, 1]` back to the
//! curve's own parameter `t`, so a crossing found on the screen splits the curve where it
//! is:
//!
//! - a line: `t = t0 + s (t1 - t0)`;
//! - a circle or ellipse arc of half-sweep `h` about `m` (at most a quarter turn, so the
//!   control triangle is well shaped): the rational quadratic of weight `cos h`, for which
//!   `tan((t - m) / 2) = tan(h / 2) (2 s - 1)`;
//! - a circle or ellipse whose plane holds the direction of sight (seen edge-on, within
//!   [`crate::PARALLEL`]): its image is a segment the curve runs along and back, so it is cut
//!   at its two turning points and each part is a [`Curve2::Line`], `s` the distance along
//!   it: with the image `c + R cos(t - φ) L`, `s = (cos(t - φ) - cos(t0 - φ)) /
//!   (cos(t1 - φ) - cos(t0 - φ))`.

use core::f64::consts::{FRAC_PI_2, PI};

use scx_curve::Curve;
use scx_num::fmath;
use scx_num::frame::Frame;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::Curve2;

use crate::tolerance::PARALLEL;
use crate::view::View;

/// A body's view: its local view (the world view's axes in the body's coordinates) and the
/// screen and depth of its frame's origin, measured from the scene's centre.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lens {
    pub(crate) frame: Frame,
    pub(crate) local: View,
    offset: [f64; 3],
}

impl Lens {
    /// The lens of a body placed by `frame`, with screen coordinates measured from
    /// `origin`; `None` when the view cannot be carried into the frame.
    pub(crate) fn new(view: &View, frame: Frame, origin: Point3) -> Option<Self> {
        let local = view.in_frame(&frame)?;
        let o = frame.origin() - origin;
        Some(Self {
            frame,
            local,
            offset: [o.dot(*view.x()), o.dot(*view.y()), o.dot(*view.d())],
        })
    }

    /// The screen point and depth of the body point `p`.
    pub(crate) fn image(&self, p: Point3) -> (Point2, f64) {
        let r = p.to_vec();
        (
            Point2::new(
                self.offset[0] + r.dot(*self.local.x()),
                self.offset[1] + r.dot(*self.local.y()),
            ),
            self.offset[2] + r.dot(*self.local.d()),
        )
    }

    /// The world point of the body point `p`.
    pub(crate) fn world(&self, p: Point3) -> Point3 {
        crate::view::place_point(&self.frame, p)
    }

    /// The body point of the world point `p`.
    pub(crate) fn body(&self, p: Point3) -> Point3 {
        let r = p - self.frame.origin();
        Point3::new(
            r.dot(*self.frame.x()),
            r.dot(*self.frame.y()),
            r.dot(*self.frame.z()),
        )
    }

    /// The body direction of the world direction `v`.
    pub(crate) fn body_vec(&self, v: Vec3) -> Vec3 {
        Vec3::new(
            v.dot(*self.frame.x()),
            v.dot(*self.frame.y()),
            v.dot(*self.frame.z()),
        )
    }
}

/// How an image piece's parameter maps back to the curve's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Map {
    /// `t = t0 + s (t1 - t0)`.
    Linear { t0: f64, t1: f64 },
    /// `t = m + 2 atan(k (2 s - 1))`, `k = tan(h / 2)`.
    Arc { m: f64, k: f64 },
    /// `t = base + dir acos(g0 + s (g1 - g0))`.
    EdgeOn {
        base: f64,
        dir: f64,
        g0: f64,
        g1: f64,
    },
}

impl Map {
    /// The curve's parameter at the image's `s`.
    pub(crate) fn t(&self, s: f64) -> f64 {
        let s = s.clamp(0.0, 1.0);
        match *self {
            Self::Linear { t0, t1 } => t0 + s * (t1 - t0),
            Self::Arc { m, k } => m + 2.0 * fmath::atan(k * (2.0 * s - 1.0)),
            Self::EdgeOn { base, dir, g0, g1 } => {
                base + dir * fmath::acos((g0 + s * (g1 - g0)).clamp(-1.0, 1.0))
            }
        }
    }
}

/// One piece of a curve's image.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Piece {
    /// The image.
    pub(crate) image: Curve2,
    /// Back to the curve's parameter.
    pub(crate) map: Map,
    /// The curve's parameters the piece covers.
    pub(crate) t: [f64; 2],
    /// The image's box `[x_min, y_min, x_max, y_max]`, from its control points (a rational
    /// Bézier of positive weights lies in their hull).
    pub(crate) bounds: [f64; 4],
    /// The depths of the control points, least and greatest.
    pub(crate) depth: [f64; 2],
}

/// Why a curve has no exact image here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoImage {
    /// A NURBS curve, parabola or hyperbola: waits on step 5's planar widening.
    Pending,
    /// The curve runs along the direction of sight: its image is a point.
    Point,
}

/// The image pieces of `curve` over `[t0, t1]` under `lens`.
pub(crate) fn pieces(curve: &Curve, t0: f64, t1: f64, lens: &Lens) -> Result<Vec<Piece>, NoImage> {
    match curve {
        Curve::Line(l) => line(l.point(t0), l.point(t1), t0, t1, lens),
        Curve::Circle(c) => {
            let r = c.radius().mm();
            Ok(conic(
                c.frame(),
                r,
                r * c.sense().sign(),
                curve,
                t0,
                t1,
                lens,
            ))
        }
        Curve::Ellipse(e) => Ok(conic(
            e.frame(),
            e.semi_x().mm(),
            e.semi_y().mm() * e.sense().sign(),
            curve,
            t0,
            t1,
            lens,
        )),
        _ => Err(NoImage::Pending),
    }
}

fn line(a: Point3, b: Point3, t0: f64, t1: f64, lens: &Lens) -> Result<Vec<Piece>, NoImage> {
    let (pa, da) = lens.image(a);
    let (pb, db) = lens.image(b);
    // Seen end on, within rounding of its length: a point, not a line.
    if (pb - pa).length() <= PARALLEL * (b - a).length() {
        return Err(NoImage::Point);
    }
    Ok(vec![Piece {
        image: Curve2::Line {
            start: flush(pa),
            end: flush(pb),
        },
        map: Map::Linear { t0, t1 },
        t: [t0, t1],
        bounds: hull(&[pa, pb]),
        depth: [da.min(db), da.max(db)],
    }])
}

/// The pieces of the conic `o + a cos t x + b sin t y` (the frame's axes) over `[t0, t1]`.
fn conic(
    frame: &Frame,
    a: f64,
    b: f64,
    curve: &Curve,
    t0: f64,
    t1: f64,
    lens: &Lens,
) -> Vec<Piece> {
    let (x, y) = (*frame.x(), *frame.y());
    let o = frame.origin();
    let d = *lens.local.d();
    // Seen edge on: the plane holds the direction of sight.
    if frame.z().dot(d).abs() <= PARALLEL {
        return edge_on(frame, a, b, curve, t0, t1, lens);
    }
    let n = ((t1 - t0) / FRAC_PI_2).ceil().max(1.0);
    // `n` is a small whole number: at most four pieces a turn over a finite range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = n as usize;
    let step = (t1 - t0) / n;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let lo = if i == 0 { t0 } else { t0 + (i as f64) * step };
        let hi = if i + 1 == count {
            t1
        } else {
            t0 + ((i + 1) as f64) * step
        };
        let (h, m) = (0.5 * (hi - lo), 0.5 * (lo + hi));
        let w = fmath::cos(h);
        let apex = o + (x * (a * fmath::cos(m)) + y * (b * fmath::sin(m))) * (1.0 / w);
        let (ps, ds) = lens.image(curve.point(lo));
        let (pe, de) = lens.image(curve.point(hi));
        let (pa, dapex) = lens.image(apex);
        out.push(Piece {
            image: Curve2::Conic {
                start: flush(ps),
                apex: flush(pa),
                end: flush(pe),
                weight: w,
            },
            map: Map::Arc {
                m,
                k: fmath::tan(0.5 * h),
            },
            t: [lo, hi],
            bounds: hull(&[ps, pa, pe]),
            depth: [ds.min(de).min(dapex), ds.max(de).max(dapex)],
        });
    }
    out
}

/// The pieces of a conic seen edge-on: lines between its turning points.
fn edge_on(
    frame: &Frame,
    a: f64,
    b: f64,
    curve: &Curve,
    t0: f64,
    t1: f64,
    lens: &Lens,
) -> Vec<Piece> {
    let (lx, ly) = (lens.local.x(), lens.local.y());
    // The image's direction: the larger of the axes' images.
    let ax = (*frame.x()) * a;
    let ay = (*frame.y()) * b;
    let img = |v: Vec3| [v.dot(*lx), v.dot(*ly)];
    let (ix, iy) = (img(ax), img(ay));
    let len = |p: [f64; 2]| fmath::hypot(p[0], p[1]);
    let dir = if len(ix) >= len(iy) { ix } else { iy };
    let dl = len(dir);
    // `f(t) = A cos t + B sin t` along the image direction, `R cos(t - φ)`.
    let ca = (ix[0] * dir[0] + ix[1] * dir[1]) / dl;
    let cb = (iy[0] * dir[0] + iy[1] * dir[1]) / dl;
    let phi = fmath::atan2(cb, ca);
    // The turning points `φ + jπ` inside the range.
    let mut cuts = vec![t0];
    let first = ((t0 - phi) / PI).floor() + 1.0;
    let mut j = first;
    while phi + j * PI < t1 {
        let c = phi + j * PI;
        if c > t0 {
            cuts.push(c);
        }
        j += 1.0;
    }
    cuts.push(t1);
    let mut out = Vec::with_capacity(cuts.len() - 1);
    for w in cuts.windows(2) {
        let &[lo, hi] = w else {
            continue;
        };
        if hi <= lo {
            continue;
        }
        let pl = curve.point(lo);
        let ph = curve.point(hi);
        let (ps, ds) = lens.image(pl);
        let (pe, de) = lens.image(ph);
        if (pe - ps).length() <= PARALLEL * (ph - pl).length() {
            continue;
        }
        // Which half turn of `t - φ` the piece is in, and the branch of `acos` there.
        let half = ((0.5 * (lo + hi) - phi) / PI).floor();
        let (base, sign) = if half.rem_euclid(2.0) == 0.0 {
            (phi + half * PI, 1.0)
        } else {
            (phi + (half + 1.0) * PI, -1.0)
        };
        let g = |t: f64| fmath::cos(t - phi);
        out.push(Piece {
            image: Curve2::Line {
                start: flush(ps),
                end: flush(pe),
            },
            map: Map::EdgeOn {
                base,
                dir: sign,
                g0: g(lo),
                g1: g(hi),
            },
            t: [lo, hi],
            bounds: hull(&[ps, pe]),
            depth: [ds.min(de), ds.max(de)],
        });
    }
    out
}

/// A coordinate below `1e-100` in size is rounding noise around 0, which `scx_planar`'s
/// exact predicates refuse: it is written as 0 (a move of under `1e-100` mm).
fn flush(p: Point2) -> Point2 {
    let f = |v: f64| if v.abs() < 1e-100 { 0.0 } else { v };
    Point2::new(f(p.x), f(p.y))
}

fn hull(ps: &[Point2]) -> [f64; 4] {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in ps {
        b[0] = b[0].min(p.x);
        b[1] = b[1].min(p.y);
        b[2] = b[2].max(p.x);
        b[3] = b[3].max(p.y);
    }
    b
}

/// Whether two boxes meet, each grown by `pad`.
pub(crate) fn meet(a: &[f64; 4], b: &[f64; 4], pad: f64) -> bool {
    a[0] <= b[2] + pad && b[0] <= a[2] + pad && a[1] <= b[3] + pad && b[1] <= a[3] + pad
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;
    use core::f64::consts::TAU;
    use scx_curve::Circle;
    use scx_num::frame::Sense;
    use scx_num::units::Length;
    use scx_num::vector3d::Vec3;

    fn bezier(c: &Curve2, s: f64) -> Point2 {
        match *c {
            Curve2::Line { start, end } => start + (end - start) * s,
            Curve2::Conic {
                start,
                apex,
                end,
                weight,
            } => {
                let (b0, b1, b2) = ((1.0 - s) * (1.0 - s), 2.0 * s * (1.0 - s) * weight, s * s);
                let w = b0 + b1 + b2;
                Point2::new(
                    (b0 * start.x + b1 * apex.x + b2 * end.x) / w,
                    (b0 * start.y + b1 * apex.y + b2 * end.y) / w,
                )
            }
            _ => unreachable!(),
        }
    }

    fn check(view: View, curve: &Curve, t0: f64, t1: f64) {
        let lens = Lens::new(&view, Frame::WORLD, Point3::new(0.0, 0.0, 0.0)).unwrap();
        let ps = pieces(curve, t0, t1, &lens).unwrap();
        assert!(!ps.is_empty());
        for p in &ps {
            for i in 0..=16 {
                let s = f64::from(i) / 16.0;
                let t = p.map.t(s);
                assert!(t >= p.t[0] - 1e-12 && t <= p.t[1] + 1e-12);
                let want = lens.image(curve.point(t)).0;
                let got = bezier(&p.image, s);
                assert!((want - got).length() < 1e-12, "{want:?} {got:?} at s {s}");
            }
        }
    }

    #[test]
    fn an_arc_image_maps_back_to_its_angle() {
        let f = Frame::new(
            Point3::new(1.0, 2.0, 3.0),
            Vec3::new(0.2, -0.4, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap();
        let c = Curve::Circle(Circle::new(f, Length::from_mm(2.5), Sense::Reversed).unwrap());
        let view = View::new(Vec3::new(1.0, -2.0, 0.5), Vec3::new(0.0, 0.0, 1.0)).unwrap();
        check(view, &c, 0.0, TAU);
        check(view, &c, 1.0, 2.2);
    }

    #[test]
    fn an_edge_on_circle_maps_back_along_its_segment() {
        let f = Frame::new(
            Point3::new(1.0, 2.0, 3.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.3, 0.0),
        )
        .unwrap();
        let c = Curve::Circle(Circle::new(f, Length::from_mm(2.0), Sense::Forward).unwrap());
        let view = View::new(Vec3::new(1.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)).unwrap();
        check(view, &c, 0.0, TAU);
        check(view, &c, -1.0, 4.0);
    }
}
