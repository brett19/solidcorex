// Written for solidcorex. Apache-2.0.
//! Seeded generators of well- and ill-conditioned inputs, from [`scx_num::hash::Rng`]:
//! equal seeds give equal data on every machine.
//!
//! The ill-conditioned kinds are the ones geometry code gets wrong: knots clustered within
//! a few ulps, repeated knots (C0 and discontinuous joins), weights near zero, parameters
//! next to a knot, and near-tangent pairs.

use scx_num::hash::Rng;

use crate::analytic::{Circle, Frame, Line, Plane, Sphere};
use crate::data::{CurveData, SurfaceData};

/// How interior knots are placed in a clamped knot vector on `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Knots {
    /// Evenly spaced.
    Uniform,
    /// Uniformly random, sorted.
    Random,
    /// Random, with some within a few ulps of their neighbour.
    Clustered,
    /// Random, with some repeated up to the degree (C0 joins) or once more (a break).
    Multiple,
}

/// How weights are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weights {
    /// None: a polynomial B-spline.
    Unit,
    /// In `[1/2, 2]`.
    Moderate,
    /// Moderate, with some between `2^-40` and `2^-20`.
    NearZero,
}

/// The shape of a generated curve or surface direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Spec {
    /// The degree.
    pub degree: usize,
    /// The control point count (at least `degree + 1`).
    pub count: usize,
    /// The knot placement.
    pub knots: Knots,
}

/// `x` moved by `n` ulps (up for positive `n`), through zero and the subnormals, for a
/// finite `x` (`f64::next_up` is newer than the workspace's Rust).
fn ulps(x: f64, n: i64) -> f64 {
    let step = |x: f64| {
        let up = n > 0;
        if x == 0.0 {
            let tiny = f64::from_bits(1);
            return if up { tiny } else { -tiny };
        }
        let b = x.to_bits();
        f64::from_bits(if (x > 0.0) == up { b + 1 } else { b - 1 })
    };
    (0..n.unsigned_abs()).fold(x, |x, _| step(x))
}

/// `[0, len)` uniformly.
fn below(rng: &mut Rng, len: usize) -> usize {
    // Fine for the small ranges here; the bias is far below any test's sensitivity.
    (rng.next_u64() % len.max(1) as u64) as usize
}

/// A clamped knot vector on `[0, 1]`: `degree + 1` zeros, `count - degree - 1` interior
/// knots, `degree + 1` ones.
pub fn knots(rng: &mut Rng, spec: Spec) -> Vec<f64> {
    let (p, interior) = (spec.degree, spec.count.saturating_sub(spec.degree + 1));
    let mut inner: Vec<f64> = match spec.knots {
        Knots::Uniform => (1..=interior)
            .map(|i| i as f64 / (interior + 1) as f64)
            .collect(),
        _ => (0..interior)
            .map(|_| 0.02 + 0.96 * rng.next_f64())
            .collect(),
    };
    inner.sort_by(f64::total_cmp);
    match spec.knots {
        Knots::Clustered => {
            // Pull about half the knots to within 1..8 ulps of the previous one.
            for i in 1..inner.len() {
                if rng.next_u64() & 1 == 0 {
                    if let Some(&prev) = inner.get(i - 1) {
                        let n = 1 + below(rng, 8) as i64;
                        if let Some(k) = inner.get_mut(i) {
                            *k = ulps(prev, n).min(0.99);
                        }
                    }
                }
            }
        }
        Knots::Multiple => {
            // Copy a knot over its next neighbours, never more than p + 1 in all.
            let mut i = 0;
            while i < inner.len() {
                let run = 1 + below(rng, p + 1);
                let v = inner.get(i).copied().unwrap_or(0.5);
                for k in inner.iter_mut().skip(i).take(run) {
                    *k = v;
                }
                i += run;
            }
        }
        Knots::Uniform | Knots::Random => {}
    }
    inner.sort_by(f64::total_cmp);
    core::iter::repeat_n(0.0, p + 1)
        .chain(inner)
        .chain(core::iter::repeat_n(1.0, p + 1))
        .collect()
}

/// `count` weights of a kind (`None` for [`Weights::Unit`]).
pub fn weights(rng: &mut Rng, count: usize, kind: Weights) -> Option<Vec<f64>> {
    let moderate = |rng: &mut Rng| 0.5 + 1.5 * rng.next_f64();
    match kind {
        Weights::Unit => None,
        Weights::Moderate => Some((0..count).map(|_| moderate(rng)).collect()),
        Weights::NearZero => Some(
            (0..count)
                .map(|_| {
                    if below(rng, 3) == 0 {
                        let e = 20 + below(rng, 21) as i32;
                        (1.0 + rng.next_f64()) * crate::dd::pow2(-e)
                    } else {
                        moderate(rng)
                    }
                })
                .collect(),
        ),
    }
}

