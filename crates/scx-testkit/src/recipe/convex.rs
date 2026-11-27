// Written for solidcorex. Apache-2.0.
//! Two convex blocks against each other, by clipping polygons with half-spaces: an
//! independent route to the volume and area of a union, difference or intersection of two
//! blocks at any frames.
//!
//! The route shares nothing with the closed forms of [`crate::solid`] or the membership
//! oracle of [`crate::csg`]: it clips each face of one block by the six planes of the other
//! (Sutherland-Hodgman), which leaves the part of that face inside the other block. The
//! parts, over both blocks, bound the intersection, so `V = (1/3) sum h_f A_f` over them
//! (`h_f` the plane's offset from the origin along its outward normal) and the area is the
//! sum of the parts' areas.
//!
//! **Condition.** No face of one block may lie in the plane of a face of the other
//! (a clipped face then counts on both sides); the recipes that use this module are in
//! general position, and [`overlap`] returns `None` when it finds two such planes.

use crate::analytic::Frame;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// A planar polygon with its outward unit normal.
#[derive(Clone, Debug)]
struct Face {
    poly: Vec<V3>,
    normal: V3,
}

impl Face {
    /// `n . x = offset` is the face's plane.
    fn offset(&self) -> f64 {
        self.poly.first().map_or(0.0, |&p| dot(self.normal, p))
    }

    fn area(&self) -> f64 {
        let Some(&p0) = self.poly.first() else {
            return 0.0;
        };
        let mut s = [0.0; 3];
        for w in self.poly.windows(2) {
            if let [a, b] = w {
                let c = cross(sub(*a, p0), sub(*b, p0));
                s = [s[0] + c[0], s[1] + c[1], s[2] + c[2]];
            }
        }
        0.5 * dot(s, s).sqrt()
    }
}

/// The part of `poly` with `n . x <= d` (Sutherland-Hodgman).
fn clip(poly: &[V3], n: V3, d: f64) -> Vec<V3> {
    let mut out = Vec::new();
    for (i, &p) in poly.iter().enumerate() {
        let q = poly[(i + 1) % poly.len()];
        let (fp, fq) = (dot(n, p) - d, dot(n, q) - d);
        if fp <= 0.0 {
            out.push(p);
        }
        if (fp < 0.0 && fq > 0.0) || (fp > 0.0 && fq < 0.0) {
            let t = fp / (fp - fq);
            out.push([
                p[0] + t * (q[0] - p[0]),
                p[1] + t * (q[1] - p[1]),
                p[2] + t * (q[2] - p[2]),
            ]);
        }
    }
    out
}

/// A block's boundary as six outward faces.
#[derive(Clone, Debug)]
pub struct Hull {
    faces: Vec<Face>,
}

impl Hull {
    /// The block with its corner at `frame.origin` and extents `size` along the axes.
    #[must_use]
    pub fn block(frame: &Frame, size: [f64; 3]) -> Hull {
        let axes = [frame.x, frame.y, frame.z];
        let at = |a: usize, side: f64, b: f64, c: f64| -> V3 {
            let (ib, ic) = ((a + 1) % 3, (a + 2) % 3);
            let mut p = frame.origin;
            for (axis, t) in [(a, side * size[a]), (ib, b * size[ib]), (ic, c * size[ic])] {
                p = [
                    p[0] + t * axes[axis][0],
                    p[1] + t * axes[axis][1],
                    p[2] + t * axes[axis][2],
                ];
            }
            p
        };
        let mut faces = Vec::new();
        for (a, axis) in axes.iter().enumerate() {
            for side in [0.0, 1.0] {
                let sign = if side == 0.0 { -1.0 } else { 1.0 };
                faces.push(Face {
                    poly: vec![
                        at(a, side, 0.0, 0.0),
                        at(a, side, 1.0, 0.0),
                        at(a, side, 1.0, 1.0),
                        at(a, side, 0.0, 1.0),
                    ],
                    normal: axis.map(|v| sign * v),
                });
            }
        }
        Hull { faces }
    }

    /// The enclosed volume.
    #[must_use]
    pub fn volume(&self) -> f64 {
        self.faces
            .iter()
            .map(|f| f.offset() * f.area())
            .sum::<f64>()
            / 3.0
    }

    /// The boundary's area.
    #[must_use]
    pub fn area(&self) -> f64 {
        self.faces.iter().map(Face::area).sum()
    }

    /// Each face of `self` cut down to the inside of `other`.
    fn inside(&self, other: &Hull) -> Vec<Face> {
        self.faces
            .iter()
            .map(|f| {
                let mut poly = f.poly.clone();
                for g in &other.faces {
                    if poly.is_empty() {
                        break;
                    }
                    poly = clip(&poly, g.normal, g.offset());
                }
                Face {
                    poly,
                    normal: f.normal,
                }
            })
            .collect()
    }
}

/// The overlap of two blocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Overlap {
    /// The volume of the intersection.
    pub volume: f64,
    /// The area of `a`'s boundary inside `b`.
    pub a_in_b: f64,
    /// The area of `b`'s boundary inside `a`.
    pub b_in_a: f64,
}

impl Overlap {
    /// Volume and area of `a` united with `b`.
    #[must_use]
    pub fn union(&self, a: &Hull, b: &Hull) -> (f64, f64) {
        (
            a.volume() + b.volume() - self.volume,
            a.area() - self.a_in_b + b.area() - self.b_in_a,
        )
    }

    /// Volume and area of `a` less `b`.
    #[must_use]
    pub fn difference(&self, a: &Hull) -> (f64, f64) {
        (
            a.volume() - self.volume,
            a.area() - self.a_in_b + self.b_in_a,
        )
    }

    /// Volume and area of `a` meet `b`.
    #[must_use]
    pub fn intersection(&self) -> (f64, f64) {
        (self.volume, self.a_in_b + self.b_in_a)
    }
}

/// The overlap of `a` and `b`, or `None` when a face of one lies in the plane of a face of
/// the other (to within a relative `1e-9`), where this route does not hold.
#[must_use]
pub fn overlap(a: &Hull, b: &Hull) -> Option<Overlap> {
    let scale = a
        .faces
        .iter()
        .chain(&b.faces)
        .map(|f| f.offset().abs())
        .fold(1.0, f64::max);
    for f in &a.faces {
        for g in &b.faces {
            let c = cross(f.normal, g.normal);
            if dot(c, c) < 1e-18 {
                let (same, opposite) = (f.offset() - g.offset(), f.offset() + g.offset());
                let gap = if dot(f.normal, g.normal) > 0.0 {
                    same
                } else {
                    opposite
                };
                if gap.abs() < 1e-9 * scale {
                    return None;
                }
            }
        }
    }
    let in_b = a.inside(b);
    let in_a = b.inside(a);
    let volume = in_b
        .iter()
        .chain(&in_a)
        .map(|f| f.offset() * f.area())
        .sum::<f64>()
        / 3.0;
    Some(Overlap {
        volume,
        a_in_b: in_b.iter().map(Face::area).sum(),
        b_in_a: in_a.iter().map(Face::area).sum(),
    })
}
