// Written for solidcorex. Apache-2.0.
//! The curved generators: a capped cylinder, a cone to its apex, a sphere of lunes and a
//! torus of patches. Each splits its periodic surface into `n` faces along rulings,
//! meridians or parallels, so one face crosses the seam; the cone's apex and the sphere's
//! poles are collapsed edges, one per face, whose uses are the pole's uv segment.

use std::f64::consts::{PI, TAU};

use scx_brep::{Draft, VertexId};
use scx_curve::{Circle, Curve};
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_surface::{Cone, Cylinder, Plane, Sphere, Surface, Torus};

use super::mesh::{Builder, Ed, Step};
use super::{Dice, GenError, Place, Result};

const FWD: Sense = Sense::Forward;
const REV: Sense = Sense::Reversed;

/// `n` angles round a turn from near 0, each moved up to 0.3 of a step, and the first
/// again a turn on.
fn angles(dice: &mut Dice, n: usize) -> Vec<f64> {
    let step = TAU / n as f64;
    let mut a: Vec<f64> = (0..n)
        .map(|i| step * (i as f64 + dice.jitter(0.3)))
        .collect();
    if let Some(&first) = a.first() {
        a.push(first + TAU);
    }
    a
}

fn at<T: Copy>(v: &[T], i: usize) -> Result<T> {
    v.get(i).copied().ok_or(GenError::Degenerate("index"))
}

fn ed(v: &[Ed], i: usize) -> Result<Ed> {
    v.get(i).cloned().ok_or(GenError::Degenerate("edge index"))
}

fn circle(frame: Frame, radius: f64) -> Result<Curve> {
    Ok(Curve::Circle(Circle::new(
        frame,
        Length::from_mm(radius),
        Sense::Forward,
    )?))
}

/// The arcs of the circle about local z at height `z` and of `radius`, between successive
/// `theta`, from `ring[i]` to `ring[i + 1]`.
fn ring_arcs(
    b: &mut Builder,
    place: &Place,
    z: f64,
    radius: f64,
    theta: &[f64],
    ring: &[VertexId],
) -> Result<Vec<Ed>> {
    let c = circle(
        place.frame_at([0.0, 0.0, z], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])?,
        radius,
    )?;
    let n = ring.len();
    (0..n)
        .map(|i| {
            b.edge(
                c.clone(),
                at(theta, i)?,
                at(theta, i + 1)?,
                [at(ring, i)?, at(ring, (i + 1) % n)?],
            )
        })
        .collect()
}

/// The planar cap at height `z` bounded by `arcs`, front along local `+z` when `up`.
fn cap(
    b: &mut Builder,
    place: &Place,
    z: f64,
    up: bool,
    arcs: &[Ed],
    sides: [scx_brep::ShellId; 2],
) -> Result<()> {
    let normal = if up { 1.0 } else { -1.0 };
    let frame = place.frame_at([0.0, 0.0, z], [0.0, 0.0, normal], [1.0, 0.0, 0.0])?;
    let steps: Vec<Step> = if up {
        arcs.iter().map(|e| Step::Use(e.clone(), FWD)).collect()
    } else {
        arcs.iter()
            .rev()
            .map(|e| Step::Use(e.clone(), REV))
            .collect()
    };
    b.face(Surface::Plane(Plane::new(frame)), sides, &[steps])?;
    Ok(())
}

