// Written for solidcorex. Apache-2.0.
//! The per-face memo (research 72 MA5): a face's integral about its own reference, kept in
//! the caller's [`Scratch`] and keyed by the bits it is computed from.
//!
//! A face is integrated about its own reference with floors from its own box
//! ([`face::integrate`]), so its [`FaceIntegral`] (values, and the stated error with its
//! rounding bound, 72 §14.31) is a function of what that integral reads, and nothing else
//! of the body. The key is those bits, written by the record format's own writers
//! ([`scx_record::Record`]), so every bit counts:
//!
//! - which integrals ([`Set`]) and the accuracy, as its bits;
//! - the face's grown box ([`Face::bounding_box`]): it fixes the reference, the floors and
//!   the rounding test, and holds the face's accuracy and the body's resolution;
//! - the surface, its sense included (the natural normal the integral is taken about);
//! - each loop's uses in order: the use's sense, its pcurve and interval (or its point) and
//!   its recorded bound; and for a use with a bound on a curve edge, what its gap closure
//!   reads: the edge's curve and the use's end vertices' points.
//!
//! Left out on purpose: the face's id, its uses' edge ids and its sides. None is read by the
//! integral (an id appears only in an error, and an error is never kept); the sides give
//! the sign the body sums the face with, applied after ([`crate::body`]). So a face a
//! Boolean keeps, renumbers or turns over (a subtract swaps a kept tool face's sides) hits.
//!
//! **A hit is the recompute.** An entry holds the full bits beside the result, and a hit
//! needs them equal, so a hash collision only replaces the entry. A hit charges the work
//! its fill recorded, at once (04, 11-27: work does not depend on the memo's warmth), so a
//! budget runs out on a hit exactly where the recompute runs out, as `Exhausted` for the
//! call (the budget's `used` then holds the face's whole work, where a recompute stops
//! partway). Only a finished integral is kept: an error, the budget's included, keeps
//! nothing. A face whose bits cannot be written (a value not finite, a kind with no record,
//! no box) is integrated without the memo.

use scx_brep::op::Scratch;
use scx_brep::{EdgeShape, Entity, Face, UseGeom};
use scx_num::frame::Sense;
use scx_num::hash::mix;
use scx_record::{Record, Sink, WriteError, Writer};
use scx_surface::Surface;

use crate::error::MassError;
use crate::face::{self, FaceIntegral, fail};
use crate::integrand::Set;
use crate::kronrod::Meter;

/// One face's integral as filled: the bits it came from, the integral, and the work it
/// charged.
struct Entry {
    bits: Vec<u8>,
    integral: FaceIntegral,
    work: u64,
}

/// The face's `set` integrals at `accuracy`, from `scratch` when the same bits were
/// integrated before, else by [`face::integrate`] and kept.
pub(crate) fn integrate(
    face: Face<'_>,
    set: Set,
    accuracy: f64,
    scratch: &mut Scratch,
    meter: &mut Meter<'_>,
) -> Result<FaceIntegral, MassError> {
    let Some(bits) = bits(face, face.surface(), set, accuracy) else {
        return face::integrate(face, set, accuracy, meter);
    };
    let key = key(&bits);
    if let Some(e) = scratch.get::<Entry>(key)
        && e.bits == bits
    {
        meter
            .charge(e.work)
            .map_err(|f| fail(Entity::from(face.id()), f))?;
        return Ok(e.integral);
    }
    let before = meter.used();
    let integral = face::integrate(face, set, accuracy, meter)?;
    let work = meter.used().saturating_sub(before);
    scratch.insert(
        key,
        Entry {
            bits,
            integral,
            work,
        },
    );
    Ok(integral)
}

/// The scratch key: the bits folded eight bytes at a time.
fn key(bits: &[u8]) -> u64 {
    let mut words = Vec::with_capacity(bits.len().div_ceil(8) + 1);
    words.push(bits.len() as u64);
    for c in bits.chunks(8) {
        let mut w = [0u8; 8];
        for (d, s) in w.iter_mut().zip(c) {
            *d = *s;
        }
        words.push(u64::from_le_bytes(w));
    }
    mix(&words)
}

