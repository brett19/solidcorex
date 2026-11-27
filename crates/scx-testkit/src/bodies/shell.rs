// Written for solidcorex. Apache-2.0.
//! A quarter of a cylindrical shell (the pin's `create_cylindrical_box` between two radii
//! over 0 to 90 degrees), for bodies far thinner than they are wide.

use std::f64::consts::FRAC_PI_2;

use scx_brep::Draft;
use scx_curve::{Circle, Curve};
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Cylinder, Plane, Surface};

use super::mesh::{Builder, Step};
use super::{GenError, Result};

const FWD: Sense = Sense::Forward;
const REV: Sense = Sense::Reversed;

/// The quarter shell between radii `r` and `r + t` about world z, over the angles 0 to
/// 90 degrees and heights 0 to `length`: six faces (the outer wall, the inner wall facing
/// the axis, two annular sector caps and two rectangles in the planes `y = 0` and
/// `x = 0`). Its volume is `pi / 4 ((r + t)^2 - r^2) length`.
///
/// # Errors
///
/// A radius, thickness or length that is not finite and above zero, or a part the
/// builder refuses.
pub fn quarter_shell(r: f64, t: f64, length: f64) -> Result<Draft> {
    if !([r, t, length].iter().all(|x| x.is_finite() && *x > 0.0) && (r + t).is_finite()) {
        return Err(GenError::Degenerate("quarter shell size"));
    }
    let outer = r + t;
    let frame = |o: [f64; 3], z: [f64; 3], x: [f64; 3]| {
        Frame::new(
            Point3::new(o[0], o[1], o[2]),
            Vec3::new(z[0], z[1], z[2]),
            Vec3::new(x[0], x[1], x[2]),
        )
        .ok_or(GenError::Degenerate("frame"))
    };
    let axis = |z: f64| frame([0.0, 0.0, z], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let mut b = Builder::new();
    let sides = {
        let (o, i) = b.solid_shells()?;
        [o, i]
    };
    // v[radius][angle][height]: radius 0 inner, angle 0 at 0 degrees, height 0 at z 0.
    let mut v = [[[None; 2]; 2]; 2];
    for (ri, rad) in [r, outer].into_iter().enumerate() {
        for (ai, (x, y)) in [(rad, 0.0), (0.0, rad)].into_iter().enumerate() {
            for (hi, z) in [0.0, length].into_iter().enumerate() {
                if let Some(slot) = v
                    .get_mut(ri)
                    .and_then(|a| a.get_mut(ai))
                    .and_then(|h| h.get_mut(hi))
                {
                    *slot = Some(b.vertex(Point3::new(x, y, z))?);
                }
            }
        }
    }
    let at = |ri: usize, ai: usize, hi: usize| {
        v.get(ri)
            .and_then(|a| a.get(ai))
            .and_then(|h| h.get(hi))
            .copied()
            .flatten()
            .ok_or(GenError::Degenerate("vertex"))
    };
    // arc[radius][height], from angle 0 to 90 degrees.
    let mut arcs = Vec::new();
    for (ri, rad) in [r, outer].into_iter().enumerate() {
        let mut row = Vec::new();
        for (hi, z) in [0.0, length].into_iter().enumerate() {
            let c = Curve::Circle(Circle::new(axis(z)?, Length::from_mm(rad), FWD)?);
            row.push(b.edge(c, 0.0, FRAC_PI_2, [at(ri, 0, hi)?, at(ri, 1, hi)?])?);
        }
        arcs.push(row);
    }
    let arc = |ri: usize, hi: usize| {
        arcs.get(ri)
            .and_then(|row| row.get(hi))
            .cloned()
            .ok_or(GenError::Degenerate("arc"))
    };
    // ruling[radius][angle], bottom to top; radial[angle][height], inner to outer.
    let mut rulings = Vec::new();
    for ri in 0..2 {
        rulings.push([
            b.line(at(ri, 0, 0)?, at(ri, 0, 1)?)?,
            b.line(at(ri, 1, 0)?, at(ri, 1, 1)?)?,
        ]);
    }
    let mut radials = Vec::new();
    for ai in 0..2 {
        radials.push([
            b.line(at(0, ai, 0)?, at(1, ai, 0)?)?,
            b.line(at(0, ai, 1)?, at(1, ai, 1)?)?,
        ]);
    }
    let ruling = |ri: usize, ai: usize| {
        rulings
            .get(ri)
            .and_then(|x| x.get(ai))
            .cloned()
            .ok_or(GenError::Degenerate("ruling"))
    };
    let radial = |ai: usize, hi: usize| {
        radials
            .get(ai)
            .and_then(|x| x.get(hi))
            .cloned()
            .ok_or(GenError::Degenerate("radial"))
    };
    // The outer wall faces away from the axis, the inner wall toward it.
    let wall = Surface::Cylinder(Cylinder::new(axis(0.0)?, Length::from_mm(outer), FWD)?);
    let steps = vec![
        Step::Use(arc(1, 0)?, FWD),
        Step::Use(ruling(1, 1)?, FWD),
        Step::Use(arc(1, 1)?, REV),
        Step::Use(ruling(1, 0)?, REV),
    ];
    b.face(wall, sides, &[steps])?;
    let wall = Surface::Cylinder(Cylinder::new(axis(0.0)?, Length::from_mm(r), REV)?);
    let steps = vec![
        Step::Use(ruling(0, 0)?, FWD),
        Step::Use(arc(0, 1)?, FWD),
        Step::Use(ruling(0, 1)?, REV),
        Step::Use(arc(0, 0)?, REV),
    ];
    b.face(wall, sides, &[steps])?;
    // The caps: the bottom faces -z, the top +z.
    let bottom = Surface::Plane(Plane::new(frame(
        [0.0; 3],
        [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0],
    )?));
    let steps = vec![
        Step::Use(arc(0, 0)?, FWD),
        Step::Use(radial(1, 0)?, FWD),
        Step::Use(arc(1, 0)?, REV),
        Step::Use(radial(0, 0)?, REV),
    ];
    b.face(bottom, sides, &[steps])?;
    let top = Surface::Plane(Plane::new(axis(length)?));
    let steps = vec![
        Step::Use(radial(0, 1)?, FWD),
        Step::Use(arc(1, 1)?, FWD),
        Step::Use(radial(1, 1)?, REV),
        Step::Use(arc(0, 1)?, REV),
    ];
    b.face(top, sides, &[steps])?;
    // The sides: y = 0 faces -y, x = 0 faces -x.
    let side = Surface::Plane(Plane::new(frame(
        [0.0; 3],
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0],
    )?));
    let steps = vec![
        Step::Use(radial(0, 0)?, FWD),
        Step::Use(ruling(1, 0)?, FWD),
        Step::Use(radial(0, 1)?, REV),
        Step::Use(ruling(0, 0)?, REV),
    ];
    b.face(side, sides, &[steps])?;
    let side = Surface::Plane(Plane::new(frame(
        [0.0; 3],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
    )?));
    let steps = vec![
        Step::Use(ruling(0, 1)?, FWD),
        Step::Use(radial(1, 1)?, FWD),
        Step::Use(ruling(1, 1)?, REV),
        Step::Use(radial(1, 0)?, REV),
    ];
    b.face(side, sides, &[steps])?;
    Ok(b.d)
}