/// The cylinder about local z from 0 to `h`, `n` wall faces between rulings, capped. Seed
/// 0: radius 1, height 2 (the pin's `create_cylinder(o, 1, 2)`).
pub(crate) fn cylinder(dice: &mut Dice, n: usize) -> Result<Draft> {
    let place = Place {
        frame: dice.frame(),
    };
    let r = dice.range(0.5, 3.0, 1.0);
    let h = dice.range(0.5, 3.0, 2.0);
    let theta = angles(dice, n);
    let mut b = Builder::new();
    let sides = {
        let (o, i) = b.solid_shells()?;
        [o, i]
    };
    let mut bottom = Vec::with_capacity(n);
    let mut top = Vec::with_capacity(n);
    for &t in theta.iter().take(n) {
        let (x, y) = (r * fmath::cos(t), r * fmath::sin(t));
        bottom.push(b.vertex(place.point([x, y, 0.0]))?);
        top.push(b.vertex(place.point([x, y, h]))?);
    }
    let low = ring_arcs(&mut b, &place, 0.0, r, &theta, &bottom)?;
    let high = ring_arcs(&mut b, &place, h, r, &theta, &top)?;
    let rulings = (0..n)
        .map(|i| b.line(at(&bottom, i)?, at(&top, i)?))
        .collect::<Result<Vec<_>>>()?;
    let wall = Surface::Cylinder(Cylinder::new(place.frame, Length::from_mm(r), FWD)?);
    for i in 0..n {
        let steps = [
            Step::Use(ed(&low, i)?, FWD),
            Step::Use(ed(&rulings, (i + 1) % n)?, FWD),
            Step::Use(ed(&high, i)?, REV),
            Step::Use(ed(&rulings, i)?, REV),
        ];
        b.face(wall.clone(), sides, &[steps.to_vec()])?;
    }
    cap(&mut b, &place, 0.0, false, &low, sides)?;
    cap(&mut b, &place, h, true, &high, sides)?;
    Ok(b.d)
}

/// The cone over the base circle of radius `r` to its apex at height `h`, `n` wall faces
/// between rulings, each with a collapsed edge at the apex; a planar base. Seed 0: radius
/// 1, height 1.
pub(crate) fn cone(dice: &mut Dice, n: usize) -> Result<Draft> {
    let place = Place {
        frame: dice.frame(),
    };
    let r = dice.range(0.5, 3.0, 1.0);
    let h = dice.range(0.5, 3.0, 1.0);
    let theta = angles(dice, n);
    let mut b = Builder::new();
    let sides = {
        let (o, i) = b.solid_shells()?;
        [o, i]
    };
    let base = theta
        .iter()
        .take(n)
        .map(|&t| b.vertex(place.point([r * fmath::cos(t), r * fmath::sin(t), 0.0])))
        .collect::<Result<Vec<_>>>()?;
    let apex = b.vertex(place.point([0.0, 0.0, h]))?;
    let arcs = ring_arcs(&mut b, &place, 0.0, r, &theta, &base)?;
    let rulings = (0..n)
        .map(|i| b.line(at(&base, i)?, apex))
        .collect::<Result<Vec<_>>>()?;
    // Narrowing along z: a negative half-angle (the pin's sign).
    let half_angle = -fmath::atan(r / h);
    let wall = Surface::Cone(Cone::new(place.frame, Length::from_mm(r), half_angle, FWD)?);
    for i in 0..n {
        let pole = b.collapsed(apex)?;
        let steps = vec![
            Step::Use(ed(&arcs, i)?, FWD),
            Step::Use(ed(&rulings, (i + 1) % n)?, FWD),
            Step::Pole(pole, -1.0),
            Step::Use(ed(&rulings, i)?, REV),
        ];
        b.face(wall.clone(), sides, &[steps])?;
    }
    cap(&mut b, &place, 0.0, false, &arcs, sides)?;
    Ok(b.d)
}

