// Written for solidcorex. Apache-2.0.
//! An edge's length: `∫ |C'(t)| dt` over its interval by the 7-point Kronrod rule, split at
//! a NURBS curve's knots, each piece held to a tenth of the accuracy against the larger of
//! its own length and its share of the edge's box.
//!
//! The pin's edge properties also integrate a tube of a given cross-section about the
//! curve (`edge_properties`' tube terms); no caller here asks for those, so they are not
//! ported.

use scx_brep::op::Scratch;
use scx_brep::{Edge, EdgeShape, Entity};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_nurbs::Approach;

use crate::error::{MassDefect, MassError, MassUnsupported};
use crate::face::{FIRST_WORK, fail, reachable};
use crate::kronrod::{self, K7, Meter, Tol};
use crate::spec::{Estimate, RelAccuracy};

/// The edge's length; zero for a collapsed edge.
///
/// # Errors
///
/// [`MassUnsupported::NoBox`] for an edge with no box; the budget, the stop flag and the
/// integrator's defects.
pub fn edge_length(
    edge: Edge<'_>,
    accuracy: RelAccuracy,
    scratch: &mut Scratch,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Estimate, MassError> {
    let _ = scratch;
    let at = Entity::from(edge.id());
    let EdgeShape::Curve { curve, interval } = edge.shape() else {
        return Ok(Estimate {
            value: 0.0,
            error: 0.0,
        });
    };
    let bbox = edge
        .bounding_box()
        .ok_or(MassError::Unsupported(MassUnsupported::NoBox { at }))?;
    let size = bbox.max_length();
    // As a face's: the coordinates' rounding is what a piece cannot be held below.
    let noise = reachable(bbox.min(), bbox.max(), size);
    let (lo, hi) = (interval.min(), interval.max());
    let mut ends = vec![lo];
    if let Curve::Nurbs(n) = curve {
        for &k in n.knots() {
            if lo < k && k < hi && ends.last().is_some_and(|&l| l < k) {
                ends.push(k);
            }
        }
    }
    ends.push(hi);
    let mut meter = Meter::new(budget, stop);
    let width = hi - lo;
    let run = |held: f64, meter: &mut Meter<'_>| -> Result<(f64, f64), MassError> {
        let (mut value, mut error) = ([0.0], [0.0]);
        for w in ends.windows(2) {
            let [a, b] = *w else { continue };
            let floor = [size * (b - a) / width];
            let tol = Tol {
                accuracy: held,
                floor: &floor,
            };
            kronrod::integrate(
                &K7,
                (a, b),
                tol,
                &mut value,
                &mut error,
                meter,
                |t, out, _| {
                    let [d] = curve.jet::<1>(t, Approach::Above).d;
                    if let Some(v) = out.value.first_mut() {
                        *v = d.length();
                    }
                    Ok(())
                },
            )
            .map_err(|f| fail(at, f))?;
        }
        Ok((value[0], error[0]))
    };
    // As a face's ([`crate::face::integrate`]): where the coordinates' rounding is above
    // the accuracy, a first attempt on fewer pieces, then one held to the rounding.
    let held = accuracy.get() / 10.0;
    let (value, error) = if noise <= held {
        run(held, &mut meter)?
    } else {
        meter.allow(Some(FIRST_WORK));
        let first = run(held, &mut meter);
        meter.allow(None);
        match first {
            Err(MassError::Defect(MassDefect::StepControl { .. })) => {
                let (v, e) = run(noise, &mut meter)?;
                (v, e + noise * size)
            }
            done => done?,
        }
    };
    Ok(Estimate { value, error })
}