/// `count` points in `[-scale, scale)^D`.
pub fn points<const D: usize>(rng: &mut Rng, count: usize, scale: f64) -> Vec<[f64; D]> {
    (0..count)
        .map(|_| core::array::from_fn(|_| scale * rng.next_signed_unit()))
        .collect()
}

/// A random curve.
pub fn curve<const D: usize>(rng: &mut Rng, spec: Spec, kind: Weights) -> CurveData<D> {
    CurveData {
        degree: spec.degree,
        knots: knots(rng, spec),
        points: points(rng, spec.count, 1.0),
        weights: weights(rng, spec.count, kind),
    }
}

/// A random surface.
pub fn surface<const D: usize>(
    rng: &mut Rng,
    (su, sv): (Spec, Spec),
    kind: Weights,
) -> SurfaceData<D> {
    let n = su.count * sv.count;
    SurfaceData {
        degrees: (su.degree, sv.degree),
        knots_u: knots(rng, su),
        knots_v: knots(rng, sv),
        points: points(rng, n, 1.0),
        weights: weights(rng, n, kind),
    }
}

/// A parameter in `[lo, hi]` of a knot vector: uniform, or (half the time) on or within
/// 4 ulps of one of its distinct knots in the domain.
pub fn parameter(rng: &mut Rng, knots: &[f64], (lo, hi): (f64, f64)) -> f64 {
    if rng.next_u64() & 1 == 0 {
        return lo + (hi - lo) * rng.next_f64();
    }
    let inside: Vec<f64> = knots
        .iter()
        .copied()
        .filter(|&k| lo <= k && k <= hi)
        .collect();
    let k = inside.get(below(rng, inside.len())).copied().unwrap_or(lo);
    let n = below(rng, 9) as i64 - 4;
    ulps(k, n).clamp(lo, hi)
}

/// A random unit vector (rounded, so unit to an ulp or so).
pub fn direction(rng: &mut Rng) -> [f64; 3] {
    loop {
        let v: [f64; 3] = core::array::from_fn(|_| rng.next_signed_unit());
        let n2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
        if n2 > 1e-4 && n2 <= 1.0 {
            let n = n2.sqrt();
            return v.map(|x| x / n);
        }
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// A random right-handed frame at an origin in `[-scale, scale)^3`, orthonormal to an ulp
/// or so (the references use it as given).
pub fn frame(rng: &mut Rng, scale: f64) -> Frame {
    let origin = core::array::from_fn(|_| scale * rng.next_signed_unit());
    let z = direction(rng);
    let x = loop {
        let t = direction(rng);
        let c = cross(z, t);
        let n2 = c[0] * c[0] + c[1] * c[1] + c[2] * c[2];
        if n2 > 1e-4 {
            let n = n2.sqrt();
            break c.map(|v| v / n);
        }
    };
    let y = cross(z, x);
    Frame { origin, x, y, z }
}

/// `p + d v`.
fn offset(p: [f64; 3], d: f64, v: [f64; 3]) -> [f64; 3] {
    [p[0] + d * v[0], p[1] + d * v[1], p[2] + d * v[2]]
}

/// A circle and a line in its plane at distance `radius + gap` from its centre: tangent
/// at `gap = 0`, crossing it for small negative `gap`.
pub fn near_tangent_circle_line(rng: &mut Rng, gap: f64) -> (Circle, Line) {
    let frame = frame(rng, 1.0);
    let radius = 0.25 + rng.next_f64();
    let d = radius + gap;
    let origin = offset(frame.origin, d, frame.x);
    (
        Circle { frame, radius },
        Line {
            origin,
            direction: frame.y,
        },
    )
}

/// A sphere and a plane at distance `radius + gap` from its centre.
pub fn near_tangent_sphere_plane(rng: &mut Rng, gap: f64) -> (Sphere, Plane) {
    let frame = frame(rng, 1.0);
    let radius = 0.25 + rng.next_f64();
    let d = radius + gap;
    let origin = offset(frame.origin, d, frame.z);
    (
        Sphere { frame, radius },
        Plane {
            frame: Frame {
                origin,
                x: frame.x,
                y: frame.y,
                z: frame.z,
            },
        },
    )
}