/// The sphere of `n` lunes between meridians, each with a collapsed edge at either pole.
/// Seed 0: radius 1 about the origin.
pub(crate) fn sphere(dice: &mut Dice, n: usize) -> Result<Draft> {
    let place = Place {
        frame: dice.frame(),
    };
    let r = dice.range(0.5, 3.0, 1.0);
    let theta = angles(dice, n);
    let mut b = Builder::new();
    let sides = {
        let (o, i) = b.solid_shells()?;
        [o, i]
    };
    let south = b.vertex(place.point([0.0, 0.0, -r]))?;
    let north = b.vertex(place.point([0.0, 0.0, r]))?;
    // Meridian i: the half circle from the south pole to the north through D(theta_i), in
    // the frame x = -Z, y = D (z = -D').
    let meridians = theta
        .iter()
        .take(n)
        .map(|&t| {
            let (c, s) = (fmath::cos(t), fmath::sin(t));
            let frame = place.frame_at([0.0; 3], [s, -c, 0.0], [0.0, 0.0, -1.0])?;
            b.edge(circle(frame, r)?, 0.0, PI, [south, north])
        })
        .collect::<Result<Vec<_>>>()?;
    let surface = Surface::Sphere(Sphere::new(place.frame, Length::from_mm(r), FWD)?);
    for i in 0..n {
        let (np, sp) = (b.collapsed(north)?, b.collapsed(south)?);
        let steps = vec![
            Step::Use(ed(&meridians, (i + 1) % n)?, FWD),
            Step::Pole(np, -1.0),
            Step::Use(ed(&meridians, i)?, REV),
            Step::Pole(sp, 1.0),
        ];
        b.face(surface.clone(), sides, &[steps])?;
    }
    Ok(b.d)
}

/// The torus of `n x n` patches between parallels and meridians. Seed 0: major radius 2,
/// minor 1, about the origin.
pub(crate) fn torus(dice: &mut Dice, n: usize) -> Result<Draft> {
    let place = Place {
        frame: dice.frame(),
    };
    let major = dice.range(1.5, 3.0, 2.0);
    let minor = dice.range(0.3, 0.9, 0.5) * major;
    let theta = angles(dice, n);
    let omega = angles(dice, n);
    let mut b = Builder::new();
    let sides = {
        let (o, i) = b.solid_shells()?;
        [o, i]
    };
    // vertex (i, j) at theta_i round the axis, omega_j round the tube from its outer equator.
    let mut grid = Vec::with_capacity(n * n);
    for j in 0..n {
        let w = at(&omega, j)?;
        for i in 0..n {
            let t = at(&theta, i)?;
            let rho = major + minor * fmath::cos(w);
            grid.push(b.vertex(place.point([
                rho * fmath::cos(t),
                rho * fmath::sin(t),
                minor * fmath::sin(w),
            ]))?);
        }
    }
    let v = |i: usize, j: usize| at(&grid, (j % n) * n + (i % n));
    // Parallel (i, j) from vertex (i, j) to (i + 1, j); meridian (i, j) from (i, j) to
    // (i, j + 1).
    let mut parallels = Vec::with_capacity(n * n);
    for j in 0..n {
        let w = at(&omega, j)?;
        let ring = (0..n).map(|i| v(i, j)).collect::<Result<Vec<_>>>()?;
        parallels.extend(ring_arcs(
            &mut b,
            &place,
            minor * fmath::sin(w),
            major + minor * fmath::cos(w),
            &theta,
            &ring,
        )?);
    }
    let mut meridians = Vec::with_capacity(n * n);
    for j in 0..n {
        for i in 0..n {
            let t = at(&theta, i)?;
            let (c, s) = (fmath::cos(t), fmath::sin(t));
            let frame = place.frame_at([major * c, major * s, 0.0], [s, -c, 0.0], [c, s, 0.0])?;
            meridians.push(b.edge(
                circle(frame, minor)?,
                at(&omega, j)?,
                at(&omega, j + 1)?,
                [v(i, j)?, v(i, j + 1)?],
            )?);
        }
    }
    let surface = Surface::Torus(Torus::new(
        place.frame,
        Length::from_mm(major),
        Length::from_mm(minor),
        FWD,
    )?);
    let idx = |i: usize, j: usize| (j % n) * n + (i % n);
    for j in 0..n {
        for i in 0..n {
            let steps = vec![
                Step::Use(ed(&parallels, idx(i, j))?, FWD),
                Step::Use(ed(&meridians, idx(i + 1, j))?, FWD),
                Step::Use(ed(&parallels, idx(i, j + 1))?, REV),
                Step::Use(ed(&meridians, idx(i, j))?, REV),
            ];
            b.face(surface.clone(), sides, &[steps])?;
        }
    }
    Ok(b.d)
}