/// The bits the face's `set` integral at `accuracy` reads, with `surface` as its surface
/// (the face's own, but for the tests); `None` when they cannot be written.
fn bits(face: Face<'_>, surface: &Surface, set: Set, accuracy: f64) -> Option<Vec<u8>> {
    let b = face.bounding_box()?;
    let mut w = Writer::new();
    write(&mut w, face, surface, set, accuracy, b.min(), b.max()).ok()?;
    Some(w.finish())
}

fn sense(w: &mut Writer, s: Sense) {
    w.u8(match s {
        Sense::Forward => 0,
        Sense::Reversed => 1,
    });
}

fn write(
    w: &mut Writer,
    face: Face<'_>,
    surface: &Surface,
    set: Set,
    accuracy: f64,
    lo: scx_num::vector3d::Point3,
    hi: scx_num::vector3d::Point3,
) -> Result<(), WriteError> {
    w.u8(match set {
        Set::Area => 0,
        Set::Volume => 1,
        Set::Full => 2,
    });
    w.f64(accuracy)?;
    for x in [lo.x, lo.y, lo.z, hi.x, hi.y, hi.z] {
        w.f64(x)?;
    }
    surface.put(w)?;
    w.u64(face.loops().len() as u64);
    for lp in face.loops() {
        w.u64(lp.len() as u64);
        for u in lp.uses() {
            sense(w, u.sense());
            match u.geom() {
                UseGeom::Pcurve { curve, interval } => {
                    w.u8(0);
                    curve.put(w)?;
                    w.f64(interval.min())?;
                    w.f64(interval.max())?;
                }
                UseGeom::Point(p) => {
                    w.u8(1);
                    w.f64(p.x)?;
                    w.f64(p.y)?;
                }
            }
            let Some(bound) = u.bound() else {
                w.u8(0);
                continue;
            };
            w.u8(1);
            w.f64(bound.acc.length().mm())?;
            // What the gap closure reads (`face::strip`).
            match u.edge().shape() {
                EdgeShape::Curve { curve, .. } => {
                    w.u8(0);
                    curve.put(w)?;
                    for p in [u.start().point(), u.end().point()] {
                        w.f64(p.x)?;
                        w.f64(p.y)?;
                        w.f64(p.z)?;
                    }
                }
                EdgeShape::Collapsed => w.u8(1),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use scx_brep::Document;
    use scx_num::frame::Sense;
    use scx_surface::{Cylinder, Surface};
    use scx_testkit::bodies::{Kind, generate};

    use super::bits;
    use crate::integrand::Set;

    fn cylinder() -> scx_brep::Brep {
        let d = generate(Kind::Cylinder, 0, 1).unwrap().draft;
        d.finish(&Document::default())
            .map_err(|u| u.cause)
            .unwrap()
            .0
    }

    #[test]
    fn a_flipped_sense_another_set_or_accuracy_is_another_key() {
        let body = cylinder();
        let wall = body
            .faces()
            .find(|f| matches!(f.surface(), Surface::Cylinder(_)))
            .unwrap();
        let Surface::Cylinder(c) = wall.surface() else {
            panic!("a cylinder wall");
        };
        let other = match c.sense() {
            Sense::Forward => Sense::Reversed,
            Sense::Reversed => Sense::Forward,
        };
        let flipped = Surface::Cylinder(Cylinder::new(*c.frame(), c.radius(), other).unwrap());
        let own = bits(wall, wall.surface(), Set::Volume, 1e-9).unwrap();
        assert_eq!(own, bits(wall, wall.surface(), Set::Volume, 1e-9).unwrap());
        assert_ne!(own, bits(wall, &flipped, Set::Volume, 1e-9).unwrap());
        assert_ne!(own, bits(wall, wall.surface(), Set::Full, 1e-9).unwrap());
        assert_ne!(own, bits(wall, wall.surface(), Set::Volume, 1e-10).unwrap());
        // The next accuracy up by one bit is another key too.
        let up = f64::from_bits(1e-9f64.to_bits() + 1);
        assert_ne!(own, bits(wall, wall.surface(), Set::Volume, up).unwrap());
    }
}
